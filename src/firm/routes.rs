use super::{Firm, FResult, missing, models::*};
use crate::referral::routes::{ApiError, require_admin};
use axum::{Json, Router, extract::{Path, Query, State}, http::{HeaderValue, header}, middleware, response::{IntoResponse, Response}, routing::{get, post}};
use serde_json::{Value, json};
use std::sync::Arc;

type S = State<Arc<Firm>>;
type R = FResult<Json<Value>>;
fn ok<T: serde::Serialize>(value: T) -> R { Ok(Json(json!(value))) }

pub fn router(firm: Arc<Firm>) -> Router {
    Router::new()
        .route("/api/firm/summary", get(summary))
        .route("/api/firm/templates", get(templates).post(create_template))
        .route("/api/firm/templates/{id}", get(template).put(update_template).delete(delete_template))
        .route("/api/firm/templates/{id}/instantiate", post(instantiate))
        .route("/api/firm/projects", get(projects).post(create_project))
        .route("/api/firm/projects/{id}", get(project).put(update_project).delete(delete_project))
        .route("/api/firm/projects/{id}/tasks/{task_id}", axum::routing::put(project_task))
        .route("/api/firm/workflows", get(workflows).post(create_workflow))
        .route("/api/firm/workflows/tick", post(tick))
        .route("/api/firm/workflows/{id}", get(workflow).put(update_workflow).delete(delete_workflow))
        .route("/api/firm/workflows/{id}/run", post(run))
        .route("/api/firm/runs", get(runs))
        .route("/api/firm/proposals", post(proposal))
        .route("/api/firm/approvals", get(approvals))
        .route("/api/firm/approvals/{id}/decide", post(decide))
        .route("/api/firm/closes", get(closes).post(create_close))
        .route("/api/firm/closes/{id}", get(close).put(update_close))
        .route("/api/firm/closes/{id}/tasks/{task_id}", axum::routing::put(close_task))
        .route("/api/firm/closes/{id}/transition", post(transition))
        .route("/api/firm/account-templates", get(account_templates).post(create_account_template))
        .route("/api/firm/account-templates/{id}", get(account_template).put(update_account_template))
        .route("/api/firm/account-templates/{id}/preview", post(preview))
        .route("/api/firm/account-templates/{id}/apply", post(apply_chart))
        .route("/api/firm/contacts", get(contacts).post(create_contact))
        .route("/api/firm/playbooks", post(playbook))
        .route("/api/firm/exports", get(exports).post(create_export))
        .route("/api/firm/exports/{id}", axum::routing::delete(delete_export))
        .route("/api/firm/exports/{id}/download", get(download))
        .route("/api/firm/spreadsheet-sync", get(spreadsheet))
        .route("/api/firm/integrations", get(integrations))
        .route("/api/firm/audit", get(audit))
        .layer(middleware::from_fn(require_admin))
        .with_state(firm)
}

