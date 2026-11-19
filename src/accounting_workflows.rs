use super::{
    accounting_archive as archive, accounting_authority as authority,
    accounting_dispatch as dispatch, accounting_graph as graph, accounting_release as release,
    capture_close, require_firm_wide, require_individual,
};
use crate::{company_books::CompanyBooks, firm::staff::Identity, referral::routes::ApiError};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

type Books = State<Arc<CompanyBooks>>;
type Actor = Extension<Identity>;
type Output = Result<Json<Value>, ApiError>;

pub(super) fn router() -> Router<Arc<CompanyBooks>> {
    Router::new()
        .route(
            "/api/accounting-extension/archive/{company}/{source}/events",
            post(ingest),
        )
        .route(
            "/api/accounting-extension/archive/{company}/{source}/search",
            post(search),
        )
        .route(
            "/api/accounting-extension/authority/{jurisdiction}",
            get(resolve).post(propose_rule),
        )
        .route(
            "/api/accounting-extension/authority/{jurisdiction}/review",
            post(approve_rule),
        )
        .route(
            "/api/accounting-extension/releases/{profile}",
            get(release_status).post(propose_release),
        )
        .route(
            "/api/accounting-extension/releases/{profile}/review",
            post(review_release),
        )
        .route(
            "/api/accounting-extension/releases/{profile}/suspend",
            post(suspend_release),
        )
        .route(
            "/api/accounting-extension/specialists/run",
            post(specialist),
        )
        .route(
            "/api/accounting-extension/companies/{company}/workpapers/{workpaper}",
            get(inspect_workpaper).post(prepare_workpaper),
        )
        .route(
            "/api/accounting-extension/companies/{company}/workpapers/{workpaper}/review",
            post(review_workpaper),
        )
        .route(
            "/api/accounting-extension/simulations/{company}/{operation}",
            get(read_simulation).post(simulate),
        )
        .route(
            "/api/accounting-extension/simulations/{company}/{operation}/journal",
            post(sandbox_journal),
        )
        .route(
            "/api/accounting-extension/simulations/{company}/{operation}/read-back",
            post(sandbox_read_back),
        )
        .route(
            "/api/accounting-extension/companies/{company}/evidence-packets",
            post(prepare_packet),
        )
        .route(
            "/api/accounting-extension/companies/{company}/evidence-packets/{packet}",
            get(packet_status),
        )
        .layer(DefaultBodyLimit::max(512 * 1024))
}

async fn prepare_workpaper(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, id)): Path<(String, String)>,
    Json(input): Json<super::accounting_workpaper_review::Prepare>,
) -> Output {
    super::accounting_workpaper_review::prepare(
        &books.store,
        &actor,
        &company,
        &id,
        input,
        Utc::now(),
        |source| partition_approved(&company, source).is_ok(),
    )
    .await
    .map(Json)
}

async fn inspect_workpaper(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, id)): Path<(String, String)>,
) -> Output {
    super::accounting_workpaper_review::inspect(
        &books.store,
        &actor,
        &company,
        &id,
        Utc::now(),
        |source| partition_approved(&company, source).is_ok(),
    )
    .await
    .map(Json)
}

async fn review_workpaper(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, id)): Path<(String, String)>,
    Json(input): Json<super::accounting_workpaper_review::Review>,
) -> Output {
    super::accounting_workpaper_review::review(
        &books.store,
        &actor,
        &company,
        &id,
        input,
        Utc::now(),
        |source| partition_approved(&company, source).is_ok(),
    )
    .await
    .map(Json)
}

async fn sandbox_journal(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, operation)): Path<(String, String)>,
    Json(journal): Json<dispatch::SandboxJournal>,
) -> Output {
    require_firm_wide(&actor)?;
    let receipt =
        dispatch::sandbox_post(&books.store, &actor, &company, &operation, journal).await?;
    Ok(Json(
        serde_json::to_value(receipt).map_err(archive::unavailable)?,
    ))
}

async fn sandbox_read_back(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, operation)): Path<(String, String)>,
) -> Output {
    require_firm_wide(&actor)?;
    let record =
        dispatch::sandbox_reconcile(&books.store, &actor, &company, &operation, Utc::now()).await?;
    Ok(Json(
        serde_json::to_value(record).map_err(archive::unavailable)?,
    ))
}

async fn prepare_packet(
    State(books): Books,
    Extension(actor): Actor,
    Path(company): Path<String>,
    Json(request): Json<graph::Request>,
) -> Output {
    require_individual(&actor)?;
    for source in &request.sources {
        partition_approved(&company, source)?;
    }
    graph::prepare(&books.store, &actor, &company, request, Utc::now())
        .await
        .map(Json)
}

async fn packet_status(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, packet)): Path<(String, String)>,
) -> Output {
    require_individual(&actor)?;
    graph::status(
        &books.store,
        &actor,
        &company,
        &packet,
        Utc::now(),
        |company, source| partition_approved(company, source).is_ok(),
    )
    .await
    .map(Json)
}

