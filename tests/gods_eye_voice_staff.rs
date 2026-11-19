//! God's Eye by voice with real staff sessions (synthetic accounts only): a client-scoped
//! account gets 403, a firm-wide admin gets the spoken view. Own binary: the staff registry is global.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.4.0.9");
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
async fn account(r: &axum::Router, username: &str, display: &str, role: &str, clients: Value) -> String {
    ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":username,"display_name":display,"role":role,"clients":clients,"temporary_password":PW}))).await;
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":username,"password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gods_eye_voice_is_firm_wide_staff_only() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::remove_var("OPENAI_API_KEY");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let shared = Store::memory();
    let r = app_with(move || shared.clone());
    let turn = json!({"transcript": "Nixon, God's Eye", "channel": "push_to_talk"});

    let preparer = account(&r, "gev-preparer", "Synthetic Preparer", "preparer", json!(["synthetic-co"])).await;
    let reviewer = account(&r, "gev-reviewer", "Synthetic Reviewer", "reviewer", json!(["synthetic-co"])).await;
    for tok in [&preparer, &reviewer] {
        let (status, body) = call(&r, "POST", "/api/nixon/voice", tok, Some(turn.clone())).await;
        assert_eq!(status, 403, "{body}");
        assert!(!body.to_string().contains("God's Eye as of"), "no view content leaks in a refusal");
        assert_eq!(call(&r, "POST", "/api/nixon/voice/realtime", tok, Some(json!({"sdp":"v=0"}))).await.0, 403);
        assert_eq!(call(&r, "POST", "/api/os/ask", tok, Some(json!({"messages":[{"role":"user","content":"God's Eye"}]}))).await.0, 403);
    }
    let admin = account(&r, "gev-admin", "Synthetic Admin", "admin", json!(["*"])).await;
    let said = ok(&r, "POST", "/api/nixon/voice", &admin, Some(turn.clone())).await;
    assert_eq!(said["intent"], "gods_eye");
    assert_eq!(said["open"]["view"], "gods-eye");
    assert!(said["say"].as_str().unwrap().starts_with("God's Eye as of "));
}
