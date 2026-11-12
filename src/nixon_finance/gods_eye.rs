//! God's Eye: one internal, firm-wide live view over every company's books,
//! cash, filings and deadlines.
//!
//! This extends Nixon and does not replace it. The watchers read what NXT
//! already holds: the firm ledger and each client's company ledger (through
//! Nixon's own `load_context` + `analyze`), imported company books (trial
//! balance and general ledger, read-only), and tax returns (Filing Desk
//! deadline rule). A run is saved as a Nixon [`Review`] with `scope:
//! "gods_eye"`, so drafted fixes are ordinary Nixon proposals decided through
//! `POST /api/nixon/reviews/{id}/proposals/{pid}/decide` (approve or dismiss,
//! named person, preparer cannot approve). Approval records a person. It does
//! not post, file, pay, message or contact anyone. Execution stays refused.
//!
//! Guardrails:
//! - read-only on books. No ledger, company-book or QuickBooks write path is called;
//! - internal staff only. The routes sit behind the workspace key and
//!   `firm_wide_only`. There is no client-facing route, link or notification;
//! - watchers are deterministic and need no AI. Optional AI drafting goes
//!   through [`super::ai_gateway`] only, is off unless `GODS_EYE_AI=on`, and is
//!   always classed as client data (zero-retention gateway hosts only);
//! - CRA notices have no source in NXT today, so that watcher reports
//!   `no_source` instead of inventing one.

use super::ai_gateway::{self, Sensitivity};
use super::models::*;
use super::service::{self, NixonError};
use super::voice::Scope;
use crate::ledger::engine::Ledger;
use crate::ledger::models::{AccountRole, DocKind};
use chrono::{Datelike, Duration, NaiveDate};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const SCOPE: &str = "gods_eye";
pub const PREPARED_BY: &str = "God's Eye watcher";
const MAX_COMPANIES: usize = 200;
const GL_PAGE: usize = 1000;
const GL_MAX_PAGES: usize = 5;
const WINDOW_DAYS: i64 = 120;
/// Screen `large-entry-v1`: an entry at least this many times the window median and at least the floor.
const LARGE_ENTRY_MULTIPLE: i128 = 10;
const LARGE_ENTRY_FLOOR_CENTS: i128 = 1_000_000;

/// Every watcher, the NXT source it reads (verified in code), and its trigger.
pub fn watchers() -> Value {
    json!([
        {"id":"gst_remittance","title":"Missed GST/HST remittance","status":"active",
         "source":"Ledger accounts with role tax_payable/tax_recoverable, Ledger::sales_tax (lines 101-109) and tax_payment entries (src/ledger/reports.rs, engine.rs)",
         "trigger":"Last closed calendar quarter has positive net tax, its due date (one month after quarter end) has passed, and no tax_payment entry is dated after the quarter end. NXT does not store a filing frequency, so quarterly is assumed and shown as a gap."},
        {"id":"payroll_remittance","title":"Missed payroll source-deduction remittance","status":"active",
         "source":"Imported company books: payroll-named liability accounts (company_books::is_payroll_account) in the general ledger (CompanyBooks::general_ledger_range)",
         "trigger":"A payroll liability account accrued credits in a month and shows no debit (remittance) after that month by the 15th of the next month, measured at the book's period end."},
        {"id":"unusual_transactions","title":"Unusual transactions","status":"active",
         "source":"Nixon screens expense-shift-v1 and duplicate journals on ledgers (analyze.rs); screen large-entry-v1 on imported company books",
         "trigger":"Month-to-month shift, identical journals, or an imported entry at least 10x the window median and at least $10,000.00."},
        {"id":"books_integrity","title":"Unreconciled or unbalanced books","status":"active",
         "source":"Nixon analyze on each ledger (balanced journals, trial balance, reconciliations, pending feed); CompanyBooks::trial_balance controls and needs_review label",
         "trigger":"Unbalanced journals or trial balance, reconciliation differences, pending bank rows, failed import controls, or a needs_review import."},
        {"id":"filing_deadlines","title":"Upcoming T1/T2/GST deadlines","status":"active",
         "source":"Tax::list returns plus filing_desk::deadline_for (T1 April 30; T2 six months after period end); GST quarter due dates from the ledger",
         "trigger":"An unfiled T1/T2 within 60 days of its statutory deadline or past it, or a GST/HST quarter due within 30 days."},
        {"id":"ar_aging","title":"AR aging","status":"active",
         "source":"Ledger::aging(invoice) on the firm ledger and each company ledger (src/ledger/reports.rs)",
         "trigger":"Open invoice balance more than 60 days past due."},
        {"id":"cra_notices","title":"CRA notices","status":"no_source",
         "source":"None. NXT has no CRA notice inbox, connector or record type (only Notice of Assessment reminders inside tax calculations).",
         "trigger":"Not running. A notice source needs its own reviewed connector."}
    ])
}

