//! Ledger capability: the `catch_up_cleanup` and `monthly_close` steps.
//!
//! It reads the client's company books and reports what the ledger can prove:
//! the books are committed without an open review, the trial balance balances,
//! and the period is locked (for `monthly_close`, through the last month-end).
//!
//! That is only part of each step's gate. NXT holds no bank reconciliation or
//! subledger allocation and does not track exceptions, so the evidence lists
//! them as not checked, and both steps are locked to `human`: `Ready` puts the
//! step in the review queue for a person to attest the rest. It only reads and
//! gives the agent no actions; it never posts to the books or locks a period.

use super::capability::{BoxFut, Capability, Readiness, StepContext};
use crate::accounting_extension::capture_close;
use crate::company_books::CompanyBooks;
use chrono::{Datelike, NaiveDate, Utc};
use serde_json::{Value, json};
use std::sync::Arc;

const BASELINE_NOT_CHECKED: [&str; 3] = ["bank reconciliation", "subledger allocations", "open exceptions"];
const NOT_CHECKED: [&str; 4] = ["independent source completeness", "bank reconciliation", "subledger allocations", "open exceptions"];

pub struct LedgerClose {
    company_books: Arc<CompanyBooks>,
    as_of: Option<NaiveDate>,
    accounting_controls: bool,
}

fn last_month_end(today: NaiveDate) -> Option<String> {
    Some(today.with_day(1)?.pred_opt()?.format("%Y-%m-%d").to_string())
}

fn cents(v: &Value) -> Option<i128> {
    v.as_str()?.parse().ok()
}

/// What keeps a committed book from being shown to a reviewer as closed.
fn blockers(evidence: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if evidence["review_status"] == "needs_review" {
        out.push("books were committed as needs_review; resolve the review first".to_string());
    }
    match (cents(&evidence["total_debit_cents"]), cents(&evidence["total_credit_cents"])) {
        (Some(d), Some(c)) if d == c => {}
        (Some(d), Some(c)) => out.push(format!("trial balance is out by {} cents", (d - c).abs())),
        _ => out.push("trial balance totals could not be read".to_string()),
    }
    match (evidence["locked_through"].as_str(), evidence["required_locked_through"].as_str()) {
        (None, _) => out.push("no period is locked; a reviewer locks the period once it is reconciled".to_string()),
        (Some(locked), Some(required)) if locked < required => out.push(format!("period is locked through {locked}; {required} is not closed yet")),
        _ => {}
    }
    out
}

impl LedgerClose {
    pub fn new(company_books: Arc<CompanyBooks>) -> Self {
        Self { company_books, as_of: None, accounting_controls: false }
    }

    pub(crate) fn with_accounting_controls(mut self, enabled: bool) -> Self {
        self.accounting_controls = enabled;
        self
    }

    /// Assess as of a fixed date instead of today.
    pub fn as_of(mut self, date: NaiveDate) -> Self {
        self.as_of = Some(date);
        self
    }

    fn required_lock(&self, step: &str) -> Option<String> {
        (step == "monthly_close").then(|| last_month_end(self.as_of.unwrap_or_else(|| Utc::now().date_naive()))).flatten()
    }

    async fn assess_baseline(&self, ctx: &StepContext) -> Readiness {
        let blocked = |reason: String| Readiness::Blocked { reasons: vec![reason], evidence: json!({"not_checked": BASELINE_NOT_CHECKED}) };
        let detail = match self.company_books.detail(&ctx.client_id).await {
            Ok(v) => v,
            Err(e) => return blocked(format!("company books unavailable: {}", e.1)),
        };
        if detail["stage"] != "import_committed" {
            return blocked(format!("books are not committed (stage: {})", detail["stage"].as_str().unwrap_or("unknown")));
        }
        let trial = match self.company_books.trial_balance(&ctx.client_id).await {
            Ok(v) => v,
            Err(e) => return blocked(format!("trial balance unavailable: {}", e.1)),
        };
        let evidence = json!({
            "head_sha256": detail["head_sha256"],
            "locked_through": trial["head"]["locked_through"],
            "required_locked_through": self.required_lock(&ctx.step),
            "review_status": trial["head"]["review"]["status"],
            "total_debit_cents": trial["analysis"]["total_debit_cents"],
            "total_credit_cents": trial["analysis"]["total_credit_cents"],
            "not_checked": BASELINE_NOT_CHECKED,
        });
        readiness(evidence, false)
    }
}

