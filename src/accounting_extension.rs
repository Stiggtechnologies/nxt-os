//! Accounting evidence, scoped retrieval, specialist workflows, and release review.
#[cfg(test)]
#[path = "accounting_acceptance.rs"]
mod accounting_acceptance;
#[path = "accounting_additional_procedures.rs"]
mod accounting_additional_procedures;
#[path = "accounting_archive.rs"]
mod accounting_archive;
#[path = "accounting_authority.rs"]
mod accounting_authority;
#[cfg(test)]
#[path = "accounting_continuity_tests.rs"]
mod accounting_continuity_tests;
#[path = "accounting_dispatch.rs"]
mod accounting_dispatch;
#[path = "accounting_graph.rs"]
mod accounting_graph;
#[cfg(test)]
#[path = "accounting_hybrid_tests.rs"]
mod accounting_hybrid_tests;
#[path = "accounting_release.rs"]
mod accounting_release;
#[cfg(test)]
#[path = "accounting_review_tests.rs"]
mod accounting_review_tests;
#[cfg(test)]
#[path = "accounting_service_procedure_tests.rs"]
mod accounting_service_procedure_tests;
#[path = "accounting_service_procedures.rs"]
mod accounting_service_procedures;
#[path = "accounting_specialist.rs"]
mod accounting_specialist;
#[path = "accounting_workflows.rs"]
mod accounting_workflows;
#[path = "accounting_workpaper_review.rs"]
mod accounting_workpaper_review;
use crate::{
    company_books::CompanyBooks,
    firm::staff::Identity,
    referral::routes::{ApiError, require_admin},
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::sync::Arc;

const RELEASE_GATES: [(&str, &str, [&str; 3]); 7] = [
    (
        "LW-01",
        "Independent accuracy and completeness",
        ["LW-01A", "LW-01B", "LW-01C"],
    ),
    (
        "LW-02",
        "Administrative authority",
        ["LW-02A", "LW-02B", "LW-02C"],
    ),
    (
        "LW-03",
        "Release management",
        ["LW-03A", "LW-03B", "LW-03C"],
    ),
    (
        "LW-04",
        "Manual takeover and corrections",
        ["LW-04A", "LW-04B", "LW-04C"],
    ),
    (
        "LW-05",
        "Archive coverage and permissions",
        ["LW-05A", "LW-05B", "LW-05C"],
    ),
    (
        "LW-06",
        "Firm-specific control matrix",
        ["LW-06A", "LW-06B", "LW-06C"],
    ),
    (
        "LW-07",
        "Operating targets and recovery",
        ["LW-07A", "LW-07B", "LW-07C"],
    ),
];
const AGENT_CATALOG: &str = include_str!("accounting_agent_catalog.json");

pub fn router(books: Arc<CompanyBooks>) -> Router {
    Router::new()
        .route("/api/accounting-extension/readiness", get(readiness))
        .route(
            "/api/accounting-extension/agent-catalog",
            get(agent_catalog),
        )
        .route(
            "/api/accounting-extension/companies/{company}/close-evidence",
            get(close_evidence),
        )
        .merge(accounting_workflows::router())
        .layer(middleware::from_fn(require_admin))
        .layer(middleware::map_response(
            |mut response: Response| async move {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-store"),
                );
                response
            },
        ))
        .with_state(books)
}

fn require_individual(actor: &Identity) -> Result<(), ApiError> {
    if actor.staff_id().is_none() || !actor.mfa || actor.must_change_password {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "verified staff session with MFA required".into(),
        ));
    }
    Ok(())
}

fn require_firm_wide(actor: &Identity) -> Result<(), ApiError> {
    require_individual(actor)?;
    if !actor.all_clients() {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "firm-wide access required".into(),
        ));
    }
    Ok(())
}

async fn readiness(Extension(actor): Extension<Identity>) -> Result<Json<Value>, ApiError> {
    require_firm_wide(&actor)?;
    Ok(Json(release_readiness()))
}

async fn agent_catalog(Extension(actor): Extension<Identity>) -> Result<Response, ApiError> {
    require_firm_wide(&actor)?;
    Ok(([(header::CONTENT_TYPE, "application/json")], AGENT_CATALOG).into_response())
}

fn release_readiness() -> Value {
    let gates: Vec<_> = RELEASE_GATES
        .iter()
        .map(|(id, name, tests)| {
            json!({
                "id": id,
                "name": name,
                "tests": tests,
                "status": "production_evidence_not_configured",
            })
        })
        .collect();
    json!({
        "mode": "read_only",
        "live_writes_enabled": false,
        "archive_ingestion_available": true,
        "production_release_review_available": true,
        "production_dispatch_available": false,
        "gates": gates,
    })
}

async fn close_evidence(
    State(books): State<Arc<CompanyBooks>>,
    Extension(actor): Extension<Identity>,
    Path(company): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_individual(&actor)?;
    if !crate::company_books::valid_id(&company) || !actor.can_client(&company) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "client scope required".into(),
        ));
    }
    capture_close(&books, &company).await.map(Json)
}

pub(crate) async fn capture_close(books: &CompanyBooks, company: &str) -> Result<Value, ApiError> {
    let trial = books.trial_balance(company).await?;
    let head = trial["head_sha256"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "committed evidence version unavailable".into(),
            )
        })?;
    Ok(close_summary(&trial, head))
}

fn integer(value: &Value) -> Option<i128> {
    let number = value.as_str()?.parse::<i128>().ok()?;
    (number >= 0).then_some(number)
}

