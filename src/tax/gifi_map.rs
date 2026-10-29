//! Builds GIFI Schedule 100 (balance sheet) and Schedule 125 (income
//! statement) rows from a company-books trial balance and an explicit,
//! reviewer-supplied account -> GIFI code mapping. Nothing is guessed: every
//! non-zero account must be mapped or it is reported as unmapped and the
//! result is not ready for review.
use super::corporate::GifiRow;
use crate::company_books::models::Balance;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping { pub account_id: String, pub gifi_code: String }

fn cents(s: &str) -> Result<i64, String> { s.trim().parse::<i64>().map_err(|_| format!("invalid cents value {s}")) }
fn code_num(c: &str) -> Option<u32> { if c.len() == 4 && c.bytes().all(|b| b.is_ascii_digit()) { c.parse().ok() } else { None } }

/// Section and natural sign for a GIFI detail code (+1 debit-normal, -1 credit-normal).
fn classify(code: u32) -> Option<(&'static str, i64)> {
    match code {
        1000..=2598 => Some(("balance_sheet", 1)),
        2600..=3498 => Some(("balance_sheet", -1)),
        3500..=3619 | 3621..=3639 => Some(("balance_sheet", -1)),
        8000..=8298 => Some(("income_statement", -1)),
        8300..=8517 | 8520..=9366 => Some(("income_statement", 1)),
        _ => None,
    }
}
const TOTALS: &[u32] = &[2599, 3499, 3620, 3640, 8299, 8518, 9367, 9368, 9999];

