use super::accounting_archive as archive;
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use chrono::NaiveDate;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Sources = BTreeMap<String, String>;
type Result<T> = std::result::Result<T, ApiError>;

const REVISION: &str = "accounting-service-procedures-v1";

pub(super) fn run(
    role: &str,
    effective_on: NaiveDate,
    sources: &Sources,
    required: bool,
) -> Result<Value> {
    let supplemental = super::accounting_additional_procedures::find(role);
    let procedure = match role {
        "Transaction intake and coding" => transaction_population,
        "Accounts payable and expenses" => invoice_duplicates,
        "Accounts receivable and collections" => receivable_aging,
        "Treasury and disbursements" => treasury_cover,
        "Fixed assets, leases, and inventory" => asset_rollforward,
        "Revenue recognition and contract accounting" => contract_cutoff,
        "Consolidation, intercompany, and FX" => intercompany_match,
        "Industry and fund accounting" => fund_restrictions,
        "Payroll processing and exception review" => payroll_net,
        "Payroll reconciliation and corrections" => payroll_settlement,
        "Tax intake and carryforwards" => carryforward_rollforward,
        "Canadian corporate tax" => book_to_tax,
        "US business and pass-through tax" => partner_allocations,
        "Indirect tax" => indirect_tax,
        "Tax filing and authorization coordination" => filing_match,
        "Evidence, confirmations, and sampling" => sampling_and_confirmation,
        "Substantive testing" => assertion_coverage,
        "Audit completion and reporting" => misstatement_review,
        "Agreed-upon procedures" => procedure_coverage,
        "Engagement quality review coordination" => independent_quality_review,
        _ if supplemental.is_some() => {
            if !required && !has_service_inputs(sources) {
                return Ok(json!({"state":"control_only","reason":"service_inputs_not_supplied"}));
            }
            let (passes, findings) = super::accounting_additional_procedures::run(
                supplemental.unwrap(),
                effective_on,
                sources,
            )?;
            return Ok(json!({
                "revision":"accounting-additional-procedures-v1",
                "state":if passes {"prepared_for_review"} else {"procedure_exception"},
                "passes":passes, "findings":findings, "release_authorized":false,
            }));
        }
        _ => {
            if required {
                return Err(archive::error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "service procedure is not available for this role",
                ));
            }
            return Ok(json!({"state":"control_only","reason":"service_procedure_unavailable"}));
        }
    };
    if !required && !has_service_inputs(sources) {
        return Ok(json!({"state":"control_only","reason":"service_inputs_not_supplied"}));
    }
    let (passes, findings) = procedure(effective_on, sources)?;
    Ok(json!({
        "revision":REVISION, "state":if passes {"prepared_for_review"} else {"procedure_exception"},
        "passes": passes, "findings":findings, "release_authorized":false,
    }))
}

fn has_service_inputs(sources: &Sources) -> bool {
    sources.values().any(|text| {
        serde_json::from_str::<Value>(text).is_ok_and(|doc| doc.get("procedure_inputs").is_some())
    })
}

fn fields<T: DeserializeOwned>(sources: &Sources, name: &str) -> Result<T> {
    sources
        .get(name)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|doc| doc.get("procedure_inputs").cloned())
        .and_then(|fields| serde_json::from_value(fields).ok())
        .ok_or_else(|| {
            archive::error(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!("missing or invalid procedure_inputs in {name}"),
            )
        })
}

fn unique(rows: &[String]) -> bool {
    !rows.is_empty()
        && rows.len() <= 5_000
        && rows.iter().all(|id| archive::safe_id(id))
        && rows.iter().collect::<BTreeSet<_>>().len() == rows.len()
}

fn same_set(a: &[String], b: &[String]) -> bool {
    unique(a) && unique(b) && a.iter().collect::<BTreeSet<_>>() == b.iter().collect::<BTreeSet<_>>()
}

