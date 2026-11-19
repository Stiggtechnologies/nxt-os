//! Fixed, synthetic acceptance fixtures for the protected accounting Preview.
use super::{
    accounting_acceptance as acceptance, accounting_archive as archive,
    accounting_continuity_tests as continuity, accounting_hybrid_tests as hybrid,
    accounting_review_tests as workpapers, missing_batch_case, require_firm_wide,
};
use crate::{
    company_books::{CompanyBooks, acceptance_fixture::accounting_close_evidence_case},
    firm::staff::{CreateUser, Identity, Role, Staff},
    referral::routes::{ApiError, constant_time_eq}, store::Store,
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::{Duration, Instant}};

const BRANCH: &str = "review/accounting-extension-staging";
const COMMIT_HEADER: &str = "x-nxt-acceptance-commit";
const DEPLOYMENT_HEADER: &str = "x-nxt-acceptance-deployment";
const INITIAL_ADMIN: &str = "accounting-stage-admin-a";
const BOOTSTRAP_CLAIM: &str = "accounting:hosted-acceptance:initial-admin:v1";

fn allowed(flag: Option<&str>, environment: Option<&str>, branch: Option<&str>) -> bool {
    flag == Some("on") && environment == Some("preview") && branch == Some(BRANCH)
}

fn enabled() -> bool {
    allowed(
        std::env::var("NXT_ACCOUNTING_EXTENSION").ok().as_deref(),
        std::env::var("VERCEL_ENV").ok().as_deref(),
        std::env::var("VERCEL_GIT_COMMIT_REF").ok().as_deref(),
    )
}

pub(super) fn router() -> Router<Arc<CompanyBooks>> {
    if !enabled() {
        return Router::new();
    }
    Router::new().route("/api/accounting-extension/hosted-acceptance/{case_id}", post(execute))
}

pub(super) fn bootstrap_router() -> Router<Arc<CompanyBooks>> {
    if !enabled() || bootstrap_digest().is_none() {
        return Router::new();
    }
    Router::new()
        .route("/api/accounting-extension/hosted-bootstrap", post(bootstrap))
        .layer(DefaultBodyLimit::max(1024))
}

fn hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn bootstrap_digest() -> Option<String> {
    let digest = std::env::var("NXT_ACCOUNTING_STAGE_BOOTSTRAP_SHA256").ok()?;
    let shared_access = crate::firm::staff::break_glass_enabled()
        && std::env::var("ADMIN_API_KEY").is_ok_and(|key| !key.trim().is_empty());
    (!shared_access && hex_digest(&digest)).then_some(digest)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    temporary_password: String,
}

async fn bootstrap(
    State(books): State<Arc<CompanyBooks>>,
    headers: HeaderMap,
    Json(input): Json<Bootstrap>,
) -> Result<Json<Value>, ApiError> {
    let rejected = || ApiError(StatusCode::UNAUTHORIZED, "staging bootstrap authorization required".into());
    let expected = bootstrap_digest().ok_or_else(rejected)?;
    let ticket = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer ")).filter(|value| hex_digest(value))
        .ok_or_else(rejected)?;
    if !constant_time_eq(archive::sha256(ticket.as_bytes()).as_bytes(), expected.as_bytes()) {
        return Err(rejected());
    }
    pinned_deployment(&headers)?;
    if !hex_digest(&input.temporary_password) || input.temporary_password == ticket {
        return Err(ApiError(StatusCode::BAD_REQUEST, "a separate random staging password is required".into()));
    }
    verify_redis(&fixture(&books.store)?).await?;
    let staff = Staff::new(books.store.clone());
    let claimed = || ApiError(StatusCode::CONFLICT, "initial staging credentials were already issued".into());
    if !staff.users().await?.is_empty()
        || !books.store.set_nx(BOOTSTRAP_CLAIM, "claimed").await.map_err(|_| storage_unavailable())? {
        return Err(claimed());
    }
    let result = staff.create_user(&Identity::break_glass(), CreateUser {
        username: INITIAL_ADMIN.into(),
        display_name: "Accounting Staging Admin A".into(),
        role: Role::Admin,
        clients: vec!["*".into()],
        temporary_password: input.temporary_password,
    }).await?;
    Ok(Json(result))
}

