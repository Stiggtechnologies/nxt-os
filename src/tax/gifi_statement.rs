//! GIFI summary income statement from declared income-statement rows.
//!
//! Ranges follow the rollup rule this engine already enforces: revenue detail is
//! 8000-8298 with subtotal 8299, expenses are 8521-9367 with subtotal 9368, and
//! net income is 9999. This is a summary, not a complete GIFI S125 statement.

use super::corporate::GifiRow;
use serde_json::{Value, json};

const REVENUE_DETAIL: (u32, u32) = (8000, 8298);
const REVENUE_TOTAL: &str = "8299";
const EXPENSE_DETAIL: (u32, u32) = (8521, 9367);
const EXPENSE_TOTAL: &str = "9368";
const NET_TOTAL: &str = "9999";

fn in_range(code: &str, (low, high): (u32, u32)) -> bool {
    code.parse::<u32>().map(|value| (low..=high).contains(&value)).unwrap_or(false)
}

fn group(income: &[&GifiRow], range: (u32, u32), total_code: &str) -> (i64, i64) {
    let detail: Vec<&&GifiRow> = income.iter().filter(|row| in_range(&row.code, range)).collect();
    if detail.is_empty() {
        let total = income.iter().find(|row| row.code == total_code);
        return (total.map(|row| row.current_cents).unwrap_or(0), total.map(|row| row.prior_cents).unwrap_or(0));
    }
    (detail.iter().map(|row| row.current_cents).sum(), detail.iter().map(|row| row.prior_cents).sum())
}

pub fn build(rows: &[GifiRow]) -> Value {
    let income: Vec<&GifiRow> = rows.iter().filter(|row| row.section == "income_statement").collect();
    let (revenue, revenue_prior) = group(&income, REVENUE_DETAIL, REVENUE_TOTAL);
    let (expenses, expenses_prior) = group(&income, EXPENSE_DETAIL, EXPENSE_TOTAL);
    let net = revenue - expenses;
    let net_prior = revenue_prior - expenses_prior;
    let declared = income.iter().find(|row| row.code == NET_TOTAL).map(|row| row.current_cents);
    let lines: Vec<Value> = income
        .iter()
        .map(|row| json!({
            "code": row.code, "label": row.label,
            "current_cents": row.current_cents, "prior_cents": row.prior_cents,
        }))
        .collect();
    json!({
        "revenue_cents": revenue, "prior_revenue_cents": revenue_prior,
        "expense_cents": expenses, "prior_expense_cents": expenses_prior,
        "net_cents": net, "prior_net_cents": net_prior,
        "declared_net_cents": declared,
        "matches_declared_net": declared.map(|value| value == net),
        "lines": lines,
        "note": "Summary from declared GIFI income-statement rows: revenue detail 8000-8298 (or 8299), expenses 8521-9367 (or 9368), net compared with 9999 when present. Subtotal and detail rows are never summed together. This is not a complete GIFI S125 statement and does not verify the declarations.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, current: i64, prior: i64) -> GifiRow {
        GifiRow { section: "income_statement".into(), code: code.into(), label: format!("code {code}"), current_cents: current, prior_cents: prior }
    }

    #[test]
    fn detail_rows_are_summed_and_compared_with_the_declared_net() {
        let rows = vec![row("8000", 100_000, 90_000), row("8100", 20_000, 10_000), row("8521", 40_000, 30_000), row("9368", 40_000, 30_000), row("9999", 80_000, 70_000)];
        let statement = build(&rows);
        assert_eq!(statement["revenue_cents"], 120_000);
        assert_eq!(statement["expense_cents"], 40_000);
        assert_eq!(statement["net_cents"], 80_000);
        assert_eq!(statement["prior_net_cents"], 70_000);
        assert_eq!(statement["matches_declared_net"], true);
    }

    #[test]
    fn subtotals_are_used_only_when_detail_is_absent() {
        let rows = vec![row("8299", 150_000, 140_000), row("9368", 50_000, 45_000), row("9999", 90_000, 80_000)];
        let statement = build(&rows);
        assert_eq!(statement["revenue_cents"], 150_000);
        assert_eq!(statement["expense_cents"], 50_000);
        assert_eq!(statement["net_cents"], 100_000);
        assert_eq!(statement["matches_declared_net"], false);
        assert_eq!(statement["declared_net_cents"], 90_000);
    }
}
