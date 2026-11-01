//! Read-only, immutable parallel-test source archives. Never financial import.
pub mod models;
mod routes;
use crate::{referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use models::*;
pub use routes::router;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const CATALOG_KEY: &str = "migrations:catalog:v1";
const CHUNK_MAX: usize = 512 * 1024;
const ARCHIVE_MAX: usize = 4 * 1024 * 1024;
const REFERENCED_MAX: usize = 24 * 1024 * 1024;
const MANIFEST_MAX: usize = 128 * 1024;
const SNAPSHOTS_MAX: usize = 500;
// The store RPC wraps the catalog as a JSON string. Even at twice its size
// after escaping, this stays below the store's 24 MiB response limit.
const CATALOG_MAX: usize = 8 * 1024 * 1024;
const LIST_MAX: usize = 1024 * 1024;
const IDENTITY_LIMIT: &str =
    "Shared workspace key; reviewer labels are metadata, not verified identities.";
type MResult<T> = Result<T, ApiError>;

pub struct Migrations {
    pub store: Store,
}
pub(crate) fn bad(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}
fn conflict(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
fn missing() -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        "published migration snapshot or report not found".into(),
    )
}
fn storage_error(_: String) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "migration archive storage unavailable".into(),
    )
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn column(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 3 || !value.bytes().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let number = value
        .bytes()
        .fold(0u32, |total, c| total * 26 + u32::from(c - b'A' + 1));
    (number <= 16_384).then_some(number)
}
fn controls() -> Value {
    json!({"shadow_only":true,"live_cutover_allowed":false,"source_authoritative":true,"identity_limit":IDENTITY_LIMIT})
}
fn controlled(mut value: Value) -> Value {
    value
        .as_object_mut()
        .unwrap()
        .extend(controls().as_object().unwrap().clone());
    value
}
fn manifest_bytes(manifest: &SnapshotManifest) -> MResult<Vec<u8>> {
    serde_json::to_vec(manifest).map_err(|_| bad("manifest cannot be encoded"))
}
fn validate_manifest(manifest: &SnapshotManifest) -> MResult<()> {
    if manifest.format != "nxt-shadow-snapshot/v1"
        || manifest.mode != "parallel_test"
        || manifest.source_of_truth != "original_system"
        || manifest.source_system != "quickbooks"
    {
        return Err(bad(
            "only QuickBooks parallel-test snapshots with the original system authoritative are supported",
        ));
    }
    if !identifier(&manifest.id)
        || !text(&manifest.cohort_ref, 128)
        || !text(&manifest.company_name, 240)
        || !text(&manifest.reviewer_label, 240)
        || DateTime::parse_from_rfc3339(&manifest.captured_at).is_err()
        || NaiveDate::parse_from_str(&manifest.cutoff_date, "%Y-%m-%d").is_err()
        || manifest.cutoff_date.len() != 10
    {
        return Err(bad(
            "invalid snapshot identity, dates or responsibility metadata",
        ));
    }
    let archive = &manifest.source_archive;
    if !text(&archive.file_name, 240)
        || archive.file_name.contains(['/', '\\', '"', ':'])
        || !archive.file_name.to_ascii_lowercase().ends_with(".zip")
        || !digest(&archive.sha256)
        || archive.encoding != "hex"
        || archive.bytes == 0
        || archive.bytes > ARCHIVE_MAX
        || archive.chunks.is_empty()
        || archive.chunks.len() > 128
        || archive.chunks.iter().any(|hash| !digest(hash))
    {
        return Err(bad("invalid source archive or archive bound"));
    }
    if manifest.reports.is_empty()
        || manifest.reports.len() > 16
        || manifest.exceptions.len() > 5000
        || manifest.limitations.is_empty()
        || manifest.limitations.len() > 128
        || manifest.limitations.iter().any(|note| !text(note, 4096))
    {
        return Err(bad("invalid reports, exceptions or explicit limitations"));
    }
    let mut ids = HashSet::new();
    for report in &manifest.reports {
        if !identifier(&report.id)
            || !ids.insert(&report.id)
            || !text(&report.title, 240)
            || !text(&report.sheet_name, 240)
            || !digest(&report.source_sha256)
            || report.source_bytes == 0
            || report.source_bytes > 32 * 1024 * 1024
            || report.row_count > 100_000
            || report.columns.is_empty()
            || report.columns.len() > 16_384
            || report.parts.len() > 128
            || (report.parts.is_empty() != (report.row_count == 0))
        {
            return Err(bad("invalid report identity, source reference or capacity"));
        }
        let mut previous = 0;
        for letter in &report.columns {
            let number = column(letter).ok_or_else(|| bad("invalid worksheet column"))?;
            if number <= previous {
                return Err(bad("report columns must be unique and in worksheet order"));
            }
            previous = number;
        }
        let mut last = 0;
        let mut count = 0;
        for part in &report.parts {
            if !digest(&part.sha256)
                || part.row_count == 0
                || part.row_count > 100_000
                || part.row_start == 0
                || part.row_end > 1_048_576
                || part.row_end < part.row_start
                || part.row_start <= last
                || part.row_count as u64 > u64::from(part.row_end - part.row_start) + 1
            {
                return Err(bad("invalid or overlapping report part ranges"));
            }
            last = part.row_end;
            count += part.row_count;
        }
        if count != report.row_count {
            return Err(bad("report row total does not match its parts"));
        }
    }
    let mut exceptions = HashSet::new();
    for exception in &manifest.exceptions {
        if !ids.contains(&exception.report_id)
            || exception.row == 0
            || exception.row > 1_048_576
            || !identifier(&exception.code)
            || !text(&exception.note, 4096)
            || !exceptions.insert((&exception.report_id, exception.row, &exception.code))
        {
            return Err(bad("invalid source exception or report reference"));
        }
    }
    if manifest_bytes(manifest)?.len() > MANIFEST_MAX {
        return Err(bad("snapshot manifest exceeds 128 KiB"));
    }
    Ok(())
}

