use super::{ImportError, OpenBatch, ParallelInput, SourceImport};
use axum::{
    Json, Router,
    extract::{Path, State},
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
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut response: Response| async move {
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            response
        }))
        .with_state(state)
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
