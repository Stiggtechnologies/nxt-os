use super::engine::*;
use super::models::*;
use crate::referral::routes::{ApiError, require_admin};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

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
        .layer(middleware::from_fn(require_admin))
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