fn sum(rows: &[i64]) -> i128 {
    rows.iter().copied().map(i128::from).sum()
}
type Findings = Result<(bool, Value)>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ids {
    ids: Vec<String>,
}

fn transaction_population(_: NaiveDate, sources: &Sources) -> Findings {
    let expected: Ids = fields(sources, "source_manifest")?;
    let coded: Ids = fields(sources, "transaction_register")?;
    let missing: Vec<_> = expected
        .ids
        .iter()
        .filter(|id| !coded.ids.contains(id))
        .collect();
    let extra: Vec<_> = coded
        .ids
        .iter()
        .filter(|id| !expected.ids.contains(id))
        .collect();
    Ok((
        same_set(&expected.ids, &coded.ids),
        json!({"missing":missing,"extra":extra}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invoice {
    supplier: String,
    number: String,
    cents: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invoices {
    invoices: Vec<Invoice>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payees {
    verified_supplier_ids: Vec<String>,
}

fn invoice_duplicates(_: NaiveDate, sources: &Sources) -> Findings {
    let invoices: Invoices = fields(sources, "invoice_register")?;
    let payees: Payees = fields(sources, "payee_verification")?;
    let mut seen = BTreeSet::new();
    let mut exceptions = Vec::new();
    for (row, invoice) in invoices.invoices.iter().enumerate() {
        let supplier = invoice.supplier.trim().to_ascii_lowercase();
        let number = invoice.number.trim().to_ascii_lowercase();
        if !archive::safe_id(&supplier)
            || number.is_empty()
            || number.len() > 80
            || invoice.cents <= 0
            || !seen.insert((supplier.clone(), number))
            || !payees
                .verified_supplier_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&supplier))
        {
            exceptions.push(row + 1);
        }
    }
    let total: i128 = invoices.invoices.iter().map(|i| i128::from(i.cents)).sum();
    Ok((
        !invoices.invoices.is_empty() && exceptions.is_empty(),
        json!({"exception_rows":exceptions,"invoice_cents":total.to_string()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenInvoice {
    due_on: NaiveDate,
    open_cents: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Aging {
    invoices: Vec<OpenInvoice>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Balance {
    balance_cents: i64,
}

fn receivable_aging(as_of: NaiveDate, sources: &Sources) -> Findings {
    let aging: Aging = fields(sources, "aging_schedule")?;
    let ledger: Balance = fields(sources, "cash_application")?;
    let mut buckets = [0_i128; 5];
    for invoice in &aging.invoices {
        let days = (as_of - invoice.due_on).num_days();
        let bucket = match days {
            ..=0 => 0,
            1..=30 => 1,
            31..=60 => 2,
            61..=90 => 3,
            _ => 4,
        };
        buckets[bucket] += i128::from(invoice.open_cents);
    }
    let difference = buckets.iter().sum::<i128>() - i128::from(ledger.balance_cents);
    Ok((
        difference == 0 && aging.invoices.iter().all(|invoice| invoice.open_cents >= 0),
        json!({"buckets_cents":buckets.map(|amount| amount.to_string()),"aging_difference_cents":difference.to_string()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Amounts {
    amounts_cents: Vec<i64>,
}

fn treasury_cover(_: NaiveDate, sources: &Sources) -> Findings {
    let cash: Balance = fields(sources, "cash_forecast")?;
    let payments: Amounts = fields(sources, "payment_register")?;
    let available = i128::from(cash.balance_cents) - sum(&payments.amounts_cents);
    Ok((
        available >= 0
            && !payments.amounts_cents.is_empty()
            && payments.amounts_cents.iter().all(|x| *x > 0),
        json!({"remaining_cents":available.to_string()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Movements {
    opening_cents: i64,
    additions_cents: Vec<i64>,
    removals_cents: Vec<i64>,
    reported_closing_cents: i64,
}

fn asset_rollforward(_: NaiveDate, sources: &Sources) -> Findings {
    let asset: Movements = fields(sources, "asset_register")?;
    let inventory: Movements = fields(sources, "inventory_rollforward")?;
    let calculate = |movement: &Movements| {
        let closing = i128::from(movement.opening_cents) + sum(&movement.additions_cents)
            - sum(&movement.removals_cents);
        let tied = closing >= 0
            && closing == i128::from(movement.reported_closing_cents)
            && movement.opening_cents >= 0
            && movement
                .additions_cents
                .iter()
                .chain(&movement.removals_cents)
                .all(|x| *x >= 0);
        (closing, tied)
    };
    let (asset_closing, assets_tie) = calculate(&asset);
    let (inventory_closing, inventory_ties) = calculate(&inventory);
    Ok((
        assets_tie && inventory_ties,
        json!({
            "asset_closing_cents":asset_closing.to_string(),
            "inventory_closing_cents":inventory_closing.to_string(),
            "valuation_and_leases_require_review":true,
        }),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevenueRow {
    earned_on: NaiveDate,
    cents: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revenue {
    entries: Vec<RevenueRow>,
}

fn contract_cutoff(as_of: NaiveDate, sources: &Sources) -> Findings {
    let entries: Revenue = fields(sources, "contract_register")?;
    let booked: Balance = fields(sources, "cutoff_schedule")?;
    let earned: i128 = entries
        .entries
        .iter()
        .filter(|entry| entry.earned_on <= as_of)
        .map(|entry| i128::from(entry.cents))
        .sum();
    Ok((
        earned == i128::from(booked.balance_cents),
        json!({"earned_through_period_cents":earned.to_string(),
        "uses_service_dates_from_source":true}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Intercompany {
    by_pair_cents: BTreeMap<String, i64>,
}

fn intercompany_match(_: NaiveDate, sources: &Sources) -> Findings {
    let booked: Intercompany = fields(sources, "entity_trial_balances")?;
    let eliminations: Intercompany = fields(sources, "elimination_register")?;
    let nonzero: Vec<_> = booked
        .by_pair_cents
        .keys()
        .chain(eliminations.by_pair_cents.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|key| {
            !archive::safe_id(key)
                || match (
                    booked.by_pair_cents.get(*key),
                    eliminations.by_pair_cents.get(*key),
                ) {
                    (Some(a), Some(b)) => i128::from(*a) + i128::from(*b) != 0,
                    _ => true,
                }
        })
        .collect();
    Ok((
        !booked.by_pair_cents.is_empty() && nonzero.is_empty(),
        json!({"unmatched_pairs":nonzero}),
    ))
}

fn fund_restrictions(_: NaiveDate, sources: &Sources) -> Findings {
    let allowed: Ids = fields(sources, "restriction_terms")?;
    let applied: Ids = fields(sources, "allocation_schedule")?;
    let outside: Vec<_> = applied
        .ids
        .iter()
        .filter(|code| !allowed.ids.contains(code))
        .collect();
    Ok((
        unique(&allowed.ids) && unique(&applied.ids) && outside.is_empty(),
        json!({"outside_restrictions":outside}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payroll {
    gross_cents: i64,
    deductions_cents: Vec<i64>,
    net_cents: i64,
}

fn payroll_net(_: NaiveDate, sources: &Sources) -> Findings {
    let payroll: Payroll = fields(sources, "payroll_register")?;
    let net = i128::from(payroll.gross_cents) - sum(&payroll.deductions_cents);
    Ok((
        net >= 0
            && net == i128::from(payroll.net_cents)
            && payroll.deductions_cents.iter().all(|cents| *cents >= 0),
        json!({"calculated_net_cents":net.to_string()}),
    ))
}

fn payroll_settlement(_: NaiveDate, sources: &Sources) -> Findings {
    let register: Balance = fields(sources, "payroll_register")?;
    let bank: Balance = fields(sources, "bank_settlement")?;
    let clearing: Balance = fields(sources, "ledger_clearing")?;
    Ok((
        register.balance_cents == bank.balance_cents
            && register.balance_cents == clearing.balance_cents,
        json!({"register_to_bank_cents":(i128::from(register.balance_cents)-i128::from(bank.balance_cents)).to_string(),
               "register_to_clearing_cents":(i128::from(register.balance_cents)-i128::from(clearing.balance_cents)).to_string()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Carryforward {
    prior_closing_cents: i64,
    additions_cents: Vec<i64>,
    applied_cents: Vec<i64>,
    reported_closing_cents: i64,
}

fn carryforward_rollforward(_: NaiveDate, sources: &Sources) -> Findings {
    let prior: Balance = fields(sources, "prior_returns")?;
    let schedule: Carryforward = fields(sources, "carryforward_schedule")?;
    let closing = i128::from(prior.balance_cents) + sum(&schedule.additions_cents)
        - sum(&schedule.applied_cents);
    Ok((
        prior.balance_cents >= 0
            && prior.balance_cents == schedule.prior_closing_cents
            && closing >= 0
            && closing == i128::from(schedule.reported_closing_cents)
            && schedule
                .additions_cents
                .iter()
                .chain(&schedule.applied_cents)
                .all(|x| *x >= 0),
        json!({"calculated_closing_cents":closing.to_string(),"eligibility_requires_review":true}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaxBridge {
    increase_cents: Vec<i64>,
    decrease_cents: Vec<i64>,
    reported_taxable_income_cents: i64,
}

fn book_to_tax(_: NaiveDate, sources: &Sources) -> Findings {
    let books: Balance = fields(sources, "corporate_profile")?;
    let schedule: TaxBridge = fields(sources, "book_to_tax_schedule")?;
    let taxable = i128::from(books.balance_cents) + sum(&schedule.increase_cents)
        - sum(&schedule.decrease_cents);
    Ok((
        taxable == i128::from(schedule.reported_taxable_income_cents)
            && schedule
                .increase_cents
                .iter()
                .chain(&schedule.decrease_cents)
                .all(|x| *x >= 0),
        json!({"reconciled_taxable_income_cents":taxable.to_string(),"tax_position_requires_review":true}),
    ))
}

fn partner_allocations(_: NaiveDate, sources: &Sources) -> Findings {
    let classification: Balance = fields(sources, "entity_classification")?;
    let entries: Intercompany = fields(sources, "allocation_register")?;
    let total: i128 = entries
        .by_pair_cents
        .values()
        .copied()
        .map(i128::from)
        .sum();
    Ok((
        total == i128::from(classification.balance_cents)
            && !entries.by_pair_cents.is_empty()
            && entries.by_pair_cents.keys().all(|id| archive::safe_id(id)),
        json!({"allocated_cents":total.to_string(),"basis_and_allocations_require_review":true}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaxCollected {
    collected_cents: i64,
    adjustments_cents: Vec<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetTax {
    claimed_credits_cents: i64,
    reported_net_cents: i64,
}

fn indirect_tax(_: NaiveDate, sources: &Sources) -> Findings {
    let sales: TaxCollected = fields(sources, "taxable_sales_register")?;
    let reconciliation: NetTax = fields(sources, "remittance_reconciliation")?;
    let net = i128::from(sales.collected_cents) + sum(&sales.adjustments_cents)
        - i128::from(reconciliation.claimed_credits_cents);
    Ok((
        net == i128::from(reconciliation.reported_net_cents)
            && reconciliation.claimed_credits_cents >= 0,
        json!({"reconciled_net_tax_cents":net.to_string(),"rates_and_credit_eligibility_require_review":true}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FilingSnapshot {
    artifact_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FilingAuthorization {
    authorized_artifact_sha256: String,
    obtained_on: NaiveDate,
    expires_on: NaiveDate,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FilingDiagnostics {
    blockers: u32,
}

fn filing_match(as_of: NaiveDate, sources: &Sources) -> Findings {
    let frozen: FilingSnapshot = fields(sources, "frozen_return")?;
    let authorization: FilingAuthorization = fields(sources, "filing_authorization")?;
    let diagnostics: FilingDiagnostics = fields(sources, "filing_diagnostics")?;
    let matched = archive::valid_hash(&frozen.artifact_sha256)
        && frozen.artifact_sha256 == authorization.authorized_artifact_sha256;
    Ok((
        matched
            && authorization.obtained_on <= as_of
            && as_of <= authorization.expires_on
            && diagnostics.blockers == 0,
        json!({"artifact_matches":matched,"diagnostic_blockers":diagnostics.blockers,"signature_authenticity_requires_review":true}),
    ))
}

fn sampling_and_confirmation(_: NaiveDate, sources: &Sources) -> Findings {
    let population: Ids = fields(sources, "population_manifest")?;
    let sample: Ids = fields(sources, "sample_register")?;
    let confirmations: Ids = fields(sources, "confirmation_log")?;
    let contained = sample.ids.iter().all(|id| population.ids.contains(id));
    Ok((
        unique(&population.ids) && contained && same_set(&sample.ids, &confirmations.ids),
        json!({"sample_in_population":contained,"population_count":population.ids.len(),"sample_count":sample.ids.len(),
            "representativeness_requires_review":true}),
    ))
}

fn assertion_coverage(_: NaiveDate, sources: &Sources) -> Findings {
    let assertions: Ids = fields(sources, "assertion_map")?;
    let tests: Ids = fields(sources, "testing_workpapers")?;
    let open: FilingDiagnostics = fields(sources, "exceptions_register")?;
    Ok((
        same_set(&assertions.ids, &tests.ids) && open.blockers == 0,
        json!({"required_assertions":assertions.ids.len(),"tested_assertions":tests.ids.len(),"open_exceptions":open.blockers}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Misstatements {
    uncorrected_cents: Vec<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Materiality {
    escalation_cents: i64,
}

fn misstatement_review(_: NaiveDate, sources: &Sources) -> Findings {
    let items: Misstatements = fields(sources, "misstatement_register")?;
    let threshold: Materiality = fields(sources, "completion_checklist")?;
    let absolute_total: i128 = items
        .uncorrected_cents
        .iter()
        .map(|cents| i128::from(*cents).abs())
        .sum();
    Ok((
        threshold.escalation_cents > 0 && absolute_total < i128::from(threshold.escalation_cents),
        json!({"absolute_uncorrected_cents":absolute_total.to_string(),
            "aggregate_signed_cents":sum(&items.uncorrected_cents).to_string(),"opinion_requires_professional_judgment":true}),
    ))
}

fn procedure_coverage(_: NaiveDate, sources: &Sources) -> Findings {
    let agreed: Ids = fields(sources, "agreed_procedures")?;
    let evidence: Ids = fields(sources, "procedure_evidence")?;
    let findings: Ids = fields(sources, "factual_findings")?;
    Ok((
        same_set(&agreed.ids, &evidence.ids) && same_set(&agreed.ids, &findings.ids),
        json!({"agreed_count":agreed.ids.len(),"evidence_count":evidence.ids.len(),"finding_count":findings.ids.len()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reviewer {
    engagement_lead_id: String,
    reviewer_id: String,
    eligible: bool,
}

fn independent_quality_review(_: NaiveDate, sources: &Sources) -> Findings {
    let reviewer: Reviewer = fields(sources, "reviewer_eligibility")?;
    let review_points: FilingDiagnostics = fields(sources, "review_points")?;
    let independent = archive::safe_id(&reviewer.engagement_lead_id)
        && archive::safe_id(&reviewer.reviewer_id)
        && reviewer.engagement_lead_id != reviewer.reviewer_id
        && reviewer.eligible;
    Ok((
        independent && review_points.blockers == 0,
        json!({"separate_eligible_reviewer_on_supplied_records":independent,"open_review_points":review_points.blockers}),
    ))
}
