use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub account_id: String,
    pub debit_cents: String,
    pub credit_cents: String,
    pub source_row: u32,
    pub memo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_metadata: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub date: String,
    pub source_id: String,
    pub memo: String,
    pub lines: Vec<Line>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_metadata: Option<serde_json::Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Chunk {
    pub accounts: Vec<Account>,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Company {
    pub name: String,
    pub source_company_id: String,
    pub currency: String,
    pub identity_verified: bool,
    pub currency_verified: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub system: String,
    pub archive_sha256: String,
    pub journal_sha256: String,
    pub trial_balance_sha256: String,
    pub period_start: String,
    pub period_end: String,
    pub full_history: bool,
    pub record_id_coverage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_lock: Option<SourcePeriodLock>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourcePeriodLock {
    pub verified: bool,
    pub closed_through: Option<String>,
    pub evidence_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub account_id: String,
    pub debit_cents: String,
    pub credit_cents: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<ControlBasis>,
    /// Explicit, labelled amount of this trial-balance row carried by source
    /// entries held for review (never posted). The control must equal posted
    /// activity plus this residual; residuals must net to zero across the book.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_residual: Option<HeldResidual>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HeldResidual {
    pub debit_cents: String,
    pub credit_cents: String,
}
fn is_zero(n: &usize) -> bool {
    *n == 0
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlBasis {
    SourceReportRow {
        report_sha256: String,
        row: u32,
    },
    DerivedZeroJournalOmission {
        journal_sha256: String,
        source_row_count: u32,
        journal_debit_cents: String,
        journal_credit_cents: String,
    },
    /// Multi-year books: an account absent from the as-of trial balance because
    /// QuickBooks closed it at fiscal year ends. Source journal activity and the
    /// labelled derived year-end close lines are proven separately and must net
    /// to zero together.
    DerivedZeroAfterYearEndClose {
        journal_sha256: String,
        source_row_count: u32,
        journal_debit_cents: String,
        journal_credit_cents: String,
        close_line_count: u32,
        close_debit_cents: String,
        close_credit_cents: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    pub request_id: String,
    pub expected_head: Option<String>,
    pub company: Company,
    pub source: Source,
    pub chunks: Vec<String>,
    pub trial_balance: Vec<Control>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Balance {
    pub account_id: String,
    pub name: String,
    pub kind: Option<String>,
    pub debit_cents: String,
    pub credit_cents: String,
    pub journal_debit_cents: String,
    pub journal_credit_cents: String,
    pub source_control_basis: Option<ControlBasis>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Analysis {
    pub accounts: usize,
    pub entries: usize,
    pub lines: usize,
    pub total_debit_cents: String,
    pub total_credit_cents: String,
    pub balances: Vec<Balance>,
    pub blockers: Vec<String>,
    pub classified_reports_ready: bool,
    pub controls_passed: bool,
    pub independent_trial_balance_accounts: usize,
    pub derived_zero_accounts: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub held_residual_accounts: usize,
    pub control_provenance_complete: bool,
    pub chart_of_accounts_complete: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preview {
    pub format: String,
    pub company_id: String,
    pub input: PreviewInput,
    pub analysis: Analysis,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CommitInput {
    pub preview_id: String,
    pub expected_head: Option<String>,
    pub request_id: String,
    pub reviewer_label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Head {
    pub format: String,
    pub company_id: String,
    pub preview_id: String,
    pub revision: u64,
    pub committed_at: String,
    pub commit_request_id: String,
    pub reviewer_label: String,
    pub previous_head: Option<String>,
    pub journal_chunks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_through: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<Operation>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeLine {
    pub account_id: String,
    pub debit_cents: String,
    pub credit_cents: String,
    pub memo: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalInput {
    pub request_id: String,
    pub expected_head: String,
    pub operator_label: String,
    pub date: String,
    pub memo: String,
    pub lines: Vec<NativeLine>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReverseInput {
    pub request_id: String,
    pub expected_head: String,
    pub operator_label: String,
    pub date: String,
    pub memo: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LockInput {
    pub request_id: String,
    pub expected_head: String,
    pub operator_label: String,
    pub locked_through: String,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Journal {
        input: JournalInput,
        journal_sha256: String,
    },
    Reversal {
        input: ReverseInput,
        entry_id: String,
        journal_sha256: String,
    },
    PeriodLock {
        input: LockInput,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeJournal {
    pub format: String,
    pub company_id: String,
    pub id: String,
    pub date: String,
    pub memo: String,
    pub lines: Vec<NativeLine>,
    pub reversal_of: Option<String>,
    pub request_id: String,
}
