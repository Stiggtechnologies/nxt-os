use crate::{firm::staff::Identity, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SHARD_BYTES: usize = 1_000_000;
const MAX_CHUNK_BYTES: usize = 6_000;
const MAX_CHUNKS: usize = 64;
const MAX_FRESHNESS_SECONDS: i64 = 900;

pub(super) fn error(code: StatusCode, message: &str) -> ApiError {
    ApiError(code, message.into())
}

pub(super) fn unavailable(_: impl std::fmt::Display) -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "accounting storage unavailable",
    )
}

pub(super) fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

pub(super) fn sha256(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

pub(super) fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())
}

pub(super) async fn load<T: serde::de::DeserializeOwned>(
    store: &Store,
    key: &str,
) -> Result<(Option<String>, Option<T>), ApiError> {
    let raw = store.get(key).await.map_err(unavailable)?;
    let value = raw
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(unavailable)?;
    Ok((raw, value))
}

pub(super) async fn save<T: Serialize>(
    store: &Store,
    key: &str,
    old: Option<&str>,
    value: &T,
) -> Result<(), ApiError> {
    let raw = serde_json::to_string(value).map_err(unavailable)?;
    if raw.len() > MAX_SHARD_BYTES {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "accounting partition capacity reached",
        ));
    }
    if !store
        .compare_exchange(key, old, &raw)
        .await
        .map_err(unavailable)?
    {
        return Err(error(
            StatusCode::CONFLICT,
            "concurrent accounting update; reload and retry",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Chunk {
    pub locator: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Embedding>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Embedding {
    model: String,
    values: Vec<i16>,
}

impl Embedding {
    fn valid(&self) -> bool {
        safe_id(&self.model)
            && (2..=384).contains(&self.values.len())
            && self.values.iter().any(|&value| value != 0)
    }

    fn similarity(&self, other: &Self) -> Option<f64> {
        if self.model != other.model || self.values.len() != other.values.len() {
            return None;
        }
        let (dot, left, right) = self.values.iter().zip(&other.values).fold(
            (0_f64, 0_f64, 0_f64),
            |(dot, left, right), (&a, &b)| {
                let (a, b) = (f64::from(a), f64::from(b));
                (dot + a * b, left + a * a, right + b * b)
            },
        );
        (left > 0.0 && right > 0.0).then(|| dot / (left.sqrt() * right.sqrt()))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    pub id: String,
    pub version: String,
    pub content_sha256: String,
    pub from: NaiveDate,
    pub through: NaiveDate,
    pub user_ids: BTreeSet<String>,
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Record {
    version: String,
    hold: bool,
    seen_versions: BTreeSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<Document>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    preserved: Vec<Document>,
}

#[derive(Default, Serialize, Deserialize)]
struct Partition {
    sequence: u64,
    last_event_hash: String,
    #[serde(default)]
    revision_sha256: String,
    permissions_observed: Option<DateTime<Utc>>,
    records: BTreeMap<String, Record>,
}

impl Partition {
    fn revision(&self) -> &str {
        if self.revision_sha256.is_empty() {
            &self.last_event_hash
        } else {
            &self.revision_sha256
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Event {
    pub sequence: u64,
    pub change: Change,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Change {
    Upsert {
        document: Document,
    },
    Revoke {
        document_id: String,
        expected_version: String,
    },
    SetHold {
        document_id: String,
        expected_version: String,
        hold: bool,
    },
    Dispose {
        document_id: String,
        expected_version: String,
    },
    PermissionsObserved {
        observed_at: DateTime<Utc>,
    },
}

pub(super) fn key(company: &str, source: &str) -> Result<String, ApiError> {
    if !safe_id(company) || !safe_id(source) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid company or source partition",
        ));
    }
    Ok(format!("accounting:archive:v1:{company}:{source}"))
}

fn validate(document: &Document) -> bool {
    safe_id(&document.id)
        && safe_id(&document.version)
        && valid_hash(&document.content_sha256)
        && document.from <= document.through
        && document.user_ids.len() <= 128
        && document
            .user_ids
            .iter()
            .all(|id| safe_id(id) && id.starts_with("usr_"))
        && !document.chunks.is_empty()
        && document.chunks.len() <= MAX_CHUNKS
        && document.chunks.iter().all(|c| {
            !c.locator.trim().is_empty()
                && c.locator.len() <= 240
                && !c.locator.chars().any(char::is_control)
                && !c.text.trim().is_empty()
                && c.text.len() <= MAX_CHUNK_BYTES
                && !c.text.contains('\0')
                && c.embedding.as_ref().is_none_or(Embedding::valid)
        })
        && document
            .chunks
            .iter()
            .map(|c| &c.locator)
            .collect::<BTreeSet<_>>()
            .len()
            == document.chunks.len()
}

fn current<'a>(
    partition: &'a mut Partition,
    id: &str,
    version: &str,
) -> Result<&'a mut Record, ApiError> {
    partition
        .records
        .get_mut(id)
        .filter(|r| r.version == version)
        .ok_or_else(|| {
            error(
                StatusCode::CONFLICT,
                "archive version changed or unavailable",
            )
        })
}

pub(super) async fn apply(
    store: &Store,
    company: &str,
    source: &str,
    event: Event,
    now: DateTime<Utc>,
) -> Result<u64, ApiError> {
    let key = key(company, source)?;
    let (raw, saved) = load::<Partition>(store, &key).await?;
    let mut partition = saved.unwrap_or_default();
    let event_hash = sha256(&serde_json::to_vec(&event).map_err(unavailable)?);
    if event.sequence != 0
        && event.sequence == partition.sequence
        && event_hash == partition.last_event_hash
    {
        return Ok(partition.sequence);
    }
    if partition.sequence.checked_add(1) != Some(event.sequence) {
        return Err(error(
            StatusCode::CONFLICT,
            "archive event sequence gap or conflict",
        ));
    }
    match event.change {
        Change::Upsert { document } => {
            if !validate(&document) {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "invalid authorized document manifest",
                ));
            }
            let record = partition
                .records
                .entry(document.id.clone())
                .or_insert_with(|| Record {
                    version: String::new(),
                    hold: false,
                    seen_versions: BTreeSet::new(),
                    active: None,
                    preserved: Vec::new(),
                });
            if !record.seen_versions.insert(document.version.clone()) {
                return Err(error(
                    StatusCode::CONFLICT,
                    "document version cannot be reused",
                ));
            }
            if let Some(old) = record.active.take().filter(|_| record.hold) {
                record.preserved.push(old);
            }
            record.version = document.version.clone();
            record.active = Some(document);
        }
        Change::Revoke {
            document_id,
            expected_version,
        } => {
            let record = current(&mut partition, &document_id, &expected_version)?;
            if let Some(document) = record.active.as_mut() {
                document.user_ids.clear();
            }
        }
        Change::SetHold {
            document_id,
            expected_version,
            hold,
        } => {
            current(&mut partition, &document_id, &expected_version)?.hold = hold;
        }
        Change::Dispose {
            document_id,
            expected_version,
        } => {
            let record = current(&mut partition, &document_id, &expected_version)?;
            if record.hold {
                return Err(error(StatusCode::CONFLICT, "legal hold blocks disposition"));
            }
            record.active = None;
            record.preserved.clear();
        }
        Change::PermissionsObserved { observed_at } => {
            if observed_at > now
                || partition
                    .permissions_observed
                    .is_some_and(|old| observed_at < old)
            {
                return Err(error(
                    StatusCode::CONFLICT,
                    "invalid permission observation time",
                ));
            }
            partition.permissions_observed = Some(observed_at);
        }
    }
    partition.sequence = event.sequence;
    partition.revision_sha256 =
        sha256(format!("{}:{}", partition.revision(), event_hash).as_bytes());
    partition.last_event_hash = event_hash;
    save(store, &key, raw.as_deref(), &partition).await?;
    Ok(partition.sequence)
}

#[derive(Serialize)]
pub(super) struct Citation {
    pub source: String,
    pub document_id: String,
    pub version: String,
    pub content_sha256: String,
    pub locator: String,
    pub excerpt: String,
    pub excerpt_sha256: String,
    pub relationship: &'static str,
    lexical_hits: usize,
    semantic_similarity: Option<f64>,
    #[serde(skip)]
    score: f64,
}

#[derive(Serialize)]
pub(super) struct Search {
    pub coverage: &'static str,
    pub sequence: u64,
    pub revision_sha256: Option<String>,
    pub citations: Vec<Citation>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SearchQuery {
    pub query: String,
    pub as_of: NaiveDate,
    #[serde(default)]
    pub embedding: Option<Embedding>,
}

#[derive(Serialize)]
pub(super) struct ArchiveSearch {
    pub complete_for_requested_sources: bool,
    pub sources: BTreeMap<String, Search>,
}

pub(super) async fn search_sources(
    store: &Store,
    actor: &Identity,
    company: &str,
    sources: Vec<String>,
    input: SearchQuery,
    now: DateTime<Utc>,
) -> Result<ArchiveSearch, ApiError> {
    if sources.is_empty()
        || sources.len() > 24
        || sources.iter().collect::<BTreeSet<_>>().len() != sources.len()
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "provide 1 to 24 different archive partitions",
        ));
    }
    let mut results = BTreeMap::new();
    for source in sources {
        let result = search(store, actor, company, &source, input.clone(), now).await?;
        results.insert(source, result);
    }
    for (source, result) in &results {
        if result.coverage != "authorized_partition_searched" {
            continue;
        }
        let (_, current) = load::<Partition>(store, &key(company, source)?).await?;
        if !current.is_some_and(|value| {
            value.sequence == result.sequence
                && Some(value.revision()) == result.revision_sha256.as_deref()
        }) {
            return Err(error(
                StatusCode::CONFLICT,
                "archive changed during retrieval; retry",
            ));
        }
    }
    let complete = results
        .values()
        .all(|result| result.coverage == "authorized_partition_searched");
    Ok(ArchiveSearch {
        complete_for_requested_sources: complete,
        sources: results,
    })
}

pub(super) async fn source_revision(
    store: &Store,
    company: &str,
    source: &str,
    now: DateTime<Utc>,
) -> Result<Option<String>, ApiError> {
    let (_, stored) = load::<Partition>(store, &key(company, source)?).await?;
    Ok(stored
        .filter(|p| fresh(p, now))
        .map(|p| p.revision().to_owned()))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceRef {
    pub source: String,
    pub document_id: String,
    pub version: String,
    pub sha256: String,
}

pub(super) async fn verify_evidence(
    store: &Store,
    actor: &Identity,
    company: &str,
    reference: &EvidenceRef,
    effective_on: NaiveDate,
    now: DateTime<Utc>,
) -> Result<String, ApiError> {
    let denied = || error(StatusCode::NOT_FOUND, "verified source unavailable");
    if actor.staff_id().is_none()
        || !actor.can_client(company)
        || !safe_id(&reference.document_id)
        || !safe_id(&reference.version)
        || !valid_hash(&reference.sha256)
    {
        return Err(denied());
    }
    let key = key(company, &reference.source)?;
    let (raw, stored) = load::<Partition>(store, &key).await?;
    let partition = stored
        .filter(|partition| fresh(partition, now))
        .ok_or_else(denied)?;
    let doc = partition
        .records
        .get(&reference.document_id)
        .and_then(|record| record.active.as_ref())
        .filter(|doc| {
            doc.user_ids.contains(&actor.user_id)
                && doc.version == reference.version
                && doc.content_sha256 == reference.sha256
                && doc.from <= effective_on
                && effective_on <= doc.through
        })
        .ok_or_else(denied)?;
    let text: String = doc.chunks.iter().map(|chunk| chunk.text.as_str()).collect();
    if text.len() > 96 * 1024
        || sha256(text.as_bytes()) != reference.sha256
        || store.get(&key).await.map_err(unavailable)? != raw
    {
        return Err(denied());
    }
    Ok(text)
}

fn fresh(partition: &Partition, now: DateTime<Utc>) -> bool {
    partition.permissions_observed.is_some_and(|seen| {
        (0..=MAX_FRESHNESS_SECONDS).contains(&now.signed_duration_since(seen).num_seconds())
    })
}

fn terms(query: &str) -> BTreeSet<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub(super) async fn search(
    store: &Store,
    actor: &Identity,
    company: &str,
    source: &str,
    input: SearchQuery,
    now: DateTime<Utc>,
) -> Result<Search, ApiError> {
    if !actor.can_client(company) || actor.staff_id().is_none() {
        return Err(error(StatusCode::FORBIDDEN, "client scope required"));
    }
    if input.query.trim().is_empty()
        || input.query.len() > 240
        || input
            .embedding
            .as_ref()
            .is_some_and(|embedding| !embedding.valid())
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "query must contain 1 to 240 bytes",
        ));
    }
    let query = terms(&input.query);
    if query.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "query must include searchable terms",
        ));
    }
    let key = key(company, source)?;
    let (raw, saved) = load::<Partition>(store, &key).await?;
    let Some(partition) = saved else {
        return Ok(Search {
            coverage: "unavailable",
            sequence: 0,
            revision_sha256: None,
            citations: Vec::new(),
        });
    };
    if !fresh(&partition, now) {
        return Ok(Search {
            coverage: "permission_feed_stale",
            sequence: 0,
            revision_sha256: None,
            citations: Vec::new(),
        });
    }
    let mut citations = Vec::new();
    for record in partition.records.values() {
        let Some(doc) = record.active.as_ref().filter(|doc| {
            doc.user_ids.contains(&actor.user_id)
                && doc.from <= input.as_of
                && input.as_of <= doc.through
        }) else {
            continue;
        };
        for chunk in &doc.chunks {
            let lexical_hits = query.intersection(&terms(&chunk.text)).count();
            let semantic_similarity = input
                .embedding
                .as_ref()
                .zip(chunk.embedding.as_ref())
                .and_then(|(query, document)| query.similarity(document));
            if lexical_hits > 0 || semantic_similarity.is_some_and(|score| score >= 0.55) {
                let score = lexical_hits as f64 / query.len() as f64
                    + semantic_similarity.unwrap_or_default().max(0.0) * 0.75;
                citations.push(Citation {
                    source: source.into(),
                    document_id: doc.id.clone(),
                    version: doc.version.clone(),
                    content_sha256: doc.content_sha256.clone(),
                    locator: chunk.locator.clone(),
                    excerpt: chunk.text.clone(),
                    excerpt_sha256: sha256(chunk.text.as_bytes()),
                    relationship: "EXTRACTED_FROM",
                    lexical_hits,
                    semantic_similarity,
                    score,
                });
            }
        }
    }
    citations.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.document_id.cmp(&b.document_id))
            .then(a.locator.cmp(&b.locator))
    });
    citations.truncate(8);
    if store.get(&key).await.map_err(unavailable)? != raw {
        return Err(error(
            StatusCode::CONFLICT,
            "archive changed during retrieval; retry",
        ));
    }
    Ok(Search {
        coverage: "authorized_partition_searched",
        sequence: partition.sequence,
        revision_sha256: Some(partition.revision().to_owned()),
        citations,
    })
}
