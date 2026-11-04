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
            supersedes_head: None,
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
        supersedes_head: None,
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
#[test]
fn durable_needs_review_needs_the_owner_switch() {
    assert!(needs_review_permitted("memory", None));
    assert!(!needs_review_permitted("supabase", None));
    assert!(!needs_review_permitted("supabase", Some("off")));
    assert!(needs_review_permitted("supabase", Some(" ON ")));
    assert!(!needs_review_permitted("redis", Some("on")));
    assert!(!needs_review_permitted("unavailable", Some("on")));
}
#[test]
fn durable_limits_default_to_today_and_cap_at_what_chunks_can_hold() {
    let today = (MAX_BOOK, 100_000, 500_000);
    assert_eq!(durable_limits(None), today);
    assert_eq!(durable_limits(Some("nonsense")), today);
    assert_eq!(durable_limits(Some("1")), today);
    assert_eq!(durable_limits(Some("48")), (48 << 20, 200_000, 1_000_000));
    assert_eq!(durable_limits(Some("9999")).0, 128 << 20);
    assert!(durable_limits(Some("9999")).0 <= 256 * MAX_CHUNK);
}
#[tokio::test]
async fn review_blockers_require_needs_review() {
    let (s, input) = staged().await;
    let p = s.preview("test", input).await.ok().unwrap();
    let mut c = review_commit(p["preview_id"].as_str().unwrap(), false);
    c.review_blockers = vec!["x".into()];
    assert_eq!(s.commit("test", c).await.err().unwrap().0, StatusCode::BAD_REQUEST);
}
async fn staged_for(s: &CompanyBooks, company: &str, source: &str, extra: Option<Entry>) -> PreviewInput {
    let mut c: Chunk = serde_json::from_value(json!({"accounts":[{"id":"a","name":"Synthetic A","kind":null},{"id":"b","name":"Synthetic B","kind":null}],"entries":[{"id":"j","date":"2026-01-01","source_id":"synthetic-row-1","memo":"Synthetic","lines":[{"account_id":"a","debit_cents":"1","credit_cents":"0","source_row":1,"memo":"Synthetic"},{"account_id":"b","debit_cents":"0","credit_cents":"1","source_row":2,"memo":"Synthetic"}]}]})).unwrap();
    if let Some(e) = extra {
        c.entries.push(e);
    }
    let h = s.upload(company, c).await.ok().unwrap();
    serde_json::from_value(json!({"request_id":"preview","expected_head":null,"company":{"name":"Synthetic","source_company_id":source,"currency":"CAD","identity_verified":true,"currency_verified":true},"source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},"chunks":[h["sha256"]],"trial_balance":[{"account_id":"a","debit_cents":"1","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},{"account_id":"b","debit_cents":"0","credit_cents":"1","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}}]})).unwrap()
}
fn plain_commit(id: &str, request: &str) -> CommitInput {
    CommitInput {
        preview_id: id.into(),
        expected_head: None,
        request_id: request.into(),
        reviewer_label: "Synthetic".into(),
        needs_review: false,
        review_blockers: vec![],
        suspense_account_id: None,
        supersedes_head: None,
    }
}
fn derived_close() -> Entry {
    let meta = json!({"format": DERIVED_CLOSE_FORMAT, "fiscal_year_end": "2026-06-30"});
    serde_json::from_value(json!({"id":"y-close","date":"2026-06-30","source_id":"derived-year-end-close:2026-06-30","memo":"Derived close","source_metadata":meta,"lines":[{"account_id":"a","debit_cents":"0","credit_cents":"0","source_row":0,"memo":"Derived","source_metadata":meta},{"account_id":"b","debit_cents":"0","credit_cents":"0","source_row":0,"memo":"Derived","source_metadata":meta}]})).unwrap()
}
async fn needs_review_base(s: &CompanyBooks, company: &str) -> (String, PreviewInput) {
    let mut input = staged_for(s, company, "synthetic", None).await;
    let good = input.clone();
    input.trial_balance[0].debit_cents = "2".into();
    let p = s.preview(company, input).await.ok().unwrap();
    let c = s
        .commit(company, review_commit(p["preview_id"].as_str().unwrap(), true))
        .await
        .ok()
        .unwrap();
    (c["head_sha256"].as_str().unwrap().to_string(), good)
}
#[tokio::test]
async fn needs_review_base_rebaselines_cleanly_and_keeps_history() {
    let s = CompanyBooks::new(Store::memory());
    let (old_head, mut good) = needs_review_base(&s, "test").await;
    good.request_id = "preview-fixed".into();
    let p = s.preview("test", good).await.ok().unwrap();
    assert_eq!(p["stage"], "preview_ready");
    let id = p["preview_id"].as_str().unwrap();
    // Without supersedes_head the refresh stays refused.
    let e = s.commit("test", plain_commit(id, "rebase")).await.err().unwrap();
    assert_eq!(e.0, StatusCode::CONFLICT);
    let mut wrong = plain_commit(id, "rebase");
    wrong.supersedes_head = Some("f".repeat(64));
    assert_eq!(s.commit("test", wrong).await.err().unwrap().0, StatusCode::CONFLICT);
    let mut ok = plain_commit(id, "rebase");
    ok.supersedes_head = Some(old_head.clone());
    let c = s.commit("test", ok.clone()).await.ok().unwrap();
    assert_eq!(c["head"]["supersedes"], old_head.as_str());
    assert!(c["head"].get("review").is_none());
    assert_eq!(s.commit("test", ok).await.ok().unwrap()["idempotent_replay"], true);
    let tb = s.trial_balance("test").await.ok().unwrap();
    assert_eq!(tb["reconciliation"]["source_controls_passed"], true);
    let list = s.list().await.ok().unwrap();
    assert_eq!(list["companies"][0]["head"]["supersedes"], old_head.as_str());
    // The replaced receipt is still stored.
    let key = CompanyBooks::key("test", "receipt", &old_head).ok().unwrap();
    assert!(s.store.get(&key).await.unwrap().is_some());
    // A clean, committed base cannot be re-baselined again.
    let mut again = plain_commit(id, "rebase-2");
    again.supersedes_head = Some(c["head_sha256"].as_str().unwrap().into());
    assert_eq!(s.commit("test", again).await.err().unwrap().0, StatusCode::CONFLICT);
}
#[tokio::test]
async fn rebaseline_refuses_lost_or_foreign_source_data() {
    let s = CompanyBooks::new(Store::memory());
    let (old_head, good) = needs_review_base(&s, "test").await;
    // Different source exports.
    let mut other = good.clone();
    other.request_id = "preview-other".into();
    other.source.journal_sha256 = "e".repeat(64);
    let p = s.preview("test", other).await.ok().unwrap();
    let mut c = plain_commit(p["preview_id"].as_str().unwrap(), "rebase-other");
    c.supersedes_head = Some(old_head.clone());
    c.needs_review = true;
    c.review_blockers = vec!["synthetic".into()];
    assert_eq!(s.commit("test", c).await.err().unwrap().0, StatusCode::CONFLICT);
    // An added entry that is not a labelled derived close.
    let extra: Entry = serde_json::from_value(json!({"id":"x","date":"2026-02-01","source_id":"synthetic-row-3","memo":"Synthetic","lines":[{"account_id":"a","debit_cents":"5","credit_cents":"0","source_row":3,"memo":"Synthetic"},{"account_id":"b","debit_cents":"0","credit_cents":"5","source_row":4,"memo":"Synthetic"}]})).unwrap();
    let mut added = staged_for(&s, "test", "synthetic", Some(extra)).await;
    added.request_id = "preview-added".into();
    let p = s.preview("test", added).await.ok().unwrap();
    let mut c = plain_commit(p["preview_id"].as_str().unwrap(), "rebase-added");
    c.supersedes_head = Some(old_head.clone());
    c.needs_review = true;
    c.review_blockers = vec!["synthetic".into()];
    let e = s.commit("test", c).await.err().unwrap();
    assert_eq!(e.0, StatusCode::CONFLICT);
    assert!(e.1.contains("drop or alter"));
}
#[test]
fn carried_over_accepts_only_unchanged_entries_plus_derived_closes() {
    let accounts: Vec<Account> = serde_json::from_value(json!([{"id":"a","name":"A","kind":null},{"id":"b","name":"B","kind":null}])).unwrap();
    let base: Entry = serde_json::from_value(json!({"id":"j","date":"2026-01-01","source_id":"r1","memo":"m","lines":[{"account_id":"a","debit_cents":"1","credit_cents":"0","source_row":1,"memo":"m"},{"account_id":"b","debit_cents":"0","credit_cents":"1","source_row":2,"memo":"m"}]})).unwrap();
    let old = vec![base.clone()];
    assert!(source_carried_over(&accounts, &old, &accounts, &[base.clone(), derived_close()]));
    let mut changed = base.clone();
    changed.lines[0].debit_cents = "2".into();
    assert!(!source_carried_over(&accounts, &old, &accounts, &[changed]));
    assert!(!source_carried_over(&accounts, &old, &accounts, &[derived_close()]));
    let renamed: Vec<Account> = serde_json::from_value(json!([{"id":"a","name":"Other","kind":null},{"id":"b","name":"B","kind":null}])).unwrap();
    assert!(!source_carried_over(&accounts, &old, &renamed, &[base]));
}
#[tokio::test]
async fn archive_is_terminal_and_keeps_the_book_readable() {
    let s = CompanyBooks::new(Store::memory());
    for company in ["keep", "dupe"] {
        let input = staged_for(&s, company, "synthetic", None).await;
        let p = s.preview(company, input).await.ok().unwrap();
        s.commit(company, plain_commit(p["preview_id"].as_str().unwrap(), "c"))
            .await
            .ok()
            .unwrap();
    }
    let head = s.current("dupe").await.ok().unwrap().unwrap().0;
    let input = |merged: Option<&str>, request: &str| ArchiveInput {
        request_id: request.into(),
        expected_head: head.clone(),
        operator_label: "Synthetic operator".into(),
        merged_into: merged.map(Into::into),
        note: "Synthetic duplicate".into(),
    };
    assert_eq!(s.archive("dupe", input(Some("dupe"), "a1")).await.err().unwrap().0, StatusCode::BAD_REQUEST);
    assert_eq!(s.archive("dupe", input(Some("missing"), "a1")).await.err().unwrap().0, StatusCode::CONFLICT);
    let a = s.archive("dupe", input(Some("keep"), "a1")).await.ok().unwrap();
    assert_eq!(a["head"]["operation"]["kind"], "archive");
    assert_eq!(s.archive("dupe", input(Some("keep"), "a1")).await.ok().unwrap()["idempotent_replay"], true);
    let new_head = a["head_sha256"].as_str().unwrap().to_string();
    let lock = LockInput {
        request_id: "l1".into(),
        expected_head: new_head,
        operator_label: "Synthetic operator".into(),
        locked_through: "2026-12-31".into(),
        note: "Synthetic".into(),
    };
    let e = s.lock_period("dupe", lock).await.err().unwrap();
    assert_eq!(e.0, StatusCode::CONFLICT);
    assert!(e.1.contains("archived"));
    s.trial_balance("dupe").await.ok().unwrap();
    let list = s.list().await.ok().unwrap();
    let dupe = list["companies"].as_array().unwrap().iter().find(|c| c["company_id"] == "dupe").unwrap().clone();
    assert_eq!(dupe["head"]["archived"], true);
    assert_eq!(dupe["head"]["merged_into"], "keep");
}
#[tokio::test]
async fn one_active_book_per_real_source_company() {
    let s = CompanyBooks::new(Store::memory());
    let src = "quickbooks_online:synthetic-co";
    let first = staged_for(&s, "first", src, None).await;
    let p = s.preview("first", first).await.ok().unwrap();
    s.commit("first", plain_commit(p["preview_id"].as_str().unwrap(), "c")).await.ok().unwrap();
    assert_eq!(s.source_owner(src).await.ok().unwrap().as_deref(), Some("first"));
    let second = staged_for(&s, "second", src, None).await;
    let p2 = s.preview("second", second).await.ok().unwrap();
    let id2 = p2["preview_id"].as_str().unwrap().to_string();
    let e = s.commit("second", plain_commit(&id2, "c")).await.err().unwrap();
    assert_eq!(e.0, StatusCode::CONFLICT);
    assert!(e.1.contains("first"));
    assert!(s.current("second").await.ok().unwrap().is_none());
    let head = s.current("first").await.ok().unwrap().unwrap().0;
    s.archive("first", ArchiveInput {
        request_id: "a".into(),
        expected_head: head,
        operator_label: "Synthetic operator".into(),
        merged_into: None,
        note: "Synthetic retire".into(),
    })
    .await
    .ok()
    .unwrap();
    s.commit("second", plain_commit(&id2, "c")).await.ok().unwrap();
    assert_eq!(s.source_owner(src).await.ok().unwrap().as_deref(), Some("second"));
}
