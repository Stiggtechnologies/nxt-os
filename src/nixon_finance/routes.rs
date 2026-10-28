//! HTTP surface for Nixon. Every route is behind the workspace key.
//! Execute is refused. Actions are stored as blocked and not applied.

use super::service::{self, ActionRequest, DecisionRequest, NixonError, ReviewRequest};
use crate::referral::routes::{ApiError, require_admin};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

type S = State<super::voice::Scope>;

impl From<NixonError> for ApiError {
    fn from(e: NixonError) -> Self {
        ApiError(
            StatusCode::from_u16(e.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            e.message,
        )
    }
}

fn ok<T: serde::Serialize>(r: Result<T, NixonError>) -> Result<Json<Value>, ApiError> {
    r.map(|v| Json(json!(v))).map_err(ApiError::from)
}

pub fn router(scope: super::voice::Scope) -> Router {
    Router::new()
        .route("/api/nixon/status", get(status))
        .route("/api/nixon/reviews", get(list_reviews).post(create_review))
        .route("/api/nixon/reviews/{id}", get(get_review))
        .route(
            "/api/nixon/reviews/{id}/proposals/{pid}/decide",
            post(decide),
        )
        .route(
            "/api/nixon/reviews/{id}/proposals/{pid}/execute",
            post(execute),
        )
        .route("/api/nixon/actions", post(action))
        .route("/api/nixon/voice", post(voice))
        .route("/api/nixon/voice/realtime", post(realtime))
        .route("/api/nixon/voice/speak", post(speak))
        .layer(middleware::from_fn(require_admin))
        .with_state(scope)
}

async fn status() -> Json<Value> {
    Json(service::status_body())
}

async fn create_review(
    State(ledger): S,
    Json(body): Json<ReviewRequest>,
) -> Result<Json<Value>, ApiError> {
    ok(service::run_review(&ledger.ledger, body).await)
}

async fn list_reviews(State(ledger): S) -> Result<Json<Value>, ApiError> {
    ok(service::list_reviews(&ledger.ledger).await)
}

async fn get_review(State(ledger): S, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    ok(service::get_review(&ledger.ledger, &id).await)
}

async fn decide(
    State(ledger): S,
    Path((id, pid)): Path<(String, String)>,
    Json(body): Json<DecisionRequest>,
) -> Result<Json<Value>, ApiError> {
    ok(service::decide(&ledger.ledger, &id, &pid, body).await)
}

#[derive(Deserialize)]
struct ExecuteBody {
    #[serde(default)]
    actor_name: String,
}

async fn execute(
    State(ledger): S,
    Path((id, pid)): Path<(String, String)>,
    body: Result<Json<ExecuteBody>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let actor = body.map(|b| b.0.actor_name).unwrap_or_default();
    Err(service::refuse_execute(&ledger.ledger, &id, &pid, &actor)
        .await
        .unwrap_err()
        .into())
}

async fn action(
    State(ledger): S,
    Json(body): Json<ActionRequest>,
) -> Result<Json<Value>, ApiError> {
    ok(service::block_action(&ledger.ledger, body).await)
}

async fn voice(State(scope): S, Json(body): Json<super::voice::VoiceTurn>) -> Result<Json<Value>, ApiError> {
    ok(super::voice::handle(&scope, body).await)
}

async fn realtime(State(scope): S, Json(body): Json<super::speech::RealtimeOffer>) -> Result<Json<Value>, ApiError> {
    match super::speech::negotiate(&scope.ledger, body).await {
        Ok(value) => Ok(Json(value)),
        Err(error) if error.message == "realtime_unconfigured" => {
            Ok(Json(json!({ "fallback": "browser", "error": "realtime_unconfigured" })))
        }
        Err(error) => Err(error.into()),
    }
}

async fn speak(
    State(scope): S,
    Json(body): Json<super::speech::SpeakRequest>,
) -> Result<axum::response::Response, ApiError> {
    match super::speech::synthesize(&scope.ledger, body).await {
        Ok(bytes) => Ok((
            [(axum::http::header::CONTENT_TYPE, "audio/mpeg")],
            bytes,
        )
            .into_response()),
        Err(error) if error.message == "tts_unconfigured" => Ok(Json(json!({ "fallback": "browser", "error": "tts_unconfigured" })).into_response()),
        Err(error) => Err(error.into()),
    }
}
