//! Whole-book export with a content hash the operator can re-check.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
async fn book_export_bundles_the_books_and_its_hash_matches_the_payload() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    call(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": cust["id"], "date": "2026-10-01",
        "lines": [{"description": "Work", "qty": 1, "rate": 50000, "account_id": "acc_4100", "tax_code": "GST"}]}))).await;

    let (status, export) = call(&r, "GET", "/api/ledger/export", None).await;
    assert_eq!(status, 200, "{export}");
    assert_eq!(export["payload"]["format"], "nxt-ledger-book/v1");
    assert!(!export["payload"]["accounts"].as_array().unwrap().is_empty());
    assert_eq!(export["payload"]["documents"].as_array().unwrap().len(), 1);
    assert_eq!(export["payload"]["contacts"].as_array().unwrap().len(), 1);

    let sha = export["sha256"].as_str().unwrap();
    assert_eq!(sha.len(), 64);
    let recomputed = hex::encode(Sha256::digest(serde_json::to_vec(&export["payload"]).unwrap()));
    assert_eq!(sha, recomputed, "the published hash must match the payload bytes");
}
