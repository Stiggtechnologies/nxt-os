use super::{
    accounting_acceptance as fixture, accounting_archive as archive,
    accounting_workpaper_review as workflow,
};
#[cfg(test)]
use super::accounting_specialist as specialist;
use crate::store::Store;
use serde_json::{Value, json};

fn control(kind: &str) -> Value {
    match kind {
        "unique_candidate" => json!({"kind":kind,"candidates":1}),
        "no_unresolved_exceptions" => json!({"kind":kind,"unresolved":0}),
        "population_tie" => json!({"kind":kind,"expected":10,"accounted_for":10}),
        "amount_tie" => {
            json!({"kind":kind,"source_cents":100,"adjusted_cents":20,"target_cents":120})
        }
        _ => json!({"kind":"review_only"}),
    }
}

#[cfg(test)]
async fn setup(role: &str, keys: &str, control: Value) -> (Store, Value) {
    setup_with_procedure(role, keys, control, None).await
}

#[cfg(test)]
async fn setup_with_procedure(
    role: &str,
    keys: &str,
    control: Value,
    procedure: Option<Value>,
) -> (Store, Value) {
    setup_in(Store::memory(), role, keys, control, procedure).await
}

async fn setup_in(store: Store, role: &str, keys: &str, control: Value, procedure: Option<Value>) -> (Store, Value) {
    let mut evidence = serde_json::Map::new();
    let mut facts = control.as_object().unwrap().clone();
    facts.remove("kind");
    let facts: Vec<_> = facts.into_iter().collect();
    for (index, field) in keys.split(',').enumerate() {
        let id = format!("doc{index}");
        let source_facts: serde_json::Map<_, _> =
            facts.iter().skip(index).step_by(3).cloned().collect();
        let mut source = json!({"fixture":field,"workpaper_facts":source_facts});
        if let Some(inputs) = procedure
            .as_ref()
            .and_then(|procedure| procedure.get(field))
        {
            source["procedure_inputs"] = inputs.clone();
        }
        let text = source.to_string();
        let hash = archive::sha256(text.as_bytes());
        fixture::send(
            &store,
            (index + 1) as u64,
            json!({
                "kind":"upsert","document":{
                    "id":id,"version":"v1","content_sha256":hash,
                    "from":"2024-01-01","through":"2024-12-31",
                    "user_ids":["usr_preparer","usr_reviewer"],
                    "chunks":[{"locator":"line 1","text":text}]
                }
            }),
        )
        .await;
        evidence.insert(
            field.into(),
            json!({"source":fixture::SOURCE,"document_id":id,"version":"v1","sha256":hash}),
        );
    }
    fixture::send(
        &store,
        4,
        json!({"kind":"permissions_observed","observed_at":fixture::instant()}),
    )
    .await;
    (
        store,
        json!({"role":role,"evidence":evidence,"effective_on":"2024-10-01","control":control,
               "require_service_procedure":procedure.is_some()}),
    )
}

#[tokio::test]
async fn service_procedures_are_frozen_replayed_and_gate_review() {
    let preparer = fixture::person("usr_preparer");
    let reviewer = fixture::person("usr_reviewer");
    let now = fixture::instant();
    for (role, inputs) in super::accounting_service_procedure_tests::fixtures() {
        let (_, keys, kind) = specialist::contracts()
            .find(|(name, _, _)| *name == role)
            .unwrap();
        let (store, request) =
            setup_with_procedure(role, keys, control(kind), Some(inputs.clone())).await;
        let draft = workflow::prepare(
            &store,
            &preparer,
            fixture::COMPANY,
            "wp",
            serde_json::from_value(request).unwrap(),
            now,
            |_| true,
        )
        .await
        .unwrap_or_else(|error| panic!("{role}: {}", error.1));
        assert_eq!(draft["result"]["service_procedure"]["passes"], true);
        let decision = workflow::review(
            &store,
            &reviewer,
            fixture::COMPANY,
            "wp",
            approval(draft["digest"].as_str().unwrap()),
            now,
            |_| true,
        )
        .await
        .unwrap_or_else(|error| panic!("{role}: {}", error.1));
        assert_eq!(decision["live_write"], false);
        if role == "Accounts payable and expenses" {
            let mut wrong = inputs;
            wrong["invoice_register"]["invoices"][0]["supplier"] = json!("unknown");
            let (store, request) =
                setup_with_procedure(role, keys, control(kind), Some(wrong)).await;
            let draft = workflow::prepare(
                &store,
                &preparer,
                fixture::COMPANY,
                "wp",
                serde_json::from_value(request).unwrap(),
                now,
                |_| true,
            )
            .await
            .ok()
            .unwrap();
            assert_eq!(draft["result"]["state"], "control_exception");
            assert!(
                workflow::review(
                    &store,
                    &reviewer,
                    fixture::COMPANY,
                    "wp",
                    approval(draft["digest"].as_str().unwrap()),
                    now,
                    |_| true
                )
                .await
                .is_err()
            );
        }
    }
}

