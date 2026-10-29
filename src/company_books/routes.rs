use super::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderValue, header},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;
type S = State<Arc<CompanyBooks>>;
type R = Result<Json<Value>>;
pub fn router(state: Arc<CompanyBooks>) -> Router {
    let roster = super::roster::router(state.clone());
    Router::new()
        .route("/api/company-books", get(list))
        .route("/api/company-books/{company}", get(detail))
        .route("/api/company-books/{company}/chunks", post(chunk))
        .route(
            "/api/company-books/{company}/imports/preview",
            post(preview),
        )
        .route("/api/company-books/{company}/imports/commit", post(commit))
        .route(
            "/api/company-books/{company}/imports/{preview}",
            get(preview_detail),
        )
        .route("/api/company-books/{company}/entries", get(entries))
        .route("/api/company-books/{company}/journals", post(journal))
        .route(
            "/api/company-books/{company}/journals/{entry}/reverse",
            post(reverse),
        )
        .route(
            "/api/company-books/{company}/period-lock",
            post(lock_period),
        )
        .route("/api/company-books/{company}/parallel/classes", post(add_class))
        .route("/api/company-books/{company}/parallel/documents", post(open_document))
        .route("/api/company-books/{company}/parallel/payments", post(apply_payment))
        .route("/api/company-books/{company}/parallel/subledger", get(subledger))
        .route("/api/company-books/{company}/parallel/journals/{entry}/sparse", post(sparse_native))
        .route("/api/company-books/{company}/parallel/journals/{entry}/delete", post(delete_native))
        .route("/api/company-books/{company}/parallel/attachments", post(attach))
        .route("/api/company-books/{company}/parallel/sales-tax", post(sales_tax))
        .route("/api/company-books/{company}/parallel/departments", post(add_department))
        .route("/api/company-books/{company}/parallel/items", post(add_item))
        .route("/api/company-books/{company}/parallel/employees", post(add_employee))
        .route("/api/company-books/{company}/parallel/estimates", post(add_estimate))
        .route("/api/company-books/{company}/parallel/purchase-orders", post(add_purchase_order))
        .route("/api/company-books/{company}/parallel/time", post(add_time))
        .route("/api/company-books/{company}/parallel/budgets", post(add_budget))
        .route("/api/company-books/{company}/parallel/payroll", post(payroll_run))
        .route("/api/company-books/{company}/parallel/daily", get(daily))
        .route(
            "/api/company-books/{company}/register/{account}",
            get(register),
        )
        .route("/api/company-books/{company}/trial-balance", get(tb))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            r.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            r
        }))
        .with_state(state)
        .merge(roster)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryPage {
    #[serde(default)]
    offset: usize,
    #[serde(default = "inventory_limit")]
    limit: usize,
    #[serde(default, rename = "path")]
    transport_path: Option<String>,
}
fn inventory_limit() -> usize {
    25
}
async fn list(State(s): S, Query(p): Query<InventoryPage>) -> R {
    if p.transport_path.as_deref().is_some_and(|v| {
        v.trim_start_matches('/')
            .strip_prefix("api/")
            .unwrap_or(v.trim_start_matches('/'))
            != "company-books"
    }) {
        return Err(bad("Transport path mismatch"));
    }
    Ok(Json(s.list_page(p.offset, p.limit).await?))
}
async fn detail(State(s): S, Path(c): Path<String>, Query(p): Query<InventoryPage>) -> R {
    if p.transport_path.as_deref().is_some_and(|v| {
        v.trim_start_matches('/')
            .strip_prefix("api/")
            .unwrap_or(v.trim_start_matches('/'))
            != format!("company-books/{c}")
    }) {
        return Err(bad("Transport path mismatch"));
    }
    Ok(Json(s.detail_page(&c, p.offset, p.limit).await?))
}
async fn chunk(State(s): S, Path(c): Path<String>, Json(v): Json<Chunk>) -> R {
    Ok(Json(s.upload(&c, v).await?))
}
async fn preview(State(s): S, Path(c): Path<String>, Json(v): Json<PreviewInput>) -> R {
    Ok(Json(s.preview(&c, v).await?))
}
async fn commit(State(s): S, Path(c): Path<String>, Json(v): Json<CommitInput>) -> R {
    Ok(Json(s.commit(&c, v).await?))
}
async fn preview_detail(State(s): S, Path((c, p)): Path<(String, String)>) -> R {
    Ok(Json(s.preview_detail(&c, &p).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
    #[serde(default = "page_bytes")]
    max_bytes: usize,
    #[serde(default, rename = "path")]
    transport_path: Option<String>,
}
fn limit() -> usize {
    100
}
fn page_bytes() -> usize {
    DEFAULT_PAGE_BYTES
}
impl Page {
    fn check(&self, expected: &str) -> Result<()> {
        if self.transport_path.as_deref().is_some_and(|p| {
            p.trim_start_matches('/')
                .strip_prefix("api/")
                .unwrap_or(p.trim_start_matches('/'))
                != expected
        }) {
            return Err(bad("Transport path mismatch"));
        }
        Ok(())
    }
}
async fn entries(State(s): S, Path(c): Path<String>, Query(p): Query<Page>) -> R {
    p.check(&format!("company-books/{c}/entries"))?;
    Ok(Json(
        s.entries_page(&c, p.offset, p.limit, p.max_bytes, None)
            .await?,
    ))
}
async fn register(State(s): S, Path((c, a)): Path<(String, String)>, Query(p): Query<Page>) -> R {
    p.check(&format!("company-books/{c}/register/{a}"))?;
    Ok(Json(
        s.entries_page(&c, p.offset, p.limit, p.max_bytes, Some(&a))
            .await?,
    ))
}
async fn tb(State(s): S, Path(c): Path<String>, Query(p): Query<Page>) -> R {
    p.check(&format!("company-books/{c}/trial-balance"))?;
    Ok(Json(
        s.trial_balance_page(&c, p.offset, p.limit, p.max_bytes)
            .await?,
    ))
}
async fn journal(State(s): S, Path(c): Path<String>, Json(v): Json<JournalInput>) -> R {
    Ok(Json(s.journal(&c, v).await?))
}
async fn reverse(
    State(s): S,
    Path((c, e)): Path<(String, String)>,
    Json(v): Json<ReverseInput>,
) -> R {
    Ok(Json(s.reverse(&c, &e, v).await?))
}
async fn lock_period(State(s): S, Path(c): Path<String>, Json(v): Json<LockInput>) -> R {
    Ok(Json(s.lock_period(&c, v).await?))
}
async fn add_class(State(s): S, Path(c): Path<String>, Json(v): Json<super::subledger::ClassInput>) -> R {
    Ok(Json(s.add_class(&c, v).await?))
}
async fn open_document(State(s): S, Path(c): Path<String>, Json(v): Json<super::subledger::DocumentInput>) -> R {
    Ok(Json(s.open_document(&c, v).await?))
}
async fn apply_payment(State(s): S, Path(c): Path<String>, Json(v): Json<super::subledger::PaymentInput>) -> R {
    Ok(Json(s.apply_payment(&c, v).await?))
}
async fn subledger(State(s): S, Path(c): Path<String>) -> R { Ok(Json(s.subledger(&c).await?)) }
async fn sparse_native(State(s): S, Path((c, e)): Path<(String, String)>, Json(v): Json<super::subledger::SparseInput>) -> R {
    Ok(Json(s.sparse_native(&c, &e, v).await?))
}
async fn delete_native(State(s): S, Path((c, e)): Path<(String, String)>, Json(v): Json<super::subledger::DeleteInput>) -> R {
    Ok(Json(s.delete_native(&c, &e, v).await?))
}
async fn attach(State(s): S, Path(c): Path<String>, Json(v): Json<super::subledger::AttachmentInput>) -> R {
    Ok(Json(s.attach(&c, v).await?))
}
async fn sales_tax(State(s): S, Path(c): Path<String>, Json(v): Json<super::subledger::SalesTaxInput>) -> R {
    Ok(Json(s.set_sales_tax(&c, v).await?))
}
async fn add_department(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::NamedInput>) -> R { Ok(Json(s.add_department(&c, v).await?)) }
async fn add_item(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::NamedInput>) -> R { Ok(Json(s.add_item(&c, v).await?)) }
async fn add_employee(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::NamedInput>) -> R { Ok(Json(s.add_employee(&c, v).await?)) }
async fn add_estimate(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::NonPostingInput>) -> R { Ok(Json(s.add_estimate(&c, v).await?)) }
async fn add_purchase_order(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::NonPostingInput>) -> R { Ok(Json(s.add_purchase_order(&c, v).await?)) }
async fn add_time(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::TimeInput>) -> R { Ok(Json(s.add_time(&c, v).await?)) }
async fn add_budget(State(s): S, Path(c): Path<String>, Json(v): Json<super::daily::BudgetInput>) -> R { Ok(Json(s.add_budget(&c, v).await?)) }
async fn payroll_run(State(s): S, Path(c): Path<String>) -> R { Ok(Json(s.payroll_run(&c).await?)) }
async fn daily(State(s): S, Path(c): Path<String>) -> R { Ok(Json(s.daily(&c).await?)) }
