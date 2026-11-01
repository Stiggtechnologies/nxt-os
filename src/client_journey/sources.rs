//! Source-software capability: the `books_access` and `data_migration` steps.
//!
//! For each system the client declared (QuickBooks Online/Desktop, DT Max,
//! Caseware, WebTax, DocuSign, payroll) it reads what the existing importers
//! hold for this client and reports one state per system:
//! - `received`: source records are in NXT but not yet the client's books
//!   (a staged QuickBooks preview, or a published QuickBooks snapshot);
//! - `imported`: the records are committed and linked to the client
//!   (committed company books, or DT Max files linked to the client);
//! - `missing`: nothing yet;
//! - `not_automated`: NXT has no importer for this system yet, so staff
//!   confirm it by hand. The lane and its status come from source-import coverage.
//!
//! `books_access` is ready when every declared system is received or imported;
//! `data_migration` when every declared system is imported. Nothing here writes.

use super::capability::{BoxFut, Capability, Readiness, SourceSystem, StepContext};
use crate::{company_books::CompanyBooks, migrations::Migrations, source_import::SourceImport};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct SourceSoftwareImport {
    pub company_books: Arc<CompanyBooks>,
    pub migrations: Arc<Migrations>,
    pub source_import: Arc<SourceImport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    NotAutomated,
    Missing,
    Received,
    Imported,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::NotAutomated => "not_automated",
            State::Missing => "missing",
            State::Received => "received",
            State::Imported => "imported",
        }
    }
}

struct SystemStatus {
    system: SourceSystem,
    state: State,
    detail: String,
}

/// Preservation lane for systems that have no client-level importer yet.
fn unautomated_lane(system: SourceSystem) -> Option<&'static str> {
    match system {
        SourceSystem::Caseware => Some("caseware_engagements"),
        SourceSystem::Webtax => Some("webtax_t2"),
        SourceSystem::Docusign => Some("docusign_signatures"),
        SourceSystem::Payroll => Some("quickbooks_payroll"),
        _ => None,
    }
}

impl SourceSoftwareImport {
    async fn quickbooks(&self, client: &str) -> (State, String) {
        let stage = match self.company_books.detail(client).await {
            Ok(v) => v["stage"].as_str().unwrap_or("").to_string(),
            Err(e) => return (State::Missing, format!("company books unavailable: {}", e.1)),
        };
        match stage.as_str() {
            "import_committed" => return (State::Imported, "company books committed".into()),
            "staged" => return (State::Received, "company books staged; commit the reviewed preview".into()),
            _ => {}
        }
        let snapshots = match self.migrations.list().await {
            Ok(v) => v["snapshots"].as_array().cloned().unwrap_or_default(),
            Err(e) => return (State::Missing, format!("migration snapshots unavailable: {}", e.1)),
        };
        let mine = snapshots.iter().filter(|s| s["cohort_ref"].as_str() == Some(client)).count();
        if mine > 0 {
            (State::Received, format!("{mine} QuickBooks snapshot(s) published; stage and commit company books"))
        } else {
            (State::Missing, "no QuickBooks export received for this client".into())
        }
    }

    async fn dtmax(&self, client: &str, company: Option<&str>) -> (State, String) {
        let Some(company) = company else {
            return (State::Missing, "set sources.dtmax_company to the DT Max company scope".into());
        };
        match self.source_import.client_files(company, client).await {
            Ok(v) => match v["files"].as_array().map_or(0, Vec::len) {
                0 => (State::Missing, format!("no DT Max files linked to this client in {company}")),
                n => (State::Imported, format!("{n} DT Max file(s) preserved and linked")),
            },
            Err(e) => (State::Missing, format!("DT Max files unavailable: {}", e.1)),
        }
    }

    async fn statuses(&self, ctx: &StepContext) -> Vec<SystemStatus> {
        let coverage = SourceImport::coverage();
        let mut out = Vec::new();
        for &system in &ctx.sources.systems {
            let (state, detail) = match system {
                SourceSystem::QuickbooksOnline | SourceSystem::QuickbooksDesktop => self.quickbooks(&ctx.client_id).await,
                SourceSystem::Dtmax => self.dtmax(&ctx.client_id, ctx.sources.dtmax_company.as_deref()).await,
                other => {
                    let lane = unautomated_lane(other).unwrap_or("none");
                    let status = coverage["lanes"].as_array().and_then(|lanes| lanes.iter().find(|l| l["lane"] == lane))
                        .and_then(|l| l["importer_status"].as_str()).unwrap_or("unknown").to_string();
                    (State::NotAutomated, format!("no client importer yet (lane {lane}: {status}); staff confirm by hand"))
                }
            };
            out.push(SystemStatus { system, state, detail });
        }
        out
    }
}

fn evidence(statuses: &[SystemStatus]) -> Value {
    json!({"systems": statuses.iter().map(|s| json!({
        "system": s.system,
        "label": s.system.label(),
        "state": s.state.name(),
        "detail": s.detail,
    })).collect::<Vec<_>>()})
}

impl Capability for SourceSoftwareImport {
    fn id(&self) -> &'static str {
        "source_software_import"
    }
    fn steps(&self) -> &'static [&'static str] {
        &["books_access", "data_migration"]
    }
    fn assess<'a>(&'a self, ctx: &'a StepContext) -> BoxFut<'a, Readiness> {
        Box::pin(async move {
            if ctx.sources.systems.is_empty() {
                return if ctx.step == "data_migration" {
                    Readiness::Skip { reason: "client declared no prior software".into() }
                } else {
                    Readiness::Blocked { reasons: vec!["declare the client's current software first".into()], evidence: json!({"systems": []}) }
                };
            }
            let statuses = self.statuses(ctx).await;
            let needed = if ctx.step == "data_migration" { State::Imported } else { State::Received };
            let reasons: Vec<String> = statuses.iter().filter(|s| s.state < needed)
                .map(|s| format!("{}: {}", s.system.label(), s.detail)).collect();
            if reasons.is_empty() {
                Readiness::Ready { evidence: evidence(&statuses) }
            } else {
                Readiness::Blocked { reasons, evidence: evidence(&statuses) }
            }
        })
    }
}
