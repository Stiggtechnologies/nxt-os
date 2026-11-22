//! Customer credit memos: post as a receivable credit, apply to invoices and void.
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

fn customer(name: &str) -> Value {
    json!({"kind": "customer", "name": name})
}

fn invoice(contact_id: &str) -> Value {
    json!({"contact_id": contact_id, "date": "2026-03-01", "lines": [
        {"description": "Consulting", "qty": 1, "rate": 50000, "account_id": "acc_4100", "tax_code": "HST13"}
    ]})
}

fn credit(contact_id: &str, rate: i64) -> Value {
    json!({"contact_id": contact_id, "date": "2026-03-05", "lines": [
        {"description": "Returned work", "qty": 1, "rate": rate, "account_id": "acc_4100", "tax_code": "HST13"}
    ]})
}

#[tokio::test]
async fn credit_memo_posts_reverses_and_applies_without_a_second_entry() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(customer("Maple Co"))).await;
    let cid = cust["id"].as_str().unwrap().to_string();
    let (status, inv) = call(&r, "POST", "/api/ledger/invoices", Some(invoice(&cid))).await;
    assert_eq!(status, 200, "{inv}");
    assert_eq!(inv["total"], 56500);

    let (status, cm) = call(&r, "POST", "/api/ledger/credit-memos", Some(credit(&cid, 10000))).await;
    assert_eq!(status, 200, "{cm}");
    assert_eq!(cm["kind"], "credit_memo");
    assert_eq!(cm["number"], "CM-0001");
    assert_eq!(cm["total"], 11300);
    assert_eq!(cm["paid"], 0);

    // The memo credited AR: 56,500 invoiced less 11,300 credited.
    let (_, tb) = call(&r, "GET", "/api/ledger/reports/trial-balance?to=2026-03-31", None).await;
    let ar = tb["rows"].as_array().unwrap().iter().find(|row| row["account_id"] == "acc_1100").unwrap();
    assert_eq!(ar["debit"].as_i64().unwrap() - ar["credit"].as_i64().unwrap(), 45200);

    let (_, before) = call(&r, "GET", "/api/ledger/entries", None).await;
    let (status, applied) = call(&r, "POST", &format!("/api/ledger/credit-memos/{}/apply", cm["id"].as_str().unwrap()),
        Some(json!({"invoice_id": inv["id"], "amount": 11300}))).await;
    assert_eq!(status, 200, "{applied}");
    assert_eq!(applied["paid"], 11300);
    assert_eq!(applied["total"], 11300);
    let (_, after) = call(&r, "GET", "/api/ledger/entries", None).await;
    assert_eq!(before.as_array().unwrap().len(), after.as_array().unwrap().len(), "applying must not post again");

    let (_, inv_after) = call(&r, "GET", &format!("/api/ledger/docs/{}", inv["id"].as_str().unwrap()), None).await;
    assert_eq!(inv_after["doc"]["paid"], 11300);
    assert_eq!(inv_after["doc"]["total"], 56500);

    // Fully applied: nothing left to apply.
    let (status, value) = call(&r, "POST", &format!("/api/ledger/credit-memos/{}/apply", cm["id"].as_str().unwrap()),
        Some(json!({"invoice_id": inv["id"], "amount": 100}))).await;
    assert_eq!(status, 400, "{value}");

    // Voiding takes the credit back off the invoice.
    let (status, voided) = call(&r, "POST", &format!("/api/ledger/docs/{}/void", cm["id"].as_str().unwrap()), Some(json!({}))).await;
    assert_eq!(status, 200, "{voided}");
    let (_, inv_restored) = call(&r, "GET", &format!("/api/ledger/docs/{}", inv["id"].as_str().unwrap()), None).await;
    assert_eq!(inv_restored["doc"]["paid"], 0);
    let (status, _) = call(&r, "POST", &format!("/api/ledger/docs/{}/void", cm["id"].as_str().unwrap()), Some(json!({}))).await;
    assert_eq!(status, 409);
}

#[tokio::test]
async fn credit_memo_rejects_another_customers_invoice_and_over_application() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, a) = call(&r, "POST", "/api/ledger/contacts", Some(customer("Maple Co"))).await;
    let (_, b) = call(&r, "POST", "/api/ledger/contacts", Some(customer("Birch Co"))).await;
    let (_, a_inv) = call(&r, "POST", "/api/ledger/invoices", Some(invoice(a["id"].as_str().unwrap()))).await;
    let (_, cm) = call(&r, "POST", "/api/ledger/credit-memos", Some(credit(b["id"].as_str().unwrap(), 10000))).await;
    let (status, value) = call(&r, "POST", &format!("/api/ledger/credit-memos/{}/apply", cm["id"].as_str().unwrap()),
        Some(json!({"invoice_id": a_inv["id"], "amount": 5000}))).await;
    assert_eq!(status, 400, "{value}");

    let (_, b_inv) = call(&r, "POST", "/api/ledger/invoices", Some(invoice(b["id"].as_str().unwrap()))).await;
    let (status, value) = call(&r, "POST", &format!("/api/ledger/credit-memos/{}/apply", cm["id"].as_str().unwrap()),
        Some(json!({"invoice_id": b_inv["id"], "amount": 20000}))).await;
    assert_eq!(status, 400, "over-application: {value}");

    let (_, list) = call(&r, "GET", "/api/ledger/credit-memos", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["paid"], 0);
}
