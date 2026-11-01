//! Catch-up and monthly close against a client's company books. The ledger's
//! evidence queues these steps for a reviewer; it never closes them.
use axum::http::StatusCode;
use chrono::NaiveDate;
use nxt_os::client_journey::Actor;
use nxt_os::client_journey::capability::{Capabilities, SourceProfile};
use nxt_os::client_journey::ledger::LedgerClose;
use nxt_os::client_journey::service::{Journeys, StaffAction};
use nxt_os::company_books::{CompanyBooks, models::*};
use nxt_os::store::Store;
use serde_json::{Value, json};
use std::sync::Arc;

fn reviewer() -> Actor {
    Actor::Human { name: "Synthetic reviewer".into(), role: "Reviewer".into() }
}

async fn committed_books(client: &str) -> Arc<CompanyBooks> {
    let books = Arc::new(CompanyBooks::new(Store::memory()));
    let chunk: Chunk = serde_json::from_value(json!({
        "accounts":[{"id":"cash","name":"Cash","kind":"asset"},{"id":"equity","name":"Equity","kind":"equity"}],
        "entries":[{"id":"src-1","date":"2026-01-01","source_id":"archive-row:synthetic:1","memo":"Opening","lines":[
            {"account_id":"cash","debit_cents":"10000","credit_cents":"0","source_row":1,"memo":"Debit"},
            {"account_id":"equity","debit_cents":"0","credit_cents":"10000","source_row":2,"memo":"Credit"}
        ]}]
    })).unwrap();
    let uploaded = books.upload(client, chunk).await.ok().unwrap();
    let input: PreviewInput = serde_json::from_value(json!({
        "request_id":"preview-close","expected_head":null,
        "company":{"name":"Synthetic","source_company_id":"realm","currency":"CAD","identity_verified":true,"currency_verified":true},
        "source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},
        "chunks":[uploaded["sha256"]],
        "trial_balance":[
            {"account_id":"cash","debit_cents":"10000","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},
            {"account_id":"equity","debit_cents":"0","credit_cents":"10000","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}}
        ]
    })).unwrap();
    let preview = books.preview(client, input).await.ok().unwrap();
    books.commit(client, CommitInput { preview_id: preview["preview_id"].as_str().unwrap().into(), expected_head: None, request_id: "commit-close".into(), reviewer_label: "Synthetic reviewer".into(), needs_review: false, review_blockers: vec![], suspense_account_id: None }).await.ok().unwrap();
    books
}

async fn lock(books: &CompanyBooks, client: &str, through: &str) {
    let head = books.detail(client).await.ok().unwrap()["head_sha256"].as_str().unwrap().to_string();
    let input = LockInput { request_id: format!("lock-{through}"), expected_head: head, operator_label: "Synthetic reviewer".into(), locked_through: through.into(), note: "synthetic".into() };
    books.lock_period(client, input).await.ok().unwrap();
}

/// The firm with the ledger capability only, walked by staff up to `until`.
async fn firm_at(books: Arc<CompanyBooks>, client: &str, until: &str) -> Journeys {
    let cap = LedgerClose::new(books).as_of(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    let journeys = Journeys::new(Store::memory(), Capabilities::new().with(Arc::new(cap))).with_policy(r#"{"catch_up_cleanup":"auto"}"#);
    journeys.create(client, SourceProfile::default()).await.map_err(|e| e.1).unwrap();
    let keys: Vec<String> = journeys.def().steps.iter().map(|s| s.key.clone()).collect();
    for step in keys.iter().take_while(|k| k.as_str() != until) {
        let j = journeys.staff(client, step, StaffAction::Complete, reviewer(), true, "").await.map_err(|e| e.1).unwrap();
        if j["status"][step] == "awaiting_approval" {
            journeys.staff(client, step, StaffAction::Approve, reviewer(), true, "approved").await.map_err(|e| e.1).unwrap();
        }
    }
    journeys
}

fn state(j: &Value) -> &str {
    j["last_assessment"]["readiness"]["state"].as_str().unwrap_or("")
}

#[tokio::test]
async fn catch_up_waits_for_a_locked_period_then_for_a_reviewer() {
    let books = committed_books("close-co").await;
    let journeys = firm_at(books.clone(), "close-co", "catch_up_cleanup").await;

    let j = journeys.advance("close-co").await.map_err(|e| e.1).unwrap();
    assert_eq!(state(&j), "blocked");
    assert!(j["last_assessment"]["readiness"]["reasons"][0].as_str().unwrap().contains("no period is locked"), "{j}");

    lock(&books, "close-co", "2026-01-31").await;
    let j = journeys.advance("close-co").await.map_err(|e| e.1).unwrap();
    assert_eq!(state(&j), "ready");
    assert_eq!(j["moved"], json!([]), "an auto override cannot close a locked step");
    let pending = &j["pending_review"];
    assert_eq!(pending["step"], "catch_up_cleanup");
    assert_eq!(pending["evidence"]["locked_through"], "2026-01-31");
    assert!(pending["evidence"]["not_checked"].as_array().unwrap().contains(&json!("bank reconciliation")));

    let err = journeys.staff("close-co", "catch_up_cleanup", StaffAction::Complete, reviewer(), false, "").await.unwrap_err();
    assert_eq!(err.0, StatusCode::FORBIDDEN);
    let err = journeys.staff("close-co", "catch_up_cleanup", StaffAction::Skip, reviewer(), false, "not needed").await.unwrap_err();
    assert_eq!(err.0, StatusCode::FORBIDDEN);

    let j = journeys.staff("close-co", "catch_up_cleanup", StaffAction::Approve, reviewer(), true, "bank reconciled").await.map_err(|e| e.1).unwrap();
    assert!(j["pending_review"].is_null());
    assert_eq!(j["progress"]["current_step"], "monthly_close");
}

#[tokio::test]
async fn monthly_close_needs_the_last_month_end_locked() {
    let books = committed_books("month-co").await;
    let journeys = firm_at(books.clone(), "month-co", "monthly_close").await;

    lock(&books, "month-co", "2026-01-31").await;
    let j = journeys.advance("month-co").await.map_err(|e| e.1).unwrap();
    assert_eq!(state(&j), "blocked");
    assert!(j["last_assessment"]["readiness"]["reasons"][0].as_str().unwrap().contains("2026-02-28"), "{j}");

    lock(&books, "month-co", "2026-02-28").await;
    let j = journeys.advance("month-co").await.map_err(|e| e.1).unwrap();
    assert_eq!(j["pending_review"]["step"], "monthly_close");
}

#[tokio::test]
async fn a_client_without_books_is_blocked_and_not_sent_to_the_agent() {
    let books = Arc::new(CompanyBooks::new(Store::memory()));
    let journeys = firm_at(books, "empty-co", "catch_up_cleanup").await;
    let j = journeys.advance("empty-co").await.map_err(|e| e.1).unwrap();
    assert_eq!(state(&j), "blocked");
    assert!(journeys.work_item("empty-co").await.map_err(|e| e.1).unwrap().is_none());
}
