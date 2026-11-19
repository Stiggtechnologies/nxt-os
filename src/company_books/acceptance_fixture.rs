use super::*;
#[cfg(test)]
use std::sync::atomic::Ordering;

pub(super) async fn staged(store: Store) -> (CompanyBooks, PreviewInput) {
    let s = CompanyBooks::new(store);
    let c:Chunk=serde_json::from_value(json!({"accounts":[{"id":"a","name":"Synthetic A","kind":null},{"id":"b","name":"Synthetic B","kind":null}],"entries":[{"id":"j","date":"2026-01-01","source_id":"synthetic-row-1","memo":"Synthetic","lines":[{"account_id":"a","debit_cents":"1","credit_cents":"0","source_row":1,"memo":"Synthetic"},{"account_id":"b","debit_cents":"0","credit_cents":"1","source_row":2,"memo":"Synthetic"}]}]})).unwrap();
    let h = s.upload("test", c).await.ok().unwrap();
    let input:PreviewInput=serde_json::from_value(json!({"request_id":"preview","expected_head":null,"company":{"name":"Synthetic","source_company_id":"synthetic","currency":"CAD","identity_verified":true,"currency_verified":true},"source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},"chunks":[h["sha256"]],"trial_balance":[{"account_id":"a","debit_cents":"1","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},{"account_id":"b","debit_cents":"0","credit_cents":"1","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}}]})).unwrap();
    (s, input)
}

pub(crate) async fn accounting_close_evidence_case(store: Store) {
    let (books, input) = staged(store).await;
    let preview = books.preview("test", input).await.ok().unwrap();
    let preview_id = preview["preview_id"].as_str().unwrap();
    let commit = books.commit("test", CommitInput {
        preview_id: preview_id.into(),
        expected_head: None,
        request_id: "commit-review".into(),
        reviewer_label: "Synthetic".into(),
        needs_review: false,
        review_blockers: vec![],
        suspense_account_id: None,
        supersedes_head: None,
    }).await.ok().unwrap();
    #[cfg(test)]
    books.source_loads.store(0, Ordering::Relaxed);
    let evidence = crate::accounting_extension::capture_close(&books, "test").await.ok().unwrap();
    assert_eq!(evidence["head_sha256"], commit["head_sha256"]);
    assert_eq!(evidence["source_evidence"]["archive_sha256"], "a".repeat(64));
    #[cfg(test)]
    assert_eq!(books.source_loads.load(Ordering::Relaxed), 1);
    assert!(evidence["checks"].as_array().unwrap().iter().all(|check| check["passed"] == true));
    assert_eq!(evidence["release_authorized"], false);
    assert!(crate::accounting_extension::capture_close(&books, "other").await.is_err());
}
