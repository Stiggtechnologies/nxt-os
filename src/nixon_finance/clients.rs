//! Per-client view: one read-only picture of a client across the journey and
//! that client's company books. Nixon reports what it finds here; it does not
//! post, approve, or move the journey. Callers filter by staff assignment in
//! the routes, so nothing here sees another client's data on their behalf.

use super::voice::Scope;
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const BOOK_PAGE: usize = 50;
const MAX_BOOKS: usize = 500;

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

/// Every client the firm has a journey or a book for.
pub async fn client_ids(scope: &Scope) -> Result<BTreeSet<String>, ApiError> {
    let journeys = scope.journeys.list().await?;
    let mut ids: BTreeSet<String> = journeys["journeys"].as_array().into_iter().flatten()
        .filter_map(|j| j["client_id"].as_str().map(String::from))
        .collect();
    ids.extend(book_ids(scope).await?);
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

/// One row per client; `visible` is the caller's assignment check.
pub async fn list(scope: &Scope, visible: impl Fn(&str) -> bool) -> Result<Value, ApiError> {
    let mut rows = Vec::new();
    for id in client_ids(scope).await?.iter().filter(|c| visible(c)) {
        let j = journey(scope, id).await?;
        let b = scope.company_books.detail(id).await.ok();
        rows.push(json!({
            "client_id": id,
            "books_stage": b.as_ref().map(|b| &b["stage"]),
            "current_step": j.as_ref().map(|j| &j["progress"]["current_step"]),
            "percent": j.as_ref().map(|j| &j["progress"]["percent"]),
            "waiting_on_person": j.as_ref().is_some_and(|j| !j["pending_review"].is_null()),
        }));
    }
    Ok(json!({"clients": rows}))
}

pub async fn view(scope: &Scope, client: &str) -> Result<Value, ApiError> {
    let j = journey(scope, client).await?;
    let b = books(scope, client).await?;
    if j.is_none() && b.is_none() {
        return Err(ApiError(StatusCode::NOT_FOUND, "no journey or books for this client".into()));
    }
    let found = findings(j.as_ref(), b.as_ref());
    Ok(json!({
        "client_id": client,
        "read_only": true,
        "journey": j.as_ref().map(|j| json!({"progress": j["progress"], "pending_review": j["pending_review"], "last_assessment": j["last_assessment"], "sources": j["sources"]})),
        "books": b,
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
