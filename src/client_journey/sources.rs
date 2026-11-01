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
//! `data_migration` when every declared system is imported. Assessment only
//! reads. The agent actions below write only by rules staff set: DT Max files
//! are linked by the file stems staff confirmed for this client, never guessed.

use super::capability::{ActionSpec, BoxFut, Capability, Readiness, SourceSystem, StepContext};
use crate::{company_books::CompanyBooks, migrations::Migrations, source_import::{SourceImport, files::{ClientLinkInput, ResolveInput}}};
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
    fn actions(&self, _step: &str) -> Vec<ActionSpec> {
        vec![
            ActionSpec {
                name: "list_unmatched_dtmax_files",
                description: "List DT Max files preserved in the client's DT Max company that are not linked to any client yet, with each file's linkage key (file stem).",
                input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            },
            ActionSpec {
                name: "link_dtmax_files_by_confirmed_keys",
                description: "Register the client's staff-confirmed DT Max keys and link every unmatched file whose linkage key is one of them. Files with other keys are left for staff. Safe to repeat.",
                input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            },
            ActionSpec {
                name: "list_quickbooks_previews",
                description: "Show the client's company-books import stage and staged QuickBooks previews with their blockers. Read only.",
                input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            },
        ]
    }
    fn act<'a>(&'a self, ctx: &'a StepContext, name: &'a str, _input: &'a Value) -> BoxFut<'a, Result<Value, String>> {
        Box::pin(async move {
            let company = || ctx.sources.dtmax_company.as_deref().ok_or_else(|| "no DT Max company scope is set for this client".to_string());
            match name {
                "list_unmatched_dtmax_files" => {
                    let v = self.source_import.unmatched_files(company()?).await.map_err(|e| e.1)?;
                    let files: Vec<Value> = v["files"].as_array().cloned().unwrap_or_default().iter()
                        .map(|f| json!({"sha256": f["sha256"], "linkage_key": f["linkage_key"], "year": f["year"], "original_path": f["original_path"]})).collect();
                    Ok(json!({"unmatched": files.len(), "files": files}))
                }
                "link_dtmax_files_by_confirmed_keys" => {
                    let company = company()?;
                    let keys: Vec<String> = ctx.sources.dtmax_keys.iter().map(|k| k.trim().to_ascii_uppercase()).collect();
                    if keys.is_empty() {
                        return Err("staff have not confirmed this client's DT Max keys; ask a human".into());
                    }
                    self.source_import.register_client_links(company, ClientLinkInput { client_id: ctx.client_id.clone(), dtmax_keys: keys.clone() })
                        .await.map_err(|e| e.1)?;
                    let v = self.source_import.unmatched_files(company).await.map_err(|e| e.1)?;
                    let mut linked = Vec::new();
                    for f in v["files"].as_array().cloned().unwrap_or_default() {
                        let (Some(sha), Some(key)) = (f["sha256"].as_str(), f["linkage_key"].as_str()) else { continue };
                        if !keys.iter().any(|k| k == key) { continue; }
                        self.source_import.resolve_file(company, sha, ResolveInput {
                            client_id: ctx.client_id.clone(),
                            reviewer_label: format!("agent:{}", self.id()),
                        }).await.map_err(|e| e.1)?;
                        linked.push(json!({"sha256": sha, "linkage_key": key}));
                    }
                    Ok(json!({"registered_keys": keys, "linked": linked}))
                }
                "list_quickbooks_previews" => {
                    let v = self.company_books.detail(&ctx.client_id).await.map_err(|e| e.1)?;
                    Ok(json!({"stage": v["stage"], "previews": v["previews"]}))
                }
                other => Err(format!("unknown action {other}")),
            }
        })
    }
}
