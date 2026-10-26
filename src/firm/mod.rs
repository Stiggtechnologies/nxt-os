//! Firm Operations: durable internal work, proposals, approvals and reporting.
//! Client/company onboarding and external-account connections are deliberately absent.

pub mod models;
mod reports;
mod routes;
mod xlsx;
pub use routes::router;

use crate::{ledger::engine::Ledger, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use models::*;
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};

const STATE_KEY: &str = "firm:state:v1";
const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
pub type FResult<T> = Result<T, ApiError>;

pub struct Firm {
    pub store: Store,
    pub ledger: Arc<Ledger>,
}

fn bad(message: impl Into<String>) -> ApiError { ApiError(StatusCode::BAD_REQUEST, message.into()) }
fn conflict(message: impl Into<String>) -> ApiError { ApiError(StatusCode::CONFLICT, message.into()) }
fn missing(kind: &str) -> ApiError { ApiError(StatusCode::NOT_FOUND, format!("{kind} not found")) }
fn id(prefix: &str) -> String { format!("{prefix}_{}", uuid::Uuid::new_v4().simple()) }
fn now() -> String { Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true) }

fn text(value: &str, field: &str, max: usize, required: bool) -> FResult<()> {
    if value.len() > max || (required && value.trim().is_empty()) || value.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(bad(format!("{field} must be {}at most {max} bytes and contain no control characters", if required { "nonempty, " } else { "" })));
    }
    Ok(())
}

fn date(value: &str) -> FResult<NaiveDate> {
    if value.len() != 10 || value.as_bytes().get(4) != Some(&b'-') || value.as_bytes().get(7) != Some(&b'-') {
        return Err(bad("expected a YYYY-MM-DD date"));
    }
    let d = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| bad("invalid calendar date"))?;
    if !("1900-01-01"..="9998-12-31").contains(&value) { return Err(bad("date must be between 1900 and 9998")); }
    Ok(d)
}

fn optional_date(value: &Option<String>) -> FResult<()> {
    if let Some(v) = value { date(v)?; }
    Ok(())
}

fn utc_timestamp(value: &str) -> FResult<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| bad("expected an RFC3339 UTC timestamp"))?;
    if parsed.offset().local_minus_utc() != 0 { return Err(bad("timestamp must use UTC (Z or +00:00)")); }
    let utc = parsed.with_timezone(&Utc);
    date(&utc.format("%Y-%m-%d").to_string())?;
    Ok(utc)
}

fn capacity(len: usize, max: usize, kind: &str) -> FResult<()> {
    if len >= max { Err(conflict(format!("{kind} capacity reached ({max}); archive or remove existing records before adding more"))) } else { Ok(()) }
}

fn audit(state: &mut FirmState, action: &str, kind: &str, entity: &str, detail: impl Into<String>) {
    state.audit.insert(0, AuditEvent { id: id("audit"), at: now(), action: action.into(), entity_type: kind.into(), entity_id: entity.into(), detail: detail.into() });
    state.audit.truncate(1000);
}

fn validate_task(input: &TaskInput) -> FResult<()> {
    text(&input.title, "task title", 200, true)?;
    text(&input.assignee, "assignee", 120, false)?;
    optional_date(&input.due_date)
}

fn tasks(inputs: &[TaskInput]) -> FResult<Vec<Task>> {
    if inputs.len() > 100 { return Err(bad("at most 100 tasks are allowed")); }
    inputs.iter().map(|t| {
        validate_task(t)?;
        if t.id.is_some() { return Err(bad("task ids may only be supplied when updating their existing project")); }
        Ok(Task { id: id("task"), title: t.title.trim().into(), assignee: t.assignee.trim().into(), due_date: t.due_date.clone(), completed: false, completed_at: None })
    }).collect()
}

fn project_tasks(inputs: &[TaskInput], existing: Option<&Project>) -> FResult<Vec<Task>> {
    if inputs.len() > 100 { return Err(bad("at most 100 tasks are allowed")); }
    let mut seen = HashSet::new();
    inputs.iter().map(|input| {
        validate_task(input)?;
        if let Some(task_id) = &input.id {
            if !seen.insert(task_id.clone()) { return Err(bad("duplicate task id in project update")); }
            let mut task = existing.and_then(|p| p.tasks.iter().find(|t| t.id == *task_id)).cloned().ok_or_else(|| bad("task id does not belong to this project"))?;
            task.title = input.title.trim().into(); task.assignee = input.assignee.trim().into(); task.due_date = input.due_date.clone();
            Ok(task)
        } else {
            Ok(Task { id: id("task"), title: input.title.trim().into(), assignee: input.assignee.trim().into(), due_date: input.due_date.clone(), completed: false, completed_at: None })
        }
    }).collect()
}

fn update_task(task: &mut Task, input: &TaskUpdate, at: &str) -> FResult<()> {
    if let Some(assignee) = &input.assignee { text(assignee, "assignee", 120, false)?; task.assignee = assignee.trim().into(); }
    if let Some(due) = &input.due_date { date(due)?; task.due_date = Some(due.clone()); }
    task.completed = input.completed;
    task.completed_at = if input.completed { task.completed_at.clone().or_else(|| Some(at.into())) } else { None };
    Ok(())
}

