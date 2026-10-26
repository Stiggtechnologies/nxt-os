//! Explicit, reviewable copies and proposals between native NXT modules.
//! None of these helpers posts working adjustments to the source Ledger.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    EResult, analysis, bad, conflict, fresh, hash, models::Account,
    storage_models::EngagementRecord,
};
use crate::{
    firm::{
        Firm,
        models::{ProposalInput, WorkflowAction},
    },
    ledger::{
        engine::Ledger,
        models::{Account as LedgerAccount, AccountKind, Company, Entry},
    },
    tax::{
        Tax,
        corporate::{CorporateProfile, GifiRow},
        models::ReturnInput,
    },
};

const MAX_AMOUNT: i128 = 100_000_000_000_000;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize, PartialEq)]
struct LedgerRead {
    company: Option<String>,
    sequence: Option<String>,
    accounts: Vec<(String, String)>,
    entries: Vec<(String, String)>,
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

async fn indexed_ids(ledger: &Ledger, name: &str) -> EResult<Vec<String>> {
    let mut ids = ledger.store.smembers(&format!("gl:{name}")).await?;
    ids.sort();
    if ids.len() > 498
        || ids.iter().any(|id| !safe_id(id))
        || ids.windows(2).any(|ids| ids[0] == ids[1])
    {
        return Err(conflict(format!(
            "Ledger {name} index exceeds capture bounds or contains invalid IDs"
        )));
    }
    Ok(ids)
}

async fn read_ledger(ledger: &Ledger) -> EResult<LedgerRead> {
    let account_ids = indexed_ids(ledger, "accounts").await?;
    let entry_ids = indexed_ids(ledger, "entries").await?;
    let mut keys = vec!["gl:company".into(), "gl:seq:entry".into()];
    keys.extend(account_ids.iter().map(|id| format!("gl:accounts:{id}")));
    keys.extend(entry_ids.iter().map(|id| format!("gl:entries:{id}")));
    if keys.len() > 500 {
        return Err(conflict(
            "Ledger capture requires at most 500 total stored keys including company/sequence. Use a reviewed native trial balance for a larger source.",
        ));
    }
    // One MGET: one SQL statement snapshot for the native Supabase RPC, one
    // Redis MGET or one Memory lock. Index membership is checked on both sides.
    let values = ledger.store.mget(&keys).await?;
    if values.len() != keys.len() {
        return Err(conflict(
            "Ledger storage returned an incomplete capture read",
        ));
    }
    if account_ids != indexed_ids(ledger, "accounts").await?
        || entry_ids != indexed_ids(ledger, "entries").await?
    {
        return Err(conflict(
            "Ledger indexes changed during capture; stop source writes and retry",
        ));
    }
    let mut bytes = 0usize;
    for value in values.iter().flatten() {
        bytes = bytes
            .checked_add(value.len())
            .ok_or_else(|| conflict("Ledger capture size overflow"))?;
        if value.len() > 256 * 1024 || bytes > MAX_SOURCE_BYTES {
            return Err(conflict(
                "Ledger capture exceeds its 4 MiB source bound; use a reviewed native trial balance",
            ));
        }
    }
    let mut account_rows = Vec::with_capacity(account_ids.len());
    let mut entry_rows = Vec::with_capacity(entry_ids.len());
    for (id, value) in account_ids.iter().zip(values.iter().skip(2)) {
        account_rows.push((
            id.clone(),
            value
                .clone()
                .ok_or_else(|| conflict("Ledger account index references a missing record"))?,
        ));
    }
    for (id, value) in entry_ids
        .iter()
        .zip(values.iter().skip(2 + account_ids.len()))
    {
        entry_rows.push((
            id.clone(),
            value
                .clone()
                .ok_or_else(|| conflict("Ledger entry index references a missing record"))?,
        ));
    }
    let result = LedgerRead {
        company: values[0].clone(),
        sequence: values[1].clone(),
        accounts: account_rows,
        entries: entry_rows,
    };
    if result
        .company
        .as_ref()
        .is_some_and(|value| value.len() > 64 * 1024)
    {
        return Err(conflict(
            "Ledger company configuration exceeds capture bounds",
        ));
    }
    Ok(result)
}

fn canonical_date(value: &str) -> EResult<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| conflict("Ledger contains an invalid posting date"))?;
    if value.len() != 10
        || date.format("%Y-%m-%d").to_string() != value
        || !(1900..=2200).contains(&date.year())
    {
        return Err(conflict(
            "Bridge dates must be canonical dates from 1900 through 2200",
        ));
    }
    Ok(date)
}