#[derive(Debug, Deserialize, Default)]
pub struct ViewQuery {
    /// YYYY-MM-DD. Defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RunRequest {
    #[serde(default)]
    pub as_of: Option<String>,
}

struct Company {
    id: String,
    name: String,
    sources: Vec<&'static str>,
}

struct Hit {
    company: String,
    watcher: &'static str,
    finding: Finding,
    draft_fix: String,
    recommendation: String,
}

fn day(s: &str) -> Result<NaiveDate, NixonError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| NixonError::new(400, "as_of must be YYYY-MM-DD"))
}

fn err500<E: std::fmt::Display>(e: E) -> NixonError {
    NixonError::new(500, e.to_string())
}

pub fn dollars(cents: i128) -> String {
    let neg = cents < 0;
    let c = cents.unsigned_abs();
    let whole = (c / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${grouped}.{:02}", if neg { "-" } else { "" }, c % 100)
}

fn evidence(id: &str, reference: String, system: &str, date: Option<String>, note: String) -> Evidence {
    Evidence {
        id: id.into(),
        class: "measured".into(),
        level: "verified_measurement".into(),
        role: "supporting".into(),
        source_system: system.into(),
        reference,
        source_date: date,
        note,
    }
}

#[allow(clippy::too_many_arguments)]
fn hit(company: &str, watcher: &'static str, key: &str, risk: &str, title: String, summary: String, ev: Vec<Evidence>, gaps: Vec<Gap>, draft_fix: String, recommendation: String) -> Hit {
    Hit {
        company: company.into(),
        watcher,
        finding: Finding {
            id: format!("ge_{watcher}_{company}_{key}"),
            capability: watcher.into(),
            risk_level: risk.into(),
            title,
            summary,
            requires_human_review: true,
            autonomy_level: "advisory".into(),
            evidence: ev,
            hypotheses: vec![],
            gaps,
            proposal_id: None,
        },
        draft_fix,
        recommendation,
    }
}

fn quarter_bounds(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    let q0 = (d.month0() / 3) * 3 + 1;
    let start = NaiveDate::from_ymd_opt(d.year(), q0, 1).unwrap();
    let end = start.checked_add_months(chrono::Months::new(3)).unwrap() - Duration::days(1);
    (start, end)
}

fn month_end(d: NaiveDate) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
    first.checked_add_months(chrono::Months::new(1)).unwrap() - Duration::days(1)
}

// ---- ledgers (firm book and each client's company ledger) ---------------------

