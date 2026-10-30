//! Real DT Max client roster and return-year index, for the local NXT OS test copy only.
//!
//! Every request must carry `"test_copy": true`, and the server refuses unless its store
//! is the local memory/dev-file backend. Hosted (Supabase/Redis) backends, which back
//! production, always refuse. Clients are keyed by the DT Max `Client #`. Return files are
//! indexed by client number and tax year from the file's own `DTMAX\0YY` header; their
//! section payloads are vendor-encoded and are not decoded or stored here.
//! SIN is never accepted. Responses carry counts and reasons, never client fields.

use super::{ImportError, bad, conflict, storage, valid_token};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, ImportError>;
const MAX_BATCH: usize = 1_000;
const TEXT_FIELDS: &[(&str, usize)] = &[
    ("name", 200), ("processing_status", 80), ("email", 200), ("home_phone", 40), ("work_phone", 40),
    ("cell_phone", 40), ("street", 200), ("city", 100), ("postal_code", 16), ("province", 40), ("country", 60),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientBatch {
    pub test_copy: bool,
    pub clients: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnBatch {
    pub test_copy: bool,
    pub returns: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlipBatch {
    pub test_copy: bool,
    pub slips: Vec<Value>,
}

/// One T5008 row from a DT Max auto-fill (`afr-prd`) CSV. Box values are kept verbatim as text.
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct T5008Slip {
    client_number: String,
    recipient_name: String,
    tax_year: u16,
    source_file_sha256: String,
    row: u32,
    boxes: BTreeMap<String, String>,
}

const T5008_BOXES: &[&str] = &["11", "13", "15", "16", "17", "18", "19", "20", "21", "22", "23", "24"];

#[derive(Serialize, Deserialize, PartialEq)]
struct ReturnIndex {
    client_number: String,
    tax_year: u16,
    extension: String,
    size_bytes: u64,
    sha256: String,
    header_year: u16,
    sections: u32,
    last_saved_epoch: Option<u64>,
    content_state: String,
}

fn k(company: &str, rest: &str) -> String { format!("dtmaxtc:{company}:{rest}") }
fn s(e: &str) -> ImportError { storage(e) }
fn client_number_ok(v: &str) -> bool { !v.is_empty() && v.len() <= 12 && v.bytes().all(|b| b.is_ascii_digit()) }
fn reject(out: &mut BTreeMap<String, u64>, reason: &str) { *out.entry(reason.into()).or_default() += 1; }

impl super::SourceImport {
    fn test_copy_guard(&self, company: &str, flag: bool) -> Result<()> {
        if !valid_token(company) { return Err(bad("Invalid company id")); }
        if !flag { return Err(conflict("test_copy_flag_required: real DT Max data loads only with \"test_copy\": true")); }
        if self.store.backend_name() != "memory" {
            return Err(conflict("test_copy_only: real DT Max data loads only into a local test-copy store, never a hosted or production backend"));
        }
        Ok(())
    }

    /// Upsert clients by `client_number`. Rows are validated one by one; a bad row is
    /// counted by reason and the rest of the batch still loads.
    pub async fn test_copy_clients(&self, company: &str, batch: ClientBatch) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.clients.len() > MAX_BATCH { return Err(bad("At most 1,000 clients per batch")); }
        let (mut created, mut updated, mut unchanged) = (0u64, 0u64, 0u64);
        let mut rejected = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for row in &batch.clients {
            let Some(obj) = row.as_object() else { reject(&mut rejected, "not_an_object"); continue };
            match clean_client(obj) {
                Err(reason) => reject(&mut rejected, reason),
                Ok((number, record)) => {
                    if !seen.insert(number.clone()) { reject(&mut rejected, "duplicate_in_batch"); continue; }
                    let key = k(company, &format!("client:{number}"));
                    let body = record.to_string();
                    match self.store.get(&key).await.map_err(|e| s(&e))? {
                        Some(old) if old == body => unchanged += 1,
                        Some(_) => { self.store.set(&key, &body).await.map_err(|e| s(&e))?; updated += 1 }
                        None => { self.store.set(&key, &body).await.map_err(|e| s(&e))?; created += 1 }
                    }
                    self.store.sadd(&k(company, "clients"), &number).await.map_err(|e| s(&e))?;
                }
            }
        }
        Ok(json!({"company_id": company, "test_copy": true, "submitted": batch.clients.len(),
            "created": created, "updated": updated, "unchanged": unchanged, "rejected": rejected}))
    }

    /// Index return files by client number and tax year. A return whose client is not in
    /// the roster is kept and counted as unlinked.
    pub async fn test_copy_returns(&self, company: &str, batch: ReturnBatch) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.returns.len() > MAX_BATCH { return Err(bad("At most 1,000 returns per batch")); }
        let clients: BTreeSet<String> = self.store.smembers(&k(company, "clients")).await.map_err(|e| s(&e))?.into_iter().collect();
        let (mut linked, mut unlinked, mut unchanged) = (0u64, 0u64, 0u64);
        let mut rejected = BTreeMap::new();
        for row in &batch.returns {
            let parsed: ReturnIndex = match serde_json::from_value(row.clone()) { Ok(r) => r, Err(_) => { reject(&mut rejected, "invalid_fields"); continue } };
            if let Err(reason) = check_return(&parsed) { reject(&mut rejected, reason); continue; }
            let member = format!("{}:{}", parsed.client_number, parsed.tax_year);
            let key = k(company, &format!("return:{member}"));
            let body = serde_json::to_string(&parsed).map_err(|_| s("encode failed"))?;
            match self.store.get(&key).await.map_err(|e| s(&e))? {
                Some(old) if old == body => unchanged += 1,
                Some(_) => { reject(&mut rejected, "conflicting_return_for_client_year"); continue }
                None => self.store.set(&key, &body).await.map_err(|e| s(&e))?,
            }
            self.store.sadd(&k(company, "returns"), &member).await.map_err(|e| s(&e))?;
            if clients.contains(&parsed.client_number) { linked += 1 } else { unlinked += 1 }
        }
        Ok(json!({"company_id": company, "test_copy": true, "submitted": batch.returns.len(),
            "linked": linked, "unlinked": unlinked, "unchanged_replays": unchanged, "rejected": rejected,
            "content_decoded": false}))
    }

    /// Store T5008 slip rows. Identity is source-file SHA-256 plus row number, so a replay
    /// is `unchanged_replays`. Client number `0` (DT Max "no client") is kept as unlinked.
    pub async fn test_copy_slips(&self, company: &str, batch: SlipBatch) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.slips.len() > MAX_BATCH { return Err(bad("At most 1,000 slips per batch")); }
        let clients: BTreeSet<String> = self.store.smembers(&k(company, "clients")).await.map_err(|e| s(&e))?.into_iter().collect();
        let (mut linked, mut unlinked, mut unchanged) = (0u64, 0u64, 0u64);
        let mut rejected = BTreeMap::new();
        for row in &batch.slips {
            let parsed: T5008Slip = match serde_json::from_value(row.clone()) { Ok(r) => r, Err(_) => { reject(&mut rejected, "invalid_fields"); continue } };
            if let Err(reason) = check_slip(&parsed) { reject(&mut rejected, reason); continue; }
            let member = format!("{}:{}:{}:{}", parsed.client_number, parsed.tax_year, parsed.source_file_sha256, parsed.row);
            let key = k(company, &format!("t5008:{member}"));
            let body = serde_json::to_string(&parsed).map_err(|_| s("encode failed"))?;
            match self.store.get(&key).await.map_err(|e| s(&e))? {
                Some(old) if old == body => unchanged += 1,
                Some(_) => { reject(&mut rejected, "conflicting_slip_row"); continue }
                None => self.store.set(&key, &body).await.map_err(|e| s(&e))?,
            }
            self.store.sadd(&k(company, "t5008"), &member).await.map_err(|e| s(&e))?;
            if clients.contains(&parsed.client_number) { linked += 1 } else { unlinked += 1 }
        }
        Ok(json!({"company_id": company, "test_copy": true, "submitted": batch.slips.len(),
            "linked": linked, "unlinked": unlinked, "unchanged_replays": unchanged, "rejected": rejected}))
    }

    /// Counts only: roster size, contact coverage, and returns by tax year.
    pub async fn test_copy_summary(&self, company: &str) -> Result<Value> {
        if !valid_token(company) { return Err(bad("Invalid company id")); }
        let numbers = self.store.smembers(&k(company, "clients")).await.map_err(|e| s(&e))?;
        let set: BTreeSet<&str> = numbers.iter().map(String::as_str).collect();
        let (mut with_address, mut with_email, mut with_phone) = (0u64, 0u64, 0u64);
        for chunk in numbers.chunks(500) {
            let keys: Vec<String> = chunk.iter().map(|n| k(company, &format!("client:{n}"))).collect();
            for v in self.store.mget(&keys).await.map_err(|e| s(&e))?.into_iter().flatten() {
                let v: Value = serde_json::from_str(&v).unwrap_or(Value::Null);
                if v.get("street").is_some() || v.get("city").is_some() { with_address += 1 }
                if v.get("email").is_some() { with_email += 1 }
                if ["home_phone", "work_phone", "cell_phone"].iter().any(|f| v.get(*f).is_some()) { with_phone += 1 }
            }
        }
        let mut by_year: BTreeMap<String, Value> = BTreeMap::new();
        let mut linked_clients = BTreeSet::new();
        let (mut total, mut unlinked) = (0u64, 0u64);
        for m in self.store.smembers(&k(company, "returns")).await.map_err(|e| s(&e))? {
            let Some((num, year)) = m.split_once(':') else { continue };
            total += 1;
            let is_linked = set.contains(num);
            if is_linked { linked_clients.insert(num.to_string()); } else { unlinked += 1 }
            let e = by_year.entry(year.to_string()).or_insert_with(|| json!({"linked": 0, "unlinked": 0}));
            let f = if is_linked { "linked" } else { "unlinked" };
            e[f] = json!(e[f].as_u64().unwrap_or(0) + 1);
        }
        let mut slips_by_year: BTreeMap<String, u64> = BTreeMap::new();
        let (mut slips, mut slips_unlinked) = (0u64, 0u64);
        for m in self.store.smembers(&k(company, "t5008")).await.map_err(|e| s(&e))? {
            let mut parts = m.splitn(3, ':');
            let (Some(num), Some(year)) = (parts.next(), parts.next()) else { continue };
            slips += 1;
            if !set.contains(num) { slips_unlinked += 1 }
            *slips_by_year.entry(year.to_string()).or_default() += 1;
        }
        Ok(json!({"company_id": company, "test_copy": true, "clients": numbers.len(),
            "clients_with_address": with_address, "clients_with_email": with_email, "clients_with_phone": with_phone,
            "returns": total, "returns_unlinked": unlinked, "clients_with_returns": linked_clients.len(),
            "returns_by_year": by_year, "content_decoded": false,
            "t5008_slips": slips, "t5008_slips_unlinked": slips_unlinked, "t5008_by_year": slips_by_year}))
    }
}

