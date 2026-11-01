//! Carries a client's imported chart and balances into its own ledger. A reviewer
//! sees the plan first and posts it once, as one dated entry, bound to the exact
//! imported book that was reviewed. The imported book is only read, and QuickBooks
//! stays authoritative until a person records that client's cutover.

use super::{engine::*, models::*};
use crate::{company_books::{CompanyBooks, DEFAULT_PAGE_BYTES}, referral::routes::ApiError};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const PAGE: usize = 200;
const CLAIM: &str = "opening-claim";
const RECEIPT: &str = "opening-receipt";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    /// Imported account id to kind, for accounts the source did not classify.
    #[serde(default)]
    pub classifications: BTreeMap<String, AccountKind>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostInput {
    pub expected_head: String,
    pub reviewer: String,
    #[serde(default)]
    pub classifications: BTreeMap<String, AccountKind>,
}

struct Row {
    source_id: String,
    name: String,
    kind: Option<AccountKind>,
    /// Debit positive, credit negative.
    net: i64,
    target: Option<Account>,
    code: String,
}

struct Plan {
    head: String,
    date: String,
    currency: String,
    rows: Vec<Row>,
    blockers: Vec<String>,
}

fn conflict(message: String) -> ApiError {
    ApiError(StatusCode::CONFLICT, message)
}

/// Names compare without case or a trailing qualifier: "Accounts Receivable (A/R)".
fn plain(name: &str) -> String {
    name.split('(').next().unwrap_or(name).trim().to_ascii_lowercase()
}

fn kind_of(v: &Value) -> Option<AccountKind> {
    serde_json::from_value(v.clone()).ok()
}

fn cents(v: &Value) -> Result<i64, ApiError> {
    let bad = || ApiError(StatusCode::INTERNAL_SERVER_ERROR, "an imported balance could not be read".into());
    v.as_str().ok_or_else(bad)?.parse().map_err(|_| bad())
}

async fn plan(books: &CompanyBooks, ledger: &Ledger, company: &str, kinds: &BTreeMap<String, AccountKind>) -> Result<Plan, ApiError> {
    let head = books.detail(company).await?["head_sha256"].as_str().map(String::from)
        .ok_or_else(|| conflict("this company has no committed imported books".into()))?;
    let (mut balances, mut offset) = (Vec::new(), 0);
    let first = loop {
        let page = books.trial_balance_page(company, offset, PAGE, DEFAULT_PAGE_BYTES).await?;
        balances.extend(page["balances"].as_array().cloned().unwrap_or_default());
        match page["next_offset"].as_u64() {
            Some(next) => offset = next as usize,
            None => break page,
        }
    };
    let mut blockers = Vec::new();
    if !first["head"]["review"].is_null() {
        blockers.push("The imported books were committed as needs_review.".to_string());
    }
    if first["analysis"]["total_debit_cents"] != first["analysis"]["total_credit_cents"] {
        blockers.push("The imported trial balance does not balance.".into());
    }
    if !ledger.entries().await?.is_empty() {
        blockers.push("This ledger already has entries, so an opening balance would double count.".into());
    }
    let mut existing = ledger.accounts().await?;
    existing.retain(|a| !a.archived);
    let mut taken = HashSet::new();
    let mut rows = Vec::new();
    for (i, b) in balances.iter().enumerate() {
        let source_id = b["account_id"].as_str().unwrap_or_default().to_string();
        let name = b["name"].as_str().unwrap_or_default().to_string();
        let kind = kind_of(&b["kind"]).or_else(|| kinds.get(&source_id).copied());
        let net = cents(&b["debit_cents"])? - cents(&b["credit_cents"])?;
        let target = existing.iter().find(|a| Some(a.kind) == kind && plain(&a.name) == plain(&name) && !taken.contains(&a.id)).cloned();
        if let Some(a) = &target { taken.insert(a.id.clone()); }
        if kind.is_none() {
            blockers.push(format!("{name} needs an account type."));
        }
        rows.push(Row { source_id, name, kind, net, target, code: format!("Q{:05}", i + 1) });
    }
    if rows.iter().all(|r| r.net == 0) {
        blockers.push("Every imported balance is zero, so there is nothing to carry over.".into());
    }
    let date = first["source"]["period_end"].as_str().unwrap_or_default().to_string();
    let currency = first["company"]["currency"].as_str().unwrap_or("CAD").to_string();
    Ok(Plan { head, date, currency, rows, blockers })
}

