//! Financial reports computed from posted (non-void) entries.

use super::engine::{LResult, Ledger, parse_date, today};
use super::models::*;
use chrono::{Datelike, NaiveDate};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

struct Books {
    company: Company,
    accounts: Vec<Account>,
    entries: Vec<Entry>,
}

impl Books {
    fn by_id(&self) -> HashMap<&str, &Account> {
        self.accounts.iter().map(|a| (a.id.as_str(), a)).collect()
    }

    /// Net (debit - credit) per account for entries with from <= date <= to.
    fn net(&self, from: Option<&str>, to: &str) -> HashMap<String, i64> {
        let mut m: HashMap<String, i64> = HashMap::new();
        for e in self.entries.iter().filter(|e| !e.voided && e.date.as_str() <= to && from.is_none_or(|f| e.date.as_str() >= f)) {
            for l in &e.lines {
                *m.entry(l.account_id.clone()).or_default() += l.debit - l.credit;
            }
        }
        m
    }
}

fn natural(kind: AccountKind, net_debit: i64) -> i64 {
    if kind.debit_normal() { net_debit } else { -net_debit }
}

fn row(a: &Account, amount: i64) -> Value {
    json!({ "account_id": a.id, "code": a.code, "name": a.name, "amount": amount })
}

fn section(books: &Books, kind: AccountKind, net: &HashMap<String, i64>) -> (Vec<Value>, i64) {
    let mut rows = Vec::new();
    let mut total = 0;
    for a in books.accounts.iter().filter(|a| a.kind == kind) {
        let v = natural(kind, *net.get(&a.id).unwrap_or(&0));
        if v != 0 {
            rows.push(row(a, v));
            total += v;
        }
    }
    (rows, total)
}

