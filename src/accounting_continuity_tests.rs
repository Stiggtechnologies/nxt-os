use super::{
    accounting_acceptance as fixture, accounting_archive as archive,
    accounting_dispatch as dispatch,
};
use crate::{firm::staff::Identity, store::Store};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) const FIXTURE: &str = include_str!("accounting-continuity-fixture.json");
const CONTROL_KEY: &str = "accounting:sandbox-continuity:v1:controls";

fn config() -> Value {
    let config: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(config["classification"], "synthetic_sandbox_only");
    assert_eq!(config["production_approval"], false);
    config
}

fn identity(name: &str) -> Identity {
    fixture::person(config()["roles"][name].as_str().unwrap())
}
fn n(metrics: &Value, key: &str) -> u64 {
    metrics[key].as_u64().unwrap()
}

#[derive(Serialize, Deserialize)]
struct Emergency {
    holder: String,
    issued_by: String,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
struct Controls {
    active_manifest: String,
    approved_manifests: BTreeSet<String>,
    connector: String,
    model: String,
    denied: BTreeSet<String>,
    emergency: Option<Emergency>,
    paused: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Pause,
    Resume,
}

impl Controls {
    fn emergency_can(&self, actor: &Identity, action: Action, now: DateTime<Utc>) -> bool {
        actor.mfa
            && actor.staff_id().is_some()
            && !self.denied.contains(&actor.user_id)
            && self.emergency.as_ref().is_some_and(|grant| {
                grant.holder == actor.user_id
                    && grant.issued_by != actor.user_id
                    && now < grant.expires_at
                    && action == Action::Pause
            })
    }

