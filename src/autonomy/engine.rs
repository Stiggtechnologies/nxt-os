//! The run engine: durable run state, planner → policy → executor → verifier loop,
//! idempotency, rollback, budgets, kill switch and audit.

use super::guard::{effective_tier, redact, redact_json};
use super::policy;
use super::tools::{self, ActionClass, Level, ToolSpec};
use crate::agent::Model;
use crate::client_journey::capability::BoxFut;
use crate::ledger::engine::{Ledger, now};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tries per step before the run is handed to a person (act, verify, adjust, retry once).
pub const MAX_ATTEMPTS: u32 = 2;
const AUDIT_KEY: &str = "autonomy:audit";
const AUDIT_CAP: usize = 10_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Budget {
    pub max_steps: u32,
    pub max_wall_ms: u64,
    /// Estimated model spend, in micro-USD (1,000,000 = US$1).
    pub max_cost_micro_usd: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self { max_steps: 12, max_wall_ms: 120_000, max_cost_micro_usd: 50_000 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Spent {
    pub steps: u32,
    pub cost_micro_usd: u64,
    pub wall_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    VerifiedDone,
    NeedsHuman,
    Refused,
    BudgetExhausted,
    Killed,
    Failed,
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, RunStatus::Running | RunStatus::NeedsHuman)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub n: u32,
    pub tool: String,
    pub action_class: ActionClass,
    pub idempotency_key: String,
    pub input: Value,
    /// ok, verified, replayed, refused, verify_failed, rolled_back, failed.
    pub outcome: String,
    pub detail: String,
    pub attempts: u32,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub review_id: String,
    pub proposal_id: String,
    pub proposal_kind: String,
    pub action_class: ActionClass,
    /// Who approved the proposal through Nixon's `decide` (the approval this run executes).
    pub approved_by: String,
    pub started_by: String,
    pub status: RunStatus,
    pub reason: String,
    pub budget: Budget,
    pub spent: Spent,
    pub started_ms: i64,
    pub created_at: String,
    pub updated_at: String,
    pub planner: String,
    pub steps: Vec<Step>,
    pub notifications: Vec<String>,
}

impl Run {
    pub fn done(&self, tool: &str) -> bool {
        self.steps.iter().any(|s| s.tool == tool && matches!(s.outcome.as_str(), "verified" | "replayed"))
    }
}

#[derive(Debug, Clone)]
pub struct AutoError {
    pub status: u16,
    pub message: String,
}

impl AutoError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

impl From<String> for AutoError {
    fn from(m: String) -> Self {
        Self::new(500, m)
    }
}

#[derive(Debug, Clone)]
pub struct PlannedStep {
    pub tool: String,
    pub input: Value,
    pub cost_micro_usd: u64,
}

/// Picks the next step from the run's memory. `Ok(None)` means the plan is complete.
pub trait Planner: Send + Sync {
    fn name(&self) -> &'static str;
    fn next<'a>(&'a self, run: &'a Run, proposal: &'a Value) -> BoxFut<'a, Result<Option<PlannedStep>, String>>;
}

/// Phase 1: a fixed plan per proposal class. Needs no model.
pub struct Deterministic;

impl Planner for Deterministic {
    fn name(&self) -> &'static str {
        "deterministic"
    }
    fn next<'a>(&'a self, run: &'a Run, proposal: &'a Value) -> BoxFut<'a, Result<Option<PlannedStep>, String>> {
        Box::pin(async move {
            let summary = proposal["summary"].as_str().unwrap_or("").chars().take(500).collect::<String>();
            let plan: Vec<(&str, Value)> = vec![
                ("read_proposal", json!({})),
                ("add_internal_note", json!({"text": format!("Autonomy is following up on an approved proposal: {summary}")})),
                ("create_internal_task", json!({"title": format!("Follow up: {summary}"), "assignee_role": "Reviewer"})),
                ("mark_finding_followed_up", json!({})),
                ("recheck_proposal", json!({})),
            ];
            Ok(plan.into_iter().find(|(t, _)| !run.done(t)).map(|(t, input)| PlannedStep { tool: t.into(), input, cost_micro_usd: 0 }))
        })
    }
}

