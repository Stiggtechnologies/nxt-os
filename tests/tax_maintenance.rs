//! Read-only maintenance dry run for the tax workspace.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::Value;
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k").body(Body::empty()).unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn maintenance_check_is_a_read_only_dry_run() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, report) = call(&r, "GET", "/api/tax/maintenance/check").await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["ok"], true);
    assert_eq!(report["returns"], 0);
    assert_eq!(report["families"], 0);
    assert_eq!(report["checks"].as_array().unwrap().len(), 4);
    assert_eq!(report["state_sha256"].as_str().unwrap().len(), 64);
    assert!(report["note"].as_str().unwrap().contains("Read-only"));
    let names: Vec<&str> = report["checks"].as_array().unwrap().iter().map(|c| c["check"].as_str().unwrap()).collect();
    assert!(names.contains(&"duplicate return ids"));
    assert!(names.contains(&"families reference stored returns"));
    assert!(names.contains(&"batch archives reference stored returns"));

    // No write path: the endpoint is GET-only.
    let (status, _) = call(&r, "POST", "/api/tax/maintenance/check").await;
    assert_eq!(status, 405);
}
