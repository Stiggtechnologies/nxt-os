//! Payroll settings are a typed, read-only view of the pinned statutory amounts.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::Value;
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k").body(Body::empty()).unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn payroll_settings_expose_the_pinned_rates_and_reject_edits() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, s) = call(&r, "GET", "/api/ledger/payroll-settings").await;
    assert_eq!(status, 200, "{s}");
    assert_eq!(s["jurisdiction"], "Alberta");
    assert_eq!(s["year"], 2026);
    assert_eq!(s["editable"], false);
    assert_eq!(s["cpp_rate_bp"], 595);
    assert_eq!(s["cpp2_rate_bp"], 400);
    assert_eq!(s["ei_rate_bp"], 163);
    assert_eq!(s["cpp_ympe_cents"], 7_460_000);
    assert_eq!(s["federal_brackets"].as_array().unwrap().len(), 4);
    assert_eq!(s["alberta_brackets"].as_array().unwrap().len(), 5);
    assert_eq!(s["sources"].as_array().unwrap().len(), 3);
    assert!(s["note"].as_str().unwrap().contains("cannot be edited"));

    // There is deliberately no write path.
    let (status, _) = call(&r, "POST", "/api/ledger/payroll-settings").await;
    assert_eq!(status, 405);
    let (status, _) = call(&r, "PUT", "/api/ledger/payroll-settings").await;
    assert_eq!(status, 405);
}
