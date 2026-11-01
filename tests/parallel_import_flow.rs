use std::time::Instant;

use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

const HASH: &str = "abababababababababababababababababababababababababababababababab";

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

fn row(lane: &str, index: u64) -> Value {
    match lane {
        "quickbooks" => {
            let (record_type, amount_state, amount, hash) = match index {
                0 => ("bill", "cents", Some("100"), None),
                1 => ("payment", "cents", Some("100"), None),
                2 => ("payroll_item", "cents", Some("100"), None),
                3 => ("attachment", "absent", None, Some(HASH)),
                _ => ("invoice", "cents", Some("100"), None),
            };
            let mut value = json!({"source_key": format!("qb{index:06}"), "record_type": record_type, "name": format!("Synthetic {record_type} {index}"), "amount_state": amount_state});
            if let Some(cents) = amount { value["amount_cents"] = json!(cents); }
            if let Some(content) = hash { value["content_sha256"] = json!(content); }
            value
        }
        "caseware" => json!({"source_key": format!("cw{index:06}"), "record_type": "engagement_file", "name": format!("Workpaper {index}"), "amount_state": "absent", "content_sha256": HASH}),
        "webtax" => json!({"source_key": format!("wt{index:06}"), "record_type": "archive_member", "name": format!("Archive {index}"), "amount_state": "absent", "content_sha256": HASH}),
        "docusign" => json!({"source_key": format!("ds{index:06}"), "record_type": if index % 2 == 0 { "signed_document" } else { "audit_trail" }, "name": format!("Envelope artifact {index}"), "amount_state": "absent", "content_sha256": HASH}),
        "firm" => json!({"source_key": format!("fm{index:06}"), "record_type": match index % 3 { 0 => "project", 1 => "workflow", _ => "close" }, "name": format!("Firm row {index}"), "amount_state": "absent"}),
        _ => json!({}),
    }
}

fn exceptions(lane: &str) -> Vec<Value> {
    let first = row(lane, 0);
    let key = first["source_key"].as_str().unwrap();
    vec![
        json!({"source_key": key, "record_type": first.get("record_type").and_then(|v| v.as_str()).unwrap_or("account"), "name": "Duplicate", "amount_state": "absent"}),
        json!({"source_key": format!("{lane}-bad"), "record_type": first.get("record_type").and_then(|v| v.as_str()).unwrap_or("invoice"), "name": "Bad hash or amount", "amount_state": "cents", "amount_cents": "10.5"}),
        json!({"source_key": format!("{lane}-blank"), "record_type": first.get("record_type").and_then(|v| v.as_str()).unwrap_or("invoice"), "name": "", "amount_state": "absent"}),
    ]
}

fn amount_for(lane: &str) -> &'static str {
    if lane == "quickbooks" { "599900" } else { "0" }
}

fn format_for(lane: &str) -> &'static str {
    match lane {
        "quickbooks" => "nxt-quickbooks-preservation-batch/v1",
        "caseware" => "nxt-caseware-preservation-batch/v1",
        "webtax" => "nxt-webtax-preservation-batch/v1",
        "docusign" => "nxt-docusign-preservation-batch/v1",
        "firm" => "nxt-firm-preservation-batch/v1",
        _ => "",
    }
}

fn kind_for(lane: &str) -> &'static str {
    match lane {
        "quickbooks" => "declared_documents",
        "caseware" => "declared_files",
        "firm" => "declared_firm",
        _ => "declared_archive",
    }
}

