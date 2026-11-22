//! Revenue by customer and item, including credit-memo reductions.
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
async fn revenue_groups_by_customer_and_item_and_credit_memos_subtract() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, maple) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let (_, birch) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Birch Co"}))).await;
    let mid = maple["id"].as_str().unwrap().to_string();

    call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": mid, "date": "2026-06-01", "lines": [
        {"description": "Consulting", "qty": 2, "rate": 10000, "account_id": "acc_4100", "tax_code": "NONE"},
        {"description": "Training", "qty": 1, "rate": 5000, "account_id": "acc_4100", "tax_code": "NONE"}
    ]}))).await;
    call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": birch["id"], "date": "2026-06-02", "lines": [
        {"description": "Consulting", "qty": 1, "rate": 7000, "account_id": "acc_4100", "tax_code": "NONE"}
    ]}))).await;
    call(&r, "POST", "/api/ledger/credit-memos", Some(json!({"contact_id": mid, "date": "2026-06-05", "lines": [
        {"description": "Consulting", "qty": 1, "rate": 10000, "account_id": "acc_4100", "tax_code": "NONE"}
    ]}))).await;

    let (status, report) = call(&r, "GET", "/api/ledger/reports/item-profitability?from=2026-06-01&to=2026-06-30", None).await;
    assert_eq!(status, 200, "{report}");
    let rows = report["rows"].as_array().unwrap();
    let find = |customer: &str, item: &str| rows.iter().find(|r| r["customer"] == customer && r["item"] == item).unwrap().clone();
    let maple_consulting = find("Maple Co", "Consulting");
    assert_eq!(maple_consulting["quantity"], 1.0);
    assert_eq!(maple_consulting["revenue_cents"], 10000);
    assert_eq!(maple_consulting["documents"], 2);
    assert_eq!(find("Maple Co", "Training")["revenue_cents"], 5000);
    assert_eq!(find("Birch Co", "Consulting")["revenue_cents"], 7000);
    assert_eq!(report["total_cents"], 22000);
    // Highest revenue first.
    assert_eq!(rows[0]["customer"], "Maple Co");
    assert_eq!(rows[0]["item"], "Consulting");

    // A window with no documents reports nothing rather than carrying balances.
    let (_, empty) = call(&r, "GET", "/api/ledger/reports/item-profitability?from=2026-07-01&to=2026-07-31", None).await;
    assert_eq!(empty["rows"], json!([]));
    assert_eq!(empty["total_cents"], 0);
}
