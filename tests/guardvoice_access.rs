//! Synthetic acceptance: the GuardVoice desk crosses clients, so it needs a
//! firm-wide account, and only a reviewer or admin may decide an approval.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.3.0.1");
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
async fn the_phone_desk_is_firm_wide_and_approvals_need_a_reviewer() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let scoped = preparer(&r, "scoped", json!(["client-a"])).await;
    // '*' (firm-wide) is admin-only: a preparer cannot be given it, so the desk is admin-only too.
    let (s, _) = call(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"firmwide","display_name":"Synthetic Preparer","role":"preparer","clients":["*"],"temporary_password":PW}))).await;
    assert_eq!(s, 400);
    let both = preparer(&r, "both", json!(["client-a", "client-b"])).await;

    assert_eq!(call(&r, "GET", "/api/guardvoice/calls", &scoped, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/guardvoice/calls", &both, None).await.0, 403);
    ok(&r, "GET", "/api/guardvoice/calls", "k", None).await;

    let body = || Some(json!({"decision": "approve"}));
    assert_eq!(call(&r, "POST", "/api/guardvoice/approvals/none/decide", &both, body()).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/guardvoice/approvals/none/decide", "k", body()).await.0, 400, "an admin reaches the lookup");
}