fn clean_client(obj: &Map<String, Value>) -> std::result::Result<(String, Value), &'static str> {
    let mut out = Map::new();
    for (field, value) in obj {
        if field.eq_ignore_ascii_case("sin") { return Err("sin_not_accepted"); }
        if field == "client_number" || field == "source_lists" { continue; }
        let Some((_, max)) = TEXT_FIELDS.iter().find(|(f, _)| f == field) else { return Err("unknown_field") };
        if value.is_null() { continue; }
        let Some(text) = value.as_str() else { return Err("field_not_text") };
        let text = text.trim();
        if text.chars().count() > *max { return Err("field_too_long"); }
        if !text.is_empty() { out.insert(field.clone(), json!(text)); }
    }
    let number = obj.get("client_number").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if !client_number_ok(number) { return Err("invalid_client_number"); }
    if !out.contains_key("name") { return Err("missing_name"); }
    if let Some(list) = obj.get("source_lists") {
        let list = list.as_array().ok_or("invalid_source_lists")?;
        if list.len() > 8 || !list.iter().all(|v| v.as_str().is_some_and(valid_token)) { return Err("invalid_source_lists"); }
        out.insert("source_lists".into(), Value::Array(list.clone()));
    }
    out.insert("client_number".into(), json!(number));
    Ok((number.to_string(), Value::Object(out)))
}