fn approval(digest: &str) -> workflow::Review {
    serde_json::from_value(
        json!({"expected_digest":digest,"decision":"approve","inspected_sources":true}),
    )
    .unwrap()
}

#[cfg(test)]
fn supplemental_fixture(spec: &str, allowed: &str) -> (Value, Value) {
    let mut documents = json!({});
    let mut first_failure = None;
    for check in spec.split(';') {
        let parts: Vec<_> = check.split(':').collect();
        let values = match parts[0] {
            "cover" | "same" => vec![json!(["item1"]), json!(["item1"])],
            "disjoint" => vec![json!([]), json!(["item1"])],
            "equal" => vec![json!("entity1"), json!("entity1")],
            "hash" => vec![json!("a".repeat(64)), json!("a".repeat(64))],
            "zero" => vec![json!(0)],
            "total" => vec![json!([40, 60]), json!(100)],
            "bridge" => vec![json!(80), json!([20]), json!(100)],
            "between" => vec![json!(100), json!(0), json!(200)],
            "at_most" => vec![json!(100), json!(200)],
            "active" => vec![json!("2024-01-01"), json!("2024-12-31")],
            "at_least_date" => vec![json!("2024-12-31")],
            invalid => panic!("unknown check {invalid}"),
        };
        assert_eq!(values.len(), parts.len() - 1);
        for (path, value) in parts[1..].iter().zip(values) {
            let (source, field) = path.split_once('.').unwrap();
            assert!(
                allowed.split(',').any(|name| name == source),
                "{path} is outside {allowed}"
            );
            if !documents[source][field].is_null() {
                assert_eq!(
                    documents[source][field], value,
                    "incompatible fixture for {path}"
                );
            }
            documents[source][field] = value;
        }
        first_failure.get_or_insert_with(|| {
            let value = match parts[0] {
                "cover" | "same" => json!(["unaccounted"]),
                "disjoint" => json!(["item1"]),
                "equal" => json!("entity2"),
                "hash" => json!("b".repeat(64)),
                "zero" => json!(1),
                "total" => json!([99]),
                "bridge" | "between" | "at_most" => json!(999),
                "active" => json!("2024-11-01"),
                "at_least_date" => json!("2024-01-01"),
                _ => unreachable!(),
            };
            (parts[1].split_once('.').unwrap(), value)
        });
    }
    assert_eq!(documents.as_object().unwrap().len(), 3);
    let ((source, field), failed) = first_failure.unwrap();
    let mut failing = documents.clone();
    failing[source][field] = failed;
    (documents, failing)
}

#[tokio::test]
async fn remaining_57_service_procedures_verify_sources_and_gate_independent_review() {
    let preparer = fixture::person("usr_preparer");
    let reviewer = fixture::person("usr_reviewer");
    let now = fixture::instant();
    let previous: std::collections::BTreeSet<_> =
        super::accounting_service_procedure_tests::fixtures()
            .into_iter()
            .map(|(role, _)| role)
            .collect();
    let mut roles = previous.clone();
    for (role, spec) in super::accounting_additional_procedures::specs() {
        assert!(roles.insert(role), "duplicate procedure for {role}");
        let (_, keys, kind) = specialist::contracts()
            .find(|(name, _, _)| *name == role)
            .unwrap();
        let (passing, failing) = supplemental_fixture(spec, keys);
        for (inputs, expected) in [(passing, true), (failing, false)] {
            let (store, request) =
                setup_with_procedure(role, keys, control(kind), Some(inputs)).await;
            let draft = workflow::prepare(
                &store,
                &preparer,
                fixture::COMPANY,
                "wp",
                serde_json::from_value(request).unwrap(),
                now,
                |_| true,
            )
            .await
            .unwrap_or_else(|error| panic!("{role}: {}", error.1));
            assert_eq!(draft["result"]["evidence_content_verified"], true);
            assert_eq!(
                draft["result"]["service_procedure"]["passes"], expected,
                "{role}: {draft}"
            );
            let inspected =
                workflow::inspect(&store, &reviewer, fixture::COMPANY, "wp", now, |_| true)
                    .await
                    .ok()
                    .unwrap();
            assert_eq!(inspected["sources"].as_object().unwrap().len(), 3);
            let digest = draft["digest"].as_str().unwrap();
            let decision = workflow::review(
                &store,
                &reviewer,
                fixture::COMPANY,
                "wp",
                approval(digest),
                now,
                |_| true,
            )
            .await;
            if expected {
                let decision = decision.unwrap_or_else(|error| panic!("{role}: {}", error.1));
                assert_eq!(decision["review"]["review"]["decision"], "approve");
                assert_eq!(decision["live_write"], false);
            } else {
                assert!(decision.is_err(), "an exception was approved for {role}");
            }
        }
    }
    assert_eq!(roles.len() - previous.len(), 57);
    let expected: std::collections::BTreeSet<_> =
        specialist::contracts().map(|(role, _, _)| role).collect();
    assert_eq!(roles, expected);
}

