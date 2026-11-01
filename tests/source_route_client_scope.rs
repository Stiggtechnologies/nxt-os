//! Synthetic acceptance: source lists and source imports honour staff client scope.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.2");
    if !tok.is_empty() { q = q.header("authorization", format!("Bearer {tok}")); }
    let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
async fn ok(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, m, uri, tok, body).await;
    assert_eq!(s, 200, "{m} {uri}: {v}");
    v
}
async fn scoped_token(r: &axum::Router) -> String {
    ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":"scoped","display_name":"Sam Scoped","role":"preparer","clients":["client-a"],"temporary_password":"synthetic-pass-phrase-1"}))).await;
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":"scoped","password":"synthetic-pass-phrase-1"}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":"synthetic-pass-phrase-1","new_password":"synthetic-pass-phrase-2"}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scoped_staff_reach_only_their_clients_source_records() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let tok = scoped_token(&r).await;

    ok(&r, "GET", "/api/company-books/client-a/source-lists", &tok, None).await;
    assert_eq!(call(&r, "GET", "/api/company-books/client-b/source-lists", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", &format!("/api/company-books/client-b/source-lists/employees/{}/records", "a".repeat(64)), &tok, None).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/company-books/client-b/source-lists/chunks", &tok, Some(json!({"capture":{}}))).await.0, 403);
    ok(&r, "GET", "/api/company-books/client-b/source-lists", "k", None).await;

    let files = "/api/source-imports/dtmax/companies/firm-co";
    ok(&r, "GET", "/api/source-imports/coverage", &tok, None).await;
    ok(&r, "GET", &format!("{files}/clients/client-a/files"), &tok, None).await;
    assert_eq!(call(&r, "GET", &format!("{files}/clients/client-b/files"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", &format!("{files}/files/unmatched"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", &format!("{files}/test-copy/summary"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "POST", &format!("{files}/clients"), &tok, Some(json!({"client_id":"client-a","dtmax_keys":["X"]}))).await.0, 403);
    assert_eq!(call(&r, "GET", &format!("/api/source-imports/dtmax/batches/{}", "a".repeat(16)), &tok, None).await.0, 403);
    ok(&r, "GET", &format!("{files}/test-copy/summary"), "k", None).await;
}
