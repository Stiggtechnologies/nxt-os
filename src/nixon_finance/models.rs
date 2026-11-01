//! Structured contracts for a Nixon review.
//!
//! These mirror SyncAI's governed capability envelope: every finding has a
//! risk level and evidence, hypotheses are not causes, and execution stays off.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How a figure entered the review. Ledger reads are always `workspace_books`.
/// A packet has to name one of the other three; Nixon will not guess.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    WorkspaceBooks,
    LiveExternal,
    Simulated,
    CustomerOperational,
}

impl SourceClass {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceClass::WorkspaceBooks => "workspace_books",
            SourceClass::LiveExternal => "live_external",
            SourceClass::Simulated => "simulated",
            SourceClass::CustomerOperational => "customer_operational",
        }
    }

    /// Packets must say where they came from. The live books are not a packet.
    pub fn allowed_on_packet(self) -> bool {
        !matches!(self, SourceClass::WorkspaceBooks)
    }
}

/// Screening constants for the expense and income shift check.
/// These are Nixon's screen, not a budget, a management limit, or a tax rule.
pub const SHIFT_SCREEN: &str = "expense-shift-v1";
pub const SHIFT_MIN_CENTS: i64 = 50_000;
pub const SHIFT_MIN_RATIO_BP: i64 = 5_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxAccount {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub role: String,
    pub debit_normal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxLine {
    pub account_id: String,
    pub debit: i64,
    pub credit: i64,
    pub cleared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxEntry {
    pub id: String,
    pub date: String,
    pub memo: String,
    pub source: String,
    pub source_id: Option<String>,
    pub voided: bool,
    pub lines: Vec<CtxLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxFeed {
    pub id: String,
    pub account_id: String,
    pub date: String,
    pub description: String,
    pub amount: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxRecon {
    pub id: String,
    pub account_id: String,
    pub statement_date: String,
    pub statement_balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxDoc {
    pub id: String,
    pub number: String,
    pub kind: String,
    pub entry_id: String,
    pub voided: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtxPayment {
    pub id: String,
    pub entry_id: String,
    pub voided: bool,
}

/// Read-only projection of the workspace books. Nixon never writes this back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceContext {
    pub as_of: String,
    pub currency: String,
    pub company_name: String,
    pub closing_date: Option<String>,
    pub source_class: SourceClass,
    pub accounts: Vec<CtxAccount>,
    pub entries: Vec<CtxEntry>,
    pub feed: Vec<CtxFeed>,
    pub reconciliations: Vec<CtxRecon>,
    pub docs: Vec<CtxDoc>,
    pub payments: Vec<CtxPayment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutoverLine {
    pub account_code: String,
    #[serde(default)]
    pub debit: i64,
    #[serde(default)]
    pub credit: i64,
}

/// Source trial balance submitted for a migration check. Not a posting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutoverPacket {
    pub source_class: SourceClass,
    pub as_of: String,
    pub currency: String,
    #[serde(default)]
    pub period_lock: Option<String>,
    pub prepared_by: String,
    #[serde(default)]
    pub label: String,
    pub lines: Vec<CutoverLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceItem {
    pub id: String,
    pub label: String,
    /// `complete`, `open`, `blocked`, or `unreviewed`. Other values stay visible.
    pub state: String,
    #[serde(default)]
    pub note: String,
}

/// Read-only Firm or Tax checklist. Nixon does not complete tasks or file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfacePacket {
    /// `firm` or `tax`.
    pub surface: String,
    pub source_class: SourceClass,
    pub as_of: String,
    pub prepared_by: String,
    #[serde(default)]
    pub filing_requested: bool,
    #[serde(default)]
    pub items: Vec<SurfaceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    /// measured, calculated, documented, or ai_inference.
    pub class: String,
    /// verified_measurement, confirmed_history, calculation, or ai_hypothesis.
    pub level: String,
    /// supporting, contradicting, or context.
    pub role: String,
    pub source_system: String,
    pub reference: String,
    pub source_date: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gap {
    pub id: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    /// Always `hypothesis`. Nixon does not promote this to a cause.
    pub label: String,
    pub statement: String,
    pub confidence: Option<f64>,
    pub confidence_refusal: String,
    pub evidence_ids: Vec<String>,
    pub gap_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub finding_id: Option<String>,
    pub kind: String,
    pub changes_books: bool,
    pub files_tax: bool,
    /// Always `disabled` in this module.
    pub execution: String,
    /// Always false. Approval does not apply the proposal.
    pub applied: bool,
    pub status: String,
    pub summary: String,
    pub prepared_by: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub capability: String,
    pub risk_level: String,
    pub title: String,
    pub summary: String,
    pub requires_human_review: bool,
    pub autonomy_level: String,
    pub evidence: Vec<Evidence>,
    pub hypotheses: Vec<Hypothesis>,
    pub gaps: Vec<Gap>,
    pub proposal_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    pub id: String,
    pub name: String,
    /// passed, failed, not_run, or refused.
    pub state: String,
    pub detail: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub proposal_id: String,
    pub actor_name: String,
    pub decision: String,
    pub note: String,
    pub at: String,
    pub execution: String,
    /// Approval is not verification.
    pub verification_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub findings: Vec<Finding>,
    pub proposals: Vec<Proposal>,
    pub verifications: Vec<Verification>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub correlation_id: String,
    pub created_at: String,
    pub as_of: String,
    pub currency: String,
    pub source_class: SourceClass,
    pub digest: String,
    pub idempotency_key: Option<String>,
    pub autonomy_level: String,
    pub execution_mode: String,
    pub posting: String,
    pub filing: String,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub proposals: Vec<Proposal>,
    pub verifications: Vec<Verification>,
    pub decisions: Vec<Decision>,
    pub cutover: Option<CutoverPacket>,
    pub surfaces: Vec<SurfacePacket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateRecord {
    pub id: String,
    pub correlation_id: String,
    pub created_at: String,
    pub kind: String,
    pub actor_name: String,
    pub note: String,
    pub autonomy_requested: String,
    pub autonomy_applied: String,
    pub execution: String,
    pub applied: bool,
    pub status: String,
    pub changes_books: bool,
    pub files_tax: bool,
    pub payload: Value,
}

pub fn acknowledge_proposal(finding_id: &str, summary: &str, prepared_by: &str) -> Proposal {
    Proposal {
        id: format!("p_{finding_id}"),
        finding_id: Some(finding_id.to_string()),
        kind: "acknowledge".into(),
        changes_books: false,
        files_tax: false,
        execution: "disabled".into(),
        applied: false,
        status: "pending_approval".into(),
        summary: summary.into(),
        prepared_by: prepared_by.into(),
        payload: serde_json::json!({
            "effect": "none",
            "note": "A decision records the named person. It does not post, file, or edit the books."
        }),
    }
}

pub fn blocked_proposal(
    id: &str,
    kind: &str,
    summary: &str,
    prepared_by: &str,
    payload: Value,
) -> Proposal {
    let files_tax = kind == "file_return";
    let changes_books = matches!(
        kind,
        "post_journal" | "void_entry" | "categorize_feed" | "lock_period" | "remit_tax"
    );
    Proposal {
        id: id.into(),
        finding_id: None,
        kind: kind.into(),
        changes_books,
        files_tax,
        execution: "disabled".into(),
        applied: false,
        status: "blocked".into(),
        summary: summary.into(),
        prepared_by: prepared_by.into(),
        payload,
    }
}
