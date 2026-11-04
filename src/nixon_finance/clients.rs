//! Per-client view: one read-only picture of a client across the journey,
//! that client's imported company books and its own ledger. Nixon reports what it finds here; it does not
//! post, approve, or move the journey. Callers filter by staff assignment in
//! the routes, so nothing here sees another client's data on their behalf.

use super::voice::Scope;
use crate::ledger::{engine::{Ledger, LedgerError, parse_date, today}, models::DocKind, routes::{company_book_open, opened_company_books}};
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use serde_json::{Value, json};
use crate::client_journey::service::Journeys;
use crate::company_books::{CompanyBooks, valid_id};
use crate::store::Store;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_BOOKS: usize = 500;
const RECENT_ENTRIES: usize = 25;

fn finding(severity: &str, title: String, source: &str) -> Value {
    json!({"severity": severity, "title": title, "source": source})
}

/// Every committed or staged company book id: one index read plus one pointer MGET.
async fn book_stages(books: &CompanyBooks) -> Result<BTreeMap<String, String>, ApiError> {
    let page = books.list_summaries(0, MAX_BOOKS).await?;
    Ok(page["companies"].as_array().into_iter().flatten()
        .filter_map(|c| Some((c["company_id"].as_str()?.to_string(), c["stage"].as_str()?.to_string())))
        .collect())
}

/// Every client the firm has a journey, imported books or a ledger for.
pub async fn client_ids(scope: &Scope) -> Result<BTreeSet<String>, ApiError> {
    Ok(base_rows(&scope.company_books, &scope.journeys, &scope.ledger.store).await?.keys().cloned().collect())
}

/// One row per known client, built from three batched reads (journeys, book
/// pointers, opened ledgers) instead of several reads per client.
async fn base_rows(books: &CompanyBooks, journeys: &Journeys, ledger_store: &Store) -> Result<BTreeMap<String, Value>, ApiError> {
    let journeys = journeys.list().await?;
    let stages = book_stages(books).await?;
    let opened: BTreeSet<String> = opened_company_books(ledger_store).await.map_err(storage)?.into_iter().collect();
    let by_client: BTreeMap<String, &Value> = journeys["journeys"].as_array().into_iter().flatten()
        .filter_map(|j| Some((j["client_id"].as_str()?.to_string(), j)))
        .collect();
    let mut ids: BTreeSet<String> = by_client.keys().cloned().collect();
    ids.extend(stages.keys().cloned());
    ids.extend(opened.iter().cloned());
    Ok(ids.into_iter().map(|id| {
        let j = by_client.get(&id).copied();
        let row = row(&id, j, stages.get(&id).map(String::as_str), opened.contains(&id));
        (id, row)
    }).collect())
}

fn row(id: &str, j: Option<&Value>, stage: Option<&str>, ledger_open: bool) -> Value {
    // Same shape the per-client detail() call produced: an untouched valid id reads as acquisition_pending.
    let books_stage = stage.map(String::from).or_else(|| valid_id(id).then(|| "acquisition_pending".to_string()));
    json!({
        "client_id": id,
        "books_stage": books_stage,
        "ledger_open": ledger_open,
        "current_step": j.map(|j| &j["progress"]["current_step"]),
        "percent": j.map(|j| &j["progress"]["percent"]),
        "waiting_on_person": j.is_some_and(|j| !j["pending_review"].is_null()),
    })
}

const CACHE_TTL: Duration = Duration::from_secs(45);
type Cached = (Instant, Arc<BTreeMap<String, Value>>);
fn cache() -> &'static Mutex<HashMap<String, Cached>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Base rows, cached per tenant (remote store) for a short TTL. Process-memory
/// stores (local/test copies) are never cached so writes show immediately.
async fn cached_rows(scope: &Scope) -> Result<Arc<BTreeMap<String, Value>>, ApiError> {
    let key = scope.company_books.store.tenant_key();
    if let Some(k) = &key {
        if let Some((at, rows)) = cache().lock().unwrap().get(k) {
            if at.elapsed() < CACHE_TTL {
                return Ok(rows.clone());
            }
        }
    }
    let rows = Arc::new(base_rows(&scope.company_books, &scope.journeys, &scope.ledger.store).await?);
    if let Some(k) = key {
        let mut c = cache().lock().unwrap();
        c.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        c.insert(k, (Instant::now(), rows.clone()));
    }
    Ok(rows)
}

