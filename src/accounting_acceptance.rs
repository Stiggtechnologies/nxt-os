use super::{
    RELEASE_GATES, accounting_archive as archive, accounting_authority as authority,
    accounting_release as release,
};
use crate::{
    firm::staff::{Identity, Role},
    store::Store,
};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) const COMPANY: &str = "company-a";
pub(super) const SOURCE: &str = "file-share-2024";

pub(super) fn instant() -> DateTime<Utc> {
    "2026-11-18T12:00:00Z".parse().unwrap()
}
fn day(input: &str) -> NaiveDate {
    input.parse().unwrap()
}
pub(super) fn person(username: &str) -> Identity {
    Identity {
        user_id: username.into(),
        display_name: username.into(),
        role: Role::Admin,
        clients: vec![COMPANY.into()],
        break_glass: false,
        mfa: true,
        must_change_password: false,
    }
}
fn event(sequence: u64, change: Value) -> archive::Event {
    serde_json::from_value(json!({"sequence":sequence,"change":change})).unwrap()
}
pub(super) fn doc(version: &str, text: &str) -> Value {
    json!({"kind":"upsert","document":{
        "id":"doc1", "version":version, "content_sha256":"a".repeat(64),
        "from":"2024-01-01","through":"2024-12-31","user_ids":["usr_reader"],
        "chunks":[{"locator":"page 2","text":text}]
    }})
}
pub(super) async fn send(store: &Store, seq: u64, change: Value) {
    assert_eq!(
        archive::apply(store, COMPANY, SOURCE, event(seq, change), instant())
            .await
            .ok(),
        Some(seq)
    );
}
async fn search(store: &Store, actor: &Identity, now: DateTime<Utc>) -> archive::Search {
    archive::search(
        store,
        actor,
        COMPANY,
        SOURCE,
        archive::SearchQuery {
            query: "rent".into(),
            as_of: day("2024-11-18"),
            embedding: None,
        },
        now,
    )
    .await
    .ok()
    .unwrap()
}
async fn initialized() -> Store {
    let store = Store::memory();
    send(&store, 1, doc("v1", "Rent source amount")).await;
    send(
        &store,
        2,
        json!({"kind":"permissions_observed","observed_at":instant()}),
    )
    .await;
    store
}
fn release_input(complete: bool) -> release::Manifest {
    let mut tests = serde_json::Map::new();
    for id in RELEASE_GATES.iter().flat_map(|(_, _, ids)| ids.iter()) {
        if complete || *id != "LW-07C" {
            tests.insert(
                id.to_string(),
                json!({"result":"pass","artifact_sha256":"a".repeat(64),"artifact_id":id}),
            );
        }
    }
    serde_json::from_value(json!({
        "firm_entity":"firm1","service":"close","client_class":"pilot","jurisdiction":"CA",
        "operation":"post-journal","connector":"internal","environment":"production","region":"CA",
        "release_sha256":"a".repeat(64),"controls_sha256":"b".repeat(64),
        "expires_at":instant()+Duration::days(1),"tests":tests
    }))
    .unwrap()
}
fn rule(version: &str, date_from: &str, date_through: &str) -> authority::RuleInput {
    authority::RuleInput {
        rule: "close-rule".into(),
        version: version.into(),
        publisher: "Synthetic test authority".into(),
        source_url: "https://example.invalid/source".into(),
        source_sha256: "e".repeat(64),
        effective_from: day(date_from),
        effective_through: day(date_through),
        statement: "Synthetic approved fixture rule".into(),
    }
}
fn approval(version: &str, node_hash: String) -> authority::Approval {
    authority::Approval {
        rule: "close-rule".into(),
        version: version.into(),
        node_hash,
    }
}

#[tokio::test]
async fn archive_events_are_ordered_idempotent_and_version_bound() {
    let store = initialized().await;
    send(&store, 3, doc("v2", "Rent corrected")).await;
    send(&store, 3, doc("v2", "Rent corrected")).await;
    for (sequence, change) in [
        (3, doc("v2", "Rent altered")),
        (5, doc("v3", "Rent gap")),
        (4, doc("v1", "Rent replay")),
    ] {
        let error = archive::apply(&store, COMPANY, SOURCE, event(sequence, change), instant())
            .await
            .err()
            .unwrap();
        assert_eq!(error.0, StatusCode::CONFLICT);
    }
    let result = search(&store, &person("usr_reader"), instant()).await;
    assert_eq!(result.citations.len(), 1);
    assert_eq!(result.citations[0].version, "v2");
    assert_eq!(result.citations[0].locator, "page 2");
}

