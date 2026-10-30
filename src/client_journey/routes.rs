use super::capability::SourceProfile;
use super::service::{Journeys, StaffAction};
use super::Actor;
use crate::agent::Agent;
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

pub struct JourneyApi {
    pub journeys: Arc<Journeys>,
    /// None when no model is configured; agent routes then refuse.
    pub agent: Option<Agent>,
}

type S = State<Arc<JourneyApi>>;
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

pub fn router(journeys: Arc<Journeys>, agent: Option<Agent>) -> Router {
    let state = Arc::new(JourneyApi { journeys, agent });
    let api = Router::new()
        .route("/api/journeys", get(list).post(create))
        .route("/api/journeys/coverage", get(coverage))
        .route("/api/journeys/reviews", get(reviews))
        .route("/api/journeys/{client}/work", post(work))
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

async fn list(State(api): S) -> R {
    Ok(Json(api.journeys.list().await?))
}
async fn coverage(State(api): S) -> Json<Value> {
    Json(api.journeys.coverage())
}
async fn create(State(api): S, Json(b): Json<CreateBody>) -> R {
    Ok(Json(api.journeys.create(&b.client_id, b.sources).await?))
}
async fn detail(State(api): S, Path(client): Path<String>) -> R {
    Ok(Json(api.journeys.get(&client).await?))
}
async fn sources(State(api): S, Path(client): Path<String>, Json(b): Json<SourceProfile>) -> R {
    Ok(Json(api.journeys.set_sources(&client, b).await?))
}
async fn advance(State(api): S, Path(client): Path<String>) -> R {
    Ok(Json(api.journeys.advance(&client).await?))
}
async fn reviews(State(api): S) -> R {
    Ok(Json(api.journeys.reviews().await?))
}
async fn work(State(api): S, Path(client): Path<String>) -> R {
    let agent = api.agent.as_ref().ok_or_else(|| ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE, "the firm agent needs an ANTHROPIC_API_KEY configured for this deployment".into()))?;
    Ok(Json(agent.work(&client).await?))
}
async fn staff(State(api): S, Extension(id): Extension<Identity>, Path((client, step)): Path<(String, String)>, Json(b): Json<StaffBody>) -> R {
    if !id.can_client(&client) {
        return Err(ApiError(axum::http::StatusCode::FORBIDDEN, "you are not assigned to this client".into()));
    }
    let actor = Actor::Human { name: id.label(), role: format!("{:?}", id.role) };
    Ok(Json(api.journeys.staff(&client, &step, b.action, actor, id.is_reviewer(), &b.note).await?))
}

/// Scheduled pass over every journey (see vercel.json). Requires CRON_SECRET and
/// NXT_JOURNEY_CRON=on. With NXT_JOURNEY_AGENT=on and a model configured, the
/// agent works each journey; otherwise journeys only advance. Nothing sends or files.
async fn cron_tick(State(api): S, h: HeaderMap) -> Response {
    if let Err(r) = crate::firm::staff::cron_authorized(&h) {
        return r;
    }
    if !std::env::var("NXT_JOURNEY_CRON").is_ok_and(|v| v.trim().eq_ignore_ascii_case("on")) {
        return Json(json!({"skipped": true, "reason": "NXT_JOURNEY_CRON is not on"})).into_response();
    }
    let agent = api.agent.as_ref().filter(|_| std::env::var("NXT_JOURNEY_AGENT").is_ok_and(|v| v.trim().eq_ignore_ascii_case("on")));
    let Some(agent) = agent else {
        return match api.journeys.tick().await {
            Ok(v) => Json(v).into_response(),
            Err(e) => e.into_response(),
        };
    };
    let ids = match api.journeys.list().await {
        Ok(v) => v["journeys"].as_array().cloned().unwrap_or_default(),
        Err(e) => return e.into_response(),
    };
    let mut results = Vec::new();
    for id in ids.iter().filter_map(|j| j["client_id"].as_str()) {
        match agent.work(id).await {
            Ok(v) => results.push(json!({"client_id": id, "worked": v["worked"], "step": v["step"], "escalated": v["escalated"]})),
            Err(e) => results.push(json!({"client_id": id, "error": e.1})),
        }
    }
    Json(json!({"agent": true, "journeys": results.len(), "results": results})).into_response()
}
