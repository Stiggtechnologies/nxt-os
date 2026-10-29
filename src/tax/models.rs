use serde::{Deserialize, Serialize};
use serde_json::Value;

fn object() -> Value { serde_json::json!({}) }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Party {
    #[serde(default)] pub id: String,
    pub role: String,
    pub name: String,
    #[serde(default)] pub identifier: String,
    #[serde(default)] pub share_basis_points: i64,
    #[serde(default)] pub allocation_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalculationSnapshot {
    pub id: String,
    pub input_revision: u64,
    pub request_revision: u64,
    pub idempotency_key: Option<String>,
    #[serde(default)] pub input_document: Option<ReturnInput>,
    pub input_hash: String,
    pub output_hash: String,
    pub created_at: String,
    pub result: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub at: String,
    pub reviewer: String,
    pub note: String,
    pub input_hash: String,
    pub output_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaxReturn {
    pub id: String,
    pub program: String,
    pub tax_year: i32,
    pub jurisdiction: String,
    pub name: String,
    pub client_ref: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub inputs: Value,
    pub parties: Vec<Party>,
    #[serde(default)] pub corporate: Option<super::corporate::CorporateProfile>,
    pub notes: String,
    pub revision: u64,
    pub input_revision: u64,
    pub status: String,
    pub calculation: Option<CalculationSnapshot>,
    #[serde(default)] pub calculations: Vec<CalculationSnapshot>,
    pub review: Option<Review>,
    #[serde(default)] pub reviews: Vec<Review>,
    pub rollforward_from: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Prior parallel comparisons. Absent on records saved before this field existed.
    #[serde(default)]
    pub parallel_comparisons: Vec<Value>,
    /// Operator-entered prior-year amounts. Empty is omitted. They are not applied to the current calculation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prior_lines: Vec<PriorLine>,
    /// Preparer sign-off recorded when the return enters review; cleared on reopen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparer_signoff: Option<SignOff>,
    /// A human's record that the return was filed OUTSIDE NXT. NXT never transmits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filed_record: Option<FiledRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorLine {
    pub code: String,
    pub cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnInput {
    pub program: String,
    pub tax_year: i32,
    pub jurisdiction: String,
    pub name: String,
    #[serde(default)] pub client_ref: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    #[serde(default = "object")] pub inputs: Value,
    #[serde(default)] pub parties: Vec<Party>,
    #[serde(default)] pub corporate: Option<super::corporate::CorporateProfile>,
    #[serde(default)] pub notes: String,
    pub expected_revision: Option<u64>,
}

impl From<&TaxReturn> for ReturnInput {
    fn from(r: &TaxReturn) -> Self {
        Self { program: r.program.clone(), tax_year: r.tax_year, jurisdiction: r.jurisdiction.clone(), name: r.name.clone(), client_ref: r.client_ref.clone(), period_start: r.period_start.clone(), period_end: r.period_end.clone(), inputs: r.inputs.clone(), parties: r.parties.clone(), corporate:r.corporate.clone(), notes: r.notes.clone(), expected_revision: None }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionInput {
    pub expected_revision: u64,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    pub expected_revision: u64,
    pub status: String,
    #[serde(default)] pub note: String,
    #[serde(default)] pub reviewer: String,
    /// Only for status "filed": the reference a human received when filing outside NXT.
    #[serde(default)] pub filing_reference: String,
    /// Only for status "filed": the src/signatures envelope holding the signed T183/T183CORP.
    #[serde(default)] pub t183_envelope_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollforwardInput {
    pub expected_revision: u64,
    pub tax_year: i32,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub return_id: String,
    pub at: String,
    pub action: String,
    pub detail: String,
    /// Hash chain: each event commits to the previous event's hash. Absent on pre-chain events.
    #[serde(default, skip_serializing_if = "String::is_empty")] pub prev_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignOff { pub by: String, pub at: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportPreview {
    pub id: String,
    pub source_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub summary: Value,
    pub warnings: Vec<String>,
    pub committed_return_id: Option<String>,
    pub document: ReturnInput,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyLink {
    pub id: String,
    pub label: String,
    pub primary_return_id: String,
    pub spouse_return_id: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaxState {
    pub returns: Vec<TaxReturn>,
    pub audit: Vec<AuditEvent>,
    pub imports: Vec<ImportPreview>,
    #[serde(default)]
    pub families: Vec<FamilyLink>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FiledRecord { pub by: String, pub at: String, pub reference: String, pub t183_envelope_id: String }
