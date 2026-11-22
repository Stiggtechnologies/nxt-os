//! Financial reports computed from posted (non-void) entries.

use super::engine::{LResult, Ledger, parse_date, today};
use super::models::*;
use chrono::{Datelike, NaiveDate};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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
    /// Cash moving through bank accounts, by month. Credit-card movements are excluded and
    /// the caller is told so rather than seeing a blended figure.
    pub async fn cash_flow(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        let b = self.books().await?;
        let banks: Vec<&Account> = b.accounts.iter().filter(|a| a.role == AccountRole::Bank).collect();
        let is_bank = |id: &str| banks.iter().any(|a| a.id == id);
        let mut months: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        let (mut total_in, mut total_out) = (0_i64, 0_i64);
        for entry in b.entries.iter().filter(|e| !e.voided && e.date.as_str() >= from && e.date.as_str() <= to) {
            let month = entry.date.chars().take(7).collect::<String>();
            for line in entry.lines.iter().filter(|l| is_bank(&l.account_id)) {
                let slot = months.entry(month.clone()).or_insert((0, 0));
                if line.debit > 0 { slot.0 += line.debit; total_in += line.debit; }
                if line.credit > 0 { slot.1 += line.credit; total_out += line.credit; }
            }
        }
        let opening_net = b.net(None, &day_before(from));
        let opening: i64 = banks.iter().map(|a| natural(a.kind, *opening_net.get(&a.id).unwrap_or(&0))).sum();
        let balances: Vec<Value> = banks.iter().map(|a| {
            let closing_net = b.net(None, to);
            row(a, natural(a.kind, *closing_net.get(&a.id).unwrap_or(&0)))
        }).collect();
        let rows: Vec<Value> = months.iter().map(|(month, (inflow, outflow))| json!({
            "month": month, "in_cents": inflow, "out_cents": outflow, "net_cents": inflow - outflow
        })).collect();
        let closing = opening + total_in - total_out;
        Ok(json!({
            "from": from, "to": to, "basis": "debits and credits on bank-role accounts",
            "months": rows,
            "accounts": balances,
            "opening_balance_cents": opening,
            "total_in_cents": total_in,
            "total_out_cents": total_out,
            "net_cents": total_in - total_out,
            "closing_balance_cents": closing,
            "excludes": ["credit card accounts", "unposted or voided entries"]
        }))
    }


    async fn books(&self) -> LResult<Books> {
        // Reports are reads: never seed the chart from here.
        Ok(Books { company: self.company().await?, accounts: self.accounts_read().await?, entries: self.entries().await? })
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

// ---- REA 2026-11-05: QuickBooks report parity (monthly/comparison P&L, balances) ----

fn add_months(d: NaiveDate, n: u32) -> NaiveDate {
    let m0 = d.month0() + n;
    NaiveDate::from_ymd_opt(d.year() + (m0 / 12) as i32, m0 % 12 + 1, 1).unwrap()
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl Ledger {
    /// Budget versus actual over [from, to]. Budget cells are monthly; actuals are the
    /// account's natural movement from the committed journal for the same window.
    pub async fn budget_vs_actual(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        if to < from {
            return Err(super::engine::LedgerError::BadRequest("'to' is before 'from'".into()));
        }
        let books = self.books().await?;
        let net = books.net(Some(from), to);
        let from_month = &from[..7.min(from.len())];
        let to_month = &to[..7.min(to.len())];
        let mut budgeted: BTreeMap<String, i64> = BTreeMap::new();
        for cell in self.budgets().await? {
            if cell.month.as_str() >= from_month && cell.month.as_str() <= to_month {
                *budgeted.entry(cell.account_id).or_insert(0) += cell.amount;
            }
        }
        let mut ids: BTreeSet<String> = budgeted.keys().cloned().collect();
        // Budget-vs-actual is a profit-and-loss view: balance-sheet movement only
        // appears when the operator explicitly budgeted that account.
        for account in &books.accounts {
            if matches!(account.kind, AccountKind::Income | AccountKind::Expense)
                && *net.get(&account.id).unwrap_or(&0) != 0
            {
                ids.insert(account.id.clone());
            }
        }
        let accounts: HashMap<&str, &Account> = books.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
        let mut rows = Vec::new();
        let (mut total_budget, mut total_actual) = (0_i64, 0_i64);
        for id in ids {
            let Some(account) = accounts.get(id.as_str()) else { continue };
            let budget = *budgeted.get(&id).unwrap_or(&0);
            let raw = *net.get(&id).unwrap_or(&0);
            let actual = if account.kind.debit_normal() { raw } else { -raw };
            let variance = actual - budget;
            let variance_pct = if budget == 0 { Value::Null } else { json!(((variance as f64 / budget as f64) * 10000.0).round() / 100.0) };
            total_budget += budget;
            total_actual += actual;
            rows.push(json!({
                "account_id": id, "code": account.code, "name": account.name, "kind": account.kind,
                "budget_cents": budget, "actual_cents": actual,
                "variance_cents": variance, "variance_pct": variance_pct,
            }));
        }
        rows.sort_by(|a, b| a["code"].as_str().cmp(&b["code"].as_str()));
        let totals = json!({
            "budget_cents": total_budget, "actual_cents": total_actual,
            "variance_cents": total_actual - total_budget,
        });
        Ok(json!({ "from": from, "to": to, "rows": rows, "totals": totals,
            "basis": "monthly budget cells versus the committed journal's natural movement" }))
    }

    /// Revenue by customer and item (invoice line description). Credit memos reduce
    /// the same customer/item row, so the report follows the books rather than
    /// counting issuances only.
    pub async fn item_profitability(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        if to < from {
            return Err(super::engine::LedgerError::BadRequest("'to' is before 'from'".into()));
        }
        let contacts: HashMap<String, String> = self.contacts(None).await?.into_iter().map(|c| (c.id, c.name)).collect();
        let mut rows: BTreeMap<(String, String), (f64, i64, usize)> = BTreeMap::new();
        for kind in [DocKind::Invoice, DocKind::CreditMemo] {
            let sign: f64 = if kind == DocKind::CreditMemo { -1.0 } else { 1.0 };
            for doc in self.docs(Some(kind)).await? {
                if doc.voided || doc.date.as_str() < from || doc.date.as_str() > to { continue; }
                for line in &doc.lines {
                    let name = contacts.get(&doc.contact_id).cloned().unwrap_or_default();
                    let row = rows.entry((name, line.description.clone())).or_insert((0.0, 0, 0));
                    row.0 += sign * line.qty;
                    row.1 += (sign * line.amount as f64).round() as i64;
                    row.2 += 1;
                }
            }
        }
        let mut out: Vec<Value> = rows
            .into_iter()
            .filter(|(_, (qty, cents, _))| *qty != 0.0 || *cents != 0)
            .map(|((customer, item), (qty, cents, count))| json!({
                "customer": customer, "item": item, "quantity": qty,
                "revenue_cents": cents, "documents": count,
            }))
            .collect();
        out.sort_by(|a, b| b["revenue_cents"].as_i64().cmp(&a["revenue_cents"].as_i64()));
        let total: i64 = out.iter().map(|row| row["revenue_cents"].as_i64().unwrap_or(0)).sum();
        Ok(json!({
            "from": from, "to": to, "rows": out, "total_cents": total,
            "note": "Revenue before tax, grouped by customer and invoice line description. Credit memos subtract from the same row; costs are not allocated, so this is not a gross-margin claim.",
        }))
    }

    /// Profit and loss grouped by class/department tag. Unassigned postings appear
    /// in their own row so the totals always reconcile to the P&L.
    pub async fn profit_loss_by_class(&self, from: &str, to: &str) -> LResult<Value> {
        parse_date(from)?;
        parse_date(to)?;
        if to < from {
            return Err(super::engine::LedgerError::BadRequest("'to' is before 'from'".into()));
        }
        let b = self.books().await?;
        let accounts: HashMap<&str, &Account> = b.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
        let mut buckets: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        for entry in b.entries.iter().filter(|e| !e.voided && e.date.as_str() >= from && e.date.as_str() <= to) {
            for line in &entry.lines {
                let Some(account) = accounts.get(line.account_id.as_str()) else { continue };
                let key = line.class_id.clone().unwrap_or_default();
                let slot = buckets.entry(key).or_insert((0, 0));
                match account.kind {
                    AccountKind::Income => slot.0 += line.credit - line.debit,
                    AccountKind::Expense => slot.1 += line.debit - line.credit,
                    _ => {}
                }
            }
        }
        let classes = self.classes().await?;
        let name_of = |id: &str| -> String {
            if id.is_empty() { return "Unassigned".into(); }
            classes.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_else(|| format!("Unknown class {id}"))
        };
        let mut rows = Vec::new();
        let (mut total_income, mut total_expense) = (0_i64, 0_i64);
        for (id, (income, expense)) in &buckets {
            if *income == 0 && *expense == 0 { continue; }
            total_income += income;
            total_expense += expense;
            rows.push(json!({
                "class_id": id, "name": name_of(id),
                "income_cents": income, "expense_cents": expense, "net_cents": income - expense,
            }));
        }
        rows.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(json!({
            "from": from, "to": to, "rows": rows,
            "totals": { "income_cents": total_income, "expense_cents": total_expense, "net_cents": total_income - total_expense },
            "note": "Income and expense postings only; balance-sheet lines carry their class but are not part of this report.",
        }))
    }

    /// Cash forecast: open invoices expected in, open bills expected out, and any
    /// scheduled recurring entry that touches a bank account, bucketed by week.
    /// This is a projection over records that already exist; it invents nothing.
    pub async fn forecast(&self, from: &str, to: &str) -> LResult<Value> {
        let f = parse_date(from)?;
        let t = parse_date(to)?;
        if t < f {
            return Err(super::engine::LedgerError::BadRequest("'to' is before 'from'".into()));
        }
        if (t - f).num_days() > 366 {
            return Err(super::engine::LedgerError::BadRequest("select a forecast window of a year or less".into()));
        }
        let b = self.books().await?;
        let banks: Vec<&Account> = b.accounts.iter().filter(|a| a.role == AccountRole::Bank).collect();
        let is_bank = |id: &str| banks.iter().any(|a| a.id == id);
        let opening_net = b.net(None, &day_before(from));
        let opening: i64 = banks.iter().map(|a| natural(a.kind, *opening_net.get(&a.id).unwrap_or(&0))).sum();
        let weeks = (((t - f).num_days() + 7) / 7).max(1) as usize;
        let mut buckets: Vec<(i64, i64)> = vec![(0, 0); weeks];
        let slot = |date: &str| -> usize {
            let days = (parse_date(date).unwrap() - f).num_days();
            if days < 0 { 0 } else { (days / 7).min(weeks as i64 - 1) as usize }
        };
        let mut invoices = 0;
        for doc in self.docs(Some(DocKind::Invoice)).await? {
            let bal = doc.balance();
            if bal <= 0 || doc.voided || doc.date.as_str() > to || doc.due_date.as_str() > to { continue; }
            let i = slot(&doc.due_date);
            buckets[i].0 += bal;
            invoices += 1;
        }
        let mut bills = 0;
        for doc in self.docs(Some(DocKind::Bill)).await? {
            let bal = doc.balance();
            if bal <= 0 || doc.voided || doc.date.as_str() > to || doc.due_date.as_str() > to { continue; }
            let i = slot(&doc.due_date);
            buckets[i].1 += bal;
            bills += 1;
        }
        let mut recurring = 0;
        for template in self.recurring().await? {
            if template.next_date.as_str() < from || template.next_date.as_str() > to { continue; }
            let i = slot(&template.next_date);
            let mut touches_cash = false;
            for line in &template.lines {
                if !is_bank(&line.account_id) { continue; }
                touches_cash = true;
                buckets[i].0 += line.debit;
                buckets[i].1 += line.credit;
            }
            if touches_cash { recurring += 1; }
        }
        let mut rows = Vec::new();
        let mut cash = opening;
        for (i, (inflow, outflow)) in buckets.iter().enumerate() {
            let start = f + chrono::Duration::days(7 * i as i64);
            let end = (start + chrono::Duration::days(6)).min(t);
            cash += inflow - outflow;
            rows.push(json!({
                "start": start.format("%Y-%m-%d").to_string(),
                "end": end.format("%Y-%m-%d").to_string(),
                "expected_in_cents": inflow,
                "expected_out_cents": outflow,
                "net_cents": inflow - outflow,
                "projected_cash_cents": cash,
            }));
        }
        Ok(json!({
            "from": from, "to": to, "basis": "expected receipts, expected payments and scheduled recurring entries",
            "opening_cash_cents": opening,
            "buckets": rows,
            "closing_cash_cents": cash,
            "sources": { "open_invoices": invoices, "open_bills": bills, "recurring_entries": recurring },
            "excludes": ["new sales not yet invoiced", "payroll and tax remittances not recorded", "financing and asset purchases not recorded"],
            "note": "A projection over recorded documents, not a guarantee. Overdue documents are expected in the first week.",
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
