use axum::{body::Body, http::Request};
use nxt_os::{
    app_with,
    source_import::{
        SourceImport,
        preserved_records::{SourceFactBatch, SourceFactPage},
        source_summary_figures::{PROFILE_SHA, SummaryFigureInput},
    },
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
fn digest(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}
fn page(y: u16) -> SourceFactPage {
    serde_json::from_value(
        json!({"kind":"source-summary-figures","offset":0,"limit":25,"source_return_year":y}),
    )
    .unwrap()
}
fn input(v: Value) -> SummaryFigureInput {
    SummaryFigureInput {
        test_copy: true,
        profile_sha256: PROFILE_SHA.into(),
        record: v,
    }
}
fn rows(path: &str) -> Vec<Value> {
    std::fs::read(path)
        .unwrap()
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect()
}
async fn controls(s: &SourceImport, c: &str) -> Vec<Value> {
    vec![
        s.test_copy_summary(c).await.unwrap(),
        s.test_copy_source_fact_summary(c).await.unwrap(),
        s.test_copy_native_slip_summary(c).await.unwrap(),
        s.test_copy_native_supplement_summary(c).await.unwrap(),
        s.test_copy_legacy_owner_summary(c).await.unwrap(),
        s.test_copy_personal_source_summary(c).await.unwrap(),
        s.test_copy_printed_cached_figure_summary(c).await.unwrap(),
        s.test_copy_preserved_pdf_figure_summary(c).await.unwrap(),
    ]
}
#[tokio::test]
async fn summary_routes_require_admin_and_reject_unknown_input() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "summary-local-test");
    }
    let app = app_with(Store::memory);
    for (method, path) in [
        ("POST", "source-summary-figures"),
        ("GET", "source-summary-figures/summary"),
        (
            "GET",
            "clients/100/source-facts?kind=source-summary-figures&source_return_year=2005",
        ),
    ] {
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!(
                        "/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/{path}"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
    }
    let r=app.oneshot(Request::builder().method("POST").uri("/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/source-summary-figures").header("authorization","Bearer summary-local-test").header("content-type","application/json").body(Body::from(json!({"test_copy":true,"profile_sha256":PROFILE_SHA,"record":{}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(r.status(), 400);
}
#[tokio::test]
async fn actual_summary_exact_replay_null_refund_and_existing_sources_preserved() {
    let Ok(path) = std::env::var("NXT_SUMMARY_FIGURE_OBSERVATION") else {
        return;
    };
    let raw = std::fs::read(path).unwrap();
    assert_eq!(
        digest(&raw),
        "327ad9bb4037a532cb3da1eaa6276eeb51b98042acf51c924681b19ab67b856b"
    );
    let row: Value = serde_json::from_slice(&raw).unwrap();
    let store = Store::memory();
    let s = SourceImport::new(store.clone());
    let c = "source-summary-private-qa";
    let mut snapshots = Vec::new();
    let ps: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    let profiles = ps["clients"].as_array().unwrap();
    assert_eq!(profiles.len(), 5856);
    for p in profiles {
        let cid = p["client_number"].as_str().unwrap();
        let k = format!("dtmaxtc:{c}:client:{cid}");
        let v = p.to_string();
        store.set(&k, &v).await.unwrap();
        store
            .sadd(&format!("dtmaxtc:{c}:clients"), cid)
            .await
            .unwrap();
        snapshots.push((k, v));
    }
    let native = rows(&std::env::var("NXT_PRESERVED_NATIVE_FIXTURE").unwrap());
    assert_eq!(native.len(), 32744);
    assert!(
        !native
            .iter()
            .any(|v| v["client_number"] == row["client_number"] && v["tax_year"] == 2005)
    );
    for batch in native.chunks(1000) {
        let r = s
            .test_copy_historical_lines(
                c,
                SourceFactBatch {
                    test_copy: true,
                    records: batch.to_vec(),
                },
            )
            .await
            .unwrap();
        assert!(r["rejected"].as_object().unwrap().is_empty());
    }
    for v in native {
        let k = format!(
            "dtmaxtc:{c}:historical_lines:{}:{}:{}",
            v["client_number"].as_str().unwrap(),
            v["tax_year"],
            v["archive_sha256"].as_str().unwrap()
        );
        snapshots.push((k.clone(), store.get(&k).await.unwrap().unwrap()));
    }
    for (env, kind, pin, count) in [
        (
            "NXT_PRESERVED_CACHED_FIXTURE",
            "printed_cached_figures",
            "502e76e71cc132a5f74154c364ed398e0eae5cbb8c595a2caceff934b2ae0bf6",
            2527,
        ),
        (
            "NXT_PRESERVED_PDF_OBSERVATIONS",
            "preserved_pdf_figures",
            "4a22f166c06c7f22be26b4aba232a86260f5b22ad82fc5c1c59463c80e3a1d51",
            5,
        ),
    ] {
        let values = rows(&std::env::var(env).unwrap());
        assert_eq!(values.len(), count);
        for v in values {
            let id = format!(
                "{}:{}:{}",
                v["client_number"].as_str().unwrap(),
                v["source_return_year"],
                v["pdf"]["sha256"].as_str().unwrap()
            );
            let prefix = format!("dtmaxtc:{c}:{kind}:{pin}");
            let k = format!("{prefix}:record:{id}");
            let raw = v.to_string();
            store.set(&k, &raw).await.unwrap();
            store.sadd(&format!("{prefix}:all"), &id).await.unwrap();
            store
                .sadd(&format!("{prefix}:digests"), &digest(raw.as_bytes()))
                .await
                .unwrap();
            snapshots.push((k, raw));
        }
    }
    let mut metadata_count = 0;
    for env in [
        "NXT_DTMAX_PREPACKET_METADATA",
        "NXT_DTMAX_REVIEWED_PACKET_MANIFEST",
        "NXT_DTMAX_SOURCE_ARCHIVE_MANIFEST",
        "NXT_DTMAX_ADDITIONAL_SOURCE_ARCHIVE_MANIFEST",
    ] {
        let p = std::env::var(env).unwrap();
        let m: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        for v in m["records"].as_array().unwrap() {
            let mut v = v.clone();
            v.as_object_mut().unwrap().remove("file");
            let id = format!(
                "{}:{}:{}",
                v["client_number"].as_str().unwrap(),
                v["tax_year"],
                v["sha256"].as_str().unwrap()
            );
            let k = format!("dtmaxtc:{c}:return_pdf:record:{id}");
            let raw = v.to_string();
            store.set(&k, &raw).await.unwrap();
            store
                .sadd(
                    &format!(
                        "dtmaxtc:{c}:return_pdf:client:{}",
                        v["client_number"].as_str().unwrap()
                    ),
                    &id,
                )
                .await
                .unwrap();
            snapshots.push((k, raw));
            metadata_count += 1;
        }
    }
    assert_eq!(metadata_count, 14234);
    let manifest_path = std::env::var("NXT_DTMAX_SOURCE_ARCHIVE_MANIFEST").unwrap();
    let m: Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let source = m["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["sha256"] == row["pdf"]["sha256"])
        .unwrap();
    let bytes = std::fs::read(
        std::path::Path::new(&manifest_path)
            .parent()
            .unwrap()
            .join(source["file"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(digest(&bytes), row["pdf"]["sha256"]);
    let before = controls(&s, c).await;
    assert_eq!(before[6]["PDF_observation_records"], 2527);
    assert_eq!(before[7]["PDF_observation_records"], 5);
    assert!(
        s.test_copy_source_summary_figures(c, input(row.clone()))
            .await
            .is_err()
    );
    assert_eq!(
        s.test_copy_source_summary_figure_summary(c).await.unwrap()["PDF_observation_records"],
        0
    );
    for (i, b) in bytes.chunks(1048576).enumerate() {
        let k = format!(
            "dtmaxtc:{c}:return_pdf:identification_archive_blob:{}:chunk:{i}",
            source["sha256"].as_str().unwrap()
        );
        let raw = hex::encode(b);
        store.set(&k, &raw).await.unwrap();
        snapshots.push((k, raw));
    }
    let first_chunk = format!(
        "dtmaxtc:{c}:return_pdf:identification_archive_blob:{}:chunk:0",
        source["sha256"].as_str().unwrap()
    );
    let original_chunk = store.get(&first_chunk).await.unwrap().unwrap();
    store.set(&first_chunk, "00").await.unwrap();
    assert!(
        s.test_copy_source_summary_figures(c, input(row.clone()))
            .await
            .is_err()
    );
    assert_eq!(
        s.test_copy_source_summary_figure_summary(c).await.unwrap()["PDF_observation_records"],
        0
    );
    store.set(&first_chunk, &original_chunk).await.unwrap();
    for field in [
        "client_number",
        "source_return_year",
        "native_context_state",
        "observations",
        "pdf",
        "financial_values_approved",
    ] {
        let mut bad = row.clone();
        bad[field] = Value::Null;
        assert!(
            s.test_copy_source_summary_figures(c, input(bad))
                .await
                .is_err()
        );
    }
    let r = s
        .test_copy_source_summary_figures(c, input(row.clone()))
        .await
        .unwrap();
    assert_eq!(r["created"], 1);
    assert_eq!(
        s.test_copy_source_summary_figures(c, input(row.clone()))
            .await
            .unwrap()["unchanged_replays"],
        1
    );
    let stored_key = format!("dtmaxtc:{c}:source_summary_figures:{PROFILE_SHA}:record");
    let saved = store.get(&stored_key).await.unwrap().unwrap();
    store.set(&stored_key, "{}").await.unwrap();
    assert!(
        s.test_copy_source_summary_figures(c, input(row.clone()))
            .await
            .is_err()
    );
    assert_eq!(store.get(&stored_key).await.unwrap().as_deref(), Some("{}"));
    assert!(
        s.test_copy_client_source_summary_figures(
            c,
            row["client_number"].as_str().unwrap(),
            page(2005)
        )
        .await
        .is_err()
    );
    store.set(&stored_key, &saved).await.unwrap();
    let r = s
        .test_copy_client_source_summary_figures(
            c,
            row["client_number"].as_str().unwrap(),
            page(2005),
        )
        .await
        .unwrap();
    assert_eq!(r["records"], json!([row]));
    assert_eq!(
        r["records"][0]["observations"][4]["state"],
        "requested_line_not_printed"
    );
    assert!(r["records"][0]["observations"][4]["printed_cents"].is_null());
    assert_eq!(
        s.test_copy_client_source_summary_figures(c, "999999999", page(2005))
            .await
            .unwrap()["total"],
        0
    );
    assert_eq!(
        s.test_copy_client_source_summary_figures(
            c,
            row["client_number"].as_str().unwrap(),
            page(2006)
        )
        .await
        .unwrap()["total"],
        0
    );
    assert_eq!(
        s.test_copy_source_summary_figure_summary("another-namespace")
            .await
            .unwrap()["PDF_observation_records"],
        0
    );
    assert_eq!(controls(&s, c).await, before);
    for (k, v) in snapshots {
        assert_eq!(
            store.get(&k).await.unwrap().as_deref(),
            Some(v.as_str()),
            "Earlier source or original bytes changed"
        );
    }
    let summary = s.test_copy_source_summary_figure_summary(c).await.unwrap();
    assert_eq!(summary["printed_amount_observations"], 5);
    assert_eq!(summary["requested_lines_not_printed"], 1);
    println!(
        "SOURCE_SUMMARY_EXACT: native=32744; profiles=5856; metadata=14234; printed=2527; preserved=5; numeric=5; missing_refund=null; originals_unchanged=true"
    );
}
