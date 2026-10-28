use super::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::header,
    middleware,
    response::Response,
    routing::{get, post},
};
use std::sync::Arc;
type S = State<Arc<Signatures>>;
type R = SResult<Json<Value>>;
pub fn router(state: Arc<Signatures>) -> Router {
    Router::new()
        .route("/api/signatures/catalog", get(catalog))
        .route("/api/signatures", get(list).post(create))
        .route("/api/signatures/audit", get(audit))
        .route("/api/signatures/{id}", get(detail).put(update))
        .route("/api/signatures/{id}/transition", post(transition))
        .route("/api/signatures/{id}/folder", post(assign_folder))
        .route("/api/signatures/{id}/lock", post(lock_envelope))
        .route("/api/signatures/{id}/unlock", post(unlock_envelope))
        .route("/api/signatures/{id}/bulk-rehearsal", post(bulk))
        .route("/api/signatures/folders", post(create_folder))
        .route("/api/signatures/{id}/template", post(create_template))
        .route("/api/signatures/{id}/package", get(package))
        .route("/api/signatures/templates/{id}", get(template))
        .route(
            "/api/signatures/templates/{id}/instantiate",
            post(instantiate),
        )
        .route("/api/signatures/templates/{id}/favorite", post(favorite))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut()
                .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
            r.headers_mut()
                .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
            r
        }))
        .with_state(state)
}
async fn catalog() -> R {
    Ok(Json(Signatures::catalog()))
}
async fn list(State(s): S) -> R {
    Ok(Json(s.list().await?))
}
async fn detail(State(s): S, Path(id): Path<String>) -> R {
    Ok(Json(s.get(&id).await?))
}
async fn create(State(s): S, Json(v): Json<SaveInput>) -> R {
    Ok(Json(s.save(None, v).await?))
}
async fn update(State(s): S, Path(id): Path<String>, Json(v): Json<SaveInput>) -> R {
    Ok(Json(s.save(Some(&id), v).await?))
}
async fn transition(State(s): S, Path(id): Path<String>, Json(v): Json<TransitionInput>) -> R {
    Ok(Json(s.transition(&id, v).await?))
}
async fn create_template(State(s): S, Path(id): Path<String>, Json(v): Json<TemplateInput>) -> R {
    Ok(Json(s.create_template(&id, v).await?))
}
async fn template(State(s): S, Path(id): Path<String>) -> R {
    Ok(Json(s.template(&id).await?))
}
async fn instantiate(State(s): S, Path(id): Path<String>, Json(v): Json<InstantiateInput>) -> R {
    Ok(Json(s.instantiate(&id, v).await?))
}
async fn package(State(s): S, Path(id): Path<String>) -> R {
    Ok(Json(s.package(&id).await?))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderName { name: String }
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FavoriteInput { favorite: bool }
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkInput { expected_revision: u64, expected_content_hash: String, operator_label: String, note: String, copies: u8 }
async fn create_folder(State(s): S, Json(v): Json<FolderName>) -> R { Ok(Json(s.create_folder(&v.name).await?)) }
async fn assign_folder(State(s): S, Path(id): Path<String>, Json(v): Json<models::TransitionInput>) -> R { Ok(Json(s.assign_folder(&id, v).await?)) }
async fn lock_envelope(State(s): S, Path(id): Path<String>, Json(v): Json<models::TransitionInput>) -> R { Ok(Json(s.set_lock(&id, v, true).await?)) }
async fn unlock_envelope(State(s): S, Path(id): Path<String>, Json(v): Json<models::TransitionInput>) -> R { Ok(Json(s.set_lock(&id, v, false).await?)) }
async fn favorite(State(s): S, Path(id): Path<String>, Json(v): Json<FavoriteInput>) -> R { Ok(Json(s.favorite_template(&id, v.favorite).await?)) }
async fn bulk(State(s): S, Path(id): Path<String>, Json(v): Json<BulkInput>) -> R {
    let input = models::TransitionInput { expected_revision: v.expected_revision, expected_content_hash: v.expected_content_hash, operator_label: v.operator_label, action: "parallel_bulk".into(), note: v.note, field_values: Vec::new() };
    Ok(Json(s.bulk_rehearse(&id, input, v.copies).await?))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQuery {
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
    #[serde(default, rename = "path")]
    transport_path: Option<String>,
}
async fn audit(State(s): S, Query(q): Query<AuditQuery>) -> R {
    if let Some(path) = &q.transport_path {
        let path = path.trim_start_matches('/');
        let path = path.strip_prefix("api/").unwrap_or(path);
        if path != "signatures/audit" {
            return Err(bad("Transport path does not match the requested audit"));
        }
    }
    Ok(Json(s.audit(q.offset, q.limit.unwrap_or(100)).await?))
}
