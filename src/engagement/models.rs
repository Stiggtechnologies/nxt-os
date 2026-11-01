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
    /// For `kind == "tax"` entries: the T2 Schedule 1 line the entry reconciles on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule1_line: Option<String>,
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

/// A copied local-account context digest, in addition to the required text anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AccountAnchor {
    pub engagement_id: String,
    pub account_id: String,
    pub context_sha256: String,
}
impl<'de> Deserialize<'de> for AccountAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectOnly;
        impl<'de> serde::de::Visitor<'de> for ObjectOnly {
            type Value = AccountAnchor;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an account anchor object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<AccountAnchor, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { engagement_id: String, account_id: String, context_sha256: String }
                let value = Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(AccountAnchor { engagement_id: value.engagement_id, account_id: value.account_id, context_sha256: value.context_sha256 })
            }
        }
        deserializer.deserialize_map(ObjectOnly)
    }
}
fn present_account_anchor<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<AccountAnchor>, D::Error> {
    AccountAnchor::deserialize(deserializer).map(Some)
}

/// Exact recorded UTF-8 selection, retained even when the current body is stale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TextRangeAnchor {
    pub engagement_id: String,
    pub start_utf8_byte: u32,
    pub end_utf8_byte: u32,
    pub selection_sha256: String,
    pub selected_text: String,
}
impl<'de> Deserialize<'de> for TextRangeAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectOnly;
        impl<'de> serde::de::Visitor<'de> for ObjectOnly {
            type Value = TextRangeAnchor;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("a text range anchor object") }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<TextRangeAnchor, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { engagement_id: String, start_utf8_byte: u32, end_utf8_byte: u32, selection_sha256: String, selected_text: String }
                let v=Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(TextRangeAnchor{engagement_id:v.engagement_id,start_utf8_byte:v.start_utf8_byte,end_utf8_byte:v.end_utf8_byte,selection_sha256:v.selection_sha256,selected_text:v.selected_text})
            }
        }
        deserializer.deserialize_map(ObjectOnly)
    }
}
fn present_text_range_anchor<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<TextRangeAnchor>,D::Error>{TextRangeAnchor::deserialize(d).map(Some)}

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
    // Missing is legacy body-only scope; a present anchor must be an object, not null.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present_account_anchor")]
    pub account_anchor: Option<AccountAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present_text_range_anchor")]
    pub text_range_anchor: Option<TextRangeAnchor>,
}

/// Original NXT planning/evidence distinction; no proprietary document format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkpaperType {
    #[default]
    Manual,
    Placeholder,
    ExternalReference,
}
impl WorkpaperType {
    pub fn is_manual(&self) -> bool {
        *self == Self::Manual
    }
}
impl<'de> Deserialize<'de> for WorkpaperType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "manual" => Ok(Self::Manual),
            "placeholder" => Ok(Self::Placeholder),
            "external_reference" => Ok(Self::ExternalReference),
            _ => Err(serde::de::Error::unknown_variant(&value, &["manual", "placeholder", "external_reference"])),
        }
    }
}

// A missing URL is legacy absence; a present URL must be a string, never null.
fn present_reference_url<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
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
    // Appended/omitted so legacy manual documents retain exact signed preimages.
    #[serde(default, skip_serializing_if = "WorkpaperType::is_manual")]
    pub document_type: WorkpaperType,
    /// Labels only. Empty is omitted so older workpaper hashes stay stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// A text reference. It is not a URL and nothing is fetched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present_reference_url")]
    pub reference_url: Option<String>,
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