fn amount(value: i128) -> EResult<i64> {
    if !(-MAX_AMOUNT..=MAX_AMOUNT).contains(&value) {
        return Err(conflict(
            "Bridge amount exceeds the native per-value integer-cent bound",
        ));
    }
    i64::try_from(value).map_err(|_| conflict("Bridge arithmetic exceeds integer-cent bounds"))
}

fn plus(value: &mut i128, addend: i128) -> EResult<()> {
    *value = value
        .checked_add(addend)
        .ok_or_else(|| conflict("Bridge arithmetic overflow"))?;
    Ok(())
}

fn ledger_kind(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset => "asset",
        AccountKind::Liability => "liability",
        AccountKind::Equity => "equity",
        AccountKind::Income => "revenue",
        AccountKind::Expense => "expense",
    }
}

fn ledger_accounts(source: &LedgerRead, record: &EngagementRecord) -> EResult<Vec<Account>> {
    analysis::validate(&record.document).map_err(bad)?;
    let company: Company = serde_json::from_str(source.company.as_deref().ok_or_else(|| {
        conflict("Explicitly configure the Ledger company and CAD currency before capture")
    })?)
    .map_err(|_| conflict("Ledger company configuration is malformed"))?;
    if company.currency != "CAD" || record.document.currency != "CAD" {
        return Err(conflict(
            "Ledger capture supports an explicitly configured CAD company only",
        ));
    }
    let mut accounts = BTreeMap::new();
    let mut codes = BTreeSet::new();
    for (id, raw) in &source.accounts {
        let account: LedgerAccount = serde_json::from_str(raw).map_err(|_| {
            conflict("Ledger contains a malformed account; no records were skipped")
        })?;
        if account.id != *id || !codes.insert(account.code.clone()) {
            return Err(conflict(
                "Ledger account identity/code is inconsistent or duplicated",
            ));
        }
        accounts.insert(id.as_str(), account);
    }
    if accounts.is_empty() {
        return Err(conflict(
            "Ledger has no explicitly stored accounts; capture does not seed a chart",
        ));
    }
    let sequence = match source.sequence.as_deref() {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| conflict("Ledger entry sequence is malformed"))?,
        None if source.entries.is_empty() => 0,
        None => return Err(conflict("Ledger entry sequence is missing")),
    };
    if sequence != source.entries.len() {
        return Err(conflict(
            "Ledger sequence and indexed postings differ; complete or recover source writes before capture",
        ));
    }
    let mut numbers = BTreeSet::new();
    let mut current: BTreeMap<&str, i128> = BTreeMap::new();
    let mut prior: BTreeMap<&str, i128> = BTreeMap::new();
    let mut historical_current: BTreeMap<&str, i128> = BTreeMap::new();
    let mut historical_prior: BTreeMap<&str, i128> = BTreeMap::new();
    let mut lines = 0usize;
    for (id, raw) in &source.entries {
        let entry: Entry = serde_json::from_str(raw).map_err(|_| {
            conflict("Ledger contains a malformed posting; no records were skipped")
        })?;
        if entry.id != *id
            || entry.number < 1
            || usize::try_from(entry.number)
                .ok()
                .is_none_or(|number| number > sequence)
            || !numbers.insert(entry.number)
        {
            return Err(conflict("Ledger entry identity/sequence is inconsistent"));
        }
        canonical_date(&entry.date)?;
        lines = lines
            .checked_add(entry.lines.len())
            .ok_or_else(|| conflict("Ledger capture line count overflow"))?;
        if entry.lines.len() < 2 || entry.lines.len() > 100 || lines > 50_000 {
            return Err(conflict(
                "Ledger capture supports 2..100 lines per posting and at most 50,000 source lines",
            ));
        }
        let mut debit = 0i128;
        let mut credit = 0i128;
        for line in &entry.lines {
            let (key, account) = accounts
                .get_key_value(line.account_id.as_str())
                .ok_or_else(|| conflict("Ledger posting references an unknown account"))?;
            if line.debit < 0
                || line.credit < 0
                || i128::from(line.debit) > MAX_AMOUNT
                || i128::from(line.credit) > MAX_AMOUNT
                || (line.debit > 0 && line.credit > 0)
                || (line.debit == 0 && line.credit == 0)
            {
                return Err(conflict(
                    "Ledger posting contains invalid one-sided integer-cent amounts",
                ));
            }
            plus(&mut debit, i128::from(line.debit))?;
            plus(&mut credit, i128::from(line.credit))?;
            if entry.voided {
                continue;
            }
            let net = i128::from(line.debit) - i128::from(line.credit);
            if entry.date <= record.document.period_end {
                plus(current.entry(*key).or_default(), net)?;
            }
            if entry.date < record.document.period_start
                && matches!(account.kind, AccountKind::Income | AccountKind::Expense)
            {
                plus(historical_current.entry(*key).or_default(), net)?;
            }
            if let Some(end) = &record.document.prior_period_end {
                if &entry.date <= end {
                    plus(prior.entry(*key).or_default(), net)?;
                }
            }
            if let Some(start) = &record.document.prior_period_start {
                if &entry.date < start
                    && matches!(account.kind, AccountKind::Income | AccountKind::Expense)
                {
                    plus(historical_prior.entry(*key).or_default(), net)?;
                }
            }
        }
        if debit != credit {
            return Err(conflict(
                "Ledger contains an unbalanced posting; capture was rejected",
            ));
        }
    }
    if historical_current
        .values()
        .chain(historical_prior.values())
        .any(|net| *net != 0)
    {
        return Err(conflict(
            "Historical income/expense balances are not closed for the requested periods. Complete controlled source closing or import a reviewed native trial balance; no retained-earnings adjustment is inferred.",
        ));
    }
    let mut result = Vec::with_capacity(accounts.len());
    for (id, account) in accounts {
        let net = current.get(id).copied().unwrap_or_default();
        let comparative = prior.get(id).copied().unwrap_or_default();
        result.push(Account {
            id: account.id,
            code: account.code,
            name: account.name,
            kind: ledger_kind(account.kind).into(),
            group_id: None,
            debit_cents: amount(net.max(0))?,
            credit_cents: amount((-net).max(0))?,
            prior_debit_cents: amount(comparative.max(0))?,
            prior_credit_cents: amount((-comparative).max(0))?,
        });
    }
    let mut test = record.document.clone();
    test.accounts = result.clone();
    test.adjustments.clear();
    test.workpapers.clear();
    test.issues.clear();
    analysis::validate(&test).map_err(bad)?;
    Ok(result)
}