#[tokio::test]
async fn vendor_files_stay_blocked_and_six_thousand_rows_reconcile_in_parallel() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);
    for lane in ["quickbooks", "caseware", "webtax", "docusign", "firm"] {
        let (status, blocked) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches"), Some(json!({
            "format": format_for(lane), "cohort_ref": "synthetic", "synthetic": true, "filing_requested": false,
            "tax_cases_imported": 0, "source_kind": "vendor_backup", "expected_records": 1, "expected_accepted_amount_cents": "0",
        }))).await;
        assert_eq!(status, 409, "{lane} {blocked}");
        assert!(blocked["error"].as_str().unwrap().contains("blocked_pending_sample"), "{lane}");
        assert_eq!(blocked["write_back"], false);

        let started = Instant::now();
        let open = json!({
            "format": format_for(lane), "cohort_ref": format!("synthetic-{lane}"), "synthetic": true, "filing_requested": false,
            "tax_cases_imported": 0, "source_kind": kind_for(lane), "expected_records": 6_003, "expected_accepted_amount_cents": amount_for(lane),
        });
        let (status, opened) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches"), Some(open)).await;
        assert_eq!(status, 200, "{lane} {opened}");
        let batch = opened["batch_id"].as_str().unwrap();
        let mut records = (0..6_000).map(|index| row(lane, index)).collect::<Vec<_>>();
        records.extend(exceptions(lane));
        let mut seq = 0u32;
        for chunk in records.chunks(500) {
            if seq == 2 {
                let (status, gap) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/chunks"), Some(json!({"seq": 9, "records": chunk}))).await;
                assert_eq!(status, 409, "{lane} {gap}");
                let (status, replay) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/chunks"), Some(json!({"seq": 0, "records": &records[..500]}))).await;
                assert_eq!(status, 200, "{lane} {replay}");
                assert_eq!(replay["idempotent_replay"], true);
                assert_eq!(replay["records_accepted"], 1_000);
            }
            let (status, body) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/chunks"), Some(json!({"seq": seq, "records": chunk}))).await;
            assert_eq!(status, 200, "{lane} seq {seq} {body}");
            seq += 1;
        }
        let (status, receipt) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/commit"), Some(json!({"request_id": format!("{lane}-6000"), "reviewer_label": "Synthetic reviewer"}))).await;
        assert_eq!(status, 200, "{lane} {receipt}");
        assert_eq!(receipt["records_accepted"], 6_000, "{lane}");
        assert_eq!(receipt["records_excepted"], 3, "{lane}");
        assert_eq!(receipt["accepted_amount_cents"], amount_for(lane), "{lane}");
        assert_eq!(receipt["filing_enabled"], false);
        assert_eq!(receipt["write_back"], false);
        assert_eq!(receipt["vendor_format_accepted"], false);
        let (status, recovered) = call(&router, "GET", &format!("/api/source-imports/{lane}/batches/{batch}/recovery"), None).await;
        assert_eq!(status, 200, "{lane} {recovered}");
        assert_eq!(recovered["recovery_matches"], true);
        let report_body = json!({"source_records": 6_000, "source_amount_cents": amount_for(lane), "as_of": "2026-10-28"});
        let (status, report) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/parallel"), Some(report_body.clone())).await;
        assert_eq!(status, 200, "{lane} {report}");
        assert_eq!(report["source_of_truth"], "legacy");
        assert_eq!(report["write_back"], false);
        assert_eq!(report["decommission_allowed"], false);
        assert_eq!(report["decommission_decision_owner"], "Orville");
        assert_eq!(report["counts"]["difference"], "0");
        assert_eq!(report["balances"]["difference_cents"], "0");
        assert_eq!(report["counts"]["nxt_excepted"], 3);
        assert!(report["exit_criteria"].as_array().unwrap().iter().any(|item| item["id"] == "orville_decision" && item["met"] == false));
        let (status, again) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/parallel"), Some(report_body)).await;
        assert_eq!(status, 200, "{lane} {again}");
        assert_eq!(again["idempotent_replay"], true);
        assert_eq!(again["report_id"], report["report_id"]);
        let (status, mismatch) = call(&router, "POST", &format!("/api/source-imports/{lane}/batches/{batch}/parallel"), Some(json!({"source_records": 5800, "source_amount_cents": "1", "as_of": "2026-10-28"}))).await;
        assert_eq!(status, 200, "{lane} {mismatch}");
        assert_ne!(mismatch["counts"]["difference"], "0");
        assert_eq!(mismatch["decommission_allowed"], false);
        let elapsed_ms = started.elapsed().as_millis();
        eprintln!("parallel_{lane}_6000_elapsed_ms={elapsed_ms}");
        assert!(elapsed_ms < 15_000, "{lane} took {elapsed_ms} ms");
    }
}
