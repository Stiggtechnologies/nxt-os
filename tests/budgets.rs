//! Budget cells and the budget-versus-actual report.
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
async fn budget_cells_compare_against_the_committed_journal() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let (status, cell) = call(&r, "POST", "/api/ledger/budgets", Some(json!({
        "account_id": "acc_5000", "month": "2026-03", "amount": 100000
    }))).await;
    assert_eq!(status, 200, "{cell}");
    assert_eq!(cell["amount"], 100000);
    // A budget outside the reporting window must not be counted.
    call(&r, "POST", "/api/ledger/budgets", Some(json!({"account_id": "acc_5000", "month": "2026-02", "amount": 5000}))).await;
    // Re-posting the same cell replaces it.
    call(&r, "POST", "/api/ledger/budgets", Some(json!({"account_id": "acc_5000", "month": "2026-03", "amount": 100000}))).await;
    let (_, cells) = call(&r, "GET", "/api/ledger/budgets", None).await;
    assert_eq!(cells.as_array().unwrap().len(), 2);

    // March: 600.00 of supplies expense and 1,000.00 of revenue.
    call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-03-10", "memo": "Supplies",
        "lines": [{"account_id": "acc_5000", "debit": 60000}, {"account_id": "acc_1000", "credit": 60000}]}))).await;
    call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-03-12", "memo": "Sale",
        "lines": [{"account_id": "acc_1000", "debit": 100000}, {"account_id": "acc_4100", "credit": 100000}]}))).await;

    let (status, report) = call(&r, "GET", "/api/ledger/reports/budget-vs-actual?from=2026-03-01&to=2026-03-31", None).await;
    assert_eq!(status, 200, "{report}");
    let rows = report["rows"].as_array().unwrap();
    let supplies = rows.iter().find(|row| row["account_id"] == "acc_5000").unwrap();
    assert_eq!(supplies["budget_cents"], 100000);
    assert_eq!(supplies["actual_cents"], 60000);
    assert_eq!(supplies["variance_cents"], -40000);
    assert_eq!(supplies["variance_pct"], -40.0);

    // An account with activity but no budget still appears, with no percentage.
    let revenue = rows.iter().find(|row| row["account_id"] == "acc_4100").unwrap();
    assert_eq!(revenue["budget_cents"], 0);
    assert_eq!(revenue["actual_cents"], 100000);
    assert_eq!(revenue["variance_pct"], Value::Null);

    assert_eq!(report["totals"]["budget_cents"], 100000);
    assert_eq!(report["totals"]["actual_cents"], 160000);

    let (status, _) = call(&r, "DELETE", "/api/ledger/budgets/acc_5000/2026-03", None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&r, "DELETE", "/api/ledger/budgets/acc_5000/2026-03", None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn budget_input_is_validated_before_it_is_stored() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    for (body, why) in [
        (json!({"account_id": "acc_5000", "month": "2026-13", "amount": 1000}), "bad month"),
        (json!({"account_id": "acc_5000", "month": "2026-03", "amount": -1}), "negative amount"),
        (json!({"account_id": "acc_1000", "month": "2026-03", "amount": 1000}), "bank account"),
        (json!({"account_id": "acc_9999", "month": "2026-03", "amount": 1000}), "unknown account"),
    ] {
        let (status, value) = call(&r, "POST", "/api/ledger/budgets", Some(body)).await;
        assert_eq!(status, 400, "{why}: {value}");
    }
    let (_, cells) = call(&r, "GET", "/api/ledger/budgets", None).await;
    assert_eq!(cells, json!([]));
}
