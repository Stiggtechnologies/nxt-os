//! Reusable preparer text snippets for tax workpaper notes.
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
async fn snippets_are_saved_listed_and_deleted() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, saved) = call(&r, "POST", "/api/tax/snippets", Some(json!({
        "name": "Missing receipts", "text": "Receipts requested from the client on the engagement call."
    }))).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["name"], "Missing receipts");
    call(&r, "POST", "/api/tax/snippets", Some(json!({"name": "Awaiting NOA", "text": "Review pending the 2025 notice of assessment."}))).await;
    let (_, list) = call(&r, "GET", "/api/tax/snippets", None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["name"], "Awaiting NOA", "sorted by name");

    let (status, value) = call(&r, "POST", "/api/tax/snippets", Some(json!({"name": "awaiting noa", "text": "x"}))).await;
    assert_eq!(status, 400, "duplicate name: {value}");
    let (status, _) = call(&r, "POST", "/api/tax/snippets", Some(json!({"name": "  ", "text": "x"}))).await;
    assert_eq!(status, 400);
    let (status, _) = call(&r, "POST", "/api/tax/snippets", Some(json!({"name": "Long", "text": "x".repeat(601)}))).await;
    assert_eq!(status, 400);

    let (status, _) = call(&r, "DELETE", &format!("/api/tax/snippets/{}", saved["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&r, "DELETE", &format!("/api/tax/snippets/{}", saved["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 404);
}
