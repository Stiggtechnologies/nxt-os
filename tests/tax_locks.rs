//! Per-return editing locks: acquire, enforce, take over, release.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = token { req = req.header("authorization", format!("Bearer {token}")); }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn return_body(client: &str) -> Value {
    json!({"program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "Locked workpaper", "client_ref": client, "inputs": {}})
}
fn update_body(client: &str, revision: Value) -> Value {
    json!({"expected_revision": revision, "program": "T1", "tax_year": 2025, "jurisdiction": "AB",
        "name": "Locked workpaper", "client_ref": client, "inputs": {}, "parties": [], "notes": ""})
}

#[tokio::test]
async fn locks_are_acquired_enforced_taken_over_and_released() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let (_, record) = call(&r, "POST", "/api/tax/returns", Some("k"), Some(return_body("A"))).await;
    let id = record["id"].as_str().unwrap().to_string();

    let (status, empty) = call(&r, "GET", &format!("/api/tax/returns/{id}/lock"), Some("k"), None).await;
    assert_eq!(status, 200);
    assert_eq!(empty["lock"], Value::Null);

    // A staff preparer for client A.
    let password = "Temp-pass-12345";
    let (status, _) = call(&r, "POST", "/api/staff/users", Some("k"), Some(json!({
        "username": "prep", "display_name": "Pat Preparer", "role": "preparer", "clients": ["A"], "temporary_password": password
    }))).await;
    assert_eq!(status, 200);
    let (_, login) = call(&r, "POST", "/api/staff/login", None, Some(json!({"username": "prep", "password": password}))).await;
    let token = login["token"].as_str().unwrap().to_string();
    let (_, enroll) = call(&r, "POST", "/api/staff/mfa/enroll", Some(&token), Some(json!({}))).await;
    let secret = enroll["secret"].as_str().unwrap().to_string();
    call(&r, "POST", "/api/staff/mfa/confirm", Some(&token), Some(json!({"code": staff::totp_now(&secret).unwrap()}))).await;
    call(&r, "POST", "/api/staff/me/password", Some(&token), Some(json!({"current_password": password, "new_password": "Temp-pass-67890"}))).await;

    // The shared key takes the lock; the staff write is refused.
    let (status, lock) = call(&r, "POST", &format!("/api/tax/returns/{id}/lock"), Some("k"), Some(json!({}))).await;
    assert_eq!(status, 200, "{lock}");
    assert_eq!(lock["holder"], "Shared workspace key");
    let revision = record["revision"].clone();
    let (status, value) = call(&r, "PUT", &format!("/api/tax/returns/{id}"), Some(&token), Some(update_body("A", revision.clone()))).await;
    assert_eq!(status, 409, "{value}");
    assert!(value["error"].as_str().unwrap().contains("editing lock"));

    // Takeover moves the lock to the staff account; the shared key is then refused.
    let (status, taken) = call(&r, "POST", &format!("/api/tax/returns/{id}/lock"), Some(&token), Some(json!({"takeover": true}))).await;
    assert_eq!(status, 200, "{taken}");
    assert!(taken["holder"].as_str().unwrap().contains("Pat Preparer"));
    let (status, value) = call(&r, "PUT", &format!("/api/tax/returns/{id}"), Some("k"), Some(update_body("A", revision))).await;
    assert_eq!(status, 409, "{value}");

    // The holder releases; the shared key can write again.
    let (status, _) = call(&r, "DELETE", &format!("/api/tax/returns/{id}/lock"), Some(&token), None).await;
    assert_eq!(status, 200);
    let (_, current) = call(&r, "GET", &format!("/api/tax/returns/{id}"), Some("k"), None).await;
    let (status, value) = call(&r, "PUT", &format!("/api/tax/returns/{id}"), Some("k"), Some(update_body("A", current["revision"].clone()))).await;
    assert_eq!(status, 200, "{value}");
}
