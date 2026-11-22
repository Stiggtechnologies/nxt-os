//! Assumption-based cash plan: planned monthly flows over today's bank balance.
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
async fn plan_carries_the_bank_balance_forward_by_planned_monthly_flows() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    // 500.00 of opening cash.
    call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-06-30", "memo": "Opening",
        "lines": [{"account_id": "acc_1000", "debit": 50000}, {"account_id": "acc_3000", "credit": 50000}]}))).await;

    let (status, saved) = call(&r, "PUT", "/api/ledger/plan", Some(json!({"monthly_in_cents": 100000, "monthly_out_cents": 40000}))).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["monthly_in_cents"], 100000);
    let (_, plan) = call(&r, "GET", "/api/ledger/plan", None).await;
    assert_eq!(plan["monthly_out_cents"], 40000);

    let (status, p) = call(&r, "GET", "/api/ledger/reports/cash-plan?from=2026-07-01&to=2026-09-30", None).await;
    assert_eq!(status, 200, "{p}");
    assert_eq!(p["opening_cash_cents"], 50000);
    let rows = p["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["month"], "2026-07");
    assert_eq!(rows[0]["net_cents"], 60000);
    assert_eq!(rows[0]["projected_cash_cents"], 110000);
    assert_eq!(rows[2]["projected_cash_cents"], 230000);
    assert_eq!(p["closing_cash_cents"], 230000);

    // Negative assumptions are refused.
    let (status, value) = call(&r, "PUT", "/api/ledger/plan", Some(json!({"monthly_in_cents": -1, "monthly_out_cents": 0}))).await;
    assert_eq!(status, 400, "{value}");
}

#[tokio::test]
async fn empty_plan_is_zero_and_projection_still_returns_months() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, p) = call(&r, "GET", "/api/ledger/reports/cash-plan?from=2026-08-01&to=2026-08-31", None).await;
    assert_eq!(status, 200, "{p}");
    assert_eq!(p["opening_cash_cents"], 0);
    assert_eq!(p["rows"].as_array().unwrap().len(), 1);
    assert_eq!(p["closing_cash_cents"], 0);
}
