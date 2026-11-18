use super::engine::*;
use super::models::*;
use super::opening;
use crate::firm::staff::Identity;
use crate::referral::routes::{ApiError, require_admin};
use crate::{company_books::CompanyBooks, store::Store};
use axum::{
    Json, Router,
    extract::{Path, Query, Request, State},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

type S = State<Arc<Ledger>>;
type R = Result<Json<Value>, ApiError>;

impl From<LedgerError> for ApiError {
    fn from(e: LedgerError) -> Self {
        match e {
            LedgerError::BadRequest(m) => ApiError(StatusCode::BAD_REQUEST, m),
            LedgerError::NotFound(m) => ApiError(StatusCode::NOT_FOUND, m),
            LedgerError::Conflict(m) => ApiError(StatusCode::CONFLICT, m),
            LedgerError::Storage(m) => ApiError(StatusCode::INTERNAL_SERVER_ERROR, m),
        }
    }
}

fn ok<T: serde::Serialize>(r: LResult<T>) -> R {
    r.map(|v| Json(json!(v))).map_err(ApiError::from)
}

#[derive(Deserialize, Default)]
pub struct Range {
    from: Option<String>,
    to: Option<String>,
    as_of: Option<String>,
    account_id: Option<String>,
    kind: Option<String>,
}

impl Range {
    fn to(&self) -> String {
        self.to.clone().or(self.as_of.clone()).unwrap_or_else(today)
    }
    fn from(&self) -> String {
        self.from.clone().unwrap_or_else(|| format!("{}-01-01", &today()[..4]))
    }
}

pub fn router(st: Arc<Ledger>) -> Router {
    // The firm's own book. Staff limited to specific clients use /api/company-ledger.
    routes(st).layer(middleware::from_fn(crate::firm::staff::firm_wide_only)).layer(middleware::from_fn(require_admin))
}

/// Each client's own book, served by the same handlers at
/// `/api/company-ledger/{company}/...`. `POST .../open` creates a book; every
/// other route answers 404 until then, so a mistyped id cannot start one.
pub fn company_router(store: Store, books: Arc<CompanyBooks>) -> Router {
    Router::new()
        .route("/api/company-ledger/{company}/{*rest}", any(company_book))
        .layer(middleware::from_fn(client_scope))
        .layer(middleware::from_fn(require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            r
        }))
        .with_state(Arc::new(CompanyState { store, books }))
}

struct CompanyState {
    store: Store,
    books: Arc<CompanyBooks>,
}

async fn client_scope(req: Request, next: middleware::Next) -> Response {
    crate::firm::staff::scope_guard(req, next, COMPANY_BASE, &[], |c| async move { Some(c) }).await
}

const COMPANY_BASE: &str = "/api/company-ledger";

const OPENED: &str = "company-ledger:index";

fn opened_key(company: &str) -> String {
    format!("company-ledger:{company}:opened")
}

/// Whether this client has a ledger book of its own.
pub async fn company_book_open(store: &Store, company: &str) -> Result<bool, String> {
    if !crate::company_books::valid_id(company) {
        return Ok(false);
    }
    Ok(store.get(&opened_key(company)).await?.is_some())
}

/// Every client with an open ledger book.
pub async fn opened_company_books(store: &Store) -> Result<Vec<String>, String> {
    store.smembers(OPENED).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenBook {
    name: String,
    #[serde(default)]
    province: Option<String>,
}

async fn json_body<T: serde::de::DeserializeOwned>(req: Request) -> Result<T, ApiError> {
    let bytes = axum::body::to_bytes(req.into_body(), 2 << 20)
        .await
        .map_err(|_| ApiError(StatusCode::PAYLOAD_TOO_LARGE, "request body too large".into()))?;
    serde_json::from_slice(&bytes).map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid request: {e}")))
}

async fn open_book(store: &Store, company: &str, req: Request) -> R {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|_| ApiError(StatusCode::PAYLOAD_TOO_LARGE, "request body too large".into()))?;
    let input: OpenBook = serde_json::from_slice(&bytes).map_err(|_| ApiError(StatusCode::BAD_REQUEST, "a book needs a company name".into()))?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 300 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "a book needs a company name".into()));
    }
    let storage = |e: String| ApiError::from(LedgerError::Storage(e));
    let ledger = Ledger::for_company(store.clone(), company);
    // The first request settles the book's identity; a retry changes nothing.
    // Imported company-books and native company-ledger can coexist: Parallel testing reads the
    // import via migrations GL, while opening-balances may still seed a native book from it.
    // Empty-state UX is handled in ledger.js (probe company-books on 404 and open the import GL).
    if store.get(&opened_key(company)).await.map_err(storage)?.is_some() {
        store.sadd(OPENED, company).await.map_err(storage)?;
        return Ok(Json(json!({"company_id": company, "company": ledger.company().await?, "already_open": true})));
    }
    let defaults = Company::default();
    let book = Company { name: name.into(), province: input.province.unwrap_or(defaults.province.clone()), ..defaults };
    ledger.save_company(&book).await?;
    ledger.audit("book.opened", name.into()).await?;
    store.set_nx(&opened_key(company), "1").await.map_err(storage)?;
    store.sadd(OPENED, company).await.map_err(storage)?;
    Ok(Json(json!({"company_id": company, "company": book, "already_open": false})))
}

