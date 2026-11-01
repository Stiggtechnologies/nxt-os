use super::*;
use std::sync::atomic::Ordering;
async fn staged() -> (CompanyBooks, PreviewInput) {
    let s = CompanyBooks::new(Store::memory());
    let c:Chunk=serde_json::from_value(json!({"accounts":[{"id":"a","name":"Synthetic A","kind":null},{"id":"b","name":"Synthetic B","kind":null}],"entries":[{"id":"j","date":"2026-01-01","source_id":"synthetic-row-1","memo":"Synthetic","lines":[{"account_id":"a","debit_cents":"1","credit_cents":"0","source_row":1,"memo":"Synthetic"},{"account_id":"b","debit_cents":"0","credit_cents":"1","source_row":2,"memo":"Synthetic"}]}]})).unwrap();
    let h = s.upload("test", c).await.ok().unwrap();
    let input:PreviewInput=serde_json::from_value(json!({"request_id":"preview","expected_head":null,"company":{"name":"Synthetic","source_company_id":"synthetic","currency":"CAD","identity_verified":true,"currency_verified":true},"source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},"chunks":[h["sha256"]],"trial_balance":[{"account_id":"a","debit_cents":"1","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},{"account_id":"b","debit_cents":"0","credit_cents":"1","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}}]})).unwrap();
    (s, input)
}
#[tokio::test]
async fn each_financial_read_loads_source_chunks_once() {
    let (s, input) = staged().await;
    let p = s.preview("test", input).await.ok().unwrap();
    s.commit(
        "test",
        CommitInput {
            preview_id: p["preview_id"].as_str().unwrap().into(),
            expected_head: None,
            request_id: "commit".into(),
            reviewer_label: "Synthetic".into(),
            needs_review: false,
            review_blockers: vec![],
            suspense_account_id: None,
        },
    )
    .await
    .ok()
    .unwrap();
    s.source_loads.store(0, Ordering::Relaxed);
    s.entries("test", 0, 100, None).await.ok().unwrap();
    assert_eq!(s.source_loads.load(Ordering::Relaxed), 1);
    s.source_loads.store(0, Ordering::Relaxed);
    s.entries("test", 0, 100, Some("a")).await.ok().unwrap();
    assert_eq!(s.source_loads.load(Ordering::Relaxed), 1);
    s.source_loads.store(0, Ordering::Relaxed);
    s.trial_balance("test").await.ok().unwrap();
    assert_eq!(s.source_loads.load(Ordering::Relaxed), 1);
    s.source_loads.store(0, Ordering::Relaxed);
    s.detail("test").await.ok().unwrap();
    assert_eq!(s.source_loads.load(Ordering::Relaxed), 1);
}
#[tokio::test]
async fn interrupted_local_staging_never_publishes_dangling_global_inventory() {
    let (s, input) = staged().await;
    let (a, e) = s.data("test", &input.chunks).await.ok().unwrap();
    let p = Preview {
        format: "nxt-company-book-preview/v1".into(),
        company_id: "test".into(),
        analysis: CompanyBooks::analyze_bounded(&input, &a, &e, 500_000).ok().unwrap(),
        input: input.clone(),
    };
    let id = hash(encoded(&p).ok().unwrap().as_bytes());
    s.immutable(&CompanyBooks::key("test", "preview", &id).ok().unwrap(), &p)
        .await
        .ok()
        .unwrap();
    s.stage_preview_references("test", &id).await.ok().unwrap();
    assert!(s.store.smembers(INDEX).await.unwrap().is_empty());
    assert_eq!(s.list().await.ok().unwrap()["total"], 0);
    assert!(s.current("test").await.ok().unwrap().is_none());
    s.preview("test", input).await.ok().unwrap();
    assert_eq!(s.list().await.ok().unwrap()["total"], 1);
}
fn review_commit(id: &str, needs_review: bool) -> CommitInput {
    CommitInput {
        preview_id: id.into(),
        expected_head: None,
        request_id: "commit-review".into(),
        reviewer_label: "Synthetic".into(),
        needs_review,
        review_blockers: if needs_review {
            vec!["held_line:row=7:amount_without_account".into()]
        } else {
            vec![]
        },
        suspense_account_id: needs_review.then(|| "suspense".to_string()),
    }
}
#[tokio::test]
async fn needs_review_commits_blocked_books_on_test_copy_only() {
    let (s, mut input) = staged().await;
    // Break the trial-balance control so the preview is blocked.
    input.trial_balance[0].debit_cents = "2".into();
    let p = s.preview("test", input).await.ok().unwrap();
    assert_eq!(p["stage"], "assessment_blocked");
    let id = p["preview_id"].as_str().unwrap();
    // Without the flag the strict gate is unchanged.
    let e = s.commit("test", review_commit(id, false)).await.err().unwrap();
    assert_eq!(e.0, StatusCode::CONFLICT);
    let c = s.commit("test", review_commit(id, true)).await.ok().unwrap();
    assert_eq!(c["head"]["review"]["status"], "needs_review");
    let blockers = c["head"]["review"]["blockers"].as_array().unwrap();
    assert!(blockers.iter().any(|b| b == "held_line:row=7:amount_without_account"));
    assert!(blockers.len() > 1);
    // Exact retry is idempotent; the book stays readable and labelled.
    let r = s.commit("test", review_commit(id, true)).await.ok().unwrap();
    assert_eq!(r["idempotent_replay"], true);
    let tb = s.trial_balance("test").await.ok().unwrap();
    assert_eq!(tb["reconciliation"]["review_status"], "needs_review");
    assert_eq!(tb["reconciliation"]["source_controls_passed"], false);
    let list = s.list().await.ok().unwrap();
    assert_eq!(list["companies"][0]["head"]["review"]["status"], "needs_review");
}
#[tokio::test]
async fn needs_review_is_refused_outside_test_copy() {
    let s = CompanyBooks::new(Store::Unavailable { reason: "production".into() });
    let e = s
        .commit("test", review_commit(&"e".repeat(64), true))
        .await
        .err()
        .unwrap();
    assert_eq!(e.0, StatusCode::CONFLICT);
    assert!(e.1.contains("test_copy_only"));
    assert!(!s.test_copy());
}
#[tokio::test]
async fn review_blockers_require_needs_review() {
    let (s, input) = staged().await;
    let p = s.preview("test", input).await.ok().unwrap();
    let mut c = review_commit(p["preview_id"].as_str().unwrap(), false);
    c.review_blockers = vec!["x".into()];
    assert_eq!(s.commit("test", c).await.err().unwrap().0, StatusCode::BAD_REQUEST);
}
