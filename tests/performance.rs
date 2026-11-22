//! Performance centre: trailing-year metrics computed from the ledger alone.
use axum::{body::Body, http::Request};
use chrono::{Duration, Utc};
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
async fn performance_reports_trailing_revenue_margin_and_working_capital() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let today = Utc::now().date_naive();
    let (invoice_date, payment_date) = (today - Duration::days(30), today - Duration::days(25));

    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let (_, vend) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "vendor", "name": "Birch Supply"}))).await;
    let (_, inv) = call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date": invoice_date.to_string(),
        "lines": [{"description": "Work", "qty": 1, "rate": 10000, "account_id": "acc_4100", "tax_code": "NONE"}]}))).await;
    call(&r, "POST", "/api/ledger/bills", Some(json!({"contact_id": vend["id"], "date": invoice_date.to_string(),
        "lines": [{"description": "Stock", "qty": 1, "rate": 4000, "account_id": "acc_5000", "tax_code": "NONE"}]}))).await;
    call(&r, "POST", "/api/ledger/payments/received", Some(json!({"contact_id": cust["id"], "date": payment_date.to_string(),
        "account_id": "acc_1000", "amount": 6000, "applications": [{"doc_id": inv["id"], "amount": 6000}]}))).await;

    let (status, p) = call(&r, "GET", "/api/ledger/reports/performance", None).await;
    assert_eq!(status, 200, "{p}");
    assert_eq!(p["revenue_cents"], 10000);
    assert_eq!(p["expense_cents"], 4000);
    assert_eq!(p["net_cents"], 6000);
    assert_eq!(p["margin_pct"], 60.0);
    assert_eq!(p["cash_cents"], 6000);
    assert_eq!(p["receivable_cents"], 4000);
    assert_eq!(p["payable_cents"], 4000);
    assert_eq!(p["ar_days"], 146.0);
    assert_eq!(p["ap_days"], 365.0);
    assert_eq!(p["top_customers"][0]["customer"], "Maple Co");
    assert_eq!(p["top_customers"][0]["revenue_cents"], 10000);
    assert!(p["monthly"].as_array().unwrap().iter().any(|m| m["revenue_cents"] == 10000));
    assert!(p["note"].as_str().unwrap().contains("No external benchmark"));
}