fn builtin_templates() -> Vec<ProjectTemplate> {
    [
        ("monthly_books", "Monthly bookkeeping", vec!["Review supporting documents", "Reconcile balances", "Review ledger exceptions", "Prepare reporting package"]),
        ("year_end", "Year-end preparation", vec!["Complete period reconciliations", "Review supporting schedules", "Prepare year-end reporting", "Document reviewer sign-off"]),
        ("onboarding", "Engagement planning", vec!["Confirm engagement scope", "Define responsibilities", "Agree on reporting deadlines"]),
    ].into_iter().map(|(key, name, labels)| ProjectTemplate {
        id: format!("builtin_{key}"), name: name.into(), description: "Reusable planning checklist; no company or client data is created.".into(),
        tasks: labels.into_iter().map(|title| TemplateTask { title: title.into(), assignee: String::new(), due_offset_days: 0 }).collect(),
        version: 1, builtin: true, created_at: String::new(), updated_at: String::new(),
    }).collect()
}

impl Firm {
    pub fn new(store: Store, ledger: Arc<Ledger>) -> Self { Self { store, ledger } }

    pub async fn snapshot(&self) -> FResult<FirmState> {
        let mut state: FirmState = self.store.get_json(STATE_KEY).await?.unwrap_or_default();
        for template in builtin_templates() {
            if !state.templates.iter().any(|t| t.id == template.id) { state.templates.push(template); }
        }
        Ok(state)
    }

