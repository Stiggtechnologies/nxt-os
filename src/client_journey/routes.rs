use super::capability::SourceProfile;
use super::service::{Journeys, StaffAction};
use super::Actor;
use crate::firm::staff::Identity;
use crate::referral::routes::ApiError;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

type S = State<Arc<Journeys>>;
type R = Result<Json<Value>, ApiError>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBody {
    client_id: String,
    #[serde(default)]
    sources: SourceProfile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StaffBody {
    action: StaffAction,
    #[serde(default)]
    note: String,
}

pub fn router(state: Arc<Journeys>) -> Router {
    let api = Router::new()
        .route("/api/journeys", get(list).post(create))
        .route("/api/journeys/coverage", get(coverage))
        .route("/api/journeys/{client}", get(detail))
        .route("/api/journeys/{client}/sources", put(sources))
        .route("/api/journeys/{client}/advance", post(advance))
        .route("/api/journeys/{client}/steps/{step}", post(staff))
        .layer(middleware::from_fn(crate::referral::routes::require_admin));
    Router::new()
        .merge(api)
        .route("/api/cron/journey-tick", get(cron_tick))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            r
        }))
        .with_state(state)
}

async fn list(State(j): S) -> R {
    Ok(Json(j.list().await?))
}
async fn coverage(State(j): S) -> Json<Value> {
    Json(j.coverage())
}
async fn create(State(j): S, Json(b): Json<CreateBody>) -> R {
    Ok(Json(j.create(&b.client_id, b.sources).await?))
}
async fn detail(State(j): S, Path(client): Path<String>) -> R {
    Ok(Json(j.get(&client).await?))
}
async fn sources(State(j): S, Path(client): Path<String>, Json(b): Json<SourceProfile>) -> R {
    Ok(Json(j.set_sources(&client, b).await?))
}
async fn advance(State(j): S, Path(client): Path<String>) -> R {
    Ok(Json(j.advance(&client).await?))
}
async fn staff(State(j): S, Extension(id): Extension<Identity>, Path((client, step)): Path<(String, String)>, Json(b): Json<StaffBody>) -> R {
    if !id.can_client(&client) {
        return Err(ApiError(axum::http::StatusCode::FORBIDDEN, "you are not assigned to this client".into()));
    }
    let actor = Actor::Human { name: id.label(), role: format!("{:?}", id.role) };
    Ok(Json(j.staff(&client, &step, b.action, actor, id.is_reviewer(), &b.note).await?))
}

/// Scheduled advance of every journey (see vercel.json). Requires CRON_SECRET and
/// NXT_JOURNEY_CRON=on; capabilities only read, so a tick never sends or files.
async fn cron_tick(State(j): S, h: HeaderMap) -> Response {
    if let Err(r) = crate::firm::staff::cron_authorized(&h) {
        return r;
    }
    if !std::env::var("NXT_JOURNEY_CRON").is_ok_and(|v| v.trim().eq_ignore_ascii_case("on")) {
        return Json(json!({"skipped": true, "reason": "NXT_JOURNEY_CRON is not on"})).into_response();
    }
    match j.tick().await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}
