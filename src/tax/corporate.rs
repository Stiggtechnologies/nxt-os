//! Native corporate identity and manually entered GIFI workpapers.
//! Selected total checks are independent of any vendor's file formats/runtime.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::calculations::{Diagnostic, Source};

const MAX_AMOUNT: i64 = 100_000_000_000_000;
const GIFI_SOURCE: &str = "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/rc4088/general-index-financial-information-gifi.html";
const BRIDGE_SOURCE: &str = "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/international-financial-reporting-standards-ifrs/impact-on-filing.html";
pub const SCOPE: &str = "Independent corporate identity and single aggregated GIFI workpaper. Checks selected closing/opening balance-sheet totals and ordinary income-statement totals, then reconciles GIFI 9999 plus verified signed adjustments to declared taxable income. Individual detail codes, schedules, tax adjustments, legal identity and filing eligibility are not independently verified or CRA-certified.";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CorporateProfile {
    pub business_number: String,
    pub rc_account: String,
    pub legal_name: String,
    pub name_changed: bool,
    pub articles_available: bool,
    pub corporation_type: String,
    pub classification_changed_on: Option<String>,
    pub province: String,
    pub address: String,
    pub books_address: String,
    pub officer_name: String,
    pub officer_title: String,
    pub gifi: Vec<GifiRow>,
    pub adjustments: Vec<Adjustment>,
    pub reconciliation_confirmed: bool,
    pub notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GifiRow {
    pub section: String,
    pub code: String,
    pub label: String,
    pub current_cents: i64,
    pub prior_cents: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adjustment {
    pub id: String,
    pub label: String,
    /// Positive adds to book income; negative deducts. No tax adjustment is inferred.
    pub amount_cents: i64,
}

fn text(value: &str, field: &str, max: usize, multiline: bool) -> Result<(), String> {
    if value.len() > max || value.chars().any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\t'))) {
        return Err(format!("{field} exceeds its text bound or contains unsupported control characters"));
    }
    Ok(())
}

fn amount(value: i64) -> bool { (-MAX_AMOUNT..=MAX_AMOUNT).contains(&value) }
fn digits(value: &str, length: usize) -> bool { value.len() == length && value.bytes().all(|b| b.is_ascii_digit()) }

/// Draft identity fields may be empty. Populated values and financial rows are
/// structurally validated, without asserting that an identifier exists at CRA.
pub fn validate(profile: &CorporateProfile) -> Result<(), String> {
    text(&profile.business_number, "business_number", 9, false)?;
    if !profile.business_number.is_empty() && !digits(&profile.business_number, 9) {
        return Err("business_number must be empty or exactly nine digits".into());
    }
    text(&profile.rc_account, "rc_account", 15, false)?;
    if !profile.rc_account.is_empty() {
        if !profile.rc_account.is_ascii() { return Err("RC accounts must contain ASCII letters and digits".into()); }
        let suffix = if profile.rc_account.len() == 15 {
            let (bn, suffix) = profile.rc_account.split_at(9);
            if !digits(bn, 9) { return Err("Full RC account must start with a nine-digit business number".into()); }
            if !profile.business_number.is_empty() && bn != profile.business_number {
                return Err("RC account business number does not match business_number".into());
            }
            suffix
        } else { profile.rc_account.as_str() };
        if suffix.len() != 6 || !suffix.starts_with("RC") || !digits(&suffix[2..], 4) {
            return Err("rc_account must be RC followed by four digits, or the full nine-digit BN plus RC and four digits".into());
        }
    }
    for (value, field, bound, multiline) in [
        (&profile.legal_name, "legal_name", 200, false),
        (&profile.address, "address", 2000, true),
        (&profile.books_address, "books_address", 2000, true),
        (&profile.officer_name, "officer_name", 200, false),
        (&profile.officer_title, "officer_title", 120, false),
        (&profile.notes, "notes", 16_000, true),
    ] { text(value, field, bound, multiline)?; }
    if !["", "ccpc", "other_private", "public", "controlled_by_public", "non_resident", "other"].contains(&profile.corporation_type.as_str()) {
        return Err("Unsupported corporation_type value".into());
    }
    if !["", "AB", "BC", "MB", "NB", "NL", "NS", "NT", "NU", "ON", "PE", "QC", "SK", "YT"].contains(&profile.province.as_str()) {
        return Err("province must be empty or a Canadian province/territory code".into());
    }
    if let Some(value) = &profile.classification_changed_on {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| "classification_changed_on must be a valid YYYY-MM-DD date")?;
        if value.len() != 10 || date.format("%Y-%m-%d").to_string() != *value {
            return Err("classification_changed_on must be a valid YYYY-MM-DD date".into());
        }
    }
    if profile.gifi.len() > 200 { return Err("At most 200 GIFI rows are allowed".into()); }
    let mut codes = BTreeSet::new();
    for row in &profile.gifi {
        if !["balance_sheet", "opening_balance_sheet", "income_statement"].contains(&row.section.as_str()) {
            return Err("GIFI section must be balance_sheet, opening_balance_sheet or income_statement".into());
        }
        if !digits(&row.code, 4) { return Err("GIFI codes must contain exactly four ASCII digits".into()); }
        if !codes.insert((row.section.as_str(), row.code.as_str())) { return Err("Duplicate GIFI section/code".into()); }
        text(&row.label, "GIFI label", 200, false)?;
        if !amount(row.current_cents) || !amount(row.prior_cents) { return Err("GIFI amounts exceed the signed integer-cent bound".into()); }
    }
    if profile.adjustments.len() > 200 { return Err("At most 200 book-to-tax adjustments are allowed".into()); }
    let mut ids = BTreeSet::new();
    for adjustment in &profile.adjustments {
        if adjustment.id.is_empty() || adjustment.id.len() > 80 || !adjustment.id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':')) {
            return Err("Adjustment IDs must be 1–80 ASCII letters, digits, hyphens, underscores, dots or colons".into());
        }
        if !ids.insert(adjustment.id.as_str()) { return Err("Duplicate adjustment id".into()); }
        text(&adjustment.label, "adjustment label", 200, false)?;
        if adjustment.label.trim().is_empty() { return Err("Each adjustment requires a descriptive label".into()); }
        if !amount(adjustment.amount_cents) { return Err("Adjustment amount exceeds the signed integer-cent bound".into()); }
    }
    Ok(())
}

