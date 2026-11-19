use super::{
    accounting_acceptance as fixture, accounting_archive as archive,
    accounting_dispatch as dispatch,
};
use crate::store::Store;
use serde_json::json;

#[tokio::test]
async fn hybrid_finds_semantic_only_match_and_hides_revoked_document() {
    let store = Store::memory();
    let actor = fixture::person("usr_reader");
    let now = fixture::instant();
    let mut change = fixture::doc("v1", "Rent payable");
    change["document"]["chunks"][0]["embedding"] = json!({"model":"test-v1","values":[20,10]});
    fixture::send(&store, 1, change).await;
    fixture::send(
        &store,
        2,
        json!({"kind":"permissions_observed","observed_at":now}),
    )
    .await;
    let query = || {
        serde_json::from_value(json!({
            "query":"lease", "as_of":"2024-10-01",
            "embedding":{"model":"test-v1","values":[20,10]}
        }))
        .unwrap()
    };
    let report = archive::search(
        &store,
        &actor,
        fixture::COMPANY,
        fixture::SOURCE,
        query(),
        now,
    )
    .await
    .ok()
    .unwrap();
    let serialized = serde_json::to_value(report).unwrap();
    assert_eq!(serialized["citations"][0]["lexical_hits"], 0);
    assert!(
        serialized["citations"][0]["semantic_similarity"]
            .as_f64()
            .unwrap()
            > 0.99
    );
    fixture::send(
        &store,
        3,
        json!({"kind":"revoke","document_id":"doc1","expected_version":"v1"}),
    )
    .await;
    assert!(
        archive::search(
            &store,
            &actor,
            fixture::COMPANY,
            fixture::SOURCE,
            query(),
            now
        )
        .await
        .ok()
        .unwrap()
        .citations
        .is_empty()
    );
}

#[tokio::test]
async fn sandbox_journal_posts_once_and_reads_back_after_unknown_outcome() {
    sandbox_journal_posts_once_and_reads_back_after_unknown_outcome_case(Store::memory()).await;
}

pub(super) async fn sandbox_journal_posts_once_and_reads_back_after_unknown_outcome_case(store: Store) {
    use dispatch::{Command, Operation, Path};
    let owner = fixture::person("usr_owner");
    let reviewer = fixture::person("usr_reviewer");
    let company = fixture::COMPANY;
    let now = fixture::instant();
    let id = dispatch::operation_id(company, "sandbox-case")
        .ok()
        .unwrap();
    let journal = || {
        serde_json::from_value::<dispatch::SandboxJournal>(json!({
        "currency":"CAD","lines":[{"account":"cash","cents":125},{"account":"equity","cents":-125}]
    })).unwrap()
    };
    let hash = archive::sha256(&serde_json::to_vec(&journal()).unwrap());
    let operation = Operation {
        profile: "sandbox-journal".into(),
        company: company.into(),
        business_key: "sandbox-case".into(),
        payload_sha256: hash.clone(),
        manifest_sha256: "a".repeat(64),
        correction_of: None,
    };
    assert!(
        dispatch::apply(
            &store,
            &owner,
            company,
            &id,
            Command::Prepare { operation },
            now
        )
        .await
        .is_ok()
    );
    assert!(
        dispatch::apply(
            &store,
            &owner,
            company,
            &id,
            Command::Claim {
                path: Path::Automated,
                expected_payload_sha256: hash.clone()
            },
            now
        )
        .await
        .is_ok()
    );
    assert!(
        dispatch::apply(
            &store,
            &owner,
            company,
            &id,
            Command::Start {
                claim_revision: 2,
                expected_payload_sha256: hash
            },
            now
        )
        .await
        .is_ok()
    );
    assert!(
        dispatch::sandbox_post(&store, &reviewer, company, &id, journal())
            .await
            .is_err()
    );
    let wrong = serde_json::from_value(json!({"currency":"CAD","lines":[{"account":"cash","cents":126},{"account":"equity","cents":-126}]})).unwrap();
    assert!(
        dispatch::sandbox_post(&store, &owner, company, &id, wrong)
            .await
            .is_err()
    );
    let first = dispatch::sandbox_post(&store, &owner, company, &id, journal())
        .await
        .ok()
        .unwrap();
    let second = dispatch::sandbox_post(&store, &owner, company, &id, journal())
        .await
        .ok()
        .unwrap();
    assert!(first == second);
    assert!(
        dispatch::apply(
            &store,
            &owner,
            company,
            &id,
            Command::MarkUnknown {
                attempt_revision: 3
            },
            now
        )
        .await
        .is_ok()
    );
    assert!(
        dispatch::sandbox_reconcile(&store, &owner, company, &id, now)
            .await
            .is_err()
    );
    let result = dispatch::sandbox_reconcile(&store, &reviewer, company, &id, now)
        .await
        .ok()
        .unwrap();
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["state"], "reconciled");
    assert_eq!(result["live_write_authorized"], false);
}
