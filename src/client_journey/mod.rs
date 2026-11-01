//! NXT Accounting guided client walk: step definitions, gated state machine,
//! progress view, a per-client append-only status ledger, and the value ladder
//! (see `ladder.rs`).
//!
//! Guided client walk principle: the client is walked one step at a time, and
//! every step has one clear action, a visible outcome / value delivered, a gate
//! that must pass before the next step, and a progress indicator. Only legally
//! required steps are human-gated. External hooks (e-sign, filing, payment,
//! offer sending) are interfaces with stubs only.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod capability;
pub mod ladder;
mod routes;
pub mod service;
pub mod sources;
pub use ladder::*;
pub use routes::router;

pub const JOURNEY_JSON: &str = include_str!("journey.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    Auto,
    Human,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDef {
    pub n: u32,
    pub key: String,
    pub phase: String,
    pub title: String,
    /// The one clear action the client (or firm) takes at this step.
    pub action: String,
    /// The visible outcome / value delivered to the client.
    pub outcome: String,
    /// What must be true before the next step can start.
    pub gate_check: String,
    pub gate: Gate,
    #[serde(default)]
    pub human_reason: Option<String>,
    pub skippable: bool,
    pub executor: String,
    /// If set, this is a natural point to surface the next value-ladder rung
    /// above the given tier. Offers always need human approval before sending.
    #[serde(default)]
    pub offer_point_after_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyDef {
    pub journey_id: String,
    pub version: u32,
    pub principle: String,
    pub phases: Vec<String>,
    pub steps: Vec<StepDef>,
}

/// What the client sees: where they are, what to do, what is next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub percent: u32,
    pub phase: Option<String>,
    pub current_step: Option<String>,
    pub current_action: Option<String>,
    pub current_outcome: Option<String>,
    pub gate_check: Option<String>,
    pub next_step: Option<String>,
}

impl JourneyDef {
    pub fn load() -> Result<Self, String> {
        let def: JourneyDef = serde_json::from_str(JOURNEY_JSON).map_err(|e| e.to_string())?;
        def.validate()?;
        Ok(def)
    }
    pub fn validate(&self) -> Result<(), String> {
        for (i, s) in self.steps.iter().enumerate() {
            if s.n as usize != i + 1 {
                return Err(format!("step {} out of order", s.key));
            }
            if s.gate == Gate::Human && s.human_reason.as_deref().unwrap_or("").is_empty() {
                return Err(format!("human gate {} needs a legal reason", s.key));
            }
            for (field, v) in [("action", &s.action), ("outcome", &s.outcome), ("gate_check", &s.gate_check)] {
                if v.trim().is_empty() {
                    return Err(format!("step {} needs a {field}", s.key));
                }
            }
            if !self.phases.contains(&s.phase) {
                return Err(format!("step {} has unknown phase {}", s.key, s.phase));
            }
        }
        Ok(())
    }
    pub fn step(&self, key: &str) -> Option<&StepDef> {
        self.steps.iter().find(|s| s.key == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    InProgress,
    AwaitingApproval,
    Completed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Actor {
    Automation { executor: String },
    Human { name: String, role: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub seq: u64,
    pub step: String,
    pub from: StepStatus,
    pub to: StepStatus,
    pub actor: Actor,
    pub at: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JourneyError {
    UnknownStep(String),
    PriorStepIncomplete { step: String, blocked_by: String },
    InvalidTransition { step: String, from: StepStatus, to: StepStatus },
    HumanApprovalRequired(String),
    NotSkippable(String),
}

/// Per-client journey state plus an append-only status ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientJourney {
    pub client_id: String,
    pub status: BTreeMap<String, StepStatus>,
    pub ledger: Vec<LedgerEntry>,
    /// Append-only value-ladder offer events.
    #[serde(default)]
    pub offers: Vec<OfferEvent>,
}

impl ClientJourney {
    pub fn new(def: &JourneyDef, client_id: &str) -> Self {
        let status = def.steps.iter().map(|s| (s.key.clone(), StepStatus::Pending)).collect();
        Self { client_id: client_id.to_string(), status, ledger: Vec::new(), offers: Vec::new() }
    }

    pub fn status_of(&self, key: &str) -> Option<StepStatus> {
        self.status.get(key).copied()
    }

    /// Next step that is not yet completed or skipped.
    pub fn current_step<'a>(&self, def: &'a JourneyDef) -> Option<&'a StepDef> {
        def.steps.iter().find(|s| {
            !matches!(self.status_of(&s.key), Some(StepStatus::Completed | StepStatus::Skipped))
        })
    }

    pub fn progress(&self, def: &JourneyDef) -> Progress {
        let done = def.steps.iter().filter(|s| matches!(self.status_of(&s.key), Some(StepStatus::Completed | StepStatus::Skipped))).count();
        let total = def.steps.len();
        let cur = self.current_step(def);
        let next = cur.and_then(|c| def.steps.iter().find(|s| s.n > c.n && !matches!(self.status_of(&s.key), Some(StepStatus::Completed | StepStatus::Skipped))));
        Progress {
            done,
            total,
            percent: if total == 0 { 100 } else { (done * 100 / total) as u32 },
            phase: cur.map(|c| c.phase.clone()),
            current_step: cur.map(|c| c.key.clone()),
            current_action: cur.map(|c| c.action.clone()),
            current_outcome: cur.map(|c| c.outcome.clone()),
            gate_check: cur.map(|c| c.gate_check.clone()),
            next_step: next.map(|n| n.key.clone()),
        }
    }

    pub fn is_complete(&self, def: &JourneyDef) -> bool {
        self.current_step(def).is_none()
    }

    fn check_prior(&self, def: &JourneyDef, step: &StepDef) -> Result<(), JourneyError> {
        for prior in def.steps.iter().take_while(|s| s.n < step.n) {
            match self.status_of(&prior.key) {
                Some(StepStatus::Completed | StepStatus::Skipped) => {}
                _ => {
                    return Err(JourneyError::PriorStepIncomplete {
                        step: step.key.clone(),
                        blocked_by: prior.key.clone(),
                    })
                }
            }
        }
        Ok(())
    }

    fn transition(&mut self, def: &JourneyDef, key: &str, to: StepStatus, actor: Actor, at: &str, note: &str) -> Result<(), JourneyError> {
        let step = def.step(key).ok_or_else(|| JourneyError::UnknownStep(key.to_string()))?;
        let from = self.status_of(key).unwrap_or(StepStatus::Pending);
        use StepStatus::*;
        let allowed = match (from, to) {
            (Pending, InProgress) | (Failed, InProgress) | (Pending, Skipped) => true,
            (InProgress, Completed) => step.gate == Gate::Auto,
            (InProgress, AwaitingApproval) => step.gate == Gate::Human,
            (AwaitingApproval, Completed) => true,
            (InProgress, Failed) | (AwaitingApproval, Failed) => true,
            _ => false,
        };
        if !allowed {
            if from == InProgress && to == Completed && step.gate == Gate::Human {
                return Err(JourneyError::HumanApprovalRequired(key.to_string()));
            }
            return Err(JourneyError::InvalidTransition { step: key.to_string(), from, to });
        }
        if matches!(to, InProgress | Skipped) {
            self.check_prior(def, step)?;
        }
        if to == Skipped && !step.skippable {
            return Err(JourneyError::NotSkippable(key.to_string()));
        }
        if from == AwaitingApproval && to == Completed && !matches!(actor, Actor::Human { .. }) {
            return Err(JourneyError::HumanApprovalRequired(key.to_string()));
        }
        self.status.insert(key.to_string(), to);
        let seq = self.ledger.len() as u64 + 1;
        self.ledger.push(LedgerEntry { seq, step: key.to_string(), from, to, actor, at: at.to_string(), note: note.to_string() });
        Ok(())
    }

    pub fn start(&mut self, def: &JourneyDef, key: &str, actor: Actor, at: &str) -> Result<(), JourneyError> {
        self.transition(def, key, StepStatus::InProgress, actor, at, "started")
    }
    /// Auto steps complete; human-gated steps move to AwaitingApproval.
    pub fn finish_work(&mut self, def: &JourneyDef, key: &str, actor: Actor, at: &str) -> Result<StepStatus, JourneyError> {
        let gate = def.step(key).ok_or_else(|| JourneyError::UnknownStep(key.to_string()))?.gate;
        let to = if gate == Gate::Human { StepStatus::AwaitingApproval } else { StepStatus::Completed };
        self.transition(def, key, to, actor, at, "work finished")?;
        Ok(to)
    }
    pub fn approve(&mut self, def: &JourneyDef, key: &str, approver: Actor, at: &str, note: &str) -> Result<(), JourneyError> {
        self.transition(def, key, StepStatus::Completed, approver, at, note)
    }
    pub fn skip(&mut self, def: &JourneyDef, key: &str, actor: Actor, at: &str, reason: &str) -> Result<(), JourneyError> {
        self.transition(def, key, StepStatus::Skipped, actor, at, reason)
    }
    pub fn fail(&mut self, def: &JourneyDef, key: &str, actor: Actor, at: &str, reason: &str) -> Result<(), JourneyError> {
        self.transition(def, key, StepStatus::Failed, actor, at, reason)
    }
}

/// Hook interfaces. Only stubs exist; nothing here sends, charges, or files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    StubOnly { would_do: String },
}

pub trait ESignProvider {
    fn request_signature(&self, client_id: &str, document: &str) -> HookOutcome;
}
pub trait FilingProvider {
    fn prepare_filing(&self, client_id: &str, return_kind: &str) -> HookOutcome;
}
pub trait PaymentProvider {
    fn request_payment(&self, client_id: &str, amount_cents: u64, currency: &str) -> HookOutcome;
}
pub trait MigrationExecutor {
    /// Executed by scripts/autonomous-client-path.py (owned separately).
    fn command_for(&self, client_id: &str) -> Vec<String>;
}

pub struct StubHooks;
impl ESignProvider for StubHooks {
    fn request_signature(&self, client_id: &str, document: &str) -> HookOutcome {
        HookOutcome::StubOnly { would_do: format!("request e-signature of {document} from {client_id}") }
    }
}
impl FilingProvider for StubHooks {
    fn prepare_filing(&self, client_id: &str, return_kind: &str) -> HookOutcome {
        HookOutcome::StubOnly { would_do: format!("stage {return_kind} filing for {client_id}; human preparer submits") }
    }
}
impl PaymentProvider for StubHooks {
    fn request_payment(&self, client_id: &str, amount_cents: u64, currency: &str) -> HookOutcome {
        HookOutcome::StubOnly { would_do: format!("request {amount_cents} {currency} cents from {client_id}") }
    }
}
impl MigrationExecutor for StubHooks {
    fn command_for(&self, client_id: &str) -> Vec<String> {
        vec!["python3".into(), "scripts/autonomous-client-path.py".into(), "--client".into(), client_id.into()]
    }
}