pub fn build(balances: &[Balance], mapping: &[Mapping]) -> Result<Value, String> {
    if mapping.len() > 2000 { return Err("mapping too large".into()); }
    let map: BTreeMap<&str, &str> = mapping.iter().map(|m| (m.account_id.as_str(), m.gifi_code.as_str())).collect();
    let mut detail: BTreeMap<u32, (i64, Vec<String>)> = BTreeMap::new();
    let mut unmapped = vec![];
    let (mut dr, mut cr) = (0i64, 0i64);
    for b in balances {
        let (d, c) = (cents(&b.debit_cents)?, cents(&b.credit_cents)?);
        dr = dr.checked_add(d).ok_or("overflow")?; cr = cr.checked_add(c).ok_or("overflow")?;
        let net = d - c;
        let Some(code) = map.get(b.account_id.as_str()) else { if net != 0 { unmapped.push(json!({"account_id": b.account_id, "name": b.name, "net_debit_cents": net})); } continue; };
        let n = code_num(code).ok_or_else(|| format!("GIFI code {code} must be four digits"))?;
        if TOTALS.contains(&n) { return Err(format!("map accounts to detail codes, not total code {n}")); }
        let (_, sign) = classify(n).ok_or_else(|| format!("GIFI code {n} is outside the supported Schedule 100/125 ranges"))?;
        let e = detail.entry(n).or_insert((0, vec![]));
        e.0 += net * sign; e.1.push(b.account_id.clone());
    }
    let sum = |lo: u32, hi: u32| detail.iter().filter(|(k, _)| (lo..=hi).contains(*k)).map(|(_, v)| v.0).sum::<i64>();
    let revenue = detail.iter().filter(|(k, _)| (8000..=8298).contains(*k)).map(|(_, v)| v.0).sum::<i64>();
    let cogs = detail.iter().filter(|(k, _)| (8300..=8517).contains(*k)).map(|(_, v)| v.0).sum::<i64>();
    let opex = detail.iter().filter(|(k, _)| (8520..=9366).contains(*k)).map(|(_, v)| v.0).sum::<i64>();
    let net_income = revenue - cogs - opex;
    let assets = sum(1000, 2598);
    let liabilities = detail.iter().filter(|(k, _)| (2600..=3498).contains(*k)).map(|(_, v)| v.0).sum::<i64>();
    // Pre-closing trial balance: current-year net income is added to retained earnings (3600) on Schedule 100.
    let equity_before = detail.iter().filter(|(k, _)| (3500..=3639).contains(*k)).map(|(_, v)| v.0).sum::<i64>();
    let equity = equity_before + net_income;
    let mut rows: Vec<GifiRow> = vec![];
    for (k, (v, _)) in &detail {
        let section = classify(*k).unwrap().0;
        let amount = if *k == 3600 { v + net_income } else { *v };
        rows.push(GifiRow { section: section.into(), code: format!("{k:04}"), label: String::new(), current_cents: amount, prior_cents: 0 });
    }
    if !detail.contains_key(&3600) && net_income != 0 { rows.push(GifiRow { section: "balance_sheet".into(), code: "3600".into(), label: "Retained earnings incl. current-year net income".into(), current_cents: net_income, prior_cents: 0 }); }
    for (code, section, v) in [(2599, "balance_sheet", assets), (3499, "balance_sheet", liabilities), (3620, "balance_sheet", equity), (3640, "balance_sheet", liabilities + equity), (8299, "income_statement", revenue), (8518, "income_statement", cogs), (9367, "income_statement", opex), (9368, "income_statement", cogs + opex), (9999, "income_statement", net_income)] {
        rows.push(GifiRow { section: section.into(), code: code.to_string(), label: "Total".into(), current_cents: v, prior_cents: 0 });
    }
    rows.sort_by(|a, b| a.section.cmp(&b.section).then(a.code.cmp(&b.code)));
    let mut issues = vec![];
    if dr != cr { issues.push(json!({"code": "trial_balance_unbalanced", "blocks_review": true, "message": format!("Trial balance debits {dr} != credits {cr}")})); }
    if !unmapped.is_empty() { issues.push(json!({"code": "unmapped_accounts", "blocks_review": true, "message": format!("{} non-zero accounts have no GIFI code", unmapped.len())})); }
    if assets != liabilities + equity { issues.push(json!({"code": "balance_sheet_out_of_balance", "blocks_review": true, "message": format!("2599 {assets} != 3640 {}", liabilities + equity)})); }
    Ok(json!({"rows": rows, "unmapped": unmapped, "issues": issues, "ready": issues.is_empty(),
        "note": "Generated from the company-books trial balance and the supplied mapping. Review every mapping; 9999 here is accounting net income before Schedule 1 adjustments."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bal(id: &str, d: i64, c: i64) -> Balance { Balance { account_id: id.into(), name: id.into(), kind: None, debit_cents: d.to_string(), credit_cents: c.to_string(), journal_debit_cents: "0".into(), journal_credit_cents: "0".into(), source_control_basis: None } }
    fn m(a: &str, g: &str) -> Mapping { Mapping { account_id: a.into(), gifi_code: g.into() } }
    #[test] fn balanced_books_produce_balanced_schedules() {
        let tb = [bal("cash", 150_000, 0), bal("ap", 0, 20_000), bal("shares", 0, 10_000), bal("re", 0, 40_000), bal("sales", 0, 200_000), bal("rent", 120_000, 0)];
        let map = [m("cash", "1001"), m("ap", "2620"), m("shares", "3500"), m("re", "3600"), m("sales", "8000"), m("rent", "8910")];
        let out = build(&tb, &map).unwrap();
        assert_eq!(out["ready"], true, "{out}");
        let get = |c: &str| out["rows"].as_array().unwrap().iter().find(|r| r["code"] == c).unwrap()["current_cents"].as_i64().unwrap();
        assert_eq!(get("9999"), 80_000); assert_eq!(get("2599"), 150_000); assert_eq!(get("3600"), 120_000); assert_eq!(get("3640"), 150_000);
    }
    #[test] fn unmapped_and_total_codes_are_caught() {
        let out = build(&[bal("cash", 100, 0), bal("x", 0, 100)], &[m("cash", "1001")]).unwrap();
        assert_eq!(out["ready"], false);
        assert!(build(&[bal("cash", 100, 0)], &[m("cash", "2599")]).is_err());
        assert!(build(&[bal("cash", 100, 0)], &[m("cash", "7000")]).is_err());
    }
}
