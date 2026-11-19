use crate::{firm::staff::Identity, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{
    RELEASE_GATES,
    accounting_archive::{error, load, safe_id, save, sha256, unavailable, valid_hash},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FixtureEvidence {
    pub result: TestResult,
    pub artifact_sha256: String,
    pub artifact_id: String,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum TestResult {
    Pass,
    Fail,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub firm_entity: String,
    pub service: String,
    pub client_class: String,
    pub jurisdiction: String,
    pub operation: String,
    pub connector: String,
    pub environment: String,
    pub region: String,
    pub release_sha256: String,
    pub controls_sha256: String,
    pub expires_at: DateTime<Utc>,
    pub tests: BTreeMap<String, FixtureEvidence>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Release {
    manifest: Manifest,
    manifest_sha256: String,
    proposed_by: String,
    proposed_at: DateTime<Utc>,
    reviewed_by: Option<String>,
    suspended_by: Option<String>,
}

fn key(profile: &str) -> Result<String, ApiError> {
    if !safe_id(profile) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid production profile"));
    }
    Ok(format!("accounting:release:v1:{profile}"))
}

fn fields_valid(m: &Manifest, now: DateTime<Utc>) -> bool {
    [
        &m.firm_entity,
        &m.service,
        &m.client_class,
        &m.jurisdiction,
        &m.operation,
        &m.connector,
        &m.region,
    ]
    .into_iter()
    .all(|v| safe_id(v))
        && m.environment == "production"
        && m.expires_at > now
        && valid_hash(&m.release_sha256)
        && valid_hash(&m.controls_sha256)
}

fn required_cases_pass(m: &Manifest) -> bool {
    let expected: Vec<_> = RELEASE_GATES
        .iter()
        .flat_map(|(_, _, cases)| cases.iter())
        .collect();
    m.tests.len() == expected.len()
        && expected.into_iter().all(|id| {
            m.tests.get(*id).is_some_and(|result| {
                result.result == TestResult::Pass
                    && valid_hash(&result.artifact_sha256)
                    && safe_id(&result.artifact_id)
            })
        })
}

pub(super) async fn propose(
    store: &Store,
    actor: &Identity,
    profile: &str,
    manifest: Manifest,
    now: DateTime<Utc>,
) -> Result<String, ApiError> {
    if !fields_valid(&manifest, now) || manifest.tests.len() > 21 {
        return Err(error(StatusCode::BAD_REQUEST, "invalid release manifest"));
    }
    let key = key(profile)?;
    let (raw, existing) = load::<Release>(store, &key).await?;
    if existing.is_some() {
        return Err(error(
            StatusCode::CONFLICT,
            "profile version already exists; propose a new version",
        ));
    }
    let encoded = serde_json::to_vec(&(profile, &manifest)).map_err(unavailable)?;
    let manifest_sha256 = sha256(&encoded);
    save(
        store,
        &key,
        raw.as_deref(),
        &Release {
            manifest,
            manifest_sha256: manifest_sha256.clone(),
            proposed_by: actor.user_id.clone(),
            proposed_at: now,
            reviewed_by: None,
            suspended_by: None,
        },
    )
    .await?;
    Ok(manifest_sha256)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewInput {
    pub manifest_sha256: String,
}

pub(super) async fn review(
    store: &Store,
    actor: &Identity,
    profile: &str,
    input: ReviewInput,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let key = key(profile)?;
    let (raw, stored) = load::<Release>(store, &key).await?;
    let mut release =
        stored.ok_or_else(|| error(StatusCode::NOT_FOUND, "release profile unavailable"))?;
    if release.proposed_by == actor.user_id {
        return Err(error(
            StatusCode::FORBIDDEN,
            "independent release reviewer required",
        ));
    }
    if release.manifest_sha256 != input.manifest_sha256
        || release.reviewed_by.is_some()
        || release.suspended_by.is_some()
        || !fields_valid(&release.manifest, now)
        || !required_cases_pass(&release.manifest)
    {
        return Err(error(
            StatusCode::CONFLICT,
            "release evidence is stale, incomplete, or failed",
        ));
    }
    release.reviewed_by = Some(actor.user_id.clone());
    save(store, &key, raw.as_deref(), &release).await
}

pub(super) async fn suspend(
    store: &Store,
    actor: &Identity,
    profile: &str,
) -> Result<(), ApiError> {
    let key = key(profile)?;
    let (raw, stored) = load::<Release>(store, &key).await?;
    let mut release =
        stored.ok_or_else(|| error(StatusCode::NOT_FOUND, "release profile unavailable"))?;
    if release.suspended_by.is_some() {
        return Ok(());
    }
    release.suspended_by = Some(actor.user_id.clone());
    save(store, &key, raw.as_deref(), &release).await
}

pub(super) async fn status(
    store: &Store,
    profile: &str,
    now: DateTime<Utc>,
) -> Result<serde_json::Value, ApiError> {
    let (_, result) = load::<Release>(store, &key(profile)?).await?;
    let release =
        result.ok_or_else(|| error(StatusCode::NOT_FOUND, "release profile unavailable"))?;
    let state = if release.suspended_by.is_some() {
        "suspended"
    } else if now >= release.manifest.expires_at {
        "expired"
    } else if release.reviewed_by.is_some() {
        "reviewed_pending_production_controls"
    } else {
        "draft"
    };
    Ok(serde_json::json!({
        "state": state,
        "manifest_sha256": release.manifest_sha256,
        "reported_cases_complete": required_cases_pass(&release.manifest),
        "live_write_authorized": false,
        "pending": [
            "verified independent acceptance evidence",
            "firm operations, security and professional release authority",
            "approved connector integrated with the dispatch gate",
        ],
        "release": release,
    }))
}
