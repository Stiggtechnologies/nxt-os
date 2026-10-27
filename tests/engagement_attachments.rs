//! Synthetic opaque bytes only. No source engagement, executable or client file.
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use nxt_os::{engagement, firm, ledger, store::Store, tax};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

async fn raw(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, HeaderMap, Vec<u8>) {
    let mut req = Request::builder().method(method).uri(uri);
    if auth {
        req = req.header("authorization", "Bearer k");
    }
    let req = if let Some(body) = body {
        req.header("content-type", "application/json")
            .body(Body::from(body.to_string()))
    } else {
        req.body(Body::empty())
    }
    .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    (
        status,
        headers,
        axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}
async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    status: u16,
) -> Value {
    let (actual, _, bytes) = raw(router, method, uri, body, true).await;
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(actual, status, "{method} {uri}: {value}");
    value
}
fn fixture() -> Value {
    json!({"name":"SYNTHETIC file preservation only","period_start":"2025-01-01","period_end":"2025-12-31", "accounts":[{"id":"cash","code":"001","name":"Synthetic cash","kind":"asset","group_id":"ga","debit_cents":1000},{"id":"capital","code":"003","name":"Synthetic equity","kind":"equity","group_id":"ge","credit_cents":1000}],"groups":[{"id":"ga","code":"A","name":"Synthetic asset","kind":"asset"},{"id":"ge","code":"E","name":"Synthetic equity","kind":"equity"}],"workpapers":[{"id":"paper1","reference":"A","title":"Synthetic evidence","body":"Synthetic text context","required":true}]})
}
fn path(r: &Value, suffix: &str) -> String {
    format!("/api/engagements/{}{suffix}", r["id"].as_str().unwrap())
}
fn upload(r: &Value, id: &str, bytes: &[u8]) -> Value {
    json!({"expected_revision":r["revision"],"upload_id":id,"file_name":"Synthetic Café.pdf","operator_label":"Synthetic preparer","note":"Opaque synthetic test bytes; not a real PDF or client file.","content_hex":hex::encode(bytes)})
}
async fn calculate(router: &axum::Router, r: &Value) -> Value {
    call(
        router,
        "POST",
        &path(r, "/calculate"),
        Some(json!({"expected_revision":r["revision"]})),
        200,
    )
    .await
}
async fn sign(router: &axum::Router, r: &Value, stage: &str) -> Value {
    call(router,"POST",&path(r,"/signoffs"),Some(json!({"expected_revision":r["revision"],"workpaper_id":"paper1","stage":stage,"operator":"Synthetic reviewer","note":"Synthetic file checkpoint"})),200).await
}
async fn transition(router: &axum::Router, r: &Value, status: &str) -> Value {
    call(router,"POST",&path(r,"/transition"),Some(json!({"expected_revision":r["revision"],"status":status,"operator":"Synthetic reviewer","note":"Synthetic file lifecycle check"})),200).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opaque_file_versions_preserve_bytes_review_and_origin_without_financial_writes() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let ledger = Arc::new(ledger::engine::Ledger::new(Store::memory()));
    let tax = Arc::new(tax::Tax::new(Store::memory()));
    let firm = Arc::new(firm::Firm::new(Store::memory(), ledger.clone()));
    let engine = Arc::new(engagement::Engagements::new(
        Store::memory(),
        ledger.clone(),
        tax,
        firm,
    ));
    let router = engagement::router(engine.clone()).merge(ledger::routes::router(ledger));
    let ledger_before = call(&router, "GET", "/api/ledger/entries", None, 200).await;
    let created = call(&router, "POST", "/api/engagements", Some(fixture()), 200).await;
    let route = path(&created, "/workpapers/paper1/attachments");
    let bytes = vec![0, 255, 128, 1, 13, 10, 65];
    let first = upload(&created, "upload_one", &bytes);
    assert_eq!(
        raw(&router, "POST", &route, Some(first.clone()), false)
            .await
            .0,
        401
    );
    let one = call(&router, "POST", &route, Some(first.clone()), 200).await;
    let v1 = one["document"]["workpapers"][0]["attachment_versions"][0].clone();
    assert_eq!(v1["sha256"], hex::encode(Sha256::digest(&bytes)));
    assert_eq!(v1["version"], 1);
    let state_one = engine
        .store
        .get("engagements:state:v1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        call(&router, "POST", &route, Some(first.clone()), 200).await,
        one
    );
    assert_eq!(
        engine
            .store
            .get("engagements:state:v1")
            .await
            .unwrap()
            .unwrap(),
        state_one
    );
    let download = format!("{route}/{}", v1["id"].as_str().unwrap());
    assert_eq!(raw(&router, "GET", &download, None, false).await.0, 401);
    let (status, headers, received) = raw(&router, "GET", &download, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(received, bytes);
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert!(
        headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .contains("filename*=UTF-8''Synthetic%20Caf%C3%A9.pdf")
    );
    assert!(
        headers["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    for variant in [
        "changed_retry",
        "traversal",
        "empty",
        "uppercase",
        "oversized",
        "stale",
    ] {
        let mut bad = upload(&one, "upload_invalid", b"Different");
        match variant {
            "changed_retry" => {
                bad = first.clone();
                bad["content_hex"] = json!("aa");
            }
            "traversal" => bad["file_name"] = json!("../private.pdf"),
            "empty" => bad["content_hex"] = json!(""),
            "uppercase" => bad["content_hex"] = json!("FF"),
            "oversized" => bad["content_hex"] = json!("aa".repeat(256 * 1024 + 1)),
            "stale" => bad["expected_revision"] = created["revision"].clone(),
            _ => unreachable!(),
        }
        call(
            &router,
            "POST",
            &route,
            Some(bad),
            if ["changed_retry", "stale"].contains(&variant) {
                409
            } else {
                400
            },
        )
        .await;
        assert_eq!(
            engine
                .store
                .get("engagements:state:v1")
                .await
                .unwrap()
                .unwrap(),
            state_one
        );
    }
    let mut erase = one["document"].clone();
    erase["expected_revision"] = one["revision"].clone();
    erase["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("attachment_versions");
    call(&router, "PUT", &path(&one, ""), Some(erase.clone()), 409).await;
    erase["workpapers"] = json!([]);
    call(&router, "PUT", &path(&one, ""), Some(erase), 409).await;
    let other = call(&router, "POST", "/api/engagements", Some(fixture()), 200).await;
    assert_eq!(
        raw(
            &router,
            "GET",
            &path(
                &other,
                &format!(
                    "/workpapers/paper1/attachments/{}",
                    v1["id"].as_str().unwrap()
                )
            ),
            None,
            true
        )
        .await
        .0,
        404
    );
    let calculated = calculate(&router, &one).await;
    let prepared = sign(&router, &calculated, "prepared").await;
    let second_bytes = vec![0, 255, 128, 1, 13, 10, 66];
    let two = call(
        &router,
        "POST",
        &route,
        Some(upload(&prepared, "upload_two", &second_bytes)),
        200,
    )
    .await;
    assert!(two["snapshot"].is_null());
    assert_eq!(two["signoffs"], json!([]));
    assert_eq!(
        two["document"]["workpapers"][0]["attachment_versions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        two["snapshots"][0]["input_document"]["workpapers"][0]["attachment_versions"],
        json!([v1])
    );
    assert_eq!(raw(&router, "GET", &download, None, true).await.2, bytes);
    let package = call(&router, "GET", &path(&two, "/package"), None, 200).await;
    assert_eq!(package["attachment_manifest"]["file_bytes_in_json"], false);
    assert!(!package.to_string().contains("content_hex"));
    let bundle_path = path(&two, "/attachments/package");
    assert_eq!(raw(&router, "GET", &bundle_path, None, false).await.0, 401);
    let bundle = raw(&router, "GET", &bundle_path, None, true).await;
    assert_eq!(bundle.0, 200);
    assert_eq!(bundle.1["content-type"], "application/zip");
    assert!(bundle.2.starts_with(b"PK\x03\x04"));
    assert!(bundle.2.windows(bytes.len()).any(|w| w == bytes));
    assert!(
        bundle
            .2
            .windows(second_bytes.len())
            .any(|w| w == second_bytes)
    );
    let mut ready = calculate(&router, &two).await;
    ready = sign(&router, &ready, "prepared").await;
    ready = transition(&router, &ready, "in_review").await;
    ready = sign(&router, &ready, "reviewed").await;
    let locked = transition(&router, &ready, "locked").await;
    call(
        &router,
        "POST",
        &route,
        Some(upload(&locked, "upload_three", b"Third version")),
        409,
    )
    .await;
    assert_eq!(
        call(&router, "POST", &route, Some(first), 200).await,
        locked
    );
    let target=call(&router,"POST",&path(&locked,"/rollforward"),Some(json!({"expected_revision":locked["revision"],"period_start":"2026-01-01","period_end":"2026-12-31"})),200).await;
    assert!(
        target["document"]["workpapers"][0]
            .get("attachment_versions")
            .is_none()
    );
    assert_eq!(
        target["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["attachment_versions"],
        locked["document"]["workpapers"][0]["attachment_versions"]
    );
    let target_bundle = raw(
        &router,
        "GET",
        &path(&target, "/attachments/package"),
        None,
        true,
    )
    .await
    .2;
    let reopened = transition(&router, &locked, "draft").await;
    call(
        &router,
        "POST",
        &route,
        Some(upload(&reopened, "upload_three", b"Third version")),
        200,
    )
    .await;
    assert_eq!(
        raw(
            &router,
            "GET",
            &path(&target, "/attachments/package"),
            None,
            true
        )
        .await
        .2,
        target_bundle
    );
    let baseline = engine
        .store
        .get("engagements:state:v1")
        .await
        .unwrap()
        .unwrap();
    for corruption in ["bytes", "owner", "missing", "metadata", "order"] {
        let mut state: Value = serde_json::from_str(&baseline).unwrap();
        match corruption {
            "bytes" => state["attachment_blobs"][0]["content_hex"] = json!("aa"),
            "owner" => state["attachment_blobs"][0]["engagement_id"] = json!("eng_other"),
            "missing" => {
                state["attachment_blobs"].as_array_mut().unwrap().remove(0);
            }
            "metadata" => {
                state["engagements"][0]["document"]["workpapers"][0]["attachment_versions"][0]["file_name"] =
                    json!("Changed.pdf")
            }
            "order" => {
                state["engagements"][0]["document"]["workpapers"][0]["attachment_versions"][0]["version"] =
                    json!(99)
            }
            _ => unreachable!(),
        }
        let corrupt = state.to_string();
        engine
            .store
            .set("engagements:state:v1", &corrupt)
            .await
            .unwrap();
        assert_eq!(raw(&router, "GET", &download, None, true).await.0, 409);
        call(&router, "GET", &path(&created, "/package"), None, 409).await;
        assert_eq!(
            engine
                .store
                .get("engagements:state:v1")
                .await
                .unwrap()
                .unwrap(),
            corrupt
        );
    }
    engine
        .store
        .set("engagements:state:v1", &baseline)
        .await
        .unwrap();
    assert_eq!(
        call(&router, "GET", "/api/ledger/entries", None, 200).await,
        ledger_before
    );
}