#[tokio::test]
async fn archive_coverage_and_permission_expiry_are_distinct() {
    let actor = person("usr_reader");
    let empty = search(&Store::memory(), &actor, instant()).await;
    assert_eq!(empty.coverage, "unavailable");
    let store = initialized().await;
    let expired = search(&store, &actor, instant() + Duration::seconds(901)).await;
    assert_eq!(expired.coverage, "permission_feed_stale");
    assert!(expired.citations.is_empty());
    let no_match = archive::search(
        &store,
        &actor,
        COMPANY,
        SOURCE,
        archive::SearchQuery {
            query: "missing".into(),
            as_of: day("2024-10-01"),
            embedding: None,
        },
        instant(),
    )
    .await
    .ok()
    .unwrap();
    assert_eq!(no_match.coverage, "authorized_partition_searched");
    assert!(no_match.citations.is_empty());
}

#[tokio::test]
async fn archive_uses_client_scope_and_document_grant_and_rechecks_after_revocation() {
    let store = initialized().await;
    let assigned = person("usr_reader");
    let absent = person("usr_unassigned_to_document");
    assert_eq!(
        search(&store, &assigned, instant()).await.citations.len(),
        1
    );
    assert!(
        search(&store, &absent, instant())
            .await
            .citations
            .is_empty()
    );
    let mut cross = person("usr_reader");
    cross.role = Role::Reviewer;
    cross.clients = vec!["company-b".into()];
    let error = archive::search(
        &store,
        &cross,
        COMPANY,
        SOURCE,
        archive::SearchQuery {
            query: "rent".into(),
            as_of: day("2024-10-01"),
            embedding: None,
        },
        instant(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.0, StatusCode::FORBIDDEN);
    send(
        &store,
        3,
        json!({"kind":"revoke","document_id":"doc1","expected_version":"v1"}),
    )
    .await;
    assert!(
        search(&store, &assigned, instant())
            .await
            .citations
            .is_empty()
    );
}

#[tokio::test]
async fn archive_hold_prevents_disposition_then_disposition_removes_text() {
    let store = initialized().await;
    send(
        &store,
        3,
        json!({"kind":"set_hold","document_id":"doc1","expected_version":"v1","hold":true}),
    )
    .await;
    let dispose = || json!({"kind":"dispose","document_id":"doc1","expected_version":"v1"});
    assert!(
        archive::apply(&store, COMPANY, SOURCE, event(4, dispose()), instant())
            .await
            .is_err()
    );
    send(&store, 4, doc("v2", "Rent amended")).await;
    assert!(
        store
            .get("accounting:archive:v1:company-a:file-share-2024")
            .await
            .unwrap()
            .unwrap()
            .contains("Rent source amount")
    );
    send(
        &store,
        5,
        json!({"kind":"set_hold","document_id":"doc1","expected_version":"v2","hold":false}),
    )
    .await;
    send(
        &store,
        6,
        json!({"kind":"dispose","document_id":"doc1","expected_version":"v2"}),
    )
    .await;
    let stored = store
        .get("accounting:archive:v1:company-a:file-share-2024")
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.contains("Rent source amount") && !stored.contains("Rent amended"));
    assert!(
        search(&store, &person("usr_reader"), instant())
            .await
            .citations
            .is_empty()
    );
}

#[tokio::test]
async fn authority_requires_independent_approval_and_keeps_effective_periods() {
    let store = Store::memory();
    let proposer = person("usr_writer");
    let reviewer = person("usr_reviewer");
    let hash = authority::propose(
        &store,
        &proposer,
        "CA",
        rule("v1", "2024-01-01", "2024-12-31"),
        instant(),
    )
    .await
    .ok()
    .unwrap();
    assert!(
        authority::approve(
            &store,
            &proposer,
            "CA",
            approval("v1", hash.clone()),
            instant()
        )
        .await
        .is_err()
    );
    let pending = authority::resolve(&store, "CA", day("2024-01-02"), instant())
        .await
        .ok()
        .unwrap();
    assert_eq!(pending["nodes"], json!([]));
    authority::approve(
        &store,
        &reviewer,
        "CA",
        approval("v1", hash.clone()),
        instant(),
    )
    .await
    .ok()
    .unwrap();
    let next = authority::propose(
        &store,
        &proposer,
        "CA",
        rule("v2", "2025-01-01", "2025-12-31"),
        instant(),
    )
    .await
    .ok()
    .unwrap();
    authority::approve(&store, &reviewer, "CA", approval("v2", next), instant())
        .await
        .ok()
        .unwrap();
    let old = authority::resolve(&store, "CA", day("2024-01-02"), instant())
        .await
        .ok()
        .unwrap();
    assert_eq!(old["nodes"][0]["node_hash"], hash);
    assert_eq!(old["nodes"][0]["edge"]["source_sha256"], "e".repeat(64));
    let unknown = authority::resolve(
        &store,
        "CA",
        day("2024-01-02"),
        instant() - Duration::seconds(1),
    )
    .await
    .ok()
    .unwrap();
    assert_eq!(unknown["nodes"], json!([]));
}