async fn company_book(State(state): State<Arc<CompanyState>>, mut req: Request) -> Response {
    let store = &state.store;
    // Read the raw path: this is the same text the client scope guard checked.
    let path = req.uri().path().to_string();
    let Some((company, rest)) = path.strip_prefix(COMPANY_BASE).and_then(|p| p.strip_prefix('/')).and_then(|p| p.split_once('/')) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !crate::company_books::valid_id(company) {
        return ApiError(StatusCode::BAD_REQUEST, "Invalid company id".into()).into_response();
    }
    if rest == "open" && req.method() == axum::http::Method::POST {
        return open_book(store, company, req).await.into_response();
    }
    match store.get(&opened_key(company)).await {
        Ok(Some(_)) => {}
        Ok(None) => return ApiError(StatusCode::NOT_FOUND, "this company has no ledger yet".into()).into_response(),
        Err(e) => return ApiError::from(LedgerError::Storage(e)).into_response(),
    }
    let post = req.method() == axum::http::Method::POST;
    if post && rest == "opening-balances/preview" {
        let book = Ledger::for_company(store.clone(), company);
        let run = async { Ok::<_, ApiError>(Json(opening::preview(&state.books, &book, company, json_body(req).await?).await?)) };
        return run.await.into_response();
    }
    if post && rest == "opening-balances" {
        let book = Ledger::for_company(store.clone(), company);
        let run = async { Ok::<_, ApiError>(Json(opening::post(&state.books, &book, company, json_body(req).await?).await?)) };
        return run.await.into_response();
    }
    let target = match req.uri().query() {
        Some(q) => format!("/api/ledger/{rest}?{q}"),
        None => format!("/api/ledger/{rest}"),
    };
    let Ok(uri) = target.parse::<Uri>() else { return StatusCode::BAD_REQUEST.into_response() };
    *req.uri_mut() = uri;
    // Keep only the verified identity: the outer route's path parameters would otherwise reach the inner handlers.
    let identity = req.extensions().get::<Identity>().cloned();
    *req.extensions_mut() = Default::default();
    if let Some(identity) = identity {
        req.extensions_mut().insert(identity);
    }
    let ledger = Arc::new(Ledger::for_company(store.clone(), company));
    match routes(ledger).oneshot(req).await {
        Ok(response) => response,
        Err(never) => match never {},
    }
}

