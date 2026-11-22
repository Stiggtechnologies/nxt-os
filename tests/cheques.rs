//! Cheque face for recorded bill payments; no MICR, signing or payment initiation.
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
async fn a_bill_payment_renders_a_cheque_face_with_words() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, vend) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "vendor", "name": "Birch Supply"}))).await;
    let (_, bill) = call(&r, "POST", "/api/ledger/bills", Some(json!({"contact_id": vend["id"], "date": "2026-09-01",
        "lines": [{"description": "Stock", "qty": 1, "rate": 12345, "account_id": "acc_5000", "tax_code": "NONE"}]}))).await;
    let (status, pay) = call(&r, "POST", "/api/ledger/payments/made", Some(json!({"contact_id": vend["id"], "date": "2026-09-05",
        "account_id": "acc_1000", "amount": 12345, "applications": [{"doc_id": bill["id"], "amount": 12345}]}))).await;
    assert_eq!(status, 200, "{pay}");


    let (status, cheque) = call(&r, "GET", &format!("/api/ledger/payments/{}/cheque", pay["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200, "{cheque}");
    assert_eq!(cheque["payee"], "Birch Supply");
    assert_eq!(cheque["amount_cents"], 12345);
    assert_eq!(cheque["amount_words"], "one hundred twenty-three dollars and 45/100");
    assert_eq!(cheque["date"], "2026-09-05");
    assert!(cheque["disclaimer"].as_str().unwrap().contains("MICR"));

    // Larger amounts and a memo fall back to the settled document numbers.
    let (_, bill2) = call(&r, "POST", "/api/ledger/bills", Some(json!({"contact_id": vend["id"], "date": "2026-09-01",
        "lines": [{"description": "More", "qty": 1, "rate": 100000, "account_id": "acc_5000", "tax_code": "NONE"}]}))).await;
    let (_, pay2) = call(&r, "POST", "/api/ledger/payments/made", Some(json!({"contact_id": vend["id"], "date": "2026-09-06",
        "account_id": "acc_1000", "amount": 100000, "applications": [{"doc_id": bill2["id"], "amount": 100000}]}))).await;
    let (_, cheque2) = call(&r, "GET", &format!("/api/ledger/payments/{}/cheque", pay2["id"].as_str().unwrap()), None).await;
    assert_eq!(cheque2["amount_words"], "one thousand dollars and 00/100");
    assert_eq!(cheque2["memo"], bill2["number"]);

    let (status, _) = call(&r, "GET", "/api/ledger/payments/does-not-exist/cheque", None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn only_live_payments_made_print_a_cheque() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let (_, inv) = call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date": "2026-09-01",
        "lines": [{"description": "Work", "qty": 1, "rate": 1000, "account_id": "acc_4100", "tax_code": "NONE"}]}))).await;
    let (_, recv) = call(&r, "POST", "/api/ledger/payments/received", Some(json!({"contact_id": cust["id"], "date": "2026-09-02",
        "account_id": "acc_1000", "amount": 1000, "applications": [{"doc_id": inv["id"], "amount": 1000}]}))).await;
    let (status, value) = call(&r, "GET", &format!("/api/ledger/payments/{}/cheque", recv["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 400, "{value}");

    let (_, vend) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "vendor", "name": "Birch Supply"}))).await;
    let (_, bill) = call(&r, "POST", "/api/ledger/bills", Some(json!({"contact_id": vend["id"], "date": "2026-09-01",
        "lines": [{"description": "Stock", "qty": 1, "rate": 500, "account_id": "acc_5000", "tax_code": "NONE"}]}))).await;
    let (_, pay) = call(&r, "POST", "/api/ledger/payments/made", Some(json!({"contact_id": vend["id"], "date": "2026-09-05",
        "account_id": "acc_1000", "amount": 500, "applications": [{"doc_id": bill["id"], "amount": 500}]}))).await;
    call(&r, "POST", &format!("/api/ledger/payments/{}/void", pay["id"].as_str().unwrap()), Some(json!({}))).await;
    let (status, value) = call(&r, "GET", &format!("/api/ledger/payments/{}/cheque", pay["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 409, "{value}");
}