async fn ledger_watch(ledger: &Ledger, company: &str, as_of: NaiveDate, out: &mut Vec<Hit>) -> Result<String, NixonError> {
    let profile = ledger.company().await?;
    let accounts = ledger.accounts().await?;
    let entries = ledger.entries().await?;
    let as_of_s = as_of.to_string();

    // Books integrity, unreconciled and unusual: Nixon's own screens, reused as-is.
    if !entries.is_empty() {
        let mut ctx = service::load_context(ledger).await?;
        ctx.as_of = as_of_s.clone();
        let analysis = super::analyze::analyze(&ctx, None, &[]).map_err(|m| NixonError::new(400, m))?;
        for f in analysis.findings {
            let watcher: &'static str = match f.capability.as_str() {
                "variance" => "unusual_transactions",
                "reconciliation" | "anomaly" => "books_integrity",
                _ => continue,
            };
            let (draft, rec) = match watcher {
                "unusual_transactions" => (
                    "Pull the entries behind this screen, confirm each with source documents, and note the business reason on the client file.".to_string(),
                    "Review with the client's bookkeeper before month-end close. Nothing is reclassified automatically.".to_string(),
                ),
                _ => (
                    "Open the cited entries or reconciliation in the ledger and correct them in a staff-reviewed journal. Nixon does not draft plugs.".to_string(),
                    "Resolve before relying on these books for a filing or a client report.".to_string(),
                ),
            };
            let key = f.id.trim_start_matches("f_").to_string();
            let mut h = hit(company, watcher, &key, &f.risk_level, f.title.clone(), f.summary.clone(), f.evidence.clone(), f.gaps.clone(), draft, rec);
            h.finding.hypotheses = f.hypotheses;
            out.push(h);
        }
    }

    // GST/HST: last closed quarter.
    let tax_accounts: Vec<&str> = accounts.iter().filter(|a| matches!(a.role, AccountRole::TaxPayable | AccountRole::TaxRecoverable)).map(|a| a.id.as_str()).collect();
    if !tax_accounts.is_empty() && !entries.is_empty() {
        let (cur_start, _) = quarter_bounds(as_of);
        let q_end = cur_start - Duration::days(1);
        let (q_start, _) = quarter_bounds(q_end);
        let due = month_end(q_end.checked_add_months(chrono::Months::new(1)).unwrap());
        let st = ledger.sales_tax(&q_start.to_string(), &q_end.to_string()).await?;
        let net = st["line_109_net_tax"].as_i64().unwrap_or(0) as i128;
        let activity = st["line_105_tax_collected"].as_i64().unwrap_or(0) != 0 || st["line_108_itcs"].as_i64().unwrap_or(0) != 0;
        let paid = entries.iter().any(|e| !e.voided && e.source == "tax_payment" && e.date.as_str() > q_end.to_string().as_str() && e.date.as_str() <= as_of_s.as_str());
        let label = format!("{}-Q{}", q_end.year(), q_end.month0() / 3 + 1);
        let gap = Gap { id: "g_gst_frequency".into(), note: "NXT stores no GST/HST filing frequency for this company. The screen assumes quarterly; monthly or annual filers have different due dates.".into() };
        let ev = vec![evidence("e_gst_wp", format!("gst:{company}:{q_start}..{q_end}"), "nxt.ledger.sales_tax", Some(q_end.to_string()),
            format!("Working paper {q_start} to {q_end}: line 105 {} collected, line 108 {} ITCs, line 109 net {}.", dollars(st["line_105_tax_collected"].as_i64().unwrap_or(0) as i128), dollars(st["line_108_itcs"].as_i64().unwrap_or(0) as i128), dollars(net)))];
        if as_of > due && net > 0 && !paid {
            out.push(hit(company, "gst_remittance", &label, "high",
                format!("No GST/HST remittance recorded for {label}"),
                format!("{label} shows {} net GST/HST owing. The assumed due date {due} has passed and no tax payment is recorded after {q_end}.", dollars(net)),
                ev, vec![gap],
                format!("Staff: check the CRA business account for the {label} return and payment. If it was paid outside NXT, record the remittance with its payment date in a reviewed journal. If it was not filed, prepare the return from the working paper (net {}) for the client's sign-off. NXT does not file or pay.", dollars(net)),
                "Treat as urgent: late GST/HST remittances attract penalties and interest.".into()));
        } else if as_of <= due && activity && !paid {
            let days = (due - as_of).num_days();
            if days <= 30 {
                out.push(hit(company, "filing_deadlines", &format!("gst_{label}"), if days <= 14 { "medium" } else { "low" },
                    format!("GST/HST return for {label} due {due}"),
                    format!("{days} days left. Net per the working paper: {}.", dollars(net)),
                    ev, vec![gap],
                    format!("Staff: review the {label} working paper, confirm ITCs against receipts, and prepare the return for the client's sign-off before {due}."),
                    "Schedule preparation now. NXT does not file.".into()));
            }
        }
    }

    // AR aging.
    let aging = ledger.aging(DocKind::Invoice, &as_of_s).await?;
    let over60 = aging["totals"]["61_90"].as_i64().unwrap_or(0) as i128 + aging["totals"]["over_90"].as_i64().unwrap_or(0) as i128;
    if over60 > 0 {
        let over90 = aging["totals"]["over_90"].as_i64().unwrap_or(0) as i128;
        let docs: Vec<&Value> = aging["documents"].as_array().into_iter().flatten().filter(|d| d["days_overdue"].as_i64().unwrap_or(0) > 60).collect();
        let ev = docs.iter().take(5).enumerate().map(|(i, d)| evidence(&format!("e_ar_{i}"), format!("doc:{}", d["id"].as_str().unwrap_or("")), "nxt.ledger.aging", d["due_date"].as_str().map(String::from),
            format!("Invoice {} is {} days past due with {} open.", d["number"].as_str().unwrap_or(""), d["days_overdue"], dollars(d["balance"].as_i64().unwrap_or(0) as i128)))).collect();
        out.push(hit(company, "ar_aging", "over60", if over90 > 0 { "medium" } else { "low" },
            format!("{} receivable more than 60 days past due", dollars(over60)),
            format!("{} invoice(s) are over 60 days past due; {} of it is over 90 days.", docs.len(), dollars(over90)),
            ev, vec![],
            "Staff: draft a collection follow-up for the client's review. Nothing is sent from NXT. Consider an allowance for doubtful accounts at year-end.".into(),
            "Review the oldest balances first and confirm none are disputed or already paid.".into()));
    }
    Ok(profile.name)
}