fn diagnostic(diagnostics: &mut Vec<Diagnostic>, code: &str, message: impl Into<String>, field: Option<String>, blocking: bool) {
    diagnostics.push(Diagnostic { code: code.into(), severity: (if blocking { "error" } else { "warning" }).into(), message: message.into(), field, blocks_review: blocking });
}
fn source(title: &str, url: &str) -> Source { Source { title: title.into(), url: url.into() } }
fn sources() -> Vec<Source> { vec![
    source("CRA GIFI required totals, validity checks and net income codes", GIFI_SOURCE),
    source("CRA Schedule 1 starts from GIFI 9999", BRIDGE_SOURCE),
    source("CRA corporation account format", "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/business-registration/business-number-program-account/need-program-accounts/corporation-income-tax.html"),
    source("CRA corporation classification", "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/corporations/type-corporation.html"),
] }

fn pair(profile: &CorporateProfile, section: &str, code: &str) -> Option<(i64, i64)> {
    profile.gifi.iter().find(|r| r.section == section && r.code == code).map(|r| (r.current_cents, r.prior_cents))
}
fn total(totals: &mut BTreeMap<String, i64>, key: &str, value: Option<i64>) { if let Some(value) = value { totals.insert(key.into(), value); } }
fn checked(value: i128) -> Option<i64> { i64::try_from(value).ok().filter(|v| amount(*v)) }
fn selected_section(code: &str) -> Option<&'static str> {
    match code {
        "2599" | "3499" | "3620" | "3640" | "3660" | "3680" | "3700" | "3701" | "3702" | "3720" | "3740" | "3849" => Some("balance"),
        "8299" | "9368" | "9369" | "9659" | "9898" | "9899" | "9970" | "9975" | "9976" | "9980" | "9985" | "9990" | "9995" | "9998" | "9999" => Some("income"),
        _ => None,
    }
}