/// Phase 2: a model (DeepSeek on ZDR hosts via the gateway) proposes the next tool. It is
/// offered only tools that are executable; hard gates are not offered, and are refused if
/// named anyway. The prompt is built from redacted run memory.
pub struct ModelPlanner {
    pub model: Arc<dyn Model>,
}

const PLANNER_PROMPT: &str = "You are the NXT OS autonomy planner. An authorized person approved the proposal below. \
Choose the single next tool that carries it out using only internal NXT records, or reply with plain text DONE when \
the work is complete and verified. You cannot change the books, post to QuickBooks, file with CRA, contact a client, \
send any message, move money or reveal identifiers. If something needs a person, reply with plain text ESCALATE and a reason.";

impl Planner for ModelPlanner {
    fn name(&self) -> &'static str {
        "model"
    }
    fn next<'a>(&'a self, run: &'a Run, proposal: &'a Value) -> BoxFut<'a, Result<Option<PlannedStep>, String>> {
        Box::pin(async move {
            let offered: Vec<Value> = tools::REGISTRY
                .iter()
                .filter(|t| t.executable)
                .map(|t| json!({"name": t.name, "description": t.description, "input_schema": {"type": "object"}}))
                .collect();
            let memory: Vec<Value> = run.steps.iter().map(|s| json!({"tool": s.tool, "outcome": s.outcome, "detail": s.detail})).collect();
            let context = redact_json(&json!({"proposal": {"kind": proposal["kind"], "summary": proposal["summary"]}, "done": memory}));
            let messages = vec![json!({"role": "user", "content": context.to_string()})];
            let reply = self.model.respond(PLANNER_PROMPT, &offered, &messages).await.map_err(|e| format!("planner model failed: {e:?}"))?;
            let usage = &reply["usage"];
            let cost = (usage["input_tokens"].as_u64().unwrap_or(0) * super::gateway::PRICE_IN_TENTHS
                + usage["output_tokens"].as_u64().unwrap_or(0) * super::gateway::PRICE_OUT_TENTHS)
                / 10;
            let content = reply["content"].as_array().cloned().unwrap_or_default();
            if let Some(call) = content.iter().find(|c| c["type"] == "tool_use") {
                return Ok(Some(PlannedStep { tool: call["name"].as_str().unwrap_or("").into(), input: call["input"].clone(), cost_micro_usd: cost }));
            }
            let text: String = content.iter().filter_map(|c| c["text"].as_str()).collect();
            if text.trim_start().starts_with("ESCALATE") {
                return Err(format!("planner escalated: {}", redact(text.trim())));
            }
            Ok(None)
        })
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// `NXT_AUTONOMOUS_EXECUTION=on`. Off by default.
    pub enabled: bool,
    /// `NXT_AUTONOMY_KILL=on` stops every run regardless of the store flag.
    pub kill_env: bool,
    pub budget: Budget,
    pub ai: Option<super::gateway::GatewayConfig>,
}

impl Config {
    pub fn from_env() -> Self {
        let on = |k: &str| std::env::var(k).is_ok_and(|v| v.trim().eq_ignore_ascii_case("on"));
        Self { enabled: on("NXT_AUTONOMOUS_EXECUTION"), kill_env: on("NXT_AUTONOMY_KILL"), budget: Budget::default(), ai: super::gateway::GatewayConfig::from_env() }
    }
    /// Everything off: the production default when no env var is set.
    pub fn off() -> Self {
        Self { enabled: false, kill_env: false, budget: Budget::default(), ai: None }
    }
}