fn readiness(evidence: Value, accounting_controls: bool) -> Readiness {
    let mut reasons = blockers(&evidence);
    if accounting_controls {
        for check in evidence["checks"].as_array().into_iter().flatten() {
            if check["passed"] != true && check["id"] != "trial_balance_balanced" {
                reasons.push(format!("close control failed: {}", check["id"].as_str().unwrap_or("unknown")));
            }
        }
    }
    if reasons.is_empty() { Readiness::Ready { evidence } } else { Readiness::Blocked { reasons, evidence } }
}

impl Capability for LedgerClose {
    fn id(&self) -> &'static str {
        "ledger_close"
    }
    fn steps(&self) -> &'static [&'static str] {
        &["catch_up_cleanup", "monthly_close"]
    }
    fn assess<'a>(&'a self, ctx: &'a StepContext) -> BoxFut<'a, Readiness> {
        Box::pin(async move {
            if !self.accounting_controls {
                return self.assess_baseline(ctx).await;
            }
            let blocked = |reason: String| Readiness::Blocked { reasons: vec![reason], evidence: json!({"not_checked": NOT_CHECKED}) };
            let report = match capture_close(&self.company_books, &ctx.client_id).await {
                Ok(v) => v,
                Err(e) => return blocked(format!("trial balance unavailable: {}", e.1)),
            };
            let evidence = json!({
                "head_sha256": report["head_sha256"],
                "locked_through": report["locked_through"],
                "required_locked_through": self.required_lock(&ctx.step),
                "review_status": report["review_status"],
                "total_debit_cents": report["total_debit_cents"],
                "total_credit_cents": report["total_credit_cents"],
                "checks": report["checks"],
                "not_checked": NOT_CHECKED,
            });
            readiness(evidence, true)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Value {
        json!({"locked_through": "2026-02-28", "required_locked_through": "2026-02-28", "review_status": null, "total_debit_cents": "100", "total_credit_cents": "100"})
    }

    #[test]
    fn last_month_end_handles_year_and_leap_boundaries() {
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(last_month_end(day(2026, 1, 15)).unwrap(), "2025-12-31");
        assert_eq!(last_month_end(day(2028, 3, 1)).unwrap(), "2028-02-29");
    }

    #[test]
    fn clean_locked_book_has_no_blockers() {
        assert!(blockers(&book()).is_empty());
    }

    #[tokio::test]
    async fn default_off_preserves_legacy_close_evidence_for_uncommitted_books() {
        let capability = LedgerClose::new(Arc::new(CompanyBooks::new(crate::store::Store::memory())));
        let ctx = StepContext {
            client_id: "demo".into(), step: "monthly_close".into(), sources: Default::default(),
        };
        let outcome = capability.assess(&ctx).await;
        assert_eq!(outcome, Readiness::Blocked {
            reasons: vec!["books are not committed (stage: acquisition_pending)".into()],
            evidence: json!({"not_checked": BASELINE_NOT_CHECKED}),
        });
        let mut evidence = book();
        evidence["checks"] = json!([{"id":"source_import_controls_passed","passed":false}]);
        assert!(matches!(readiness(evidence.clone(),false), Readiness::Ready { .. }));
        assert!(matches!(readiness(evidence,true), Readiness::Blocked { .. }));
    }

    #[test]
    fn each_failed_check_blocks() {
        for (field, value, expected) in [
            ("review_status", json!("needs_review"), "needs_review"),
            ("total_credit_cents", json!("90"), "out by 10 cents"),
            ("total_debit_cents", json!(null), "could not be read"),
            ("locked_through", json!(null), "no period is locked"),
            ("locked_through", json!("2026-01-31"), "not closed yet"),
        ] {
            let mut b = book();
            b[field] = value;
            let reasons = blockers(&b);
            assert!(reasons.iter().any(|r| r.contains(expected)), "{field}: {reasons:?}");
        }
    }
}
