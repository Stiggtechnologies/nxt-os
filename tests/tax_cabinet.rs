//! Cabinet packet: a printable HTML workpaper for an external document cabinet.
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
async fn cabinet_packet_is_escaped_metadata_and_not_a_certified_form() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, record) = call(&r, "POST", "/api/tax/returns", Some(json!({
        "program": "T1", "tax_year": 2025, "jurisdiction": "AB",
        "name": "Maple & Co <test>", "client_ref": "A/1", "inputs": {}
    }))).await;
    assert_eq!(status, 200, "{record}");
    let id = record["id"].as_str().unwrap();

    let (status, packet) = call(&r, "GET", &format!("/api/tax/returns/{id}/cabinet-export"), None).await;
    assert_eq!(status, 200, "{packet}");
    assert_eq!(packet["format"], "nxt-tax-cabinet/v1");
    assert_eq!(packet["filename"], "A_1-T1-2025.html");
    let html = packet["html"].as_str().unwrap();
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Maple &amp; Co &lt;test&gt;"), "name must be escaped");
    assert!(!html.contains("Maple & Co <test>"), "raw name must not appear");
    assert!(html.contains("not a certified form"));
    assert!(packet["note"].as_str().unwrap().contains("Not a certified form"));
}
