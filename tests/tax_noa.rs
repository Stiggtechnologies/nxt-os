//! Notice-of-assessment lane: metadata plus optional stored file, per return.
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

fn record(client: &str) -> Value {
    json!({"program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "NOA sample", "client_ref": client, "inputs": {}})
}

#[tokio::test]
async fn notices_are_stored_listed_downloaded_and_deleted() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (_, a) = call(&r, "POST", "/api/tax/returns", Some(record("A"))).await;
    let (_, b) = call(&r, "POST", "/api/tax/returns", Some(record("B"))).await;
    let a_id = a["id"].as_str().unwrap().to_string();
    let b_id = b["id"].as_str().unwrap().to_string();

    let (status, added) = call(&r, "POST", &format!("/api/tax/returns/{a_id}/noa"), Some(json!({
        "received_date": "2026-11-01", "balance_cents": 12300, "note": "2025 NOA received",
        "filename": "noa-2025.pdf", "content_type": "application/pdf", "data_base64": "aGVsbG8="
    }))).await;
    assert_eq!(status, 200, "{added}");
    assert_eq!(added["notice"]["has_file"], true);
    assert_eq!(added["notice"]["client_ref"], "A");
    let noa_id = added["notice"]["id"].as_str().unwrap().to_string();

    // Metadata-only lane entry.
    let (status, _) = call(&r, "POST", &format!("/api/tax/returns/{a_id}/noa"), Some(json!({
        "received_date": "2026-10-15", "filename": "filed-on-paper.pdf"
    }))).await;
    assert_eq!(status, 200);

    let (_, listed) = call(&r, "GET", &format!("/api/tax/returns/{a_id}/noa"), None).await;
    assert_eq!(listed["notices"].as_array().unwrap().len(), 2);
    assert_eq!(listed["notices"][0]["received_date"], "2026-11-01");
    let (_, other) = call(&r, "GET", &format!("/api/tax/returns/{b_id}/noa"), None).await;
    assert_eq!(other["notices"], json!([]));

    let (status, full) = call(&r, "GET", &format!("/api/tax/returns/{a_id}/noa/{noa_id}"), None).await;
    assert_eq!(status, 200, "{full}");
    assert_eq!(full["notice"]["data_base64"], "aGVsbG8=");

    for (body, why) in [
        (json!({"received_date": "2026-13-40", "filename": "x.pdf"}), "bad date"),
        (json!({"received_date": "2026-10-01", "filename": "  "}), "blank filename"),
    ] {
        let (status, value) = call(&r, "POST", &format!("/api/tax/returns/{a_id}/noa"), Some(body)).await;
        assert_eq!(status, 400, "{why}: {value}");
    }

    let (status, _) = call(&r, "DELETE", &format!("/api/tax/returns/{a_id}/noa/{noa_id}"), None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&r, "DELETE", &format!("/api/tax/returns/{a_id}/noa/{noa_id}"), None).await;
    assert_eq!(status, 404);
}