fn routes(st: Arc<Ledger>) -> Router {
    Router::new()
        .route("/api/ledger/meta", get(meta))
        .route("/api/ledger/company", get(company).put(save_company))
        .route("/api/ledger/accounts", get(accounts).post(create_account))
        .route("/api/ledger/accounts/{id}", axum::routing::put(update_account))
        .route("/api/ledger/accounts/{id}/register", get(register))
        .route("/api/ledger/contacts", get(contacts).post(create_contact))
        .route("/api/ledger/contacts/{id}", get(contact).put(update_contact))
        .route("/api/ledger/entries", get(entries).post(create_journal))
        .route("/api/ledger/entries/{id}/void", post(void_entry))
        .route("/api/ledger/invoices", get(invoices).post(create_invoice))
        .route("/api/ledger/bills", get(bills).post(create_bill))
        .route("/api/ledger/docs/{id}", get(doc))
        .route("/api/ledger/docs/{id}/void", post(void_doc))
        .route("/api/ledger/payments", get(payments))
        .route("/api/ledger/payments/received", post(pay_received))
        .route("/api/ledger/payments/made", post(pay_made))
        .route("/api/ledger/payments/{id}/void", post(void_payment))
        .route("/api/ledger/money/{direction}", post(money))
        .route("/api/ledger/transfers", post(transfer))
        .route("/api/ledger/tax-payments", post(tax_payment))
        .route("/api/ledger/feed", get(feed))
        .route("/api/ledger/feed/import", post(import_feed))
        .route("/api/ledger/feed/{id}/categorize", post(categorize))
        .route("/api/ledger/feed/{id}/exclude", post(exclude))
        .route("/api/ledger/reconcile", get(reconciliations).post(reconcile))
        .route("/api/ledger/reports/{name}", get(report))
        .route("/api/ledger/audit", get(audit))
        .with_state(st)
}

async fn meta() -> R {
    Ok(Json(json!({ "tax_codes": tax_codes(), "today": today() })))
}
async fn company(State(l): S) -> R {
    ok(l.company().await)
}
async fn save_company(State(l): S, Json(c): Json<Company>) -> R {
    ok(l.save_company(&c).await.map(|_| c))
}
async fn accounts(State(l): S) -> R {
    ok(l.accounts().await)
}
async fn create_account(State(l): S, Json(b): Json<AccountInput>) -> R {
    ok(l.save_account(None, b).await)
}
async fn update_account(State(l): S, Path(id): Path<String>, Json(b): Json<AccountInput>) -> R {
    ok(l.save_account(Some(&id), b).await)
}
async fn register(State(l): S, Path(id): Path<String>) -> R {
    ok(l.register(&id).await)
}
async fn contacts(State(l): S, Query(q): Query<Range>) -> R {
    let kind = match q.kind.as_deref() {
        Some("customer") => Some(ContactKind::Customer),
        Some("vendor") => Some(ContactKind::Vendor),
        _ => None,
    };
    ok(l.contacts(kind).await)
}
async fn contact(State(l): S, Path(id): Path<String>) -> R {
    let c = l.contact(&id).await?;
    let t = today();
    let docs: Vec<Value> = l
        .docs(None)
        .await?
        .into_iter()
        .filter(|d| d.contact_id == id)
        .map(|d| json!({ "status": d.status(&t), "balance": d.balance(), "doc": d }))
        .collect();
    let open: i64 = docs.iter().map(|d| d["balance"].as_i64().unwrap_or(0)).sum();
    Ok(Json(json!({ "contact": c, "documents": docs, "open_balance": open })))
}
async fn create_contact(State(l): S, Json(b): Json<ContactInput>) -> R {
    ok(l.save_contact(None, b).await)
}
async fn update_contact(State(l): S, Path(id): Path<String>, Json(b): Json<ContactInput>) -> R {
    ok(l.save_contact(Some(&id), b).await)
}
async fn entries(State(l): S) -> R {
    let mut e = l.entries().await?;
    e.reverse();
    ok(Ok(e))
}
async fn create_journal(State(l): S, Json(b): Json<JournalInput>) -> R {
    ok(l.create_journal(b).await)
}
async fn void_entry(State(l): S, Path(id): Path<String>) -> R {
    ok(l.void_journal(&id).await)
}

