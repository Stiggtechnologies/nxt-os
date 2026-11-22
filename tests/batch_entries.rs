//! Batch posting: validate every entry, then post them as one unit.
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

fn entry(memo: &str, amount: i64) -> Value {
    json!({"date": "2026-07-15", "memo": memo, "lines": [
        {"account_id": "acc_6300", "debit": amount},
        {"account_id": "acc_1000", "credit": amount}
    ]})
}

#[tokio::test]
async fn a_valid_batch_posts_every_entry() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, posted) = call(&r, "POST", "/api/ledger/entries/batch", Some(json!({"entries": [entry("One", 1000), entry("Two", 2500)]}))).await;
    assert_eq!(status, 200, "{posted}");
    assert_eq!(posted.as_array().unwrap().len(), 2);
    let (_, entries) = call(&r, "GET", "/api/ledger/entries", None).await;
    assert_eq!(entries.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn one_bad_entry_stops_the_whole_batch() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let mut bad = entry("Bad", 1000);
    bad["lines"][1]["credit"] = json!(900);
    let (status, value) = call(&r, "POST", "/api/ledger/entries/batch", Some(json!({"entries": [entry("Good", 1000), bad]}))).await;
    assert_eq!(status, 400, "{value}");
    let (_, entries) = call(&r, "GET", "/api/ledger/entries", None).await;
    assert_eq!(entries, json!([]), "nothing may be posted when one entry fails");

    let (status, _) = call(&r, "POST", "/api/ledger/entries/batch", Some(json!({"entries": []}))).await;
    assert_eq!(status, 400);
}
