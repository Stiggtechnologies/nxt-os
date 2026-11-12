//! Admin API for autonomous execution. Signed-in firm-wide staff only; granting or
//! revoking standing approvals and the kill switch need an admin.

use super::engine::{AutoError, Autonomy};
use super::policy;
use super::tools::{ActionClass, REGISTRY};
use crate::firm::staff::{Identity, Role};
use crate::referral::routes::{ApiError, require_admin};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

type S = State<Arc<Autonomy>>;

impl From<AutoError> for ApiError {
    fn from(e: AutoError) -> Self {
        ApiError(StatusCode::from_u16(e.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), e.message)
    }
}

fn err((status, msg): (u16, String)) -> ApiError {
    ApiError(StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), msg)
}

fn admin_only(id: &Identity) -> Result<(), ApiError> {
    if id.role == Role::Admin { Ok(()) } else { Err(ApiError(StatusCode::FORBIDDEN, "only an admin can change autonomy policy".into())) }
}

fn can_start(id: &Identity) -> Result<(), ApiError> {
    if matches!(id.role, Role::Admin | Role::Reviewer) { Ok(()) } else { Err(ApiError(StatusCode::FORBIDDEN, "only a reviewer or admin can start an autonomy run".into())) }
}

pub fn router(autonomy: Arc<Autonomy>) -> Router {
    Router::new()
        .route("/api/autonomy/status", get(status))
        .route("/api/autonomy/tools", get(tools))
        .route("/api/autonomy/policies", get(list_policies).post(grant))
        .route("/api/autonomy/policies/{class}/revoke", post(revoke))
        .route("/api/autonomy/kill", post(kill))
        .route("/api/autonomy/runs", get(list_runs).post(start))
        .route("/api/autonomy/runs/{id}", get(get_run))
        .route("/api/autonomy/runs/{id}/resume", post(resume))
        .route("/api/autonomy/audit", get(audit))
        .layer(middleware::from_fn(crate::firm::staff::firm_wide_only))
        .layer(middleware::from_fn(require_admin))
        .with_state(autonomy)
}

async fn status(State(a): S) -> Result<Json<Value>, ApiError> {
    let mut v = a.status();
    v["kill_switch"] = json!(a.killed().await?);
    Ok(Json(v))
}

async fn tools() -> Json<Value> {
    Json(json!(REGISTRY))
}

async fn list_policies(State(a): S) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(policy::list(&a.ledger.store).await.map_err(|m| ApiError(StatusCode::INTERNAL_SERVER_ERROR, m))?)))
}

#[derive(Deserialize)]
struct GrantBody {
    action_class: String,
    reason: String,
    days: i64,
    max_runs_per_day: u32,
}

async fn grant(State(a): S, Extension(id): Extension<Identity>, Json(b): Json<GrantBody>) -> Result<Json<Value>, ApiError> {
    admin_only(&id)?;
    let class = ActionClass::parse(&b.action_class).ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "unknown action class".into()))?;
    let result = policy::grant(&a.ledger.store, class, &id.display_name, &b.reason, b.days, b.max_runs_per_day).await;
    match result {
        Ok(p) => {
            a.audit("policy_granted", json!({"class": class.as_str(), "by": id.display_name, "expires_at": p.expires_at, "max_runs_per_day": p.max_runs_per_day})).await?;
            Ok(Json(json!(p)))
        }
        Err(e) => {
            a.audit("policy_grant_refused", json!({"class": class.as_str(), "by": id.display_name, "reason": e.1})).await?;
            Err(err(e))
        }
    }
}

async fn revoke(State(a): S, Extension(id): Extension<Identity>, Path(class): Path<String>) -> Result<Json<Value>, ApiError> {
    admin_only(&id)?;
    let class = ActionClass::parse(&class).ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "unknown action class".into()))?;
    let p = policy::revoke(&a.ledger.store, class, &id.display_name).await.map_err(err)?;
    a.audit("policy_revoked", json!({"class": class.as_str(), "by": id.display_name})).await?;
    Ok(Json(json!(p)))
}

#[derive(Deserialize)]
struct KillBody {
    on: bool,
    #[serde(default)]
    reason: String,
}

async fn kill(State(a): S, Extension(id): Extension<Identity>, Json(b): Json<KillBody>) -> Result<Json<Value>, ApiError> {
    // Anyone firm-wide may stop autonomy; only an admin may switch it back on.
    if !b.on {
        admin_only(&id)?;
    }
    Ok(Json(a.set_kill(b.on, &id.display_name, &b.reason).await?))
}

#[derive(Deserialize)]
struct StartBody {
    review_id: String,
    proposal_id: String,
}

async fn start(State(a): S, Extension(id): Extension<Identity>, Json(b): Json<StartBody>) -> Result<Json<Value>, ApiError> {
    can_start(&id)?;
    Ok(Json(json!(a.start(&b.review_id, &b.proposal_id, &id.display_name).await?)))
}

async fn list_runs(State(a): S) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(a.list_runs().await?)))
}

async fn get_run(State(a): S, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(a.get_run(&id).await?)))
}

async fn resume(State(a): S, Extension(who): Extension<Identity>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    can_start(&who)?;
    Ok(Json(json!(a.resume(&id, &who.display_name).await?)))
}

async fn audit(State(a): S) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(a.audit_log(200).await?)))
}
