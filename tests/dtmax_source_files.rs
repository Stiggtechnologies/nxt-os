//! Synthetic DT Max original-file preservation. No real client data.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const BASE: &str = "/api/source-imports/dtmax/companies";

async fn raw(router: &axum::Router, method: &str, uri: &str, body: Option<Value>, auth: bool) -> (u16, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(uri);
    if auth { req = req.header("authorization", "Bearer k"); }
    let req = match body {
        Some(v) => req.header("content-type", "application/json").body(Body::from(v.to_string())),
        None => req.body(Body::empty()),
    }.unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    (status, axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec())
}
async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let (s, b) = raw(router, method, uri, body, true).await;
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
fn file(path: &str, year: u16, ft: &str, bytes: &[u8]) -> Value {
    json!({"synthetic": true, "original_path": path, "year": year, "file_type": ft,
           "sha256": hex::encode(Sha256::digest(bytes)), "content_hex": hex::encode(bytes)})
}
fn router() -> axum::Router {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    app_with(Store::memory)
}

#[tokio::test]
async fn files_link_dedupe_and_stay_immutable() {
    let r = router();
    let (s, b) = call(&r, "POST", &format!("{BASE}/co-a/clients"), Some(json!({"client_id":"client-1","dtmax_keys":["smithj"]}))).await;
    assert_eq!(s, 200, "{b}");
    let bytes = b"synthetic dt max 2019 return";
    let (s, b) = call(&r, "POST", &format!("{BASE}/co-a/files"), Some(file("Y:\\DTMax\\2019\\SMITHJ.T19", 2019, "t19", bytes))).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["client_id"], "client-1");
    assert_eq!(b["linkage_rule"], "dtmax_key_exact");
    assert_eq!(b["duplicate_hash"], false);
    assert_eq!(b["immutable"], true);
    let sha = b["sha256"].as_str().unwrap().to_string();
    // Exact replay is idempotent.
    let (s, again) = call(&r, "POST", &format!("{BASE}/co-a/files"), Some(file("Y:\\DTMax\\2019\\SMITHJ.T19", 2019, "t19", bytes))).await;
    assert_eq!(s, 200);
    assert_eq!(again["duplicate_hash"], true);
    assert_eq!(again["path_alias_recorded"], false);
    assert_eq!(again["observed_paths"].as_array().unwrap().len(), 1);
    // Same bytes at another path dedupe by hash; the original record is kept.
    let (_, alias) = call(&r, "POST", &format!("{BASE}/co-a/files"), Some(file("Y:\\Copy\\2019\\SMITHJ.T19", 2019, "t19", bytes))).await;
    assert_eq!(alias["duplicate_hash"], true);
    assert_eq!(alias["path_alias_recorded"], true);
    assert_eq!(alias["original_path"], "Y:\\DTMax\\2019\\SMITHJ.T19");
    assert_eq!(alias["observed_paths"].as_array().unwrap().len(), 2);
    let (s, content) = raw(&r, "GET", &format!("{BASE}/co-a/files/{sha}/content"), None, true).await;
    assert_eq!(s, 200);
    assert_eq!(content, bytes);
    let (_, list) = call(&r, "GET", &format!("{BASE}/co-a/clients/client-1/files"), None).await;
    assert_eq!(list["files"].as_array().unwrap().len(), 1);
    // A linked file cannot be relinked; a key cannot move clients.
    call(&r, "POST", &format!("{BASE}/co-a/clients"), Some(json!({"client_id":"client-2","dtmax_keys":["other"]}))).await;
    let (s, _) = call(&r, "POST", &format!("{BASE}/co-a/files/{sha}/link"), Some(json!({"client_id":"client-2","reviewer_label":"rev"}))).await;
    assert_eq!(s, 409);
    let (s, _) = call(&r, "POST", &format!("{BASE}/co-a/clients"), Some(json!({"client_id":"client-2","dtmax_keys":["SMITHJ"]}))).await;
    assert_eq!(s, 409);
}