async fn simulate(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, operation)): Path<(String, String)>,
    Json(command): Json<dispatch::Command>,
) -> Output {
    require_firm_wide(&actor)?;
    if matches!(&command, dispatch::Command::Reconcile { .. })
        && dispatch::read(&books.store, &company, &operation)
            .await?
            .uses_journal_connector()
    {
        return Err(archive::error(
            StatusCode::CONFLICT,
            "sandbox journal requires destination read-back",
        ));
    }
    let record = dispatch::apply(
        &books.store,
        &actor,
        &company,
        &operation,
        command,
        Utc::now(),
    )
    .await?;
    Ok(Json(
        serde_json::to_value(record).map_err(archive::unavailable)?,
    ))
}

async fn read_simulation(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, operation)): Path<(String, String)>,
) -> Output {
    require_firm_wide(&actor)?;
    let record = dispatch::read(&books.store, &company, &operation).await?;
    Ok(Json(
        serde_json::to_value(record).map_err(archive::unavailable)?,
    ))
}

fn partition_approved(company: &str, source: &str) -> Result<(), ApiError> {
    let allowed = std::env::var("NXT_ACCOUNTING_ARCHIVE_APPROVED_PARTITIONS").unwrap_or_default();
    let target = format!("{company}/{source}");
    if !archive::safe_id(company)
        || !archive::safe_id(source)
        || !allowed.split(',').any(|entry| entry.trim() == target)
    {
        return Err(archive::error(
            StatusCode::FORBIDDEN,
            "archive partition is outside approved scope",
        ));
    }
    Ok(())
}

async fn ingest(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, source)): Path<(String, String)>,
    Json(event): Json<archive::Event>,
) -> Output {
    require_firm_wide(&actor)?;
    partition_approved(&company, &source)?;
    if !matches!(books.store.backend_name(), "supabase" | "redis") {
        return Err(archive::error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent archive storage required",
        ));
    }
    let sequence = archive::apply(&books.store, &company, &source, event, Utc::now()).await?;
    Ok(Json(json!({"sequence": sequence})))
}

async fn search(
    State(books): Books,
    Extension(actor): Actor,
    Path((company, source)): Path<(String, String)>,
    Json(input): Json<archive::SearchQuery>,
) -> Output {
    require_individual(&actor)?;
    partition_approved(&company, &source)?;
    let result =
        archive::search(&books.store, &actor, &company, &source, input, Utc::now()).await?;
    Ok(Json(
        serde_json::to_value(result).map_err(archive::unavailable)?,
    ))
}

#[derive(Deserialize)]
struct EffectiveDate {
    effective_on: NaiveDate,
}

async fn resolve(
    State(books): Books,
    Extension(actor): Actor,
    Path(jurisdiction): Path<String>,
    Query(date): Query<EffectiveDate>,
) -> Output {
    require_individual(&actor)?;
    authority::resolve(&books.store, &jurisdiction, date.effective_on, Utc::now())
        .await
        .map(Json)
}

async fn propose_rule(
    State(books): Books,
    Extension(actor): Actor,
    Path(jurisdiction): Path<String>,
    Json(input): Json<authority::RuleInput>,
) -> Output {
    require_firm_wide(&actor)?;
    let hash = authority::propose(&books.store, &actor, &jurisdiction, input, Utc::now()).await?;
    Ok(Json(json!({"node_hash": hash, "state": "pending_review"})))
}

async fn approve_rule(
    State(books): Books,
    Extension(actor): Actor,
    Path(jurisdiction): Path<String>,
    Json(input): Json<authority::Approval>,
) -> Output {
    require_firm_wide(&actor)?;
    authority::approve(&books.store, &actor, &jurisdiction, input, Utc::now()).await?;
    Ok(Json(json!({"state": "published"})))
}

async fn propose_release(
    State(books): Books,
    Extension(actor): Actor,
    Path(profile): Path<String>,
    Json(input): Json<release::Manifest>,
) -> Output {
    require_firm_wide(&actor)?;
    let hash = release::propose(&books.store, &actor, &profile, input, Utc::now()).await?;
    Ok(Json(json!({"manifest_sha256": hash, "state": "draft"})))
}

async fn review_release(
    State(books): Books,
    Extension(actor): Actor,
    Path(profile): Path<String>,
    Json(input): Json<release::ReviewInput>,
) -> Output {
    require_firm_wide(&actor)?;
    release::review(&books.store, &actor, &profile, input, Utc::now()).await?;
    release::status(&books.store, &profile, Utc::now())
        .await
        .map(Json)
}

async fn release_status(
    State(books): Books,
    Extension(actor): Actor,
    Path(profile): Path<String>,
) -> Output {
    require_firm_wide(&actor)?;
    release::status(&books.store, &profile, Utc::now())
        .await
        .map(Json)
}