// ---- imported company books (read-only) ----------------------------------------

fn cents(v: &Value) -> i128 {
    v.as_str().and_then(|s| s.parse::<i128>().ok()).unwrap_or(0)
}

fn liability_payroll(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    crate::company_books::is_payroll_account(name)
        && ["payable", "liabilit", "deduct", "remit", "withh", "cpp", "ei"].iter().any(|t| n.contains(t))
        && !["expense", "wage", "salar", "vacation"].iter().any(|t| n.contains(t))
}

async fn book_watch(scope: &Scope, company: &str, as_of: NaiveDate, out: &mut Vec<Hit>, gaps: &mut Vec<Value>) -> Result<Option<String>, NixonError> {
    let books = &scope.company_books;
    let tb = books.trial_balance(company).await.map_err(|e| err500(e.1))?;
    if tb["head"]["operation"]["kind"] == "archive" {
        return Ok(None); // Archived books (e.g. retired QuickBooks Desktop files) are not watched.
    }
    let name = tb["company"]["name"].as_str().unwrap_or(company).to_string();
    let analysis = &tb["analysis"];
    let (dr, cr) = (cents(&analysis["total_debit_cents"]), cents(&analysis["total_credit_cents"]));
    let tb_ref = format!("company-books:{company}/trial-balance");
    if dr != cr {
        out.push(hit(company, "books_integrity", "tb_unbalanced", "critical",
            "Imported trial balance does not balance".into(),
            format!("Debits {} and credits {} differ by {}.", dollars(dr), dollars(cr), dollars(dr - cr)),
            vec![evidence("e_tb", tb_ref.clone(), "nxt.company_books", None, "Combined trial balance totals.".into())], vec![],
            "Staff: compare the source trial balance export with the imported journal. Re-baseline from a corrected export if needed. NXT does not post a plug.".into(),
            "Do not rely on these books until they balance.".into()));
    }
    let review = &tb["reconciliation"];
    if review["review_status"].as_str().is_some_and(|s| s != "passed") || review["source_controls_passed"] == false {
        out.push(hit(company, "books_integrity", "needs_review", "medium",
            "Imported books are marked needs review".into(),
            format!("Import review status is {}; source controls passed: {}.", review["review_status"].as_str().unwrap_or("not recorded"), review["source_controls_passed"]),
            vec![evidence("e_review", tb_ref.clone(), "nxt.company_books", None, "Import review state on the committed head.".into())], vec![],
            "Staff: clear the listed blockers with a re-baseline from the same source exports, then confirm the controls pass.".into(),
            "Keep this company out of filing work until the review clears.".into()));
    }
    if review["combined_balances_match_source"] == false {
        out.push(hit(company, "books_integrity", "native_delta", "low",
            "Books differ from the imported QuickBooks baseline".into(),
            "Native NXT journals changed balances after import. That is expected after adjustments but needs a reviewer's sign-off.".into(),
            vec![evidence("e_delta", tb_ref.clone(), "nxt.company_books", None, "native_delta on the trial balance.".into())], vec![],
            "Staff: review the native adjustments list and confirm each is approved. Nothing is written back to QuickBooks.".into(),
            "Confirm before the next close.".into()));
    }

    // Window at the book's period end, never later than as_of.
    let end = tb["source"]["period_end"].as_str().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).map(|d| d.min(as_of)).unwrap_or(as_of);
    let from = end - Duration::days(WINDOW_DAYS);
    let mut rows: Vec<Value> = vec![];
    let mut offset = 0;
    for _ in 0..GL_MAX_PAGES {
        let page = books.general_ledger_range(company, offset, GL_PAGE, Some(&from.to_string()), Some(&end.to_string())).await.map_err(|e| err500(e.1))?;
        rows.extend(page["rows"].as_array().cloned().unwrap_or_default());
        match page["next_offset"].as_u64() {
            Some(n) => offset = n as usize,
            None => { offset = 0; break; }
        }
    }
    if offset != 0 {
        gaps.push(json!({"company_id": company, "note": format!("Only the first {} general-ledger lines from {from} to {end} were screened.", GL_PAGE * GL_MAX_PAGES)}));
    }

    // Payroll source deductions.
    let mut by_acct: BTreeMap<String, (BTreeMap<String, i128>, Vec<String>)> = BTreeMap::new();
    for r in &rows {
        let acct = r["account_name"].as_str().unwrap_or("");
        if !liability_payroll(acct) {
            continue;
        }
        let date = r["date"].as_str().unwrap_or("").to_string();
        let e = by_acct.entry(acct.to_string()).or_default();
        let c = cents(&r["credit_cents"]);
        if c > 0 {
            *e.0.entry(date.get(..7).unwrap_or("").to_string()).or_default() += c;
        }
        if cents(&r["debit_cents"]) > 0 {
            e.1.push(date);
        }
    }
    for (acct, (credits, debits)) in &by_acct {
        let Some((month, accrued)) = credits.iter().rev().find(|(m, _)| {
            NaiveDate::parse_from_str(&format!("{m}-15"), "%Y-%m-%d").ok().and_then(|d| d.checked_add_months(chrono::Months::new(1))).is_some_and(|due| due <= end)
        }) else { continue };
        let m_end = month_end(NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").unwrap());
        let due = NaiveDate::parse_from_str(&format!("{month}-15"), "%Y-%m-%d").unwrap().checked_add_months(chrono::Months::new(1)).unwrap();
        let remitted = debits.iter().any(|d| d.as_str() > m_end.to_string().as_str() && d.as_str() <= end.to_string().as_str());
        if !remitted {
            let key: String = acct.chars().filter(|c| c.is_ascii_alphanumeric()).take(24).collect::<String>().to_ascii_lowercase();
            out.push(hit(company, "payroll_remittance", &format!("{month}_{key}"), "high",
                format!("No remittance seen from {acct} for {month}"),
                format!("{acct} accrued {} in {month}. No debit (remittance) is recorded after {m_end} through {end}; the regular-remitter due date was {due}.", dollars(*accrued)),
                vec![evidence("e_payroll", format!("company-books:{company}/general-ledger?from={from}&to={end}"), "nxt.company_books", Some(end.to_string()), format!("Payroll liability lines on {acct}."))],
                vec![Gap { id: "g_remitter_type".into(), note: "Remitter type (regular, quarterly, accelerated) is not recorded in NXT. The screen uses the regular 15th-of-next-month due date.".into() }],
                format!("Staff: check the CRA payroll account (PD7A) for the {month} remittance. If it was paid, the payment is missing from the books: ask the client's bookkeeper to record it in QuickBooks. If not, prepare the remittance amount for the client's approval. NXT does not pay or post."),
                "Treat as urgent: late source-deduction remittances carry penalties and director liability.".into()));
        }
    }

    // Unusual: large entries versus the window median.
    let mut totals: BTreeMap<String, (i128, String)> = BTreeMap::new();
    for r in &rows {
        let e = totals.entry(r["entry_id"].as_str().unwrap_or("").to_string()).or_insert((0, r["date"].as_str().unwrap_or("").to_string()));
        e.0 += cents(&r["debit_cents"]);
    }
    if totals.len() >= 10 {
        let mut v: Vec<i128> = totals.values().map(|t| t.0).collect();
        v.sort();
        let median = v[v.len() / 2];
        let threshold = (median * LARGE_ENTRY_MULTIPLE).max(LARGE_ENTRY_FLOOR_CENTS);
        let mut big: Vec<(&String, &(i128, String))> = totals.iter().filter(|(_, t)| t.0 >= threshold).collect();
        big.sort_by(|a, b| b.1.0.cmp(&a.1.0));
        if !big.is_empty() {
            let ev = big.iter().take(5).enumerate().map(|(i, (id, t))| evidence(&format!("e_large_{i}"), format!("company-books:{company}/entries/{id}"), "nxt.company_books", Some(t.1.clone()), format!("Entry of {} on {}.", dollars(t.0), t.1))).collect();
            out.push(hit(company, "unusual_transactions", "large_entries", "medium",
                format!("{} unusually large entr{} in the last {WINDOW_DAYS} days", big.len(), if big.len() == 1 { "y" } else { "ies" }),
                format!("Screen large-entry-v1: at least {} (10x the window median of {}) .", dollars(threshold), dollars(median)),
                ev,
                vec![Gap { id: "g_large_screen".into(), note: "A size screen is not evidence of an error or a cause.".into() }],
                "Staff: match each entry to its source document and confirm the coding with the client's bookkeeper.".into(),
                "Review before the next close.".into()));
        }
    }
    Ok(Some(name))
}

