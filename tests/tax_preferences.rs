//! Per-operator workspace preferences (workspace key when using the shared key).
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
async fn preferences_default_and_round_trip() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, defaults) = call(&r, "GET", "/api/tax/preferences", None).await;
    assert_eq!(status, 200, "{defaults}");
    assert_eq!(defaults["density"], "comfortable");
    assert_eq!(defaults["default_view"], "worklist");
    assert_eq!(defaults["show_keyword_codes"], true);

    let (status, saved) = call(&r, "PUT", "/api/tax/preferences", Some(json!({
        "density": "compact", "default_view": "reports", "show_keyword_codes": false
    }))).await;
    assert_eq!(status, 200, "{saved}");
    let (_, current) = call(&r, "GET", "/api/tax/preferences", None).await;
    assert_eq!(current["density"], "compact");
    assert_eq!(current["default_view"], "reports");
    assert_eq!(current["show_keyword_codes"], false);
}

#[tokio::test]
async fn preferences_reject_unknown_values() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    for body in [
        json!({"density": "tiny", "default_view": "worklist", "show_keyword_codes": true}),
        json!({"density": "compact", "default_view": "dashboard", "show_keyword_codes": true}),
    ] {
        let (status, value) = call(&r, "PUT", "/api/tax/preferences", Some(body)).await;
        assert_eq!(status, 400, "{value}");
    }
    let (_, current) = call(&r, "GET", "/api/tax/preferences", None).await;
    assert_eq!(current["density"], "comfortable", "a rejected update must not stick");
}
