//! Automated NXT Accounting client journey: step definitions, gated state
//! machine, and a per-client status ledger.
//!
//! The step list adapts the AIMOS "CVI 20/40" guided-process pattern. That
//! process could not be located in Stiggtechnologies/AIMOS, so the 20-step
//! structure (each step = execute + verify, 40 sub-steps) is an assumption.
//! Only legally required steps are human-gated; everything else is automatable.
//! External hooks (e-sign, filing, payment) are interfaces with stubs only.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    pub cvi_step: String,
    pub title: String,
    pub gate: Gate,
    #[serde(default)]
    pub human_reason: Option<String>,
    pub skippable: bool,
    pub executor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyDef {
    pub journey_id: String,
    pub version: u32,
    pub source_pattern: String,
    pub steps: Vec<StepDef>,
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
}

impl ClientJourney {
    pub fn new(def: &JourneyDef, client_id: &str) -> Self {
        let status = def.steps.iter().map(|s| (s.key.clone(), StepStatus::Pending)).collect();
        Self { client_id: client_id.to_string(), status, ledger: Vec::new() }
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