fn check_return(r: &ReturnIndex) -> std::result::Result<(), &'static str> {
    if !client_number_ok(&r.client_number) { return Err("invalid_client_number"); }
    if !(1990..=2035).contains(&r.tax_year) { return Err("tax_year_out_of_range"); }
    if r.extension != format!("P{:02}", r.tax_year % 100) { return Err("extension_does_not_match_year"); }
    if r.header_year != r.tax_year { return Err("header_year_does_not_match"); }
    if r.sha256.len() != 64 || !r.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return Err("invalid_sha256"); }
    if r.size_bytes == 0 || r.size_bytes > 64 * 1024 * 1024 { return Err("invalid_size"); }
    if !matches!(r.content_state.as_str(), "vendor_encoded_not_decoded") { return Err("invalid_content_state"); }
    Ok(())
}

fn check_slip(r: &T5008Slip) -> std::result::Result<(), &'static str> {
    if !client_number_ok(&r.client_number) { return Err("invalid_client_number"); }
    if !(1990..=2035).contains(&r.tax_year) { return Err("tax_year_out_of_range"); }
    let name = r.recipient_name.trim();
    if name.is_empty() || name.chars().count() > 200 { return Err("invalid_recipient_name"); }
    if r.source_file_sha256.len() != 64 || !r.source_file_sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return Err("invalid_sha256"); }
    if r.row == 0 || r.row > 100_000 { return Err("invalid_row"); }
    if r.boxes.is_empty() { return Err("empty_slip"); }
    for (b, v) in &r.boxes {
        if !T5008_BOXES.contains(&b.as_str()) { return Err("unknown_box"); }
        if v.chars().count() > 200 { return Err("field_too_long"); }
    }
    Ok(())
}