fn balance_sheet(profile: &CorporateProfile, section: &str, diagnostics: &mut Vec<Diagnostic>, totals: &mut BTreeMap<String, i64>) -> Value {
    let assets = pair(profile, section, "2599");
    let liabilities = pair(profile, section, "3499");
    let equity = pair(profile, section, "3620");
    let complete = assets.is_some() && liabilities.is_some() && equity.is_some();
    for (code, value) in [("2599", assets), ("3499", liabilities), ("3620", equity)] {
        if value.is_none() { diagnostic(diagnostics, "gifi_required_total", format!("{section} requires GIFI {code}, even when its amount is zero."), Some(format!("corporate_profile.gifi.{section}.{code}")), true); }
    }
    let difference = match (assets, liabilities, equity) {
        (Some(a), Some(l), Some(e)) => Some((a.0 - l.0 - e.0, a.1 - l.1 - e.1)),
        _ => None,
    };
    if difference.is_some_and(|d| d != (0, 0)) {
        diagnostic(diagnostics, "gifi_unbalanced", format!("{section}: total assets must equal total liabilities plus shareholder equity in both current and prior columns."), Some(format!("corporate_profile.gifi.{section}")), true);
    }
    if let (Some(a), Some(l), Some(e), Some(t)) = (assets, liabilities, equity, pair(profile, section, "3640")) {
        if t != (l.0 + e.0, l.1 + e.1) || t != a {
            diagnostic(diagnostics, "gifi_total_mismatch", format!("{section}: GIFI 3640 does not reconcile to assets and liabilities plus shareholder equity."), Some(format!("corporate_profile.gifi.{section}.3640")), true);
        }
    }
    for (label, value) in [("assets", assets), ("liabilities", liabilities), ("equity", equity)] {
        total(totals, &format!("{section}_{label}_current"), value.map(|v| v.0));
        total(totals, &format!("{section}_{label}_prior"), value.map(|v| v.1));
    }
    json!({"section":section,"complete":complete,"balanced":difference.map(|d|d==(0,0)),"current_difference_cents":difference.map(|d|d.0),"prior_difference_cents":difference.map(|d|d.1)})
}

