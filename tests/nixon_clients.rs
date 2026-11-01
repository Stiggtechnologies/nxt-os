//! Synthetic acceptance: Nixon's per-client view shows staff only the clients
//! they are assigned to, and stays read-only.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.4.0.1");
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
async fn preparer(r: &axum::Router, name: &str, clients: Value) -> String {
    ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":name,"display_name":"Synthetic Preparer","role":"preparer","clients":clients,"temporary_password":PW}))).await;
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":name,"password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_account_sees_only_its_clients() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let tok = preparer(&r, "scoped", json!(["client-a"])).await;
    for c in ["client-a", "client-b"] {
        ok(&r, "POST", "/api/journeys", "k", Some(json!({"client_id": c}))).await;
    }

    let mine = ok(&r, "GET", "/api/nixon/clients", &tok, None).await;
    assert_eq!(mine["clients"].as_array().unwrap().len(), 1, "{mine}");
    assert_eq!(mine["clients"][0]["client_id"], "client-a");
    assert_eq!(ok(&r, "GET", "/api/nixon/clients", "k", None).await["clients"].as_array().unwrap().len(), 2);

    let v = ok(&r, "GET", "/api/nixon/clients/client-a", &tok, None).await;
    assert_eq!(v["read_only"], true);
    assert!(v["findings"].to_string().contains("No company books"), "{v}");

    assert_eq!(call(&r, "GET", "/api/nixon/clients/client-b", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/nixon/clients/no-such-client", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/nixon/clients/no-such-client", "k", None).await.0, 404);
}