impl Migrations {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
    fn decode_catalog(raw: Option<&str>) -> MResult<Catalog> {
        match raw {
            None => Ok(Catalog::default()),
            Some(raw) => {
                if raw.len() > CATALOG_MAX {
                    return Err(conflict(
                        "migration catalog exceeds its bound; recovery required",
                    ));
                }
                let catalog: Catalog = serde_json::from_str(raw)
                    .map_err(|_| conflict("migration catalog is corrupt; recovery required"))?;
                if catalog.snapshots.len() > SNAPSHOTS_MAX {
                    return Err(conflict("migration catalog capacity is corrupt"));
                }
                let mut ids = HashSet::new();
                for record in &catalog.snapshots {
                    validate_manifest(&record.manifest)
                        .map_err(|_| conflict("stored migration manifest is invalid"))?;
                    if !ids.insert(&record.manifest.id)
                        || sha(&manifest_bytes(&record.manifest)?) != record.manifest_sha256
                    {
                        return Err(conflict("stored migration manifest integrity check failed"));
                    }
                }
                Ok(catalog)
            }
        }
    }
    async fn load(&self) -> MResult<Catalog> {
        let raw = self.store.get(CATALOG_KEY).await.map_err(storage_error)?;
        Self::decode_catalog(raw.as_deref())
    }
    pub async fn upload_chunk(&self, input: ChunkInput) -> MResult<Value> {
        if !digest(&input.sha256)
            || input.content.is_empty()
            || input.content.len() > CHUNK_MAX
            || sha(input.content.as_bytes()) != input.sha256
        {
            return Err(bad(
                "chunk must contain 1–512 KiB of exact UTF-8 content matching its SHA-256",
            ));
        }
        let key = format!("migrations:blob:{}", input.sha256);
        let inserted = self
            .store
            .compare_exchange(&key, None, &input.content)
            .await
            .map_err(storage_error)?;
        if !inserted
            && self
                .store
                .get(&key)
                .await
                .map_err(storage_error)?
                .as_deref()
                != Some(&input.content)
        {
            return Err(conflict(
                "existing immutable chunk differs; recovery required",
            ));
        }
        Ok(json!({"sha256":input.sha256,"bytes":input.content.len(),"replayed":!inserted}))
    }
    async fn chunks(&self, hashes: &[String]) -> MResult<HashMap<String, String>> {
        let mut seen = HashSet::new();
        let unique = hashes
            .iter()
            .filter(|hash| seen.insert((*hash).clone()))
            .cloned()
            .collect::<Vec<_>>();
        if unique.len() > 512 || unique.iter().any(|hash| !digest(hash)) {
            return Err(bad("snapshot chunk references exceed their bound"));
        }
        let mut result = HashMap::new();
        let mut total = 0usize;
        for batch in unique.chunks(8) {
            let keys = batch
                .iter()
                .map(|hash| format!("migrations:blob:{hash}"))
                .collect::<Vec<_>>();
            let values = self.store.mget(&keys).await.map_err(storage_error)?;
            if values.len() != batch.len() {
                return Err(conflict("chunk lookup was incomplete"));
            }
            for (hash, value) in batch.iter().zip(values) {
                let value = value.ok_or_else(|| {
                    conflict("upload every referenced chunk before publishing the snapshot")
                })?;
                if value.is_empty() || value.len() > CHUNK_MAX || sha(value.as_bytes()) != *hash {
                    return Err(conflict("stored chunk integrity check failed"));
                }
                total = total
                    .checked_add(value.len())
                    .ok_or_else(|| bad("referenced chunk size overflow"))?;
                if total > REFERENCED_MAX {
                    return Err(bad("snapshot referenced content exceeds 24 MiB"));
                }
                result.insert(hash.clone(), value);
            }
        }
        Ok(result)
    }
    fn archive(manifest: &SnapshotManifest, chunks: &HashMap<String, String>) -> MResult<Vec<u8>> {
        let mut encoded = String::new();
        for hash in &manifest.source_archive.chunks {
            let value = chunks
                .get(hash)
                .ok_or_else(|| conflict("archive chunk unavailable"))?;
            if value.len() % 2 != 0 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(bad("archive chunks must be complete hexadecimal bytes"));
            }
            if encoded.len() + value.len() > ARCHIVE_MAX * 2 {
                return Err(bad("source archive exceeds 4 MiB"));
            }
            encoded.push_str(value);
        }
        let bytes = hex::decode(encoded).map_err(|_| bad("invalid archive hex encoding"))?;
        if bytes.len() != manifest.source_archive.bytes
            || sha(&bytes) != manifest.source_archive.sha256
        {
            return Err(conflict(
                "original source archive byte count or SHA-256 does not match",
            ));
        }
        Ok(bytes)
    }
    fn part(
        manifest: &SnapshotManifest,
        report: &Report,
        part: &ReportPart,
        raw: &str,
    ) -> MResult<ReportRows> {
        let rows: ReportRows =
            serde_json::from_str(raw).map_err(|_| bad("invalid source row JSON schema"))?;
        if rows.format != "nxt-shadow-report-rows/v1"
            || rows.cohort_ref != manifest.cohort_ref
            || rows.source_archive_sha256 != manifest.source_archive.sha256
            || rows.source_report_sha256 != report.source_sha256
            || rows.report_id != report.id
            || rows.rows.len() != part.row_count
            || rows.rows.first().map(|r| r.0) != Some(part.row_start)
            || rows.rows.last().map(|r| r.0) != Some(part.row_end)
        {
            return Err(bad(
                "source row part ownership, hashes or declared row range does not match",
            ));
        }
        let allowed = report.columns.iter().cloned().collect::<HashSet<_>>();
        let mut previous = 0;
        for (number, cells) in &rows.rows {
            if *number <= previous || *number > 1_048_576 || cells.len() > report.columns.len() {
                return Err(bad("source rows must be unique and in worksheet order"));
            }
            previous = *number;
            let mut previous_column = 0;
            for (letter, source_type, raw_value, formula, cents) in cells {
                let number = column(letter).ok_or_else(|| bad("invalid source cell column"))?;
                if number <= previous_column
                    || !allowed.contains(letter)
                    || !["n", "s", "inlineStr", "str"].contains(&source_type.as_str())
                    || raw_value.len() > 65_536
                    || formula.as_ref().is_some_and(|v| v.len() > 16_384)
                    || cents.as_ref().is_some_and(|v| {
                        let digits = v.strip_prefix('-').unwrap_or(v);
                        v.len() > 32
                            || digits.is_empty()
                            || !digits.bytes().all(|c| c.is_ascii_digit())
                    })
                {
                    return Err(bad("invalid source cell schema, order or metadata bound"));
                }
                previous_column = number;
            }
        }
        Ok(rows)
    }
    async fn validate_content(&self, manifest: &SnapshotManifest) -> MResult<()> {
        let hashes = manifest
            .source_archive
            .chunks
            .iter()
            .chain(
                manifest
                    .reports
                    .iter()
                    .flat_map(|report| report.parts.iter().map(|part| &part.sha256)),
            )
            .cloned()
            .collect::<Vec<_>>();
        let chunks = self.chunks(&hashes).await?;
        Self::archive(manifest, &chunks)?;
        let mut cells = 0usize;
        let mut rows_by_report = HashMap::new();
        for report in &manifest.reports {
            let mut worksheet_rows = HashSet::new();
            for part in &report.parts {
                let content = chunks
                    .get(&part.sha256)
                    .ok_or_else(|| conflict("row chunk unavailable"))?;
                let rows = Self::part(manifest, report, part, content)?;
                for (number, values) in rows.rows {
                    worksheet_rows.insert(number);
                    cells += values.len();
                    if cells > 500_000 {
                        return Err(bad("snapshot exceeds 500,000 source cells"));
                    }
                }
            }
            rows_by_report.insert(report.id.clone(), worksheet_rows);
        }
        for exception in &manifest.exceptions {
            if !rows_by_report
                .get(&exception.report_id)
                .is_some_and(|rows| rows.contains(&exception.row))
            {
                return Err(bad("source exception refers to an absent worksheet row"));
            }
        }
        Ok(())
    }
    pub async fn publish(&self, manifest: SnapshotManifest) -> MResult<Value> {
        validate_manifest(&manifest)?;
        let manifest_sha256 = sha(&manifest_bytes(&manifest)?);
        let existing = self.load().await?;
        if let Some(record) = existing
            .snapshots
            .iter()
            .find(|record| record.manifest.id == manifest.id)
        {
            if record.manifest_sha256 != manifest_sha256 {
                return Err(conflict(
                    "snapshot ID already names a different immutable manifest",
                ));
            }
            return Ok(controlled(
                json!({"manifest":record.manifest,"manifest_sha256":record.manifest_sha256,"published_at":record.published_at,"replayed":true}),
            ));
        }
        if existing.snapshots.len() >= SNAPSHOTS_MAX {
            return Err(conflict("migration snapshot capacity reached (500)"));
        }
        self.validate_content(&manifest).await?;
        let record = SnapshotRecord {
            manifest,
            manifest_sha256,
            published_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        };
        for _ in 0..16 {
            let raw = self.store.get(CATALOG_KEY).await.map_err(storage_error)?;
            let mut catalog = Self::decode_catalog(raw.as_deref())?;
            // Decoding and CAS use the same preimage; an intervening publication
            // causes the comparison to fail, never a lost snapshot.
            if let Some(existing) = catalog
                .snapshots
                .iter()
                .find(|item| item.manifest.id == record.manifest.id)
            {
                if existing.manifest_sha256 != record.manifest_sha256 {
                    return Err(conflict(
                        "snapshot ID already names a different immutable manifest",
                    ));
                }
                return Ok(controlled(
                    json!({"manifest":existing.manifest,"manifest_sha256":existing.manifest_sha256,"published_at":existing.published_at,"replayed":true}),
                ));
            }
            if catalog.snapshots.len() >= SNAPSHOTS_MAX {
                return Err(conflict("migration snapshot capacity reached (500)"));
            }
            catalog.snapshots.push(record.clone());
            let encoded =
                serde_json::to_string(&catalog).map_err(|_| bad("catalog cannot be encoded"))?;
            if encoded.len() > CATALOG_MAX {
                return Err(conflict(
                    "migration catalog exceeds 8 MiB; no snapshot was published",
                ));
            }
            if self
                .store
                .compare_exchange(CATALOG_KEY, raw.as_deref(), &encoded)
                .await
                .map_err(storage_error)?
            {
                return Ok(controlled(
                    json!({"manifest":record.manifest,"manifest_sha256":record.manifest_sha256,"published_at":record.published_at,"replayed":false}),
                ));
            }
        }
        Err(conflict(
            "migration catalog changed repeatedly; retry publication",
        ))
    }
    pub async fn list(&self) -> MResult<Value> {
        let catalog = self.load().await?;
        let summaries=catalog.snapshots.iter().map(|record|{let m=&record.manifest;json!({"id":m.id,"client_ref":m.cohort_ref,"cohort_ref":m.cohort_ref,"company_name":m.company_name,"source_system":m.source_system,"captured_at":m.captured_at,"cutoff_date":m.cutoff_date,"reviewer_label":m.reviewer_label,"manifest_sha256":record.manifest_sha256,"published_at":record.published_at,"report_count":m.reports.len(),"row_count":m.reports.iter().map(|r|r.row_count).sum::<usize>(),"exception_count":m.exceptions.len(),"archive_bytes":m.source_archive.bytes})}).collect::<Vec<_>>();
        let response =
            controlled(json!({"snapshots":summaries,"storage":self.store.backend_name()}));
        if serde_json::to_vec(&response)
            .map_err(|_| bad("snapshot summaries cannot be encoded"))?
            .len()
            > LIST_MAX
        {
            return Err(conflict(
                "migration snapshot summaries exceed 1 MiB; recovery required",
            ));
        }
        Ok(response)
    }
    async fn record(&self, id: &str) -> MResult<SnapshotRecord> {
        if !identifier(id) {
            return Err(bad("invalid snapshot ID"));
        }
        self.load()
            .await?
            .snapshots
            .into_iter()
            .find(|record| record.manifest.id == id)
            .ok_or_else(missing)
    }
    /// Client label of a snapshot for staff scoping: the cohort reference, which is the
    /// company id company books uses as the client label.
    pub async fn client_of(&self, id: &str) -> Option<String> {
        self.record(id).await.ok().map(|r| r.manifest.cohort_ref)
    }
    pub async fn detail(&self, id: &str) -> MResult<Value> {
        let record = self.record(id).await?;
        Ok(controlled(
            json!({"manifest":record.manifest,"manifest_sha256":record.manifest_sha256,"published_at":record.published_at,"storage":self.store.backend_name()}),
        ))
    }
    pub async fn rows(
        &self,
        id: &str,
        report_id: &str,
        from_row: u32,
        limit: usize,
    ) -> MResult<Value> {
        if !identifier(report_id)
            || from_row == 0
            || from_row > 1_048_576
            || !(1..=200).contains(&limit)
        {
            return Err(bad("invalid report ID or page; limit must be 1–200"));
        }
        let record = self.record(id).await?;
        let report = record
            .manifest
            .reports
            .iter()
            .find(|report| report.id == report_id)
            .ok_or_else(missing)?;
        let mut rows = Vec::new();
        let mut row_bytes = 0usize;
        let mut next_from_row = None;
        'parts: for part in &report.parts {
            if part.row_end < from_row {
                continue;
            }
            let chunks = self.chunks(&[part.sha256.clone()]).await?;
            let decoded = Self::part(&record.manifest, report, part, &chunks[&part.sha256])?;
            for row in decoded.rows.into_iter().filter(|row| row.0 >= from_row) {
                let bytes = serde_json::to_vec(&row)
                    .map_err(|_| bad("source row cannot be encoded"))?
                    .len();
                if rows.len() >= limit || row_bytes + bytes > 2 * 1024 * 1024 {
                    next_from_row = Some(row.0);
                    break 'parts;
                }
                row_bytes += bytes;
                rows.push(row);
            }
        }
        Ok(controlled(
            json!({"snapshot_id":id,"report_id":report_id,"from_row":from_row,"limit":limit,"rows":rows,"next_from_row":next_from_row,"has_more":next_from_row.is_some()}),
        ))
    }
    pub async fn download_archive(&self, id: &str) -> MResult<Vec<u8>> {
        let record = self.record(id).await?;
        let chunks = self.chunks(&record.manifest.source_archive.chunks).await?;
        Self::archive(&record.manifest, &chunks)
    }
}
