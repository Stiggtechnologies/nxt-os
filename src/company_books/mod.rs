//! Company-isolated, source-bound journal books. One CAS publishes the book and receipt.
mod adjustments;
pub mod models;
mod roster;
mod routes;
#[cfg(test)]
mod tests;
use crate::{referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{NaiveDate, Utc};
use models::*;
pub use routes::router;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
type Result<T> = std::result::Result<T, ApiError>;
const MAX_CHUNK: usize = 512 * 1024;
const MAX_BOOK: usize = 24 * 1024 * 1024;
const INDEX: &str = "company-books:companies:v1";
pub const DEFAULT_PAGE_BYTES: usize = 768 * 1024;
fn page_rows(
    rows: Vec<Value>,
    offset: usize,
    limit: usize,
    max_bytes: usize,
) -> Result<(Vec<Value>, usize, Option<usize>, usize)> {
    if limit == 0 || limit > 200 || !(32 * 1024..=1024 * 1024).contains(&max_bytes) {
        return Err(bad("Page limit must be 1-200 and max_bytes 32768-1048576"));
    }
    let total = rows.len();
    let mut page = vec![];
    let mut bytes = 2usize;
    for row in rows.into_iter().skip(offset).take(limit) {
        let next = encoded(&row)?.len() + usize::from(!page.is_empty());
        if bytes.saturating_add(next) > max_bytes {
            if page.is_empty() {
                return Err(err(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "One row exceeds requested page byte budget; retry max_bytes=1048576 without advancing offset",
                ));
            }
            break;
        }
        bytes += next;
        page.push(row);
    }
    let next = offset.saturating_add(page.len());
    let continuation = (next < total).then_some(next);
    Ok((page, total, continuation, bytes))
}
pub struct CompanyBooks {
    pub store: Store,
    #[cfg(test)]
    source_loads: std::sync::atomic::AtomicUsize,
}
struct BookSnapshot {
    head_id: String,
    head: Head,
    preview: Preview,
    accounts: Vec<Account>,
    source_entries: Vec<Entry>,
    native_entries: Vec<Entry>,
}
fn err(code: StatusCode, message: &str) -> ApiError {
    ApiError(code, message.into())
}
fn bad(message: &str) -> ApiError {
    err(StatusCode::BAD_REQUEST, message)
}
fn corrupt() -> ApiError {
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Company book is incomplete or corrupt; no partial book was returned",
    )
}
fn storage(_: String) -> ApiError {
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "Company book storage unavailable",
    )
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn encoded<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|_| bad("Object cannot be encoded"))
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_- .".contains(&b))
        && !s.contains(' ')
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text(s: &str, max: usize) -> bool {
    s.len() <= max && !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
}
fn date(s: &str) -> bool {
    s.len() == 10 && NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}
