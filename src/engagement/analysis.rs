use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::models::*;

const MAX_AMOUNT: i64 = 100_000_000_000_000;
const MAX_JSON_CENTS: i128 = 9_007_199_254_740_991;
pub(super) const RULESET: &str = "nxt-engagement-workpapers-v1";
const SCOPE: &str = "Original NXT engagement workpapers: exact-cent trial balance, local working adjustments, group reconciliation and text workpaper controls. No Ledger posting, assurance opinion, certified GIFI or proprietary Caseware format compatibility.";

fn text(
    value: &str,
    label: &str,
    max: usize,
    required: bool,
    multiline: bool,
) -> Result<(), String> {
    if value.len() > max || (required && value.trim().is_empty()) {
        return Err(format!(
            "{label} must {}be at most {max} UTF-8 bytes",
            if required { "be nonempty and " } else { "" }
        ));
    }
    if value
        .chars()
        .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
    {
        return Err(format!("{label} contains unsupported control characters"));
    }
    Ok(())
}

fn id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
    {
        return Err(format!(
            "{label} must contain 1..64 ASCII letters, digits, underscores or hyphens"
        ));
    }
    Ok(())
}

fn tickmark_definition(mark: &TickmarkDefinition) -> Result<(), String> {
    id(&mark.id, "Tickmark definition ID")?;
    if mark.code.is_empty() || mark.code.len() > 12
        || !mark.code.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c,b'_'|b'-')) {
        return Err("Custom tickmark code must contain 1..12 ASCII uppercase letters, digits, underscores or hyphens".into());
    }
    if ["NOTE","AG","RC","TR","IN","CF"].contains(&mark.code.as_str()) {
        return Err("Custom tickmark code is reserved for a builtin annotation".into());
    }
    text(&mark.label,"Tickmark label",128,true,false)?;
    text(&mark.description,"Tickmark description",1024,true,false)?;
    if mark.label.chars().chain(mark.description.chars()).any(|c| matches!(c,'\u{2028}'|'\u{2029}')) {
        return Err("Custom tickmark label and description must be single-line text".into());
    }
    Ok(())
}

fn code(value: &str, label: &str) -> Result<(), String> {
    text(value, label, 64, true, false)?;
    if value != value.trim() {
        return Err(format!(
            "{label} cannot contain leading or trailing whitespace"
        ));
    }
    Ok(())
}

fn date(value: &str, label: &str) -> Result<NaiveDate, String> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("{label} must be YYYY-MM-DD"))?;
    if parsed.format("%Y-%m-%d").to_string() != value || !(1900..=2200).contains(&parsed.year()) {
        return Err(format!(
            "{label} must be a canonical date from 1900 through 2200"
        ));
    }
    Ok(parsed)
}

fn annual(start: &str, end: &str, label: &str) -> Result<(NaiveDate, NaiveDate), String> {
    let start = date(start, &format!("{label} start"))?;
    let end = date(end, &format!("{label} end"))?;
    if end < start || (end - start).num_days() + 1 > 366 {
        return Err(format!("{label} must contain 1..366 inclusive days"));
    }
    Ok((start, end))
}

fn kind(value: &str) -> bool {
    matches!(
        value,
        "asset" | "liability" | "equity" | "revenue" | "expense"
    )
}

fn balance(debit: i64, credit: i64, label: &str) -> Result<(), String> {
    if !(0..=MAX_AMOUNT).contains(&debit) || !(0..=MAX_AMOUNT).contains(&credit) {
        return Err(format!(
            "{label} amounts must be integer cents from 0 through {MAX_AMOUNT}"
        ));
    }
    if debit > 0 && credit > 0 {
        return Err(format!(
            "{label} must be one-sided; use debit or credit, not both"
        ));
    }
    Ok(())
}

fn unique(
    values: impl IntoIterator<Item = String>,
    label: &str,
) -> Result<BTreeSet<String>, String> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(format!("Duplicate {label}"));
        }
    }
    Ok(seen)
}