fn fixture(store: &Store) -> Result<Store, ApiError> {
    let configured_url = std::env::var("UPSTASH_REDIS_REST_URL").ok();
    let Store::Redis { url, fixture_namespace: None, .. } = store else {
        return Err(storage_unavailable());
    };
    let verified_url = url::Url::parse(url).ok().filter(|parsed| {
        parsed.scheme() == "https" && parsed.username().is_empty() && parsed.password().is_none()
            && parsed.host_str().is_some_and(|host| host.ends_with(".upstash.io"))
            && parsed.path() == "/" && parsed.port().is_none()
            && parsed.query().is_none() && parsed.fragment().is_none()
    });
    if verified_url.is_none() || configured_url.as_deref().map(|value| value.trim_end_matches('/')) != Some(url)
        || std::env::var_os("KV_REST_API_URL").is_some()
        || std::env::var_os("KV_REST_API_TOKEN").is_some() {
        return Err(storage_unavailable());
    }
    store.redis_fixture().map_err(|_| storage_unavailable())
}

fn storage_unavailable() -> ApiError {
    ApiError(StatusCode::SERVICE_UNAVAILABLE, "branch-specific Upstash storage is required".into())
}

async fn verify_redis(store: &Store) -> Result<(), ApiError> {
    let value = uuid::Uuid::new_v4().to_string();
    let key = "redis-probe";
    let changed = store.compare_exchange(key, None, &value).await.map_err(|_| storage_unavailable())?;
    let rejected = store.compare_exchange(key, None, "unexpected").await.map_err(|_| storage_unavailable())?;
    let read_back = store.get(key).await.map_err(|_| storage_unavailable())?;
    if changed && !rejected && read_back.as_deref() == Some(value.as_str()) {
        Ok(())
    } else {
        Err(storage_unavailable())
    }
}

async fn execute(
    State(books): State<Arc<CompanyBooks>>,
    Extension(actor): Extension<Identity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    require_firm_wide(&actor)?;
    if id != "probe" && !super::RELEASE_GATES.iter().any(|(_, _, ids)| ids.contains(&id.as_str())) {
        return Err(ApiError(StatusCode::NOT_FOUND, "unknown acceptance case".into()));
    }
    let (deployment, commit) = pinned_deployment(&headers)?;
    verify_redis(&fixture(&books.store)?).await?;
    let started = Instant::now();
    let result = if id == "probe" {
        "pass"
    } else {
        let original = fixture(&books.store)?;
        let restore = if id == "LW-07B" { Some((fixture(&books.store)?, fixture(&books.store)?)) } else { None };
        let case = id.clone();
        let mut task = tokio::spawn(async move { run(&case, original, restore).await });
        match tokio::time::timeout(Duration::from_secs(90), &mut task).await {
            Ok(Ok(())) => "pass",
            Ok(Err(_)) => "fail",
            Err(_) => {
                task.abort();
                "timeout"
            }
        }
    };
    Ok(Json(json!({
        "case_id": id,
        "result": result,
        "classification": "hosted_synthetic_sandbox_only",
        "branch": BRANCH,
        "deployment_id": deployment,
        "commit_sha": commit,
        "storage": "redis",
        "redis_round_trip_verified": true,
        "executor_staff_id": actor.user_id,
        "elapsed_ms": started.elapsed().as_millis(),
        "continuity_fixture_sha256": archive::sha256(continuity::FIXTURE.replace("\r\n", "\n").as_bytes()),
        "live_writes_enabled": false,
        "production_dispatch_available": false,
    })))
}

