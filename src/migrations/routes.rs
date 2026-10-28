use super::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderValue, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;
type S = State<Arc<Migrations>>;
type R = MResult<Json<Value>>;
pub fn router(state: Arc<Migrations>) -> Router {
    Router::new()
        .route("/api/migrations", get(list))
        .route("/api/migrations/chunks", post(chunk))
        .route("/api/migrations/snapshots", post(publish))
        .route("/api/migrations/{id}", get(detail))
        .route("/api/migrations/{id}/reports/{report_id}/rows", get(rows))
        .route("/api/migrations/{id}/archive", get(archive))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(
            |mut response: Response| async move {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-store"),
                );
                response.headers_mut().insert(
                    header::X_CONTENT_TYPE_OPTIONS,
                    HeaderValue::from_static("nosniff"),
                );
                response
            },
        ))
        .with_state(state)
}
async fn list(State(state): S) -> R {
    Ok(Json(state.list().await?))
}
async fn chunk(State(state): S, Json(input): Json<ChunkInput>) -> R {
    Ok(Json(state.upload_chunk(input).await?))
}
async fn publish(State(state): S, Json(input): Json<SnapshotManifest>) -> R {
    Ok(Json(state.publish(input).await?))
}
async fn detail(State(state): S, Path(id): Path<String>) -> R {
    Ok(Json(state.detail(&id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    // Vercel's /api/:path* rewrite retains this transport query parameter.
    #[serde(default, rename = "path")]
    transport_path: Option<String>,
    #[serde(default = "first_row")]
    from_row: u32,
    #[serde(default = "page_size")]
    limit: usize,
}
fn first_row() -> u32 {
    1
}
fn page_size() -> usize {
    100
}
async fn rows(
    State(state): S,
    Path((id, report_id)): Path<(String, String)>,
    Query(page): Query<Page>,
) -> R {
    if let Some(path) = &page.transport_path {
        let path = path.trim_start_matches('/');
        let path = path.strip_prefix("api/").unwrap_or(path);
        if path != format!("migrations/{id}/reports/{report_id}/rows") {
            return Err(bad("Transport path does not match the requested report"));
        }
    }
    Ok(Json(
        state
            .rows(&id, &report_id, page.from_row, page.limit)
            .await?,
    ))
}
async fn archive(State(state): S, Path(id): Path<String>) -> MResult<Response> {
    let bytes = state.download_archive(&id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"nxt-shadow-{id}.zip\""),
            ),
        ],
        bytes,
    )
        .into_response())
}
