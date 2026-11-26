//! Client renumbering: the old reference is kept as provenance and staff assignments move.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k");
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

fn record(client: &str) -> Value {
    json!({"program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "Renumbered", "client_ref": client, "inputs": {}})
}

#[tokio::test]
async fn renumbering_moves_returns_and_staff_assignments_and_logs_provenance() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    call(&r, "POST", "/api/staff/users", Some(json!({
        "username": "prep", "display_name": "Pat Preparer", "role": "preparer", "clients": ["A"], "temporary_password": "Temp-pass-12345"
    }))).await;
    call(&r, "POST", "/api/tax/returns", Some(record("A"))).await;
    call(&r, "POST", "/api/tax/returns", Some(record("A"))).await;

    let (status, result) = call(&r, "POST", "/api/tax/clients/renumber", Some(json!({"from": "A", "to": "B"}))).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["returns"], 2);
    assert_eq!(result["staff_accounts"], 1);

    let (_, list) = call(&r, "GET", "/api/tax/returns", None).await;
    assert!(list.as_array().unwrap().iter().all(|row| row["client_ref"] == "B"));

    let (_, renames) = call(&r, "GET", "/api/tax/clients/renames", None).await;
    assert_eq!(renames.as_array().unwrap().len(), 1);
    assert_eq!(renames[0]["from"], "A");
    assert_eq!(renames[0]["to"], "B");

    let (_, users) = call(&r, "GET", "/api/staff/users", None).await;
    let user = users["users"].as_array().unwrap().iter().find(|u| u["username"] == "prep").unwrap();
    assert_eq!(user["clients"], json!(["B"]));
}

#[tokio::test]
async fn renumbering_rejects_unknown_same_and_taken_references() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    call(&r, "POST", "/api/tax/returns", Some(record("A"))).await;
    call(&r, "POST", "/api/tax/returns", Some(record("C"))).await;
    for (body, expect, why) in [
        (json!({"from": "A", "to": "A"}), 400, "unchanged"),
        (json!({"from": "A", "to": "C"}), 409, "target exists"),
        (json!({"from": "NOPE", "to": "B"}), 404, "unknown source"),
    ] {
        let (status, value) = call(&r, "POST", "/api/tax/clients/renumber", Some(body)).await;
        assert_eq!(status, expect, "{why}: {value}");
    }
    let (_, list) = call(&r, "GET", "/api/tax/returns", None).await;
    assert!(list.as_array().unwrap().iter().any(|row| row["client_ref"] == "A"));
    let (_, renames) = call(&r, "GET", "/api/tax/clients/renames", None).await;
    assert_eq!(renames, json!([]), "rejected renumbering must not be logged");
}