// ---- tax deadlines -------------------------------------------------------------

async fn tax_watch(scope: &Scope, as_of: NaiveDate, out: &mut Vec<Hit>) -> Result<(), NixonError> {
    let returns = scope.tax.list(None).await.map_err(|e| err500(e.1))?;
    for r in returns {
        let program = r["program"].as_str().unwrap_or("");
        let status = r["status"].as_str().unwrap_or("");
        if !matches!(program, "T1" | "T2") || matches!(status, "filed" | "archived") {
            continue;
        }
        let year = r["tax_year"].as_i64().unwrap_or(0) as i32;
        let deadline = crate::tax::filing_desk::deadline_for(program, year, r["period_end"].as_str());
        let Ok(d) = NaiveDate::parse_from_str(&deadline, "%Y-%m-%d") else { continue };
        let days = (d - as_of).num_days();
        if days > 60 {
            continue;
        }
        let company = r["client_ref"].as_str().filter(|s| !s.is_empty()).unwrap_or("unassigned").to_string();
        let id = r["id"].as_str().unwrap_or("");
        let risk = if days < 0 { "high" } else if days <= 14 { "medium" } else { "low" };
        let when = if days < 0 { format!("{} days past the {deadline} deadline", -days) } else { format!("{days} days to the {deadline} deadline") };
        out.push(hit(&company, "filing_deadlines", &format!("{program}_{year}_{id}"), risk,
            format!("{program} {year} return is {status}: {when}"),
            format!("Return {id} ({program} {year}) is not filed. Statutory deadline {deadline}."),
            vec![evidence("e_return", format!("tax:{id}"), "nxt.tax", Some(deadline.clone()), format!("Return status {status}."))], vec![],
            "Staff: move the return through review and sign-off, then to the internal Filing Desk. A human files outside NXT.".into(),
            if days < 0 { "File as soon as possible to limit late-filing penalties.".into() } else { "Plan preparer and reviewer time now.".into() }));
    }
    Ok(())
}