fn income_statement(profile: &CorporateProfile, diagnostics: &mut Vec<Diagnostic>, totals: &mut BTreeMap<String, i64>) -> Value {
    let get = |code| pair(profile, "income_statement", code);
    let revenue = get("8299"); let expenses = get("9368"); let after_tax = get("9999");
    for (code, value) in [("8299", revenue), ("9368", expenses), ("9999", after_tax)] {
        if value.is_none() { diagnostic(diagnostics, "gifi_required_total", format!("This non-farming income workpaper requires GIFI {code}, even when zero."), Some(format!("corporate_profile.gifi.income_statement.{code}")), true); }
    }
    // The native T2 scope is ordinary, non-farming active business. Do not silently
    // interpret farming or extraordinary/OCI fields as the same income regime.
    let special = profile.gifi.iter().any(|row| row.section == "income_statement" &&
        (("9370"..="9899").contains(&row.code.as_str()) || ["9975", "9976", "9980", "9985", "9998"].contains(&row.code.as_str())) &&
        (row.current_cents != 0 || row.prior_cents != 0));
    if special { diagnostic(diagnostics, "gifi_special_income", "Farming, extraordinary or other comprehensive income needs additional native rules. Values are retained, but this ordinary-income workpaper cannot validate them.", Some("corporate_profile.gifi.income_statement".into()), true); }
    let ordinary = match (revenue, expenses) { (Some(r), Some(e)) => Some((r.0 - e.0, r.1 - e.1)), _ => None };
    if let Some(net) = ordinary {
        for code in ["9369", "9970"] { if get(code).is_some_and(|v| v != net) {
            diagnostic(diagnostics, "gifi_income_mismatch", format!("GIFI {code} must equal revenue 8299 less expenses 9368 in this non-farming scope."), Some(format!("corporate_profile.gifi.income_statement.{code}")), true);
        } }
    }
    let current_tax = get("9990").unwrap_or((0, 0));
    let deferred_tax = get("9995").unwrap_or((0, 0));
    let expected_after = ordinary.map(|n| (n.0 - current_tax.0 - deferred_tax.0, n.1 - current_tax.1 - deferred_tax.1));
    if !special && expected_after.zip(after_tax).is_some_and(|(expected, actual)| expected != actual) {
        diagnostic(diagnostics, "gifi_after_tax_mismatch", "GIFI 9999 must reconcile to ordinary income less declared current (9990) and deferred (9995) income-tax provisions. Enter any nonzero provisions explicitly; do not substitute pre-tax GIFI 9970 for after-tax GIFI 9999.", Some("corporate_profile.gifi.income_statement.9999".into()), true);
    }
    for (label, value) in [("revenue", revenue), ("expenses", expenses), ("before_tax", ordinary), ("after_tax", after_tax)] {
        total(totals, &format!("income_{label}_current"), value.map(|v| v.0));
        total(totals, &format!("income_{label}_prior"), value.map(|v| v.1));
    }
    json!({"complete":revenue.is_some()&&expenses.is_some()&&after_tax.is_some(),"ordinary_income_current_cents":ordinary.map(|v|v.0),"ordinary_income_prior_cents":ordinary.map(|v|v.1),"after_tax_current_cents":after_tax.map(|v|v.0),"after_tax_prior_cents":after_tax.map(|v|v.1),"expected_after_tax_current_cents":expected_after.map(|v|v.0),"expected_after_tax_prior_cents":expected_after.map(|v|v.1),"special_income_present":special})
}