fn pinned_deployment(headers: &HeaderMap) -> Result<(String, String), ApiError> {
    let deployment = std::env::var("VERCEL_DEPLOYMENT_ID").map_err(|_| deployment_unavailable())?;
    let commit = std::env::var("VERCEL_GIT_COMMIT_SHA").map_err(|_| deployment_unavailable())?;
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) || deployment.is_empty() {
        return Err(deployment_unavailable());
    }
    if headers.get(COMMIT_HEADER).and_then(|value| value.to_str().ok()) != Some(commit.as_str())
        || headers.get(DEPLOYMENT_HEADER).and_then(|value| value.to_str().ok()) != Some(deployment.as_str()) {
        return Err(ApiError(StatusCode::PRECONDITION_FAILED, "accounting preview deployment changed".into()));
    }
    Ok((deployment, commit))
}

fn deployment_unavailable() -> ApiError {
    ApiError(StatusCode::SERVICE_UNAVAILABLE, "hosted deployment identity unavailable".into())
}

async fn run(id: &str, store: Store, restore: Option<(Store, Store)>) {
    use acceptance::*;
    match id {
        "LW-01A" => missing_batch_case(),
        "LW-01B" => accounting_close_evidence_case(store).await,
        "LW-01C" => authority_requires_independent_approval_and_keeps_effective_periods_case(store).await,
        "LW-02A" => release_blocks_missing_acceptance_case_and_rejects_self_review_case(store).await,
        "LW-02B" => workpapers::workpaper_rejects_unbound_inputs_self_review_digest_changes_and_revoked_sources_case(store).await,
        "LW-02C" => continuity::sandbox_lw_02c_emergency_expires_and_cannot_restore_authority_case(store).await,
        "LW-03A" => continuity::sandbox_lw_03a_rejects_unpinned_connector_and_model_before_dispatch_case(store).await,
        "LW-03B" | "LW-06C" => evidence_graph_tracks_new_authority_and_archive_revocation_without_exposing_other_owners_case(store).await,
        "LW-03C" => continuity::sandbox_lw_03c_rollback_preserves_revocations_and_completed_receipts_case(store).await,
        "LW-04A" => sandbox_lw_04a_manual_and_automated_claim_are_exclusive_case(store).await,
        "LW-04B" => hybrid::sandbox_journal_posts_once_and_reads_back_after_unknown_outcome_case(store).await,
        "LW-04C" => sandbox_lw_04b_and_04c_uncertainty_requires_reconciliation_and_correction_links_original_case(store).await,
        "LW-05A" => archive_coverage_and_permission_expiry_are_distinct_case(store).await,
        "LW-05B" => archive_uses_client_scope_and_document_grant_and_rechecks_after_revocation_case(store).await,
        "LW-05C" => archive_hold_prevents_disposition_then_disposition_removes_text_case(store).await,
        "LW-06A" => production_case_inventory_covers_all_seven_gates_case(),
        "LW-06B" => release_demands_exact_version_and_remains_blocked_even_with_21_reported_cases_case(store).await,
        "LW-07A" => continuity::sandbox_lw_07a_peak_drains_within_fixture_queue_and_deadline_targets_case(store).await,
        "LW-07B" => {
            let (provider, restored) = restore.expect("recovery requires separate isolated stores");
            continuity::sandbox_lw_07b_restore_reapplies_revocation_and_hold_then_reconciles_provider_receipt_case(store, provider, restored).await;
        }
        "LW-07C" => continuity::sandbox_lw_07c_provider_outage_routes_to_exclusive_manual_fallback_before_deadline_case(store).await,
        _ => unreachable!("route validates acceptance case inventory"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_runner_requires_the_exact_preview_and_opt_in() {
        assert!(allowed(Some("on"), Some("preview"), Some(BRANCH)));
        for (flag, environment, branch) in [
            (None, Some("preview"), Some(BRANCH)),
            (Some("on"), Some("production"), Some(BRANCH)),
            (Some("on"), Some("preview"), Some("main")),
            (Some("on"), None, None),
        ] {
            assert!(!allowed(flag, environment, branch));
        }
    }

    #[tokio::test]
    async fn hosted_dispatch_table_runs_all_registered_cases() {
        for (_, _, cases) in super::super::RELEASE_GATES {
            for id in cases {
                let restore = (id == "LW-07B").then(|| (Store::memory(), Store::memory()));
                run(id, Store::memory(), restore).await;
            }
        }
    }
}