// ---- view, run, draft ----------------------------------------------------------

async fn collect(scope: &Scope, as_of: NaiveDate) -> Result<(Vec<Company>, Vec<Hit>, Vec<Value>), NixonError> {
    let mut companies: BTreeMap<String, Company> = BTreeMap::new();
    let mut hits = vec![];
    let mut gaps = vec![];
    let firm_name = ledger_watch(&scope.ledger, "firm", as_of, &mut hits).await?;
    companies.insert("firm".into(), Company { id: "firm".into(), name: firm_name, sources: vec!["firm_ledger"] });

    let opened = crate::ledger::routes::opened_company_books(&scope.ledger.store).await.map_err(err500)?;
    for id in opened.into_iter().take(MAX_COMPANIES) {
        let l = Ledger::for_company(scope.ledger.store.clone(), &id);
        let name = ledger_watch(&l, &id, as_of, &mut hits).await?;
        companies.entry(id.clone()).or_insert(Company { id: id.clone(), name, sources: vec![] }).sources.push("company_ledger");
    }
    let page = scope.company_books.list_summaries(0, 500).await.map_err(|e| err500(e.1))?;
    let committed: Vec<String> = page["companies"].as_array().into_iter().flatten()
        .filter(|c| c["stage"] == "import_committed")
        .filter_map(|c| c["company_id"].as_str().map(String::from)).take(MAX_COMPANIES).collect();
    for id in committed {
        match book_watch(scope, &id, as_of, &mut hits, &mut gaps).await {
            Ok(Some(name)) => {
                let c = companies.entry(id.clone()).or_insert(Company { id: id.clone(), name: name.clone(), sources: vec![] });
                if c.name.is_empty() || c.name == id { c.name = name; }
                c.sources.push("imported_company_books");
            }
            Ok(None) => {}
            Err(e) => gaps.push(json!({"company_id": id, "note": format!("Books could not be read: {}", e.message)})),
        }
    }
    tax_watch(scope, as_of, &mut hits).await?;
    for h in &hits {
        companies.entry(h.company.clone()).or_insert(Company { id: h.company.clone(), name: h.company.clone(), sources: vec!["tax_workspace"] });
    }
    let rank = |r: &str| match r { "critical" => 0, "high" => 1, "medium" => 2, "low" => 3, _ => 4 };
    hits.sort_by(|a, b| rank(&a.finding.risk_level).cmp(&rank(&b.finding.risk_level)).then(a.company.cmp(&b.company)).then(a.finding.id.cmp(&b.finding.id)));
    Ok((companies.into_values().collect(), hits, gaps))
}