fn references(
    values: &[String],
    allowed: &BTreeSet<String>,
    max: usize,
    label: &str,
) -> Result<(), String> {
    if values.len() > max {
        return Err(format!("Too many {label}"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        id(value, label)?;
        if !allowed.contains(value) {
            return Err(format!("Unknown {label}: {value}"));
        }
        if !seen.insert(value) {
            return Err(format!("Duplicate {label}: {value}"));
        }
    }
    Ok(())
}

fn visit<'a>(
    node: &'a str,
    graph: &BTreeMap<&'a str, &'a [String]>,
    active: &mut BTreeSet<&'a str>,
    complete: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    if complete.contains(node) {
        return Ok(());
    }
    if !active.insert(node) {
        return Err("Workpaper dependency graph contains a cycle".into());
    }
    if let Some(dependencies) = graph.get(node) {
        for dependency in *dependencies {
            visit(dependency, graph, active, complete)?;
        }
    }
    active.remove(node);
    complete.insert(node);
    Ok(())
}

/// Validate a bounded input document without executing or posting any adjustment.
/// Unbalanced/incomplete working drafts are retained for analysis diagnostics.
pub fn validate(input: &EngagementInput) -> Result<(), String> {
    text(&input.name, "Engagement name", 256, true, false)?;
    text(&input.client_ref, "Client reference", 200, false, false)?;
    text(&input.notes, "Engagement notes", 16 * 1024, false, true)?;
    if !matches!(
        input.kind.as_str(),
        "internal" | "compilation" | "review" | "audit"
    ) {
        return Err("Unknown engagement kind".into());
    }
    if input.currency != "CAD" {
        return Err(
            "This ruleset supports CAD only; no foreign-exchange conversion is implemented".into(),
        );
    }
    if input.tickmark_catalogue.len() > 50 {
        return Err("Custom tickmark catalogue exceeds 50 definitions".into());
    }
    let mut tickmark_ids=BTreeSet::new();
    let mut tickmark_codes=BTreeSet::new();
    for mark in &input.tickmark_catalogue {
        tickmark_definition(mark)?;
        if !tickmark_ids.insert(&mark.id) {return Err("Duplicate tickmark definition ID".into());}
        if !tickmark_codes.insert(mark.code.to_ascii_uppercase()) {return Err("Duplicate case-insensitive tickmark code".into());}
    }
    let (start, end) = annual(&input.period_start, &input.period_end, "Current period")?;
    match (&input.prior_period_start, &input.prior_period_end) {
        (Some(a), Some(b)) => {
            let (_, prior_end) = annual(a, b, "Prior period")?;
            if prior_end >= start {
                return Err("Prior period must end before the current period starts".into());
            }
        }
        (None, None) => {}
        _ => return Err("Prior period start and end must be supplied together".into()),
    }
    if input.accounts.len() > 500
        || input.groups.len() > 200
        || input.adjustments.len() > 200
        || input.workpapers.len() > 100
        || input.issues.len() > 200
    {
        return Err("Engagement exceeds the 500 account, 200 group, 200 adjustment, 100 workpaper or 200 issue limit".into());
    }
    let accounts = unique(input.accounts.iter().map(|a| a.id.clone()), "account ID")?;
    unique(
        input.accounts.iter().map(|a| a.code.clone()),
        "account code",
    )?;
    let groups = unique(input.groups.iter().map(|a| a.id.clone()), "group ID")?;
    unique(input.groups.iter().map(|a| a.code.clone()), "group code")?;
    let adjustments = unique(
        input.adjustments.iter().map(|a| a.id.clone()),
        "adjustment ID",
    )?;
    unique(
        input.adjustments.iter().map(|a| a.reference.clone()),
        "adjustment reference",
    )?;
    let papers = unique(
        input.workpapers.iter().map(|a| a.id.clone()),
        "workpaper ID",
    )?;
    unique(
        input.workpapers.iter().map(|a| a.reference.clone()),
        "workpaper reference",
    )?;
    unique(input.issues.iter().map(|a| a.id.clone()), "issue ID")?;
    for group in &input.groups {
        id(&group.id, "Group ID")?;
        code(&group.code, "Group code")?;
        text(&group.name, "Group name", 256, true, false)?;
        if !kind(&group.kind) {
            return Err("Unknown group kind".into());
        }
        if let Some(gifi) = &group.gifi_code {
            if gifi.len() != 4 || !gifi.bytes().all(|c| c.is_ascii_digit()) {
                return Err("GIFI metadata must be exactly four ASCII digits".into());
            }
            if matches!(
                gifi.as_str(),
                "2599" | "3499" | "3620" | "3640" | "8299" | "9368" | "9970" | "9999"
            ) {
                return Err("Aggregate GIFI totals cannot be used as detail group mappings".into());
            }
            if matches!(gifi.as_str(), "9990" | "9995") && group.kind != "expense" {
                return Err("GIFI 9990 and 9995 metadata is restricted to expense groups".into());
            }
        }
    }
    for account in &input.accounts {
        id(&account.id, "Account ID")?;
        code(&account.code, "Account code")?;
        text(&account.name, "Account name", 256, true, false)?;
        if !kind(&account.kind) {
            return Err("Unknown account kind".into());
        }
        balance(
            account.debit_cents,
            account.credit_cents,
            "Account current balance",
        )?;
        balance(
            account.prior_debit_cents,
            account.prior_credit_cents,
            "Account prior balance",
        )?;
        if let Some(group_id) = &account.group_id {
            id(group_id, "Account group ID")?;
            if !groups.contains(group_id) {
                return Err(format!("Unknown account group ID: {group_id}"));
            }
            let group = input
                .groups
                .iter()
                .find(|g| &g.id == group_id)
                .ok_or("Unknown account group")?;
            if group.kind != account.kind {
                return Err("Account and assigned group kinds must match".into());
            }
        }
    }
    let mut line_count = 0usize;
    for adjustment in &input.adjustments {
        id(&adjustment.id, "Adjustment ID")?;
        code(&adjustment.reference, "Adjustment reference")?;
        text(
            &adjustment.description,
            "Adjustment description",
            4096,
            false,
            true,
        )?;
        if !matches!(adjustment.kind.as_str(), "adjusting" | "reclassifying") {
            return Err("Only local adjusting and reclassifying entries are supported".into());
        }
        let entry_date = date(&adjustment.date, "Adjustment date")?;
        if entry_date < start || entry_date > end {
            return Err("Adjustment date must fall within the current engagement period".into());
        }
        line_count = line_count
            .checked_add(adjustment.lines.len())
            .ok_or("Adjustment line count overflow")?;
        if adjustment.lines.len() > 100 || line_count > 5000 {
            return Err(
                "Adjustments exceed the 100 lines per entry or 5000 total lines limit".into(),
            );
        }
        for line in &adjustment.lines {
            id(&line.account_id, "Adjustment account ID")?;
            if !accounts.contains(&line.account_id) {
                return Err(format!(
                    "Unknown adjustment account ID: {}",
                    line.account_id
                ));
            }
            balance(line.debit_cents, line.credit_cents, "Adjustment line")?;
        }
    }
    let mut annotation_count = 0usize;
    let mut annotation_ids = BTreeSet::new();
    for paper in &input.workpapers {
        id(&paper.id, "Workpaper ID")?;
        code(&paper.reference, "Workpaper reference")?;
        text(&paper.title, "Workpaper title", 256, true, false)?;
        text(&paper.folder, "Workpaper folder", 256, false, false)?;
        text(&paper.body, "Workpaper body", 16 * 1024, false, true)?;
        if paper.document_type == WorkpaperType::Placeholder
            && (!paper.body.is_empty() || !paper.annotations.is_empty() || !paper.attachment_versions.is_empty())
        {
            return Err("Placeholder workpapers must have an exactly empty body and no annotations or file versions".into());
        }
        references(
            &paper.account_ids,
            &accounts,
            500,
            "workpaper account reference",
        )?;
        references(
            &paper.adjustment_ids,
            &adjustments,
            200,
            "workpaper adjustment reference",
        )?;
        references(&paper.depends_on, &papers, 100, "workpaper dependency")?;
        super::attachments::validate_paper_shape(paper).map_err(|_| "Invalid workpaper file-version metadata".to_string())?;
        annotation_count += paper.annotations.len();
        if paper.annotations.len() > 50 || annotation_count > 500 {
            return Err("Annotations exceed 50 per workpaper or 500 per engagement".into());
        }
        for annotation in &paper.annotations {
            id(&annotation.id, "Annotation ID")?;
            if !annotation_ids.insert(&annotation.id) {
                return Err("Duplicate annotation ID".into());
            }
            if !matches!(
                annotation.kind.as_str(),
                "note" | "agreed" | "recalculated" | "traced" | "inspected" | "confirmed" | "custom"
            ) {
                return Err("Unknown native annotation kind".into());
            }
            match (annotation.kind.as_str(),&annotation.custom_mark) {
                ("custom",Some(mark))=>tickmark_definition(mark)?,
                ("custom",None)=>return Err("Custom annotation requires a copied tickmark definition".into()),
                (_,Some(_))=>return Err("Builtin annotations cannot carry a custom tickmark snapshot".into()),
                (_,None)=>{}
            }
            text(&annotation.note, "Annotation note", 4096, true, true)?;
            text(
                &annotation.operator_label,
                "Annotation operator label",
                128,
                true,
                false,
            )?;
            if annotation.body_sha256.len() != 64
                || !annotation
                    .body_sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err("Annotation body anchor must be a lowercase SHA-256".into());
            }
            if let Some(anchor) = &annotation.account_anchor {
                id(&anchor.engagement_id, "Account annotation engagement ID")?;
                id(&anchor.account_id, "Account annotation account ID")?;
                if !accounts.contains(&anchor.account_id) || !paper.account_ids.contains(&anchor.account_id) {
                    return Err("Account annotation must target an existing account referenced by its manual workpaper".into());
                }
                if anchor.context_sha256.len() != 64 || !anchor.context_sha256.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
                    return Err("Account annotation context must be a lowercase SHA-256".into());
                }
            }
        }
    }
    let graph: BTreeMap<&str, &[String]> = input
        .workpapers
        .iter()
        .map(|p| (p.id.as_str(), p.depends_on.as_slice()))
        .collect();
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for paper in &input.workpapers {
        visit(&paper.id, &graph, &mut active, &mut complete)?;
    }
    for issue in &input.issues {
        id(&issue.id, "Issue ID")?;
        text(&issue.title, "Issue title", 256, true, false)?;
        text(&issue.detail, "Issue detail", 4096, false, true)?;
        text(&issue.resolution, "Issue resolution", 4096, false, true)?;
        if !matches!(issue.priority.as_str(), "low" | "medium" | "high") {
            return Err("Unknown issue priority".into());
        }
        if !matches!(issue.status.as_str(), "open" | "resolved") {
            return Err("Unknown issue status".into());
        }
        if issue.status == "resolved" && issue.resolution.trim().is_empty() {
            return Err("Resolved issues require a resolution note".into());
        }
        if let Some(paper_id) = &issue.workpaper_id {
            id(paper_id, "Issue workpaper ID")?;
            if !papers.contains(paper_id) {
                return Err(format!("Unknown issue workpaper ID: {paper_id}"));
            }
        }
    }
    Ok(())
}

