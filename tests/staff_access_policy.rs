//! Synthetic acceptance: firm-wide routes and reviewer-only book actions refuse scoped preparers.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.4");
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
async fn scoped_preparer_stays_inside_assigned_clients_and_cannot_approve() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let tok = scoped_token(&r).await;

    ok(&r, "GET", "/api/os/status", &tok, None).await;
    ok(&r, "GET", "/api/nixon/clients", &tok, None).await;
    for path in ["/api/ledger/accounts", "/api/ledger/reports/dashboard", "/api/os/insights", "/api/nixon/reviews"] {
        assert_eq!(call(&r, "GET", path, &tok, None).await.0, 403, "{path}");
    }
    assert_eq!(call(&r, "POST", "/api/os/ask", &tok, Some(json!({"messages":[]}))).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/nixon/actions", &tok, Some(json!({}))).await.0, 403);

    let commit = json!({"preview_id":"a".repeat(64),"expected_head":null,"request_id":"commit-1","reviewer_label":"Sam"});
    let (s, v) = call(&r, "POST", "/api/company-books/client-a/imports/commit", &tok, Some(commit.clone())).await;
    assert_eq!(s, 403, "{v}");
    assert!(v["error"].as_str().unwrap().contains("reviewer"), "{v}");
    assert_eq!(call(&r, "POST", "/api/company-books/client-a/period-lock", &tok, Some(json!({}))).await.0, 403);

    let (s, _) = call(&r, "GET", "/api/ledger/accounts", "k", None).await;
    assert_eq!(s, 200);
    assert_ne!(call(&r, "POST", "/api/company-books/client-a/imports/commit", "k", Some(commit)).await.0, 403);
}