async fn journey(scope: &Scope, client: &str) -> Result<Option<Value>, ApiError> {
    match scope.journeys.get(client).await {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.0 == StatusCode::NOT_FOUND || e.0 == StatusCode::BAD_REQUEST => Ok(None),
        Err(e) => Err(e),
    }
}

async fn books(scope: &Scope, client: &str) -> Result<Option<Value>, ApiError> {
    let detail = match scope.company_books.detail(client).await {
        Ok(v) => v,
        Err(e) if e.0 == StatusCode::NOT_FOUND || e.0 == StatusCode::BAD_REQUEST => return Ok(None),
        Err(e) => return Err(e),
    };
    // The books service answers for any id, so an untouched id means no books.
    if detail["stage"] == "acquisition_pending" {
        return Ok(None);
    }
    let committed = detail["stage"] == "import_committed";
    let trial = if committed { Some(scope.company_books.trial_balance(client).await?) } else { None };
    Ok(Some(json!({"stage": detail["stage"], "previews": detail["previews"], "trial_balance": trial})))
}

fn storage(e: String) -> ApiError {
    LedgerError::Storage(e).into()
}

/// The client's own ledger, as of today. Reading it posts nothing.
async fn ledger(scope: &Scope, client: &str) -> Result<Option<Value>, ApiError> {
    if !company_book_open(&scope.ledger.store, client).await.map_err(storage)? {
        return Ok(None);
    }
    let book = Ledger::for_company(scope.ledger.store.clone(), client);
    let company = book.company().await?;
    let as_of = today();
    let from = Ledger::fiscal_year_start(&company, parse_date(&as_of)?).to_string();
    let mut entries = book.entries().await?;
    entries.reverse();
    entries.truncate(RECENT_ENTRIES);
    Ok(Some(json!({
        "name": company.name,
        "closing_date": company.closing_date,
        "as_of": as_of,
        "trial_balance": book.trial_balance(&as_of).await?,
        "profit_loss": book.profit_loss(&from, &as_of).await?,
        "balance_sheet": book.balance_sheet(&as_of).await?,
        "ar_aging": book.aging(DocKind::Invoice, &as_of).await?,
        "ap_aging": book.aging(DocKind::Bill, &as_of).await?,
        "recent_entries": entries,
    })))
}

fn cents(v: &Value) -> Option<i128> {
    v.as_str()?.parse().ok()
}

fn findings(journey: Option<&Value>, books: Option<&Value>) -> Vec<Value> {
    let mut out = Vec::new();
    match journey {
        None => out.push(finding("info", "No client journey has been started.".into(), "journey")),
        Some(j) => {
            if let Some(step) = j["pending_review"]["step"].as_str() {
                out.push(finding("attention", format!("{step} is waiting for a person."), "journey"));
            }
            for r in j["last_assessment"]["readiness"]["reasons"].as_array().into_iter().flatten().filter_map(|r| r.as_str()) {
                out.push(finding("attention", r.to_string(), "journey"));
            }
        }
    }
    let Some(b) = books else {
        out.push(finding("attention", "No company books exist for this client.".into(), "books"));
        return out;
    };
    if b["stage"] != "import_committed" {
        out.push(finding("attention", format!("Books are not committed yet (stage: {}).", b["stage"].as_str().unwrap_or("unknown")), "books"));
        return out;
    }
    let combined = &b["trial_balance"]["analysis"];
    match (cents(&combined["total_debit_cents"]), cents(&combined["total_credit_cents"])) {
        (Some(d), Some(c)) if d == c => {}
        (Some(d), Some(c)) => out.push(finding("critical", format!("Trial balance is out by {} cents.", (d - c).abs()), "books")),
        _ => out.push(finding("attention", "Trial balance totals could not be read.".into(), "books")),
    }
    if b["trial_balance"]["head"]["review"]["status"] == "needs_review" {
        out.push(finding("attention", "Books were committed as needs_review.".into(), "books"));
    }
    out
}