/// A stable double read is a reviewed copy, not a cross-key transaction or proof
/// that the source stayed frozen after capture. The caller stages a CAS preview.
pub async fn ledger_document(ledger: &Ledger, record: &EngagementRecord) -> EResult<Value> {
    let first = read_ledger(ledger).await?;
    let second = read_ledger(ledger).await?;
    if first != second {
        return Err(conflict(
            "Ledger changed between capture reads; stop source writes and retry",
        ));
    }
    let accounts = ledger_accounts(&first, record)?;
    let source_hash = hash(&first)?;
    Ok(json!({ "accounts": accounts, "source_hash": source_hash,
        "provenance": { "source": "nxt_ledger", "hash_scope": "sorted indexed account/posting IDs and exact stored record strings, company and entry sequence", "source_hash": source_hash,
            "consistency": "stable_double_read_not_transactional", "source_write_freeze_verified": false, "review_required": true,
            "target_engagement_id": record.id, "target_revision": record.revision, "target_input_hash": hash(&record.document)?,
            "period_start": record.document.period_start, "period_end": record.document.period_end, "prior_period_start": record.document.prior_period_start, "prior_period_end": record.document.prior_period_end,
            "source_accounts": first.accounts.len(), "source_postings": first.entries.len(), "zero_accounts_retained": true, "ledger_writes": false }
    }))
}

fn snapshot_money(value: &Value, key: &str) -> EResult<i64> {
    value[key]
        .as_i64()
        .ok_or_else(|| conflict(format!("Saved engagement amount {key} is unavailable")))
}

