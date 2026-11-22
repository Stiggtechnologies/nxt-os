//! Saved report views: name an existing report with its date/account filters.
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
async fn saved_views_are_listed_deleted_and_validated() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, view) = call(&r, "POST", "/api/ledger/saved-reports", Some(json!({
        "name": "Q1 profit", "report": "profit-loss", "from": "2026-01-01", "to": "2026-03-31", "account_id": "acc_4100"
    }))).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["report"], "profit-loss");
    assert_eq!(view["account_id"], "acc_4100");
    call(&r, "POST", "/api/ledger/saved-reports", Some(json!({"name": "Year to date", "report": "trial-balance", "from": "2026-01-01", "to": "2026-12-31"}))).await;
    let (_, saved) = call(&r, "GET", "/api/ledger/saved-reports", None).await;
    assert_eq!(saved.as_array().unwrap().len(), 2);
    assert_eq!(saved[0]["name"], "Q1 profit");

    for body in [
        json!({"name": "Q1 profit", "report": "profit-loss", "from": "2026-01-01", "to": "2026-03-31"}),
        json!({"name": "Made up", "report": "not-a-report", "from": "2026-01-01", "to": "2026-03-31"}),
        json!({"name": "Bad dates", "report": "profit-loss", "from": "nope", "to": "2026-03-31"}),
        json!({"name": "   ", "report": "profit-loss", "from": "2026-01-01", "to": "2026-03-31"}),
    ] {
        let (status, value) = call(&r, "POST", "/api/ledger/saved-reports", Some(body)).await;
        assert_eq!(status, 400, "{value}");
    }

    let (status, _) = call(&r, "DELETE", &format!("/api/ledger/saved-reports/{}", view["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&r, "DELETE", &format!("/api/ledger/saved-reports/{}", view["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 404);
}
