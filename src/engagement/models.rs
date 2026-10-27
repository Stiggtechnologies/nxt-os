use serde::{Deserialize, Serialize};
use super::attachment_models::AttachmentVersion;

fn cad() -> String {
    "CAD".into()
}
fn internal() -> String {
    "internal".into()
}
fn adjusting() -> String {
    "adjusting".into()
}
fn medium() -> String {
    "medium".into()
}
fn open() -> String {
    "open".into()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub debit_cents: i64,
    #[serde(default)]
    pub credit_cents: i64,
    #[serde(default)]
    pub prior_debit_cents: i64,
    #[serde(default)]
    pub prior_credit_cents: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub gifi_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustmentLine {
    pub account_id: String,
    #[serde(default)]
    pub debit_cents: i64,
    #[serde(default)]
    pub credit_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjustment {
    pub id: String,
    pub reference: String,
    pub date: String,
    #[serde(default = "adjusting")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub lines: Vec<AdjustmentLine>,
}

/// Original NXT single-line text definition, copied into each custom annotation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickmarkDefinition {
    pub id: String,
    pub code: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub kind: String,
    pub note: String,
    pub operator_label: String,
    pub body_sha256: String,
    // Appended/omitted to keep legacy signed annotation preimages unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_mark: Option<TickmarkDefinition>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workpaper {
    pub id: String,
    pub reference: String,
    pub title: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub account_ids: Vec<String>,
    #[serde(default)]
    pub adjustment_ids: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    // Empty values are omitted to preserve existing signed input preimages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachment_versions: Vec<AttachmentVersion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default = "medium")]
    pub priority: String,
    #[serde(default)]
    pub blocking: bool,
    #[serde(default = "open")]
    pub status: String,
    #[serde(default)]
    pub workpaper_id: Option<String>,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub carry_forward: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngagementInput {
    pub name: String,
    #[serde(default)]
    pub client_ref: String,
    #[serde(default = "internal")]
    pub kind: String,
    pub period_start: String,
    pub period_end: String,
    #[serde(default)]
    pub prior_period_start: Option<String>,
    #[serde(default)]
    pub prior_period_end: Option<String>,
    #[serde(default = "cad")]
    pub currency: String,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub adjustments: Vec<Adjustment>,
    #[serde(default)]
    pub workpapers: Vec<Workpaper>,
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
    // Appended/omitted to preserve legacy engagement input and snapshot hashes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickmark_catalogue: Vec<TickmarkDefinition>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn empty_annotations_preserve_legacy_workpaper_preimage() {
        let legacy = r#"{"id":"p1","reference":"A","title":"Legacy","folder":"","body":"Existing evidence","required":true,"account_ids":[],"adjustment_ids":[],"depends_on":[]}"#;
        let decoded: Workpaper = serde_json::from_str(legacy).unwrap();
        assert!(decoded.annotations.is_empty());
        assert_eq!(serde_json::to_string(&decoded).unwrap(), legacy);
        let mut explicit_empty = serde_json::to_value(&decoded).unwrap();
        explicit_empty["annotations"] = serde_json::json!([]);
        let decoded_empty: Workpaper = serde_json::from_value(explicit_empty).unwrap();
        assert_eq!(serde_json::to_string(&decoded_empty).unwrap(), legacy);
    }

    #[test]
    fn absent_or_empty_catalogue_preserves_legacy_input_preimage() {
        let legacy=r#"{"name":"Legacy","client_ref":"","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","prior_period_start":null,"prior_period_end":null,"currency":"CAD","accounts":[],"groups":[],"adjustments":[],"workpapers":[],"issues":[],"notes":"","expected_revision":null}"#;
        let decoded:EngagementInput=serde_json::from_str(legacy).unwrap();
        assert!(decoded.tickmark_catalogue.is_empty());
        assert_eq!(serde_json::to_string(&decoded).unwrap(),legacy);
        assert_eq!(super::super::hash(&decoded).map_err(|e|e.1).unwrap(),hex::encode(sha2::Sha256::digest(legacy.as_bytes())));
        let mut explicit=serde_json::to_value(&decoded).unwrap();explicit["tickmark_catalogue"]=serde_json::json!([]);
        assert_eq!(serde_json::to_string(&serde_json::from_value::<EngagementInput>(explicit).unwrap()).unwrap(),legacy);
    }

    #[test]
    fn absent_or_null_custom_snapshot_preserves_legacy_annotation_preimage() {
        let legacy=format!("{{\"id\":\"a1\",\"kind\":\"note\",\"note\":\"Legacy note\",\"operator_label\":\"Unverified label\",\"body_sha256\":\"{}\"}}","a".repeat(64));
        let decoded:Annotation=serde_json::from_str(&legacy).unwrap();assert!(decoded.custom_mark.is_none());
        assert_eq!(serde_json::to_string(&decoded).unwrap(),legacy);
        assert_eq!(super::super::hash(&decoded).map_err(|e|e.1).unwrap(),hex::encode(sha2::Sha256::digest(legacy.as_bytes())));
        let mut explicit=serde_json::to_value(decoded).unwrap();explicit["custom_mark"]=serde_json::Value::Null;
        assert_eq!(serde_json::to_string(&serde_json::from_value::<Annotation>(explicit).unwrap()).unwrap(),legacy);
    }
}