async fn suspend_release(
    State(books): Books,
    Extension(actor): Actor,
    Path(profile): Path<String>,
) -> Output {
    require_firm_wide(&actor)?;
    release::suspend(&books.store, &actor, &profile).await?;
    release::status(&books.store, &profile, Utc::now())
        .await
        .map(Json)
}

#[derive(Deserialize)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
enum Task {
    RoleWorkpaper {
        company: String,
        workpaper: super::accounting_specialist::Input,
    },
    BankAndBalanceSheetReconciliation {
        company: String,
        statement_cents: i64,
        ledger_cents: i64,
        adjustments_cents: Vec<i64>,
    },
    FinancialModelingAndScenarioAnalysis {
        company: String,
        opening_cents: i64,
        scenarios: std::collections::BTreeMap<String, Vec<i64>>,
    },
    CloseAndGeneralLedger {
        company: String,
    },
    FirmKnowledgeCuration {
        company: String,
        source: String,
        query: String,
        as_of: NaiveDate,
        #[serde(default)]
        embedding: Option<archive::Embedding>,
    },
    TaxAuthorityResearch {
        jurisdiction: String,
        effective_on: NaiveDate,
    },
}

async fn specialist(
    State(books): Books,
    Extension(actor): Actor,
    Json(task): Json<Task>,
) -> Output {
    require_individual(&actor)?;
    let (role, result) = match task {
        Task::RoleWorkpaper { company, workpaper } => {
            require_company(&actor, &company)?;
            return super::accounting_specialist::run(workpaper).map(Json);
        }
        Task::BankAndBalanceSheetReconciliation {
            company,
            statement_cents,
            ledger_cents,
            adjustments_cents,
        } => {
            require_company(&actor, &company)?;
            if adjustments_cents.len() > 500 {
                return Err(archive::error(
                    StatusCode::BAD_REQUEST,
                    "too many reconciling items",
                ));
            }
            let adjustments: i128 = adjustments_cents.into_iter().map(i128::from).sum();
            let difference = i128::from(statement_cents) + adjustments - i128::from(ledger_cents);
            (
                "bank_and_balance_sheet_reconciliation",
                json!({
                    "difference_cents": difference.to_string(),
                    "ties_on_supplied_inputs": difference == 0,
                    "source_verification_required": true,
                }),
            )
        }
        Task::FinancialModelingAndScenarioAnalysis {
            company,
            opening_cents,
            scenarios,
        } => {
            require_company(&actor, &company)?;
            if scenarios.is_empty()
                || scenarios.len() > 20
                || scenarios.iter().any(|(id, periods)| {
                    !archive::safe_id(id) || periods.is_empty() || periods.len() > 120
                })
            {
                return Err(archive::error(
                    StatusCode::BAD_REQUEST,
                    "invalid forecast scenarios",
                ));
            }
            let balances: std::collections::BTreeMap<_, _> = scenarios
                .into_iter()
                .map(|(id, periods)| {
                    let balances: Vec<_> = periods
                        .into_iter()
                        .scan(i128::from(opening_cents), |balance, flow| {
                            *balance += i128::from(flow);
                            Some(balance.to_string())
                        })
                        .collect();
                    (id, balances)
                })
                .collect();
            (
                "financial_modeling_and_scenario_analysis",
                json!({
                    "balances_cents": balances, "assumptions_verified": false,
                }),
            )
        }
        Task::CloseAndGeneralLedger { company } => {
            if !crate::company_books::valid_id(&company) || !actor.can_client(&company) {
                return Err(archive::error(
                    StatusCode::FORBIDDEN,
                    "client scope required",
                ));
            }
            (
                "close_and_general_ledger",
                capture_close(&books, &company).await?,
            )
        }
        Task::FirmKnowledgeCuration {
            company,
            source,
            query,
            as_of,
            embedding,
        } => {
            partition_approved(&company, &source)?;
            let result = archive::search(
                &books.store,
                &actor,
                &company,
                &source,
                archive::SearchQuery {
                    query,
                    as_of,
                    embedding,
                },
                Utc::now(),
            )
            .await?;
            (
                "firm_knowledge_curation",
                serde_json::to_value(result).map_err(archive::unavailable)?,
            )
        }
        Task::TaxAuthorityResearch {
            jurisdiction,
            effective_on,
        } => {
            let result =
                authority::resolve(&books.store, &jurisdiction, effective_on, Utc::now()).await?;
            ("tax_authority_research", result)
        }
    };
    Ok(Json(
        json!({"role": role, "state": "prepared_for_review", "evidence": result, "live_write": false}),
    ))
}

fn require_company(actor: &Identity, company: &str) -> Result<(), ApiError> {
    if !crate::company_books::valid_id(company) || !actor.can_client(company) {
        return Err(archive::error(
            StatusCode::FORBIDDEN,
            "client scope required",
        ));
    }
    Ok(())
}