fn tax_package(record: &EngagementRecord, jurisdiction: &str) -> EResult<Value> {
    if jurisdiction != "AB" || !record.document.period_end.starts_with("2025-") {
        return Err(conflict(
            "Tax bridge supports native 2025 Alberta T2 drafts only",
        ));
    }
    let snapshot = fresh(record)?;
    if hash(&snapshot.input_document)? != snapshot.input_hash {
        return Err(conflict(
            "Saved snapshot input document does not match its hash",
        ));
    }
    let current = &snapshot.result["financial_statements"]["current"];
    let prior = &snapshot.result["financial_statements"]["prior"];
    let used: BTreeSet<&str> = record
        .document
        .accounts
        .iter()
        .filter_map(|account| account.group_id.as_deref())
        .collect();
    let mut mapped: BTreeMap<String, (i128, i128)> = BTreeMap::new();
    for group in &record.document.groups {
        if !used.contains(group.id.as_str()) {
            continue;
        }
        let Some(gifi) = group
            .gifi_code
            .as_deref()
            .filter(|code| matches!(*code, "9990" | "9995"))
        else {
            continue;
        };
        if group.kind != "expense" {
            return Err(conflict(
                "Tax provisions require explicit expense group mappings",
            ));
        }
        let row = snapshot.result["groups"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"] == group.id))
            .ok_or_else(|| conflict("Saved group reconciliation is missing"))?;
        let amounts = mapped.entry(gifi.into()).or_default();
        plus(
            &mut amounts.0,
            i128::from(snapshot_money(row, "current_net_cents")?),
        )?;
        plus(
            &mut amounts.1,
            i128::from(snapshot_money(row, "prior_net_cents")?),
        )?;
    }
    let mut rows = Vec::new();
    for (code, (current, prior)) in &mapped {
        rows.push(GifiRow {
            section: "income_statement".into(),
            code: code.clone(),
            label: format!("Explicit tax provision mapping {code}"),
            current_cents: amount(*current)?,
            prior_cents: amount(*prior)?,
        });
    }
    let mut pair =
        |section: &str, code: &str, label: &str, current: i128, prior: i128| -> EResult<()> {
            rows.push(GifiRow {
                section: section.into(),
                code: code.into(),
                label: label.into(),
                current_cents: amount(current)?,
                prior_cents: amount(prior)?,
            });
            Ok(())
        };
    let get = |statement: &Value, key: &str| -> EResult<i128> {
        Ok(i128::from(snapshot_money(statement, key)?))
    };
    pair(
        "balance_sheet",
        "2599",
        "Total assets",
        get(current, "assets_cents")?,
        get(prior, "assets_cents")?,
    )?;
    pair(
        "balance_sheet",
        "3499",
        "Total liabilities",
        get(current, "liabilities_cents")?,
        get(prior, "liabilities_cents")?,
    )?;
    pair(
        "balance_sheet",
        "3620",
        "Total equity including current-period earnings",
        get(current, "equity_cents")? + get(current, "net_income_cents")?,
        get(prior, "equity_cents")? + get(prior, "net_income_cents")?,
    )?;
    let provision = |code: &str| mapped.get(code).copied().unwrap_or_default();
    let current_tax = provision("9990");
    let deferred_tax = provision("9995");
    let ordinary_expense = (
        get(current, "expenses_cents")? - current_tax.0 - deferred_tax.0,
        get(prior, "expenses_cents")? - current_tax.1 - deferred_tax.1,
    );
    let revenue = (get(current, "revenue_cents")?, get(prior, "revenue_cents")?);
    pair(
        "income_statement",
        "8299",
        "Total revenue",
        revenue.0,
        revenue.1,
    )?;
    pair(
        "income_statement",
        "9368",
        "Ordinary expenses excluding explicitly mapped tax provisions",
        ordinary_expense.0,
        ordinary_expense.1,
    )?;
    pair(
        "income_statement",
        "9369",
        "Ordinary net income before tax",
        revenue.0 - ordinary_expense.0,
        revenue.1 - ordinary_expense.1,
    )?;
    pair(
        "income_statement",
        "9970",
        "Net income before tax in the ordinary scoped workpaper",
        revenue.0 - ordinary_expense.0,
        revenue.1 - ordinary_expense.1,
    )?;
    pair(
        "income_statement",
        "9999",
        "Net income after recorded expenses and provisions",
        get(current, "net_income_cents")?,
        get(prior, "net_income_cents")?,
    )?;
    let profile = CorporateProfile { province: "AB".into(), gifi: rows, reconciliation_confirmed: false,
        notes: "Selected aggregate workpaper copy from NXT Engagements; not a complete GIFI statement. Amounts remain internal CAD cents, not filing-rounded dollars. No arbitrary detail GIFI mappings are copied. Current/deferred tax provisions are separated only from explicit expense mappings 9990/9995; verify provisions before review. No taxable-income, CCPC, eligibility or tax-adjustment inference.".into(), ..CorporateProfile::default() };
    crate::tax::corporate::validate(&profile).map_err(|error| {
        conflict(format!(
            "Tax bridge exceeds the corporate workpaper scope: {error}"
        ))
    })?;
    let catalog = crate::tax::catalog::catalog();
    let mut defaults = catalog["programs"]
        .as_array()
        .and_then(|programs| programs.iter().find(|program| program["id"] == "T2"))
        .map(|program| program["defaults"].clone())
        .ok_or_else(|| conflict("Native T2 catalog is unavailable"))?;
    let start = canonical_date(&record.document.period_start)?;
    let end = canonical_date(&record.document.period_end)?;
    defaults["fiscal_days"] = json!((end - start).num_days() + 1);
    if record.document.client_ref.len() > 100 {
        return Err(conflict(
            "Client reference exceeds the Tax workpaper's 100-byte bound",
        ));
    }
    let input = ReturnInput {
        program: "T2".into(),
        tax_year: 2025,
        jurisdiction: "AB".into(),
        name: format!("T2 draft from {}", record.id),
        client_ref: record.document.client_ref.clone(),
        period_start: Some(record.document.period_start.clone()),
        period_end: Some(record.document.period_end.clone()),
        inputs: defaults,
        parties: Vec::new(),
        corporate: Some(profile),
        notes: format!(
            "Engagement reviewed copy: {} input revision {}. Input SHA-256 {}. Output SHA-256 {}. Taxable income and tax adjustments remain unverified; no source or existing Tax record is overwritten.",
            record.id, record.input_revision, snapshot.input_hash, snapshot.output_hash
        ),
        expected_revision: None,
    };
    Ok(
        json!({ "format": "nxt-tax-return/v1", "classification": "unverified_draft_proposal", "return": input,
        "source": { "module": "engagements", "engagement_id": record.id, "input_revision": record.input_revision, "input_hash": snapshot.input_hash, "output_hash": snapshot.output_hash },
        "scope": "Selected aggregate internal workpaper only; full GIFI detail, actual-dollar filing format and certification are not implemented.",
        "sources": [{"title":"CRA RC4088 — GIFI definitions and reporting requirements","url":"https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/rc4088/general-index-financial-information-gifi.html"}], "filing_enabled": false }),
    )
}

pub async fn tax_preview(
    tax: &Tax,
    record: &EngagementRecord,
    jurisdiction: &str,
) -> EResult<Value> {
    let document = tax_package(record, jurisdiction)?;
    let preview = tax.preview_from_engagement(document.clone()).await?;
    Ok(
        json!({ "preview": preview, "source": document["source"], "source_revision": record.revision,
        "target": "tax", "target_preview_hash_scope": "SHA-256 of a namespaced canonical JSON proposal envelope, not original source bytes",
        "creates_draft_only_after_commit": true, "filing_enabled": false, "taxable_income_inferred": false,
        "warning": "Preview is a reviewed copy, not an atomic cross-module operation. Verify source hashes and all tax inputs before explicit commit." }),
    )
}

pub async fn firm_proposal(
    firm: &Firm,
    record: &EngagementRecord,
    project_id: Option<&str>,
) -> EResult<Value> {
    analysis::validate(&record.document).map_err(bad)?;
    let input_hash = hash(&record.document)?;
    let key = hash(&(
        record.id.as_str(),
        record.input_revision,
        input_hash.as_str(),
        project_id,
    ))?;
    let run = firm
        .propose_action(ProposalInput {
            name: format!("Engagement preparation {}", record.id),
            idempotency_key: format!("engprep:{key}"),
            action: WorkflowAction::CreateTask {
                title: format!(
                    "Prepare engagement {} ({} to {}), input {}. Review evidence and blockers.",
                    record.id,
                    record.document.period_start,
                    record.document.period_end,
                    &input_hash[..12]
                ),
                project_id: project_id.map(str::to_owned),
            },
        })
        .await?;
    Ok(
        json!({ "proposal": run, "source": { "module": "engagements", "engagement_id": record.id, "source_revision": record.revision, "input_revision": record.input_revision, "input_hash": input_hash },
        "human_approval_required": true, "execution_requested": false, "cross_module_atomic": false }),
    )
}

#[cfg(test)]
mod tests {
    use super::super::{models::EngagementInput, storage_models::Snapshot};
    use super::*;
    use crate::store::Store;

    fn ok<T>(result: EResult<T>) -> T {
        result.map_err(|error| error.1).unwrap()
    }

    fn fixture() -> EngagementRecord {
        let rows = [
            ("cash", "1000", "asset", 1_700_000, 0, "1000"),
            ("ap", "2000", "liability", 0, 300_000, "2620"),
            ("capital", "3000", "equity", 0, 1_200_000, "3500"),
            ("sales", "4000", "revenue", 0, 930_000, "8000"),
            ("expense", "5000", "expense", 700_000, 0, "9270"),
            ("currenttax", "5100", "expense", 20_000, 0, "9990"),
            ("deferredtax", "5200", "expense", 10_000, 0, "9995"),
        ];
        let accounts = rows.iter().map(|(id, code, kind, debit, credit, _)| json!({"id":id,"code":code,"name":id,"kind":kind,"debit_cents":debit,"credit_cents":credit,"group_id":id})).collect::<Vec<_>>();
        let groups = rows.iter().map(|(id, code, kind, _, _, gifi)| json!({"id":id,"code":code,"name":id,"kind":kind,"gifi_code":gifi})).collect::<Vec<_>>();
        let document: EngagementInput = serde_json::from_value(json!({"name":"Synthetic bridge", "period_start":"2025-01-01","period_end":"2025-12-31","accounts":accounts,"groups":groups,"workpapers":[{"id":"wp1","reference":"A","title":"Lead","body":"Synthetic support","required":true}]})).unwrap();
        let result = analysis::analyze(&document);
        assert_eq!(result["supported"], true);
        let snapshot = Snapshot {
            id: "snapshot1".into(),
            input_revision: 1,
            input_hash: ok(hash(&document)),
            output_hash: ok(hash(&result)),
            input_document: document.clone(),
            result,
            created_at: "2026-10-25T00:00:00Z".into(),
        };
        EngagementRecord {
            id: "eng_fixture".into(),
            revision: 2,
            input_revision: 1,
            status: "draft".into(),
            document,
            snapshot: Some(snapshot.clone()),
            snapshots: vec![snapshot],
            signoffs: vec![],
            signoff_history: vec![],
            lock_history: vec![],
            import_history: vec![],
            created_at: String::new(),
            updated_at: String::new(),
            rollforward_from: None,
            rollforward_origin: None,
        }
    }

    fn source(kind: &str, date: &str, voided: bool) -> LedgerRead {
        LedgerRead { company:Some(serde_json::to_string(&Company::default()).unwrap()), sequence:Some("1".into()),
            accounts:vec![
                ("a".into(), json!({"id":"a","code":"0010","name":"Asset","kind":"asset"}).to_string()),
                ("b".into(), json!({"id":"b","code":"0020","name":"Offset","kind":kind}).to_string())],
            entries:vec![("je1".into(), json!({"id":"je1","number":1,"date":date,"memo":"Synthetic","source":"journal","source_id":null,"created_at":"2025-01-01T00:00:00Z","voided":voided,"lines":[{"account_id":"a","debit":101,"credit":0},{"account_id":"b","debit":0,"credit":101}]}).to_string())] }
    }

    #[test]
    fn tax_projection_separates_verified_provisions_and_never_infers_taxable_income() {
        let record = fixture();
        let package = ok(tax_package(&record, "AB"));
        let draft: ReturnInput = serde_json::from_value(package["return"].clone()).unwrap();
        assert_eq!(draft.inputs["scope_confirmed"], false);
        assert_eq!(draft.inputs["taxable_income_cents"], 0);
        assert_eq!(draft.inputs["small_business_eligible"], false);
        let profile = draft.corporate.unwrap();
        assert!(!profile.reconciliation_confirmed);
        let row = |code: &str| {
            profile
                .gifi
                .iter()
                .find(|row| row.code == code)
                .unwrap()
                .current_cents
        };
        assert_eq!(row("2599"), 1_700_000);
        assert_eq!(row("3499"), 300_000);
        assert_eq!(row("3620"), 1_400_000);
        assert_eq!(row("9368"), 700_000);
        assert_eq!(row("9970"), 230_000);
        assert_eq!(row("9990"), 20_000);
        assert_eq!(row("9995"), 10_000);
        assert_eq!(row("9999"), 200_000);
        assert!(
            profile
                .gifi
                .iter()
                .all(|row| !["1000", "2620", "3500", "8000", "9270"].contains(&row.code.as_str()))
        );
        let validation = crate::tax::corporate::analyze(&profile, 200_000, "AB", false);
        assert_eq!(
            validation["reconciliation"]["balance_sheets"][0]["balanced"],
            true
        );
        assert_eq!(
            validation["reconciliation"]["income_statement"]["expected_after_tax_current_cents"],
            200_000
        );
        assert!(
            !validation["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic["code"] == "gifi_after_tax_mismatch")
        );
        assert_eq!(validation["supported"], false); // Confirmation remains a human action.
        assert!(tax_package(&record, "ON").is_err());
        let mut stale = record.clone();
        stale.document.accounts[0].debit_cents += 1;
        assert!(tax_package(&stale, "AB").is_err());
    }

    #[test]
    fn ledger_capture_keeps_codes_and_zero_accounts_and_rejects_corrupt_or_unclosed_books() {
        let record = fixture();
        let valid = source("liability", "2025-01-01", false);
        let rows = ok(ledger_accounts(&valid, &record));
        assert_eq!(rows[0].code, "0010");
        assert_eq!(rows[0].debit_cents, 101);
        assert_eq!(rows[1].credit_cents, 101);
        let zero = ok(ledger_accounts(
            &source("liability", "2025-01-01", true),
            &record,
        ));
        assert_eq!(zero.len(), 2);
        assert_eq!(zero[0].debit_cents, 0);
        let mut broken = valid;
        broken.sequence = Some("2".into());
        assert!(ledger_accounts(&broken, &record).is_err());
        broken = source("liability", "2025-01-01", false);
        broken.entries[0].1 = "malformed".into();
        assert!(ledger_accounts(&broken, &record).is_err());
        broken = source("liability", "2025-01-01", false);
        broken.company = None;
        assert!(ledger_accounts(&broken, &record).is_err());
        assert!(
            ledger_accounts(&source("income", "2024-12-31", false), &record)
                .unwrap_err()
                .1
                .contains("Historical income/expense")
        );
        assert_eq!(
            ok(ledger_accounts(
                &source("income", "2025-01-01", false),
                &record
            ))[1]
                .kind,
            "revenue"
        );
    }

    #[tokio::test]
    async fn tax_preview_retries_and_commit_preserve_one_unconfirmed_draft() {
        let tax = Tax::new(Store::memory());
        let record = fixture();
        let (a, b) = tokio::join!(
            tax_preview(&tax, &record, "AB"),
            tax_preview(&tax, &record, "AB")
        );
        let a = ok(a);
        let b = ok(b);
        assert_eq!(a["preview"]["id"], b["preview"]["id"]);
        assert_eq!(ok(tax.list(None).await).len(), 0);
        let id = a["preview"]["id"].as_str().unwrap();
        let retained = ok(tax.import_preview(id).await);
        assert_eq!(retained["document"]["inputs"]["taxable_income_cents"], 0);
        assert_eq!(
            retained["document"]["corporate"]["reconciliation_confirmed"],
            false
        );
        let first = ok(tax.commit_import(id).await);
        let replay = ok(tax_preview(&tax, &record, "AB").await);
        let second = ok(tax
            .commit_import(replay["preview"]["id"].as_str().unwrap())
            .await);
        assert_eq!(first.id, second.id);
        assert_eq!(first.status, "draft");
        assert!(first.calculation.is_none());
        assert_eq!(ok(tax.list(None).await).len(), 1);
    }

    #[tokio::test]
    async fn public_tax_previews_remain_independent_and_do_not_impersonate_bridge_retries() {
        let tax = Tax::new(Store::memory());
        let record = fixture();
        let package = ok(tax_package(&record, "AB"));
        let first = ok(tax.preview(package.clone()).await);
        let second = ok(tax.preview(package).await);
        assert_ne!(first["id"], second["id"]);
        let bridge = ok(tax_preview(&tax, &record, "AB").await);
        assert_ne!(bridge["preview"]["id"], first["id"]);
        assert_ne!(bridge["preview"]["id"], second["id"]);
        assert_ne!(bridge["preview"]["source_hash"], first["source_hash"]);
        assert_eq!(ok(tax.list(None).await).len(), 0);
    }
}
