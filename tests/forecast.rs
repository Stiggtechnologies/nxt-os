//! Cash forecast: recorded invoices, bills and scheduled recurring entries by week.
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
async fn forecast_buckets_recorded_documents_and_scheduled_entries_by_week() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let (_, vend) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "vendor", "name": "Birch Supply"}))).await;
    call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date": "2026-12-01", "due_date": "2026-12-10",
        "lines": [{"description": "Work", "qty": 1, "rate": 50000, "account_id": "acc_4100", "tax_code": "HST13"}]}))).await;
    call(&r, "POST", "/api/ledger/bills", Some(json!({"contact_id": vend["id"], "date": "2026-12-01", "due_date": "2026-12-20",
        "lines": [{"description": "Stock", "qty": 1, "rate": 20000, "account_id": "acc_5000", "tax_code": "NONE"}]}))).await;
    call(&r, "POST", "/api/ledger/recurring", Some(json!({"name": "Owner deposit", "frequency": "monthly", "next_date": "2026-12-15",
        "lines": [{"account_id": "acc_1000", "debit": 10000}, {"account_id": "acc_4100", "credit": 10000}]}))).await;

    let (status, f) = call(&r, "GET", "/api/ledger/reports/forecast?from=2026-12-01&to=2026-12-31", None).await;
    assert_eq!(status, 200, "{f}");
    assert_eq!(f["opening_cash_cents"], 0);
    assert_eq!(f["sources"]["open_invoices"], 1);
    assert_eq!(f["sources"]["open_bills"], 1);
    assert_eq!(f["sources"]["recurring_entries"], 1);
    let b = f["buckets"].as_array().unwrap();
    assert_eq!(b.len(), 5);
    assert_eq!(b[1]["expected_in_cents"], 56500, "invoice due 10 Dec");
    assert_eq!(b[2]["expected_in_cents"], 10000, "scheduled deposit 15 Dec");
    assert_eq!(b[2]["expected_out_cents"], 20000, "bill due 20 Dec");
    assert_eq!(f["closing_cash_cents"], 56500 + 10000 - 20000);
    assert_eq!(b[4]["end"], "2026-12-31");

    // A document outside the window is not in the projection.
    let (_, narrow) = call(&r, "GET", "/api/ledger/reports/forecast?from=2026-12-01&to=2026-12-05", None).await;
    assert_eq!(narrow["sources"]["open_invoices"], 0);
    assert_eq!(narrow["closing_cash_cents"], 0);
}
