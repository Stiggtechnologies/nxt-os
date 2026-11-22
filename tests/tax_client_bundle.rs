//! Per-client export and restore with checksum enforcement.
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

fn return_body(client_ref: &str) -> Value {
    json!({"program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "Synthetic client bundle", "client_ref": client_ref, "inputs": {}})
}

#[tokio::test]
async fn a_client_bundle_exports_and_restores_into_an_empty_workspace() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let a = app_with(Store::memory);
    let (status, record) = call(&a, "POST", "/api/tax/returns", Some(return_body("C1"))).await;
    assert_eq!(status, 200, "{record}");
    let (status, bundle) = call(&a, "GET", &format!("/api/tax/returns/{}/client-export", record["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200, "{bundle}");
    assert_eq!(bundle["format"], "nxt-tax-client/v1");
    assert_eq!(bundle["client_ref"], "C1");
    assert_eq!(bundle["returns"].as_array().unwrap().len(), 1);
    assert_eq!(bundle["sha256"].as_str().unwrap().len(), 64);

    let b = app_with(Store::memory);
    let (status, imported) = call(&b, "POST", "/api/tax/clients/import", Some(bundle.clone())).await;
    assert_eq!(status, 200, "{imported}");
    assert_eq!(imported["imported"], 1);
    assert_eq!(imported["client_ref"], "C1");
    let (_, listed) = call(&b, "GET", "/api/tax/returns", None).await;
    let rows = listed.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["client_ref"], "C1");
    assert_ne!(rows[0]["id"], record["id"], "restored records get fresh ids");

    // Restoring again would duplicate the client.
    let (status, value) = call(&b, "POST", "/api/tax/clients/import", Some(bundle.clone())).await;
    assert_eq!(status, 409, "{value}");
}

#[tokio::test]
async fn a_tampered_bundle_is_refused() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let a = app_with(Store::memory);
    let (_, record) = call(&a, "POST", "/api/tax/returns", Some(return_body("C1"))).await;
    let (_, bundle) = call(&a, "GET", &format!("/api/tax/returns/{}/client-export", record["id"].as_str().unwrap()), None).await;
    let mut tampered = bundle.clone();
    tampered["returns"][0]["name"] = json!("Altered name");
    let b = app_with(Store::memory);
    let (status, value) = call(&b, "POST", "/api/tax/clients/import", Some(tampered)).await;
    assert_eq!(status, 409, "{value}");
    assert!(value["error"].as_str().unwrap().contains("checksum"));
    let (_, listed) = call(&b, "GET", "/api/tax/returns", None).await;
    assert_eq!(listed, json!([]));
}
