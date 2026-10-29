//! DT Max test-copy roster and return-year index. Fixture rows are invented.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

const BASE: &str = "/api/source-imports/dtmax/companies/test-co/test-copy";

async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k");
    let req = match body {
        Some(v) => req.header("content-type", "application/json").body(Body::from(v.to_string())),
        None => req.body(Body::empty()),
    }.unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let b = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
fn router() -> axum::Router {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    app_with(Store::memory)
}
fn ret(num: &str, year: u16) -> Value {
    json!({"client_number": num, "tax_year": year, "extension": format!("P{:02}", year % 100), "size_bytes": 2048,
           "sha256": "a".repeat(64), "header_year": year, "sections": 7, "last_saved_epoch": 1_700_000_000u64,
           "content_state": "vendor_encoded_not_decoded"})
}

#[tokio::test]
async fn roster_requires_explicit_test_copy_flag() {
    let r = router();
    let (s, b) = call(&r, "POST", &format!("{BASE}/clients"), Some(json!({"test_copy": false, "clients": []}))).await;
    assert_eq!(s, 409);
    assert!(b["error"].as_str().unwrap().starts_with("test_copy_flag_required"));
}

#[tokio::test]
async fn roster_upserts_by_client_number_and_counts_rejects() {
    let r = router();
    let rows = json!([
        {"client_number": "101", "name": "Test Person A", "city": "Testville", "source_lists": ["xlsx", "book2"]},
        {"client_number": "102", "name": "Test Person B", "email": "b@example.invalid"},
        {"client_number": "101", "name": "Again"},
        {"client_number": "abc", "name": "Bad"},
        {"client_number": "103"},
        {"client_number": "104", "name": "X", "sin": "000000000"},
        {"client_number": "105", "name": "X", "favourite": "y"}
    ]);
    let (s, b) = call(&r, "POST", &format!("{BASE}/clients"), Some(json!({"test_copy": true, "clients": rows}))).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["created"], 2);
    for reason in ["duplicate_in_batch", "invalid_client_number", "missing_name", "sin_not_accepted", "unknown_field"] {
        assert_eq!(b["rejected"][reason], 1, "{reason}");
    }
    assert!(!b.to_string().contains("Test Person"), "responses carry counts only");
    let (_, again) = call(&r, "POST", &format!("{BASE}/clients"), Some(json!({"test_copy": true, "clients": [rows[0], {"client_number": "102", "name": "Test Person B2", "email": "b2@example.invalid"}]}))).await;
    assert_eq!((again["unchanged"].as_u64(), again["updated"].as_u64()), (Some(1), Some(1)));

    let (s, b) = call(&r, "POST", &format!("{BASE}/returns"), Some(json!({"test_copy": true, "returns": [
        ret("101", 2023), ret("101", 2024), ret("999", 2024),
        {"client_number": "102", "tax_year": 2024, "extension": "P23", "size_bytes": 1, "sha256": "a".repeat(64), "header_year": 2024, "sections": 1, "last_saved_epoch": null, "content_state": "vendor_encoded_not_decoded"}
    ]}))).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!((b["linked"].as_u64(), b["unlinked"].as_u64()), (Some(2), Some(1)));
    assert_eq!(b["rejected"]["extension_does_not_match_year"], 1);
    let (_, replay) = call(&r, "POST", &format!("{BASE}/returns"), Some(json!({"test_copy": true, "returns": [ret("101", 2023)]}))).await;
    assert_eq!(replay["unchanged_replays"], 1);

    let (s, sum) = call(&r, "GET", &format!("{BASE}/summary"), None).await;
    assert_eq!(s, 200);
    assert_eq!(sum["clients"], 2, "{sum}");
    assert_eq!(sum["clients_with_address"], 1, "{sum}");
    assert_eq!(sum["clients_with_email"], 1);
    assert_eq!(sum["returns"], 3);
    assert_eq!(sum["returns_unlinked"], 1);
    assert_eq!(sum["clients_with_returns"], 1);
    assert_eq!(sum["returns_by_year"]["2024"], json!({"linked": 1, "unlinked": 1}));
    assert_eq!(sum["content_decoded"], false);
}
