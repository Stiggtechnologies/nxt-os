use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextLink {
    pub module: String,
    pub record_id: String,
    pub label: String,
}

/// Original NXT text pages. No uploaded PDF, executable markup or external URL.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub name: String,
    pub pages: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipient {
    pub id: String,
    pub role_name: String,
    pub name: String,
    pub email: String,
    pub kind: String,
    pub routing_order: u16,
}

/// Basis-point layout on one original NXT page; never a recorded signature.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub document_id: String,
    pub recipient_id: String,
    pub page: usize,
    pub kind: String,
    pub label: String,
    pub required: bool,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Text captured during an internal rehearsal. Not a signature.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldValue {
    pub field_id: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub title: String,
    pub email_subject: String,
    pub email_message: String,
    pub context: Option<ContextLink>,
    pub documents: Vec<Document>,
    pub recipients: Vec<Recipient>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveInput {
    pub expected_revision: Option<u64>,
    pub operator_label: String,
    pub draft: Draft,
    /// NXT client label for access scoping. Kept when omitted on update.
    #[serde(default)]
    pub client_ref: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    pub expected_revision: u64,
    pub expected_content_hash: String,
    pub operator_label: String,
    pub action: String,
    pub note: String,
    #[serde(default)]
    pub field_values: Vec<FieldValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInput {
    pub expected_revision: u64,
    pub expected_content_hash: String,
    pub operator_label: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipientIdentity {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiateInput {
    pub expected_template_hash: String,
    pub operator_label: String,
    pub title: String,
    pub recipients: Vec<RecipientIdentity>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub id: String,
    pub revision: u64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub created_by_label: String,
    pub content_hash: String,
    pub reviewed_content_hash: Option<String>,
    pub reviewed_by_label: Option<String>,
    pub template_id: Option<String>,
    pub draft: Draft,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_token: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub resend_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_values: Vec<FieldValue>,
    /// NXT client label used for staff access scoping. Empty on older envelopes,
    /// which only unrestricted staff can see.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_ref: String,
}
fn is_zero(value: &u32) -> bool { *value == 0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub version: u64,
    pub created_at: String,
    pub created_by_label: String,
    pub content_hash: String,
    pub draft: Draft,
    #[serde(default, skip_serializing_if = "is_false")]
    pub favorite: bool,
}
fn is_false(value: &bool) -> bool { !*value }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEvent {
    pub id: String,
    pub occurred_at: String,
    pub record_id: String,
    pub action: String,
    pub operator_label: String,
    pub revision: u64,
    pub content_hash: String,
    pub previous_event_hash: Option<String>,
    pub note: String,
    pub event_hash: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureState {
    pub envelopes: Vec<Envelope>,
    pub templates: Vec<Template>,
    pub events: Vec<AuditEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<Folder>,
}