fn combined_matches(trial: &Value, field: &str, parse: impl Fn(&Value) -> Option<i128>) -> bool {
    let baseline = parse(&trial["source_baseline_analysis"][field]);
    let delta = parse(&trial["native_delta"][field]);
    let total = parse(&trial["analysis"][field]);
    matches!((baseline, delta, total), (Some(b), Some(d), Some(t)) if b.checked_add(d) == Some(t))
}

fn close_summary(trial: &Value, head: &str) -> Value {
    let debit = integer(&trial["analysis"]["total_debit_cents"]);
    let credit = integer(&trial["analysis"]["total_credit_cents"]);
    let checks = [
        ("trial_balance_balanced", debit.is_some() && debit == credit),
        (
            "journal_entry_count_ties",
            combined_matches(trial, "entries", |v| v.as_u64().map(i128::from)),
        ),
        (
            "journal_line_count_ties",
            combined_matches(trial, "lines", |v| v.as_u64().map(i128::from)),
        ),
        (
            "debit_total_ties",
            combined_matches(trial, "total_debit_cents", integer),
        ),
        (
            "credit_total_ties",
            combined_matches(trial, "total_credit_cents", integer),
        ),
        (
            "source_import_controls_passed",
            trial["reconciliation"]["source_baseline_unchanged"] == true
                && trial["reconciliation"]["source_controls_passed"] == true,
        ),
    ];
    let checks: Vec<_> = checks.into_iter().map(|(id, passed)| json!({
        "id": id,
        "passed": passed,
        "evidence": {
            "source": "company_books",
            "head_sha256": head,
            "sections": ["analysis", "source_baseline_analysis", "native_delta", "reconciliation"],
        },
    })).collect();
    json!({
        "version": 1,
        "head_sha256": head,
        "source_evidence": {
            "archive_sha256": trial["source"]["archive_sha256"],
            "journal_sha256": trial["source"]["journal_sha256"],
            "source_trial_balance_sha256": trial["source"]["trial_balance_sha256"],
            "period_start": trial["source"]["period_start"],
            "period_end": trial["source"]["period_end"],
        },
        "total_debit_cents": trial["analysis"]["total_debit_cents"],
        "total_credit_cents": trial["analysis"]["total_credit_cents"],
        "locked_through": trial["head"]["locked_through"],
        "review_status": trial["head"]["review"]["status"],
        "checks": checks,
        "not_checked": [
            "independent source completeness",
            "bank reconciliation",
            "subledger allocations",
            "open exceptions",
        ],
        "release_authorized": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firm::staff::Role;
    use std::collections::HashSet;

    #[test]
    fn agent_catalog_has_all_82_unique_profiles() {
        let profiles: Vec<Value> = serde_json::from_str(AGENT_CATALOG).unwrap();
        let mut names = HashSet::new();
        let mut groups = HashSet::new();
        for profile in &profiles {
            assert!(names.insert(profile["role"].as_str().unwrap()));
            groups.insert(profile["group"].as_str().unwrap());
            assert!(!profile["output"].as_str().unwrap().is_empty());
        }
        assert_eq!(names.len(), 82);
        assert_eq!(groups.len(), 8);
    }

    #[test]
    fn readiness_has_every_gate_and_case_but_cannot_authorize_live_writes() {
        let state = release_readiness();
        assert_eq!(state["live_writes_enabled"], false);
        let gates = state["gates"].as_array().unwrap();
        assert_eq!(gates.len(), 7);
        for (index, gate) in gates.iter().enumerate() {
            let id = format!("LW-{:02}", index + 1);
            assert_eq!(gate["id"], id);
            assert_eq!(
                gate["tests"],
                json!(["A", "B", "C"].map(|suffix| format!("{id}{suffix}")))
            );
            assert_eq!(gate["status"], "production_evidence_not_configured");
        }
    }

    #[test]
    fn missing_or_omitted_batch_is_detected_even_when_debits_equal_credits() {
        let ledger = json!({
            "analysis": {"entries": 7, "lines": 13, "total_debit_cents": "80", "total_credit_cents": "80"},
            "source_baseline_analysis": {"entries": 7, "lines": 13, "total_debit_cents": "80", "total_credit_cents": "80"},
            "native_delta": {"entries": 1, "lines": 2, "total_debit_cents": "20", "total_credit_cents": "20"},
            "reconciliation": {"source_baseline_unchanged": true, "source_controls_passed": true}
        });
        let result = close_summary(&ledger, "test-head");
        let checks = result["checks"].as_array().unwrap();
        assert_eq!(checks[0]["passed"], true);
        assert!(checks[1..5].iter().all(|check| check["passed"] == false));
        assert_eq!(result["release_authorized"], false);
        let missing = close_summary(&Value::Null, "test-head");
        assert!(
            missing["checks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|check| check["passed"] == false)
        );
    }

    #[test]
    fn only_current_individual_mfa_sessions_can_read() {
        assert!(require_individual(&Identity::break_glass()).is_err());
        let mut actor = Identity {
            user_id: "usr_a".into(),
            display_name: "Staff".into(),
            role: Role::Reviewer,
            clients: vec!["company-a".into()],
            break_glass: false,
            mfa: false,
            must_change_password: false,
        };
        assert!(require_individual(&actor).is_err());
        actor.mfa = true;
        assert!(require_individual(&actor).is_ok());
        assert!(require_firm_wide(&actor).is_err());
        actor.must_change_password = true;
        assert!(require_individual(&actor).is_err());
    }
}