fn day_before(d: &str) -> String {
    (parse_date(d).unwrap() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string()
}

impl Ledger {
    async fn books(&self) -> LResult<Books> {
        Ok(Books { company: self.company().await?, accounts: self.accounts().await?, entries: self.entries().await? })
    }

    pub async fn trial_balance(&self, as_of: &str) -> LResult<Value> {
        parse_date(as_of)?;
        let b = self.books().await?;
        let net = b.net(None, as_of);
        let (mut td, mut tc) = (0, 0);
        let mut rows = Vec::new();
        for a in &b.accounts {
            let v = *net.get(&a.id).unwrap_or(&0);
            if v == 0 {
                continue;
            }
            let (d, c) = if v > 0 { (v, 0) } else { (0, -v) };
            td += d;
            tc += c;
            rows.push(json!({ "account_id": a.id, "code": a.code, "name": a.name, "kind": a.kind, "debit": d, "credit": c }));
        }
        Ok(json!({ "as_of": as_of, "rows": rows, "total_debit": td, "total_credit": tc, "balanced": td == tc }))
    }

    pub async fn profit_loss(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        let b = self.books().await?;
        let net = b.net(Some(from), to);
        let (income, ti) = section(&b, AccountKind::Income, &net);
        let cogs_ids: Vec<&str> = b.accounts.iter().filter(|a| a.kind == AccountKind::Expense && a.code.starts_with('5')).map(|a| a.id.as_str()).collect();
        let (expenses, te) = section(&b, AccountKind::Expense, &net);
        let (cogs, opex): (Vec<Value>, Vec<Value>) =
            expenses.into_iter().partition(|r| cogs_ids.contains(&r["account_id"].as_str().unwrap_or("")));
        let tcogs: i64 = cogs.iter().map(|r| r["amount"].as_i64().unwrap_or(0)).sum();
        Ok(json!({
            "from": from, "to": to,
            "income": income, "total_income": ti,
            "cogs": cogs, "total_cogs": tcogs,
            "gross_profit": ti - tcogs,
            "expenses": opex, "total_expenses": te - tcogs,
            "net_income": ti - te,
        }))
    }

    pub async fn balance_sheet(&self, as_of: &str) -> LResult<Value> {
        let d = parse_date(as_of)?;
        let b = self.books().await?;
        let fy = Ledger::fiscal_year_start(&b.company, d).format("%Y-%m-%d").to_string();
        let net = b.net(None, as_of);
        let (assets, ta) = section(&b, AccountKind::Asset, &net);
        let (liabs, tl) = section(&b, AccountKind::Liability, &net);
        let (mut equity, mut teq) = section(&b, AccountKind::Equity, &net);
        // Earnings: prior fiscal years roll into retained earnings; this year shows as net income.
        let pl = |n: &HashMap<String, i64>| -> i64 {
            b.accounts
                .iter()
                .filter(|a| matches!(a.kind, AccountKind::Income | AccountKind::Expense))
                .map(|a| -n.get(&a.id).copied().unwrap_or(0))
                .sum()
        };
        let prior = pl(&b.net(None, &day_before(&fy)));
        let current = pl(&b.net(Some(&fy), as_of));
        if prior != 0 {
            equity.push(json!({ "account_id": null, "code": "", "name": "Retained earnings (prior years)", "amount": prior }));
        }
        equity.push(json!({ "account_id": null, "code": "", "name": "Net income (this fiscal year)", "amount": current }));
        teq += prior + current;
        Ok(json!({
            "as_of": as_of, "fiscal_year_start": fy,
            "assets": assets, "total_assets": ta,
            "liabilities": liabs, "total_liabilities": tl,
            "equity": equity, "total_equity": teq,
            "total_liabilities_and_equity": tl + teq,
            "balanced": ta == tl + teq,
        }))
    }

    pub async fn aging(&self, kind: DocKind, as_of: &str) -> LResult<Value> {
        let d = parse_date(as_of)?;
        let contacts: HashMap<String, String> = self.contacts(None).await?.into_iter().map(|c| (c.id, c.name)).collect();
        const BUCKETS: [&str; 5] = ["current", "1_30", "31_60", "61_90", "over_90"];
        let mut per: BTreeMap<String, [i64; 5]> = BTreeMap::new();
        let mut docs = Vec::new();
        for doc in self.docs(Some(kind)).await?.into_iter().filter(|x| x.balance() > 0 && x.date.as_str() <= as_of) {
            let days = (d - parse_date(&doc.due_date)?).num_days();
            let i = match days {
                i64::MIN..=0 => 0,
                1..=30 => 1,
                31..=60 => 2,
                61..=90 => 3,
                _ => 4,
            };
            let name = contacts.get(&doc.contact_id).cloned().unwrap_or_default();
            per.entry(name.clone()).or_insert([0; 5])[i] += doc.balance();
            docs.push(json!({ "id": doc.id, "number": doc.number, "contact": name, "due_date": doc.due_date, "days_overdue": days.max(0), "balance": doc.balance(), "bucket": BUCKETS[i] }));
        }
        let mut totals = [0i64; 5];
        let rows: Vec<Value> = per
            .into_iter()
            .map(|(name, b)| {
                for i in 0..5 {
                    totals[i] += b[i];
                }
                let mut o = json!({ "contact": name, "total": b.iter().sum::<i64>() });
                for (i, k) in BUCKETS.iter().enumerate() {
                    o[*k] = json!(b[i]);
                }
                o
            })
            .collect();
        let mut t = json!({ "total": totals.iter().sum::<i64>() });
        for (i, k) in BUCKETS.iter().enumerate() {
            t[*k] = json!(totals[i]);
        }
        Ok(json!({ "as_of": as_of, "rows": rows, "totals": t, "documents": docs }))
    }

    /// GST/HST return working paper (CRA lines 101, 105, 108, 109).
    pub async fn sales_tax(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        let b = self.books().await?;
        let ids = b.by_id();
        let (mut sales, mut collected, mut itc) = (0i64, 0i64, 0i64);
        for e in b.entries.iter().filter(|e| !e.voided && e.date.as_str() >= from && e.date.as_str() <= to) {
            for l in &e.lines {
                let Some(a) = ids.get(l.account_id.as_str()) else { continue };
                match (a.kind, a.role) {
                    (AccountKind::Income, _) => sales += l.credit - l.debit,
                    // Remittances and refunds settle the balance; they aren't tax activity.
                    (_, AccountRole::TaxPayable) if e.source != "tax_payment" => collected += l.credit - l.debit,
                    (_, AccountRole::TaxRecoverable) if e.source != "tax_payment" => itc += l.debit - l.credit,
                    _ => {}
                }
            }
        }
        Ok(json!({
            "from": from, "to": to,
            "line_101_sales": sales,
            "line_105_tax_collected": collected,
            "line_108_itcs": itc,
            "line_109_net_tax": collected - itc,
            "note": "Working paper only. Verify against your records before filing with the CRA.",
        }))
    }

    pub async fn general_ledger(&self, account_id: Option<&str>, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        let b = self.books().await?;
        let opening_net = b.net(None, &day_before(from));
        let mut out = Vec::new();
        for a in b.accounts.iter().filter(|a| account_id.is_none_or(|id| a.id == id)) {
            let mut bal = natural(a.kind, *opening_net.get(&a.id).unwrap_or(&0));
            let opening = bal;
            let mut lines = Vec::new();
            for e in b.entries.iter().filter(|e| !e.voided && e.date.as_str() >= from && e.date.as_str() <= to) {
                for l in e.lines.iter().filter(|l| l.account_id == a.id) {
                    bal += natural(a.kind, l.debit - l.credit);
                    lines.push(json!({ "entry_id": e.id, "number": e.number, "date": e.date, "source": e.source, "memo": if l.memo.is_empty() { e.memo.clone() } else { l.memo.clone() }, "debit": l.debit, "credit": l.credit, "balance": bal }));
                }
            }
            if !lines.is_empty() || opening != 0 {
                out.push(json!({ "account_id": a.id, "code": a.code, "name": a.name, "opening": opening, "closing": bal, "lines": lines }));
            }
        }
        Ok(json!({ "from": from, "to": to, "accounts": out }))
    }

    pub async fn dashboard(&self) -> LResult<Value> {
        let t = today();
        let d = parse_date(&t)?;
        let b = self.books().await?;
        let net = b.net(None, &t);
        let bal = |role: AccountRole| -> i64 {
            b.accounts.iter().filter(|a| a.role == role).map(|a| natural(a.kind, *net.get(&a.id).unwrap_or(&0))).sum()
        };
        let cash = bal(AccountRole::Bank);
        let card = bal(AccountRole::CreditCard);
        let mut months = Vec::new();
        for i in (0..6).rev() {
            let first = shift_month(NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap(), -i);
            let last = shift_month(first, 1) - chrono::Duration::days(1);
            let n = b.net(Some(&first.format("%Y-%m-%d").to_string()), &last.format("%Y-%m-%d").to_string());
            let (_, inc) = section(&b, AccountKind::Income, &n);
            let (_, exp) = section(&b, AccountKind::Expense, &n);
            months.push(json!({ "month": first.format("%Y-%m").to_string(), "income": inc, "expenses": exp, "net": inc - exp }));
        }
        let fy = Ledger::fiscal_year_start(&b.company, d).format("%Y-%m-%d").to_string();
        let ytd = b.net(Some(&fy), &t);
        let (_, yi) = section(&b, AccountKind::Income, &ytd);
        let (_, ye) = section(&b, AccountKind::Expense, &ytd);
        let invoices = self.docs(Some(DocKind::Invoice)).await?;
        let bills = self.docs(Some(DocKind::Bill)).await?;
        let overdue_inv: Vec<&Doc> = invoices.iter().filter(|x| x.status(&t) == "overdue").collect();
        let overdue_bills = bills.iter().filter(|x| x.status(&t) == "overdue").count();
        let pending_feed = self.feed().await?.iter().filter(|f| f["item"]["status"] == "pending").count();
        let tax = b
            .accounts
            .iter()
            .filter(|a| matches!(a.role, AccountRole::TaxPayable | AccountRole::TaxRecoverable))
            .map(|a| -net.get(&a.id).copied().unwrap_or(0))
            .sum::<i64>();
        Ok(json!({
            "today": t,
            "company": b.company.name,
            "cash": cash,
            "credit_card_owing": card,
            "receivable": invoices.iter().map(Doc::balance).sum::<i64>(),
            "payable": bills.iter().map(Doc::balance).sum::<i64>(),
            "overdue_invoices": overdue_inv.len(),
            "overdue_receivable": overdue_inv.iter().map(|x| x.balance()).sum::<i64>(),
            "overdue_bills": overdue_bills,
            "gst_hst_owing": tax,
            "pending_bank_transactions": pending_feed,
            "ytd_income": yi, "ytd_expenses": ye, "ytd_net": yi - ye, "fiscal_year_start": fy,
            "months": months,
        }))
    }
}

fn shift_month(d: NaiveDate, by: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month() as i32 - 1 + by;
    NaiveDate::from_ymd_opt(total.div_euclid(12), (total.rem_euclid(12) + 1) as u32, 1).unwrap()
}

// ---- REA 2026-11-05: QuickBooks report parity (cash flow, comparisons, balances) ----

/// Statement of Cash Flows section for a balance-sheet account (indirect method).
/// Bank-role accounts are cash. Fixed assets (codes 15xx-19xx, except accumulated
/// depreciation, which is a non-cash add-back) are investing; long-term liabilities
/// (25xx-29xx) and equity are financing; everything else is operating.
fn cash_flow_section(a: &Account) -> &'static str {
    let fixed = a.kind == AccountKind::Asset && a.code.len() >= 2 && ("15".."2").contains(&&a.code[..2]);
    let long_term = a.kind == AccountKind::Liability && a.code.len() >= 2 && ("25".."3").contains(&&a.code[..2]);
    if a.role == AccountRole::Bank {
        "cash"
    } else if fixed && !a.name.to_lowercase().contains("accumulated") {
        "investing"
    } else if long_term || a.kind == AccountKind::Equity {
        "financing"
    } else {
        "operating"
    }
}

