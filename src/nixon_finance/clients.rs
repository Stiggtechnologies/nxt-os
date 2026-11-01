//! Per-client view: one read-only picture of a client across the journey,
//! that client's imported company books and its own ledger. Nixon reports what it finds here; it does not
//! post, approve, or move the journey. Callers filter by staff assignment in
//! the routes, so nothing here sees another client's data on their behalf.

use super::voice::Scope;
use crate::ledger::{engine::{Ledger, LedgerError, parse_date, today}, models::DocKind, routes::{company_book_open, opened_company_books}};
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const BOOK_PAGE: usize = 50;
const MAX_BOOKS: usize = 500;
const RECENT_ENTRIES: usize = 25;

fn finding(severity: &str, title: String, source: &str) -> Value {
    json!({"severity": severity, "title": title, "source": source})
}

async fn book_ids(scope: &Scope) -> Result<Vec<String>, ApiError> {
    let mut ids = Vec::new();
    while ids.len() < MAX_BOOKS {
        let page = scope.company_books.list_page(ids.len(), BOOK_PAGE).await?;
        let rows = page["companies"].as_array().cloned().unwrap_or_default();
        ids.extend(rows.iter().filter_map(|c| c["company_id"].as_str().map(String::from)));
        if rows.is_empty() || page["has_more"] != true {
            break;
        }
    }
    Ok(ids)
}

/// Every client the firm has a journey, imported books or a ledger for.
pub async fn client_ids(scope: &Scope) -> Result<BTreeSet<String>, ApiError> {
    let journeys = scope.journeys.list().await?;
    let mut ids: BTreeSet<String> = journeys["journeys"].as_array().into_iter().flatten()
        .filter_map(|j| j["client_id"].as_str().map(String::from))
        .collect();
    ids.extend(book_ids(scope).await?);
    ids.extend(opened_company_books(&scope.ledger.store).await.map_err(storage)?);
    Ok(ids)
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
    let mut ids = client_ids(scope).await?;
    for client in assigned {
        if !ids.iter().any(|c| c.eq_ignore_ascii_case(client)) {
            ids.insert(client.clone());
        }
    }
    let mut rows = Vec::new();
    for id in ids.iter().filter(|c| visible(c)) {
        let j = journey(scope, id).await?;
        let b = scope.company_books.detail(id).await.ok();
        rows.push(json!({
            "client_id": id,
            "books_stage": b.as_ref().map(|b| &b["stage"]),
            "ledger_open": company_book_open(&scope.ledger.store, id).await.map_err(storage)?,
            "current_step": j.as_ref().map(|j| &j["progress"]["current_step"]),
            "percent": j.as_ref().map(|j| &j["progress"]["percent"]),
            "waiting_on_person": j.as_ref().is_some_and(|j| !j["pending_review"].is_null()),
        }));
    }
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