fn checked(value: i128) -> Result<i64, String> {
    let amount = i64::try_from(value)
        .map_err(|_| "Engagement aggregate exceeds the signed 64-bit cents limit")?;
    if !(-MAX_JSON_CENTS..=MAX_JSON_CENTS).contains(&value) {
        return Err("Engagement aggregate exceeds the exact integer cents range of the browser JSON interface".into());
    }
    Ok(amount)
}
fn add(total: &mut i128, value: i128) -> Result<(), String> {
    *total = total
        .checked_add(value)
        .ok_or("Engagement arithmetic overflow")?;
    Ok(())
}
fn columns(net: i128) -> Result<(i64, i64), String> {
    if net >= 0 {
        Ok((checked(net)?, 0))
    } else {
        Ok((
            0,
            checked(net.checked_neg().ok_or("Engagement arithmetic overflow")?)?,
        ))
    }
}
fn diagnostic(items: &mut Vec<Value>, code: &str, message: impl Into<String>, blocking: bool) {
    items.push(json!({ "code": code, "message": message.into(), "blocking": blocking, "severity": if blocking { "error" } else { "warning" } }));
}

#[derive(Default)]
struct Statement {
    asset: i128,
    liability: i128,
    equity: i128,
    revenue: i128,
    expense: i128,
}
impl Statement {
    fn accumulate(&mut self, kind: &str, debit_net: i128) -> Result<(), String> {
        match kind {
            "asset" => add(&mut self.asset, debit_net),
            "liability" => add(&mut self.liability, -debit_net),
            "equity" => add(&mut self.equity, -debit_net),
            "revenue" => add(&mut self.revenue, -debit_net),
            "expense" => add(&mut self.expense, debit_net),
            _ => Err("Unknown account kind".into()),
        }
    }
    fn result(&self) -> Result<Value, String> {
        let net_income = self
            .revenue
            .checked_sub(self.expense)
            .ok_or("Engagement arithmetic overflow")?;
        let difference = self
            .asset
            .checked_sub(self.liability)
            .and_then(|x| x.checked_sub(self.equity))
            .and_then(|x| x.checked_sub(net_income))
            .ok_or("Engagement arithmetic overflow")?;
        Ok(
            json!({ "assets_cents": checked(self.asset)?, "liabilities_cents": checked(self.liability)?, "equity_cents": checked(self.equity)?, "revenue_cents": checked(self.revenue)?, "expenses_cents": checked(self.expense)?, "net_income_cents": checked(net_income)?, "balance_sheet_difference_cents": checked(difference)? }),
        )
    }
}

