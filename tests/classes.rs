//! Class/department tagging on posting lines and the P&L-by-class report.
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
async fn classes_tag_postings_and_group_the_profit_and_loss() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let (status, marketing) = call(&r, "POST", "/api/ledger/classes", Some(json!({"name": "Marketing"}))).await;
    assert_eq!(status, 200, "{marketing}");
    let mid = marketing["id"].as_str().unwrap().to_string();
    let (status, value) = call(&r, "POST", "/api/ledger/classes", Some(json!({"name": "marketing"}))).await;
    assert_eq!(status, 400, "duplicate name: {value}");

    // Tagged expense and an untagged sale.
    call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-04-02", "memo": "Ads",
        "lines": [{"account_id": "acc_6000", "debit": 25000, "class_id": mid}, {"account_id": "acc_1000", "credit": 25000}]}))).await;
    call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-04-03", "memo": "Sale",
        "lines": [{"account_id": "acc_1000", "debit": 50000}, {"account_id": "acc_4100", "credit": 50000}]}))).await;

    // Tagged invoice line: the class rides onto the posting.
    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let (status, inv) = call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date": "2026-04-04",
        "lines": [{"description": "Campaign", "qty": 1, "rate": 10000, "account_id": "acc_4100", "tax_code": "NONE", "class_id": mid}]}))).await;
    assert_eq!(status, 200, "{inv}");
    let (_, entry) = call(&r, "GET", &format!("/api/ledger/docs/{}", inv["id"].as_str().unwrap()), None).await;
    let _ = entry;

    let (status, report) = call(&r, "GET", "/api/ledger/reports/profit-loss-by-class?from=2026-04-01&to=2026-04-30", None).await;
    assert_eq!(status, 200, "{report}");
    let rows = report["rows"].as_array().unwrap();
    let m = rows.iter().find(|row| row["class_id"] == mid).unwrap();
    assert_eq!(m["income_cents"], 10000);
    assert_eq!(m["expense_cents"], 25000);
    assert_eq!(m["net_cents"], -15000);
    let unassigned = rows.iter().find(|row| row["class_id"] == "").unwrap();
    assert_eq!(unassigned["income_cents"], 50000);
    assert_eq!(report["totals"]["net_cents"], 50000 + 10000 - 25000);

    // A class in use cannot be deleted; an unused one can.
    let (status, value) = call(&r, "DELETE", &format!("/api/ledger/classes/{mid}"), None).await;
    assert_eq!(status, 409, "{value}");
    let (_, unused) = call(&r, "POST", "/api/ledger/classes", Some(json!({"name": "Unused"}))).await;
    let (status, _) = call(&r, "DELETE", &format!("/api/ledger/classes/{}", unused["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn unknown_classes_are_rejected_on_journal_and_document_lines() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, value) = call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-04-02", "memo": "x",
        "lines": [{"account_id": "acc_6000", "debit": 100, "class_id": "class_missing"}, {"account_id": "acc_1000", "credit": 100}]}))).await;
    assert_eq!(status, 400, "{value}");
    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let (status, value) = call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date": "2026-04-04",
        "lines": [{"description": "x", "qty": 1, "rate": 1000, "account_id": "acc_4100", "tax_code": "NONE", "class_id": "class_missing"}]}))).await;
    assert_eq!(status, 400, "{value}");
}