fn as_of_or_today(s: Option<&str>) -> Result<NaiveDate, NixonError> {
    day(s.filter(|s| !s.is_empty()).unwrap_or(&crate::ledger::engine::today()))
}

fn guardrails() -> Value {
    json!({
        "books": "read_only", "posting": "disabled", "filing": "disabled", "payments": "disabled",
        "outbound_messages": "none", "client_contact": "none", "audience": "internal_staff_firm_wide_only",
        "decisions": "named person via /api/nixon/reviews/{id}/proposals/{pid}/decide (approve or dismiss); approval records a person and executes nothing",
    })
}

/// The live view. Reads only; writes nothing, not even an audit row.
pub async fn view(scope: &Scope, q: ViewQuery) -> Result<Value, NixonError> {
    let as_of = as_of_or_today(q.as_of.as_deref())?;
    let (companies, hits, gaps) = collect(scope, as_of).await?;
    let companies: Vec<Value> = companies.iter().map(|c| {
        let mine: Vec<&Hit> = hits.iter().filter(|h| h.company == c.id).collect();
        let n = |r: &str| mine.iter().filter(|h| h.finding.risk_level == r).count();
        json!({"company_id": c.id, "name": c.name, "sources": c.sources,
               "counts": {"critical": n("critical"), "high": n("high"), "medium": n("medium"), "low": n("low")}, "findings": mine.len()})
    }).collect();
    let findings: Vec<Value> = hits.iter().map(|h| json!({
        "company_id": h.company, "watcher": h.watcher, "finding": h.finding,
        "draft_fix": h.draft_fix, "recommendation": h.recommendation,
    })).collect();
    Ok(json!({
        "as_of": as_of.to_string(), "scope": SCOPE, "guardrails": guardrails(),
        "watchers": watchers(), "companies": companies, "findings": findings, "gaps": gaps,
        "ai": ai_gateway::describe(&ai_gateway::process_env),
    }))
}

/// Saves the current findings as a Nixon review with one pending proposal each.
pub async fn run(scope: &Scope, req: RunRequest) -> Result<Review, NixonError> {
    let as_of = as_of_or_today(req.as_of.as_deref())?;
    let (_, hits, gaps) = collect(scope, as_of).await?;
    let mut findings = vec![];
    let mut proposals = vec![];
    for h in hits {
        let mut f = h.finding;
        let pid = format!("p_{}", f.id);
        f.proposal_id = Some(pid.clone());
        proposals.push(Proposal {
            id: pid,
            finding_id: Some(f.id.clone()),
            kind: "staff_follow_up".into(),
            changes_books: false,
            files_tax: false,
            execution: "disabled".into(),
            applied: false,
            status: "pending_approval".into(),
            summary: h.draft_fix.clone(),
            prepared_by: PREPARED_BY.into(),
            payload: json!({
                "company_id": h.company, "watcher": h.watcher,
                "draft_fix": h.draft_fix, "recommendation": h.recommendation,
                "effect": "none",
                "note": "Approve records that a named person accepts this follow-up. It does not post, file, pay, message or contact anyone.",
            }),
        });
        findings.push(f);
    }
    let verifications = vec![Verification {
        id: "v_cra_notices".into(), name: "cra_notices".into(), state: "not_run".into(),
        detail: "No CRA notice source exists in NXT.".into(), evidence_ids: vec![],
    }];
    let digest = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(serde_json::to_vec(&findings).unwrap_or_default());
        hex::encode(h.finalize())
    };
    let review = Review {
        id: service::nid("ge"),
        correlation_id: service::nid("cor"),
        created_at: crate::ledger::engine::now(),
        as_of: as_of.to_string(),
        currency: "CAD".into(),
        source_class: SourceClass::WorkspaceBooks,
        digest,
        idempotency_key: None,
        autonomy_level: "advisory".into(),
        execution_mode: "advisory".into(),
        posting: "disabled".into(),
        filing: "disabled".into(),
        summary: format!("God's Eye: {} finding(s) across the firm as of {as_of}. {} gap(s).", findings.len(), gaps.len()),
        findings,
        proposals,
        verifications,
        decisions: vec![],
        cutover: None,
        surfaces: vec![],
        scope: SCOPE.into(),
    };
    service::save_review(&scope.ledger, &review).await?;
    scope.ledger.audit("nixon.gods_eye.run", format!("correlation={} review={} findings={} posting=disabled filing=disabled", review.correlation_id, review.id, review.findings.len())).await?;
    Ok(review)
}