#[tokio::test]
async fn release_blocks_missing_acceptance_case_and_rejects_self_review() {
    let store = Store::memory();
    let proposer = person("usr_writer");
    let reviewer = person("usr_reviewer");
    let hash = release::propose(&store, &proposer, "pilot", release_input(false), instant())
        .await
        .ok()
        .unwrap();
    let review = |hash: &str| release::ReviewInput {
        manifest_sha256: hash.into(),
    };
    assert_eq!(
        release::review(&store, &proposer, "pilot", review(&hash), instant())
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        release::review(&store, &reviewer, "pilot", review(&hash), instant())
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::CONFLICT
    );
    let state = release::status(&store, "pilot", instant())
        .await
        .ok()
        .unwrap();
    assert_eq!(state["reported_cases_complete"], false);
}

#[tokio::test]
async fn release_demands_exact_version_and_remains_blocked_even_with_21_reported_cases() {
    let store = Store::memory();
    let proposer = person("usr_writer");
    let reviewer = person("usr_reviewer");
    let hash = release::propose(&store, &proposer, "pilot", release_input(true), instant())
        .await
        .ok()
        .unwrap();
    let review = |hash: &str| release::ReviewInput {
        manifest_sha256: hash.into(),
    };
    assert!(
        release::review(&store, &reviewer, "pilot", review("wrong"), instant())
            .await
            .is_err()
    );
    release::review(&store, &reviewer, "pilot", review(&hash), instant())
        .await
        .ok()
        .unwrap();
    let state = release::status(&store, "pilot", instant())
        .await
        .ok()
        .unwrap();
    assert_eq!(state["reported_cases_complete"], true);
    assert_eq!(state["state"], "reviewed_pending_production_controls");
    assert_eq!(state["live_write_authorized"], false);
    let expired = release::status(&store, "pilot", instant() + Duration::days(2))
        .await
        .ok()
        .unwrap();
    assert_eq!(expired["state"], "expired");
    release::suspend(&store, &reviewer, "pilot")
        .await
        .ok()
        .unwrap();
    let suspended = release::status(&store, "pilot", instant())
        .await
        .ok()
        .unwrap();
    assert_eq!(suspended["state"], "suspended");
}

#[test]
fn production_case_inventory_covers_all_seven_gates() {
    let expected: BTreeSet<_> = (1..=7)
        .flat_map(|i| ["A", "B", "C"].map(move |suffix| format!("LW-{i:02}{suffix}")))
        .collect();
    let registered: BTreeSet<_> = RELEASE_GATES
        .iter()
        .flat_map(|(_, _, ids)| ids.iter().map(|id| id.to_string()))
        .collect();
    assert_eq!(expected, registered);
}

#[tokio::test]
async fn sandbox_lw_04a_manual_and_automated_claim_are_exclusive() {
    use super::accounting_dispatch::{Command, Operation, Path, apply, operation_id};
    let store = Store::memory();
    let first = person("usr_first");
    let second = person("usr_second");
    let id = operation_id(COMPANY, "close").ok().unwrap();
    let hash = "a".repeat(64);
    let operation = Operation {
        profile: "pilot".into(),
        company: COMPANY.into(),
        business_key: "close".into(),
        payload_sha256: hash.clone(),
        manifest_sha256: "b".repeat(64),
        correction_of: None,
    };
    apply(
        &store,
        &first,
        COMPANY,
        &id,
        Command::Prepare { operation },
        instant(),
    )
    .await
    .unwrap_or_else(|_| panic!("prepare"));
    let (manual, automated) = tokio::join!(
        apply(
            &store,
            &first,
            COMPANY,
            &id,
            Command::Claim {
                path: Path::Manual,
                expected_payload_sha256: hash.clone()
            },
            instant()
        ),
        apply(
            &store,
            &second,
            COMPANY,
            &id,
            Command::Claim {
                path: Path::Automated,
                expected_payload_sha256: hash.clone()
            },
            instant()
        )
    );
    assert_ne!(manual.is_ok(), automated.is_ok());
}

