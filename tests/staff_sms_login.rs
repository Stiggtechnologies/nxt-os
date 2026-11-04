//! Synthetic acceptance for SMS one-time-code sign-in (mock code, no Twilio, no real people).
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(r: &axum::Router, uri: &str, tok: Option<&str>, body: Value) -> (u16, Value) {
    let mut q = Request::builder().method("POST").uri(uri).header("x-forwarded-for", "10.0.0.9").header("content-type", "application/json");
    if let Some(t) = tok { q = q.header("authorization", format!("Bearer {t}")); }
    let resp = r.clone().oneshot(q.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sms_code_is_a_second_factor() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::set_var("NXT_SMS_MOCK_CODE", "246810");
    }
    let r = app_with(Store::memory);
    let pw = "synthetic-pass-phrase-1";
    let (s, u) = call(&r, "/api/staff/users", Some("k"), json!({"username":"smsuser","display_name":"SMS User","role":"admin","clients":[],"temporary_password":pw})).await;
    assert_eq!(s, 200, "{u}");
    let id = u["id"].as_str().unwrap();
    assert_eq!(call(&r, &format!("/api/staff/users/{id}"), Some("k"), json!({"phone":"not a phone"})).await.0, 400);
    let (s, v) = call(&r, &format!("/api/staff/users/{id}"), Some("k"), json!({"phone":"780-555-0100"})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["phone"], "********0100");
    // Wrong password: same answer, nothing distinguishes it.
    assert_eq!(call(&r, "/api/staff/login/sms", None, json!({"username":"smsuser","password":"wrong-pass-phrase-x"})).await.0, 200);
    assert_eq!(call(&r, "/api/staff/login/sms", None, json!({"username":"smsuser","password":pw})).await.0, 200);
    // Wrong code is refused.
    assert_eq!(call(&r, "/api/staff/login", None, json!({"username":"smsuser","password":pw,"sms_code":"000000"})).await.0, 401);
    // Right code gives a full (MFA) session, still gated on the temporary password change.
    let (s, l) = call(&r, "/api/staff/login", None, json!({"username":"smsuser","password":pw,"sms_code":"246810"})).await;
    assert_eq!(s, 200, "{l}");
    assert_eq!(l["mfa_enrollment_required"], false);
    assert_eq!(l["must_change_password"], true);
}