#[tokio::test]
async fn unmatched_queue_and_resolution() {
    let r = router();
    let (_, b) = call(&r, "POST", &format!("{BASE}/co-b/files"), Some(file("Y:/DTMax/2014/UNKNOWN.T14", 2014, "t14", b"orphan"))).await;
    assert_eq!(b["unmatched"], true);
    assert!(b["client_id"].is_null());
    let sha = b["sha256"].as_str().unwrap().to_string();
    let (_, q) = call(&r, "GET", &format!("{BASE}/co-b/files/unmatched"), None).await;
    assert_eq!(q["unmatched"], 1);
    let (s, _) = call(&r, "POST", &format!("{BASE}/co-b/files/{sha}/link"), Some(json!({"client_id":"nobody","reviewer_label":"rev"}))).await;
    assert_eq!(s, 404);
    call(&r, "POST", &format!("{BASE}/co-b/clients"), Some(json!({"client_id":"client-9","dtmax_keys":["JONES"]}))).await;
    let (s, b) = call(&r, "POST", &format!("{BASE}/co-b/files/{sha}/link"), Some(json!({"client_id":"client-9","reviewer_label":"rev-od"}))).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["linkage_rule"], "reviewer_resolved");
    let (_, q) = call(&r, "GET", &format!("{BASE}/co-b/files/unmatched"), None).await;
    assert_eq!(q["unmatched"], 0);
}

#[tokio::test]
async fn company_scope_auth_and_validation() {
    let r = router();
    let (_, b) = call(&r, "POST", &format!("{BASE}/co-c/files"), Some(file("Y:/D/2020/A.T20", 2020, "t20", b"x"))).await;
    let sha = b["sha256"].as_str().unwrap().to_string();
    let (s, _) = call(&r, "GET", &format!("{BASE}/co-d/files/{sha}"), None).await;
    assert_eq!(s, 404, "another company cannot read the file");
    let (s, _) = raw(&r, "GET", &format!("{BASE}/co-c/files/{sha}"), None, false).await;
    assert_eq!(s, 401);
    let mut real = file("Y:/D/2020/A.T20", 2020, "t20", b"x");
    real["synthetic"] = json!(false);
    assert_eq!(call(&r, "POST", &format!("{BASE}/co-c/files"), Some(real)).await.0, 409);
    assert_eq!(call(&r, "POST", &format!("{BASE}/co-c/files"), Some(file("Y:/D/2012/A.T12", 2012, "t12", b"x"))).await.0, 400);
    assert_eq!(call(&r, "POST", &format!("{BASE}/co-c/files"), Some(file("Y:/D/2020/A.T20", 2020, "pdf", b"x"))).await.0, 400);
    let mut wrong = file("Y:/D/2020/A.T20", 2020, "t20", b"x");
    wrong["sha256"] = json!("0".repeat(64));
    assert_eq!(call(&r, "POST", &format!("{BASE}/co-c/files"), Some(wrong)).await.0, 400);
}

#[tokio::test]
async fn bulk_synthetic_cohort_dedupes() {
    let r = router();
    call(&r, "POST", &format!("{BASE}/co-e/clients"), Some(json!({"client_id":"c-0","dtmax_keys":["C0000"]}))).await;
    for i in 0..300u32 {
        let year = 2013 + (i % 12) as u16;
        let body = format!("synthetic {}", i % 200);
        let path = format!("Y:/DTMax/{year}/C{:04}.T{:02}", i % 200, year % 100);
        let (s, b) = call(&r, "POST", &format!("{BASE}/co-e/files"), Some(file(&path, year, &format!("t{:02}", year % 100), body.as_bytes()))).await;
        assert_eq!(s, 200, "{b}");
    }
    let (_, q) = call(&r, "GET", &format!("{BASE}/co-e/files/unmatched"), None).await;
    assert_eq!(q["unmatched"], 199, "200 unique hashes, one linked");
}
