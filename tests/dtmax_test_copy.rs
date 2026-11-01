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

fn slip(num: &str, row: u32) -> Value {
    json!({"client_number": num, "recipient_name": "TEST_Person", "tax_year": 2023, "source_file_sha256": "b".repeat(64), "row": row,
           "boxes": {"20": "100.0", "21": "90.5", "16": "CAD", "17": "Test Security"}})
}

#[tokio::test]
async fn t5008_slips_load_replay_and_summarize() {
    let r = router();
    let (s, _) = call(&r, "POST", &format!("{BASE}/t5008"), Some(json!({"test_copy": false, "slips": []}))).await;
    assert_eq!(s, 409);
    call(&r, "POST", &format!("{BASE}/clients"), Some(json!({"test_copy": true, "clients": [{"client_number": "101", "name": "Test Person A"}]}))).await;
    let mut bad = slip("101", 3);
    bad["boxes"]["99"] = json!("x");
    let (s, b) = call(&r, "POST", &format!("{BASE}/t5008"), Some(json!({"test_copy": true, "slips": [slip("101", 1), slip("101", 2), slip("0", 1), bad, {"client_number": "101", "sin": "x"}]}))).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!((b["linked"].as_u64(), b["unlinked"].as_u64()), (Some(2), Some(1)), "{b}");
    assert_eq!(b["rejected"]["unknown_box"], 1);
    assert_eq!(b["rejected"]["invalid_fields"], 1);
    assert!(!b.to_string().contains("TEST_Person"));
    let (_, replay) = call(&r, "POST", &format!("{BASE}/t5008"), Some(json!({"test_copy": true, "slips": [slip("101", 1)]}))).await;
    assert_eq!(replay["unchanged_replays"], 1);
    let (_, sum) = call(&r, "GET", &format!("{BASE}/summary"), None).await;
    assert_eq!((sum["t5008_slips"].as_u64(), sum["t5008_slips_unlinked"].as_u64()), (Some(3), Some(1)), "{sum}");
    assert_eq!(sum["t5008_by_year"]["2023"], 3);
}

#[tokio::test]
async fn replace_if_newer_keeps_history_and_unsubscribed_flag() {
    let r = router();
    let (_, b) = call(&r, "POST", &format!("{BASE}/clients"), Some(json!({"test_copy": true, "clients": [
        {"client_number": "101", "name": "Test Person A", "unsubscribed": true}, {"client_number": "102", "name": "B", "unsubscribed": "yes"}]}))).await;
    assert_eq!(b["rejected"]["field_not_bool"], 1, "{b}");
    call(&r, "POST", &format!("{BASE}/returns"), Some(json!({"test_copy": true, "returns": [ret("101", 2024)]}))).await;
    let mut newer = ret("101", 2024); newer["sha256"] = json!("c".repeat(64)); newer["last_saved_epoch"] = json!(1_800_000_000u64);
    let mut older = ret("101", 2024); older["sha256"] = json!("d".repeat(64)); older["last_saved_epoch"] = json!(1_600_000_000u64);
    let (_, b) = call(&r, "POST", &format!("{BASE}/returns"), Some(json!({"test_copy": true, "returns": [newer.clone()]}))).await;
    assert_eq!(b["rejected"]["conflicting_return_for_client_year"], 1, "default refuses");
    let (_, b) = call(&r, "POST", &format!("{BASE}/returns"), Some(json!({"test_copy": true, "replace_if_newer": true, "returns": [newer, older]}))).await;
    assert_eq!(b["replaced_newer"], 1, "{b}");
    assert_eq!(b["rejected"]["conflicting_return_not_newer"], 1);
    let (_, sum) = call(&r, "GET", &format!("{BASE}/summary"), None).await;
    assert_eq!((sum["returns"].as_u64(), sum["returns_replaced_with_history"].as_u64(), sum["clients_unsubscribed"].as_u64()), (Some(1), Some(1), Some(1)), "{sum}");
}