pub struct Autonomy {
    pub ledger: Arc<Ledger>,
    pub config: Config,
    planner: Arc<dyn Planner>,
    /// Eval harness only: make a tool's verification fail this many times.
    faults: Mutex<HashMap<String, u32>>,
}

fn store_err(m: String) -> AutoError {
    AutoError::new(500, m)
}

fn idem_key(run_id: &str, tool: &str, input: &Value) -> String {
    let mut h = Sha256::new();
    h.update(run_id.as_bytes());
    h.update(b"|");
    h.update(tool.as_bytes());
    h.update(b"|");
    h.update(serde_json::to_string(input).unwrap_or_default().as_bytes());
    hex::encode(h.finalize())[..32].to_string()
}

impl Autonomy {
    pub fn new(ledger: Arc<Ledger>, config: Config) -> Self {
        let planner: Arc<dyn Planner> = match &config.ai {
            Some(ai) => Arc::new(ModelPlanner { model: Arc::new(super::gateway::GatewayModel::new(ai.clone())) }),
            None => Arc::new(Deterministic),
        };
        Self { ledger, config, planner, faults: Mutex::new(HashMap::new()) }
    }

    pub fn with_planner(mut self, planner: Arc<dyn Planner>) -> Self {
        self.planner = planner;
        self
    }

    #[doc(hidden)]
    pub fn inject_verify_faults(&self, tool: &str, times: u32) {
        self.faults.lock().unwrap().insert(tool.into(), times);
    }

    fn store(&self) -> &Store {
        &self.ledger.store
    }

    pub async fn audit(&self, action: &str, detail: Value) -> Result<(), AutoError> {
        let entry = json!({"at": now(), "action": action, "detail": redact_json(&detail)});
        self.store().lpush_capped(AUDIT_KEY, &entry.to_string(), AUDIT_CAP).await.map_err(store_err)?;
        let line = redact(&detail.to_string());
        self.ledger.audit(&format!("autonomy.{action}"), line).await.map_err(|e| AutoError::new(500, format!("{e:?}")))?;
        Ok(())
    }

    pub async fn audit_log(&self, limit: usize) -> Result<Vec<Value>, AutoError> {
        self.store().list_json(AUDIT_KEY, limit).await.map_err(store_err)
    }

    pub async fn killed(&self) -> Result<bool, AutoError> {
        Ok(self.config.kill_env || self.store().get("autonomy:kill").await.map_err(store_err)?.as_deref() == Some("on"))
    }

    pub async fn set_kill(&self, on: bool, actor: &str, reason: &str) -> Result<Value, AutoError> {
        self.store().set("autonomy:kill", if on { "on" } else { "off" }).await.map_err(store_err)?;
        self.audit(if on { "kill_switch_on" } else { "kill_switch_off" }, json!({"actor": actor, "reason": reason})).await?;
        Ok(json!({"kill_switch": on, "env_kill": self.config.kill_env}))
    }

    async fn load_review(&self, review_id: &str) -> Result<Value, AutoError> {
        self.store()
            .get_json::<Value>(&format!("nixon:review:{review_id}"))
            .await
            .map_err(store_err)?
            .ok_or_else(|| AutoError::new(404, format!("review '{review_id}' not found")))
    }

