use super::{AGENT_CATALOG, accounting_archive as archive, accounting_authority as authority};
use crate::{firm::staff::Identity, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub role: String,
    pub jurisdiction: String,
    pub sources: Vec<String>,
    pub query: String,
    pub effective_on: NaiveDate,
    #[serde(default)]
    pub embedding: Option<archive::Embedding>,
}

#[derive(Serialize, Deserialize)]
struct EvidenceEdge {
    source: String,
    document_id: String,
    version: String,
    locator_sha256: String,
    excerpt_sha256: String,
}

#[derive(Serialize, Deserialize)]
struct Packet {
    company: String,
    owner: String,
    role: String,
    jurisdiction: String,
    effective_on: NaiveDate,
    revisions: BTreeMap<String, Option<String>>,
    evidence: Vec<EvidenceEdge>,
    authority_node_hashes: Vec<String>,
}

fn authority_hashes(authorities: &Value) -> Vec<String> {
    authorities["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["node_hash"].as_str().map(str::to_owned))
        .collect()
}

fn lookup_key(company: &str, id: &str) -> Result<String, ApiError> {
    if !archive::safe_id(company) || !archive::valid_hash(id) {
        return Err(archive::error(
            StatusCode::BAD_REQUEST,
            "invalid evidence packet identity",
        ));
    }
    Ok(format!("accounting:graph:v1:{company}:{id}"))
}

pub(super) async fn prepare(
    store: &Store,
    actor: &Identity,
    company: &str,
    request: Request,
    now: DateTime<Utc>,
) -> Result<Value, ApiError> {
    let catalog: Vec<Value> = serde_json::from_str(AGENT_CATALOG).map_err(archive::unavailable)?;
    let role = catalog
        .iter()
        .find(|profile| profile["role"].as_str() == Some(request.role.as_str()))
        .ok_or_else(|| archive::error(StatusCode::BAD_REQUEST, "unknown specialist role"))?;
    let archive_evidence = archive::search_sources(
        store,
        actor,
        company,
        request.sources,
        archive::SearchQuery {
            query: request.query,
            as_of: request.effective_on,
            embedding: request.embedding,
        },
        now,
    )
    .await?;
    let authorities =
        authority::resolve(store, &request.jurisdiction, request.effective_on, now).await?;
    let packet = Packet {
        company: company.into(),
        owner: actor.user_id.clone(),
        role: request.role,
        jurisdiction: request.jurisdiction,
        effective_on: request.effective_on,
        revisions: archive_evidence
            .sources
            .iter()
            .map(|(source, result)| (source.clone(), result.revision_sha256.clone()))
            .collect(),
        evidence: archive_evidence
            .sources
            .values()
            .flat_map(|result| &result.citations)
            .map(|citation| EvidenceEdge {
                source: citation.source.clone(),
                document_id: citation.document_id.clone(),
                version: citation.version.clone(),
                locator_sha256: archive::sha256(citation.locator.as_bytes()),
                excerpt_sha256: citation.excerpt_sha256.clone(),
            })
            .collect(),
        authority_node_hashes: authority_hashes(&authorities),
    };
    let encoded = serde_json::to_string(&packet).map_err(archive::unavailable)?;
    let id = archive::sha256(encoded.as_bytes());
    store
        .set_nx(&lookup_key(company, &id)?, &encoded)
        .await
        .map_err(archive::unavailable)?;
    if !is_current(store, &packet, now).await? {
        return Err(archive::error(
            StatusCode::CONFLICT,
            "evidence graph changed during preparation",
        ));
    }
    Ok(json!({
        "packet_id": id, "role": role, "state": "preparation_only",
        "archive": archive_evidence, "authority": authorities, "live_write": false,
    }))
}

async fn is_current(store: &Store, packet: &Packet, now: DateTime<Utc>) -> Result<bool, ApiError> {
    for (source, expected) in &packet.revisions {
        if &archive::source_revision(store, &packet.company, source, now).await? != expected {
            return Ok(false);
        }
    }
    let rules = authority::resolve(store, &packet.jurisdiction, packet.effective_on, now).await?;
    Ok(authority_hashes(&rules) == packet.authority_node_hashes)
}

pub(super) async fn status(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    now: DateTime<Utc>,
    source_approved: impl Fn(&str, &str) -> bool,
) -> Result<Value, ApiError> {
    if !actor.can_client(company) {
        return Err(archive::error(
            StatusCode::FORBIDDEN,
            "client scope required",
        ));
    }
    let (_, packet) = archive::load::<Packet>(store, &lookup_key(company, id)?).await?;
    let packet = packet
        .filter(|record| record.owner == actor.user_id)
        .ok_or_else(|| archive::error(StatusCode::NOT_FOUND, "evidence packet unavailable"))?;
    let changed = packet
        .revisions
        .keys()
        .any(|source| !source_approved(company, source))
        || !is_current(store, &packet, now).await?;
    let incomplete = packet.revisions.values().any(Option::is_none)
        || packet.evidence.is_empty()
        || packet.authority_node_hashes.is_empty();
    Ok(json!({"packet_id": id, "role": packet.role,
        "state": if changed { "stale" } else if incomplete { "incomplete" } else { "current" },
        "live_write": false}))
}
