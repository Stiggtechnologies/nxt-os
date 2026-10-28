use std::time::Instant;

use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k");
    let req = match body {
        Some(value) => req.header("content-type", "application/json").body(Body::from(value.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn account(index: u64) -> Value {
    json!({
        "source_key": format!("a{index:06}"),
        "code": format!("C{index:06}"),
        "name": format!("Synthetic DT Max account {index}"),
        "amount_state": "cents",
        "amount_cents": "100",
    })
}

fn open_body(records: u64, amount: &str, source_kind: &str, filing: bool) -> Value {
    json!({
        "format": "nxt-dtmax-preservation-batch/v1",
        "cohort_ref": "synthetic-dtmax",
        "synthetic": true,
        "filing_requested": filing,
        "tax_cases_imported": 0,
        "source_kind": source_kind,
        "expected_records": records,
        "expected_accepted_amount_cents": amount,
    })
}

#[tokio::test]
async fn dtmax_vendor_backup_and_filing_stay_blocked() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);
    let (status, body) = call(&router, "POST", "/api/source-imports/dtmax/batches", Some(open_body(1, "0", "dtmax_backup", false))).await;
    assert_eq!(status, 409, "{body}");
    assert!(body["error"].as_str().unwrap().contains("blocked_pending_sample"));
    assert_eq!(body["filing_enabled"], false);
    let (status, body) = call(&router, "POST", "/api/source-imports/dtmax/batches", Some(open_body(1, "0", "declared_accounts", true))).await;
    assert_eq!(status, 409, "{body}");
    assert!(body["error"].as_str().unwrap().contains("filing_disabled"));
}

#[tokio::test]
async fn dtmax_six_thousand_accounts_resume_and_reconcile() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);
    let started = Instant::now();
    let (status, opened) = call(&router, "POST", "/api/source-imports/dtmax/batches", Some(open_body(6_003, "600000", "declared_accounts", false))).await;
    assert_eq!(status, 200, "{opened}");
    let batch = opened["batch_id"].as_str().unwrap();
    assert_eq!(opened["checkpoint_seq"], 0);
    assert_eq!(opened["filing_enabled"], false);
    assert_eq!(opened["live_cutover_allowed"], false);

    let mut records = (0..6_000).map(account).collect::<Vec<_>>();
    records.push(json!({
        "source_key": "a000000",
        "code": "C000000",
        "name": "Duplicate key",
        "amount_state": "cents",
        "amount_cents": "100",
    }));
    records.push(json!({
        "source_key": "bad-amount",
        "code": "BAD",
        "name": "Fractional cents",
        "amount_state": "cents",
        "amount_cents": "100.01",
    }));
    records.push(json!({
        "source_key": "missing-code",
        "code": "",
        "name": "Missing code",
        "amount_state": "absent",
    }));

    let mut seq = 0u32;
    for chunk in records.chunks(500) {
        if seq == 2 {
            let (status, gap) = call(&router, "POST", &format!("/api/source-imports/dtmax/batches/{batch}/chunks"), Some(json!({"seq": 3, "records": chunk}))).await;
            assert_eq!(status, 409, "{gap}");
            let (status, progress) = call(&router, "GET", &format!("/api/source-imports/dtmax/batches/{batch}"), None).await;
            assert_eq!(status, 200, "{progress}");
            assert_eq!(progress["checkpoint_seq"], 2);
            assert_eq!(progress["records_accepted"], 1_000);
            let (status, replay) = call(&router, "POST", &format!("/api/source-imports/dtmax/batches/{batch}/chunks"), Some(json!({"seq": 0, "records": records[..500]}))).await;
            assert_eq!(status, 200, "{replay}");
            assert_eq!(replay["idempotent_replay"], true);
            assert_eq!(replay["records_accepted"], 1_000);
        }
        let (status, body) = call(&router, "POST", &format!("/api/source-imports/dtmax/batches/{batch}/chunks"), Some(json!({"seq": seq, "records": chunk}))).await;
        assert_eq!(status, 200, "seq {seq}: {body}");
        assert_eq!(body["checkpoint_seq"], seq + 1);
        seq += 1;
    }

    let (status, progress) = call(&router, "GET", &format!("/api/source-imports/dtmax/batches/{batch}"), None).await;
    assert_eq!(status, 200, "{progress}");
    assert_eq!(progress["records_submitted"], 6_003);
    assert_eq!(progress["records_accepted"], 6_000);
    assert_eq!(progress["records_excepted"], 3);
    assert_eq!(progress["accepted_amount_cents"], "600000");
    assert_eq!(progress["remaining_records"], 0);
    assert_eq!(progress["complete"], true);

    let (status, early) = call(&router, "GET", &format!("/api/source-imports/dtmax/batches/{batch}/recovery"), None).await;
    assert_eq!(status, 409, "{early}");

    let commit = json!({"request_id": "dtmax-6000", "reviewer_label": "Synthetic reviewer"});
    let (status, receipt) = call(&router, "POST", &format!("/api/source-imports/dtmax/batches/{batch}/commit"), Some(commit.clone())).await;
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(receipt["records_accepted"], 6_000);
    assert_eq!(receipt["records_excepted"], 3);
    assert_eq!(receipt["accepted_amount_cents"], "600000");
    assert_eq!(receipt["tax_cases_imported"], 0);
    assert_eq!(receipt["filing_enabled"], false);
    assert_eq!(receipt["decoder_verified"], false);
    assert_eq!(receipt["vendor_format_accepted"], false);
    assert_eq!(receipt["recovery_matches"], true);
    assert_eq!(receipt["idempotent_replay"], false);
    let (status, replay) = call(&router, "POST", &format!("/api/source-imports/dtmax/batches/{batch}/commit"), Some(commit)).await;
    assert_eq!(status, 200, "{replay}");
    assert_eq!(replay["idempotent_replay"], true);
    assert_eq!(replay["batch_id"], receipt["batch_id"]);
    let (status, recovered) = call(&router, "GET", &format!("/api/source-imports/dtmax/batches/{batch}/recovery"), None).await;
    assert_eq!(status, 200, "{recovered}");
    assert_eq!(recovered["recovery_matches"], true);
    assert_eq!(recovered["records_accepted"], 6_000);
    let elapsed_ms = started.elapsed().as_millis();
    eprintln!("dtmax_6000_elapsed_ms={elapsed_ms}");
    assert!(elapsed_ms < 15_000, "6000-account DT Max preservation took {elapsed_ms} ms");
}
