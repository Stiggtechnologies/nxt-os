//! GIFI summary income statement endpoint.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
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

#[tokio::test]
async fn corporate_returns_render_a_gifi_summary_and_others_refuse() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, corporate) = call(&r, "POST", "/api/tax/returns", Some(json!({
        "program": "T2", "tax_year": 2025, "jurisdiction": "AB", "name": "GIFI sample", "client_ref": "C1",
        "inputs": {"scope_confirmed": true},
        "corporate": {"gifi": [
            {"section": "income_statement", "code": "8000", "label": "Sales", "current_cents": 100000, "prior_cents": 90000},
            {"section": "income_statement", "code": "8521", "label": "Advertising", "current_cents": 40000, "prior_cents": 30000}
        ]}
    }))).await;
    assert_eq!(status, 200, "{corporate}");
    let (status, statement) = call(&r, "GET", &format!("/api/tax/returns/{}/gifi-statement", corporate["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200, "{statement}");
    assert_eq!(statement["revenue_cents"], 100000);
    assert_eq!(statement["expense_cents"], 40000);
    assert_eq!(statement["net_cents"], 60000);
    assert_eq!(statement["matches_declared_net"], Value::Null);
    assert_eq!(statement["lines"].as_array().unwrap().len(), 2);
    assert!(statement["note"].as_str().unwrap().contains("not a complete GIFI S125"));

    let (_, plain) = call(&r, "POST", "/api/tax/returns", Some(json!({
        "program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "No GIFI", "client_ref": "C2", "inputs": {}
    }))).await;
    let (status, value) = call(&r, "GET", &format!("/api/tax/returns/{}/gifi-statement", plain["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 400, "{value}");
}
