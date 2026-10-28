//! Original source review archive, deliberately separate from financial inputs.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkInput {
    pub sha256: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceArchive {
    pub file_name: String,
    pub sha256: String,
    pub bytes: usize,
    pub encoding: String,
    pub chunks: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportPart {
    pub sha256: String,
    pub row_start: u32,
    pub row_end: u32,
    pub row_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub id: String,
    pub title: String,
    pub sheet_name: String,
    pub source_sha256: String,
    pub source_bytes: usize,
    pub columns: Vec<String>,
    pub row_count: usize,
    pub parts: Vec<ReportPart>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceException {
    pub report_id: String,
    pub row: u32,
    pub code: String,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotManifest {
    pub format: String,
    pub id: String,
    pub cohort_ref: String,
    pub company_name: String,
    pub source_system: String,
    pub captured_at: String,
    pub cutoff_date: String,
    pub reviewer_label: String,
    pub source_of_truth: String,
    pub mode: String,
    pub source_archive: SourceArchive,
    pub reports: Vec<Report>,
    pub exceptions: Vec<SourceException>,
    pub limitations: Vec<String>,
}

/// Column, original source type, exact decoded source text, original formula,
/// optional interpreted cents metadata. No amount is applied to native books.
pub type SourceCell = (String, String, String, Option<String>, Option<String>);
pub type SourceRow = (u32, Vec<SourceCell>);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRows {
    pub format: String,
    pub cohort_ref: String,
    pub source_archive_sha256: String,
    pub source_report_sha256: String,
    pub report_id: String,
    pub rows: Vec<SourceRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRecord {
    pub manifest: SnapshotManifest,
    pub manifest_sha256: String,
    pub published_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub snapshots: Vec<SnapshotRecord>,
}