    async fn load_proposal(&self, review_id: &str, proposal_id: &str) -> Result<(Value, Value), AutoError> {
        let review = self.load_review(review_id).await?;
        let proposal = review["proposals"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["id"] == proposal_id))
            .cloned()
            .ok_or_else(|| AutoError::new(404, "proposal not found"))?;
        Ok((review, proposal))
    }

    pub async fn save(&self, run: &Run) -> Result<(), AutoError> {
        self.store().set_json(&format!("autonomy:run:{}", run.id), run).await.map_err(store_err)
    }

    pub async fn get_run(&self, id: &str) -> Result<Run, AutoError> {
        self.store().get_json(&format!("autonomy:run:{id}")).await.map_err(store_err)?.ok_or_else(|| AutoError::new(404, "run not found"))
    }

    pub async fn list_runs(&self) -> Result<Vec<Value>, AutoError> {
        let mut out = Vec::new();
        for id in self.store().smembers("autonomy:runs").await.map_err(store_err)? {
            if let Ok(r) = self.get_run(&id).await {
                out.push(json!({"id": r.id, "review_id": r.review_id, "proposal_id": r.proposal_id, "status": r.status, "reason": r.reason, "steps": r.steps.len(), "updated_at": r.updated_at}));
            }
        }
        out.sort_by(|a, b| b["updated_at"].as_str().cmp(&a["updated_at"].as_str()));
        Ok(out)
    }

    /// Starts (or resumes) the one run for an approved proposal.
    pub async fn start(&self, review_id: &str, proposal_id: &str, started_by: &str) -> Result<Run, AutoError> {
        if !self.config.enabled {
            return Err(AutoError::new(409, "autonomous execution is off (NXT_AUTONOMOUS_EXECUTION is not on)"));
        }
        if self.killed().await? {
            return Err(AutoError::new(423, "the autonomy kill switch is on"));
        }
        let started_by = crate::nixon_finance::service::require_person(started_by).map_err(|e| AutoError::new(e.status, e.message))?;
        let (review, proposal) = self.load_proposal(review_id, proposal_id).await?;
        let kind = proposal["kind"].as_str().unwrap_or("").to_string();
        let refuse = |why: String| async move {
            self.audit("run_refused", json!({"review": review_id, "proposal": proposal_id, "kind": kind, "reason": why})).await?;
            Err::<Run, AutoError>(AutoError::new(403, why))
        };
        if proposal["status"] != "approved_no_execution" {
            return refuse(format!("only approved proposals run; this one is {}", proposal["status"].as_str().unwrap_or("unknown"))).await;
        }
        if proposal["changes_books"] == true {
            return refuse(ActionClass::BooksWrite.gate_reason().into()).await;
        }
        if proposal["files_tax"] == true {
            return refuse(ActionClass::CraFiling.gate_reason().into()).await;
        }
        let Some(class) = tools::proposal_kind_class(proposal["kind"].as_str().unwrap_or("")) else {
            return refuse(format!("proposal kind '{}' is not one autonomy knows; a person handles it", proposal["kind"].as_str().unwrap_or(""))).await;
        };
        if class.is_hard_gate() {
            return refuse(class.gate_reason().into()).await;
        }
        let approved_by = review["decisions"]
            .as_array()
            .and_then(|ds| ds.iter().rev().find(|d| d["proposal_id"] == proposal_id && d["decision"] == "approve"))
            .and_then(|d| d["actor_name"].as_str())
            .map(String::from);
        let Some(approved_by) = approved_by else {
            return refuse("no named approval is recorded for this proposal".into()).await;
        };
        let run_id = format!("run_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let claim = format!("autonomy:run-for:{review_id}:{proposal_id}");
        if !self.store().set_nx(&claim, &run_id).await.map_err(store_err)? {
            let existing = self.store().get(&claim).await.map_err(store_err)?.unwrap_or_default();
            self.audit("run_replayed", json!({"run": existing, "review": review_id, "proposal": proposal_id})).await?;
            return self.drive(&existing).await;
        }
        let at = now();
        let run = Run {
            id: run_id.clone(),
            review_id: review_id.into(),
            proposal_id: proposal_id.into(),
            proposal_kind: proposal["kind"].as_str().unwrap_or("").into(),
            action_class: class,
            approved_by: approved_by.clone(),
            started_by: started_by.clone(),
            status: RunStatus::Running,
            reason: String::new(),
            budget: self.config.budget,
            spent: Spent::default(),
            started_ms: chrono::Utc::now().timestamp_millis(),
            created_at: at.clone(),
            updated_at: at,
            planner: self.planner.name().into(),
            steps: vec![],
            notifications: vec![],
        };
        self.save(&run).await?;
        self.store().sadd("autonomy:runs", &run_id).await.map_err(store_err)?;
        self.audit("run_started", json!({"run": run_id, "review": review_id, "proposal": proposal_id, "approved_by": approved_by, "started_by": started_by, "planner": run.planner})).await?;
        self.drive(&run_id).await
    }

    /// Resumes a run that is waiting on a person (e.g. after a standing approval is granted).
    pub async fn resume(&self, run_id: &str, actor: &str) -> Result<Run, AutoError> {
        let mut run = self.get_run(run_id).await?;
        if run.status != RunStatus::NeedsHuman {
            return Err(AutoError::new(409, "only a run waiting on a person can be resumed"));
        }
        let actor = crate::nixon_finance::service::require_person(actor).map_err(|e| AutoError::new(e.status, e.message))?;
        run.status = RunStatus::Running;
        run.reason.clear();
        // A resume gets a fresh wall-clock window; steps and cost keep counting.
        run.started_ms = chrono::Utc::now().timestamp_millis();
        self.save(&run).await?;
        self.audit("run_resumed", json!({"run": run_id, "actor": actor})).await?;
        self.drive(run_id).await
    }

    async fn finish(&self, run: &mut Run, status: RunStatus, reason: impl Into<String>) -> Result<(), AutoError> {
        run.status = status;
        run.reason = redact(&reason.into());
        run.updated_at = now();
        self.save(run).await?;
        self.audit("run_status", json!({"run": run.id, "status": status, "reason": run.reason, "steps": run.spent.steps, "cost_micro_usd": run.spent.cost_micro_usd})).await
    }

    /// The act → verify → adjust loop. Saves the run after every step.
    pub async fn drive(&self, run_id: &str) -> Result<Run, AutoError> {
        let mut run = self.get_run(run_id).await?;
        if run.status != RunStatus::Running {
            return Ok(run);
        }
        // Hard upper bound independent of the budget, so a bug cannot loop forever.
        for _ in 0..64 {
            if self.killed().await? {
                self.finish(&mut run, RunStatus::Killed, "kill switch is on").await?;
                return Ok(run);
            }
            if !self.config.enabled {
                self.finish(&mut run, RunStatus::Refused, "autonomous execution is off").await?;
                return Ok(run);
            }
            let budget = run.budget;
            run.spent.wall_ms = (chrono::Utc::now().timestamp_millis() - run.started_ms).max(0) as u64;
            if run.spent.steps >= run.budget.max_steps {
                self.finish(&mut run, RunStatus::BudgetExhausted, format!("step budget of {} used", budget.max_steps)).await?;
                return Ok(run);
            }
            if run.spent.wall_ms > run.budget.max_wall_ms {
                self.finish(&mut run, RunStatus::BudgetExhausted, format!("time budget of {} ms used", budget.max_wall_ms)).await?;
                return Ok(run);
            }
            if run.spent.cost_micro_usd > run.budget.max_cost_micro_usd {
                self.finish(&mut run, RunStatus::BudgetExhausted, format!("cost budget of {} micro-USD used", budget.max_cost_micro_usd)).await?;
                return Ok(run);
            }
            let (_, proposal) = self.load_proposal(&run.review_id, &run.proposal_id).await?;
            if proposal["status"] != "approved_no_execution" {
                let st = proposal["status"].as_str().unwrap_or("unknown").to_string();
                self.finish(&mut run, RunStatus::NeedsHuman, format!("proposal is no longer approved (now {st}); stopped")).await?;
                return Ok(run);
            }
            let planned = match self.planner.next(&run, &proposal).await {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.finish(&mut run, RunStatus::VerifiedDone, "plan complete; every step verified").await?;
                    return Ok(run);
                }
                Err(e) => {
                    self.finish(&mut run, RunStatus::NeedsHuman, e).await?;
                    return Ok(run);
                }
            };
            run.spent.cost_micro_usd += planned.cost_micro_usd;
            if let Some(stop) = self.step(&mut run, planned).await? {
                let (status, reason) = stop;
                self.finish(&mut run, status, reason).await?;
                return Ok(run);
            }
            run.updated_at = now();
            self.save(&run).await?;
        }
        self.finish(&mut run, RunStatus::Failed, "loop bound reached").await?;
        Ok(run)
    }

    /// One step. Returns a terminal (status, reason) to stop the run, or None to continue.
    async fn step(&self, run: &mut Run, planned: PlannedStep) -> Result<Option<(RunStatus, String)>, AutoError> {
        let n = run.spent.steps + 1;
        let input = redact_json(&planned.input);
        let Some(spec) = tools::find(&planned.tool) else {
            self.record(run, n, &planned.tool, ActionClass::Read, "", &input, "refused", "unknown tool", 0).await?;
            return Ok(Some((RunStatus::NeedsHuman, format!("planner asked for an unknown tool '{}'", redact(&planned.tool)))));
        };
        let key = idem_key(&run.id, spec.name, &planned.input);
        // Hard gates: refused whatever the standing policy says.
        if spec.class.is_hard_gate() || !spec.executable {
            self.record(run, n, spec.name, spec.class, &key, &input, "refused", spec.class.gate_reason(), 0).await?;
            self.audit("hard_gate_refused", json!({"run": run.id, "tool": spec.name, "class": spec.class.as_str(), "reason": spec.class.gate_reason()})).await?;
            return Ok(Some((RunStatus::Refused, format!("{} is a hard gate: {}", spec.name, spec.class.gate_reason()))));
        }
        // Sensitivity: the input may not carry more sensitive data than the tool is cleared for.
        let tier = effective_tier(spec.data_tier.min(super::guard::Tier::T1), &planned.input.to_string());
        if tier > spec.data_tier {
            self.record(run, n, spec.name, spec.class, &key, &input, "refused", "input carries identifiers this tool may not hold", 0).await?;
            return Ok(Some((RunStatus::NeedsHuman, format!("{} input looked like it held identifiers ({tier:?}); a person must handle it", spec.name))));
        }
        // Autonomy level and standing approvals, checked fresh on every step.
        match spec.class.default_level() {
            Level::Auto => {}
            Level::AutoWithNotify => run.notifications.push(format!("{}: {}", spec.name, spec.description)),
            Level::NeedsApproval => {
                let active = policy::get(self.store(), spec.class).await.map_err(store_err)?.filter(|p| p.is_active(chrono::Utc::now()));
                let Some(p) = active else {
                    self.record(run, n, spec.name, spec.class, &key, &input, "refused", "needs approval: no active standing approval", 0).await?;
                    return Ok(Some((RunStatus::NeedsHuman, format!("{} needs approval: no active standing approval for {}", spec.name, spec.class.as_str()))));
                };
                if !run.done(spec.name) && !policy::take_daily_use(self.store(), &p).await.map_err(store_err)? {
                    self.record(run, n, spec.name, spec.class, &key, &input, "refused", "standing approval daily cap reached", 0).await?;
                    return Ok(Some((RunStatus::NeedsHuman, format!("daily cap of {} reached for {}", p.max_runs_per_day, spec.class.as_str()))));
                }
            }
            Level::Never => return Ok(Some((RunStatus::Refused, spec.class.gate_reason().into()))),
        }
        // Idempotency: a step already done for this input returns its stored result.
        if let Some(prev) = self.store().get(&format!("autonomy:idem:{key}")).await.map_err(store_err)? {
            self.record(run, n, spec.name, spec.class, &key, &input, "replayed", &redact(&prev), 1).await?;
            return Ok(None);
        }
        let mut attempts = 0;
        loop {
            attempts += 1;
            let result = self.execute(spec, run, &key, &planned.input).await;
            let result = match result {
                Ok(v) => v,
                Err(e) => {
                    self.record(run, n, spec.name, spec.class, &key, &input, "failed", &e, attempts).await?;
                    return Ok(Some((RunStatus::NeedsHuman, format!("{} failed: {e}", spec.name))));
                }
            };
            match self.verify(spec, run, &key).await {
                Ok(()) => {
                    let summary = redact(&result.to_string());
                    self.store().set_nx(&format!("autonomy:idem:{key}"), &summary).await.map_err(store_err)?;
                    self.record(run, n, spec.name, spec.class, &key, &input, "verified", &summary, attempts).await?;
                    return Ok(None);
                }
                Err(why) => {
                    if spec.reversible {
                        self.rollback(spec, run, &key).await?;
                    }
                    self.audit("verify_failed", json!({"run": run.id, "tool": spec.name, "attempt": attempts, "why": why, "rolled_back": spec.reversible})).await?;
                    if attempts >= MAX_ATTEMPTS {
                        self.record(run, n, spec.name, spec.class, &key, &input, "rolled_back", &why, attempts).await?;
                        return Ok(Some((RunStatus::NeedsHuman, format!("{} could not be verified after {attempts} tries: {why}", spec.name))));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn record(&self, run: &mut Run, n: u32, tool: &str, class: ActionClass, key: &str, input: &Value, outcome: &str, detail: &str, attempts: u32) -> Result<(), AutoError> {
        run.spent.steps = n;
        run.steps.push(Step { n, tool: redact(tool), action_class: class, idempotency_key: key.into(), input: input.clone(), outcome: outcome.into(), detail: redact(detail).chars().take(1_000).collect(), attempts, at: now() });
        run.updated_at = now();
        self.save(run).await?;
        self.audit("step", json!({"run": run.id, "n": n, "tool": tool, "class": class.as_str(), "outcome": outcome, "attempts": attempts})).await
    }

    fn record_key(spec: &ToolSpec, run: &Run, key: &str) -> Option<String> {
        match spec.name {
            "add_internal_note" => Some(format!("autonomy:note:{key}")),
            "create_internal_task" => Some(format!("autonomy:task:{key}")),
            "draft_internal_text" => Some(format!("autonomy:draft:{key}")),
            "mark_finding_followed_up" => Some(format!("autonomy:followup:{}:{}", run.review_id, run.proposal_id)),
            _ => None,
        }
    }

    /// Executors write only NXT's own internal autonomy records. Nothing here can reach the
    /// books, QuickBooks, CRA, Stripe, a client or any message channel.
    async fn execute(&self, spec: &ToolSpec, run: &Run, key: &str, input: &Value) -> Result<Value, String> {
        let text = |field: &str| redact(input[field].as_str().unwrap_or("")).chars().take(2_000).collect::<String>();
        let base = json!({"run_id": run.id, "review_id": run.review_id, "proposal_id": run.proposal_id, "internal_only": true, "at": now()});
        let mut record = base.clone();
        match spec.name {
            "read_proposal" | "recheck_proposal" => {
                let (review, p) = self.load_proposal(&run.review_id, &run.proposal_id).await.map_err(|e| e.message)?;
                if spec.name == "recheck_proposal" && p["status"] != "approved_no_execution" {
                    return Err(format!("proposal is now {}", p["status"].as_str().unwrap_or("unknown")));
                }
                let finding = review["findings"].as_array().and_then(|fs| fs.iter().find(|f| f["id"] == p["finding_id"])).cloned().unwrap_or(Value::Null);
                return Ok(json!({"kind": p["kind"], "status": p["status"], "summary": redact(p["summary"].as_str().unwrap_or("")), "finding": redact(finding["title"].as_str().unwrap_or(""))}));
            }
            "add_internal_note" => {
                let t = text("text");
                if t.trim().is_empty() {
                    return Err("a note needs text".into());
                }
                record["text"] = json!(t);
            }
            "create_internal_task" => {
                let t = text("title");
                if t.trim().is_empty() {
                    return Err("a task needs a title".into());
                }
                record["title"] = json!(t);
                record["assignee_role"] = json!(text("assignee_role"));
                record["status"] = json!("open");
            }
            "draft_internal_text" => {
                record["text"] = json!(text("text"));
                record["sent"] = json!(false);
            }
            "mark_finding_followed_up" => {
                record["followed_up"] = json!(true);
            }
            other => return Err(format!("no executor for {other}")),
        }
        let k = Self::record_key(spec, run, key).ok_or("no record key")?;
        self.store().set_json(&k, &record).await?;
        let index = match spec.name {
            "add_internal_note" => Some(format!("autonomy:notes:{}:{}", run.review_id, run.proposal_id)),
            "create_internal_task" => Some("autonomy:tasks".to_string()),
            "draft_internal_text" => Some("autonomy:drafts".to_string()),
            _ => None,
        };
        if let Some(index) = index {
            self.store().sadd(&index, &k).await?;
        }
        Ok(json!({"record": k}))
    }

    /// Post-condition check: re-read what the step should have produced.
    async fn verify(&self, spec: &ToolSpec, run: &Run, key: &str) -> Result<(), String> {
        {
            let mut faults = self.faults.lock().unwrap();
            if let Some(left) = faults.get_mut(spec.name) {
                if *left > 0 {
                    *left -= 1;
                    return Err("injected verification fault (eval harness)".into());
                }
            }
        }
        let Some(k) = Self::record_key(spec, run, key) else {
            // Read tools verify by having returned without error.
            return Ok(());
        };
        let rec: Value = self.store().get_json(&k).await?.ok_or("record was not written")?;
        if rec["tombstone"] == true {
            return Err("record was rolled back".into());
        }
        if rec["internal_only"] != true || rec["run_id"] != run.id.as_str() {
            return Err("record does not match this run".into());
        }
        if spec.name == "draft_internal_text" && rec["sent"] != false {
            return Err("a draft must never be marked sent".into());
        }
        Ok(())
    }

    /// Compensating action: tombstones the internal record the step wrote.
    async fn rollback(&self, spec: &ToolSpec, run: &Run, key: &str) -> Result<(), AutoError> {
        if let Some(k) = Self::record_key(spec, run, key) {
            self.store().set_json(&k, &json!({"tombstone": true, "run_id": run.id, "rolled_back_at": now()})).await.map_err(store_err)?;
            self.audit("rolled_back", json!({"run": run.id, "tool": spec.name, "record": k})).await?;
        }
        Ok(())
    }

    pub fn status(&self) -> Value {
        json!({
            "autonomous_execution": if self.config.enabled { "on" } else { "off" },
            "kill_switch_env": self.config.kill_env,
            "ai_planner": if self.config.ai.is_some() { "gateway_zdr" } else { "off" },
            "planner": self.planner.name(),
            "budget_default": self.config.budget,
            "model_routes": crate::nixon_finance::ai_gateway::describe(&crate::nixon_finance::ai_gateway::process_env)["client_data_routes"].clone(),
            "ai_flags": "NXT_AUTONOMY_AI=on and NXT_AI_GATEWAY_PAID=confirmed (routing shared with God's Eye ai_gateway)",
            "free_models": "never for autonomy prompts (always client data); NVIDIA only for non-sensitive God's Eye drafts",
            "matrix": ActionClass::ALL.iter().map(|c| json!({"class": c.as_str(), "risk": c.risk(), "level": c.default_level(), "hard_gate": c.is_hard_gate(), "standing_approval_allowed": c.default_level() == Level::NeedsApproval})).collect::<Vec<_>>(),
            "extends": "God's Eye / Nixon review proposals (nixon:review:*); approval stays in nixon_finance::service::decide",
        })
    }
}