/// One row per client; `visible` is the caller's assignment check. `assigned` clients
/// are listed even before any record exists for them.
pub async fn list(scope: &Scope, assigned: &[String], visible: impl Fn(&str) -> bool) -> Result<Value, ApiError> {
    let base = cached_rows(scope).await?;
    let mut rows: Vec<Value> = base.iter().filter(|(id, _)| visible(id)).map(|(_, r)| r.clone()).collect();
    for client in assigned {
        if !base.keys().any(|c| c.eq_ignore_ascii_case(client)) && visible(client) {
            rows.push(row(client, None, None, false));
        }
    }
    rows.sort_by(|a, b| a["client_id"].as_str().cmp(&b["client_id"].as_str()));
    rows.dedup_by(|a, b| a["client_id"] == b["client_id"]);
    Ok(json!({"clients": rows}))
}

pub async fn view(scope: &Scope, client: &str, assigned: bool) -> Result<Value, ApiError> {
    let j = journey(scope, client).await?;
    let b = books(scope, client).await?;
    let l = ledger(scope, client).await?;
    if j.is_none() && b.is_none() && l.is_none() && !assigned {
        return Err(ApiError(StatusCode::NOT_FOUND, "no journey, books or ledger for this client".into()));
    }
    let mut found = findings(j.as_ref(), b.as_ref());
    match &l {
        None => found.push(finding("info", "No client ledger has been opened.".into(), "ledger")),
        Some(l) if l["trial_balance"]["balanced"] != true => found.push(finding("critical", "The client ledger trial balance is out of balance.".into(), "ledger")),
        Some(_) => {}
    }
    Ok(json!({
        "client_id": client,
        "read_only": true,
        "journey": j.as_ref().map(|j| json!({"progress": j["progress"], "pending_review": j["pending_review"], "last_assessment": j["last_assessment"], "sources": j["sources"]})),
        "books": b,
        "ledger": l,
        "findings": found,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(debit: &str, credit: &str) -> Value {
        json!({"stage": "import_committed", "trial_balance": {"analysis": {"total_debit_cents": debit, "total_credit_cents": credit}}})
    }

    async fn seeded(n: usize) -> (Store, CompanyBooks, Journeys) {
        let store = Store::memory();
        let books = CompanyBooks::new(store.clone());
        let journeys = Journeys::new(store.clone(), crate::client_journey::capability::Capabilities::new());
        for i in 0..n {
            let id = format!("example-co-{i:03}");
            journeys.create(&id, Default::default()).await.ok().unwrap();
            store.sadd("company-books:companies:v1", &id).await.unwrap();
            store.set(&format!("company-books:{id}:latest-preview:v1"), &"a".repeat(64)).await.unwrap();
        }
        (store, books, journeys)
    }

    #[tokio::test]
    async fn listing_clients_costs_constant_store_round_trips() {
        let mut counts = vec![];
        for n in [5, 80] {
            let (store, books, journeys) = seeded(n).await;
            store.take_test_reads();
            let rows = base_rows(&books, &journeys, &store).await.ok().unwrap();
            let reads = store.take_test_reads();
            assert_eq!(rows.len(), n);
            assert!(reads.iter().flatten().all(|k| !k.contains(":preview:")), "list must not read preview blobs");
            assert_eq!(rows["example-co-000"]["books_stage"], "staged");
            assert_eq!(rows["example-co-000"]["waiting_on_person"], false);
            counts.push(reads.len());
        }
        assert_eq!(counts[0], counts[1], "round trips must not grow with client count: {counts:?}");
        assert!(counts[0] <= 5, "{counts:?}");
    }

    #[test]
    fn an_out_of_balance_book_is_critical() {
        let found = findings(Some(&json!({})), Some(&book("1000", "900")));
        assert_eq!(found[0]["severity"], "critical");
        assert!(found[0]["title"].as_str().unwrap().contains("100 cents"));
        assert!(findings(Some(&json!({})), Some(&book("1000", "1000"))).is_empty());
    }

    #[test]
    fn missing_books_and_waiting_steps_are_reported() {
        let j = json!({"pending_review": {"step": "books_access"}, "last_assessment": {"readiness": {"reasons": ["declare the client's current software first"]}}});
        let found = findings(Some(&j), None);
        let titles: Vec<&str> = found.iter().map(|f| f["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["books_access is waiting for a person.", "declare the client's current software first", "No company books exist for this client."]);
    }
}