fn money(s: &str) -> Result<i128> {
    if s.is_empty()
        || s.len() > 16
        || !s.bytes().all(|b| b.is_ascii_digit())
        || (s.len() > 1 && s.starts_with('0'))
    {
        return Err(bad(
            "Money must be a canonical nonnegative exact cents string",
        ));
    }
    let n = s.parse::<i128>().map_err(|_| bad("Invalid money"))?;
    if n > 9_007_199_254_740_991 {
        return Err(bad("Line money exceeds exact input bound"));
    }
    Ok(n)
}
fn aggregate(s: &str) -> Result<i128> {
    if s.is_empty()
        || s.len() > 39
        || !s.bytes().all(|b| b.is_ascii_digit())
        || (s.len() > 1 && s.starts_with('0'))
    {
        return Err(bad(
            "Aggregate proof must use canonical nonnegative exact cents",
        ));
    }
    s.parse::<i128>()
        .map_err(|_| bad("Aggregate proof exceeds checked integer bound"))
}
fn metadata(value: &Option<Value>) -> Result<()> {
    fn walk(v: &Value, depth: usize, nodes: &mut usize) -> bool {
        *nodes += 1;
        if depth > 8 || *nodes > 1000 {
            return false;
        }
        match v {
            Value::Object(o) => o
                .iter()
                .all(|(k, v)| k.len() <= 200 && walk(v, depth + 1, nodes)),
            Value::Array(a) => a.iter().all(|v| walk(v, depth + 1, nodes)),
            Value::String(s) => s.len() <= 8192,
            _ => true,
        }
    }
    if let Some(v) = value {
        let mut nodes = 0;
        if !v.is_object() || encoded(v)?.len() > 16 * 1024 || !walk(v, 0, &mut nodes) {
            return Err(bad(
                "Source metadata must be a bounded JSON object (16 KiB, depth 8)",
            ));
        }
    }
    Ok(())
}
fn controls() -> Value {
    json!({"source_authoritative":true,"parallel_test":true,"live_cutover_allowed":false,"identity_limit":"Source identity, currency and reviewer labels are operator assertions under a shared workspace key; they are not independently authenticated identities.","scope":"Complete submitted journal and trial balance only; attachments, bank reconciliation, subledger allocations, payroll and tax filing are not inferred."})
}
fn preview_value(id: &str, p: &Preview) -> Value {
    json!({"preview_id":id,"stage":if p.analysis.controls_passed {"preview_ready"} else {"assessment_blocked"},"preview":p,"controls":controls()})
}
impl CompanyBooks {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            #[cfg(test)]
            source_loads: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    fn key(company: &str, kind: &str, id: &str) -> Result<String> {
        if !valid_id(company) {
            return Err(bad("Invalid company id"));
        }
        Ok(format!("company-books:{company}:{kind}:{id}"))
    }
    async fn immutable<T: Serialize>(&self, key: &str, value: &T) -> Result<String> {
        let raw = encoded(value)?;
        self.store.set_nx(key, &raw).await.map_err(storage)?;
        if self.store.get(key).await.map_err(storage)?.as_deref() != Some(raw.as_str()) {
            return Err(corrupt());
        }
        Ok(raw)
    }
    async fn object<T: DeserializeOwned + Serialize>(&self, key: &str, id: &str) -> Result<T> {
        if !digest(id) {
            return Err(bad("Invalid object hash"));
        }
        let raw = self
            .store
            .get(key)
            .await
            .map_err(storage)?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Company book object not found"))?;
        if hash(raw.as_bytes()) != id {
            return Err(corrupt());
        }
        let obj: T = serde_json::from_str(&raw).map_err(|_| corrupt())?;
        if encoded(&obj)? != raw {
            return Err(corrupt());
        }
        Ok(obj)
    }
    pub async fn upload(&self, company: &str, chunk: Chunk) -> Result<Value> {
        let raw = encoded(&chunk)?;
        if raw.len() > MAX_CHUNK || (chunk.accounts.is_empty() && chunk.entries.is_empty()) {
            return Err(bad("Chunk must be nonempty and at most 512 KiB"));
        }
        let id = hash(raw.as_bytes());
        let key = Self::key(company, "chunk", &id)?;
        self.immutable(&key, &chunk).await?;
        Ok(json!({"sha256":id,"bytes":raw.len()}))
    }
    async fn data(&self, company: &str, chunks: &[String]) -> Result<(Vec<Account>, Vec<Entry>)> {
        #[cfg(test)]
        self.source_loads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if chunks.is_empty() || chunks.len() > 256 {
            return Err(bad("Book requires 1-256 chunks"));
        }
        let mut seen = HashSet::new();
        let (mut accounts, mut entries, mut size) = (vec![], vec![], 0usize);
        for id in chunks {
            if !digest(id) || !seen.insert(id) {
                return Err(bad("Duplicate chunk reference"));
            }
        }
        // Keep escaped RPC responses comfortably below the store's response bound.
        for batch in chunks.chunks(8) {
            let keys = batch
                .iter()
                .map(|id| Self::key(company, "chunk", id))
                .collect::<Result<Vec<_>>>()?;
            let values = self.store.mget(&keys).await.map_err(storage)?;
            if values.len() != batch.len() {
                return Err(corrupt());
            }
            for (id, raw) in batch.iter().zip(values) {
                let raw =
                    raw.ok_or_else(|| err(StatusCode::NOT_FOUND, "Company book chunk not found"))?;
                if hash(raw.as_bytes()) != *id {
                    return Err(corrupt());
                }
                let c: Chunk = serde_json::from_str(&raw).map_err(|_| corrupt())?;
                if encoded(&c)? != raw {
                    return Err(corrupt());
                }
                let len = raw.len();
                if len > MAX_CHUNK {
                    return Err(corrupt());
                }
                size = size.checked_add(len).ok_or_else(corrupt)?;
                if size > MAX_BOOK {
                    return Err(bad("Book exceeds 24 MiB bounded import limit"));
                }
                accounts.extend(c.accounts);
                entries.extend(c.entries);
                if accounts.len() > 20_000 || entries.len() > 100_000 {
                    return Err(bad("Book exceeds account or journal count limit"));
                }
            }
        }
        Ok((accounts, entries))
    }
    fn analyze(input: &PreviewInput, accounts: &[Account], entries: &[Entry]) -> Result<Analysis> {
        if !valid_id(&input.request_id)
            || input.expected_head.as_deref().is_some_and(|s| !digest(s))
        {
            return Err(bad("Invalid request id or expected head"));
        }
        let c = &input.company;
        let s = &input.source;
        if c.name.trim().is_empty()
            || !text(&c.name, 300)
            || !text(&c.source_company_id, 200)
            || !text(&c.currency, 3)
            || !matches!(s.system.as_str(), "quickbooks_online" | "synthetic")
            || ![
                &s.archive_sha256,
                &s.journal_sha256,
                &s.trial_balance_sha256,
            ]
            .iter()
            .all(|s| digest(s))
            || !date(&s.period_start)
            || !date(&s.period_end)
            || s.period_start > s.period_end
            || !matches!(
                s.record_id_coverage.as_str(),
                "archive_row" | "durable_source_id"
            )
        {
            return Err(bad("Invalid company/source provenance"));
        }
        let mut blockers = vec![];
        if !c.identity_verified || c.source_company_id.trim().is_empty() {
            blockers.push("source_identity_unverified".into());
        }
        if !c.currency_verified
            || c.currency.len() != 3
            || !c.currency.bytes().all(|b| b.is_ascii_uppercase())
        {
            blockers.push("source_currency_unverified".into());
        }
        if !s.full_history {
            blockers.push("source_history_incomplete".into());
        }
        match &s.period_lock {
            Some(lock)
                if lock.verified
                    && digest(&lock.evidence_sha256)
                    && lock.closed_through.as_deref().is_none_or(date) => {}
            _ => blockers.push("source_period_lock_unverified".into()),
        }
        let mut totals: BTreeMap<String, (&Account, i128, i128)> = BTreeMap::new();
        let mut account_rows: BTreeMap<String, u32> = BTreeMap::new();
        for a in accounts {
            if !valid_id(&a.id)
                || a.name.trim().is_empty()
                || !text(&a.name, 300)
                || a.kind.as_deref().is_some_and(|k| {
                    !matches!(k, "asset" | "liability" | "equity" | "income" | "expense")
                })
                || totals.insert(a.id.clone(), (a, 0, 0)).is_some()
            {
                return Err(bad("Invalid or duplicate account"));
            }
        }
        if accounts.is_empty() || entries.is_empty() {
            blockers.push("source_journal_empty".into());
        }
        let (mut ids, mut source_ids, mut rows) = (HashSet::new(), HashSet::new(), HashSet::new());
        let (mut td, mut tc, mut count) = (0i128, 0i128, 0usize);
        for e in entries {
            metadata(&e.source_metadata)?;
            if !valid_id(&e.id)
                || !ids.insert(&e.id)
                || e.source_id.trim().is_empty()
                || !text(&e.source_id, 300)
                || !source_ids.insert(&e.source_id)
                || !date(&e.date)
                || e.date < s.period_start
                || e.date > s.period_end
                || !text(&e.memo, 2000)
                || e.lines.is_empty()
                || e.lines.len() > 20_000
            {
                return Err(bad("Invalid, duplicate or out-of-period journal entry"));
            }
            let (mut d, mut cr) = (0i128, 0i128);
            for l in &e.lines {
                metadata(&l.source_metadata)?;
                count += 1;
                if count > 500_000
                    || l.source_row == 0
                    || !rows.insert(l.source_row)
                    || !text(&l.memo, 2000)
                {
                    return Err(bad("Invalid or duplicated source journal row"));
                }
                let ld = money(&l.debit_cents)?;
                let lc = money(&l.credit_cents)?;
                if ld > 0 && lc > 0 {
                    return Err(bad("Journal line has debit and credit simultaneously"));
                }
                let t = totals
                    .get_mut(&l.account_id)
                    .ok_or_else(|| bad("Journal references an undefined account"))?;
                t.1 = t.1.checked_add(ld).ok_or_else(|| bad("Amount overflow"))?;
                t.2 = t.2.checked_add(lc).ok_or_else(|| bad("Amount overflow"))?;
                *account_rows.entry(l.account_id.clone()).or_default() += 1;
                d = d.checked_add(ld).ok_or_else(|| bad("Amount overflow"))?;
                cr = cr.checked_add(lc).ok_or_else(|| bad("Amount overflow"))?;
            }
            if d != cr {
                blockers.push(format!("journal_unbalanced:{}", e.id));
            }
            td = td.checked_add(d).ok_or_else(|| bad("Amount overflow"))?;
            tc = tc.checked_add(cr).ok_or_else(|| bad("Amount overflow"))?;
        }
        let mut checks = BTreeMap::new();
        for t in &input.trial_balance {
            if !valid_id(&t.account_id)
                || checks
                    .insert(
                        t.account_id.clone(),
                        (
                            money(&t.debit_cents)?,
                            money(&t.credit_cents)?,
                            t.basis.as_ref(),
                        ),
                    )
                    .is_some()
            {
                return Err(bad("Duplicate or invalid trial balance account"));
            }
            if money(&t.debit_cents)? > 0 && money(&t.credit_cents)? > 0 {
                return Err(bad("Trial balance account has both debit and credit"));
            }
        }
        if checks.keys().any(|id| !totals.contains_key(id)) {
            return Err(bad("Trial balance references undefined account"));
        }
        let mut balances = vec![];
        let (mut independent, mut derived, mut provenance_complete) = (0usize, 0usize, true);
        let mut source_tb_rows = HashSet::new();
        for (id, (a, d, c)) in totals {
            let net = d - c;
            let (bd, bc) = (net.max(0), (-net).max(0));
            match checks.get(&id) {
                None => {
                    blockers.push(format!("trial_balance_control_missing:{id}"));
                    provenance_complete = false;
                }
                Some((sd, sc, basis)) => {
                    let matches = (*sd, *sc) == (bd, bc);
                    if !matches {
                        blockers.push(format!("trial_balance_mismatch:{id}"));
                    }
                    match basis {
                        None => {
                            blockers.push(format!("trial_balance_control_provenance_missing:{id}"));
                            provenance_complete = false;
                        }
                        Some(ControlBasis::SourceReportRow { report_sha256, row }) => {
                            if *row == 0
                                || *report_sha256 != s.trial_balance_sha256
                                || !source_tb_rows.insert(*row)
                            {
                                blockers.push(format!("trial_balance_row_provenance_invalid:{id}"));
                                provenance_complete = false;
                            } else if matches {
                                independent += 1;
                            }
                        }
                        Some(ControlBasis::DerivedZeroJournalOmission {
                            journal_sha256,
                            source_row_count,
                            journal_debit_cents,
                            journal_credit_cents,
                        }) => {
                            if *journal_sha256 != s.journal_sha256
                                || *source_row_count == 0
                                || *source_row_count != account_rows.get(&id).copied().unwrap_or(0)
                                || aggregate(journal_debit_cents)? != d
                                || aggregate(journal_credit_cents)? != c
                                || d != c
                                || *sd != 0
                                || *sc != 0
                            {
                                blockers.push(format!("derived_zero_proof_mismatch:{id}"));
                                provenance_complete = false;
                            } else if matches {
                                derived += 1;
                            }
                        }
                    }
                }
            }
            balances.push(Balance {
                account_id: id.clone(),
                name: a.name.clone(),
                kind: a.kind.clone(),
                debit_cents: bd.to_string(),
                credit_cents: bc.to_string(),
                journal_debit_cents: d.to_string(),
                journal_credit_cents: c.to_string(),
                source_control_basis: checks.get(&id).and_then(|(_, _, b)| b.map(Clone::clone)),
            });
        }
        blockers.sort();
        blockers.dedup();
        Ok(Analysis {
            accounts: accounts.len(),
            entries: entries.len(),
            lines: count,
            total_debit_cents: td.to_string(),
            total_credit_cents: tc.to_string(),
            classified_reports_ready: accounts.iter().all(|a| a.kind.is_some()),
            controls_passed: blockers.is_empty(),
            independent_trial_balance_accounts: independent,
            derived_zero_accounts: derived,
            control_provenance_complete: provenance_complete,
            chart_of_accounts_complete: false,
            balances,
            blockers,
        })
    }
    pub async fn preview(&self, company: &str, input: PreviewInput) -> Result<Value> {
        Self::key(company, "preview", "")?;
        let (a, e) = self.data(company, &input.chunks).await?;
        let analysis = Self::analyze(&input, &a, &e)?;
        let p = Preview {
            format: "nxt-company-book-preview/v1".into(),
            company_id: company.into(),
            input,
            analysis,
        };
        let raw = encoded(&p)?;
        if raw.len() > 4 * 1024 * 1024 {
            return Err(bad("Preview exceeds bounded metadata limit"));
        }
        let id = hash(raw.as_bytes());
        self.immutable(&Self::key(company, "preview", &id)?, &p)
            .await?;
        let request_key = Self::key(company, "preview-request", &p.input.request_id)?;
        self.store
            .set_nx(&request_key, &id)
            .await
            .map_err(storage)?;
        if self
            .store
            .get(&request_key)
            .await
            .map_err(storage)?
            .as_deref()
            != Some(id.as_str())
        {
            return Err(err(
                StatusCode::CONFLICT,
                "Preview request id was already used for different content",
            ));
        }
        self.stage_preview_references(company, &id).await?;
        // Publish global discovery last: all referenced local metadata exists.
        self.store.sadd(INDEX, company).await.map_err(storage)?;
        Ok(preview_value(&id, &p))
    }
    async fn load_preview(&self, company: &str, id: &str) -> Result<Preview> {
        let p = self.load_preview_metadata(company, id).await?;
        let (a, e) = self.data(company, &p.input.chunks).await?;
        if Self::analyze(&p.input, &a, &e)? != p.analysis {
            return Err(corrupt());
        }
        Ok(p)
    }
    async fn load_preview_metadata(&self, company: &str, id: &str) -> Result<Preview> {
        let p: Preview = self.object(&Self::key(company, "preview", id)?, id).await?;
        if p.company_id != company || p.format != "nxt-company-book-preview/v1" {
            return Err(corrupt());
        }
        Ok(p)
    }
    async fn preview_catalog(&self, company: &str) -> Result<(Option<String>, Vec<String>)> {
        let raw = self
            .store
            .get(&Self::key(company, "preview-catalog", "v1")?)
            .await
            .map_err(storage)?;
        let mut ids: Vec<String> = if let Some(raw) = &raw {
            if raw.len() > 1024 * 1024 {
                return Err(corrupt());
            }
            serde_json::from_str(raw).map_err(|_| corrupt())?
        } else {
            self.store
                .smembers(&Self::key(company, "previews", "v1")?)
                .await
                .map_err(storage)?
        };
        if ids.len() > 10_000 || ids.iter().any(|id| !digest(id)) {
            return Err(corrupt());
        }
        ids.sort();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(corrupt());
        }
        Ok((raw, ids))
    }
    async fn stage_preview_references(&self, company: &str, id: &str) -> Result<()> {
        let key = Self::key(company, "preview-catalog", "v1")?;
        let mut registered = false;
        for _ in 0..16 {
            let (raw, mut ids) = self.preview_catalog(company).await?;
            if ids.iter().any(|v| v == id) {
                registered = true;
                break;
            }
            if ids.len() >= 500 {
                return Err(err(
                    StatusCode::CONFLICT,
                    "Company preview creation limit reached; existing history remains readable",
                ));
            }
            ids.push(id.into());
            ids.sort();
            let next = encoded(&ids)?;
            if self
                .store
                .compare_exchange(&key, raw.as_deref(), &next)
                .await
                .map_err(storage)?
            {
                registered = true;
                break;
            }
        }
        if !registered {
            return Err(err(
                StatusCode::CONFLICT,
                "Preview catalog changed concurrently; retry exact request",
            ));
        }
        self.store
            .sadd(&Self::key(company, "previews", "v1")?, id)
            .await
            .map_err(storage)?;
        self.store
            .set(&Self::key(company, "latest-preview", "v1")?, id)
            .await
            .map_err(storage)?;
        Ok(())
    }
    pub async fn preview_detail(&self, company: &str, id: &str) -> Result<Value> {
        let p = self.load_preview(company, id).await?;
        Ok(preview_value(id, &p))
    }
    async fn current(&self, company: &str) -> Result<Option<(String, Head)>> {
        Ok(self.snapshot(company).await?.map(|s| (s.head_id, s.head)))
    }
    async fn snapshot(&self, company: &str) -> Result<Option<BookSnapshot>> {
        let key = Self::key(company, "head", "v1")?;
        let Some(raw) = self.store.get(&key).await.map_err(storage)? else {
            return Ok(None);
        };
        if !digest(&raw) {
            return Err(corrupt());
        }
        let h: Head = self
            .object(&Self::key(company, "receipt", &raw)?, &raw)
            .await
            .map_err(|e| {
                if e.0 == StatusCode::SERVICE_UNAVAILABLE {
                    e
                } else {
                    corrupt()
                }
            })?;
        if !matches!(
            h.format.as_str(),
            "nxt-company-book-head/v1" | "nxt-company-book-head/v2"
        ) || h.company_id != company
            || h.revision == 0
            || h.revision > 129
        {
            return Err(corrupt());
        }
        let p = self
            .load_preview_metadata(company, &h.preview_id)
            .await
            .map_err(|e| {
                if e.0 == StatusCode::SERVICE_UNAVAILABLE {
                    e
                } else {
                    corrupt()
                }
            })?;
        if !p.analysis.controls_passed
            || !valid_id(&h.commit_request_id)
            || h.reviewer_label.trim().is_empty()
            || chrono::DateTime::parse_from_rfc3339(&h.committed_at).is_err()
        {
            return Err(corrupt());
        }
        let (accounts, source_entries) =
            self.data(company, &p.input.chunks).await.map_err(|e| {
                if e.0 == StatusCode::SERVICE_UNAVAILABLE {
                    e
                } else {
                    corrupt()
                }
            })?;
        if Self::analyze(&p.input, &accounts, &source_entries).map_err(|_| corrupt())? != p.analysis
        {
            return Err(corrupt());
        }
        let native_entries = self
            .validate_chain_data(company, &raw, &h, &p, &accounts, &source_entries)
            .await?;
        Ok(Some(BookSnapshot {
            head_id: raw,
            head: h,
            preview: p,
            accounts,
            source_entries,
            native_entries,
        }))
    }
    pub async fn commit(&self, company: &str, input: CommitInput) -> Result<Value> {
        if !valid_id(&input.request_id)
            || input.reviewer_label.trim().is_empty()
            || !text(&input.reviewer_label, 200)
        {
            return Err(bad("Request id and reviewer label are required"));
        }
        let p = self.load_preview(company, &input.preview_id).await?;
        if p.input.expected_head != input.expected_head {
            return Err(err(
                StatusCode::CONFLICT,
                "Commit expected head differs from immutable preview",
            ));
        }
        if !p.analysis.controls_passed {
            return Err(err(
                StatusCode::CONFLICT,
                "Unresolved source identity, currency, completeness or reconciliation blockers",
            ));
        }
        // These are discovery hints only. Complete them before head publication;
        // an interrupted request is repaired by exact retry and never posts twice.
        self.stage_preview_references(company, &input.preview_id)
            .await?;
        self.store.sadd(INDEX, company).await.map_err(storage)?;
        let key = Self::key(company, "head", "v1")?;
        if let Some((raw, h)) = self.current(company).await? {
            for (rid, receipt) in self.head_chain(company, &raw, &h).await? {
                if receipt.commit_request_id == input.request_id {
                    if receipt.operation.is_none()
                        && receipt.preview_id == input.preview_id
                        && receipt.previous_head == input.expected_head
                        && receipt.reviewer_label == input.reviewer_label
                    {
                        return Ok(
                            json!({"head_sha256":rid,"head":receipt,"current_head_sha256":raw,"idempotent_replay":true,"controls":controls()}),
                        );
                    }
                    return Err(err(
                        StatusCode::CONFLICT,
                        "Request id already used for different command content",
                    ));
                }
            }
            return Err(err(
                StatusCode::CONFLICT,
                "Company book already committed; source refresh requires a separately reviewed delta importer",
            ));
        }
        if input.expected_head.is_some() {
            return Err(err(
                StatusCode::CONFLICT,
                "Expected company head does not exist",
            ));
        }
        let h = Head {
            format: "nxt-company-book-head/v1".into(),
            company_id: company.into(),
            preview_id: input.preview_id.clone(),
            revision: 1,
            committed_at: Utc::now().to_rfc3339(),
            commit_request_id: input.request_id.clone(),
            reviewer_label: input.reviewer_label.clone(),
            previous_head: None,
            journal_chunks: vec![],
            locked_through: p
                .input
                .source
                .period_lock
                .as_ref()
                .and_then(|p| p.closed_through.clone()),
            operation: None,
        };
        let raw = encoded(&h)?;
        let head_id = hash(raw.as_bytes());
        self.immutable(&Self::key(company, "receipt", &head_id)?, &h)
            .await?;
        if !self
            .store
            .compare_exchange(&key, None, &head_id)
            .await
            .map_err(storage)?
        {
            return self.commit_replay(company, &input, h.preview_id).await;
        }
        Ok(json!({"head_sha256":head_id,"head":h,"idempotent_replay":false,"controls":controls()}))
    }
    async fn commit_replay(
        &self,
        company: &str,
        input: &CommitInput,
        preview_id: String,
    ) -> Result<Value> {
        let (raw, h) = self.current(company).await?.ok_or_else(corrupt)?;
        for (rid, receipt) in self.head_chain(company, &raw, &h).await? {
            if receipt.operation.is_none()
                && receipt.commit_request_id == input.request_id
                && receipt.preview_id == preview_id
                && receipt.reviewer_label == input.reviewer_label
                && receipt.previous_head == input.expected_head
            {
                return Ok(
                    json!({"head_sha256":rid,"head":receipt,"current_head_sha256":raw,"idempotent_replay":true,"controls":controls()}),
                );
            }
        }
        Err(err(
            StatusCode::CONFLICT,
            "Company head changed concurrently",
        ))
    }
    pub async fn detail(&self, company: &str) -> Result<Value> {
        self.detail_page(company, 0, 25).await
    }
    pub async fn detail_page(&self, company: &str, offset: usize, limit: usize) -> Result<Value> {
        if limit == 0 || limit > 50 {
            return Err(bad("Preview history limit must be 1-50"));
        }
        Self::key(company, "head", "")?;
        let (_, ids) = self.preview_catalog(company).await?;
        let total = ids.len();
        let mut previews = vec![];
        for id in ids.into_iter().skip(offset).take(limit) {
            let p = self.load_preview_metadata(company, &id).await?;
            previews.push(json!({"preview_id":id,"company":p.input.company,"source":p.input.source,"accounts":p.analysis.accounts,"entries":p.analysis.entries,"lines":p.analysis.lines,"blockers":p.analysis.blockers,"stage":if p.analysis.controls_passed{"preview_ready"}else{"assessment_blocked"}}));
        }
        previews.sort_by(|a, b| a["preview_id"].as_str().cmp(&b["preview_id"].as_str()));
        let head = self.current(company).await?;
        Ok(
            json!({"company_id":company,"stage":if head.is_some(){"import_committed"}else if total==0{"acquisition_pending"}else{"staged"},"head_sha256":head.as_ref().map(|(r,_)|r),"head":head.map(|(_,h)|h),"previews":previews,"previews_total":total,"offset":offset,"limit":limit,"next_offset":if offset.saturating_add(limit)<total{Some(offset+limit)}else{None},"has_more":offset.saturating_add(limit)<total,"history_integrity":"metadata_only; active book fully validated","controls":controls()}),
        )
    }
    pub async fn list(&self) -> Result<Value> {
        self.list_page(0, 25).await
    }
    pub async fn list_page(&self, offset: usize, limit: usize) -> Result<Value> {
        if limit == 0 || limit > 50 {
            return Err(bad("Inventory page limit must be 1-50"));
        }
        let mut ids = self.store.smembers(INDEX).await.map_err(storage)?;
        if ids.len() > 500 {
            return Err(corrupt());
        }
        ids.sort();
        let total = ids.len();
        let selected = ids.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
        let mut keys = vec![];
        for id in &selected {
            keys.push(Self::key(id, "head", "v1")?);
            keys.push(Self::key(id, "latest-preview", "v1")?);
        }
        let pointers = self.store.mget(&keys).await.map_err(storage)?;
        if pointers.len() != selected.len() * 2 {
            return Err(corrupt());
        }
        let mut heads: BTreeMap<String, (String, Head)> = BTreeMap::new();
        let mut receipt_refs = vec![];
        let mut preview_refs = BTreeMap::new();
        for (i, id) in selected.iter().enumerate() {
            if let Some(hid) = &pointers[i * 2] {
                if !digest(hid) {
                    return Err(corrupt());
                }
                receipt_refs.push((id.clone(), hid.clone()));
            } else {
                let pid = if let Some(pid) = &pointers[i * 2 + 1] {
                    pid.clone()
                } else {
                    let mut older = self
                        .store
                        .smembers(&Self::key(id, "previews", "v1")?)
                        .await
                        .map_err(storage)?;
                    older.sort();
                    older.pop().ok_or_else(corrupt)?
                };
                if !digest(&pid) {
                    return Err(corrupt());
                }
                preview_refs.insert(id.clone(), pid);
            }
        }
        for batch in receipt_refs.chunks(8) {
            let keys = batch
                .iter()
                .map(|(c, h)| Self::key(c, "receipt", h))
                .collect::<Result<Vec<_>>>()?;
            let values = self.store.mget(&keys).await.map_err(storage)?;
            if values.len() != batch.len() {
                return Err(corrupt());
            }
            for ((company, hid), raw) in batch.iter().zip(values) {
                let raw = raw.ok_or_else(corrupt)?;
                if raw.len() > 512 * 1024 || hash(raw.as_bytes()) != *hid {
                    return Err(corrupt());
                }
                let h: Head = serde_json::from_str(&raw).map_err(|_| corrupt())?;
                if encoded(&h)? != raw
                    || h.company_id != *company
                    || !matches!(
                        h.format.as_str(),
                        "nxt-company-book-head/v1" | "nxt-company-book-head/v2"
                    )
                    || h.revision == 0
                    || h.revision > 129
                    || !digest(&h.preview_id)
                {
                    return Err(corrupt());
                }
                preview_refs.insert(company.clone(), h.preview_id.clone());
                heads.insert(company.clone(), (hid.clone(), h));
            }
        }
        let refs = preview_refs.into_iter().collect::<Vec<_>>();
        let mut previews = BTreeMap::new();
        for batch in refs.chunks(2) {
            let keys = batch
                .iter()
                .map(|(c, p)| Self::key(c, "preview", p))
                .collect::<Result<Vec<_>>>()?;
            let values = self.store.mget(&keys).await.map_err(storage)?;
            if values.len() != batch.len() {
                return Err(corrupt());
            }
            for ((company, pid), raw) in batch.iter().zip(values) {
                let raw = raw.ok_or_else(corrupt)?;
                if raw.len() > 4 * 1024 * 1024 || hash(raw.as_bytes()) != *pid {
                    return Err(corrupt());
                }
                let p: Preview = serde_json::from_str(&raw).map_err(|_| corrupt())?;
                if encoded(&p)? != raw
                    || p.company_id != *company
                    || p.format != "nxt-company-book-preview/v1"
                    || heads.contains_key(company) && !p.analysis.controls_passed
                {
                    return Err(corrupt());
                }
                previews.insert(company.clone(), (pid.clone(), p));
            }
        }
        let mut companies = vec![];
        for id in selected {
            let (pid, p) = previews.remove(&id).ok_or_else(corrupt)?;
            let head = heads.remove(&id);
            let head_summary=head.as_ref().map(|(_,h)|json!({"format":h.format,"company_id":h.company_id,"preview_id":h.preview_id,"revision":h.revision,"committed_at":h.committed_at,"locked_through":h.locked_through,"operation_kind":h.operation.as_ref().map(|o|match o{Operation::Journal{..}=>"journal",Operation::Reversal{..}=>"reversal",Operation::PeriodLock{..}=>"period_lock"})}));
            companies.push(json!({"company_id":id,"stage":if head.is_some(){"import_committed"}else{"staged"},"head_sha256":head.as_ref().map(|(id,_)|id),"head":head_summary,"head_truncated":true,"previews":[{"preview_id":pid,"company":p.input.company,"source":p.input.source,"accounts":p.analysis.accounts,"entries":p.analysis.entries,"lines":p.analysis.lines,"blockers":p.analysis.blockers,"stage":if p.analysis.controls_passed{"preview_ready"}else{"assessment_blocked"}}],"previews_truncated":true,"integrity_check":"metadata_only; full journal and history validation occurs on company book reads","controls":controls()}));
        }
        let v = json!({"companies":companies,"total":total,"offset":offset,"limit":limit,"has_more":offset.saturating_add(limit)<total,"integrity_check":"metadata_only; full journal and history validation occurs on company book reads","controls":controls()});
        if encoded(&v)?.len() > 4 * 1024 * 1024 {
            return Err(corrupt());
        }
        Ok(v)
    }
    async fn book(&self, company: &str) -> Result<(Head, Preview, Vec<Account>, Vec<Entry>)> {
        let s = self.snapshot(company).await?.ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "Company has no committed journal book",
            )
        })?;
        let mut entries = s.source_entries;
        entries.extend(s.native_entries);
        Ok((s.head, s.preview, s.accounts, entries))
    }
    pub async fn entries(
        &self,
        company: &str,
        offset: usize,
        limit: usize,
        account: Option<&str>,
    ) -> Result<Value> {
        self.entries_page(company, offset, limit, DEFAULT_PAGE_BYTES, account)
            .await
    }
    pub async fn entries_page(
        &self,
        company: &str,
        offset: usize,
        limit: usize,
        max_bytes: usize,
        account: Option<&str>,
    ) -> Result<Value> {
        if limit == 0 || limit > 200 {
            return Err(bad("Page limit must be 1-200"));
        }
        let (h, p, a, e) = self.book(company).await?;
        if account.is_some_and(|id| !a.iter().any(|a| a.id == id)) {
            return Err(err(
                StatusCode::NOT_FOUND,
                "Account not found in this company",
            ));
        }
        let mut result = vec![];
        let mut balance = 0i128;
        for e in e {
            if let Some(id) = account {
                for (i, l) in e.lines.iter().enumerate() {
                    if l.account_id == id {
                        balance = balance
                            .checked_add(money(&l.debit_cents)? - money(&l.credit_cents)?)
                            .ok_or_else(corrupt)?;
                        result.push(json!({"entry_id":e.id,"date":e.date,"source_id":e.source_id,"line_index":i,"line":l,"running_balance_cents":balance.to_string()}));
                    }
                }
            } else {
                result.push(serde_json::to_value(e).map_err(|_| corrupt())?);
            }
        }
        let (rows, total, next_offset, page_bytes) = page_rows(result, offset, limit, max_bytes)?;
        Ok(
            json!({"company_id":company,"company":p.input.company,"head":h,"offset":offset,"limit":limit,"total":total,"rows":rows,"next_offset":next_offset,"has_more":next_offset.is_some(),"max_bytes":max_bytes,"page_bytes":page_bytes,"order":"immutable source order","controls":controls()}),
        )
    }
    pub async fn trial_balance(&self, company: &str) -> Result<Value> {
        self.trial_balance_page(company, 0, 100, DEFAULT_PAGE_BYTES)
            .await
    }
    pub async fn trial_balance_page(
        &self,
        company: &str,
        offset: usize,
        limit: usize,
        max_bytes: usize,
    ) -> Result<Value> {
        let (h, p, _, entries) = self.book(company).await?;
        let source = p.analysis.clone();
        let mut combined = source.clone();
        let native = &entries[source.entries..];
        let (mut nd, mut nc, mut nl) = (0i128, 0i128, 0usize);
        for e in native {
            for l in &e.lines {
                let (d, c) = (money(&l.debit_cents)?, money(&l.credit_cents)?);
                nd = nd.checked_add(d).ok_or_else(corrupt)?;
                nc = nc.checked_add(c).ok_or_else(corrupt)?;
                nl += 1;
                let b = combined
                    .balances
                    .iter_mut()
                    .find(|b| b.account_id == l.account_id)
                    .ok_or_else(corrupt)?;
                let bd = b
                    .journal_debit_cents
                    .parse::<i128>()
                    .map_err(|_| corrupt())?
                    .checked_add(d)
                    .ok_or_else(corrupt)?;
                let bc = b
                    .journal_credit_cents
                    .parse::<i128>()
                    .map_err(|_| corrupt())?
                    .checked_add(c)
                    .ok_or_else(corrupt)?;
                let net = bd - bc;
                b.journal_debit_cents = bd.to_string();
                b.journal_credit_cents = bc.to_string();
                b.debit_cents = net.max(0).to_string();
                b.credit_cents = (-net).max(0).to_string();
            }
        }
        combined.entries += native.len();
        combined.lines += nl;
        combined.total_debit_cents = source
            .total_debit_cents
            .parse::<i128>()
            .map_err(|_| corrupt())?
            .checked_add(nd)
            .ok_or_else(corrupt)?
            .to_string();
        combined.total_credit_cents = source
            .total_credit_cents
            .parse::<i128>()
            .map_err(|_| corrupt())?
            .checked_add(nc)
            .ok_or_else(corrupt)?
            .to_string();
        let matches_source = source
            .balances
            .iter()
            .zip(&combined.balances)
            .all(|(a, b)| a.debit_cents == b.debit_cents && a.credit_cents == b.credit_cents);
        let mut rows = vec![];
        for (source, balance) in source.balances.iter().zip(&combined.balances) {
            let mut row = serde_json::to_value(balance).map_err(|_| corrupt())?;
            row["source_debit_cents"] = json!(source.debit_cents);
            row["source_credit_cents"] = json!(source.credit_cents);
            row["source_journal_debit_cents"] = json!(source.journal_debit_cents);
            row["source_journal_credit_cents"] = json!(source.journal_credit_cents);
            row["native_delta_debit_cents"] = json!(
                (aggregate(&balance.journal_debit_cents)?
                    - aggregate(&source.journal_debit_cents)?)
                .to_string()
            );
            row["native_delta_credit_cents"] = json!(
                (aggregate(&balance.journal_credit_cents)?
                    - aggregate(&source.journal_credit_cents)?)
                .to_string()
            );
            rows.push(row);
        }
        let (rows, total, next_offset, page_bytes) = page_rows(rows, offset, limit, max_bytes)?;
        let mut analysis = serde_json::to_value(&combined).map_err(|_| corrupt())?;
        analysis.as_object_mut().unwrap().remove("balances");
        let mut baseline = serde_json::to_value(&source).map_err(|_| corrupt())?;
        baseline.as_object_mut().unwrap().remove("balances");
        Ok(
            json!({"company_id":company,"company":p.input.company,"head":h,"source":p.input.source,"analysis":analysis,"source_baseline_analysis":baseline,"balances":rows,"offset":offset,"limit":limit,"total":total,"next_offset":next_offset,"has_more":next_offset.is_some(),"max_bytes":max_bytes,"page_bytes":page_bytes,"native_delta":{"entries":native.len(),"lines":nl,"total_debit_cents":nd.to_string(),"total_credit_cents":nc.to_string()},"reconciliation":{"source_baseline_unchanged":true,"source_controls_passed":true,"combined_balances_match_source":matches_source},"controls":controls()}),
        )
    }
}
