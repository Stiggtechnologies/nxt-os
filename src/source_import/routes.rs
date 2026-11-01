use super::files::{ClientLinkInput, FileInput, ResolveInput};
use super::{ImportError, OpenBatch, ParallelInput, SourceImport};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

type S = State<Arc<SourceImport>>;

impl IntoResponse for ImportError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1, "live_cutover_allowed": false, "filing_enabled": false, "write_back": false }))).into_response()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkBody {
    seq: u32,
    records: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitBody {
    request_id: String,
    reviewer_label: String,
}

pub fn router(state: Arc<SourceImport>) -> Router {
    Router::new()
        .route("/api/source-imports/coverage", get(coverage))
        .route("/api/source-imports/{lane}/batches", post(open))
        .route("/api/source-imports/{lane}/batches/{batch}", get(progress))
        .route("/api/source-imports/{lane}/batches/{batch}/chunks", post(chunk))
        .route("/api/source-imports/{lane}/batches/{batch}/commit", post(commit))
        .route("/api/source-imports/{lane}/batches/{batch}/recovery", get(recover))
        .route("/api/source-imports/{lane}/batches/{batch}/parallel", post(parallel).get(latest_parallel))
        .route("/api/source-imports/dtmax/companies/{company}/clients", post(register_clients))
        .route("/api/source-imports/dtmax/companies/{company}/clients/{client}/files", get(client_files))
        .route("/api/source-imports/dtmax/companies/{company}/files", post(preserve_file))
        .route("/api/source-imports/dtmax/companies/{company}/files/unmatched", get(unmatched))
        .route("/api/source-imports/dtmax/companies/{company}/files/{sha}", get(source_file))
        .route("/api/source-imports/dtmax/companies/{company}/files/{sha}/content", get(source_bytes))
        .route("/api/source-imports/dtmax/companies/{company}/files/{sha}/link", post(resolve))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients", post(test_copy_clients).get(test_copy_client_page))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/returns", post(test_copy_returns))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/t5008", post(test_copy_slips))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/summary", get(test_copy_summary))
        .layer(axum::extract::DefaultBodyLimit::max(super::files::MAX_FILE_BYTES * 2 + 64 * 1024))
        .layer(middleware::from_fn(client_scope))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut response: Response| async move {
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            response
        }))
        .with_state(state)
}

/// Client scoping: only `.../clients/{client}/files` names a client, and the coverage catalog
/// holds no client data. Batches, unmatched files and the test copy are firm-wide.
async fn client_scope(req: axum::extract::Request, next: middleware::Next) -> Response {
    let Some(id) = crate::firm::staff::scoped(&req) else { return next.run(req).await };
    let path = req.uri().path();
    if path == "/api/source-imports/coverage" { return next.run(req).await; }
    let client = path.strip_prefix("/api/source-imports/dtmax/companies/")
        .and_then(|rest| rest.split_once("/clients/"))
        .and_then(|(_, rest)| rest.strip_suffix("/files"));
    match client {
        Some(c) if id.can_client(c) => next.run(req).await,
        Some(_) => (axum::http::StatusCode::FORBIDDEN, Json(json!({"error": "you are not assigned to this client"}))).into_response(),
        None => crate::firm::staff::firm_wide_only(req, next).await,
    }
}

async fn coverage() -> Json<Value> {
    Json(SourceImport::coverage())
}

async fn open(State(state): S, Path(lane): Path<String>, Json(batch): Json<OpenBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.open(&lane, batch).await?))
}

async fn progress(State(state): S, Path((lane, batch)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.progress(&lane, &batch).await?))
}

async fn chunk(State(state): S, Path((lane, batch)): Path<(String, String)>, Json(body): Json<ChunkBody>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.chunk(&lane, &batch, body.seq, &body.records).await?))
}

async fn commit(State(state): S, Path((lane, batch)): Path<(String, String)>, Json(body): Json<CommitBody>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.commit(&lane, &batch, &body.request_id, &body.reviewer_label).await?))
}

async fn recover(State(state): S, Path((lane, batch)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.recover(&lane, &batch).await?))
}

async fn parallel(State(state): S, Path((lane, batch)): Path<(String, String)>, Json(body): Json<ParallelInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.parallel(&lane, &batch, body).await?))
}

async fn latest_parallel(State(state): S, Path((lane, batch)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.latest_parallel(&lane, &batch).await?))
}

async fn register_clients(State(state): S, Path(company): Path<String>, Json(body): Json<ClientLinkInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.register_client_links(&company, body).await?))
}

async fn preserve_file(State(state): S, Path(company): Path<String>, Json(body): Json<FileInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.preserve_file(&company, body).await?))
}

async fn unmatched(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.unmatched_files(&company).await?))
}

async fn source_file(State(state): S, Path((company, sha)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.source_file(&company, &sha).await?))
}

async fn source_bytes(State(state): S, Path((company, sha)): Path<(String, String)>) -> Result<Response, ImportError> {
    let bytes = state.source_file_bytes(&company, &sha).await?;
    Ok(([(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"))], bytes).into_response())
}

async fn resolve(State(state): S, Path((company, sha)): Path<(String, String)>, Json(body): Json<ResolveInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.resolve_file(&company, &sha, body).await?))
}

async fn client_files(State(state): S, Path((company, client)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.client_files(&company, &client).await?))
}

async fn test_copy_clients(State(state): S, Path(company): Path<String>, Json(body): Json<super::test_copy::ClientBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_clients(&company, body).await?))
}

#[derive(Deserialize)]
struct ClientPage {
    #[serde(default)]
    offset: usize,
    #[serde(default = "client_page_limit")]
    limit: usize,
}
fn client_page_limit() -> usize { 50 }

async fn test_copy_client_page(State(state): S, Path(company): Path<String>, Query(page): Query<ClientPage>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_client_page(&company, page.offset, page.limit).await?))
}

async fn test_copy_returns(State(state): S, Path(company): Path<String>, Json(body): Json<super::test_copy::ReturnBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_returns(&company, body).await?))
}

async fn test_copy_slips(State(state): S, Path(company): Path<String>, Json(body): Json<super::test_copy::SlipBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_slips(&company, body).await?))
}

async fn test_copy_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_summary(&company).await?))
}