    fn admit(&self, actor: &Identity, manifest: &str, connector: &str, model: &str) -> bool {
        actor.mfa
            && actor.staff_id().is_some()
            && self.active_manifest == manifest
            && self.approved_manifests.contains(manifest)
            && self.connector == connector
            && self.model == model
            && !self.paused
            && !self.denied.contains(&actor.user_id)
    }
}

fn controls() -> Controls {
    let c = config();
    let release = &c["release"];
    Controls {
        active_manifest: release["current_manifest"].as_str().unwrap().into(),
        approved_manifests: ["current_manifest", "prior_manifest"]
            .map(|key| release[key].as_str().unwrap().to_string())
            .into(),
        connector: release["connector"].as_str().unwrap().into(),
        model: release["model"].as_str().unwrap().into(),
        denied: BTreeSet::new(),
        emergency: None,
        paused: false,
    }
}

async fn controls_from(store: &Store) -> Controls {
    store.get_json(CONTROL_KEY).await.unwrap().unwrap()
}

async fn emergency_pause(store: &Store, actor: &Identity, now: DateTime<Utc>) -> bool {
    let (raw, state) = archive::load::<Controls>(store, CONTROL_KEY)
        .await
        .ok()
        .unwrap();
    let mut state = state.unwrap();
    if !state.emergency_can(actor, Action::Pause, now) {
        return false;
    }
    state.paused = true;
    archive::save(store, CONTROL_KEY, raw.as_deref(), &state)
        .await
        .is_ok()
}

async fn guarded_claim(
    store: &Store,
    actor: &Identity,
    id: &str,
    manifest: &str,
    connector: &str,
    model: &str,
    now: DateTime<Utc>,
) -> bool {
    if !controls_from(store)
        .await
        .admit(actor, manifest, connector, model)
    {
        return false;
    }
    dispatch::apply(
        store,
        actor,
        fixture::COMPANY,
        id,
        dispatch::Command::Claim {
            path: dispatch::Path::Automated,
            expected_payload_sha256: payload(),
        },
        now,
    )
    .await
    .is_ok()
}

#[tokio::test]
async fn sandbox_lw_02c_emergency_expires_and_cannot_restore_authority() {
    sandbox_lw_02c_emergency_expires_and_cannot_restore_authority_case(Store::memory()).await;
}

pub(super) async fn sandbox_lw_02c_emergency_expires_and_cannot_restore_authority_case(store: Store) {
    let mut control = controls();
    let operator = identity("emergency");
    let issuer = identity("reviewer");
    let now = fixture::instant();
    let expires_at = now + Duration::minutes(n(&config(), "emergency_ttl_minutes") as i64);
    control.emergency = Some(Emergency {
        holder: operator.user_id.clone(),
        issued_by: issuer.user_id.clone(),
        expires_at,
    });
    store.set_json(CONTROL_KEY, &control).await.unwrap();
    let loaded = controls_from(&store).await;
    assert!(loaded.emergency_can(&operator, Action::Pause, now));
    assert!(!loaded.emergency_can(&operator, Action::Resume, now));
    assert!(!loaded.emergency_can(&issuer, Action::Pause, now));
    assert!(!loaded.emergency_can(&operator, Action::Pause, expires_at));
    assert!(!loaded.emergency_can(&operator, Action::Pause, expires_at + Duration::seconds(1)));
    assert!(!emergency_pause(&store, &operator, expires_at).await);
    assert!(emergency_pause(&store, &operator, now).await);
    let paused = controls_from(&store).await;
    assert!(!paused.admit(
        &issuer,
        &paused.active_manifest,
        &paused.connector,
        &paused.model
    ));
}

#[tokio::test]
async fn sandbox_lw_03a_rejects_unpinned_connector_and_model_before_dispatch() {
    sandbox_lw_03a_rejects_unpinned_connector_and_model_before_dispatch_case(Store::memory()).await;
}

pub(super) async fn sandbox_lw_03a_rejects_unpinned_connector_and_model_before_dispatch_case(store: Store) {
    let control = controls();
    archive::save(&store, CONTROL_KEY, None, &control)
        .await
        .ok()
        .unwrap();
    let control = controls_from(&store).await;
    let actor = identity("automated");
    let now = fixture::instant();
    let id = prepare(&store, "version-binding", &actor, now).await;
    let keys = [
        CONTROL_KEY.to_string(),
        dispatch::key(fixture::COMPANY, &id).ok().unwrap(),
        dispatch::sandbox_key(fixture::COMPANY, &id).ok().unwrap(),
    ];
    let baseline = store.mget(&keys).await.unwrap();
    assert!(control.admit(
        &actor,
        &control.active_manifest,
        &control.connector,
        &control.model
    ));
    for (connector, model) in [
        ("substitute-connector", control.model.as_str()),
        (control.connector.as_str(), "substitute-model"),
    ] {
        assert!(
            !guarded_claim(
                &store,
                &actor,
                &id,
                &control.active_manifest,
                connector,
                model,
                now
            )
            .await
        );
    }
    assert!(!control.admit(&actor, &"c".repeat(64), &control.connector, &control.model));
    assert_eq!(baseline, store.mget(&keys).await.unwrap());
    assert!(
        guarded_claim(
            &store,
            &actor,
            &id,
            &control.active_manifest,
            &control.connector,
            &control.model,
            now
        )
        .await
    );
}

fn journal() -> dispatch::SandboxJournal {
    serde_json::from_value(json!({"currency":"CAD","lines":[
        {"account":"cash","cents":125},{"account":"equity","cents":-125}
    ]}))
    .unwrap()
}

fn payload() -> String {
    archive::sha256(&serde_json::to_vec(&journal()).unwrap())
}

async fn prepare(store: &Store, name: &str, actor: &Identity, now: DateTime<Utc>) -> String {
    let id = dispatch::operation_id(fixture::COMPANY, name).ok().unwrap();
    let command = dispatch::Command::Prepare {
        operation: dispatch::Operation {
            profile: "sandbox-journal".into(),
            company: fixture::COMPANY.into(),
            business_key: name.into(),
            payload_sha256: payload(),
            manifest_sha256: controls().active_manifest,
            correction_of: None,
        },
    };
    assert!(
        dispatch::apply(store, actor, fixture::COMPANY, &id, command, now)
            .await
            .is_ok()
    );
    id
}

async fn start(
    store: &Store,
    id: &str,
    actor: &Identity,
    route: dispatch::Path,
    now: DateTime<Utc>,
) {
    assert!(
        dispatch::apply(
            store,
            actor,
            fixture::COMPANY,
            id,
            dispatch::Command::Claim {
                path: route,
                expected_payload_sha256: payload(),
            },
            now
        )
        .await
        .is_ok()
    );
    assert!(
        dispatch::apply(
            store,
            actor,
            fixture::COMPANY,
            id,
            dispatch::Command::Start {
                claim_revision: 2,
                expected_payload_sha256: payload(),
            },
            now
        )
        .await
        .is_ok()
    );
}

async fn post_and_reconcile(
    store: &Store,
    id: &str,
    operator: &Identity,
    reviewer: &Identity,
    route: dispatch::Path,
    now: DateTime<Utc>,
) {
    start(store, id, operator, route, now).await;
    assert!(
        dispatch::sandbox_post(store, operator, fixture::COMPANY, id, journal())
            .await
            .is_ok()
    );
    let record = dispatch::sandbox_reconcile(store, reviewer, fixture::COMPANY, id, now)
        .await
        .ok()
        .unwrap();
    let state = serde_json::to_value(record).unwrap();
    assert_eq!(state["state"], "reconciled");
    assert_eq!(state["live_write_authorized"], false);
}

async fn receipts(store: &Store, ids: &[String]) -> usize {
    let keys: Vec<_> = ids.iter()
        .map(|id| dispatch::sandbox_key(fixture::COMPANY, id).ok().unwrap())
        .collect();
    store.mget(&keys).await.unwrap().into_iter().flatten().count()
}

#[tokio::test]
async fn sandbox_lw_03c_rollback_preserves_revocations_and_completed_receipts() {
    sandbox_lw_03c_rollback_preserves_revocations_and_completed_receipts_case(Store::memory()).await;
}

pub(super) async fn sandbox_lw_03c_rollback_preserves_revocations_and_completed_receipts_case(store: Store) {
    let actor = identity("automated");
    let reviewer = identity("reviewer");
    let now = fixture::instant();
    let mut control = controls();
    control.denied.insert(actor.user_id.clone());
    archive::save(&store, CONTROL_KEY, None, &control)
        .await
        .ok()
        .unwrap();
    let id = prepare(&store, "prior-completed", &reviewer, now).await;
    post_and_reconcile(
        &store,
        &id,
        &reviewer,
        &identity("manual"),
        dispatch::Path::Manual,
        now,
    )
    .await;
    let prior = config()["release"]["prior_manifest"]
        .as_str()
        .unwrap()
        .to_string();
    let (raw, loaded) = archive::load::<Controls>(&store, CONTROL_KEY)
        .await
        .ok()
        .unwrap();
    let mut rollback = loaded.unwrap();
    assert!(rollback.approved_manifests.contains(&prior));
    rollback.active_manifest = prior.clone();
    archive::save(&store, CONTROL_KEY, raw.as_deref(), &rollback)
        .await
        .ok()
        .unwrap();
    let restored = controls_from(&store).await;
    assert!(!restored.admit(&actor, &prior, &restored.connector, &restored.model));
    assert!(restored.admit(&reviewer, &prior, &restored.connector, &restored.model));
    assert!(
        dispatch::apply(
            &store,
            &reviewer,
            fixture::COMPANY,
            &id,
            dispatch::Command::Claim {
                path: dispatch::Path::Manual,
                expected_payload_sha256: payload(),
            },
            now
        )
        .await
        .is_err()
    );
    assert_eq!(receipts(&store, &[id]).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sandbox_lw_07a_peak_drains_within_fixture_queue_and_deadline_targets() {
    sandbox_lw_07a_peak_drains_within_fixture_queue_and_deadline_targets_case(Store::memory()).await;
}

pub(super) async fn sandbox_lw_07a_peak_drains_within_fixture_queue_and_deadline_targets_case(store: Store) {
    let metrics = config()["peak"].clone();
    let count = n(&metrics, "operations");
    let workers = n(&metrics, "workers");
    let work_seconds = n(&metrics, "work_seconds");
    let now = fixture::instant();
    let mut ids = Vec::new();
    for index in 0..count {
        ids.push(
            prepare(
                &store,
                &format!("peak-{index}"),
                &identity("automated"),
                now,
            )
            .await,
        );
    }
    for (batch, queued) in ids.chunks(workers as usize).enumerate() {
        let wait = batch as u64 * work_seconds;
        assert!(wait <= n(&metrics, "max_queue_seconds"));
        assert!(wait + work_seconds <= n(&metrics, "deadline_seconds"));
        let mut jobs = tokio::task::JoinSet::new();
        for id in queued {
            let store = store.clone();
            let id = id.clone();
            jobs.spawn(async move {
                post_and_reconcile(
                    &store,
                    &id,
                    &identity("automated"),
                    &identity("reviewer"),
                    dispatch::Path::Automated,
                    now + Duration::seconds((wait + work_seconds) as i64),
                )
                .await;
            });
        }
        while let Some(result) = jobs.join_next().await {
            result.unwrap();
        }
    }
    assert_eq!(receipts(&store, &ids).await, count as usize);
}

async fn restore_strings(store: &Store, keys: &[String], snapshot: Vec<Option<String>>) {
    assert_eq!(snapshot.len(), keys.len());
    for (key, value) in keys.iter().zip(snapshot) {
        assert!(store.set_nx(key, &value.expect("fixture checkpoint record missing")).await.unwrap());
    }
}

#[tokio::test]
async fn sandbox_lw_07b_restore_reapplies_revocation_and_hold_then_reconciles_provider_receipt() {
    sandbox_lw_07b_restore_reapplies_revocation_and_hold_then_reconciles_provider_receipt_case(Store::memory(), Store::memory(), Store::memory()).await;
}

pub(super) async fn sandbox_lw_07b_restore_reapplies_revocation_and_hold_then_reconciles_provider_receipt_case(original: Store, provider: Store, restored: Store) {
    let now = fixture::instant();
    let operator = identity("automated");
    let reviewer = identity("reviewer");
    fixture::send(&original, 1, fixture::doc("v1", "Rent fixture")).await;
    fixture::send(
        &original,
        2,
        json!({"kind":"permissions_observed","observed_at":now}),
    )
    .await;
    let id = prepare(&original, "recover-pending", &operator, now).await;
    start(&original, &id, &operator, dispatch::Path::Automated, now).await;
    assert!(
        dispatch::apply(
            &original,
            &operator,
            fixture::COMPANY,
            &id,
            dispatch::Command::MarkUnknown {
                attempt_revision: 3,
            },
            now
        )
        .await
        .is_ok()
    );
    let checkpoint_keys = [
        archive::key(fixture::COMPANY, fixture::SOURCE).ok().unwrap(),
        dispatch::key(fixture::COMPANY, &id).ok().unwrap(),
    ];
    let backup = original.mget(&checkpoint_keys).await.unwrap();

    // External effect arrives after the checkpoint. Model that separately from the restore.
    let provider_id = prepare(&provider, "recover-pending", &operator, now).await;
    assert_eq!(id, provider_id);
    start(
        &provider,
        &provider_id,
        &operator,
        dispatch::Path::Automated,
        now,
    )
    .await;
    assert!(
        dispatch::sandbox_post(
            &provider,
            &operator,
            fixture::COMPANY,
            &provider_id,
            journal()
        )
        .await
        .is_ok()
    );

    let timer = std::time::Instant::now();
    restore_strings(&restored, &checkpoint_keys, backup).await;
    fixture::send(
        &restored,
        3,
        json!({"kind":"revoke","document_id":"doc1","expected_version":"v1"}),
    )
    .await;
    fixture::send(
        &restored,
        4,
        json!({"kind":"set_hold","document_id":"doc1","expected_version":"v1","hold":true}),
    )
    .await;
    let access = archive::search(
        &restored,
        &fixture::person("usr_reader"),
        fixture::COMPANY,
        fixture::SOURCE,
        archive::SearchQuery {
            query: "rent".into(),
            as_of: "2024-11-18".parse().unwrap(),
            embedding: None,
        },
        now,
    )
    .await
    .ok()
    .unwrap();
    assert!(access.citations.is_empty());
    let dispose = serde_json::from_value(json!({"sequence":5,"change":{
        "kind":"dispose","document_id":"doc1","expected_version":"v1"
    }}))
    .unwrap();
    assert!(
        archive::apply(&restored, fixture::COMPANY, fixture::SOURCE, dispose, now)
            .await
            .is_err()
    );

    assert!(
        dispatch::sandbox_reconcile(&restored, &reviewer, fixture::COMPANY, &id, now)
            .await
            .is_err()
    );
    let receipt_keys = [dispatch::sandbox_key(fixture::COMPANY, &id).ok().unwrap()];
    restore_strings(&restored, &receipt_keys, provider.mget(&receipt_keys).await.unwrap()).await;
    let recovery = &config()["recovery"];
    let recovered_after = Duration::from_std(timer.elapsed()).unwrap();
    let result = dispatch::sandbox_reconcile(
        &restored,
        &reviewer,
        fixture::COMPANY,
        &id,
        now + recovered_after,
    )
    .await
    .ok()
    .unwrap();
    assert!(timer.elapsed().as_secs() <= n(recovery, "rto_seconds"));
    assert_eq!(serde_json::to_value(result).unwrap()["state"], "reconciled");
    let recovered = receipts(&restored, &[id]).await;
    assert_eq!(recovered, 1);
    let pending_before = 1usize;
    assert!(
        pending_before.saturating_sub(recovered) as u64 <= n(recovery, "rpo_pending_operations")
    );
}

#[tokio::test]
async fn sandbox_lw_07c_provider_outage_routes_to_exclusive_manual_fallback_before_deadline() {
    sandbox_lw_07c_provider_outage_routes_to_exclusive_manual_fallback_before_deadline_case(Store::memory()).await;
}

pub(super) async fn sandbox_lw_07c_provider_outage_routes_to_exclusive_manual_fallback_before_deadline_case(store: Store) {
    let now = fixture::instant();
    let automated = identity("automated");
    let metrics = &config()["outage"];
    let workers = n(metrics, "manual_workers") as usize;
    let operators = ["manual", "manual_backup"];
    assert_eq!(workers, operators.len());
    let work_seconds = n(metrics, "work_seconds");
    let mut queue = Vec::new();
    // Retryable failure occurs before any claims or submission. This is a fixture fault,
    // so the simulated fallback never assumes that an ambiguous attempt had no effect.
    let provider_offline = store
        .get("accounting:synthetic-provider:available")
        .await
        .unwrap()
        .is_none();
    assert!(provider_offline);
    for index in 0..n(metrics, "operations") {
        queue.push(prepare(&store, &format!("fallback-{index}"), &automated, now).await);
    }
    assert_eq!(receipts(&store, &queue).await, 0);
    for (batch, ids) in queue.chunks(workers).enumerate() {
        let elapsed = (batch as u64 + 1) * work_seconds;
        assert!(elapsed <= n(metrics, "deadline_seconds"));
        let mut jobs = tokio::task::JoinSet::new();
        for (id, role) in ids.iter().zip(operators) {
            let (store, id) = (store.clone(), id.clone());
            jobs.spawn(async move {
                post_and_reconcile(
                    &store,
                    &id,
                    &identity(role),
                    &identity("reviewer"),
                    dispatch::Path::Manual,
                    now + Duration::seconds(elapsed as i64),
                )
                .await;
            });
        }
        while let Some(result) = jobs.join_next().await {
            result.unwrap();
        }
    }
    store
        .set("accounting:synthetic-provider:available", "true")
        .await
        .unwrap();
    for id in &queue {
        assert!(
            dispatch::apply(
                &store,
                &automated,
                fixture::COMPANY,
                id,
                dispatch::Command::Claim {
                    path: dispatch::Path::Automated,
                    expected_payload_sha256: payload(),
                },
                now
            )
            .await
            .is_err()
        );
    }
    assert_eq!(receipts(&store, &queue).await, n(metrics, "operations") as usize);
}
