//! Receipt inbox: upload, human matching, download payload and deletion.
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

fn receipt() -> Value {
    json!({"description": "Office chairs", "date": "2026-08-10", "amount_cents": 24999,
        "filename": "receipt.png", "content_type": "image/png", "data_base64": "aGVsbG8="})
}

#[tokio::test]
async fn receipts_upload_match_and_delete() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, added) = call(&r, "POST", "/api/ledger/receipts", Some(receipt())).await;
    assert_eq!(status, 200, "{added}");
    assert_eq!(added["filename"], "receipt.png");
    assert_eq!(added["entry_id"], Value::Null);

    let (_, list) = call(&r, "GET", "/api/ledger/receipts", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0].get("data_base64").is_none(), "the list must not carry the payload");

    let id = added["id"].as_str().unwrap().to_string();
    let (_, full) = call(&r, "GET", &format!("/api/ledger/receipts/{id}"), None).await;
    assert_eq!(full["data_base64"], "aGVsbG8=");

    let (_, entry) = call(&r, "POST", "/api/ledger/entries", Some(json!({"date": "2026-08-10", "memo": "Chairs",
        "lines": [{"account_id": "acc_6300", "debit": 24999}, {"account_id": "acc_1000", "credit": 24999}]}))).await;
    let (status, matched) = call(&r, "POST", &format!("/api/ledger/receipts/{id}/match"), Some(json!({"entry_id": entry["id"]}))).await;
    assert_eq!(status, 200, "{matched}");
    assert_eq!(matched["entry_id"], entry["id"]);

    // Unmatching is explicit too.
    call(&r, "POST", &format!("/api/ledger/receipts/{id}/match"), Some(json!({"entry_id": null}))).await;
    let (_, full) = call(&r, "GET", &format!("/api/ledger/receipts/{id}"), None).await;
    assert_eq!(full["entry_id"], Value::Null);

    let (status, _) = call(&r, "POST", &format!("/api/ledger/receipts/{id}/match"), Some(json!({"entry_id": "je_missing"}))).await;
    assert_eq!(status, 404);

    let (status, _) = call(&r, "DELETE", &format!("/api/ledger/receipts/{id}"), None).await;
    assert_eq!(status, 200);
    let (_, list) = call(&r, "GET", "/api/ledger/receipts", None).await;
    assert_eq!(list, json!([]));
    let (status, _) = call(&r, "DELETE", &format!("/api/ledger/receipts/{id}"), None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn receipt_input_is_bounded() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let mut empty = receipt();
    empty["data_base64"] = json!("  ");
    let (status, _) = call(&r, "POST", "/api/ledger/receipts", Some(empty)).await;
    assert_eq!(status, 400);
    let mut huge = receipt();
    huge["data_base64"] = json!("a".repeat(2_800_001));
    let (status, _) = call(&r, "POST", "/api/ledger/receipts", Some(huge)).await;
    // Either the engine's own bound or the request body limit refuses it.
    assert!(status == 400 || status == 413, "oversized receipt returned {status}");
    let mut negative = receipt();
    negative["amount_cents"] = json!(-1);
    let (status, _) = call(&r, "POST", "/api/ledger/receipts", Some(negative)).await;
    assert_eq!(status, 400);
}