    async fn mutate<T, F>(&self, mut operation: F) -> FResult<T>
    where F: FnMut(&mut FirmState) -> FResult<T> {
        for _ in 0..12 {
            let raw = self.store.get(STATE_KEY).await?;
            let mut state: FirmState = match raw.as_deref() {
                Some(v) => serde_json::from_str(v).map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "firm state is invalid".into()))?,
                None => FirmState::default(),
            };
            for template in builtin_templates() {
                if !state.templates.iter().any(|t| t.id == template.id) { state.templates.push(template); }
            }
            let result = operation(&mut state)?;
            let next = serde_json::to_string(&state).map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "firm state serialization failed".into()))?;
            if next.len() > MAX_STATE_BYTES { return Err(conflict("firm storage capacity reached; remove unused export artifacts")); }
            if self.store.compare_exchange(STATE_KEY, raw.as_deref(), &next).await? { return Ok(result); }
        }
        Err(conflict("firm state changed concurrently; retry this request"))
    }

    pub async fn summary(&self) -> FResult<Value> {
        let s = self.snapshot().await?;
        Ok(json!({
            "projects": s.projects.len(), "open_tasks": s.projects.iter().flat_map(|p| &p.tasks).filter(|t| !t.completed).count(),
            "templates": s.templates.len(), "workflows": s.workflows.len(),
            "pending_approvals": s.approvals.iter().filter(|a| a.status == "proposed").count(),
            "closes": s.closes.len(), "runs": s.runs.len(), "exports": s.exports.len(),
            "storage": self.store.backend_name(), "clients_deferred": true,
            "scheduler": "invoke /api/firm/workflows/tick from a trusted authenticated scheduler; no background daemon is installed",
            "auth_required": crate::referral::routes::auth_required(),
            "access_model": "shared admin credential; responsibility labels are routing metadata, not verified individual identities",
        }))
    }

    pub async fn save_template(&self, existing: Option<&str>, input: TemplateInput) -> FResult<ProjectTemplate> {
        text(&input.name, "template name", 160, true)?;
        text(&input.description, "description", 2000, false)?;
        if input.tasks.is_empty() || input.tasks.len() > 100 { return Err(bad("template requires 1 to 100 tasks")); }
        for t in &input.tasks {
            text(&t.title, "task title", 200, true)?; text(&t.assignee, "assignee", 120, false)?;
            if !(-3650..=3650).contains(&t.due_offset_days) { return Err(bad("task date offsets must be between -3650 and 3650 days")); }
        }
        let record_id = existing.map(str::to_owned).unwrap_or_else(|| id("tpl"));
        let at = now();
        self.mutate(|s| {
            let previous = s.templates.iter().find(|t| t.id == record_id).cloned();
            if existing.is_some() && previous.is_none() { return Err(missing("template")); }
            if previous.as_ref().is_some_and(|p| p.builtin) { return Err(conflict("built-in templates are read-only; create a custom template")); }
            if previous.is_none() { capacity(s.templates.len(), 100, "templates")?; }
            let t = ProjectTemplate { id: record_id.clone(), name: input.name.trim().into(), description: input.description.trim().into(), tasks: input.tasks.clone(), version: previous.as_ref().map_or(1, |p| p.version + 1), builtin: false, created_at: previous.map_or(at.clone(), |p| p.created_at), updated_at: at.clone() };
            s.templates.retain(|p| p.id != record_id); s.templates.push(t.clone());
            audit(s, if existing.is_some() {"updated"} else {"created"}, "template", &record_id, &t.name);
            Ok(t)
        }).await
    }

    pub async fn delete_template(&self, record_id: &str) -> FResult<Value> {
        self.mutate(|s| {
            let t = s.templates.iter().find(|t| t.id == record_id).ok_or_else(|| missing("template"))?;
            if t.builtin { return Err(conflict("built-in templates cannot be deleted")); }
            s.templates.retain(|t| t.id != record_id); audit(s, "deleted", "template", record_id, "Custom template removed; instantiated projects preserved");
            Ok(json!({"deleted": true, "id": record_id}))
        }).await
    }

    pub async fn instantiate(&self, template_id: &str, input: InstantiateInput) -> FResult<Project> {
        text(&input.owner, "owner", 120, false)?; optional_date(&input.due_date)?;
        if let Some(name) = &input.name { text(name, "project name", 160, true)?; }
        let record_id = id("project"); let at = now();
        let base = input.due_date.as_deref().map(date).transpose()?.unwrap_or_else(|| Utc::now().date_naive());
        self.mutate(|s| {
            capacity(s.projects.len(), 500, "projects")?;
            let t = s.templates.iter().find(|t| t.id == template_id).ok_or_else(|| missing("template"))?;
            let task_inputs = t.tasks.iter().map(|t| {
                let due = base.checked_add_signed(Duration::days(t.due_offset_days)).ok_or_else(|| bad("task date offset is outside the supported calendar"))?.format("%Y-%m-%d").to_string();
                date(&due)?;
                Ok(TaskInput { id: None, title: t.title.clone(), assignee: t.assignee.clone(), due_date: Some(due) })
            }).collect::<FResult<Vec<_>>>()?;
            let p = Project { id: record_id.clone(), name: input.name.clone().unwrap_or_else(|| t.name.clone()), description: t.description.clone(), owner: input.owner.trim().into(), due_date: input.due_date.clone(), template_id: Some(template_id.into()), tasks: tasks(&task_inputs)?, status: "open".into(), created_at: at.clone(), updated_at: at.clone() };
            s.projects.push(p.clone()); audit(s, "instantiated", "project", &record_id, format!("Template {template_id}")); Ok(p)
        }).await
    }

    pub async fn save_project(&self, existing: Option<&str>, input: ProjectInput) -> FResult<Project> {
        text(&input.name, "project name", 160, true)?; text(&input.description, "description", 2000, false)?; text(&input.owner, "owner", 120, false)?; optional_date(&input.due_date)?;
        if input.tasks.len() > 100 { return Err(bad("at most 100 tasks are allowed")); }
        for task in &input.tasks { validate_task(task)?; }
        let record_id = existing.map(str::to_owned).unwrap_or_else(|| id("project")); let at = now();
        self.mutate(|s| {
            let prior = s.projects.iter().find(|p| p.id == record_id).cloned();
            if existing.is_some() && prior.is_none() { return Err(missing("project")); }
            if prior.is_none() { capacity(s.projects.len(), 500, "projects")?; }
            let new_tasks = project_tasks(&input.tasks, prior.as_ref())?;
            let status = if !new_tasks.is_empty() && new_tasks.iter().all(|task| task.completed) {"completed"} else {"open"};
            let p = Project { id: record_id.clone(), name: input.name.trim().into(), description: input.description.trim().into(), owner: input.owner.trim().into(), due_date: input.due_date.clone(), template_id: prior.as_ref().and_then(|p| p.template_id.clone()), tasks: new_tasks, status: status.into(), created_at: prior.map_or(at.clone(), |p| p.created_at), updated_at: at.clone() };
            s.projects.retain(|p| p.id != record_id); s.projects.push(p.clone()); audit(s, if existing.is_some() {"updated"} else {"created"}, "project", &record_id, &p.name); Ok(p)
        }).await
    }

    pub async fn delete_project(&self, record_id: &str) -> FResult<Value> {
        self.mutate(|s| {
            if !s.projects.iter().any(|p| p.id == record_id) { return Err(missing("project")); }
            let targets = |a: &WorkflowAction| matches!(a, WorkflowAction::CreateTask { project_id: Some(p), .. } if p == record_id);
            if s.workflows.iter().any(|w| targets(&w.action)) || s.runs.iter().any(|r| r.status == "proposed" && targets(&r.action)) { return Err(conflict("project is referenced by a workflow or pending proposal")); }
            s.projects.retain(|p| p.id != record_id); audit(s, "deleted", "project", record_id, "Project removed"); Ok(json!({"deleted": true, "id": record_id}))
        }).await
    }

    pub async fn project_task(&self, project_id: &str, task_id: &str, input: TaskUpdate) -> FResult<Project> {
        let at = now();
        self.mutate(|s| {
            let p = s.projects.iter_mut().find(|p| p.id == project_id).ok_or_else(|| missing("project"))?;
            let task = p.tasks.iter_mut().find(|t| t.id == task_id).ok_or_else(|| missing("task"))?;
            update_task(task, &input, &at)?;
            p.status = if !p.tasks.is_empty() && p.tasks.iter().all(|t| t.completed) {"completed"} else {"open"}.into();
            p.updated_at = at.clone(); let result = p.clone(); audit(s, "task_updated", "project", project_id, format!("{task_id}: completed={}", input.completed)); Ok(result)
        }).await
    }

    pub async fn save_workflow(&self, existing: Option<&str>, input: WorkflowInput) -> FResult<Workflow> {
        text(&input.name, "workflow name", 160, true)?;
        match &input.action {
            WorkflowAction::CreateTask { title, project_id } => {
                text(title, "task title", 200, true)?;
                if !input.requires_approval { return Err(bad("task creation requires human approval")); }
                if let Some(p) = project_id { text(p, "project id", 100, true)?; }
            },
            WorkflowAction::GenerateReport { report, from, to } => { reports::validate_range(report, from.clone(), to.clone())?; },
        }
        let due_at = match (&input.trigger, &input.due_at) {
            (Trigger::Manual, _) => None,
            (_, Some(v)) => Some(utc_timestamp(v)?.to_rfc3339_opts(SecondsFormat::Secs, true)),
            _ => return Err(bad("daily and weekly workflows require due_at in UTC")),
        };
        let record_id = existing.map(str::to_owned).unwrap_or_else(|| id("workflow")); let at = now();
        self.mutate(|s| {
            if let WorkflowAction::CreateTask { project_id: Some(p), .. } = &input.action {
                if !s.projects.iter().any(|project| project.id == *p) { return Err(missing("target project")); }
            }
            let prior = s.workflows.iter().find(|w| w.id == record_id).cloned();
            if existing.is_some() && prior.is_none() { return Err(missing("workflow")); }
            if prior.is_none() { capacity(s.workflows.len(), 100, "workflows")?; }
            let w = Workflow { id: record_id.clone(), name: input.name.trim().into(), trigger: input.trigger.clone(), enabled: input.enabled, requires_approval: input.requires_approval, due_at: due_at.clone(), action: input.action.clone(), created_at: prior.map_or(at.clone(), |w| w.created_at), updated_at: at.clone() };
            s.workflows.retain(|w| w.id != record_id); s.workflows.push(w.clone()); audit(s, if existing.is_some() {"updated"} else {"created"}, "workflow", &record_id, &w.name); Ok(w)
        }).await
    }

    pub async fn delete_workflow(&self, record_id: &str) -> FResult<Value> {
        self.mutate(|s| {
            if !s.workflows.iter().any(|w| w.id == record_id) { return Err(missing("workflow")); }
            s.workflows.retain(|w| w.id != record_id); audit(s, "deleted", "workflow", record_id, "Workflow removed; run history and proposal snapshots preserved"); Ok(json!({"deleted": true, "id": record_id}))
        }).await
    }

    fn propose(s: &mut FirmState, workflow_id: &str, key: &str, at: &str) -> FResult<WorkflowRun> {
        if let Some(r) = s.runs.iter().find(|r| r.workflow_id == workflow_id && r.idempotency_key == key) { return Ok(r.clone()); }
        capacity(s.runs.len(), 1000, "workflow runs")?;
        let w = s.workflows.iter().find(|w| w.id == workflow_id).ok_or_else(|| missing("workflow"))?.clone();
        if !w.enabled { return Err(conflict("workflow is disabled")); }
        let run_id = id("run");
        let approval_id = if w.requires_approval { Some(id("approval")) } else { None };
        let run = WorkflowRun { id: run_id.clone(), name: w.name, workflow_id: workflow_id.into(), idempotency_key: key.into(), status: "proposed".into(), approval_id: approval_id.clone(), action: w.action, requires_approval: w.requires_approval, created_at: at.into(), executed_at: None, result: None, attempts: 0, last_error: None };
        if let Some(aid) = approval_id {
            capacity(s.approvals.len(), 1000, "approvals")?;
            s.approvals.push(Approval { id: aid, run_id: run_id.clone(), workflow_id: workflow_id.into(), status: "proposed".into(), decision: None, note: String::new(), created_at: at.into(), decided_at: None, executed_at: None });
        }
        s.runs.push(run.clone()); audit(s, "proposed", "workflow_run", &run_id, format!("Workflow {workflow_id}; human approval={}", run.requires_approval)); Ok(run)
    }

    /// The AI may prepare only these internal actions. This path never executes them.
    /// Proposals share the reserved `agent` idempotency namespace without creating
    /// hidden workflows or schedules. Human approval uses the same immutable Run.
    pub async fn propose_action(&self, input: ProposalInput) -> FResult<WorkflowRun> {
        text(&input.name, "proposal name", 160, true)?;
        text(&input.idempotency_key, "idempotency_key", 128, true)?;
        match &input.action {
            WorkflowAction::CreateTask { title, project_id } => {
                text(title, "task title", 200, true)?;
                if let Some(pid) = project_id { text(pid, "project id", 100, true)?; }
            },
            WorkflowAction::GenerateReport { report, from, to } => { reports::validate_range(report, from.clone(), to.clone())?; },
        }
        let at = now();
        self.mutate(|s| {
            if let Some(r) = s.runs.iter().find(|r| r.workflow_id == "agent" && r.idempotency_key == input.idempotency_key) { return Ok(r.clone()); }
            if let WorkflowAction::CreateTask { project_id: Some(pid), .. } = &input.action {
                if !s.projects.iter().any(|p| p.id == *pid) { return Err(missing("target project")); }
            }
            capacity(s.runs.len(), 1000, "workflow runs")?; capacity(s.approvals.len(), 1000, "approvals")?;
            let run_id = id("run"); let approval_id = id("approval");
            let run = WorkflowRun { id: run_id.clone(), name: input.name.trim().into(), workflow_id: "agent".into(), idempotency_key: input.idempotency_key.clone(), status: "proposed".into(), approval_id: Some(approval_id.clone()), action: input.action.clone(), requires_approval: true, created_at: at.clone(), executed_at: None, result: None, attempts: 0, last_error: None };
            s.approvals.push(Approval { id: approval_id, run_id: run_id.clone(), workflow_id: "agent".into(), status: "proposed".into(), decision: None, note: String::new(), created_at: at.clone(), decided_at: None, executed_at: None });
            s.runs.push(run.clone()); audit(s, "proposed", "agent_proposal", &run_id, format!("{}; human approval required", input.name)); Ok(run)
        }).await
    }

    pub async fn run(&self, workflow_id: &str, input: RunInput) -> FResult<Value> {
        text(&input.idempotency_key, "idempotency_key", 128, true)?;
        let at = now();
        if input.dry_run {
            let s = self.snapshot().await?;
            let w = s.workflows.iter().find(|w| w.id == workflow_id).ok_or_else(|| missing("workflow"))?;
            return Ok(json!({"dry_run": true, "workflow_id": workflow_id, "action": w.action, "requires_approval": w.requires_approval, "enabled": w.enabled}));
        }
        let run = self.mutate(|s| Self::propose(s, workflow_id, &input.idempotency_key, &at)).await?;
        let run = if !run.requires_approval && run.status == "proposed" { self.execute(&run.id, None).await? } else { run };
        Ok(json!(run))
    }

    pub async fn tick(&self, input: TickInput) -> FResult<Value> {
        let at = input.now.as_deref().map(utc_timestamp).transpose()?.unwrap_or_else(Utc::now);
        let stamp = at.to_rfc3339_opts(SecondsFormat::Secs, true);
        let runs = self.mutate(|s| {
            let due: Vec<_> = s.workflows.iter().filter(|w| w.enabled && w.trigger != Trigger::Manual).filter_map(|w| w.due_at.as_ref().map(|d| (w.id.clone(), w.trigger.clone(), d.clone()))).collect();
            let mut runs = Vec::new();
            for (workflow_id, trigger, due_at) in due {
                let due = utc_timestamp(&due_at)?;
                if due > at { continue; }
                let key = format!("scheduled:{due_at}");
                let run = Self::propose(s, &workflow_id, &key, &stamp)?;
                let days = if trigger == Trigger::Weekly {7} else {1};
                // One catch-up proposal per workflow; advance to the first future slot.
                let slots = (at - due).num_seconds() / (days * 86400) + 1;
                let next = due.checked_add_signed(Duration::days(days * slots)).ok_or_else(|| bad("next schedule date is out of range"))?;
                date(&next.format("%Y-%m-%d").to_string())?;
                if let Some(w) = s.workflows.iter_mut().find(|w| w.id == workflow_id) { w.due_at = Some(next.to_rfc3339_opts(SecondsFormat::Secs, true)); w.updated_at = stamp.clone(); }
                audit(s, "dispatched", "workflow", &workflow_id, format!("Due {due_at}; advanced {slots} schedule slots; one proposal"));
                runs.push(run);
            }
            Ok(runs)
        }).await?;
        let mut results = Vec::new();
        let mut errors = Vec::new();
        let mut attempted = HashSet::new();
        for r in runs {
            attempted.insert(r.id.clone());
            if !r.requires_approval {
                match self.execute(&r.id, None).await {
                    Ok(done) => results.push(done),
                    Err(error) => { errors.push(json!({"run_id": r.id, "error": error.1})); results.push(r); },
                }
            } else { results.push(r); }
        }
        // Resume any read-only automatic run persisted before a process interruption.
        let s = self.snapshot().await?;
        for r in s.runs.iter().filter(|r| !r.requires_approval && r.status == "proposed") {
            if !attempted.contains(&r.id) {
                match self.execute(&r.id, None).await {
                    Ok(done) => results.push(done),
                    Err(error) => { errors.push(json!({"run_id": r.id, "error": error.1})); results.push(r.clone()); },
                }
            }
        }
        // Return current error/attempt metadata for failed runs, not their stale pre-attempt copies.
        if !errors.is_empty() {
            let latest = self.snapshot().await?;
            for r in &mut results {
                if let Some(current) = latest.runs.iter().find(|current| current.id == r.id) { *r = current.clone(); }
            }
        }
        Ok(json!({"processed": results.len(), "runs": results, "errors": errors}))
    }

    async fn execute(&self, run_id: &str, decision: Option<&DecisionInput>) -> FResult<WorkflowRun> {
        let initial = self.snapshot().await?;
        let run = initial.runs.iter().find(|r| r.id == run_id).ok_or_else(|| missing("run"))?.clone();
        if run.status != "proposed" { return Ok(run); }
        if run.requires_approval && decision.is_none() { return Err(conflict("human approval is required")); }
        let export = if let WorkflowAction::GenerateReport { report, from, to } = &run.action {
            match self.prepare_export(ExportInput { report: report.clone(), from: from.clone(), to: to.clone() }, Some(format!("export_{}", run.id))).await {
                Ok(export) => Some(export),
                Err(error) => { self.record_execution_failure(run_id, &error).await; return Err(error); },
            }
        } else { None };
        let at = now();
        let execution = self.mutate(|s| {
            let pos = s.runs.iter().position(|r| r.id == run_id).ok_or_else(|| missing("run"))?;
            if s.runs[pos].status != "proposed" { return Ok(s.runs[pos].clone()); }
            if let Some(aid) = s.runs[pos].approval_id.clone() {
                let a = s.approvals.iter_mut().find(|a| a.id == aid).ok_or_else(|| missing("approval"))?;
                if a.status != "proposed" { return Err(conflict("approval has already been decided")); }
                a.status = "approved".into(); a.decision = Some("approve".into()); a.decided_at = Some(at.clone()); a.note = decision.map_or(String::new(), |d| d.note.clone());
                audit(s, "approved", "approval", &aid, "Internal action approved through the workspace approval endpoint");
            }
            let result = match &run.action {
                WorkflowAction::CreateTask { title, project_id } => {
                    let task = Task { id: format!("task_{run_id}"), title: title.clone(), assignee: String::new(), due_date: None, completed: false, completed_at: None };
                    let project = if let Some(pid) = project_id {
                        let p = s.projects.iter_mut().find(|p| p.id == *pid).ok_or_else(|| missing("target project"))?;
                        capacity(p.tasks.len(), 100, "project tasks")?; p.tasks.push(task.clone()); p.status = "open".into(); p.updated_at = at.clone(); p.clone()
                    } else {
                        capacity(s.projects.len(), 500, "projects")?;
                        let p = Project { id: format!("project_{run_id}"), name: title.clone(), description: "Created by an approved Firm Operations workflow".into(), owner: String::new(), due_date: None, template_id: None, tasks: vec![task.clone()], status: "open".into(), created_at: at.clone(), updated_at: at.clone() };
                        s.projects.push(p.clone()); p
                    };
                    audit(s, "task_created", "project", &project.id, format!("Run {run_id}; task {}", task.id));
                    json!({"project_id": project.id, "task_id": task.id, "title": task.title})
                },
                WorkflowAction::GenerateReport { .. } => {
                    let artifact = export.clone().ok_or_else(|| conflict("report artifact unavailable"))?;
                    capacity(s.exports.len(), 100, "export artifacts")?;
                    let value = json!({"export_id": artifact.metadata.id, "download_url": artifact.metadata.download_url, "row_count": artifact.metadata.row_count});
                    s.exports.push(artifact); value
                },
            };
            let r = &mut s.runs[pos]; r.status = "executed".into(); r.executed_at = Some(at.clone()); r.result = Some(result); r.attempts = r.attempts.saturating_add(1); r.last_error = None;
            let result = r.clone();
            if let Some(aid) = &result.approval_id {
                if let Some(a) = s.approvals.iter_mut().find(|a| a.id == *aid) { a.status = "executed".into(); a.executed_at = Some(at.clone()); }
                audit(s, "executed", "approval", aid, format!("Run {run_id}"));
            }
            audit(s, "executed", "workflow_run", run_id, "Allowed internal action completed"); Ok(result)
        }).await;
        if let Err(error) = &execution { self.record_execution_failure(run_id, error).await; }
        execution
    }

    async fn record_execution_failure(&self, run_id: &str, error: &ApiError) {
        let detail: String = error.1.chars().take(2000).collect();
        // Best effort: preserve the original operation error if storage itself is down.
        let _ = self.mutate(|s| {
            let r = s.runs.iter_mut().find(|r| r.id == run_id).ok_or_else(|| missing("run"))?;
            if r.status != "proposed" { return Ok(()); }
            r.last_error = Some(detail.clone()); r.attempts = r.attempts.saturating_add(1);
            audit(s, "execution_failed", "workflow_run", run_id, detail.clone()); Ok(())
        }).await;
    }

    pub async fn decide(&self, approval_id: &str, input: DecisionInput) -> FResult<Approval> {
        if !matches!(input.decision.as_str(), "approve" | "reject") { return Err(bad("decision must be approve or reject")); }
        text(&input.note, "decision note", 2000, false)?;
        let s = self.snapshot().await?;
        let a = s.approvals.iter().find(|a| a.id == approval_id).ok_or_else(|| missing("approval"))?.clone();
        if let Some(prior) = &a.decision {
            return if prior == &input.decision { Ok(a) } else { Err(conflict("approval has already received a different decision")) };
        }
        if input.decision == "approve" { self.execute(&a.run_id, Some(&input)).await?; }
        else {
            let at = now();
            self.mutate(|s| {
                let a = s.approvals.iter_mut().find(|a| a.id == approval_id).ok_or_else(|| missing("approval"))?;
                if let Some(d) = &a.decision { return if d == "reject" { Ok(()) } else {Err(conflict("approval has already been approved"))}; }
                a.status = "rejected".into(); a.decision = Some("reject".into()); a.note = input.note.clone(); a.decided_at = Some(at.clone());
                let run_id = a.run_id.clone();
                if let Some(r) = s.runs.iter_mut().find(|r| r.id == run_id) { r.status = "rejected".into(); }
                audit(s, "rejected", "approval", approval_id, input.note.clone()); Ok(())
            }).await?;
        }
        let s = self.snapshot().await?;
        let final_approval = s.approvals.into_iter().find(|a| a.id == approval_id).ok_or_else(|| missing("approval"))?;
        if final_approval.decision.as_deref() != Some(input.decision.as_str()) { return Err(conflict("approval received a conflicting concurrent decision")); }
        Ok(final_approval)
    }

    pub async fn create_close(&self, input: CloseInput) -> FResult<Close> {
        text(&input.name, "close name", 160, true)?; date(&input.period_start)?; date(&input.period_end)?; optional_date(&input.due_date)?;
        if input.period_start > input.period_end { return Err(bad("period_start must not follow period_end")); }
        for role in [&input.preparer, &input.reviewer, &input.approver] { text(role, "responsibility", 120, false)?; }
        let inputs = input.tasks.clone().unwrap_or_else(|| ["Reconcile period balances", "Review supporting schedules", "Prepare financial reports"].into_iter().map(|title| TaskInput { id: None, title: title.into(), assignee: input.preparer.clone(), due_date: input.due_date.clone() }).collect());
        if inputs.is_empty() { return Err(bad("close requires at least one checklist task")); }
        let record = Close { id: id("close"), name: input.name.trim().into(), period_start: input.period_start, period_end: input.period_end, due_date: input.due_date, preparer: input.preparer.trim().into(), reviewer: input.reviewer.trim().into(), approver: input.approver.trim().into(), status: "open".into(), tasks: tasks(&inputs)?, created_at: now(), updated_at: now() };
        self.mutate(|s| { capacity(s.closes.len(), 200, "closes")?; s.closes.push(record.clone()); audit(s, "created", "close", &record.id, &record.name); Ok(record.clone()) }).await
    }

    pub async fn close_task(&self, close_id: &str, task_id: &str, input: TaskUpdate) -> FResult<Close> {
        let at = now();
        self.mutate(|s| {
            let c = s.closes.iter_mut().find(|c| c.id == close_id).ok_or_else(|| missing("close"))?;
            if c.status != "open" { return Err(conflict("reopen the close before changing its checklist")); }
            let t = c.tasks.iter_mut().find(|t| t.id == task_id).ok_or_else(|| missing("task"))?;
            update_task(t, &input, &at)?; c.updated_at = at.clone(); let result = c.clone(); audit(s, "task_updated", "close", close_id, task_id); Ok(result)
        }).await
    }

    pub async fn update_close(&self, close_id: &str, input: CloseUpdate) -> FResult<Close> {
        if let Some(name) = &input.name { text(name, "close name", 160, true)?; }
        optional_date(&input.due_date)?;
        for value in [&input.preparer, &input.reviewer, &input.approver].into_iter().flatten() { text(value, "responsibility", 120, false)?; }
        self.mutate(|s| {
            let c = s.closes.iter_mut().find(|c| c.id == close_id).ok_or_else(|| missing("close"))?;
            if c.status != "open" { return Err(conflict("reopen the close before editing responsibilities")); }
            if let Some(name) = &input.name { c.name = name.trim().into(); }
            if let Some(due) = &input.due_date { c.due_date = Some(due.clone()); }
            if let Some(value) = &input.preparer { c.preparer = value.trim().into(); }
            if let Some(value) = &input.reviewer { c.reviewer = value.trim().into(); }
            if let Some(value) = &input.approver { c.approver = value.trim().into(); }
            c.updated_at = now(); let result = c.clone();
            audit(s, "updated", "close", close_id, "Close metadata/responsibilities updated; checklist preserved"); Ok(result)
        }).await
    }

    pub async fn transition_close(&self, close_id: &str, input: TransitionInput) -> FResult<Close> {
        text(&input.note, "transition note", 2000, false)?;
        self.mutate(|s| {
            let c = s.closes.iter_mut().find(|c| c.id == close_id).ok_or_else(|| missing("close"))?;
            if c.status == input.status { return Ok(c.clone()); }
            match (c.status.as_str(), input.status.as_str()) {
                (_, "open") if !input.note.trim().is_empty() => {},
                ("open", "in_review") => {
                    if !c.tasks.iter().all(|t| t.completed) { return Err(conflict("complete every checklist task before review")); }
                    if c.preparer.is_empty() { return Err(bad("a preparer is required before review")); }
                },
                ("in_review", "approved") => { if c.reviewer.is_empty() { return Err(bad("a reviewer is required for approval")); } },
                ("approved", "closed") => { if c.approver.is_empty() { return Err(bad("an approver is required to close")); } },
                (_, "open") => return Err(bad("a note is required when reopening a close")),
                _ => return Err(conflict("invalid close transition; use open -> in_review -> approved -> closed")),
            }
            let previous = c.status.clone(); c.status = input.status.clone(); c.updated_at = now(); let result = c.clone();
            audit(s, "transitioned", "close", close_id, format!("{previous} -> {}: {}", input.status, input.note)); Ok(result)
        }).await
    }

    pub async fn save_account_template(&self, existing: Option<&str>, input: AccountTemplateInput) -> FResult<AccountTemplate> {
        validate_account_template(&input)?;
        let record_id = existing.map(str::to_owned).unwrap_or_else(|| id("coa")); let at = now();
        self.mutate(|s| {
            let prior = s.account_templates.iter().find(|t| t.id == record_id).cloned();
            if existing.is_some() && prior.is_none() { return Err(missing("account template")); }
            if prior.is_none() { capacity(s.account_templates.len(), 100, "account templates")?; }
            let mut history = prior.as_ref().map_or_else(Vec::new, |p| p.history.clone());
            if let Some(p) = &prior {
                capacity(history.len(), 20, "template revisions")?;
                history.push(AccountRevision { version: p.version, name: p.name.clone(), description: p.description.clone(), accounts: p.accounts.clone(), at: p.updated_at.clone() });
            }
            let record = AccountTemplate { id: record_id.clone(), name: input.name.trim().into(), description: input.description.trim().into(), accounts: input.accounts.clone(), version: prior.as_ref().map_or(1, |p| p.version + 1), history, created_at: prior.map_or(at.clone(), |p| p.created_at), updated_at: at.clone() };
            s.account_templates.retain(|t| t.id != record_id); s.account_templates.push(record.clone()); audit(s, "saved", "account_template", &record_id, format!("Version {}", record.version)); Ok(record)
        }).await
    }

    pub async fn preview_account_template(&self, template_id: &str, input: PreviewInput) -> FResult<Value> {
        if input.existing_codes.len() > 500 { return Err(bad("at most 500 existing account codes are allowed")); }
        let state = self.snapshot().await?;
        let template = state.account_templates.iter().find(|t| t.id == template_id).ok_or_else(|| missing("account template"))?;
        let version = input.version.unwrap_or(template.version);
        let accounts = if version == template.version { &template.accounts } else { &template.history.iter().find(|r| r.version == version).ok_or_else(|| missing("template revision"))?.accounts };
        let conflicts: Vec<_> = accounts.iter().filter(|a| input.existing_codes.contains(&a.code)).map(|a| a.code.clone()).collect();
        Ok(json!({"template_id": template_id, "version": version, "accounts": accounts, "conflicts": conflicts, "application_deferred": true, "can_apply": false, "note": "Preview only. No client/company chart or ledger is modified."}))
    }

    pub async fn create_export(&self, input: ExportInput) -> FResult<ExportJob> {
        let export = self.prepare_export(input, None).await?;
        self.mutate(|s| {
            capacity(s.exports.len(), 100, "export artifacts")?;
            s.exports.push(export.clone()); audit(s, "generated", "export", &export.metadata.id, &export.metadata.report); Ok(export.metadata.clone())
        }).await
    }

    pub async fn delete_export(&self, export_id: &str) -> FResult<Value> {
        self.mutate(|s| {
            if !s.exports.iter().any(|e| e.metadata.id == export_id) { return Err(missing("export")); }
            s.exports.retain(|e| e.metadata.id != export_id); audit(s, "deleted", "export", export_id, "CSV artifact removed; execution history preserved"); Ok(json!({"deleted": true}))
        }).await
    }
}

fn validate_account_template(input: &AccountTemplateInput) -> FResult<()> {
    text(&input.name, "account template name", 160, true)?; text(&input.description, "description", 2000, false)?;
    if input.accounts.is_empty() || input.accounts.len() > 500 { return Err(bad("account template requires 1 to 500 accounts")); }
    let mut codes = HashSet::new();
    for a in &input.accounts {
        text(&a.code, "account code", 32, true)?; text(&a.name, "account name", 160, true)?;
        if a.code.trim() != a.code || !a.code.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) { return Err(bad("account codes must use letters, numbers, dots, hyphens or underscores")); }
        if !codes.insert(a.code.to_lowercase()) { return Err(bad("duplicate account code")); }
    }
    for a in &input.accounts {
        let mut visited = HashSet::new(); let mut current = a;
        while let Some(parent) = &current.parent_code {
            if !visited.insert(current.code.clone()) { return Err(bad("account hierarchy contains a cycle")); }
            current = input.accounts.iter().find(|p| p.code == *parent).ok_or_else(|| bad("parent account code does not exist in template"))?;
            if current.kind != a.kind { return Err(bad("parent and child account kinds must match")); }
        }
    }
    Ok(())
}
