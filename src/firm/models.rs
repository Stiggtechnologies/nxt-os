use serde::{Deserialize, Serialize};
use serde_json::Value;

fn yes() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateTask {
    pub title: String,
    #[serde(default)] pub assignee: String,
    #[serde(default)] pub due_offset_days: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tasks: Vec<TemplateTask>,
    pub version: u32,
    pub builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInput {
    pub name: String,
    #[serde(default)] pub description: String,
    pub tasks: Vec<TemplateTask>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstantiateInput {
    pub name: Option<String>,
    #[serde(default)] pub owner: String,
    pub due_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub assignee: String,
    pub due_date: Option<String>,
    pub completed: bool,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskInput {
    #[serde(default)] pub id: Option<String>,
    pub title: String,
    #[serde(default)] pub assignee: String,
    pub due_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskUpdate {
    pub completed: bool,
    pub assignee: Option<String>,
    pub due_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub due_date: Option<String>,
    pub template_id: Option<String>,
    pub tasks: Vec<Task>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInput {
    pub name: String,
    #[serde(default)] pub description: String,
    #[serde(default)] pub owner: String,
    pub due_date: Option<String>,
    #[serde(default)] pub tasks: Vec<TaskInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Trigger { Manual, Daily, Weekly }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowAction {
    CreateTask { title: String, project_id: Option<String> },
    GenerateReport { report: String, from: Option<String>, to: Option<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub trigger: Trigger,
    pub enabled: bool,
    pub requires_approval: bool,
    pub due_at: Option<String>,
    pub action: WorkflowAction,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowInput {
    pub name: String,
    pub trigger: Trigger,
    #[serde(default = "yes")] pub enabled: bool,
    #[serde(default = "yes")] pub requires_approval: bool,
    pub due_at: Option<String>,
    pub action: WorkflowAction,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    pub idempotency_key: String,
    #[serde(default)] pub dry_run: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickInput { pub now: Option<String> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    #[serde(default)] pub name: String,
    pub workflow_id: String,
    pub idempotency_key: String,
    pub status: String,
    pub approval_id: Option<String>,
    pub action: WorkflowAction,
    pub requires_approval: bool,
    pub created_at: String,
    pub executed_at: Option<String>,
    pub result: Option<Value>,
    #[serde(default)] pub attempts: u32,
    #[serde(default)] pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalInput {
    pub name: String,
    pub idempotency_key: String,
    pub action: WorkflowAction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Approval {
    pub id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub status: String,
    pub decision: Option<String>,
    pub note: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub executed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionInput {
    pub decision: String,
    #[serde(default)] pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Close {
    pub id: String,
    pub name: String,
    pub period_start: String,
    pub period_end: String,
    pub due_date: Option<String>,
    pub preparer: String,
    pub reviewer: String,
    pub approver: String,
    pub status: String,
    pub tasks: Vec<Task>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseInput {
    pub name: String,
    pub period_start: String,
    pub period_end: String,
    pub due_date: Option<String>,
    #[serde(default)] pub preparer: String,
    #[serde(default)] pub reviewer: String,
    #[serde(default)] pub approver: String,
    pub tasks: Option<Vec<TaskInput>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseUpdate {
    pub name: Option<String>,
    pub due_date: Option<String>,
    pub preparer: Option<String>,
    pub reviewer: Option<String>,
    pub approver: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    pub status: String,
    #[serde(default)] pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountBlueprint {
    pub code: String,
    pub name: String,
    pub kind: crate::ledger::models::AccountKind,
    pub parent_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountRevision {
    pub version: u32,
    pub name: String,
    pub description: String,
    pub accounts: Vec<AccountBlueprint>,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub accounts: Vec<AccountBlueprint>,
    pub version: u32,
    pub history: Vec<AccountRevision>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountTemplateInput {
    pub name: String,
    #[serde(default)] pub description: String,
    pub accounts: Vec<AccountBlueprint>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    #[serde(default)] pub existing_codes: Vec<String>,
    pub version: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportInput {
    pub report: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportJob {
    pub id: String,
    pub report: String,
    pub from: String,
    pub to: String,
    pub status: String,
    pub created_at: String,
    pub download_url: String,
    pub source: String,
    pub row_count: usize,
}

// CSV is retained in storage but omitted from public metadata responses by routes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredExport {
    pub metadata: ExportJob,
    pub csv: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub at: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FirmState {
    pub templates: Vec<ProjectTemplate>,
    pub projects: Vec<Project>,
    pub workflows: Vec<Workflow>,
    pub runs: Vec<WorkflowRun>,
    pub approvals: Vec<Approval>,
    pub closes: Vec<Close>,
    pub account_templates: Vec<AccountTemplate>,
    pub exports: Vec<StoredExport>,
    pub audit: Vec<AuditEvent>,
}