/// Returns diagnostics without returning identity strings or row/adjustment labels.
/// The caller binds this analysis to the full profile/input hash before review.
pub fn analyze(profile: &CorporateProfile, taxable_income: i64, jurisdiction: &str, small_business_eligible: bool) -> Value {
    let mut diagnostics = Vec::new(); let mut totals = BTreeMap::new();
    if let Err(error) = validate(profile) {
        diagnostic(&mut diagnostics, "corporate_profile_invalid", error, Some("corporate_profile".into()), true);
        return json!({"supported":false,"scope":SCOPE,"diagnostics":diagnostics,"totals":totals,"reconciliation":{"provided":!profile.gifi.is_empty()},"sources":sources()});
    }
    if !amount(taxable_income) || taxable_income < 0 {
        diagnostic(&mut diagnostics, "taxable_income_invalid", "Declared taxable income is outside the nonnegative integer-cent workpaper bound.", Some("taxable_income_cents".into()), true);
        return json!({"supported":false,"scope":SCOPE,"diagnostics":diagnostics,"totals":totals,"reconciliation":{"provided":!profile.gifi.is_empty()},"sources":sources()});
    }
    if !profile.province.is_empty() && profile.province != jurisdiction {
        diagnostic(&mut diagnostics, "corporate_province_mismatch", "Corporate profile province differs from the tax workpaper jurisdiction; multi-jurisdiction allocation is not implemented.", Some("corporate_profile.province".into()), true);
    }
    if small_business_eligible && profile.corporation_type != "ccpc" {
        diagnostic(&mut diagnostics, "corporate_sbd_classification", "Small-business rates require a verified CCPC classification; the corporate profile does not identify this corporation as a CCPC.", Some("corporate_profile.corporation_type".into()), true);
    }
    if profile.corporation_type == "non_resident" || profile.corporation_type == "other" {
        diagnostic(&mut diagnostics, "corporate_special_classification", "This corporate classification needs tax-residency or special-corporation rules beyond the ordinary resident active-business calculator.", Some("corporate_profile.corporation_type".into()), true);
    }
    if profile.name_changed && !profile.articles_available {
        diagnostic(&mut diagnostics, "corporate_name_evidence", "A legal-name change is recorded but supporting articles have not been confirmed available.", Some("corporate_profile.articles_available".into()), true);
    }
    if profile.classification_changed_on.is_some() {
        diagnostic(&mut diagnostics, "corporate_classification_change", "A classification change is recorded. Verify the relevant throughout-year/end-of-year conditions; the recorded date does not automatically establish CCPC eligibility.", Some("corporate_profile.classification_changed_on".into()), false);
    }
    if profile.business_number.is_empty() || profile.legal_name.trim().is_empty() || profile.province.is_empty() {
        diagnostic(&mut diagnostics, "corporate_identity_incomplete", "Some optional corporate identity fields are incomplete. This workpaper does not verify legal identity or produce a filing-ready corporation profile.", Some("corporate_profile".into()), false);
    }
    for (index, row) in profile.gifi.iter().enumerate() {
        match selected_section(&row.code) {
            Some(expected) if (expected == "income") != (row.section == "income_statement") => diagnostic(&mut diagnostics, "gifi_section_mismatch", format!("Selected GIFI code {} is recorded in the wrong statement section.", row.code), Some(format!("corporate_profile.gifi.{index}.section")), true),
            None => diagnostic(&mut diagnostics, "gifi_code_unverified", format!("GIFI code {} is retained as supplied. Its meaning and validity are not verified by this native validator's selected total-code set.", row.code), Some(format!("corporate_profile.gifi.{index}.code")), false),
            _ => (),
        }
    }
    let provided = !profile.gifi.is_empty(); let mut sheets = Vec::new(); let mut income = Value::Null; let mut bridge = Value::Null;
    total(&mut totals, "declared_taxable_income", Some(taxable_income));
    if provided {
        diagnostic(&mut diagnostics, "gifi_selected_checks_only", "Only selected GIFI total relationships and a manual book-to-tax bridge are checked. Detail codes, full financial-statement completeness, multiple income statements, Schedule 1 and Division C calculations are not certified or generated.", Some("corporate_profile.gifi".into()), false);
        sheets.push(balance_sheet(profile, "balance_sheet", &mut diagnostics, &mut totals));
        if profile.gifi.iter().any(|r| r.section == "opening_balance_sheet") { sheets.push(balance_sheet(profile, "opening_balance_sheet", &mut diagnostics, &mut totals)); }
        income = income_statement(profile, &mut diagnostics, &mut totals);
    }
    if provided || !profile.adjustments.is_empty() {
        if !profile.reconciliation_confirmed {
            diagnostic(&mut diagnostics, "corporate_reconciliation_unconfirmed", "Confirm the source financial statements, GIFI 9999 amount and all signed tax adjustments before reviewing the book-to-tax reconciliation.", Some("corporate_profile.reconciliation_confirmed".into()), true);
        }
        let book = pair(profile, "income_statement", "9999").map(|v| v.0);
        let adjustments = checked(profile.adjustments.iter().map(|a| i128::from(a.amount_cents)).sum::<i128>());
        let reconciled = book.zip(adjustments).and_then(|(b, a)| checked(i128::from(b) + i128::from(a)));
        if book.is_none() { diagnostic(&mut diagnostics, "corporate_book_income_missing", "GIFI 9999 after-tax net income is required as the book-to-tax source. Pre-tax code 9970 is not a substitute.", Some("corporate_profile.gifi.income_statement.9999".into()), true); }
        if adjustments.is_none() || (book.is_some() && reconciled.is_none()) {
            diagnostic(&mut diagnostics, "corporate_reconciliation_bound", "The adjustment total or reconciled income exceeds the signed integer-cent workpaper bound.", Some("corporate_profile.adjustments".into()), true);
        }
        if reconciled.is_some_and(|v| v != taxable_income) { diagnostic(&mut diagnostics, "corporate_taxable_income_mismatch", "GIFI 9999 plus signed verified adjustments does not equal the calculator's declared taxable income. No amount has been automatically overwritten.", Some("taxable_income_cents".into()), true); }
        total(&mut totals, "book_income", book); total(&mut totals, "adjustments", adjustments); total(&mut totals, "reconciled_taxable_income", reconciled);
        bridge = json!({"source_code":"9999","book_income_cents":book,"adjustments_cents":adjustments,"reconciled_income_cents":reconciled,"declared_taxable_income_cents":taxable_income,"difference_cents":reconciled.map(|v|v-taxable_income),"confirmed":profile.reconciliation_confirmed,"matches":reconciled.map(|v|v==taxable_income)});
    }
    let supported = !diagnostics.iter().any(|d| d.blocks_review);
    json!({"supported":supported,"scope":SCOPE,"diagnostics":diagnostics,"totals":totals,"reconciliation":{"provided":provided,"balance_sheets":sheets,"income_statement":income,"book_to_tax":bridge},"sources":sources()})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(section: &str, code: &str, current: i64, prior: i64) -> GifiRow { GifiRow { section: section.into(), code: code.into(), label: String::new(), current_cents: current, prior_cents: prior } }
    fn complete() -> CorporateProfile {
        CorporateProfile { legal_name: "Synthetic Example Ltd".into(), business_number: "123456789".into(), rc_account: "RC0001".into(), corporation_type: "ccpc".into(), province: "AB".into(), reconciliation_confirmed: true,
            gifi: vec![row("balance_sheet","2599",10_000_000,8_000_000),row("balance_sheet","3499",3_000_000,2_000_000),row("balance_sheet","3620",7_000_000,6_000_000),row("income_statement","8299",20_000_000,15_000_000),row("income_statement","9368",10_000_000,8_000_000),row("income_statement","9970",10_000_000,7_000_000),row("income_statement","9990",2_300_000,1_610_000),row("income_statement","9999",7_700_000,5_390_000)],
            adjustments: vec![Adjustment { id:"tax-provision".into(),label:"Add back income-tax provision".into(),amount_cents:2_300_000 }], ..Default::default() }
    }
    fn has(value: &Value, code: &str) -> bool { value["diagnostics"].as_array().unwrap().iter().any(|d|d["code"]==code) }
    #[test]
    fn after_tax_book_income_and_explicit_provision_addback_reconcile() {
        let p=complete();validate(&p).unwrap();let a=analyze(&p,10_000_000,"AB",true);
        assert_eq!(a["supported"],true);assert_eq!(a["totals"]["book_income"],7_700_000);assert_eq!(a["totals"]["reconciled_taxable_income"],10_000_000);assert_eq!(a["reconciliation"]["book_to_tax"]["source_code"],"9999");assert_eq!(a["reconciliation"]["balance_sheets"][0]["balanced"],true);
        assert!(!a.to_string().contains("Synthetic Example"));assert!(!a.to_string().contains("123456789"));assert!(!a.to_string().contains("Add back income-tax provision"));
    }
    #[test]
    fn missing_and_unbalanced_totals_block_without_assuming_zero() {
        let mut p=complete();p.gifi.retain(|r|r.code!="3499");let a=analyze(&p,10_000_000,"AB",true);assert_eq!(a["supported"],false);assert!(has(&a,"gifi_required_total"));assert!(a["reconciliation"]["balance_sheets"][0]["balanced"].is_null());
        let mut p=complete();p.gifi[0].prior_cents+=1;let a=analyze(&p,10_000_000,"AB",true);assert!(has(&a,"gifi_unbalanced"));assert_eq!(a["reconciliation"]["balance_sheets"][0]["prior_difference_cents"],1);
    }
    #[test]
    fn income_and_taxable_bridge_mismatches_and_missing_confirmation_block() {
        let mut p=complete();p.gifi.iter_mut().find(|r|r.code=="9999").unwrap().current_cents+=1;let a=analyze(&p,10_000_000,"AB",true);assert!(has(&a,"gifi_after_tax_mismatch"));assert!(has(&a,"corporate_taxable_income_mismatch"));
        let mut p=complete();p.reconciliation_confirmed=false;assert!(has(&analyze(&p,10_000_000,"AB",true),"corporate_reconciliation_unconfirmed"));
        let mut p=complete();p.gifi.retain(|r|r.code!="9999");assert!(has(&analyze(&p,10_000_000,"AB",true),"corporate_book_income_missing"));
    }
    #[test]
    fn signed_adjustments_unknown_codes_and_opening_balances_are_preserved() {
        let mut p=complete();p.adjustments.push(Adjustment{id:"deduction".into(),label:"Verified additional deduction".into(),amount_cents:-100_000});p.gifi.push(row("balance_sheet","1999",42,0));
        for(code,current)in[("2599",100),("3499",120),("3620",-20)]{p.gifi.push(row("opening_balance_sheet",code,current,0));}
        let a=analyze(&p,9_900_000,"AB",true);assert_eq!(a["supported"],true);assert!(has(&a,"gifi_code_unverified"));assert_eq!(a["totals"]["adjustments"],2_200_000);assert_eq!(a["reconciliation"]["balance_sheets"][1]["balanced"],true);assert_eq!(p.gifi.iter().find(|r|r.code=="1999").unwrap().current_cents,42);
        p.adjustments.push(Adjustment{id:"too-large".into(),label:"Bound test".into(),amount_cents:MAX_AMOUNT});assert!(has(&analyze(&p,9_900_000,"AB",true),"corporate_reconciliation_bound"));
    }
    #[test]
    fn identity_classification_and_special_income_are_coherent() {
        let mut p=complete();p.province="ON".into();p.corporation_type="public".into();let a=analyze(&p,10_000_000,"AB",true);assert!(has(&a,"corporate_province_mismatch"));assert!(has(&a,"corporate_sbd_classification"));
        let mut p=complete();p.gifi.push(row("income_statement","9980",1,0));assert!(has(&analyze(&p,10_000_000,"AB",true),"gifi_special_income"));
        let mut p=complete();p.name_changed=true;assert!(has(&analyze(&p,10_000_000,"AB",true),"corporate_name_evidence"));
    }
    #[test]
    fn schema_bounds_and_draft_defaults_fail_closed() {
        let draft=CorporateProfile::default();validate(&draft).unwrap();let a=analyze(&draft,0,"AB",false);assert_eq!(a["supported"],true);assert_eq!(a["reconciliation"]["provided"],false);assert!(a["reconciliation"]["book_to_tax"].is_null());
        assert!(serde_json::from_value::<CorporateProfile>(json!({"secret_extra":"no"})).is_err());assert!(serde_json::from_value::<GifiRow>(json!({"current_cents":1.5})).is_err());
        let mut p=complete();p.rc_account="987654321RC0001".into();assert!(validate(&p).is_err());p.rc_account="123456789RC0001".into();validate(&p).unwrap();
        p.rc_account="12345678éRC000".into();assert!(validate(&p).is_err());p.rc_account="RC0001".into();assert!(has(&analyze(&complete(),i64::MIN,"AB",false),"taxable_income_invalid"));
        p.gifi.push(p.gifi[0].clone());assert!(validate(&p).is_err());p.gifi.pop();p.gifi[0].current_cents=i64::MIN;assert!(validate(&p).is_err());
        let mut p=complete();p.classification_changed_on=Some("2025-02-30".into());assert!(validate(&p).is_err());p.classification_changed_on=Some("2025-02-28".into());validate(&p).unwrap();
        let mut p=complete();p.gifi=vec![row("balance_sheet","2599",0,0);201];assert!(validate(&p).is_err());
    }
}
