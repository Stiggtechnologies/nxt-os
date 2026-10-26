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
        .route("/api/signatures/{id}/template", post(create_template))
        .route("/api/signatures/{id}/package", get(package))
        .route("/api/signatures/templates/{id}", get(template))
        .route(
            "/api/signatures/templates/{id}/instantiate",
            post(instantiate),
        )
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