async fn doc_list(l: &Ledger, kind: DocKind) -> R {
    let t = today();
    let names: std::collections::HashMap<String, String> = l.contacts(None).await?.into_iter().map(|c| (c.id, c.name)).collect();
    let v: Vec<Value> = l
        .docs(Some(kind))
        .await?
        .into_iter()
        .map(|d| json!({ "status": d.status(&t), "balance": d.balance(), "contact_name": names.get(&d.contact_id), "doc": d }))
        .collect();
    Ok(Json(json!(v)))
}
async fn invoices(State(l): S) -> R {
    doc_list(&l, DocKind::Invoice).await
}
async fn bills(State(l): S) -> R {
    doc_list(&l, DocKind::Bill).await
}
async fn create_invoice(State(l): S, Json(b): Json<DocInput>) -> R {
    ok(l.create_doc(DocKind::Invoice, b).await)
}
async fn create_bill(State(l): S, Json(b): Json<DocInput>) -> R {
    ok(l.create_doc(DocKind::Bill, b).await)
}
async fn doc(State(l): S, Path(id): Path<String>) -> R {
    let d = l.doc(&id).await?;
    let c = l.contact(&d.contact_id).await?;
    let company = l.company().await?;
    let status = d.status(&today());
    Ok(Json(json!({ "doc": d, "contact": c, "company": company, "status": status, "balance": d.balance() })))
}
async fn void_doc(State(l): S, Path(id): Path<String>) -> R {
    ok(l.void_doc(&id).await)
}
async fn payments(State(l): S) -> R {
    ok(l.payments().await)
}
async fn pay_received(State(l): S, Json(b): Json<PaymentInput>) -> R {
    ok(l.record_payment("received", b).await)
}
async fn pay_made(State(l): S, Json(b): Json<PaymentInput>) -> R {
    ok(l.record_payment("made", b).await)
}
async fn void_payment(State(l): S, Path(id): Path<String>) -> R {
    ok(l.void_payment(&id).await)
}
async fn money(State(l): S, Path(dir): Path<String>, Json(b): Json<MoneyInput>) -> R {
    ok(l.money(&dir, b).await)
}
async fn transfer(State(l): S, Json(b): Json<TransferInput>) -> R {
    ok(l.transfer(b).await)
}
async fn tax_payment(State(l): S, Json(b): Json<TaxPaymentInput>) -> R {
    ok(l.tax_payment(b).await)
}
async fn feed(State(l): S) -> R {
    ok(l.feed().await)
}
#[derive(Deserialize)]
struct ImportBody {
    account_id: String,
    csv: String,
}
async fn import_feed(State(l): S, Json(b): Json<ImportBody>) -> R {
    if b.csv.len() > 2_000_000 {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "CSV is larger than 2 MB".into()));
    }
    ok(l.import_feed(&b.account_id, &b.csv).await)
}
async fn categorize(State(l): S, Path(id): Path<String>, Json(b): Json<CategorizeInput>) -> R {
    ok(l.categorize(&id, b).await)
}
async fn exclude(State(l): S, Path(id): Path<String>) -> R {
    ok(l.exclude_feed(&id).await)
}
async fn reconciliations(State(l): S) -> R {
    ok(l.reconciliations().await)
}
async fn reconcile(State(l): S, Json(b): Json<ReconcileInput>) -> R {
    ok(l.reconcile(b).await)
}
async fn audit(State(l): S) -> R {
    ok(l.audit_log(500).await)
}

pub async fn run_report(l: &Ledger, name: &str, q: &Range) -> LResult<Value> {
    match name {
        "dashboard" => l.dashboard().await,
        "trial-balance" => l.trial_balance(&q.to()).await,
        "profit-loss" => l.profit_loss(&q.from(), &q.to()).await,
        "balance-sheet" => l.balance_sheet(&q.to()).await,
        "ar-aging" => l.aging(DocKind::Invoice, &q.to()).await,
        "ap-aging" => l.aging(DocKind::Bill, &q.to()).await,
        "sales-tax" => l.sales_tax(&q.from(), &q.to()).await,
        "cash-flow" => l.cash_flow(&q.from(), &q.to()).await,
        "profit-loss-by-month" => l.profit_loss_by_month(&q.from(), &q.to()).await,
        "profit-loss-comparison" => l.profit_loss_comparison(&q.from(), &q.to()).await,
        "customer-balances" => l.contact_balances(DocKind::Invoice, &q.to()).await,
        "vendor-balances" => l.contact_balances(DocKind::Bill, &q.to()).await,
        "general-ledger" => l.general_ledger(q.account_id.as_deref().filter(|s| !s.is_empty()), &q.from(), &q.to()).await,
        _ => Err(LedgerError::NotFound(format!("unknown report '{name}'"))),
    }
}

impl Range {
    pub fn new(from: Option<String>, to: Option<String>, account_id: Option<String>) -> Self {
        Range { from, to, as_of: None, account_id, kind: None }
    }
}

async fn report(State(l): S, Path(name): Path<String>, Query(q): Query<Range>) -> R {
    ok(run_report(&l, &name, &q).await)
}
