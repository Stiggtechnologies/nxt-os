//! Tool registry: every tool the engine knows, with its action class, risk and data tier.
//!
//! Hard-gated tools are declared so a planner (or a model) that asks for one is refused
//! and audited by name, but they have **no executor**: nothing in this module can post to
//! the books, file with CRA, write to QuickBooks, contact anyone, move money or reveal an
//! identifier.

use super::guard::Tier;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionClass {
    Read,
    InternalNote,
    InternalTask,
    InternalDraft,
    FirmRecordUpdate,
    JourneyInternalStep,
    // ---- hard gates: never autonomous ----
    BooksWrite,
    QuickbooksPost,
    CraFiling,
    ClientContact,
    OutboundMessage,
    Payment,
    RevealIdentifier,
    RepoProtected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Risk {
    R0,
    R1,
    R2,
    R4,
}

/// How much a class may do without a person, before any standing approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Auto,
    AutoWithNotify,
    NeedsApproval,
    Never,
}

impl ActionClass {
    pub const ALL: [ActionClass; 14] = [
        ActionClass::Read,
        ActionClass::InternalNote,
        ActionClass::InternalTask,
        ActionClass::InternalDraft,
        ActionClass::FirmRecordUpdate,
        ActionClass::JourneyInternalStep,
        ActionClass::BooksWrite,
        ActionClass::QuickbooksPost,
        ActionClass::CraFiling,
        ActionClass::ClientContact,
        ActionClass::OutboundMessage,
        ActionClass::Payment,
        ActionClass::RevealIdentifier,
        ActionClass::RepoProtected,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ActionClass::Read => "read",
            ActionClass::InternalNote => "internal_note",
            ActionClass::InternalTask => "internal_task",
            ActionClass::InternalDraft => "internal_draft",
            ActionClass::FirmRecordUpdate => "firm_record_update",
            ActionClass::JourneyInternalStep => "journey_internal_step",
            ActionClass::BooksWrite => "books_write",
            ActionClass::QuickbooksPost => "quickbooks_post",
            ActionClass::CraFiling => "cra_filing",
            ActionClass::ClientContact => "client_contact",
            ActionClass::OutboundMessage => "outbound_message",
            ActionClass::Payment => "payment",
            ActionClass::RevealIdentifier => "reveal_identifier",
            ActionClass::RepoProtected => "repo_protected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }

    /// Non-negotiable: these classes are never executed by the engine, whatever the policy.
    pub fn is_hard_gate(self) -> bool {
        self.risk() == Risk::R4
    }

    pub fn risk(self) -> Risk {
        match self {
            ActionClass::Read => Risk::R0,
            ActionClass::InternalNote | ActionClass::InternalTask | ActionClass::InternalDraft => Risk::R1,
            ActionClass::FirmRecordUpdate | ActionClass::JourneyInternalStep => Risk::R2,
            _ => Risk::R4,
        }
    }

    pub fn default_level(self) -> Level {
        match self {
            ActionClass::Read | ActionClass::InternalNote => Level::Auto,
            ActionClass::InternalTask | ActionClass::InternalDraft => Level::AutoWithNotify,
            ActionClass::FirmRecordUpdate | ActionClass::JourneyInternalStep => Level::NeedsApproval,
            _ => Level::Never,
        }
    }

    /// Why a hard gate exists, quoted in refusals and the audit log.
    pub fn gate_reason(self) -> &'static str {
        match self {
            ActionClass::BooksWrite => "company books and ledger data are never changed without explicit approval",
            ActionClass::QuickbooksPost => "nothing is posted to QuickBooks autonomously",
            ActionClass::CraFiling => "nothing is filed with or remitted to CRA autonomously",
            ActionClass::ClientContact => "no client contact: T1/T2 intake and the Filing Desk stay internal-staff-only until the first internal test return clears and Orville approves going outward",
            ActionClass::OutboundMessage => "no outbound email or message without Orville approving the wording",
            ActionClass::Payment => "no payments and no Stripe live charging",
            ActionClass::RevealIdentifier => "SINs, keys and full account numbers are never output",
            ActionClass::RepoProtected => "PR #4, .github/workflows, live_cutover_allowed and #145 are never touched",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub class: ActionClass,
    pub risk: Risk,
    /// Highest data tier the tool's input or output can carry.
    pub data_tier: Tier,
    pub reversible: bool,
    /// False for hard-gated tools: declared for refusal only.
    pub executable: bool,
}

const fn tool(name: &'static str, description: &'static str, class: ActionClass, data_tier: Tier, reversible: bool, executable: bool) -> ToolSpec {
    let risk = match class {
        ActionClass::Read => Risk::R0,
        ActionClass::InternalNote | ActionClass::InternalTask | ActionClass::InternalDraft => Risk::R1,
        ActionClass::FirmRecordUpdate | ActionClass::JourneyInternalStep => Risk::R2,
        _ => Risk::R4,
    };
    ToolSpec { name, description, class, risk, data_tier, reversible, executable }
}

/// The phase 1 registry.
pub const REGISTRY: &[ToolSpec] = &[
    tool("read_proposal", "Read the approved proposal and its finding from the God's Eye / Nixon review.", ActionClass::Read, Tier::T2, true, true),
    tool("recheck_proposal", "Confirm the proposal is still approved (not dismissed, rejected or changed).", ActionClass::Read, Tier::T1, true, true),
    tool("add_internal_note", "Attach an internal staff note to the finding. Never shown to a client.", ActionClass::InternalNote, Tier::T2, true, true),
    tool("create_internal_task", "Create an internal staff follow-up task for the finding. Never sent anywhere.", ActionClass::InternalTask, Tier::T2, true, true),
    tool("draft_internal_text", "Save a draft for staff to review. Drafts are never sent.", ActionClass::InternalDraft, Tier::T2, true, true),
    tool("mark_finding_followed_up", "Mark the finding as followed up on the autonomy record (not the books).", ActionClass::FirmRecordUpdate, Tier::T1, true, true),
    // ---- declared hard gates (no executor) ----
    tool("post_journal", "Post a journal entry.", ActionClass::BooksWrite, Tier::T2, false, false),
    tool("void_entry", "Void a ledger entry.", ActionClass::BooksWrite, Tier::T2, false, false),
    tool("categorize_feed", "Categorize bank feed lines.", ActionClass::BooksWrite, Tier::T2, false, false),
    tool("lock_period", "Lock an accounting period.", ActionClass::BooksWrite, Tier::T2, false, false),
    tool("commit_company_books", "Commit a company-books import (incl. autonomous-client-path production promote).", ActionClass::BooksWrite, Tier::T2, false, false),
    tool("quickbooks_post", "Write to QuickBooks.", ActionClass::QuickbooksPost, Tier::T2, false, false),
    tool("file_return", "File a T1/T2/GST return with CRA.", ActionClass::CraFiling, Tier::T3, false, false),
    tool("remit_tax", "Remit tax to CRA.", ActionClass::CraFiling, Tier::T3, false, false),
    tool("notify_client", "Notify a client (portal, email, SMS, document).", ActionClass::ClientContact, Tier::T3, false, false),
    tool("send_email", "Send an email.", ActionClass::OutboundMessage, Tier::T2, false, false),
    tool("send_sms", "Send a text message.", ActionClass::OutboundMessage, Tier::T2, false, false),
    tool("outbound_call", "Place a phone call.", ActionClass::OutboundMessage, Tier::T2, false, false),
    tool("stripe_charge", "Charge a card through Stripe.", ActionClass::Payment, Tier::T2, false, false),
    tool("make_payment", "Pay, refund or transfer money.", ActionClass::Payment, Tier::T2, false, false),
    tool("reveal_identifier", "Output a SIN, key or full account number.", ActionClass::RevealIdentifier, Tier::T3, false, false),
    tool("change_protected_repo", "Touch PR #4, .github/workflows, live_cutover_allowed or merge #145.", ActionClass::RepoProtected, Tier::T0, false, false),
];

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    REGISTRY.iter().find(|t| t.name == name)
}

/// Proposal kinds whose execution would itself be a hard-gated act. Matches the kinds
/// `nixon_finance::models::blocked_proposal` marks as `changes_books` / `files_tax`.
/// Unknown kinds return `None` and the run is refused: the engine never guesses that an
/// unfamiliar proposal is safe.
pub fn proposal_kind_class(kind: &str) -> Option<ActionClass> {
    Some(match kind {
        "post_journal" | "void_entry" | "categorize_feed" | "lock_period" | "commit_company_books" => ActionClass::BooksWrite,
        "remit_tax" | "file_return" => ActionClass::CraFiling,
        "quickbooks_post" => ActionClass::QuickbooksPost,
        "notify_client" | "client_reminder" => ActionClass::ClientContact,
        "send_email" | "send_sms" | "outbound_call" => ActionClass::OutboundMessage,
        "payment" | "stripe_charge" | "refund" => ActionClass::Payment,
        // Nixon `acknowledge` and God's Eye `staff_follow_up` proposals: internal work only.
        "acknowledge" | "staff_follow_up" | "follow_up" | "internal_review" | "investigate" => ActionClass::InternalTask,
        _ => return None,
    })
}
