//! Invoice form styles: accent colour and company mark, backwards compatible.
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

fn minimal() -> Value {
    json!({"name": "Stigg", "legal_name": "", "business_number": "", "province": "AB", "currency": "CAD",
        "fiscal_year_start_month": 1, "closing_date": null, "default_tax_code": "GST",
        "invoice_prefix": "INV-", "invoice_footer": "Thanks"})
}

#[tokio::test]
async fn accent_and_mark_round_trip_and_default_on_old_shapes() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    // A company document without the new fields keeps working and gets defaults.
    let (status, saved) = call(&r, "PUT", "/api/ledger/company", Some(minimal())).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["invoice_accent"], "#b8892b");
    assert_eq!(saved["invoice_show_logo"], true);

    let (status, saved) = call(&r, "PUT", "/api/ledger/company", Some({
        let mut body = minimal();
        body["invoice_accent"] = json!("#112233");
        body["invoice_show_logo"] = json!(false);
        body
    })).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["invoice_accent"], "#112233");
    assert_eq!(saved["invoice_show_logo"], false);
    let (_, company) = call(&r, "GET", "/api/ledger/company", None).await;
    assert_eq!(company["invoice_accent"], "#112233");

    for bad in ["red", "#12345", "#gggggg", "112233"] {
        let mut body = minimal();
        body["invoice_accent"] = json!(bad);
        let (status, value) = call(&r, "PUT", "/api/ledger/company", Some(body)).await;
        assert_eq!(status, 400, "{bad}: {value}");
    }
}
