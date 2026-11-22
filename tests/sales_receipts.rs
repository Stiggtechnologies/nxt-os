//! Sales receipts: an invoice settled in full at creation.
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
async fn sales_receipt_creates_a_paid_invoice_and_a_bank_deposit() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Walk-in"}))).await;

    let (status, receipt) = call(&r, "POST", "/api/ledger/sales-receipts", Some(json!({
        "contact_id": cust["id"], "date": "2026-05-02", "account_id": "acc_1000", "memo": "Counter sale",
        "lines": [{"description": "Repair", "qty": 1, "rate": 10000, "account_id": "acc_4100", "tax_code": "HST13"}]
    }))).await;
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(receipt["doc"]["total"], 11300);
    assert_eq!(receipt["doc"]["paid"], 11300);
    assert_eq!(receipt["payment"]["amount"], 11300);
    assert_eq!(receipt["payment"]["applications"][0]["amount"], 11300);

    let (_, docs) = call(&r, "GET", "/api/ledger/invoices", None).await;
    assert_eq!(docs.as_array().unwrap().len(), 1);
    assert_eq!(docs[0]["balance"], 0);
    let (_, pays) = call(&r, "GET", "/api/ledger/payments", None).await;
    assert_eq!(pays.as_array().unwrap().len(), 1);

    // Bank up 113.00, revenue 100.00, GST/HST payable 13.00.
    let (_, tb) = call(&r, "GET", "/api/ledger/reports/trial-balance?to=2026-05-31", None).await;
    let row = |id: &str| tb["rows"].as_array().unwrap().iter().find(|r| r["account_id"] == id).cloned().unwrap_or(Value::Null);
    assert_eq!(row("acc_1000")["debit"].as_i64().unwrap() - row("acc_1000")["credit"].as_i64().unwrap(), 11300);
    assert_eq!(row("acc_4100")["credit"], 10000);
    assert_eq!(row("acc_2100")["credit"], 1300);
    assert_eq!(tb["balanced"], true);
}