#[tokio::test]
async fn sandbox_lw_04b_and_04c_uncertainty_requires_reconciliation_and_correction_links_original()
{
    use super::accounting_dispatch::{Command, Operation, Path, apply, operation_id};
    let store = Store::memory();
    let first = person("usr_first");
    let second = person("usr_second");
    let id = operation_id(COMPANY, "close").ok().unwrap();
    let correction = operation_id(COMPANY, "correction").ok().unwrap();
    let hash = "a".repeat(64);
    let operation = |business_key: &str, correction_of| Operation {
        profile: "pilot".into(),
        company: COMPANY.into(),
        business_key: business_key.into(),
        payload_sha256: hash.clone(),
        manifest_sha256: "b".repeat(64),
        correction_of,
    };
    assert!(
        apply(
            &store,
            &first,
            COMPANY,
            &id,
            Command::Prepare {
                operation: operation("close", None)
            },
            instant()
        )
        .await
        .is_ok()
    );
    let claim = || Command::Claim {
        path: Path::Manual,
        expected_payload_sha256: hash.clone(),
    };
    assert!(
        apply(&store, &first, COMPANY, &id, claim(), instant())
            .await
            .is_ok()
    );
    let start = || Command::Start {
        claim_revision: 2,
        expected_payload_sha256: hash.clone(),
    };
    assert!(
        apply(&store, &second, COMPANY, &id, start(), instant())
            .await
            .is_err()
    );
    assert!(
        apply(&store, &first, COMPANY, &id, start(), instant())
            .await
            .is_ok()
    );
    assert!(
        apply(
            &store,
            &first,
            COMPANY,
            &id,
            Command::MarkUnknown {
                attempt_revision: 3
            },
            instant()
        )
        .await
        .is_ok()
    );
    assert!(
        apply(&store, &second, COMPANY, &id, claim(), instant())
            .await
            .is_err()
    );
    let prepare_correction = || Command::Prepare {
        operation: operation("correction", Some(id.clone())),
    };
    assert!(
        apply(
            &store,
            &first,
            COMPANY,
            &correction,
            prepare_correction(),
            instant()
        )
        .await
        .is_err()
    );
    let reconcile = || Command::Reconcile {
        attempt_revision: 3,
        receipt_sha256: "c".repeat(64),
        confirmed: true,
    };
    assert!(
        apply(&store, &first, COMPANY, &id, reconcile(), instant())
            .await
            .is_err()
    );
    assert!(
        apply(&store, &second, COMPANY, &id, reconcile(), instant())
            .await
            .is_ok()
    );
    let record = apply(
        &store,
        &first,
        COMPANY,
        &correction,
        prepare_correction(),
        instant(),
    )
    .await
    .ok()
    .unwrap();
    let json = serde_json::to_value(record).unwrap();
    assert_eq!(json["operation"]["correction_of"], id);
    assert_eq!(json["live_write_authorized"], false);
}

#[tokio::test]
async fn evidence_graph_tracks_new_authority_and_archive_revocation_without_exposing_other_owners()
{
    use super::accounting_graph as graph;
    let store = initialized().await;
    let actor = person("usr_reader");
    let request = || graph::Request {
        role: "Valuation".into(),
        jurisdiction: "CA".into(),
        sources: vec![SOURCE.into(), "old-archive".into()],
        query: "rent".into(),
        effective_on: day("2024-11-18"),
        embedding: None,
    };
    let packet = graph::prepare(&store, &actor, COMPANY, request(), instant())
        .await
        .ok()
        .unwrap();
    assert_eq!(packet["archive"]["complete_for_requested_sources"], false);
    let id = packet["packet_id"].as_str().unwrap();
    let status = graph::status(&store, &actor, COMPANY, id, instant(), |_, _| true)
        .await
        .ok()
        .unwrap();
    assert_eq!(status["state"], "incomplete");
    assert!(
        graph::status(
            &store,
            &person("usr_other"),
            COMPANY,
            id,
            instant(),
            |_, _| true
        )
        .await
        .is_err()
    );
    let hash = authority::propose(
        &store,
        &actor,
        "CA",
        rule("v1", "2024-01-01", "2024-12-31"),
        instant(),
    )
    .await
    .ok()
    .unwrap();
    authority::approve(
        &store,
        &person("usr_reviewer"),
        "CA",
        approval("v1", hash),
        instant(),
    )
    .await
    .ok()
    .unwrap();
    assert_eq!(
        graph::status(&store, &actor, COMPANY, id, instant(), |_, _| true)
            .await
            .ok()
            .unwrap()["state"],
        "stale"
    );
    let newer = graph::prepare(&store, &actor, COMPANY, request(), instant())
        .await
        .ok()
        .unwrap();
    let newer_id = newer["packet_id"].as_str().unwrap();
    send(
        &store,
        3,
        json!({"kind":"revoke","document_id":"doc1","expected_version":"v1"}),
    )
    .await;
    assert_eq!(
        graph::status(&store, &actor, COMPANY, newer_id, instant(), |_, _| true)
            .await
            .ok()
            .unwrap()["state"],
        "stale"
    );
    assert_eq!(
        graph::prepare(&store, &actor, COMPANY, request(), instant())
            .await
            .ok()
            .unwrap()["archive"]["sources"][SOURCE]["citations"],
        json!([])
    );
}