#[tokio::test]
async fn all_77_workpapers_verify_archived_sources_and_record_independent_review() {
    let preparer = fixture::person("usr_preparer");
    let reviewer = fixture::person("usr_reviewer");
    let mut count = 0;
    for (role, keys, kind) in specialist::contracts() {
        let (store, input) = setup(role, keys, control(kind)).await;
        let now = fixture::instant();
        let draft = workflow::prepare(
            &store,
            &preparer,
            fixture::COMPANY,
            "wp",
            serde_json::from_value(input).unwrap(),
            now,
            |_| true,
        )
        .await
        .unwrap_or_else(|error| panic!("{role}: {}", error.1));
        assert_eq!(draft["result"]["evidence_content_verified"], true);
        let digest = draft["digest"].as_str().unwrap();
        let sources = workflow::inspect(&store, &reviewer, fixture::COMPANY, "wp", now, |_| true)
            .await
            .ok()
            .unwrap();
        assert_eq!(sources["sources"].as_object().unwrap().len(), 3);
        let approved = workflow::review(
            &store,
            &reviewer,
            fixture::COMPANY,
            "wp",
            approval(digest),
            now,
            |_| true,
        )
        .await
        .unwrap_or_else(|error| panic!("{role}: {}", error.1));
        assert_eq!(approved["review"]["review"]["decision"], "approve");
        assert_eq!(approved["live_write"], false);
        count += 1;
    }
    assert_eq!(count, 77);
}

#[tokio::test]
async fn workpaper_rejects_unbound_inputs_self_review_digest_changes_and_revoked_sources() {
    workpaper_rejects_unbound_inputs_self_review_digest_changes_and_revoked_sources_case(Store::memory()).await;
}

pub(super) async fn workpaper_rejects_unbound_inputs_self_review_digest_changes_and_revoked_sources_case(store: Store) {
    let preparer = fixture::person("usr_preparer");
    let reviewer = fixture::person("usr_reviewer");
    let outsider = fixture::person("usr_outsider");
    let now = fixture::instant();
    let (store, input) = setup_in(
        store,
        "Accounts payable and expenses",
        "invoice_register,duplicate_report,payee_verification",
        control("amount_tie"),
        None,
    )
    .await;
    let prepare = |input| serde_json::from_value(input).unwrap();
    let mut altered = input.clone();
    altered["control"]["target_cents"] = json!(119);
    assert!(
        workflow::prepare(
            &store,
            &preparer,
            fixture::COMPANY,
            "wp",
            prepare(altered),
            now,
            |_| true
        )
        .await
        .is_err()
    );
    let mut forged = input.clone();
    forged["evidence"]["invoice_register"]["sha256"] = json!("b".repeat(64));
    assert!(
        workflow::prepare(
            &store,
            &preparer,
            fixture::COMPANY,
            "wp",
            prepare(forged),
            now,
            |_| true
        )
        .await
        .is_err()
    );
    let draft = workflow::prepare(
        &store,
        &preparer,
        fixture::COMPANY,
        "wp",
        prepare(input),
        now,
        |_| true,
    )
    .await
    .ok()
    .unwrap();
    let digest = draft["digest"].as_str().unwrap();
    assert!(
        workflow::inspect(&store, &outsider, fixture::COMPANY, "wp", now, |_| true)
            .await
            .is_err()
    );
    assert!(
        workflow::review(
            &store,
            &preparer,
            fixture::COMPANY,
            "wp",
            approval(digest),
            now,
            |_| true
        )
        .await
        .is_err()
    );
    assert!(
        workflow::review(
            &store,
            &reviewer,
            fixture::COMPANY,
            "wp",
            approval("stale"),
            now,
            |_| true
        )
        .await
        .is_err()
    );
    fixture::send(
        &store,
        5,
        json!({"kind":"revoke","document_id":"doc0","expected_version":"v1"}),
    )
    .await;
    assert!(
        workflow::review(
            &store,
            &reviewer,
            fixture::COMPANY,
            "wp",
            approval(digest),
            now,
            |_| true
        )
        .await
        .is_err()
    );
    assert!(
        workflow::inspect(&store, &reviewer, fixture::COMPANY, "wp", now, |_| true)
            .await
            .is_err()
    );
}
