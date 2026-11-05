//! REA 2026-11-05: QuickBooks report parity for the native Ledger —
//! Statement of Cash Flows, P&L by Month, P&L Comparison, Customer/Vendor Balance Summary.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn ok(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k");
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(s, 200, "{method} {uri} -> {v}");
    v
}

fn je(date: &str, lines: Value) -> Value {
    json!({"date": date, "memo": "t", "lines": lines})
}

#[tokio::test]
async fn quickbooks_report_parity() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    // Prior year: $1,000 consulting income, banked.
    ok(&r, "POST", "/api/ledger/entries", Some(je("2025-02-10", json!([{"account_id":"acc_1000","debit":100000},{"account_id":"acc_4100","credit":100000}])))).await;
    // Owner invests $5,000 (financing).
    ok(&r, "POST", "/api/ledger/entries", Some(je("2026-01-02", json!([{"account_id":"acc_1000","debit":500000},{"account_id":"acc_3000","credit":500000}])))).await;
    // Buys equipment $2,000 (investing).
    ok(&r, "POST", "/api/ledger/entries", Some(je("2026-01-05", json!([{"account_id":"acc_1500","debit":200000},{"account_id":"acc_1000","credit":200000}])))).await;
    // Earns $3,000 on account in Jan (operating: AR up), $1,000 cash in Feb.
    ok(&r, "POST", "/api/ledger/entries", Some(je("2026-01-20", json!([{"account_id":"acc_1100","debit":300000},{"account_id":"acc_4100","credit":300000}])))).await;
    ok(&r, "POST", "/api/ledger/entries", Some(je("2026-02-11", json!([{"account_id":"acc_1000","debit":100000},{"account_id":"acc_4100","credit":100000}])))).await;
    // Depreciation $100 (non-cash add-back).
    ok(&r, "POST", "/api/ledger/entries", Some(je("2026-02-28", json!([{"account_id":"acc_6500","debit":10000},{"account_id":"acc_1510","credit":10000}])))).await;

    let cf = ok(&r, "GET", "/api/ledger/reports/cash-flow?from=2026-01-01&to=2026-12-31", None).await;
    assert_eq!(cf["net_income"], 390000);
    assert_eq!(cf["net_cash_operating"], 390000 - 300000 + 10000);
    assert_eq!(cf["net_cash_investing"], -200000);
    assert_eq!(cf["net_cash_financing"], 500000);
    assert_eq!(cf["cash_beginning"], 100000);
    assert_eq!(cf["cash_ending"], 100000 + 500000 - 200000 + 100000);
    assert_eq!(cf["reconciles"], true);

    let m = ok(&r, "GET", "/api/ledger/reports/profit-loss-by-month?from=2026-01-01&to=2026-03-15", None).await;
    let cols = m["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 3);
    assert_eq!(cols[0]["label"], "Jan 2026");
    assert_eq!(cols[0]["net_income"], 300000);
    assert_eq!(cols[1]["net_income"], 90000);
    assert_eq!(cols[2]["to"], "2026-03-15");
    assert_eq!(m["total"]["net_income"], 390000);

    let c = ok(&r, "GET", "/api/ledger/reports/profit-loss-comparison?from=2026-01-01&to=2026-12-31", None).await;
    assert_eq!(c["prior_from"], "2025-01-01");
    assert_eq!(c["comparison"]["total_income"]["current"], 400000);
    assert_eq!(c["comparison"]["total_income"]["prior"], 100000);
    assert_eq!(c["comparison"]["total_income"]["change_pct"], 300.0);

    let cust = ok(&r, "POST", "/api/ledger/contacts", Some(json!({"kind":"customer","name":"Maple Co"}))).await;
    ok(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date":"2026-03-01",
        "lines":[{"description":"Work","qty":1,"rate":50000,"account_id":"acc_4100","tax_code":"HST13"}]}))).await;
    let cb = ok(&r, "GET", "/api/ledger/reports/customer-balances?to=2026-03-31", None).await;
    assert_eq!(cb["total"], 56500);
    assert_eq!(cb["rows"][0]["contact"], "Maple Co");
    assert_eq!(cb["rows"][0]["open_documents"], 1);
    let vb = ok(&r, "GET", "/api/ledger/reports/vendor-balances?to=2026-03-31", None).await;
    assert_eq!(vb["total"], 0);
}
