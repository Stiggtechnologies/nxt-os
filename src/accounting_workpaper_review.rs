use super::{accounting_archive as archive, accounting_specialist as specialist};
use crate::{firm::staff::Identity, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Prepare {
    pub role: String,
    pub effective_on: NaiveDate,
    pub evidence: BTreeMap<String, archive::EvidenceRef>,
    pub control: specialist::Control,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_service_procedure: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Approve,
    ReturnForCorrection,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Review {
    expected_digest: String,
    decision: Decision,
    inspected_sources: bool,
}

#[derive(Deserialize, Serialize)]
struct Record {
    id: String,
    digest: String,
    prepared_by: String,
    prepared_at: DateTime<Utc>,
    workpaper: Prepare,
    result: Value,
    review: Option<RecordedReview>,
}

#[derive(Deserialize, Serialize)]
struct RecordedReview {
    reviewed_by: String,
    reviewed_at: DateTime<Utc>,
    review: Review,
}

fn conflict(message: &str) -> ApiError {
    archive::error(StatusCode::CONFLICT, message)
}

fn key(company: &str, id: &str) -> Result<String, ApiError> {
    if !archive::safe_id(company) || !archive::safe_id(id) {
        return Err(archive::error(
            StatusCode::BAD_REQUEST,
            "invalid workpaper identity",
        ));
    }
    Ok(format!("accounting:review-workpaper:v1:{company}:{id}"))
}

fn can_read(actor: &Identity, company: &str) -> bool {
    actor.staff_id().is_some()
        && actor.mfa
        && !actor.must_change_password
        && actor.can_client(company)
}

async fn verified_sources(
    store: &Store,
    actor: &Identity,
    company: &str,
    input: &Prepare,
    now: DateTime<Utc>,
    allowed: &impl Fn(&str) -> bool,
) -> Result<BTreeMap<String, String>, ApiError> {
    let mut contents = BTreeMap::new();
    let mut snapshots = BTreeMap::new();
    let mut digests = BTreeSet::new();
    for (name, evidence) in &input.evidence {
        if !allowed(&evidence.source) || !digests.insert(&evidence.sha256) {
            return Err(conflict(
                "workpaper needs different, approved evidence for each required input",
            ));
        }
        if !snapshots.contains_key(&evidence.source) {
            let revision = archive::source_revision(store, company, &evidence.source, now).await?;
            snapshots.insert(&evidence.source, revision);
        }
        let source =
            archive::verify_evidence(store, actor, company, evidence, input.effective_on, now)
                .await?;
        contents.insert(name.clone(), source);
    }
    for (source, revision) in snapshots {
        if archive::source_revision(store, company, source, now).await? != revision {
            return Err(conflict("source changed during workpaper verification"));
        }
    }
    Ok(contents)
}

fn verify_facts(
    control: &specialist::Control,
    contents: &BTreeMap<String, String>,
) -> Result<(), ApiError> {
    let expected = serde_json::to_value(control).map_err(archive::unavailable)?;
    let mut bound_documents = BTreeSet::new();
    let numeric_fields: Vec<_> = expected
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| *key != "kind")
        .collect();
    for (field, value) in &numeric_fields {
        let mut matched = None;
        for (name, text) in contents {
            let Ok(document) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if let Some(fact) = document
                .get("workpaper_facts")
                .and_then(|facts| facts.get(*field))
            {
                if fact != *value || matched.replace(name).is_some() {
                    return Err(conflict(
                        "supplied control input differs from or is ambiguous in the archived facts",
                    ));
                }
            }
        }
        bound_documents.insert(
            matched.ok_or_else(|| conflict("control input is not bound to an archived fact"))?,
        );
    }
    if numeric_fields.len() > 1 && bound_documents.len() < 2 {
        return Err(conflict(
            "tie-out inputs must come from different verified source documents",
        ));
    }
    Ok(())
}

pub(super) async fn prepare(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    input: Prepare,
    now: DateTime<Utc>,
    allowed: impl Fn(&str) -> bool,
) -> Result<Value, ApiError> {
    if !can_read(actor, company)
        || matches!(
            actor.role,
            crate::firm::staff::Role::ReadOnly | crate::firm::staff::Role::FrontDesk
        )
    {
        return Err(archive::error(
            StatusCode::FORBIDDEN,
            "verified preparer scope required",
        ));
    }
    let key = key(company, id)?;
    let hashes = input
        .evidence
        .iter()
        .map(|(name, reference)| (name.clone(), reference.sha256.clone()))
        .collect();
    let mut result = specialist::run(specialist::Input {
        role: input.role.clone(),
        evidence_sha256: hashes,
        control: input.control.clone(),
    })?;
    if result["state"] == "missing_evidence" {
        return Err(conflict("required evidence references are missing"));
    }
    let sources = verified_sources(store, actor, company, &input, now, &allowed).await?;
    verify_facts(&input.control, &sources)?;
    let procedure = super::accounting_service_procedures::run(
        &input.role,
        input.effective_on,
        &sources,
        input.require_service_procedure,
    )?;
    if procedure["passes"] == false {
        result["state"] = json!("control_exception");
    }
    result["service_procedure"] = procedure;
    result["evidence_content_verified"] = json!(true);
    let bytes = serde_json::to_vec(&(company, id, &actor.user_id, &input, &result))
        .map_err(archive::unavailable)?;
    let digest = archive::sha256(&bytes);
    let record = Record {
        id: id.into(),
        digest,
        prepared_by: actor.user_id.clone(),
        prepared_at: now,
        workpaper: input,
        result,
        review: None,
    };
    archive::save(store, &key, None, &record).await?;
    Ok(summary(&record))
}

fn summary(record: &Record) -> Value {
    json!({
        "id": record.id, "digest": record.digest, "result": record.result,
        "prepared_by": record.prepared_by, "prepared_at": record.prepared_at,
        "review": record.review, "live_write": false,
    })
}

pub(super) async fn inspect(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    now: DateTime<Utc>,
    allowed: impl Fn(&str) -> bool,
) -> Result<Value, ApiError> {
    let denied = || archive::error(StatusCode::NOT_FOUND, "workpaper unavailable");
    if !can_read(actor, company) {
        return Err(denied());
    }
    let (_, record) = archive::load::<Record>(store, &key(company, id)?).await?;
    let record = record
        .filter(|record| actor.is_reviewer() || actor.user_id == record.prepared_by)
        .ok_or_else(denied)?;
    let sources = verified_sources(store, actor, company, &record.workpaper, now, &allowed).await?;
    let mut value = summary(&record);
    value["sources"] = json!(sources);
    Ok(value)
}

pub(super) async fn review(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    decision: Review,
    now: DateTime<Utc>,
    allowed: impl Fn(&str) -> bool,
) -> Result<Value, ApiError> {
    if !can_read(actor, company) || !actor.is_reviewer() {
        return Err(archive::error(
            StatusCode::FORBIDDEN,
            "verified reviewer scope required",
        ));
    }
    let key = key(company, id)?;
    let (raw, record) = archive::load::<Record>(store, &key).await?;
    let mut record =
        record.ok_or_else(|| archive::error(StatusCode::NOT_FOUND, "workpaper unavailable"))?;
    if record.prepared_by == actor.user_id {
        return Err(archive::error(
            StatusCode::FORBIDDEN,
            "independent reviewer required",
        ));
    }
    if record.digest != decision.expected_digest || record.review.is_some() {
        return Err(conflict("workpaper version changed or already reviewed"));
    }
    let sources = verified_sources(store, actor, company, &record.workpaper, now, &allowed).await?;
    verify_facts(&record.workpaper.control, &sources)?;
    let procedure = super::accounting_service_procedures::run(
        &record.workpaper.role,
        record.workpaper.effective_on,
        &sources,
        record.workpaper.require_service_procedure,
    )?;
    if record.result["service_procedure"] != procedure {
        return Err(conflict(
            "service result no longer matches the frozen workpaper; prepare a new version",
        ));
    }
    if matches!(decision.decision, Decision::Approve)
        && (!decision.inspected_sources || record.result["state"] != "draft_for_independent_review")
    {
        return Err(conflict(
            "approval requires inspected sources and no pending control exception",
        ));
    }
    record.review = Some(RecordedReview {
        reviewed_by: actor.user_id.clone(),
        reviewed_at: now,
        review: decision,
    });
    archive::save(store, &key, raw.as_deref(), &record).await?;
    Ok(summary(&record))
}
