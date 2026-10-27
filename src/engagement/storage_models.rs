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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_binding_sha256: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollforwardCopyPolicy {
    pub version: String,
    pub comparatives: String,
    pub current_balances: String,
    pub name: String,
    pub source_evidence: String,
    pub preserved_fields: Vec<String>,
    pub cleared_fields: Vec<String>,
    pub issues: String,
    pub source_mutated: bool,
    pub external_modules_mutated: bool,
}

impl RollforwardCopyPolicy {
    pub(crate) fn adjusted_comparatives() -> Self {
        Self {
            version: "nxt-engagement-rollforward/v1".into(),
            comparatives: "source_snapshot_adjusted_trial_balance".into(),
            current_balances: "zero".into(),
            name: "source_unless_explicitly_overridden".into(),
            source_evidence: "embedded_source_snapshot_archive_only".into(),
            preserved_fields: [
                "client_ref",
                "engagement_kind",
                "currency",
                "account_ids_codes_names_kinds_and_groups",
                "group_definitions_and_gifi_metadata",
                "workpaper_ids_references_titles_folders_and_dependencies",
                "workpaper_account_references",
            ].into_iter().map(str::to_owned).collect(),
            cleared_fields: [
                "current_account_balances",
                "local_adjustments",
                "workpaper_bodies",
                "workpaper_annotations",
                "workpaper_attachment_versions",
                "workpaper_adjustment_references",
                "notes",
                "active_calculation",
                "calculation_history",
                "signoffs",
                "signoff_history",
                "lock_history",
                "import_history",
            ].into_iter().map(str::to_owned).collect(),
            issues: "open_and_carry_forward_only".into(),
            source_mutated: false,
            external_modules_mutated: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollforwardOrigin {
    pub source_engagement_id: String,
    pub source_snapshot_id: String,
    pub source_input_revision: u64,
    pub source_input_hash: String,
    pub source_output_hash: String,
    pub source_period_start: String,
    pub source_period_end: String,
    pub target_period_start: String,
    pub target_period_end: String,
    pub copied_at: String,
    pub copy_policy: RollforwardCopyPolicy,
    /// An immutable native snapshot archive, never an active target calculation.
    /// Snapshot contains EngagementInput, not EngagementRecord, so this cannot
    /// recursively copy previous origins or the source's full history.
    pub source_snapshot: Snapshot,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollforward_origin: Option<RollforwardOrigin>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_input_sha256: Option<String>,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct EngagementState {
    pub engagements: Vec<EngagementRecord>,
    pub previews: Vec<ImportPreview>,
    pub audit: Vec<AuditEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachment_blobs: Vec<super::attachment_models::AttachmentBlob>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const INPUT_PREIMAGE: &str = r#"{"name":"Legacy","client_ref":"","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","prior_period_start":null,"prior_period_end":null,"currency":"CAD","accounts":[],"groups":[],"adjustments":[],"workpapers":[],"issues":[],"notes":"","expected_revision":null}"#;
    const INPUT_SHA256: &str = "c5f24b933b1490c487adc1d0566faeb996f7fa8e16e700569308c0269a5f8447";

    fn legacy_value() -> Value {
        json!({
            "id": "eng_legacy", "revision": 1, "input_revision": 1,
            "status": "draft", "document": serde_json::from_str::<Value>(INPUT_PREIMAGE).unwrap(),
            "snapshot": null, "snapshots": [], "signoffs": [],
            "signoff_history": [], "lock_history": [], "import_history": [],
            "created_at": "2026-10-26T00:00:00Z", "updated_at": "2026-10-26T00:00:00Z",
            "rollforward_from": null
        })
    }

    #[test]
    fn legacy_record_omits_origin_and_preserves_input_serialization_and_hash() {
        let legacy = legacy_value();
        let record: EngagementRecord = serde_json::from_value(legacy.clone()).unwrap();
        assert!(record.rollforward_origin.is_none());
        assert_eq!(serde_json::to_value(&record).unwrap(), legacy);
        assert_eq!(serde_json::to_string(&record.document).unwrap(), INPUT_PREIMAGE);
        assert_eq!(super::super::hash(&record.document).map_err(|error| error.1).unwrap(), INPUT_SHA256);
    }

    #[test]
    fn origin_archives_exact_nonrecursive_snapshot_without_changing_input_hash() {
        let mut target: EngagementRecord = serde_json::from_value(legacy_value()).unwrap();
        let result = json!({"ruleset": "storage-fixture", "diagnostics": []});
        let snapshot = Snapshot {
            id: "snapshot_legacy".into(), input_revision: 1,
            input_hash: INPUT_SHA256.into(),
            output_hash: super::super::hash(&result).map_err(|error| error.1).unwrap(),
            input_document: target.document.clone(), result,
            created_at: "2026-10-25T00:00:00Z".into(),
        };
        target.rollforward_from = Some("eng_source".into());
        target.rollforward_origin = Some(RollforwardOrigin {
            source_engagement_id: "eng_source".into(),
            source_snapshot_id: snapshot.id.clone(), source_input_revision: snapshot.input_revision,
            source_input_hash: snapshot.input_hash.clone(), source_output_hash: snapshot.output_hash.clone(),
            source_period_start: snapshot.input_document.period_start.clone(),
            source_period_end: snapshot.input_document.period_end.clone(),
            target_period_start: "2026-01-01".into(), target_period_end: "2026-12-31".into(),
            copied_at: "2026-10-26T00:00:00Z".into(),
            copy_policy: RollforwardCopyPolicy::adjusted_comparatives(), source_snapshot: snapshot.clone(),
        });
        let encoded = serde_json::to_value(&target).unwrap();
        let decoded: EngagementRecord = serde_json::from_value(encoded.clone()).unwrap();
        let origin = decoded.rollforward_origin.unwrap();
        assert_eq!(serde_json::to_value(&origin.source_snapshot).unwrap(), serde_json::to_value(snapshot).unwrap());
        assert_eq!(super::super::hash(&origin.source_snapshot.input_document).map_err(|error| error.1).unwrap(), origin.source_input_hash);
        assert_eq!(super::super::hash(&origin.source_snapshot.result).map_err(|error| error.1).unwrap(), origin.source_output_hash);
        assert!(encoded["rollforward_origin"]["source_snapshot"]["input_document"].get("rollforward_origin").is_none());
        assert!(target.snapshot.is_none() && target.snapshots.is_empty() && target.signoffs.is_empty());
        assert_eq!(super::super::hash(&target.document).map_err(|error| error.1).unwrap(), INPUT_SHA256);
        assert_eq!(origin.copy_policy.source_evidence, "embedded_source_snapshot_archive_only");
        assert!(!origin.copy_policy.source_mutated && !origin.copy_policy.external_modules_mutated);
    }
}
