use super::models::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub input_revision: u64,
    pub input_hash: String,
    pub output_hash: String,
    pub input_document: EngagementInput,
    pub result: Value,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signoff {
    pub id: String,
    pub workpaper_id: String,
    pub stage: String,
    pub operator: String,
    pub note: String,
    pub input_hash: String,
    pub output_hash: String,
    pub at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockEvent {
    pub status: String,
    pub operator: String,
    pub note: String,
    pub snapshot_id: Option<String>,
    pub input_hash: Option<String>,
    pub at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportReceipt {
    pub preview_id: String,
    pub source_hash: String,
    pub canonical_hash: String,
    pub provenance: Value,
    pub input_revision: u64,
    pub at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngagementRecord {
    pub id: String,
    pub revision: u64,
    pub input_revision: u64,
    pub status: String,
    pub document: EngagementInput,
    pub snapshot: Option<Snapshot>,
    pub snapshots: Vec<Snapshot>,
    pub signoffs: Vec<Signoff>,
    pub signoff_history: Vec<Signoff>,
    pub lock_history: Vec<LockEvent>,
    #[serde(default)]
    pub import_history: Vec<ImportReceipt>,
    pub created_at: String,
    pub updated_at: String,
    pub rollforward_from: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub engagement_id: String,
    pub action: String,
    pub detail: String,
    pub at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportPreview {
    pub id: String,
    pub engagement_id: String,
    pub expected_revision: u64,
    pub source_hash: String,
    pub canonical_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub summary: Value,
    pub warnings: Vec<String>,
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub checks: Value,
    pub committed_revision: Option<u64>,
    pub provenance: Value,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct EngagementState {
    pub engagements: Vec<EngagementRecord>,
    pub previews: Vec<ImportPreview>,
    pub audit: Vec<AuditEvent>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionInput {
    pub expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignoffInput {
    pub expected_revision: u64,
    pub workpaper_id: String,
    pub stage: String,
    pub operator: String,
    #[serde(default)]
    pub note: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    pub expected_revision: u64,
    pub status: String,
    pub operator: String,
    #[serde(default)]
    pub note: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollforwardInput {
    pub expected_revision: u64,
    pub period_start: String,
    pub period_end: String,
    pub name: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    pub expected_revision: u64,
    pub document: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitInput {
    pub expected_revision: u64,
    pub preview_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxBridgeInput {
    pub expected_revision: u64,
    pub jurisdiction: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmBridgeInput {
    pub expected_revision: u64,
    pub project_id: Option<String>,
}