fn calculate(input: &EngagementInput) -> Result<Value, String> {
    let mut diagnostics = Vec::new();
    let mut totals = [0i128; 8]; // Base D/C, adjustment D/C, adjusted D/C, prior D/C.
    let mut by_account: BTreeMap<&str, (i128, i128)> = BTreeMap::new();
    if input.accounts.is_empty() {
        diagnostic(
            &mut diagnostics,
            "empty_trial_balance",
            "Add a trial balance before review.",
            true,
        );
    }
    if input.workpapers.is_empty() {
        diagnostic(
            &mut diagnostics,
            "missing_workpapers",
            "Add engagement workpaper evidence before review.",
            true,
        );
    }
    if !input.workpapers.is_empty() && !input.workpapers.iter().any(|paper| paper.required) {
        diagnostic(
            &mut diagnostics,
            "no_required_workpapers",
            "Designate at least one required workpaper so preparation and review checkpoints can be completed.",
            true,
        );
    }
    for entry in &input.adjustments {
        let mut debit = 0i128;
        let mut credit = 0i128;
        let mut nonzero = 0;
        for line in &entry.lines {
            add(&mut debit, i128::from(line.debit_cents))?;
            add(&mut credit, i128::from(line.credit_cents))?;
            if line.debit_cents != 0 || line.credit_cents != 0 {
                nonzero += 1;
            }
            let amount = by_account.entry(&line.account_id).or_default();
            add(&mut amount.0, i128::from(line.debit_cents))?;
            add(&mut amount.1, i128::from(line.credit_cents))?;
        }
        add(&mut totals[2], debit)?;
        add(&mut totals[3], credit)?;
        if nonzero < 2 {
            diagnostic(
                &mut diagnostics,
                "incomplete_adjustment",
                format!(
                    "Adjustment {} needs at least two nonzero lines.",
                    entry.reference
                ),
                true,
            );
        }
        if entry
            .lines
            .iter()
            .any(|line| line.debit_cents == 0 && line.credit_cents == 0)
        {
            diagnostic(
                &mut diagnostics,
                "zero_adjustment_line",
                format!("Adjustment {} contains a zero-value line.", entry.reference),
                true,
            );
        }
        if debit != credit {
            diagnostic(
                &mut diagnostics,
                "unbalanced_adjustment",
                format!(
                    "Adjustment {} differs by {} cents.",
                    entry.reference,
                    checked(debit - credit)?
                ),
                true,
            );
        }
    }
    let mut accounts = Vec::with_capacity(input.accounts.len());
    let mut group_amounts: BTreeMap<&str, (i128, i128)> = BTreeMap::new();
    let mut current = Statement::default();
    let mut prior = Statement::default();
    let mut unmapped = 0usize;
    let mut has_prior = false;
    for account in &input.accounts {
        let (entry_debit, entry_credit) = by_account
            .get(account.id.as_str())
            .copied()
            .unwrap_or_default();
        let base_net = i128::from(account.debit_cents) - i128::from(account.credit_cents);
        let adjustment_net = entry_debit
            .checked_sub(entry_credit)
            .ok_or("Engagement arithmetic overflow")?;
        let adjusted_net = base_net
            .checked_add(adjustment_net)
            .ok_or("Engagement arithmetic overflow")?;
        let prior_net =
            i128::from(account.prior_debit_cents) - i128::from(account.prior_credit_cents);
        let (adjusted_debit, adjusted_credit) = columns(adjusted_net)?;
        add(&mut totals[0], i128::from(account.debit_cents))?;
        add(&mut totals[1], i128::from(account.credit_cents))?;
        add(&mut totals[4], i128::from(adjusted_debit))?;
        add(&mut totals[5], i128::from(adjusted_credit))?;
        add(&mut totals[6], i128::from(account.prior_debit_cents))?;
        add(&mut totals[7], i128::from(account.prior_credit_cents))?;
        current.accumulate(&account.kind, adjusted_net)?;
        prior.accumulate(&account.kind, prior_net)?;
        has_prior |= account.prior_debit_cents != 0 || account.prior_credit_cents != 0;
        if let Some(group_id) = &account.group_id {
            let amount = group_amounts.entry(group_id).or_default();
            add(&mut amount.0, adjusted_net)?;
            add(&mut amount.1, prior_net)?;
        } else {
            unmapped += 1;
        }
        accounts.push(json!({
            "id": account.id, "code": account.code, "name": account.name, "kind": account.kind, "group_id": account.group_id,
            "base_debit_cents": account.debit_cents, "base_credit_cents": account.credit_cents,
            "adjustment_debit_cents": checked(entry_debit)?, "adjustment_credit_cents": checked(entry_credit)?,
            "adjusted_debit_cents": adjusted_debit, "adjusted_credit_cents": adjusted_credit,
            "prior_debit_cents": account.prior_debit_cents, "prior_credit_cents": account.prior_credit_cents
        }));
    }
    let base_difference = checked(totals[0] - totals[1])?;
    let adjusted_difference = checked(totals[4] - totals[5])?;
    let prior_difference = checked(totals[6] - totals[7])?;
    if base_difference != 0 {
        diagnostic(
            &mut diagnostics,
            "unbalanced_trial_balance",
            format!("Unadjusted trial balance differs by {base_difference} cents."),
            true,
        );
    }
    if adjusted_difference != 0 {
        diagnostic(
            &mut diagnostics,
            "unbalanced_adjusted_trial_balance",
            format!("Adjusted trial balance differs by {adjusted_difference} cents."),
            true,
        );
    }
    if prior_difference != 0 {
        diagnostic(
            &mut diagnostics,
            "unbalanced_prior_trial_balance",
            format!("Prior trial balance differs by {prior_difference} cents."),
            true,
        );
    }
    if has_prior && input.prior_period_start.is_none() {
        diagnostic(
            &mut diagnostics,
            "missing_prior_period",
            "Nonzero comparative balances require explicit prior period dates.",
            true,
        );
    }
    if unmapped > 0 {
        diagnostic(
            &mut diagnostics,
            "unmapped_accounts",
            format!(
                "{unmapped} account(s) need a group mapping, including retained zero-balance accounts."
            ),
            true,
        );
    }
    for paper in &input.workpapers {
        if paper.document_type == WorkpaperType::Placeholder {
            diagnostic(
                &mut diagnostics,
                if paper.required { "required_placeholder_workpaper" } else { "optional_placeholder_pending" },
                format!("{} placeholder workpaper {} is pending planning material, not evidence; convert it to manual and add evidence before sign-off.", if paper.required { "Required" } else { "Optional" }, paper.reference),
                paper.required,
            );
            continue;
        }
        let body_hash = hex::encode(Sha256::digest(paper.body.as_bytes()));
        let mut account_contexts = BTreeMap::new();
        for annotation in &paper.annotations {
            if annotation.body_sha256 != body_hash {
                diagnostic(
                    &mut diagnostics,
                    "stale_workpaper_annotation",
                    format!(
                        "Workpaper {} annotation {} refers to an earlier text version; review and replace it or remove it.",
                        paper.reference, annotation.id
                    ),
                    true,
                );
            }
            if let Some(anchor) = &annotation.account_anchor {
                let context = account_contexts.entry((anchor.engagement_id.as_str(), anchor.account_id.as_str())).or_insert_with(|| super::account_annotations::context_sha256(input, &anchor.engagement_id, &paper.id, &anchor.account_id));
                if context.as_ref().map_or(true, |digest| digest != &anchor.context_sha256) {
                    diagnostic(&mut diagnostics, "stale_account_annotation", format!("Workpaper {} annotation {} account {} in engagement {} refers to an earlier local account context; remove it and use a new annotation ID after review.",paper.reference,annotation.id,anchor.account_id,anchor.engagement_id), true);
                }
            }
        }
        if paper.required && paper.body.trim().is_empty() {
            diagnostic(
                &mut diagnostics,
                "missing_required_workpaper",
                format!(
                    "Required workpaper {} has no evidence text.",
                    paper.reference
                ),
                true,
            );
        }
    }
    for issue in &input.issues {
        if issue.blocking && issue.status != "resolved" {
            diagnostic(
                &mut diagnostics,
                "open_blocking_issue",
                format!("Blocking issue {} remains open.", issue.id),
                true,
            );
        }
    }
    if input.groups.iter().any(|group| group.gifi_code.is_some()) {
        diagnostic(
            &mut diagnostics,
            "gifi_metadata_only",
            "GIFI codes are detail mapping metadata; this analysis does not generate or certify a full GIFI return.",
            false,
        );
    }
    if input.kind != "internal" {
        diagnostic(
            &mut diagnostics,
            "no_assurance_opinion",
            "Engagement classification is workspace metadata. This module does not produce an assurance opinion or validate professional standards compliance.",
            false,
        );
    }
    let mut groups = Vec::with_capacity(input.groups.len());
    for group in &input.groups {
        let (current_net, prior_net) = group_amounts
            .get(group.id.as_str())
            .copied()
            .unwrap_or_default();
        let sign = if matches!(group.kind.as_str(), "liability" | "equity" | "revenue") {
            -1
        } else {
            1
        };
        groups.push(json!({ "id": group.id, "code": group.code, "name": group.name, "kind": group.kind, "gifi_code": group.gifi_code, "current_net_cents": checked(current_net * sign)?, "prior_net_cents": checked(prior_net * sign)? }));
    }
    let supported = !diagnostics.iter().any(|item| item["blocking"] == true);
    Ok(json!({
        "ruleset": RULESET, "scope": SCOPE, "supported": supported, "diagnostics": diagnostics,
        "totals": {
            "unadjusted_debit_cents": checked(totals[0])?, "unadjusted_credit_cents": checked(totals[1])?, "unadjusted_difference_cents": base_difference,
            "adjustment_debit_cents": checked(totals[2])?, "adjustment_credit_cents": checked(totals[3])?,
            "adjusted_debit_cents": checked(totals[4])?, "adjusted_credit_cents": checked(totals[5])?, "adjusted_difference_cents": adjusted_difference,
            "prior_debit_cents": checked(totals[6])?, "prior_credit_cents": checked(totals[7])?, "prior_difference_cents": prior_difference,
            "unmapped_accounts": unmapped
        },
        "accounts": accounts, "groups": groups,
        "financial_statements": { "current": current.result()?, "prior": prior.result()? }
    }))
}