fn add_months(d: NaiveDate, n: u32) -> NaiveDate {
    let m0 = d.month0() + n;
    NaiveDate::from_ymd_opt(d.year() + (m0 / 12) as i32, m0 % 12 + 1, 1).unwrap()
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl Ledger {
    /// Statement of Cash Flows (indirect method), QBO "Statement of Cash Flows".
    pub async fn cash_flow(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        let b = self.books().await?;
        let net = b.net(Some(from), to);
        let net_income: i64 = b
            .accounts
            .iter()
            .filter(|a| matches!(a.kind, AccountKind::Income | AccountKind::Expense))
            .map(|a| -net.get(&a.id).copied().unwrap_or(0))
            .sum();
        let mut sec: BTreeMap<&str, (Vec<Value>, i64)> = BTreeMap::new();
        let mut cash_change = 0;
        for a in b.accounts.iter().filter(|a| matches!(a.kind, AccountKind::Asset | AccountKind::Liability | AccountKind::Equity)) {
            let d = net.get(&a.id).copied().unwrap_or(0);
            if d == 0 {
                continue;
            }
            let s = cash_flow_section(a);
            if s == "cash" {
                cash_change += d;
                continue;
            }
            // A debit increase in a non-cash account uses cash; a credit increase provides it.
            let e = sec.entry(s).or_default();
            e.0.push(row(a, -d));
            e.1 += -d;
        }
        let take = |s: &str, sec: &BTreeMap<&str, (Vec<Value>, i64)>| sec.get(s).cloned().unwrap_or_default();
        let (op, top) = take("operating", &sec);
        let (inv, tinv) = take("investing", &sec);
        let (fin, tfin) = take("financing", &sec);
        let opening = b.net(None, &day_before(from));
        let cash_ids: Vec<&str> = b.accounts.iter().filter(|a| a.role == AccountRole::Bank).map(|a| a.id.as_str()).collect();
        let cash_begin: i64 = cash_ids.iter().map(|id| opening.get(*id).copied().unwrap_or(0)).sum();
        let operating = net_income + top;
        Ok(json!({
            "from": from, "to": to,
            "net_income": net_income,
            "operating_adjustments": op, "net_cash_operating": operating,
            "investing": inv, "net_cash_investing": tinv,
            "financing": fin, "net_cash_financing": tfin,
            "net_change_in_cash": operating + tinv + tfin,
            "cash_beginning": cash_begin,
            "cash_ending": cash_begin + cash_change,
            "reconciles": operating + tinv + tfin == cash_change,
        }))
    }

    /// Profit and Loss by Month: one column per calendar month in [from, to].
    pub async fn profit_loss_by_month(&self, from: &str, to: &str) -> LResult<Value> {
        let f = parse_date(from)?;
        let t = parse_date(to)?;
        if t < f {
            return Err(super::engine::LedgerError::BadRequest("'to' is before 'from'".into()));
        }
        let mut cols = Vec::new();
        let mut start = f;
        while start <= t && cols.len() < 120 {
            let next = add_months(start, 1);
            let end = (next - chrono::Duration::days(1)).min(t);
            let pl = self.profit_loss(&ymd(start), &ymd(end)).await?;
            cols.push(json!({ "label": start.format("%b %Y").to_string(), "from": ymd(start), "to": ymd(end),
                "total_income": pl["total_income"], "total_cogs": pl["total_cogs"], "gross_profit": pl["gross_profit"],
                "total_expenses": pl["total_expenses"], "net_income": pl["net_income"], "income": pl["income"], "expenses": pl["expenses"], "cogs": pl["cogs"] }));
            start = next;
        }
        let total = self.profit_loss(from, to).await?;
        Ok(json!({ "from": from, "to": to, "columns": cols, "total": total }))
    }

    /// Profit and Loss Comparison: the range against the same range one year earlier.
    pub async fn profit_loss_comparison(&self, from: &str, to: &str) -> LResult<Value> {
        let f = parse_date(from)?;
        let t = parse_date(to)?;
        let back = |d: NaiveDate| d.with_year(d.year() - 1).unwrap_or_else(|| d - chrono::Duration::days(365));
        let (pf, pt) = (ymd(back(f)), ymd(back(t)));
        let cur = self.profit_loss(from, to).await?;
        let prior = self.profit_loss(&pf, &pt).await?;
        let mut deltas = json!({});
        for k in ["total_income", "total_cogs", "gross_profit", "total_expenses", "net_income"] {
            let (c, p) = (cur[k].as_i64().unwrap_or(0), prior[k].as_i64().unwrap_or(0));
            let pct = if p == 0 { Value::Null } else { json!(((c - p) as f64 / p.abs() as f64 * 10000.0).round() / 100.0) };
            deltas[k] = json!({ "current": c, "prior": p, "change": c - p, "change_pct": pct });
        }
        Ok(json!({ "current": cur, "prior": prior, "prior_from": pf, "prior_to": pt, "comparison": deltas }))
    }

    /// Customer / Vendor Balance Summary: open document balance per contact.
    pub async fn contact_balances(&self, kind: DocKind, as_of: &str) -> LResult<Value> {
        parse_date(as_of)?;
        let contacts: HashMap<String, String> = self.contacts(None).await?.into_iter().map(|c| (c.id, c.name)).collect();
        let mut per: BTreeMap<String, (i64, usize)> = BTreeMap::new();
        for doc in self.docs(Some(kind)).await?.into_iter().filter(|x| x.balance() > 0 && x.date.as_str() <= as_of) {
            let name = contacts.get(&doc.contact_id).cloned().unwrap_or_default();
            let e = per.entry(name).or_default();
            e.0 += doc.balance();
            e.1 += 1;
        }
        let total: i64 = per.values().map(|v| v.0).sum();
        let rows: Vec<Value> = per.into_iter().map(|(n, (b, c))| json!({ "contact": n, "balance": b, "open_documents": c })).collect();
        Ok(json!({ "as_of": as_of, "rows": rows, "total": total }))
    }
}
