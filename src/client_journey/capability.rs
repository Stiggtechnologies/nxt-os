//! Capability contract: how a module does the work of a journey step.
//!
//! The journey owns order, gates and the ledger. A capability only answers
//! "for this client, is this step's work done, and what is the evidence?".
//! It never moves the journey itself and never bypasses a human gate: a
//! `Ready` answer on a human-gated step only brings it to awaiting approval.
//! Steps with no capability stay manual until staff record them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Software the client (or the firm, for this client) uses today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSystem {
    QuickbooksOnline,
    QuickbooksDesktop,
    Dtmax,
    Caseware,
    Webtax,
    Docusign,
    Payroll,
}

impl SourceSystem {
    pub const ALL: [SourceSystem; 7] = [
        SourceSystem::QuickbooksOnline,
        SourceSystem::QuickbooksDesktop,
        SourceSystem::Dtmax,
        SourceSystem::Caseware,
        SourceSystem::Webtax,
        SourceSystem::Docusign,
        SourceSystem::Payroll,
    ];
    pub fn label(self) -> &'static str {
        match self {
            SourceSystem::QuickbooksOnline => "QuickBooks Online",
            SourceSystem::QuickbooksDesktop => "QuickBooks Desktop",
            SourceSystem::Dtmax => "DT Max",
            SourceSystem::Caseware => "Caseware",
            SourceSystem::Webtax => "WebTax",
            SourceSystem::Docusign => "DocuSign",
            SourceSystem::Payroll => "Payroll",
        }
    }
}

/// Where this client's records live in the software the firm uses now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProfile {
    #[serde(default)]
    pub systems: Vec<SourceSystem>,
    /// The DT Max company scope this client's tax files are preserved under.
    #[serde(default)]
    pub dtmax_company: Option<String>,
}

pub struct StepContext {
    pub client_id: String,
    pub step: String,
    pub sources: SourceProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Readiness {
    /// The step's work is done; `evidence` is recorded in the journey.
    Ready { evidence: Value },
    /// The step does not apply to this client (only honoured for skippable steps).
    Skip { reason: String },
    /// Not done yet; each reason names what is missing.
    Blocked { reasons: Vec<String>, evidence: Value },
}

pub trait Capability: Send + Sync {
    fn id(&self) -> &'static str;
    /// Journey step keys this capability does the work for.
    fn steps(&self) -> &'static [&'static str];
    fn assess<'a>(&'a self, ctx: &'a StepContext) -> BoxFut<'a, Readiness>;
}

#[derive(Default, Clone)]
pub struct Capabilities {
    list: Vec<Arc<dyn Capability>>,
}

impl Capabilities {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, capability: Arc<dyn Capability>) -> Self {
        self.list.push(capability);
        self
    }
    pub fn for_step(&self, step: &str) -> Option<&Arc<dyn Capability>> {
        self.list.iter().find(|c| c.steps().contains(&step))
    }
}