fn view(p: &Plan) -> Value {
    let total: i64 = p.rows.iter().map(|r| r.net.max(0)).sum();
    json!({
        "expected_head": p.head, "date": p.date, "currency": p.currency, "total_debit": total, "blockers": p.blockers,
        "accounts": p.rows.iter().map(|r| json!({
            "source_id": r.source_id, "name": r.name, "kind": r.kind,
            "debit": r.net.max(0), "credit": (-r.net).max(0),
            "ledger_account": r.target.as_ref().map(|a| format!("{} · {}", a.code, a.name)), "code": r.code,
        })).collect::<Vec<_>>(),
    })
}

pub async fn preview(books: &CompanyBooks, ledger: &Ledger, company: &str, input: PreviewInput) -> Result<Value, ApiError> {
    if let Some(receipt) = ledger.named_get(RECEIPT).await? {
        return Ok(json!({"posted": true, "receipt": receipt}));
    }
    Ok(json!({"posted": false, "plan": view(&plan(books, ledger, company, &input.classifications).await?)}))
}

pub async fn post(books: &CompanyBooks, ledger: &Ledger, company: &str, input: PostInput) -> Result<Value, ApiError> {
    if let Some(receipt) = ledger.named_get(RECEIPT).await? {
        return Err(conflict(format!("opening balances were already posted: {receipt}")));
    }
    let reviewer = input.reviewer.trim();
    if reviewer.is_empty() || reviewer.len() > 200 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "a reviewer is required".into()));
    }
    let p = plan(books, ledger, company, &input.classifications).await?;
    if p.head != input.expected_head {
        return Err(conflict("the imported books changed after this review; review them again".into()));
    }
    if let Some(blocker) = p.blockers.first() {
        return Err(conflict(blocker.clone()));
    }
    // New accounts keep one id per imported account, so an interrupted run repeats safely.
    let mut lines = Vec::new();
    let mut ids = Vec::new();
    for r in &p.rows {
        let Some(kind) = r.kind else { return Err(conflict(format!("{} needs an account type.", r.name))) };
        let id = match &r.target {
            Some(a) => a.id.clone(),
            None => {
                let id = format!("acc_imp_{}", &hex::encode(Sha256::digest(r.source_id.as_bytes()))[..16]);
                ledger.import_account(&Account { id: id.clone(), code: r.code.clone(), name: r.name.clone(), kind, role: AccountRole::None, description: "Imported chart of accounts".into(), archived: false }).await?;
                id
            }
        };
        ids.push((id.clone(), r.net));
        if r.net != 0 {
            lines.push(Line { account_id: id, debit: r.net.max(0), credit: (-r.net).max(0), ..Default::default() });
        }
    }
    if !ledger.claim(CLAIM).await? {
        return Err(conflict("opening balances are already being posted for this client".into()));
    }
    let memo = format!("Opening balances from imported books {}", &p.head[..12]);
    let entry = ledger.post_opening(&p.date, &memo, lines, &p.head).await?;
    let mut settings = ledger.company().await?;
    settings.currency = p.currency.clone();
    settings.closing_date = Some(p.date.clone());
    ledger.save_company(&settings).await?;

    // Tie out against the books as they now read, account by account.
    let tb = ledger.trial_balance(&p.date).await?;
    let net = |id: &str| tb["rows"].as_array().into_iter().flatten().find(|r| r["account_id"] == id)
        .map_or(0, |r| r["debit"].as_i64().unwrap_or(0) - r["credit"].as_i64().unwrap_or(0));
    let different: Vec<&str> = ids.iter().zip(&p.rows).filter(|((id, want), _)| net(id) != *want).map(|(_, r)| r.name.as_str()).collect();
    let receipt = json!({
        "entry_id": entry.id, "entry_number": entry.number, "date": p.date, "imported_head": p.head, "reviewer": reviewer,
        "accounts": p.rows.len(), "new_accounts": p.rows.iter().filter(|r| r.target.is_none()).count(),
        "total_debit": tb["total_debit"], "total_credit": tb["total_credit"],
        "tied_out": different.is_empty() && tb["balanced"] == true, "accounts_not_tied": different,
        "source_authoritative": true,
    });
    ledger.named_set(RECEIPT, &receipt).await?;
    ledger.audit("opening.posted", format!("#{} {} by {reviewer}", entry.number, p.date)).await?;
    Ok(receipt)
}