const DRAFT_SYSTEM: &str = "You help internal staff at a Canadian accounting firm. Rewrite the finding and the drafted follow-up as a short, plain note for the preparer (max 120 words). Do not invent figures, dates, causes or legal advice. Do not address the client. Never approve, post, file or pay.";

/// Optional AI wording for one proposal. Off by default; client data only on zero-retention hosts.
pub async fn ai_draft(scope: &Scope, review_id: &str, proposal_id: &str) -> Result<Review, NixonError> {
    if !ai_gateway::enabled(&ai_gateway::process_env) {
        return Err(NixonError::new(409, "ai_drafting_disabled: set GODS_EYE_AI=on with a zero-retention gateway to enable it. Watchers work without it."));
    }
    let mut review = service::get_review(&scope.ledger, review_id).await?;
    if review.scope != SCOPE {
        return Err(NixonError::new(400, "AI drafting is only for God's Eye runs"));
    }
    let p = review.proposals.iter().find(|p| p.id == proposal_id).ok_or_else(|| NixonError::new(404, "proposal not found"))?;
    let f = review.findings.iter().find(|f| Some(&f.id) == p.finding_id.as_ref()).ok_or_else(|| NixonError::new(404, "finding not found"))?;
    let user = format!("Finding: {}\n{}\nDrafted follow-up: {}", f.title, f.summary, p.payload["draft_fix"].as_str().unwrap_or(""));
    let d = ai_gateway::draft(Sensitivity::ClientData, DRAFT_SYSTEM, &user, &ai_gateway::process_env).await.map_err(|m| NixonError::new(502, format!("AI draft unavailable: {m}")))?;
    let p = review.proposals.iter_mut().find(|p| p.id == proposal_id).unwrap();
    p.payload["ai_draft"] = json!({
        "text": d.text, "provider": d.provider, "model": d.model, "zero_data_retention": d.zero_data_retention,
        "evidence_level": "ai_hypothesis", "note": "AI wording only. Not verified; the deterministic finding is the record.",
        "at": crate::ledger::engine::now(),
    });
    service::save_review(&scope.ledger, &review).await?;
    scope.ledger.audit("nixon.gods_eye.ai_draft", format!("correlation={} review={review_id} proposal={proposal_id} provider={} model={} zdr={}", review.correlation_id, d.provider, d.model, d.zero_data_retention)).await?;
    Ok(review)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_quarters() {
        assert_eq!(dollars(123_456_789), "$1,234,567.89");
        assert_eq!(dollars(-5), "-$0.05");
        let d = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let (s, e) = quarter_bounds(d);
        assert_eq!((s.to_string(), e.to_string()), ("2026-10-01".into(), "2026-12-31".into()));
        assert_eq!(month_end(NaiveDate::from_ymd_opt(2026, 2, 3).unwrap()).to_string(), "2026-02-28");
    }

    #[test]
    fn payroll_liability_names() {
        for n in ["Payroll Liabilities", "CPP Payable", "EI payable", "Source deductions payable", "Income tax payable"] { assert!(liability_payroll(n), "{n}"); }
        for n in ["Payroll Expenses", "Wages & Salaries", "Vacation pay accrued", "Rent"] { assert!(!liability_payroll(n), "{n}"); }
    }

    #[test]
    fn cra_notices_report_no_source() {
        let w = watchers();
        let cra = w.as_array().unwrap().iter().find(|w| w["id"] == "cra_notices").unwrap();
        assert_eq!(cra["status"], "no_source");
    }
}
