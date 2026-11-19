use crate::{firm::staff::Identity, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::accounting_archive::{error, load, safe_id, save, sha256, unavailable, valid_hash};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuleInput {
    pub rule: String,
    pub version: String,
    pub publisher: String,
    pub source_url: String,
    pub source_sha256: String,
    pub effective_from: NaiveDate,
    pub effective_through: NaiveDate,
    pub statement: String,
}

#[derive(Deserialize, Serialize)]
struct Rule {
    input: RuleInput,
    node_hash: String,
    proposed_by: String,
    recorded_at: DateTime<Utc>,
    approved_by: Option<String>,
    approved_at: Option<DateTime<Utc>>,
}

type Rules = BTreeMap<String, Rule>;

fn key(jurisdiction: &str) -> Result<String, ApiError> {
    if !safe_id(jurisdiction) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid jurisdiction"));
    }
    Ok(format!("accounting:authority:v1:{jurisdiction}"))
}

pub(super) async fn propose(
    store: &Store,
    actor: &Identity,
    jurisdiction: &str,
    input: RuleInput,
    now: DateTime<Utc>,
) -> Result<String, ApiError> {
    if !safe_id(&input.rule)
        || !safe_id(&input.version)
        || !valid_hash(&input.source_sha256)
        || input.effective_from > input.effective_through
        || input.statement.trim().is_empty()
        || input.statement.len() > 8_000
        || input.publisher.trim().is_empty()
        || input.publisher.len() > 160
        || input.source_url.len() > 512
        || !url::Url::parse(&input.source_url).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid authority node"));
    }
    let key = key(jurisdiction)?;
    let (raw, stored) = load::<Rules>(store, &key).await?;
    let mut rules = stored.unwrap_or_default();
    let version = format!("{}:{}", input.rule, input.version);
    if rules.contains_key(&version) {
        return Err(error(
            StatusCode::CONFLICT,
            "authority version already exists",
        ));
    }
    let encoded = serde_json::to_vec(&(jurisdiction, &input)).map_err(unavailable)?;
    let node_hash = sha256(&encoded);
    rules.insert(
        version,
        Rule {
            input,
            node_hash: node_hash.clone(),
            proposed_by: actor.user_id.clone(),
            recorded_at: now,
            approved_by: None,
            approved_at: None,
        },
    );
    save(store, &key, raw.as_deref(), &rules).await?;
    Ok(node_hash)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Approval {
    pub rule: String,
    pub version: String,
    pub node_hash: String,
}

pub(super) async fn approve(
    store: &Store,
    actor: &Identity,
    jurisdiction: &str,
    input: Approval,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let key = key(jurisdiction)?;
    let (raw, stored) = load::<Rules>(store, &key).await?;
    let mut rules = stored.unwrap_or_default();
    let version = format!("{}:{}", input.rule, input.version);
    let rule = rules
        .get(&version)
        .filter(|r| r.node_hash == input.node_hash)
        .ok_or_else(|| {
            error(
                StatusCode::CONFLICT,
                "authority version or hash does not match",
            )
        })?;
    if rule.proposed_by == actor.user_id || rule.approved_by.is_some() {
        return Err(error(
            StatusCode::FORBIDDEN,
            "independent approval required for pending authority",
        ));
    }
    if rules.values().any(|other| {
        other.approved_at.is_some()
            && other.input.rule == rule.input.rule
            && other.input.effective_from <= rule.input.effective_through
            && rule.input.effective_from <= other.input.effective_through
    }) {
        return Err(error(
            StatusCode::CONFLICT,
            "approved rules overlap; resolve outside publication",
        ));
    }
    let rule = rules.get_mut(&version).unwrap();
    rule.approved_by = Some(actor.user_id.clone());
    rule.approved_at = Some(now);
    save(store, &key, raw.as_deref(), &rules).await
}

pub(super) async fn resolve(
    store: &Store,
    jurisdiction: &str,
    day: NaiveDate,
    known_at: DateTime<Utc>,
) -> Result<serde_json::Value, ApiError> {
    let (_, rules) = load::<Rules>(store, &key(jurisdiction)?).await?;
    let nodes: Vec<_> = rules
        .unwrap_or_default()
        .into_values()
        .filter(|rule| {
            rule.approved_at
                .is_some_and(|approved| approved <= known_at)
                && rule.input.effective_from <= day
                && day <= rule.input.effective_through
        })
        .map(|rule| {
            serde_json::json!({
                "rule_version": rule.input,
                "node_hash": rule.node_hash,
                "approved_at": rule.approved_at,
                "edge": { "type": "DERIVED_FROM", "source_sha256": rule.input.source_sha256 }
            })
        })
        .collect();
    Ok(
        serde_json::json!({"jurisdiction": jurisdiction, "effective_on": day, "known_at": known_at, "nodes": nodes}),
    )
}
