//! Read-only adapters over the existing NXT Ledger reporting engine.
use super::{FResult, Firm, bad, date, id, models::*, now};
use chrono::{Datelike, Utc};
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn validate_range(report: &str, from: Option<String>, to: Option<String>) -> FResult<(String, String)> {
    if !["trial-balance", "profit-loss", "balance-sheet", "general-ledger", "journal"].contains(&report) {
        return Err(bad("report must be trial-balance, profit-loss, balance-sheet, general-ledger, or journal"));
    }
    let to = to.unwrap_or_else(|| Utc::now().date_naive().format("%Y-%m-%d").to_string());
    let end = date(&to)?;
    let from = from.unwrap_or_else(|| format!("{}-01-01", end.year()));
    date(&from)?;
    if from > to { return Err(bad("report from must not follow to")); }
    Ok((from, to))
}

/// RFC4180 escaping plus protection against formulas in downloaded text cells.
fn cell(value: &str) -> String {
    let leading = value.trim_start_matches(char::is_whitespace);
    let dangerous = value.starts_with(['\t', '\r', '\n']) || leading.starts_with(['=', '+', '@']) || (leading.starts_with('-') && leading.parse::<i64>().is_err());
    let value = if dangerous { format!("'{value}") } else { value.to_string() };
    if value.contains([',', '"', '\r', '\n']) { format!("\"{}\"", value.replace('"', "\"\"")) } else {value}
}

fn row(output: &mut String, fields: impl IntoIterator<Item = String>) {
    output.push_str(&fields.into_iter().map(|f| cell(&f)).collect::<Vec<_>>().join(","));
    output.push_str("\r\n");
}
fn s(v: &Value, name: &str) -> String { v[name].as_str().unwrap_or("").to_string() }
fn n(v: &Value, name: &str) -> String { v[name].as_i64().unwrap_or(0).to_string() }
fn rows<'a>(v: &'a Value, name: &str) -> impl Iterator<Item=&'a Value> { v[name].as_array().into_iter().flatten() }

impl Firm {
    pub(super) async fn prepare_export(&self, input: ExportInput, export_id: Option<String>) -> FResult<StoredExport> {
        let (from, to) = validate_range(&input.report, input.from, input.to)?;
        let mut csv = String::new(); let mut count = 0;
        match input.report.as_str() {
            "trial-balance" => {
                let data = self.ledger.trial_balance(&to).await?;
                row(&mut csv, ["account_code", "account_name", "kind", "debit_cents", "credit_cents"].map(str::to_string));
                for r in rows(&data, "rows") { row(&mut csv, [s(r,"code"),s(r,"name"),s(r,"kind"),n(r,"debit"),n(r,"credit")]); count += 1; }
                row(&mut csv, ["TOTAL".into(), String::new(), String::new(), n(&data,"total_debit"),n(&data,"total_credit")]);
            },
            "profit-loss" | "balance-sheet" => {
                let data = if input.report == "profit-loss" {self.ledger.profit_loss(&from, &to).await?} else {self.ledger.balance_sheet(&to).await?};
                row(&mut csv, ["section", "account_code", "account_name", "amount_cents"].map(str::to_string));
                let sections: &[(&str, &str)] = if input.report == "profit-loss" {&[("income","total_income"),("cogs","total_cogs"),("expenses","total_expenses")]} else {&[("assets","total_assets"),("liabilities","total_liabilities"),("equity","total_equity")]};
                for (section, total) in sections {
                    for r in rows(&data, section) { row(&mut csv, [section.to_string(),s(r,"code"),s(r,"name"),n(r,"amount")]); count += 1; }
                    row(&mut csv, ["TOTAL".into(),String::new(),section.to_string(),n(&data,total)]);
                }
                for total in if input.report == "profit-loss" {vec!["gross_profit","net_income"]} else {vec!["total_liabilities_and_equity"]} { row(&mut csv, ["TOTAL".into(),String::new(),total.into(),n(&data,total)]); }
            },
            "general-ledger" => {
                let data = self.ledger.general_ledger(None, &from, &to).await?;
                row(&mut csv, ["account_code","account_name","date","entry_number","source","memo","debit_cents","credit_cents","balance_cents"].map(str::to_string));
                for a in rows(&data,"accounts") {
                    row(&mut csv, [s(a,"code"),s(a,"name"),from.clone(),String::new(),"opening".into(),String::new(),String::new(),String::new(),n(a,"opening")]);
                    for r in rows(a,"lines") { row(&mut csv, [s(a,"code"),s(a,"name"),s(r,"date"),n(r,"number"),s(r,"source"),s(r,"memo"),n(r,"debit"),n(r,"credit"),n(r,"balance")]); count += 1; }
                    row(&mut csv, [s(a,"code"),s(a,"name"),to.clone(),String::new(),"closing".into(),String::new(),String::new(),String::new(),n(a,"closing")]);
                }
            },
            "journal" => {
                let account_list = self.ledger.accounts().await?;
                let accounts: HashMap<_,_> = account_list.iter().map(|a| (a.id.as_str(),a)).collect();
                let mut entries = self.ledger.entries().await?;
                entries.sort_by(|a,b| (&a.date,a.number,&a.id).cmp(&(&b.date,b.number,&b.id)));
                row(&mut csv, ["date","entry_number","entry_id","source","memo","account_code","account_name","debit_cents","credit_cents"].map(str::to_string));
                let (mut debit,mut credit) = (0i64,0i64);
                for e in entries.iter().filter(|e| !e.voided && e.date >= from && e.date <= to) {
                    for line in &e.lines {
                        let account = accounts.get(line.account_id.as_str());
                        row(&mut csv, [e.date.clone(),e.number.to_string(),e.id.clone(),e.source.clone(),if line.memo.is_empty() {e.memo.clone()} else {line.memo.clone()},account.map_or(String::new(),|a| a.code.clone()),account.map_or_else(|| line.account_id.clone(),|a| a.name.clone()),line.debit.to_string(),line.credit.to_string()]);
                        debit = debit.checked_add(line.debit).ok_or_else(|| bad("report total overflow"))?; credit = credit.checked_add(line.credit).ok_or_else(|| bad("report total overflow"))?; count += 1;
                    }
                }
                row(&mut csv, ["TOTAL".into(),String::new(),String::new(),String::new(),String::new(),String::new(),String::new(),debit.to_string(),credit.to_string()]);
            },
            _ => unreachable!(),
        }
        if count > 10000 || csv.len() > 1024 * 1024 { return Err(bad("report exceeds 10,000 rows or 1 MiB; narrow the report period")); }
        let export_id = export_id.unwrap_or_else(|| id("export"));
        Ok(StoredExport { metadata: ExportJob { id: export_id.clone(), report: input.report, from, to, status: "ready".into(), created_at: now(), download_url: format!("/api/firm/exports/{export_id}/download"), source: "nxt_ledger".into(), row_count: count }, csv })
    }

    pub async fn spreadsheet_csv(&self, input: ExportInput) -> FResult<String> {
        Ok(self.prepare_export(input, None).await?.csv)
    }
}
