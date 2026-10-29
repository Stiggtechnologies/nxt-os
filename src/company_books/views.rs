//! Read-only company views over a committed book: a flat general ledger
//! (every source and native line, with account names) and a payroll view
//! derived from journal lines posted to payroll-named accounts.
//!
//! The payroll view is NOT a payroll register. QuickBooks Desktop files seen
//! so far record payroll as cheques/journals against payroll accounts, so
//! the view reports what the journal proves (transactions, accounts, debit
//! and credit totals per year) and says `pay_stubs_available: false`.
use super::*;

/// Account-name tokens treated as payroll. Kept explicit and tested.
const PAYROLL_TOKENS: &[&str] = &[
    "payroll", "wage", "salar", "cpp", "employment insurance", "source deduct",
    "vacation pay", "wcb", "remittance", "paycheque", "paycheck", "employer",
    "income tax payable",
];

pub(crate) fn is_payroll_account(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    PAYROLL_TOKENS.iter().any(|t| n.contains(t))
        || n.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == "ei")
}

impl CompanyBooks {
    pub async fn general_ledger_page(&self, company: &str, offset: usize, limit: usize) -> Result<Value> {
        if limit == 0 || limit > 1000 {
            return Err(bad("Page limit must be 1-1000"));
        }
        let (h, p, accounts, entries) = self.book(company).await?;
        let names: BTreeMap<&str, &str> = accounts.iter().map(|a| (a.id.as_str(), a.name.as_str())).collect();
        let (mut total_lines, mut dr, mut cr) = (0usize, 0i128, 0i128);
        let mut rows = vec![];
        for (ei, e) in entries.iter().enumerate() {
            for (li, l) in e.lines.iter().enumerate() {
                let (d, c) = (money(&l.debit_cents)?, money(&l.credit_cents)?);
                dr = dr.checked_add(d).ok_or_else(corrupt)?;
                cr = cr.checked_add(c).ok_or_else(corrupt)?;
                if total_lines >= offset && rows.len() < limit {
                    rows.push(json!({
                        "entry_id": e.id, "entry_index": ei, "line_index": li, "date": e.date,
                        "source_id": e.source_id, "transaction_type": e.memo,
                        "account_id": l.account_id,
                        "account_name": names.get(l.account_id.as_str()).copied().unwrap_or(""),
                        "memo": l.memo, "debit_cents": l.debit_cents, "credit_cents": l.credit_cents,
                        "source_row": l.source_row, "native": ei >= p.analysis.entries,
                    }));
                }
                total_lines += 1;
            }
        }
        let next = offset + rows.len();
        Ok(json!({
            "company_id": company, "head": h, "order": "immutable source order",
            "entries_total": entries.len(), "lines_total": total_lines, "total": total_lines,
            "source_entries": p.analysis.entries, "source_lines": p.analysis.lines,
            "debit_total_cents": dr.to_string(), "credit_total_cents": cr.to_string(),
            "offset": offset, "limit": limit, "rows": rows,
            "has_more": next < total_lines, "next_offset": (next < total_lines).then_some(next),
        }))
    }

    pub async fn payroll_view(&self, company: &str) -> Result<Value> {
        let (h, _, accounts, entries) = self.book(company).await?;
        let pay: BTreeMap<&str, &str> = accounts.iter().filter(|a| is_payroll_account(&a.name))
            .map(|a| (a.id.as_str(), a.name.as_str())).collect();
        let mut years: BTreeMap<String, (usize, i128, i128)> = BTreeMap::new();
        let mut per_acct: BTreeMap<&str, (usize, i128, i128)> = BTreeMap::new();
        let mut txns = vec![];
        for e in &entries {
            let hit: Vec<_> = e.lines.iter().filter(|l| pay.contains_key(l.account_id.as_str())).collect();
            if hit.is_empty() { continue; }
            let (mut d, mut c) = (0i128, 0i128);
            for l in &hit {
                let (ld, lc) = (money(&l.debit_cents)?, money(&l.credit_cents)?);
                d += ld; c += lc;
                let s = per_acct.entry(pay[l.account_id.as_str()]).or_default();
                s.0 += 1; s.1 += ld; s.2 += lc;
            }
            let y = years.entry(e.date.get(..4).unwrap_or("").to_string()).or_default();
            y.0 += 1; y.1 += d; y.2 += c;
            txns.push(json!({"entry_id": e.id, "date": e.date, "transaction_type": e.memo,
                "source_id": e.source_id, "payroll_lines": hit.len(), "lines": e.lines.len(),
                "payroll_debit_cents": d.to_string(), "payroll_credit_cents": c.to_string()}));
        }
        Ok(json!({
            "company_id": company, "head": h,
            "basis": "journal_lines_on_payroll_named_accounts",
            "pay_stubs_available": false, "employees_available": false,
            "note": "Derived from the committed journal. Gross, net and deductions per pay run need a payroll register export (QBO payroll .xls or QBD payroll summary); none is attached to this book.",
            "payroll_accounts": per_acct.iter().map(|(n, s)| json!({"name": n, "lines": s.0,
                "debit_cents": s.1.to_string(), "credit_cents": s.2.to_string()})).collect::<Vec<_>>(),
            "transactions_total": txns.len(),
            "by_year": years.iter().map(|(y, s)| json!({"year": y, "transactions": s.0,
                "debit_cents": s.1.to_string(), "credit_cents": s.2.to_string()})).collect::<Vec<_>>(),
            "transactions": txns,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::is_payroll_account as p;
    #[test]
    fn payroll_names() {
        for n in ["Payroll Expenses", "Payroll Liabilities", "WCB", "Wages & Salaries", "CPP Payable", "EI payable", "Vacation pay accrued"] { assert!(p(n), "{n}"); }
        for n in ["Medical Expense", "Owners Draw", "Rent", "Equipment", "Receivables"] { assert!(!p(n), "{n}"); }
    }
}
