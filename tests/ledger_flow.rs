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
    let s = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (s, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn ok(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, method, uri, body).await;
    assert_eq!(s, 200, "{method} {uri} -> {v}");
    v
}

#[tokio::test]
async fn books_tie_out() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    // Unauthenticated access is refused.
    let req = Request::builder().uri("/api/ledger/accounts").body(Body::empty()).unwrap();
    assert_eq!(r.clone().oneshot(req).await.unwrap().status().as_u16(), 401);

    let accts = ok(&r, "GET", "/api/ledger/accounts", None).await;
    assert!(accts.as_array().unwrap().len() >= 30);

    let cust = ok(&r, "POST", "/api/ledger/contacts", Some(json!({"kind":"customer","name":"Maple Co","terms_days":15}))).await;
    let ven = ok(&r, "POST", "/api/ledger/contacts", Some(json!({"kind":"vendor","name":"Cloud Host Inc"}))).await;
    let cid = cust["id"].as_str().unwrap();
    let vid = ven["id"].as_str().unwrap();

    // Invoice: 10h @ $150 + $500 flat, HST 13% → 2000 + 260 = 2260.00
    let inv = ok(&r, "POST", "/api/ledger/invoices", Some(json!({
        "contact_id": cid, "date": "2026-01-10",
        "lines": [
            {"description":"Consulting","qty":10,"rate":15000,"account_id":"acc_4100","tax_code":"HST13"},
            {"description":"Setup","qty":1,"rate":50000,"account_id":"acc_4100","tax_code":"HST13"}
        ]}))).await;
    assert_eq!(inv["total"], 226000);
    assert_eq!(inv["tax"], 26000);
    assert_eq!(inv["due_date"], "2026-01-25");
    assert_eq!(inv["number"], "INV-1001");

    // Expense on invoice line must be rejected.
    let (s, _) = call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cid, "date":"2026-01-10","lines":[{"description":"x","rate":100,"account_id":"acc_6500"}]}))).await;
    assert_eq!(s, 400);

    // Bill with HST → ITC 1350.
    let bill = ok(&r, "POST", "/api/ledger/bills", Some(json!({
        "contact_id": vid, "date": "2026-01-12",
        "lines": [{"description":"Hosting","rate":40000,"account_id":"acc_6600","tax_code":"HST13"}]}))).await;
    assert_eq!(bill["total"], 45200);

    // Partial payment auto-applied, then overpayment rejected.
    let p = ok(&r, "POST", "/api/ledger/payments/received", Some(json!({"contact_id": cid, "date":"2026-01-20", "account_id":"acc_1000", "amount": 100000}))).await;
    assert_eq!(p["applications"][0]["amount"], 100000);
    let (s, e) = call(&r, "POST", "/api/ledger/payments/received", Some(json!({"contact_id": cid, "date":"2026-01-21", "account_id":"acc_1000", "amount": 999999}))).await;
    assert_eq!(s, 400, "{e}");
    ok(&r, "POST", "/api/ledger/payments/made", Some(json!({"contact_id": vid, "date":"2026-01-22", "account_id":"acc_2200", "amount": 45200}))).await;

    // Card expense entered tax-inclusive: $113.00 gross at 13% → 100 + 13.
    ok(&r, "POST", "/api/ledger/money/out", Some(json!({"account_id":"acc_2200","date":"2026-01-15","memo":"Team lunch","amounts_include_tax":true,
        "lines":[{"account_id":"acc_6200","amount":11300,"tax_code":"HST13"}]}))).await;

    // Unbalanced journal rejected; balanced journal posts.
    let (s, _) = call(&r, "POST", "/api/ledger/entries", Some(json!({"date":"2026-01-02","lines":[{"account_id":"acc_1000","debit":100},{"account_id":"acc_3000","credit":99}]}))).await;
    assert_eq!(s, 400);
    ok(&r, "POST", "/api/ledger/entries", Some(json!({"date":"2026-01-02","memo":"Owner investment","lines":[{"account_id":"acc_1000","debit":500000},{"account_id":"acc_3000","credit":500000}]}))).await;

    // Bank feed import (dedupes on re-import), categorize with rule.
    let csv = "Date,Description,Amount\n2026-01-28,SHOPIFY PAYOUT 8812,-0\n01/29/2026,\"ADOBE *CREATIVE CLOUD\",-56.49\n2026-01-30,Interest,1.25\n";
    let imp = ok(&r, "POST", "/api/ledger/feed/import", Some(json!({"account_id":"acc_1000","csv":csv}))).await;
    assert_eq!(imp["added"], 2);
    assert_eq!(imp["errors"].as_array().unwrap().len(), 1);
    let imp2 = ok(&r, "POST", "/api/ledger/feed/import", Some(json!({"account_id":"acc_1000","csv":csv}))).await;
    assert_eq!(imp2["duplicates"], 2);
    let feed = ok(&r, "GET", "/api/ledger/feed", None).await;
    let adobe = feed.as_array().unwrap().iter().find(|f| f["item"]["description"].as_str().unwrap().contains("ADOBE")).unwrap();
    let fid = adobe["item"]["id"].as_str().unwrap();
    ok(&r, "POST", &format!("/api/ledger/feed/{fid}/categorize"), Some(json!({"account_id":"acc_6600","tax_code":"HST13","create_rule":true}))).await;
    ok(&r, "POST", "/api/ledger/feed/import", Some(json!({"account_id":"acc_1000","csv":"2026-02-28,ADOBE *CREATIVE CLOUD,-56.49"}))).await;
    let feed = ok(&r, "GET", "/api/ledger/feed", None).await;
    let next = feed.as_array().unwrap().iter().find(|f| f["item"]["date"] == "2026-02-28").unwrap();
    assert_eq!(next["suggestion"]["account_id"], "acc_6600");

    // Trial balance and balance sheet must balance.
    let tb = ok(&r, "GET", "/api/ledger/reports/trial-balance?to=2026-12-31", None).await;
    assert_eq!(tb["balanced"], true, "{tb}");
    let bs = ok(&r, "GET", "/api/ledger/reports/balance-sheet?to=2026-12-31", None).await;
    assert_eq!(bs["balanced"], true, "{bs}");

    // P&L: income 2000.00; expenses 400 + 100 + 49.99 (56.49 incl. 13% → 49.99 + 6.50).
    let pl = ok(&r, "GET", "/api/ledger/reports/profit-loss?from=2026-01-01&to=2026-12-31", None).await;
    assert_eq!(pl["total_income"], 200000);
    assert_eq!(pl["total_expenses"], 40000 + 10000 + 4999);
    assert_eq!(pl["net_income"], 200000 - 54999);

    // GST/HST: collected 260.00, ITCs 52 + 13 + 6.50.
    let tax = ok(&r, "GET", "/api/ledger/reports/sales-tax?from=2026-01-01&to=2026-03-31", None).await;
    assert_eq!(tax["line_105_tax_collected"], 26000);
    assert_eq!(tax["line_108_itcs"], 5200 + 1300 + 650);
    ok(&r, "POST", "/api/ledger/tax-payments", Some(json!({"account_id":"acc_1000","date":"2026-04-15","amount":tax["line_109_net_tax"]}))).await;
    let tax2 = ok(&r, "GET", "/api/ledger/reports/sales-tax?from=2026-01-01&to=2026-12-31", None).await;
    assert_eq!(tax2["line_105_tax_collected"], 26000, "remittance must not count as tax activity");

    // AR aging: 1260.00 open, due 2026-01-25 → 31-60 bucket at 2026-03-01.
    let ar = ok(&r, "GET", "/api/ledger/reports/ar-aging?to=2026-03-01", None).await;
    assert_eq!(ar["totals"]["31_60"], 126000);

    // Reconcile chequing: select owner investment + payment = 5000 + 1000.
    let reg = ok(&r, "GET", "/api/ledger/accounts/acc_1000/register", None).await;
    let refs: Vec<String> = reg.as_array().unwrap().iter()
        .filter(|x| x["source"] == "journal" || x["source"] == "payment_received")
        .map(|x| format!("{}:{}", x["entry_id"].as_str().unwrap(), x["line_index"])).collect();
    let (s, e) = call(&r, "POST", "/api/ledger/reconcile", Some(json!({"account_id":"acc_1000","statement_date":"2026-01-31","statement_balance": 500000,"line_refs": refs}))).await;
    assert_eq!(s, 409, "{e}");
    ok(&r, "POST", "/api/ledger/reconcile", Some(json!({"account_id":"acc_1000","statement_date":"2026-01-31","statement_balance": 600000,"line_refs": refs}))).await;

    // Voiding a paid invoice is blocked until its payment is voided.
    let inv_id = inv["id"].as_str().unwrap();
    let (s, _) = call(&r, "POST", &format!("/api/ledger/docs/{inv_id}/void"), None).await;
    assert_eq!(s, 409);

    // Closing the books blocks earlier postings.
    let mut co = ok(&r, "GET", "/api/ledger/company", None).await;
    co["closing_date"] = json!("2026-01-31");
    ok(&r, "PUT", "/api/ledger/company", Some(co)).await;
    let (s, e) = call(&r, "POST", "/api/ledger/entries", Some(json!({"date":"2026-01-15","lines":[{"account_id":"acc_1000","debit":1},{"account_id":"acc_3000","credit":1}]}))).await;
    assert_eq!(s, 409, "{e}");

    // OS layer: status, insights, and Ask NXT without a key.
    let st = ok(&r, "GET", "/api/os/status", None).await;
    let installed: std::collections::BTreeSet<&str> = st["apps"].as_array().unwrap().iter().map(|app| app["id"].as_str().unwrap()).collect();
    assert_eq!(installed, ["firm", "ledger", "referrals", "tax"].into_iter().collect());
    let ins = ok(&r, "GET", "/api/os/insights", None).await;
    assert!(ins["insights"].is_array());
    unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    let (s, _) = call(&r, "POST", "/api/os/ask", Some(json!({"messages":[{"role":"user","content":"hi"}]}))).await;
    assert_eq!(s, 503);
}