/// Deterministic, exact-cent local workpaper reconciliation. This performs no I/O.
pub fn analyze(input: &EngagementInput) -> Value {
    match validate(input).and_then(|_| calculate(input)) {
        Ok(value) => value,
        Err(message) => json!({ "ruleset": RULESET, "scope": SCOPE, "supported": false,
            "diagnostics": [{ "code": "invalid_input", "message": message, "blocking": true, "severity": "error" }],
            "totals": null, "accounts": [], "groups": [], "financial_statements": { "current": null, "prior": null } }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> EngagementInput {
        let rows = [
            ("cash", "00100", "asset", 1_000_000, 0),
            ("ar", "00110", "asset", 200_000, 0),
            ("equipment", "00200", "asset", 500_000, 0),
            ("expenses", "00500", "expense", 700_000, 0),
            ("ap", "00300", "liability", 0, 300_000),
            ("capital", "00310", "equity", 0, 1_200_000),
            ("sales", "00400", "revenue", 0, 900_000),
        ];
        let accounts: Vec<Account> = rows
            .iter()
            .map(|(id, code, kind, debit, credit)| Account {
                id: id.to_string(),
                code: code.to_string(),
                name: id.to_string(),
                kind: kind.to_string(),
                group_id: Some(kind.to_string()),
                debit_cents: *debit,
                credit_cents: *credit,
                ..Account::default()
            })
            .collect();
        let groups = ["asset", "liability", "equity", "revenue", "expense"]
            .iter()
            .map(|kind| Group {
                id: kind.to_string(),
                code: kind.to_string(),
                name: kind.to_string(),
                kind: kind.to_string(),
                gifi_code: None,
            })
            .collect();
        EngagementInput {
            name: "Synthetic engagement".into(),
            client_ref: "fixture".into(),
            kind: "internal".into(),
            period_start: "2025-01-01".into(),
            period_end: "2025-12-31".into(),
            prior_period_start: None,
            prior_period_end: None,
            currency: "CAD".into(),
            accounts,
            groups,
            adjustments: vec![Adjustment {
                id: "aje1".into(),
                reference: "AJE-1".into(),
                date: "2025-12-31".into(),
                kind: "adjusting".into(),
                description: "Accrual".into(),
                lines: vec![
                    AdjustmentLine {
                        account_id: "expenses".into(),
                        debit_cents: 100_000,
                        credit_cents: 0,
                    },
                    AdjustmentLine {
                        account_id: "ap".into(),
                        debit_cents: 0,
                        credit_cents: 100_000,
                    },
                ],
            }],
            workpapers: vec![Workpaper {
                id: "wp1".into(),
                reference: "A".into(),
                title: "Lead".into(),
                body: "Synthetic reviewed support".into(),
                required: true,
                account_ids: vec!["expenses".into()],
                adjustment_ids: vec!["aje1".into()],
                ..Workpaper::default()
            }],
            issues: vec![],
            notes: String::new(),
            expected_revision: None,
            tickmark_catalogue: vec![],
        }
    }

    fn has(output: &Value, code: &str) -> bool {
        output["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == code)
    }

    #[test]
    fn exact_local_adjustments_and_statements_reconcile() {
        let input = fixture();
        validate(&input).unwrap();
        let output = analyze(&input);
        assert_eq!(output["supported"], true);
        assert_eq!(output["totals"]["unadjusted_debit_cents"], 2_400_000);
        assert_eq!(output["totals"]["adjustment_debit_cents"], 100_000);
        assert_eq!(output["totals"]["adjusted_debit_cents"], 2_500_000);
        assert_eq!(output["totals"]["adjusted_credit_cents"], 2_500_000);
        assert_eq!(
            output["financial_statements"]["current"],
            json!({ "assets_cents": 1_700_000, "liabilities_cents": 400_000, "equity_cents": 1_200_000, "revenue_cents": 900_000, "expenses_cents": 800_000, "net_income_cents": 100_000, "balance_sheet_difference_cents": 0 })
        );
        assert_eq!(output["accounts"][0]["code"], "00100");
        assert_eq!(analyze(&input), output);
        assert_eq!(input.accounts[3].debit_cents, 700_000);
    }

    #[test]
    fn contra_balances_normalize_without_losing_sign_or_cents() {
        let mut input = fixture();
        input.adjustments[0].kind = "reclassifying".into();
        input.adjustments[0].lines = vec![
            AdjustmentLine {
                account_id: "cash".into(),
                debit_cents: 0,
                credit_cents: 1_000_001,
            },
            AdjustmentLine {
                account_id: "ar".into(),
                debit_cents: 1_000_001,
                credit_cents: 0,
            },
        ];
        input.accounts.push(Account {
            id: "zero".into(),
            code: "00000".into(),
            name: "Retained zero".into(),
            kind: "asset".into(),
            group_id: Some("asset".into()),
            ..Account::default()
        });
        let output = analyze(&input);
        assert_eq!(output["supported"], true);
        assert_eq!(output["accounts"][0]["adjusted_debit_cents"], 0);
        assert_eq!(output["accounts"][0]["adjusted_credit_cents"], 1);
        assert_eq!(
            output["financial_statements"]["current"]["assets_cents"],
            1_700_000
        );
        assert_eq!(output["totals"]["adjusted_difference_cents"], 0);
        assert_eq!(output["totals"]["adjusted_debit_cents"], 2_400_001);
    }

    #[test]
    fn incomplete_and_offsetting_bad_entries_cannot_be_reviewed() {
        let mut input = fixture();
        input.adjustments[0].lines[1].credit_cents -= 1;
        let mut other = input.adjustments[0].clone();
        other.id = "aje2".into();
        other.reference = "AJE-2".into();
        other.lines[0].debit_cents = 99_999;
        other.lines[1].credit_cents = 100_000;
        input.adjustments.push(other);
        validate(&input).unwrap();
        let output = analyze(&input);
        assert_eq!(output["totals"]["adjusted_difference_cents"], 0);
        assert_eq!(output["supported"], false);
        assert_eq!(
            output["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["code"] == "unbalanced_adjustment")
                .count(),
            2
        );
        input.adjustments[0].lines.clear();
        assert!(has(&analyze(&input), "incomplete_adjustment"));
        input.adjustments[0].lines.push(AdjustmentLine {
            account_id: "cash".into(),
            ..AdjustmentLine::default()
        });
        assert!(has(&analyze(&input), "zero_adjustment_line"));
    }

    #[test]
    fn comparatives_mapping_and_evidence_are_blocking_controls() {
        let mut input = fixture();
        input.accounts[0].prior_debit_cents = 101;
        input.accounts[4].prior_credit_cents = 100;
        input.accounts[0].group_id = None;
        input.workpapers[0].body.clear();
        input.issues.push(Issue {
            id: "i1".into(),
            title: "Missing support".into(),
            detail: String::new(),
            priority: "high".into(),
            blocking: true,
            status: "open".into(),
            workpaper_id: Some("wp1".into()),
            resolution: String::new(),
            carry_forward: true,
        });
        let output = analyze(&input);
        for code in [
            "unbalanced_prior_trial_balance",
            "missing_prior_period",
            "unmapped_accounts",
            "missing_required_workpaper",
            "open_blocking_issue",
        ] {
            assert!(has(&output, code), "{code}");
        }
        input.accounts[4].prior_credit_cents = 101;
        input.prior_period_start = Some("2024-01-01".into());
        input.prior_period_end = Some("2024-12-31".into());
        input.accounts[0].group_id = Some("asset".into());
        input.workpapers[0].body = "Evidence".into();
        input.issues[0].status = "resolved".into();
        input.issues[0].resolution = "Support obtained".into();
        assert_eq!(analyze(&input)["supported"], true);
    }

    #[test]
    fn invalid_amounts_unknown_ids_cycles_and_gifi_are_rejected() {
        let base = fixture();
        let mut input = base.clone();
        input.accounts[0].debit_cents = -1;
        assert!(validate(&input).is_err());
        input = base.clone();
        input.accounts[0].debit_cents = i64::MAX;
        assert!(validate(&input).is_err());
        input = base.clone();
        input.accounts[0].credit_cents = 1;
        assert!(validate(&input).is_err());
        input = base.clone();
        input.adjustments[0].lines[0].account_id = "missing".into();
        assert!(validate(&input).is_err());
        input = base.clone();
        input.accounts[0].group_id = Some("expense".into());
        assert!(validate(&input).is_err());
        input = base.clone();
        input.workpapers[0].depends_on = vec!["wp1".into()];
        assert!(validate(&input).unwrap_err().contains("cycle"));
        input = base.clone();
        let mut next = input.workpapers[0].clone();
        next.id = "wp2".into();
        next.reference = "B".into();
        next.depends_on = vec!["wp1".into()];
        input.workpapers[0].depends_on = vec!["wp2".into()];
        input.workpapers.push(next);
        assert!(validate(&input).unwrap_err().contains("cycle"));
        input = base.clone();
        input.groups[0].gifi_code = Some("2599".into());
        assert!(validate(&input).is_err());
        input.groups[0].gifi_code = Some("9990".into());
        assert!(validate(&input).is_err());
        input.groups[0].gifi_code = Some("1000".into());
        assert!(validate(&input).is_ok());
        assert!(has(&analyze(&input), "gifi_metadata_only"));
        input = base;
        input.accounts.push(input.accounts[0].clone());
        assert!(validate(&input).unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn bounded_schema_rejects_fractional_money_extra_fields_and_long_periods() {
        let input = fixture();
        let mut raw = serde_json::to_value(&input).unwrap();
        raw["accounts"][0]["debit_cents"] = json!(0.5);
        assert!(serde_json::from_value::<EngagementInput>(raw).is_err());
        let mut raw = serde_json::to_value(&input).unwrap();
        raw["accounts"][0]["unexpected"] = true.into();
        assert!(serde_json::from_value::<EngagementInput>(raw).is_err());
        let mut modified = input.clone();
        modified.period_end = "2026-01-02".into();
        assert!(validate(&modified).is_err());
        modified = input.clone();
        modified.adjustments[0].date = "2024-12-31".into();
        assert!(validate(&modified).is_err());
        modified = input.clone();
        modified.workpapers[0].body = "x".repeat(16 * 1024 + 1);
        assert!(validate(&modified).is_err());
        modified = input;
        modified.accounts[0].id = "../cash".into();
        assert!(validate(&modified).is_err());
        assert_eq!(analyze(&modified)["totals"], Value::Null);
    }

    #[test]
    fn empty_drafts_are_valid_but_not_review_ready_and_large_exact_cents_work() {
        let mut input = fixture();
        input.accounts.clear();
        input.adjustments.clear();
        input.workpapers.clear();
        assert!(validate(&input).is_ok());
        assert!(has(&analyze(&input), "empty_trial_balance"));
        assert!(has(&analyze(&input), "missing_workpapers"));
        input = fixture();
        input.adjustments.clear();
        for account in &mut input.accounts {
            account.debit_cents = 0;
            account.credit_cents = 0;
        }
        input.workpapers[0].adjustment_ids.clear();
        input.accounts[0].debit_cents = MAX_AMOUNT;
        input.accounts[4].credit_cents = MAX_AMOUNT;
        let output = analyze(&input);
        assert_eq!(output["supported"], true);
        assert_eq!(output["totals"]["adjusted_debit_cents"], MAX_AMOUNT);
    }

    #[test]
    fn browser_json_boundary_is_exact_and_excess_fails_closed() {
        let mut input = fixture();
        input.accounts.clear();
        input.adjustments.clear();
        input.workpapers[0].account_ids.clear();
        input.workpapers[0].adjustment_ids.clear();
        let mut remaining = i64::try_from(MAX_JSON_CENTS).unwrap();
        let mut index = 0;
        while remaining > 0 {
            let amount = remaining.min(MAX_AMOUNT);
            input.accounts.push(Account {
                id: format!("a{index}"),
                code: format!("A{index:03}"),
                name: "Synthetic asset".into(),
                kind: "asset".into(),
                group_id: Some("asset".into()),
                debit_cents: amount,
                ..Account::default()
            });
            input.accounts.push(Account {
                id: format!("b{index}"),
                code: format!("B{index:03}"),
                name: "Synthetic payable".into(),
                kind: "liability".into(),
                group_id: Some("liability".into()),
                credit_cents: amount,
                ..Account::default()
            });
            remaining -= amount;
            index += 1;
        }
        let output = analyze(&input);
        assert_eq!(output["supported"], true);
        assert_eq!(
            output["totals"]["adjusted_debit_cents"].as_i64().unwrap(),
            9_007_199_254_740_991
        );
        let last = input.accounts.len();
        input.accounts[last - 2].debit_cents += 1;
        input.accounts[last - 1].credit_cents += 1;
        assert!(validate(&input).is_ok());
        let blocked = analyze(&input);
        assert_eq!(blocked["supported"], false);
        assert_eq!(blocked["totals"], Value::Null);
        assert!(
            blocked["diagnostics"][0]["message"]
                .as_str()
                .unwrap()
                .contains("exact integer cents range")
        );
    }

    #[test]
    fn optional_only_papers_do_not_claim_review_readiness() {
        let mut input = fixture();
        input.workpapers[0].required = false;
        assert!(validate(&input).is_ok());
        let output = analyze(&input);
        assert_eq!(output["supported"], false);
        assert!(has(&output, "no_required_workpapers"));
        assert_eq!(output["totals"]["adjusted_difference_cents"], 0);
        input.workpapers[0].required = true;
        assert_eq!(analyze(&input)["supported"], true);
    }
}
