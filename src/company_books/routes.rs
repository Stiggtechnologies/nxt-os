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
