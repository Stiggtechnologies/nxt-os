//! DT Max preservation import. Rows are declared synthetic accounts until a real
//! DT Max export is pinned. Vendor backups are refused. Filing stays off.
//! Chunks checkpoint, a bad row is an exception, and the rest of the chunk continues.

pub mod files;
mod lanes;
mod routes;

use crate::store::Store;
use axum::http::StatusCode;
use lanes::{profile, Profile, LANES};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub use routes::router;

const MAX_CHUNK: usize = 1_000;
const MAX_RECORDS: u64 = 100_000;
const MAX_EXCEPTIONS_STORED: usize = 500;

pub struct SourceImport {
    store: Store,
}

#[derive(Debug)]
pub struct ImportError(pub StatusCode, pub String);

type Result<T> = std::result::Result<T, ImportError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ParallelInput {
    pub source_records: u64,
    pub source_amount_cents: String,
    pub as_of: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OpenBatch {
    pub format: String,
    pub cohort_ref: String,
    pub synthetic: bool,
    pub filing_requested: bool,
    pub tax_cases_imported: u32,
    pub source_kind: String,
    pub expected_records: u64,
    pub expected_accepted_amount_cents: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct AcceptedRow {
    source_key: String,
    kind: String,
    amount_cents: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Exception {
    chunk_seq: u32,
    index: usize,
    source_key: Option<String>,
    code: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChunkSaved {
    content_sha256: String,
    submitted: u64,
    accepted: u64,
    excepted: u64,
    accepted_amount_cents: String,
    exceptions: Vec<Exception>,
    accepted_rows: Vec<AcceptedRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Head {
    lane: String,
    batch_id: String,
    open: OpenBatch,
    next_seq: u32,
    submitted: u64,
    accepted: u64,
    excepted: u64,
    absent_amounts: u64,
    accepted_amount_cents: String,
    keys: BTreeSet<String>,
    kinds_count: BTreeMap<String, u64>,
    exceptions: Vec<Exception>,
    exceptions_truncated: bool,
    chunk_sha256: Vec<String>,
    committed_request: Option<String>,
}

impl SourceImport {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn coverage() -> Value {
        json!({
            "format": "nxt-source-import-coverage/v1",
            "priority": "dtmax_tax",
            "filing_enabled": false,
            "live_cutover_allowed": false,
            "lanes": LANES.iter().map(|lane| json!({
                "module": lane.module,
                "source": lane.legacy_source.unwrap_or("none"),
                "lane": lane.id,
                "importer_status": lane.status,
                "vendor_format_observed": lane.vendor_format_observed,
                "bulk_checkpoint_api": lane.route.is_some(),
                "route": lane.route,
                "required_types": lane.required_types,
                "money_types": lane.money_types,
                "hash_types": lane.hash_types,
                "expected_export": lane.expected_export,
                "blockers": lane.blockers,
            })).collect::<Vec<_>>(),
        })
    }

    pub async fn open(&self, lane: &str, batch: OpenBatch) -> Result<Value> {
        let profile = profile(lane).ok_or_else(|| missing("Unknown preservation lane"))?;
        validate_open(profile, &batch)?;
        let batch_id = sha_json(&batch);
        let key = head_key(lane, &batch_id);
        if let Some(raw) = self.store.get(&key).await.map_err(|error| storage(&error))? {
            let head: Head = serde_json::from_str(&raw).map_err(|_| storage("corrupt batch"))?;
            if head.open != batch {
                return Err(conflict("Batch id collision with different manifest"));
            }
            return Ok(progress(&head, true));
        }
        let head = Head {
            lane: lane.into(),
            batch_id: batch_id.clone(),
            open: batch,
            next_seq: 0,
            submitted: 0,
            accepted: 0,
            excepted: 0,
            absent_amounts: 0,
            accepted_amount_cents: "0".into(),
            keys: BTreeSet::new(),
            kinds_count: BTreeMap::new(),
            exceptions: Vec::new(),
            exceptions_truncated: false,
            chunk_sha256: Vec::new(),
            committed_request: None,
        };
        let raw = serde_json::to_string(&head).map_err(|_| storage("encode"))?;
        self.store.set_nx(&key, &raw).await.map_err(|error| storage(&error))?;
        let saved = self.store.get(&key).await.map_err(|error| storage(&error))?.ok_or_else(|| storage("missing batch"))?;
        if saved != raw {
            let existing: Head = serde_json::from_str(&saved).map_err(|_| storage("corrupt batch"))?;
            if existing.open != head.open {
                return Err(conflict("Batch id collision with different manifest"));
            }
            return Ok(progress(&existing, true));
        }
        Ok(progress(&head, false))
    }

    pub async fn chunk(&self, lane: &str, batch_id: &str, seq: u32, records: &[Value]) -> Result<Value> {
        let profile = profile(lane).ok_or_else(|| missing("Unknown preservation lane"))?;
        if !digest(batch_id) || records.is_empty() || records.len() > MAX_CHUNK {
            return Err(bad("Chunk sequence requires 1 to 1000 records"));
        }
        let content = sha_json(&records);
        for _ in 0..8 {
            let key = head_key(lane, batch_id);
            let current = self.store.get(&key).await.map_err(|error| storage(&error))?.ok_or_else(|| missing("Batch not found"))?;
            let mut head: Head = serde_json::from_str(&current).map_err(|_| storage("corrupt batch"))?;
            if head.batch_id != batch_id || head.lane != lane {
                return Err(storage("batch id mismatch"));
            }
            if head.committed_request.is_some() {
                return Err(conflict("Batch is already committed"));
            }
            if seq < head.next_seq {
                let saved = self.load_chunk(lane, batch_id, seq).await?;
                if saved.content_sha256 != content {
                    return Err(conflict("Chunk sequence was already stored with different records"));
                }
                return Ok(chunk_body(&head, &saved, true));
            }
            if seq > head.next_seq {
                return Err(conflict("Checkpoint gap: resume at next_seq"));
            }
            if head.submitted.saturating_add(records.len() as u64) > head.open.expected_records {
                return Err(conflict("Chunk would pass the declared record count"));
            }
            let saved = apply_chunk(profile, &mut head, seq, records, &content)?;
            let chunk_raw = serde_json::to_string(&saved).map_err(|_| storage("encode chunk"))?;
            let chunk_key = chunk_key(lane, batch_id, seq);
            self.store.set_nx(&chunk_key, &chunk_raw).await.map_err(|error| storage(&error))?;
            let stored = self.store.get(&chunk_key).await.map_err(|error| storage(&error))?.ok_or_else(|| storage("missing chunk"))?;
            if stored != chunk_raw {
                return Err(conflict("Chunk sequence was already stored with different records"));
            }
            let next = serde_json::to_string(&head).map_err(|_| storage("encode batch"))?;
            if self.store.compare_exchange(&key, Some(&current), &next).await.map_err(|error| storage(&error))? {
                let head: Head = serde_json::from_str(&next).map_err(|_| storage("corrupt batch"))?;
                return Ok(chunk_body(&head, &saved, false));
            }
        }
        Err(conflict("Batch checkpoint changed; retry this chunk"))
    }

    pub async fn progress(&self, lane: &str, batch_id: &str) -> Result<Value> {
        Ok(progress(&self.load_head(lane, batch_id).await?, false))
    }

    pub async fn recover(&self, lane: &str, batch_id: &str) -> Result<Value> {
        let head = self.load_head(lane, batch_id).await?;
        if head.committed_request.is_none() {
            return Err(conflict("Recovery receipt requires a committed batch"));
        }
        self.receipt(&head, true).await
    }

    pub async fn commit(&self, lane: &str, batch_id: &str, request_id: &str, reviewer_label: &str) -> Result<Value> {
        let profile = profile(lane).ok_or_else(|| missing("Unknown preservation lane"))?;
        if !valid_token(request_id) || reviewer_label.trim().is_empty() || reviewer_label.len() > 200 {
            return Err(bad("Request id and reviewer label are required"));
        }
        let key = head_key(lane, batch_id);
        let current = self.store.get(&key).await.map_err(|error| storage(&error))?.ok_or_else(|| missing("Batch not found"))?;
        let mut head: Head = serde_json::from_str(&current).map_err(|_| storage("corrupt batch"))?;
        if let Some(existing) = &head.committed_request {
            if existing == request_id {
                return Ok(self.receipt(&head, true).await?);
            }
            return Err(conflict("Batch was committed with a different request id"));
        }
        self.reconcile(&head).await?;
        if profile.require_all_kinds && profile.kinds.iter().any(|kind| head.kinds_count.get(*kind).copied().unwrap_or(0) == 0) {
            return Err(conflict("Accepted rows are missing a required record type"));
        }
        if head.submitted != head.open.expected_records || head.accepted_amount_cents != head.open.expected_accepted_amount_cents {
            return Err(conflict("Accepted count or amount does not match the declared control"));
        }
        head.committed_request = Some(request_id.into());
        let next = serde_json::to_string(&head).map_err(|_| storage("encode"))?;
        if !self.store.compare_exchange(&key, Some(&current), &next).await.map_err(|error| storage(&error))? {
            let raced: Head = serde_json::from_str(&self.store.get(&key).await.map_err(|error| storage(&error))?.unwrap_or_default()).map_err(|_| conflict("Commit lost a checkpoint race"))?;
            if raced.committed_request.as_deref() == Some(request_id) {
                return Ok(self.receipt(&raced, true).await?);
            }
            return Err(conflict("Commit lost a checkpoint race"));
        }
        let head: Head = serde_json::from_str(&next).unwrap();
        Ok(self.receipt(&head, false).await?)
    }

    async fn reconcile(&self, head: &Head) -> Result<()> {
        let mut submitted = 0u64;
        let mut accepted = 0u64;
        let mut excepted = 0u64;
        let mut amount: i128 = 0;
        let mut absent = 0u64;
        let mut keys = BTreeSet::new();
        let mut kinds_count = BTreeMap::new();
        for seq in 0..head.next_seq {
            let chunk = self.load_chunk(&head.lane, &head.batch_id, seq).await?;
            submitted += chunk.submitted;
            accepted += chunk.accepted;
            excepted += chunk.excepted;
            for row in &chunk.accepted_rows {
                if !keys.insert(row.source_key.clone()) {
                    return Err(storage("Recovered duplicate accepted key"));
                }
                *kinds_count.entry(row.kind.clone()).or_default() += 1;
                match &row.amount_cents {
                    Some(cents) => amount += money(cents).map_err(|_| storage("corrupt amount"))?,
                    None => absent += 1,
                }
            }
        }
        if submitted != head.submitted || accepted != head.accepted || excepted != head.excepted || absent != head.absent_amounts || amount.to_string() != head.accepted_amount_cents || keys != head.keys || kinds_count != head.kinds_count {
            return Err(storage("Checkpoint totals do not match stored chunks"));
        }
        if accepted + excepted != submitted {
            return Err(storage("Accepted and exception counts do not cover submitted records"));
        }
        Ok(())
    }

    async fn receipt(&self, head: &Head, replay: bool) -> Result<Value> {
        self.reconcile(head).await?;
        Ok(json!({
            "format": profile(&head.lane).map(|profile| profile.receipt_format).unwrap_or("nxt-preservation-receipt/v1"),
            "lane": head.lane,
            "write_back": false,
            "batch_id": head.batch_id,
            "request_id": head.committed_request,
            "idempotent_replay": replay,
            "records_submitted": head.submitted,
            "records_accepted": head.accepted,
            "records_excepted": head.excepted,
            "absent_amounts": head.absent_amounts,
            "accepted_amount_cents": head.accepted_amount_cents,
            "chunks": head.next_seq,
            "decoder_verified": false,
            "tax_cases_imported": 0,
            "filing_enabled": false,
            "vendor_format_accepted": false,
            "real_sample_accepted": false,
            "live_cutover_allowed": false,
            "external_production_writes": 0,
            "recovery_matches": true,
        }))
    }

    pub async fn parallel(&self, lane: &str, batch_id: &str, input: ParallelInput) -> Result<Value> {
        let profile = profile(lane).ok_or_else(|| missing("Unknown preservation lane"))?;
        if !valid_date(&input.as_of) || money(&input.source_amount_cents).is_err() {
            return Err(bad("Parallel report needs a real date and a canonical legacy amount"));
        }
        let head = self.load_head(lane, batch_id).await?;
        if head.committed_request.is_none() {
            return Err(conflict("Parallel reconciliation requires a committed read-only copy"));
        }
        self.reconcile(&head).await?;
        let nxt_amount = money(&head.accepted_amount_cents).map_err(|_| storage("corrupt amount"))?;
        let source_amount = money(&input.source_amount_cents).unwrap();
        let count_difference = head.accepted as i128 - input.source_records as i128;
        let amount_difference = nxt_amount - source_amount;
        let counts_match = count_difference == 0;
        let balances_match = amount_difference == 0;
        let mut differences = Vec::new();
        if !counts_match {
            differences.push(json!({"field":"records","legacy":input.source_records.to_string(),"nxt":head.accepted.to_string(),"difference":count_difference.to_string()}));
        }
        if !balances_match {
            differences.push(json!({"field":"accepted_amount_cents","legacy":input.source_amount_cents,"nxt":head.accepted_amount_cents,"difference":amount_difference.to_string()}));
        }
        let report_id = sha(&format!("{lane}:{batch_id}:{}:{}:{}", input.as_of, input.source_records, input.source_amount_cents));
        let report = json!({
            "format": "nxt-parallel-reconciliation/v1",
            "report_id": report_id,
            "lane": lane,
            "module": profile.id,
            "batch_id": batch_id,
            "as_of": input.as_of,
            "source_of_truth": "legacy",
            "nxt_role": "read_only_copy",
            "write_back": false,
            "filing_enabled": false,
            "decommission_allowed": false,
            "decommission_decision_owner": "Orville",
            "counts": {"legacy": input.source_records, "nxt_accepted": head.accepted, "nxt_excepted": head.excepted, "difference": count_difference.to_string()},
            "balances": {"legacy_cents": input.source_amount_cents, "nxt_cents": head.accepted_amount_cents, "difference_cents": amount_difference.to_string()},
            "differences": differences,
            "exceptions": {"nxt_excepted": head.excepted, "stored": head.exceptions.len(), "truncated": head.exceptions_truncated},
            "exit_criteria": exit_criteria(counts_match, balances_match),
            "rerun": "POST this same JSON again. Identical legacy totals return the stored report. A scheduler may call it; nothing is written back to the legacy system.",
            "idempotent_replay": false,
            "live_cutover_allowed": false,
        });
        let key = format!("preserve:{lane}:{batch_id}:parallel:{report_id}");
        if let Some(existing) = self.store.get(&key).await.map_err(|error| storage(&error))? {
            let mut stored: Value = serde_json::from_str(&existing).map_err(|_| storage("corrupt report"))?;
            stored["idempotent_replay"] = json!(true);
            return Ok(stored);
        }
        let raw = serde_json::to_string(&report).map_err(|_| storage("encode report"))?;
        self.store.set_nx(&key, &raw).await.map_err(|error| storage(&error))?;
        self.store.set(&format!("preserve:{lane}:{batch_id}:parallel-latest"), &report_id).await.map_err(|error| storage(&error))?;
        Ok(report)
    }

    pub async fn latest_parallel(&self, lane: &str, batch_id: &str) -> Result<Value> {
        let _ = self.load_head(lane, batch_id).await?;
        let latest = self.store.get(&format!("preserve:{lane}:{batch_id}:parallel-latest")).await.map_err(|error| storage(&error))?.ok_or_else(|| missing("No parallel report has been run"))?;
        let raw = self.store.get(&format!("preserve:{lane}:{batch_id}:parallel:{latest}")).await.map_err(|error| storage(&error))?.ok_or_else(|| missing("No parallel report has been run"))?;
        serde_json::from_str(&raw).map_err(|_| storage("corrupt report"))
    }

    async fn load_head(&self, lane: &str, batch_id: &str) -> Result<Head> {
        if !digest(batch_id) {
            return Err(bad("Invalid batch id"));
        }
        let raw = self.store.get(&head_key(lane, batch_id)).await.map_err(|error| storage(&error))?.ok_or_else(|| missing("Batch not found"))?;
        let head: Head = serde_json::from_str(&raw).map_err(|_| storage("corrupt batch"))?;
        if head.lane != lane {
            return Err(missing("Batch not found"));
        }
        Ok(head)
    }

    async fn load_chunk(&self, lane: &str, batch_id: &str, seq: u32) -> Result<ChunkSaved> {
        let raw = self.store.get(&chunk_key(lane, batch_id, seq)).await.map_err(|error| storage(&error))?.ok_or_else(|| storage("missing chunk"))?;
        serde_json::from_str(&raw).map_err(|_| storage("corrupt chunk"))
    }
}

fn apply_chunk(profile: &Profile, head: &mut Head, seq: u32, records: &[Value], content: &str) -> Result<ChunkSaved> {
    let mut saved = ChunkSaved {
        content_sha256: content.into(),
        submitted: 0,
        accepted: 0,
        excepted: 0,
        accepted_amount_cents: "0".into(),
        exceptions: Vec::new(),
        accepted_rows: Vec::new(),
    };
    let mut amount: i128 = 0;
    for (index, record) in records.iter().enumerate() {
        saved.submitted += 1;
        match accept_record(profile, record, &mut head.keys) {
            Ok(row) => {
                *head.kinds_count.entry(row.kind.clone()).or_default() += 1;
                if let Some(cents) = &row.amount_cents {
                    amount += money(cents).map_err(|_| bad("Amount overflow"))?;
                } else {
                    head.absent_amounts += 1;
                }
                saved.accepted_rows.push(row);
                saved.accepted += 1;
            }
            Err((source_key, code)) => {
                saved.excepted += 1;
                let exception = Exception { chunk_seq: seq, index, source_key, code };
                saved.exceptions.push(exception.clone());
                if head.exceptions.len() < MAX_EXCEPTIONS_STORED {
                    head.exceptions.push(exception);
                } else {
                    head.exceptions_truncated = true;
                }
            }
        }
    }
    saved.accepted_amount_cents = amount.to_string();
    head.next_seq += 1;
    head.submitted += saved.submitted;
    head.accepted += saved.accepted;
    head.excepted += saved.excepted;
    head.accepted_amount_cents = (money(&head.accepted_amount_cents).unwrap_or(0) + amount).to_string();
    head.chunk_sha256.push(content.into());
    Ok(saved)
}

fn accept_record(profile: &Profile, record: &Value, keys: &mut BTreeSet<String>) -> std::result::Result<AcceptedRow, (Option<String>, String)> {
    let Some(object) = record.as_object() else {
        return Err((None, "record_not_object".into()));
    };
    if object.keys().any(|key| !profile.fields.contains(&key.as_str())) {
        return Err((string_field(object.get("source_key")), "unknown_field".into()));
    }
    let source_key = match object.get("source_key").and_then(Value::as_str) {
        Some(value) if valid_token(value) => value.to_string(),
        _ => return Err((string_field(object.get("source_key")), "invalid_source_key".into())),
    };
    let kind = if profile.kinds.is_empty() {
        profile.implicit_kind.to_string()
    } else {
        match object.get("record_type").and_then(Value::as_str) {
            Some(kind) if profile.kinds.contains(&kind) => kind.to_string(),
            _ => return Err((Some(source_key), "invalid_record_type".into())),
        }
    };
    if profile.require_code && !text_ok(object.get("code"), 32) {
        return Err((Some(source_key), "invalid_code".into()));
    }
    if !text_ok(object.get("name"), 300) {
        return Err((Some(source_key), "invalid_name".into()));
    }
    let money_required = !profile.kinds.is_empty() && profile.money_kinds.contains(&kind.as_str());
    let money_forbidden = !profile.kinds.is_empty() && !profile.money_kinds.contains(&kind.as_str());
    let state = object.get("amount_state").and_then(Value::as_str).unwrap_or("");
    let amount = match state {
        "absent" if money_required => return Err((Some(source_key), "amount_required".into())),
        "cents" if money_forbidden => return Err((Some(source_key), "amount_not_allowed".into())),
        "absent" => {
            if object.get("amount_cents").is_some_and(|value| !value.is_null()) {
                return Err((Some(source_key), "absent_amount_has_value".into()));
            }
            None
        }
        "cents" => {
            let Some(cents) = object.get("amount_cents").and_then(Value::as_str) else {
                return Err((Some(source_key), "invalid_amount".into()));
            };
            if money(cents).is_err() {
                return Err((Some(source_key), "invalid_amount".into()));
            }
            Some(cents.to_string())
        }
        _ => return Err((Some(source_key), "invalid_amount_state".into())),
    };
    if profile.hash_kinds.contains(&kind.as_str()) {
        let hash = object.get("content_sha256").and_then(Value::as_str).unwrap_or("");
        if !digest(hash) {
            return Err((Some(source_key), "missing_content_hash".into()));
        }
    } else if object.get("content_sha256").is_some_and(|value| !value.is_null()) {
        return Err((Some(source_key), "unexpected_hash".into()));
    }
    if !keys.insert(source_key.clone()) {
        return Err((Some(source_key), "duplicate_source_key".into()));
    }
    Ok(AcceptedRow { source_key, kind, amount_cents: amount })
}

fn validate_open(profile: &Profile, batch: &OpenBatch) -> Result<()> {
    if batch.format != profile.format || batch.source_kind != profile.source_kind {
        return Err(blocked(profile.blocked));
    }
    if !batch.synthetic {
        return Err(blocked("real_sample_required: a non-synthetic batch needs a pinned export sample"));
    }
    if batch.filing_requested || batch.tax_cases_imported != 0 {
        return Err(blocked("filing_disabled: preservation does not import or file tax cases"));
    }
    if !valid_token(&batch.cohort_ref) || batch.expected_records == 0 || batch.expected_records > MAX_RECORDS || money(&batch.expected_accepted_amount_cents).is_err() {
        return Err(bad("Cohort, expected record count, or control amount is invalid"));
    }
    Ok(())
}

fn progress(head: &Head, replay: bool) -> Value {
    json!({
        "batch_id": head.batch_id,
        "idempotent_replay": replay,
        "checkpoint_seq": head.next_seq,
        "chunks_stored": head.chunk_sha256.len(),
        "records_submitted": head.submitted,
        "records_accepted": head.accepted,
        "records_excepted": head.excepted,
        "absent_amounts": head.absent_amounts,
        "accepted_amount_cents": head.accepted_amount_cents,
        "expected_records": head.open.expected_records,
        "expected_accepted_amount_cents": head.open.expected_accepted_amount_cents,
        "remaining_records": head.open.expected_records.saturating_sub(head.submitted),
        "complete": head.submitted == head.open.expected_records,
        "committed": head.committed_request.is_some(),
        "exceptions_truncated": head.exceptions_truncated,
        "decoder_verified": false,
        "tax_cases_imported": 0,
        "filing_enabled": false,
        "vendor_format_accepted": false,
        "real_sample_accepted": false,
        "lane": head.lane,
        "write_back": false,
        "live_cutover_allowed": false,
    })
}

fn chunk_body(head: &Head, saved: &ChunkSaved, replay: bool) -> Value {
    let mut body = progress(head, replay);
    body["chunk_submitted"] = json!(saved.submitted);
    body["chunk_accepted"] = json!(saved.accepted);
    body["chunk_excepted"] = json!(saved.excepted);
    body["chunk_exceptions"] = json!(saved.exceptions);
    body
}

fn exit_criteria(counts_match: bool, balances_match: bool) -> Value {
    json!([
        {"id":"vendor_sample_pinned","met":false,"note":"No real export sample is pinned. The rows in NXT are a declared synthetic copy."},
        {"id":"counts_match","met":counts_match,"note":"This report's legacy count equals the NXT accepted count. One match does not authorize decommissioning."},
        {"id":"balances_match","met":balances_match,"note":"This report's legacy cent total equals the NXT accepted total."},
        {"id":"exceptions_reviewed","met":false,"note":"A person has not reviewed the exception list."},
        {"id":"no_write_back","met":true,"note":"This API has no path that writes to the legacy system."},
        {"id":"filing_off","met":true,"note":"Filing stays disabled."},
        {"id":"orville_decision","met":false,"note":"Decommissioning is allowed only after Orville records that decision. This software cannot record it."}
    ])
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-' && bytes.iter().enumerate().all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn text_ok(value: Option<&Value>, max: usize) -> bool {
    value.and_then(Value::as_str).is_some_and(|text| !text.is_empty() && text.len() <= max && !text.chars().any(|c| c.is_control()))
}
fn string_field(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}
fn head_key(lane: &str, id: &str) -> String { format!("preserve:{lane}:{id}") }
fn chunk_key(lane: &str, id: &str, seq: u32) -> String { format!("preserve:{lane}:{id}:chunk:{seq}") }
fn sha(bytes: &str) -> String { hex::encode(Sha256::digest(bytes.as_bytes())) }
fn sha_json(value: &impl Serialize) -> String { sha(&serde_json::to_string(value).unwrap_or_default()) }
fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80 && value.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && value.chars().all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
}
fn digest(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn money(value: &str) -> std::result::Result<i128, ()> {
    if value.is_empty() || value.len() > 18 || !value.bytes().all(|b| b.is_ascii_digit()) || (value.len() > 1 && value.starts_with('0')) { return Err(()); }
    value.parse().map_err(|_| ())
}
fn bad(message: &str) -> ImportError { ImportError(StatusCode::BAD_REQUEST, message.into()) }
fn blocked(message: &str) -> ImportError { ImportError(StatusCode::CONFLICT, message.into()) }
fn conflict(message: &str) -> ImportError { ImportError(StatusCode::CONFLICT, message.into()) }
fn missing(message: &str) -> ImportError { ImportError(StatusCode::NOT_FOUND, message.into()) }
fn storage(message: &str) -> ImportError { ImportError(StatusCode::SERVICE_UNAVAILABLE, message.to_string()) }