async fn summary(State(f): S) -> R { ok(f.summary().await?) }
async fn templates(State(f): S) -> R { ok(f.snapshot().await?.templates) }
async fn template(State(f): S, Path(id): Path<String>) -> R { ok(f.snapshot().await?.templates.into_iter().find(|t| t.id == id).ok_or_else(|| missing("template"))?) }
async fn create_template(State(f): S, Json(input): Json<TemplateInput>) -> R { ok(f.save_template(None, input).await?) }
async fn update_template(State(f): S, Path(id): Path<String>, Json(input): Json<TemplateInput>) -> R { ok(f.save_template(Some(&id), input).await?) }
async fn delete_template(State(f): S, Path(id): Path<String>) -> R { ok(f.delete_template(&id).await?) }
async fn instantiate(State(f): S, Path(id): Path<String>, Json(input): Json<InstantiateInput>) -> R { ok(f.instantiate(&id, input).await?) }
async fn projects(State(f): S) -> R { let mut v = f.snapshot().await?.projects; v.reverse(); ok(v) }
async fn project(State(f): S, Path(id): Path<String>) -> R { ok(f.snapshot().await?.projects.into_iter().find(|p| p.id == id).ok_or_else(|| missing("project"))?) }
async fn create_project(State(f): S, Json(input): Json<ProjectInput>) -> R { ok(f.save_project(None, input).await?) }
async fn update_project(State(f): S, Path(id): Path<String>, Json(input): Json<ProjectInput>) -> R { ok(f.save_project(Some(&id), input).await?) }
async fn delete_project(State(f): S, Path(id): Path<String>) -> R { ok(f.delete_project(&id).await?) }
async fn project_task(State(f): S, Path((id, task_id)): Path<(String, String)>, Json(input): Json<TaskUpdate>) -> R { ok(f.project_task(&id, &task_id, input).await?) }
async fn workflows(State(f): S) -> R { let mut v = f.snapshot().await?.workflows; v.reverse(); ok(v) }
async fn workflow(State(f): S, Path(id): Path<String>) -> R { ok(f.snapshot().await?.workflows.into_iter().find(|w| w.id == id).ok_or_else(|| missing("workflow"))?) }
async fn create_workflow(State(f): S, Json(input): Json<WorkflowInput>) -> R { ok(f.save_workflow(None, input).await?) }
async fn update_workflow(State(f): S, Path(id): Path<String>, Json(input): Json<WorkflowInput>) -> R { ok(f.save_workflow(Some(&id), input).await?) }
async fn delete_workflow(State(f): S, Path(id): Path<String>) -> R { ok(f.delete_workflow(&id).await?) }
async fn run(State(f): S, Path(id): Path<String>, Json(input): Json<RunInput>) -> R { ok(f.run(&id, input).await?) }
async fn tick(State(f): S, Json(input): Json<TickInput>) -> R { ok(f.tick(input).await?) }
async fn runs(State(f): S) -> R { let mut v = f.snapshot().await?.runs; v.reverse(); ok(v) }
async fn proposal(State(f): S, Json(input): Json<ProposalInput>) -> R { ok(f.propose_action(input).await?) }
async fn approvals(State(f): S) -> R { let mut v = f.snapshot().await?.approvals; v.reverse(); ok(v) }
async fn decide(State(f): S, Path(id): Path<String>, Json(input): Json<DecisionInput>) -> R { ok(f.decide(&id, input).await?) }
async fn closes(State(f): S) -> R { let mut v = f.snapshot().await?.closes; v.reverse(); ok(v) }
async fn close(State(f): S, Path(id): Path<String>) -> R { ok(f.snapshot().await?.closes.into_iter().find(|c| c.id == id).ok_or_else(|| missing("close"))?) }
async fn create_close(State(f): S, Json(input): Json<CloseInput>) -> R { ok(f.create_close(input).await?) }
async fn update_close(State(f): S, Path(id): Path<String>, Json(input): Json<CloseUpdate>) -> R { ok(f.update_close(&id, input).await?) }
async fn close_task(State(f): S, Path((id, task_id)): Path<(String, String)>, Json(input): Json<TaskUpdate>) -> R { ok(f.close_task(&id, &task_id, input).await?) }
async fn transition(State(f): S, Path(id): Path<String>, Json(input): Json<TransitionInput>) -> R { ok(f.transition_close(&id, input).await?) }
async fn account_templates(State(f): S) -> R { ok(f.snapshot().await?.account_templates) }
async fn account_template(State(f): S, Path(id): Path<String>) -> R { ok(f.snapshot().await?.account_templates.into_iter().find(|t| t.id == id).ok_or_else(|| missing("account template"))?) }
async fn create_account_template(State(f): S, Json(input): Json<AccountTemplateInput>) -> R { ok(f.save_account_template(None, input).await?) }
async fn update_account_template(State(f): S, Path(id): Path<String>, Json(input): Json<AccountTemplateInput>) -> R { ok(f.save_account_template(Some(&id), input).await?) }
async fn preview(State(f): S, Path(id): Path<String>, Json(input): Json<PreviewInput>) -> R { ok(f.preview_account_template(&id, input).await?) }
async fn apply_chart(State(f): S, Path(id): Path<String>, Json(input): Json<ApplyChartInput>) -> R { ok(f.apply_account_template(&id, input).await?) }
async fn contacts(State(f): S) -> R { ok(f.snapshot().await?.directory) }
async fn create_contact(State(f): S, Json(input): Json<DirectoryInput>) -> R { ok(f.save_contact(input).await?) }
async fn playbook(State(f): S, Json(input): Json<PlaybookInput>) -> R { ok(f.publish_playbook(input).await?) }
async fn exports(State(f): S) -> R { let mut v: Vec<_> = f.snapshot().await?.exports.into_iter().map(|e| e.metadata).collect(); v.reverse(); ok(v) }
async fn create_export(State(f): S, Json(input): Json<ExportInput>) -> R { ok(f.create_export(input).await?) }
async fn delete_export(State(f): S, Path(id): Path<String>) -> R { ok(f.delete_export(&id).await?) }

fn csv_response(csv: String, filename: &str) -> FResult<Response> {
    let mut response = csv.into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"" )).map_err(|_| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "invalid export filename".into()))?;
    response.headers_mut().insert(header::CONTENT_DISPOSITION, disposition);
    Ok(response)
}
#[derive(serde::Deserialize)]
struct DownloadFormat { format: Option<String> }

async fn download(State(f): S, Path(id): Path<String>, Query(query): Query<DownloadFormat>) -> FResult<Response> {
    let format = query.format.as_deref().unwrap_or("csv");
    if !matches!(format, "csv" | "xlsx") { return Err(super::bad("download format must be csv or xlsx")); }
    let artifact = f.snapshot().await?.exports.into_iter().find(|e| e.metadata.id == id).ok_or_else(|| missing("export"))?;
    if format == "xlsx" {
        let bytes = super::xlsx::workbook(&artifact.csv).map_err(super::bad)?;
        let mut response = bytes.into_response();
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"));
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        let filename = format!("attachment; filename=\"nxt-{}-{}.xlsx\"", artifact.metadata.report, artifact.metadata.to);
        response.headers_mut().insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&filename).map_err(|_| ApiError(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "invalid export filename".into()))?);
        Ok(response)
    } else { csv_response(artifact.csv, &format!("nxt-{}-{}.csv", artifact.metadata.report, artifact.metadata.to)) }
}
async fn spreadsheet(State(f): S, Query(input): Query<ExportInput>) -> FResult<Response> {
    csv_response(f.spreadsheet_csv(input).await?, "nxt-report-sync.csv")
}

async fn integrations() -> R {
    ok(json!([
        {"id":"ledger","name":"NXT Ledger","configured":true,"status":"connected","capabilities":["read_reports","csv_export","read_only_spreadsheet_sync"],"note":"Internal ledger adapter. No external-account credentials are used."},
        {"id":"quickbooks","name":"QuickBooks Online","configured":false,"status":"not_configured","capabilities":[],"note":"No OAuth connection or external data sync is configured. Client/company connections are deferred."},
        {"id":"excel","name":"Microsoft Excel","configured":false,"status":"not_configured","capabilities":[],"note":"CSV report download is available. No Excel add-in, Microsoft account connection, or write-back adapter is configured."}
    ]))
}
async fn audit(State(f): S) -> R { ok(f.snapshot().await?.audit) }
