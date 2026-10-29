//! Independent, bounded Canadian tax workpapers. These calculations do not
//! generate a complete CRA return, an electronic filing record, or certification.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const RULESET: &str = "nxt-ca-2025.1";
pub const T1_SCOPE: &str = "2025 federal and Alberta income-tax workpaper for a full-year Alberta resident, age 18–64, with ordinary Canadian T4 employment/interest only. Verified RRSP deduction and payroll credit/deduction amounts; no other income, deductions, credits, benefits, CPP/EI overpayments, elections, or special situations. Refundable benefits and a complete filing return are outside scope.";
pub const T1_ON_SCOPE: &str = "2025 federal and Ontario income-tax workpaper for a full-year Ontario resident, age 18–64, with ordinary Canadian T4 employment/interest only. Ontario tax follows the published ON428 brackets, 5.05% non-refundable credit rate, basic personal amount, optional spouse amount from a supplied net income, surtax, basic tax reduction, and Ontario health premium. Age, dependant, donation, dividend, LIFT, split-income, and minimum-tax amounts are outside scope. This is not a certified ON428 and does not file.";
pub const T2_SCOPE: &str = "2025 federal and Alberta ordinary active-business tax workpaper, permanent establishments only in Alberta, equal federal/Alberta taxable income, no associations, investment/foreign/partnership income, personal-services business, special industry/rate, credits, refundable taxes or other adjustments. SBD requires verified CCPC eligibility, prior-year taxable capital at most $10 million and adjusted aggregate investment income at most $50,000.";
pub const T3_SCOPE: &str = "2025 federal-only ordinary-income trust workpaper: income less verified deductible expenses and deductible ordinary-income allocations. Ordinary trust rate or verified GRE/QDT graduated rates; no basic personal credit. Provincial tax, capital gains, dividends, AMT, QDT recovery tax, special trusts and other credits/elections are outside scope.";
pub const T5013_SCOPE: &str = "2025 ordinary domestic general-partnership income reconciliation and contractual partner allocation workpaper. Verified general partners and fixed shares only; no limited/at-risk, non-resident, SIFT, multi-tier, special income categories, ACB, CCA, credits or filing XML. Allocation rounding reconciles exactly to net income/loss.";
const MAX_MONEY: i64 = 100_000_000_000_000;
const FEDERAL: &[(i64, i64)] = super::rates::CA_2025.federal_brackets;
const ALBERTA: &[(i64, i64)] = super::rates::CA_2025.alberta_brackets;
/// 2025 Form ON428 Part A ceilings in cents and rates in basis points.
const ONTARIO: &[(i64, i64)] = super::rates::CA_2025.ontario_brackets;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalculationLine { pub code: String, pub label: String, pub amount: i64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic { pub code: String, pub severity: String, pub message: String, pub field: Option<String>, pub blocks_review: bool }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source { pub title: String, pub url: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Calculation {
    pub ruleset: String, pub program: String, pub tax_year: i32, pub jurisdiction: String,
    pub lines: Vec<CalculationLine>, pub totals: BTreeMap<String,i64>, pub diagnostics: Vec<Diagnostic>,
    pub supported: bool, pub scope: String, pub sources: Vec<Source>,
    /// Rate table version and per-line validation status (validated/unvalidated against CRA examples).
    #[serde(default)] pub rate_table: String,
    #[serde(default)] pub line_validation: BTreeMap<String,String>,
}
impl Calculation {
    fn line(&mut self, code: &str, label: &str, amount: i64) { self.line_validation.insert(code.into(), super::rates::line_status(&self.rate_table, code).into()); self.lines.push(CalculationLine { code: code.into(), label: label.into(), amount }); }
    fn total(&mut self, key: &str, amount: i64) { self.totals.insert(key.into(), amount); }
    fn diagnostic(&mut self, code: &str, message: &str, field: Option<&str>, blocking: bool) {
        self.diagnostics.push(Diagnostic { code:code.into(), severity:(if blocking {"error"} else {"info"}).into(), message:message.into(), field:field.map(str::to_owned), blocks_review:blocking });
        if blocking { self.supported = false; }
    }
}
fn source(title: &str, url: &str) -> Source { Source { title:title.into(),url:url.into() } }
pub fn sources(program: &str) -> Vec<Source> {
    let mut s = vec![source("CRA 2025 tax packages", "https://www.canada.ca/en/revenue-agency/services/forms-publications/tax-packages-years/general-income-tax-benefit-package.html")];
    match program {
        "T1" => s.extend([
            source("CRA 2025 federal brackets", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/educational-programs/completing-basic-return.html"),
            source("CRA 2025 T1 return and credit calculation", "https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5009-r/5009-r-25e.pdf"),
            source("CRA basic personal amounts", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-30000-basic-personal-amount.html"),
            source("CRA 2025 Alberta AB428", "https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5009-c/5009-c-25e.pdf"),
            source("CRA Canada employment amount", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-31260-canada-employment-amount.html"),
            source("CRA base CPP contributions", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-30800-cpp-qpp-contributions-through-employment.html"),
            source("CRA enhanced CPP deduction", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-22215-deduction-for-cpp-or-qpp-enhanced-contributions-on-employment-income.html"),
            source("CRA RRSP deduction and verified limit", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-20800-rrsp-deduction.html"),
            source("CRA EI credit eligibility", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-31200-employment-insurance-premiums-through-employment.html"),
            source("CRA 2025 Ontario ON428", "https://www.canada.ca/en/revenue-agency/services/forms-publications/tax-packages-years/general-income-tax-benefit-package/ontario/5006-c.html"),
            source("CRA 2025 provincial rates", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/tax-rates-brackets/last-year.html"),
        ]),
        "T2" => s.extend([
            source("CRA corporation tax rates", "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/corporations/corporation-tax-rates.html"),
            source("CRA SBD eligibility and short-year limit", "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/t4012/t2-corporation-income-tax-guide-chapter-4-page-4-t2-return.html"),
            source("Alberta corporate rates", "https://www.alberta.ca/about-tax-levy-rates-prescribed-interest-rates"),
            source("Alberta corporate tax and short-year threshold", "https://www.alberta.ca/corporate-income-tax"),
        ]),
        "T3" => s.push(source("CRA T3 Trust Guide 2025", "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/t4013/t3-trust-guide.html")),
        "T5013" => s.push(source("CRA T5013 partnership reconciliation and allocation guide", "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/t4068/guide-partnership-information-return-t5013-forms.html")),
        _ => (),
    }
    s
}

/// All money is integer CAD cents. Rates use basis points. Round only at each
/// reported workpaper line, half away from zero; no floating-point arithmetic.
fn rounded_ratio(value: i64, numerator: i64, denominator: i64) -> Result<i64,String> {
    if denominator <= 0 { return Err("Invalid calculation denominator".into()); }
    let n = i128::from(value).checked_mul(i128::from(numerator)).ok_or("Calculation overflow")?;
    let d = i128::from(denominator);
    let rounded = (n.abs() + d/2) / d * n.signum();
    i64::try_from(rounded).map_err(|_| "Calculation overflow".into())
}
fn sum(values: &[i64]) -> Result<i64,String> { values.iter().try_fold(0_i64, |a,b| a.checked_add(*b).ok_or_else(|| "Calculation overflow".into())) }
fn progressive(income: i64, brackets: &[(i64,i64)]) -> Result<i64,String> {
    let mut previous = 0; let mut numerator = 0_i128;
    for &(ceiling, rate) in brackets {
        let portion = income.min(ceiling).saturating_sub(previous).max(0);
        numerator += i128::from(portion) * i128::from(rate);
        if income <= ceiling { break; } previous = ceiling;
    }
    i64::try_from((numerator + 5000) / 10000).map_err(|_| "Calculation overflow".into())
}
fn money(obj: &Map<String,Value>, key: &str) -> Result<i64,String> {
    let n = match obj.get(key) { None => 0, Some(v) => v.as_i64().ok_or_else(|| format!("{key} must be integer CAD cents"))? };
    if !(0..=MAX_MONEY).contains(&n) { return Err(format!("{key} must be between 0 and {MAX_MONEY} cents")); } Ok(n)
}
fn optional_money(obj: &Map<String,Value>, key: &str) -> Result<Option<i64>,String> {
    if !obj.contains_key(key) { return Ok(None); }
    Ok(Some(money(obj, key)?))
}
fn boolean(obj: &Map<String,Value>, key: &str) -> Result<bool,String> { match obj.get(key) { None => Ok(false), Some(v) => v.as_bool().ok_or_else(|| format!("{key} must be boolean")) } }
fn number(obj: &Map<String,Value>, key: &str, default: i64) -> Result<i64,String> { match obj.get(key) { None => Ok(default), Some(v) => v.as_i64().ok_or_else(|| format!("{key} must be an integer")) } }
fn check_fields(obj: &Map<String,Value>, keys: &[&str]) -> Result<(),String> {
    for key in obj.keys() { if !keys.contains(&key.as_str()) { return Err(format!("Unsupported input field: {key}")); } } Ok(())
}
fn require(c: &mut Calculation, obj: &Map<String,Value>, key: &str, message: &str) -> Result<(),String> {
    if !boolean(obj,key)? { c.diagnostic("verification_required",message,Some(key),true); } Ok(())
}

pub fn calculate(program: &str, year: i32, jurisdiction: &str, inputs: &Value) -> Result<Calculation,String> {
    let scope = match (program, jurisdiction) {
        ("T1","AB") => T1_SCOPE,
        ("T1","ON") => T1_ON_SCOPE,
        ("T1", _) => T1_SCOPE,
        ("T2", _) => T2_SCOPE,
        ("T3", _) => T3_SCOPE,
        ("T5013", _) => T5013_SCOPE,
        _ => return Err("Unknown tax program".into()),
    };
    let jurisdiction_ok = matches!((program, jurisdiction), ("T1","AB") | ("T1","ON") | ("T2","AB") | ("T3","CA-FED") | ("T5013","CA"));
    let expected = match program { "T1" => "AB or ON", "T2" => "AB", "T3" => "CA-FED", "T5013" => "CA", _ => "" };
    let obj = inputs.as_object().ok_or("inputs must be an object")?;
    let mut c = Calculation { ruleset:RULESET.into(),program:program.into(),tax_year:year,jurisdiction:jurisdiction.into(),lines:vec![],totals:BTreeMap::new(),diagnostics:vec![],supported:true,scope:scope.into(),sources:sources(program),rate_table:super::rates::CA_2025.id.into(),line_validation:BTreeMap::new() };
    c.diagnostic("workpaper_only","Independent native workpaper; not a complete tax return, CRA-certified software, or filing authorization. Relationship parties are routing metadata; only explicit calculator inputs affect amounts.",None,false);
    if year != 2025 { c.diagnostic("unsupported_year","Only the versioned 2025 calculation scope is implemented.",None,true); }
    if !jurisdiction_ok { c.diagnostic("unsupported_jurisdiction",&format!("This program currently supports {expected} only."),None,true); }
    if year != 2025 || !jurisdiction_ok { return Ok(c); }
    require(&mut c,obj,"scope_confirmed",scope)?;
    if boolean(obj,"other_tax_situations")? { c.diagnostic("unsupported_situation","Additional income, credits, elections or special regimes need a separately implemented ruleset; this calculation cannot be reviewed as supported.",Some("other_tax_situations"),true); }
    match (program, jurisdiction) {
        ("T1","ON") => t1_ontario(&mut c,obj)?,
        ("T1", _) => t1(&mut c,obj)?,
        ("T2", _) => t2(&mut c,obj)?,
        ("T3", _) => t3(&mut c,obj)?,
        ("T5013", _) => partnership(&mut c,obj)?,
        _ => unreachable!(),
    }
    Ok(c)
}

fn t1(c: &mut Calculation, obj: &Map<String,Value>) -> Result<(),String> {
    check_fields(obj,&["scope_confirmed","other_tax_situations","employment_income_cents","interest_income_cents","rrsp_deduction_cents","rrsp_deduction_limit_cents","cpp_base_credit_cents","cpp_enhanced_deduction_cents","ei_credit_cents","tax_withheld_cents","instalments_cents","full_year_resident","payroll_verified","rrsp_verified"])?;
    require(c,obj,"full_year_resident","Confirm full-year Canadian residence and Alberta residence at year-end; newcomer, emigrant and deemed-resident rules are outside scope.")?;
    let employment=money(obj,"employment_income_cents")?; let interest=money(obj,"interest_income_cents")?;
    let rrsp=money(obj,"rrsp_deduction_cents")?; let limit=money(obj,"rrsp_deduction_limit_cents")?;
    let cpp=money(obj,"cpp_base_credit_cents")?; let enhanced=money(obj,"cpp_enhanced_deduction_cents")?; let ei=money(obj,"ei_credit_cents")?;
    if rrsp>limit { return Err("RRSP deduction exceeds the declared verified deduction limit".into()); }
    if rrsp>0 { require(c,obj,"rrsp_verified","Verify the RRSP deduction and available deduction limit against source documents; this engine does not calculate contribution room.")?; }
    if cpp>335_610 || enhanced>107_400 || ei>107_748 { return Err("CPP/EI claim exceeds the applicable 2025 employee maximum".into()); }
    if cpp>0 || enhanced>0 || ei>0 {
        require(c,obj,"payroll_verified","Enter only verified Schedule 8 base CPP/enhanced CPP deduction and eligible EI credit amounts; T4 box 16 is not itself the base CPP credit. No automatic contribution/overpayment calculation is performed.")?;
        if sum(&[cpp,enhanced,ei])?>employment { return Err("Declared payroll claims exceed employment income".into()); }
    }
    if ei>0 && employment<=200_000 { c.diagnostic("ei_refund_required","For this simple standard-insurable-pay scope, EI on earnings of $2,000 or less is an overpayment/refund, not a line 31200 credit; overpayments are outside scope.",Some("ei_credit_cents"),true); }
    let income=sum(&[employment,interest])?; let deductions=sum(&[rrsp,enhanced])?;
    if deductions>income { return Err("Deductions exceed current income; loss and carryforward treatment is outside this scope".into()); }
    let taxable=income-deductions;
    let reduction=rounded_ratio((taxable-17_788_200).clamp(0,7_553_200),159_100,7_553_200)?;
    let bpa=1_612_900-reduction; let employment_amount=employment.min(147_100);
    let federal_before=progressive(taxable,FEDERAL)?;
    let federal_credits=rounded_ratio(sum(&[bpa,employment_amount,cpp,ei])?,1450,10000)?;
    let federal=(federal_before-federal_credits).max(0);
    let provincial_before=progressive(taxable,ALBERTA)?;
    // AB428: credits at 8%, plus the 2025 Alberta supplemental tax credit of 2% on
    // credit amounts above $60,000 (CRA T4127-JUL 2025, factor K5P).
    let ab_credit_base=sum(&[2_232_300,cpp,ei])?;
    let provincial_credits=rounded_ratio(ab_credit_base,800,10000)?;
    let ab_supplemental=rounded_ratio((ab_credit_base-6_000_000).max(0),200,10000)?;
    let provincial=(provincial_before-provincial_credits-ab_supplemental).max(0);
    let tax=sum(&[federal,provincial])?; let paid=sum(&[money(obj,"tax_withheld_cents")?,money(obj,"instalments_cents")?])?;
    for (code,label,amount) in [
        ("10100","Employment income",employment),("12100","Canadian interest income",interest),("15000","Total income",income),
        ("20800","Verified RRSP deduction",rrsp),("22215","Verified enhanced CPP deduction",enhanced),("23600","Net income",taxable),("26000","Taxable income",taxable),
        ("30000","Federal basic personal amount",bpa),("30800","Verified base CPP credit amount",cpp),("31200","Verified EI credit amount",ei),("31260","Canada employment amount",employment_amount),
        ("federal_before_credits","Federal tax before credits",federal_before),("35000","Scoped federal non-refundable credits",federal_credits),("42000","Scoped net federal income tax",federal),
        ("AB58040","Alberta basic personal amount",2_232_300),("ab_before_credits","Alberta tax before credits",provincial_before),("AB61500","Scoped Alberta non-refundable credits",provincial_credits),("ab_supplemental_credit","Alberta supplemental tax credit (2% of credit amounts over $60,000)",ab_supplemental),("42800","Scoped Alberta income tax",provincial),
    ] { c.line(code,label,amount); }
    c.total("taxable_income",taxable); c.total("federal_tax",federal); c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("payments",paid); c.total("balance",tax-paid);
    c.diagnostic("benefits_excluded","Balance covers only the stated income-tax workpaper and payments. Refundable benefits, CWB, GST/HST credits, CPP/EI adjustments and all other return amounts are excluded.",None,false);
    Ok(())
}

fn ontario_health_premium(taxable: i64) -> Result<i64,String> {
    // 2025 ON428 health-premium chart. Bounds are taxable-income cents.
    if taxable <= 2_000_000 { return Ok(0); }
    if taxable <= 2_500_000 { return rounded_ratio(taxable - 2_000_000, 600, 10_000); }
    if taxable <= 3_600_000 { return Ok(30_000); }
    if taxable <= 3_850_000 { return Ok(30_000 + rounded_ratio(taxable - 3_600_000, 600, 10_000)?); }
    if taxable <= 4_800_000 { return Ok(45_000); }
    if taxable <= 4_860_000 { return Ok(45_000 + rounded_ratio(taxable - 4_800_000, 2500, 10_000)?); }
    if taxable <= 7_200_000 { return Ok(60_000); }
    if taxable <= 7_260_000 { return Ok(60_000 + rounded_ratio(taxable - 7_200_000, 2500, 10_000)?); }
    if taxable <= 20_000_000 { return Ok(75_000); }
    if taxable <= 20_060_000 { return Ok(75_000 + rounded_ratio(taxable - 20_000_000, 2500, 10_000)?); }
    Ok(90_000)
}

fn t1_ontario(c: &mut Calculation, obj: &Map<String,Value>) -> Result<(),String> {
    check_fields(obj,&["scope_confirmed","other_tax_situations","employment_income_cents","interest_income_cents","rrsp_deduction_cents","rrsp_deduction_limit_cents","cpp_base_credit_cents","cpp_enhanced_deduction_cents","ei_credit_cents","tax_withheld_cents","instalments_cents","full_year_resident","payroll_verified","rrsp_verified","spouse_net_income_cents"])?;
    require(c,obj,"full_year_resident","Confirm full-year Canadian residence and Ontario residence at year-end; newcomer, emigrant and deemed-resident rules are outside scope.")?;
    let employment=money(obj,"employment_income_cents")?; let interest=money(obj,"interest_income_cents")?;
    let rrsp=money(obj,"rrsp_deduction_cents")?; let limit=money(obj,"rrsp_deduction_limit_cents")?;
    let cpp=money(obj,"cpp_base_credit_cents")?; let enhanced=money(obj,"cpp_enhanced_deduction_cents")?; let ei=money(obj,"ei_credit_cents")?;
    if rrsp>limit { return Err("RRSP deduction exceeds the declared verified deduction limit".into()); }
    if rrsp>0 { require(c,obj,"rrsp_verified","Verify the RRSP deduction and available deduction limit against source documents; this engine does not calculate contribution room.")?; }
    if cpp>335_610 || enhanced>107_400 || ei>107_748 { return Err("CPP/EI claim exceeds the applicable 2025 employee maximum".into()); }
    if cpp>0 || enhanced>0 || ei>0 {
        require(c,obj,"payroll_verified","Enter only verified Schedule 8 base CPP/enhanced CPP deduction and eligible EI credit amounts; T4 box 16 is not itself the base CPP credit. No automatic contribution/overpayment calculation is performed.")?;
        if sum(&[cpp,enhanced,ei])?>employment { return Err("Declared payroll claims exceed employment income".into()); }
    }
    if ei>0 && employment<=200_000 { c.diagnostic("ei_refund_required","For this simple standard-insurable-pay scope, EI on earnings of $2,000 or less is an overpayment/refund, not a line 31200 credit; overpayments are outside scope.",Some("ei_credit_cents"),true); }
    let income=sum(&[employment,interest])?; let deductions=sum(&[rrsp,enhanced])?;
    if deductions>income { return Err("Deductions exceed current income; loss and carryforward treatment is outside this scope".into()); }
    let taxable=income-deductions;
    let reduction=rounded_ratio((taxable-17_788_200).clamp(0,7_553_200),159_100,7_553_200)?;
    let bpa=1_612_900-reduction; let employment_amount=employment.min(147_100);
    let federal_before=progressive(taxable,FEDERAL)?;
    let federal_credits=rounded_ratio(sum(&[bpa,employment_amount,cpp,ei])?,1450,10000)?;
    let federal=(federal_before-federal_credits).max(0);
    let spouse_net=optional_money(obj,"spouse_net_income_cents")?;
    let spouse_amount=match spouse_net {
        None => 0,
        Some(net) => (1_190_500_i64.saturating_sub(net)).clamp(0, 1_082_300),
    };
    let ontario_before=progressive(taxable,ONTARIO)?;
    let ontario_credit_base=sum(&[1_274_700, spouse_amount, cpp, ei])?;
    let ontario_credits=rounded_ratio(ontario_credit_base,505,10000)?;
    let after_credits=(ontario_before-ontario_credits).max(0);
    // ON428 lines 66–68. No dividend tax credit or split income in this scope, so line 65 equals tax after non-refundable credits.
    let surtax_lower=if after_credits>571_000 { rounded_ratio(after_credits-571_000,2000,10000)? } else { 0 };
    let surtax_upper=if after_credits>730_700 { rounded_ratio(after_credits-730_700,3600,10000)? } else { 0 };
    let surtax=sum(&[surtax_lower,surtax_upper])?;
    let after_surtax=sum(&[after_credits,surtax])?;
    let reduction_pool=29_400_i64.saturating_mul(2);
    let tax_reduction=(reduction_pool-after_surtax).max(0);
    let after_reduction=(after_surtax-tax_reduction).max(0);
    let health=ontario_health_premium(taxable)?;
    let provincial=sum(&[after_reduction,health])?;
    let tax=sum(&[federal,provincial])?; let paid=sum(&[money(obj,"tax_withheld_cents")?,money(obj,"instalments_cents")?])?;
    for (code,label,amount) in [
        ("10100","Employment income",employment),("12100","Canadian interest income",interest),("15000","Total income",income),
        ("20800","Verified RRSP deduction",rrsp),("22215","Verified enhanced CPP deduction",enhanced),("23600","Net income",taxable),("26000","Taxable income",taxable),
        ("30000","Federal basic personal amount",bpa),("30800","Verified base CPP credit amount",cpp),("31200","Verified EI credit amount",ei),("31260","Canada employment amount",employment_amount),
        ("federal_before_credits","Federal tax before credits",federal_before),("35000","Scoped federal non-refundable credits",federal_credits),("42000","Scoped net federal income tax",federal),
        ("ON58040","Ontario basic personal amount",1_274_700),("ON58120","Ontario spouse amount from supplied net income",spouse_amount),
        ("on_before_credits","Ontario tax on taxable income",ontario_before),("ON61500","Scoped Ontario non-refundable credits",ontario_credits),
        ("on_surtax","Ontario surtax",surtax),("on_tax_reduction","Ontario basic tax reduction",tax_reduction),("on_health_premium","Ontario health premium",health),("42800","Scoped Ontario income tax",provincial),
    ] { c.line(code,label,amount); }
    c.total("taxable_income",taxable); c.total("federal_tax",federal); c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("payments",paid); c.total("balance",tax-paid);
    c.diagnostic("ontario_scope","Ontario amount uses published 2025 ON428 brackets, the 5.05% credit rate, surtax, the basic reduction only, and the health-premium chart. Dependant reductions, donations, dividends, LIFT, and minimum tax are not calculated. Cent rounding is half away from zero at each workpaper line, which can differ from dollar rounding on the paper form.",None,false);
    c.diagnostic("benefits_excluded","Balance covers only the stated income-tax workpaper and payments. Refundable benefits and all other return amounts are excluded. No return is filed.",None,false);
    Ok(())
}

fn t2(c: &mut Calculation, obj: &Map<String,Value>) -> Result<(),String> {
    check_fields(obj,&["scope_confirmed","other_tax_situations","taxable_income_cents","active_business_income_cents","business_limit_cents","fiscal_days","small_business_eligible","sbd_eligibility_verified","associated_corporations","taxable_capital_cents","passive_income_cents","tax_paid_cents"])?;
    let taxable=money(obj,"taxable_income_cents")?; let abi=money(obj,"active_business_income_cents")?;
    let limit=match obj.get("business_limit_cents") { None=>50_000_000,Some(_)=>money(obj,"business_limit_cents")? };
    if limit>50_000_000 { return Err("Annual business limit cannot exceed $500,000 in this scope".into()); }
    let days=number(obj,"fiscal_days",365)?; if !(1..=366).contains(&days) { return Err("fiscal_days must be between 1 and 366".into()); }
    let small=boolean(obj,"small_business_eligible")?;
    if boolean(obj,"associated_corporations")? { c.diagnostic("associated_corporations_unsupported","Associated-group limits, allocations and multiple year-ends are outside this calculator.",Some("associated_corporations"),true); }
    let capital=money(obj,"taxable_capital_cents")?; let passive=money(obj,"passive_income_cents")?;
    if small {
        require(c,obj,"sbd_eligibility_verified","Verify CCPC status throughout the year, ordinary active-business eligibility, both federal and Alberta business limits, no assigned limit, and prior-year capital/passive-income amounts.")?;
        if capital>1_000_000_000 || passive>5_000_000 { c.diagnostic("sbd_reduction_unsupported","Taxable-capital or passive-income business-limit reductions are outside scope; no reduced-limit claim is validated.",None,true); }
        if abi!=taxable { c.diagnostic("sbd_income_reconciliation","This narrow SBD case requires active-business income equal to taxable income. Other sources or adjustments require an expanded ruleset.",Some("active_business_income_cents"),true); }
    }
    // ITA125(5): shorter than51weeks, not every year shorter than365days.
    let annual_limit=if days<357 { rounded_ratio(limit,days,365)? } else { limit };
    let eligible=if small { taxable.min(abi).min(annual_limit) } else {0};
    let general=taxable-eligible;
    let federal=sum(&[rounded_ratio(eligible,900,10000)?,rounded_ratio(general,1500,10000)?])?;
    let provincial=sum(&[rounded_ratio(eligible,200,10000)?,rounded_ratio(general,800,10000)?])?;
    let tax=sum(&[federal,provincial])?; let paid=money(obj,"tax_paid_cents")?;
    for(code,label,amount)in[("360","Declared federal and Alberta taxable income",taxable),("400","Declared active-business income",abi),("410","Business limit after applicable short-year proration",annual_limit),("sbd_income","Income eligible for scoped lower rates",eligible),("general_income","Income at scoped general rates",general),("federal_tax","Federal Part I tax, stated ordinary-business scope",federal),("alberta_tax","Alberta ordinary-business tax",provincial)]{c.line(code,label,amount);}
    c.total("taxable_income",taxable);c.total("sbd_income",eligible);c.total("federal_tax",federal);c.total("provincial_tax",provincial);c.total("income_tax",tax);c.total("payments",paid);c.total("balance",tax-paid);
    c.diagnostic("taxable_income_supplied","Taxable income and ABI are supplied, verified amounts; book-to-tax reconciliation, CCA, losses, dividends, refundable/Part IV taxes and tax credits are not calculated. Alberta AT1 is a separate filing.",None,false);
    Ok(())
}

fn t3(c: &mut Calculation, obj: &Map<String,Value>) -> Result<(),String> {
    check_fields(obj,&["scope_confirmed","other_tax_situations","trust_type","income_cents","deductible_expenses_cents","beneficiary_allocations_cents","tax_paid_cents","classification_verified"])?;
    let kind=match obj.get("trust_type") {None=>"ordinary",Some(v)=>v.as_str().ok_or("trust_type must be text")?};
    if !["ordinary","gre","qdt"].contains(&kind) {return Err("trust_type must be ordinary, gre, or qdt".into());}
    if kind!="ordinary" {require(c,obj,"classification_verified","Verify GRE designation/36-month eligibility or QDT election/eligibility. The engine does not determine classification or QDT recovery tax.")?;}
    let income=money(obj,"income_cents")?;let expenses=money(obj,"deductible_expenses_cents")?;let allocated=money(obj,"beneficiary_allocations_cents")?;
    if sum(&[expenses,allocated])?>income{return Err("Expenses and deductible allocations exceed ordinary income; loss treatment is outside scope".into());}
    let net=income-expenses;let taxable=net-allocated;
    let tax=if kind=="ordinary"{rounded_ratio(taxable,3300,10000)?}else{progressive(taxable,FEDERAL)?};
    let paid=money(obj,"tax_paid_cents")?;
    for(code,label,amount)in[("ordinary_income","Ordinary trust income",income),("expenses","Verified deductible expenses",expenses),("net_income","Income before beneficiary deduction",net),("beneficiary_deduction","Verified deductible ordinary-income allocations",allocated),("taxable_income","Retained taxable ordinary income",taxable),("federal_tax","Federal income tax before excluded adjustments",tax)]{c.line(code,label,amount);}
    c.total("net_income",net);c.total("beneficiary_allocations",allocated);c.total("taxable_income",taxable);c.total("federal_tax",tax);c.total("payments",paid);c.total("federal_balance",tax-paid);
    c.diagnostic("federal_only","Federal subtotal only. Provincial tax, AMT, QDT recovery, special income categories and complete T3/T3-slip reporting are not calculated. No personal basic exemption is applied.",None,false);
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Partner {id:String,name:String,share_bps:i64}
fn partnership(c:&mut Calculation,obj:&Map<String,Value>)->Result<(),String>{
    check_fields(obj,&["scope_confirmed","other_tax_situations","revenue_cents","expenses_cents","addbacks_cents","deductions_cents","allocation_verified","partners"])?;
    require(c,obj,"allocation_verified","Verify the partnership agreement, ordinary general-partner status, allocation shares, and tax adjustments. This engine does not determine legal entitlement, at-risk limits or partner tax.")?;
    let revenue=money(obj,"revenue_cents")?;let expenses=money(obj,"expenses_cents")?;let addbacks=money(obj,"addbacks_cents")?;let deductions=money(obj,"deductions_cents")?;
    let book=revenue-expenses;let net=sum(&[book,addbacks,-deductions])?;
    let mut partners:Vec<Partner>=serde_json::from_value(obj.get("partners").cloned().unwrap_or(Value::Array(vec![]))).map_err(|_|"partners must be an array of {id,name,share_bps} with integer shares".to_string())?;
    if partners.is_empty()||partners.len()>100{return Err("Enter between 1 and 100 partners".into());}
    let mut ids=BTreeSet::new();let mut shares=0_i64;
    for p in &partners{
        if p.id.is_empty()||p.id.len()>64||!p.id.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'){return Err("Partner IDs must be 1–64 ASCII letters, digits, hyphens or underscores".into());}
        if !ids.insert(p.id.clone()){return Err("Partner IDs must be unique".into());}
        if p.name.trim().is_empty()||p.name.chars().count()>160||p.name.chars().any(char::is_control){return Err("Partner name must be nonempty, at most 160 characters, without controls".into());}
        if !(0..=10000).contains(&p.share_bps){return Err("Each partner share must be 0–10000 basis points".into());}
        shares=shares.checked_add(p.share_bps).ok_or("Share total overflow")?;
    }
    if shares!=10000{return Err("Partner shares must total exactly 10000 basis points (100%)".into());}
    partners.sort_by(|a,b|a.id.cmp(&b.id));
    let magnitude=i128::from(net).abs();
    let mut allocations:Vec<i64>=partners.iter().map(|p|i64::try_from(magnitude*i128::from(p.share_bps)/10000).map_err(|_|"Allocation overflow".to_string())).collect::<Result<_,_>>()?;
    let floor_sum=sum(&allocations)?;let remainder=i64::try_from(magnitude).map_err(|_|"Allocation overflow")?-floor_sum;
    let mut order:Vec<usize>=(0..partners.len()).collect();
    order.sort_by(|&a,&b|{let ar=magnitude*i128::from(partners[a].share_bps)%10000;let br=magnitude*i128::from(partners[b].share_bps)%10000;br.cmp(&ar).then_with(||partners[a].id.cmp(&partners[b].id))});
    for &i in order.iter().take(usize::try_from(remainder).map_err(|_|"Allocation overflow")?){allocations[i]+=1;}
    c.line("book_income","Revenue less book expenses",book);c.line("addbacks","Verified tax addbacks",addbacks);c.line("deductions","Verified additional tax deductions",deductions);c.line("net_income","Ordinary partnership net income/loss",net);
    for(p,amount)in partners.iter().zip(allocations.iter_mut()){*amount*=net.signum();c.line(&format!("partner.{}",p.id),&format!("{} — {} basis points",p.name,p.share_bps),*amount);c.total(&format!("partner.{}",p.id),*amount);}
    let allocated=sum(&allocations)?;if allocated!=net{return Err("Internal allocation reconciliation failed".into());}
    c.total("book_income",book);c.total("net_income",net);c.total("allocated_income",allocated);c.total("unallocated_income",net-allocated);
    c.diagnostic("allocation_workpaper","Partner amounts are ordinary-income allocations, not partner tax payable or complete T5013 slips. Largest remainders receive cents first; ties use stable partner IDs. Routing parties are not used as allocation inputs.",None,false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;use serde_json::json;
    fn calc(p:&str,j:&str,v:Value)->Calculation{calculate(p,2025,j,&v).unwrap()}
    #[test]fn t1_alberta_2025_workpaper_uses_new_rates_and_verified_claims(){
        let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000,"interest_income_cents":100_000,"rrsp_deduction_cents":500_000,"rrsp_deduction_limit_cents":500_000,"rrsp_verified":true,"cpp_base_credit_cents":200_000,"cpp_enhanced_deduction_cents":40_000,"ei_credit_cents":82_000,"payroll_verified":true,"tax_withheld_cents":800_000}));
        assert!(c.supported);assert_eq!(c.totals["taxable_income"],4_560_000);
        // Federal gross6612 -14.5%*(16129+1471+2000+820)=3651.10.
        assert_eq!(c.totals["federal_tax"],365_110);
        // Alberta8%*(45600-22323-2000-820)=1636.56.
        assert_eq!(c.totals["provincial_tax"],163_656);assert_eq!(c.totals["balance"],-271_234);
        let unverified=calc("T1","AB",json!({"employment_income_cents":5_000_000,"cpp_base_credit_cents":200_000}));assert!(!unverified.supported);
    }
    #[test]fn federal_brackets_and_bpa_phaseout_have_exact_boundaries(){
        assert_eq!(progressive(5_737_500,FEDERAL).unwrap(),831_938);assert_eq!(progressive(11_475_000,FEDERAL).unwrap(),2_008_125);assert_eq!(progressive(17_788_200,FEDERAL).unwrap(),3_649_557);assert_eq!(progressive(25_341_400,FEDERAL).unwrap(),5_839_985);
        for(income,bpa)in[(17_788_200,1_612_900),(25_341_400,1_453_800),(21_564_800,1_533_350)]{let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":income}));assert_eq!(c.lines.iter().find(|l|l.code=="30000").unwrap().amount,bpa);}
        assert_eq!(rounded_ratio(1,1,2).unwrap(),1);assert_eq!(rounded_ratio(-1,1,2).unwrap(),-1);
    }
    #[test]fn t2_general_small_and_51week_proration(){
        let general=calc("T2","AB",json!({"scope_confirmed":true,"taxable_income_cents":60_000_000}));assert_eq!(general.totals["income_tax"],13_800_000);
        let small=calc("T2","AB",json!({"scope_confirmed":true,"small_business_eligible":true,"sbd_eligibility_verified":true,"taxable_income_cents":60_000_000,"active_business_income_cents":60_000_000}));assert!(small.supported);assert_eq!(small.totals["income_tax"],7_800_000);
        for(days,limit)in[(181,24_794_521),(356,48_767_123),(357,50_000_000),(366,50_000_000)]{let c=calc("T2","AB",json!({"scope_confirmed":true,"small_business_eligible":true,"sbd_eligibility_verified":true,"taxable_income_cents":60_000_000,"active_business_income_cents":60_000_000,"fiscal_days":days}));assert_eq!(c.totals["sbd_income"],limit);}
        let blocked=calc("T2","AB",json!({"scope_confirmed":true,"small_business_eligible":true,"sbd_eligibility_verified":true,"taxable_capital_cents":1_000_000_001}));assert!(!blocked.supported);
    }
    #[test]fn t3_retained_income_and_gre_do_not_claim_personal_exemption(){
        let ordinary=calc("T3","CA-FED",json!({"scope_confirmed":true,"income_cents":10_000_000,"deductible_expenses_cents":1_000_000,"beneficiary_allocations_cents":3_000_000}));assert!(ordinary.supported);assert_eq!(ordinary.totals["taxable_income"],6_000_000);assert_eq!(ordinary.totals["federal_tax"],1_980_000);
        let gre=calc("T3","CA-FED",json!({"scope_confirmed":true,"classification_verified":true,"trust_type":"gre","income_cents":1_000_000}));assert!(gre.supported);assert_eq!(gre.totals["federal_tax"],145_000);assert!(!gre.totals.contains_key("provincial_tax"));
        let qdt=calc("T3","CA-FED",json!({"scope_confirmed":true,"trust_type":"qdt","income_cents":1_000_000}));assert!(!qdt.supported);
    }
    #[test]fn partnership_largest_remainders_reconcile_signed_cents_and_stable_ids(){
        let partners=json!([{"id":"b","name":"B","share_bps":3333},{"id":"c","name":"C","share_bps":3334},{"id":"a","name":"A","share_bps":3333}]);
        for(revenue,expense,net)in[(101,0,101),(0,101,-101),(0,0,0)]{let c=calc("T5013","CA",json!({"scope_confirmed":true,"allocation_verified":true,"revenue_cents":revenue,"expenses_cents":expense,"partners":partners}));assert!(c.supported);assert_eq!(c.totals["allocated_income"],net);assert_eq!(c.totals["unallocated_income"],0);if net!=0{assert_eq!(c.totals["partner.c"],34*net.signum());assert_eq!(c.totals["partner.a"],34*net.signum());assert_eq!(c.totals["partner.b"],33*net.signum());}}
    }
    #[test]fn invalid_and_unsupported_cases_fail_closed_without_float_or_silent_fields(){
        for v in [json!({"employment_income_cents":1.1}),json!({"employment_income_cents":-1}),json!({"employment_income_cents":i64::MAX}),json!({"mystery_income_cents":100}),json!({"scope_confirmed":"yes"}),json!({"rrsp_deduction_cents":100,"rrsp_deduction_limit_cents":99}),json!({"cpp_base_credit_cents":335611})]{assert!(calculate("T1",2025,"AB",&v).is_err());}
        assert!(!calculate("T1",2024,"AB",&json!({})).unwrap().supported);
        assert!(!calculate("T1",2025,"ON",&json!({})).unwrap().supported);
        assert!(!calculate("T1",2025,"BC",&json!({})).unwrap().supported);
        let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"other_tax_situations":true}));assert!(!c.supported);
        for partners in [json!([]),json!([{"id":"a","name":"A","share_bps":9999}]),json!([{"id":"a","name":"A","share_bps":5000},{"id":"a","name":"A","share_bps":5000}]),json!([{"id":"a","name":"A","share_bps":10000,"extra":true}])]{assert!(calculate("T5013",2025,"CA",&json!({"partners":partners})).is_err());}
    }
    /// Regression suite: hand-computed from CRA published 2025 rates (last-year.html,
    /// T4127-JUL 2025). Synthetic inputs only, no client data.
    #[test]fn regression_ab_on_published_rate_examples(){
        // Federal tax before credits on $80,000 taxable: 57,375*14.5% + 22,625*20.5% = 8,319.38 + 4,638.13 = 12,957.51
        assert_eq!(progressive(8_000_000,FEDERAL).unwrap(),1_295_750);
        // Alberta on $80,000: 60,000*8% + 20,000*10% = 6,800.00
        assert_eq!(progressive(8_000_000,ALBERTA).unwrap(),680_000);
        // Ontario on $80,000: 52,886*5.05% + 27,114*9.15% = 2,670.74 + 2,480.93 = 5,151.67
        assert_eq!(progressive(8_000_000,ONTARIO).unwrap(),515_167);
        // AB $80,000 interest only: federal 12,957.50 - 14.5%*16,129 = 10,618.79; AB 6,800 - 8%*22,323 = 5,014.16
        let ab=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":8_000_000}));
        assert!(ab.supported); assert_eq!(ab.totals["federal_tax"],1_061_879); assert_eq!(ab.totals["provincial_tax"],501_416);
        assert_eq!(ab.line_validation["federal_before_credits"],"validated");
        assert_eq!(ab.lines.iter().find(|l|l.code=="ab_supplemental_credit").unwrap().amount,0);
    }
    #[test]fn alberta_supplemental_credit_applies_above_60000_of_credit_amounts(){
        // Credit base 22,323 + 3,356.10 + 1,077.48 = 26,756.58 < 60,000 -> no supplemental.
        let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"payroll_verified":true,"employment_income_cents":10_000_000,"cpp_base_credit_cents":335_610,"ei_credit_cents":107_748}));
        assert_eq!(c.lines.iter().find(|l|l.code=="ab_supplemental_credit").unwrap().amount,0);
        assert_eq!(rounded_ratio((7_000_000_i64-6_000_000).max(0),200,10000).unwrap(),20_000);
    }
    #[test]fn t1_ontario_2025_uses_on428_brackets_surtax_reduction_and_health_premium(){
        let base=json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":4_000_000});
        let c=calc("T1","ON",base);
        assert!(c.supported, "{:?}", c.diagnostics);
        assert_eq!(c.totals["federal_tax"],346_129);
        assert_eq!(c.lines.iter().find(|l|l.code=="on_health_premium").unwrap().amount,45_000);
        assert_eq!(c.lines.iter().find(|l|l.code=="on_surtax").unwrap().amount,0);
        assert_eq!(c.totals["provincial_tax"],182_628);
        let high=calc("T1","ON",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":12_000_000}));
        assert!(high.supported);
        assert_eq!(high.totals["federal_tax"],1_910_754);
        assert_eq!(high.lines.iter().find(|l|l.code=="on_surtax").unwrap().amount,96_166);
        assert_eq!(high.lines.iter().find(|l|l.code=="on_health_premium").unwrap().amount,75_000);
        assert_eq!(high.totals["provincial_tax"],1_016_554);
        let spouse=calc("T1","ON",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":4_000_000,"spouse_net_income_cents":0}));
        assert_eq!(spouse.lines.iter().find(|l|l.code=="ON58120").unwrap().amount,1_082_300);
        assert!(spouse.totals["provincial_tax"]<c.totals["provincial_tax"]);
        assert!(calculate("T1",2025,"AB",&json!({"scope_confirmed":true,"full_year_resident":true,"spouse_net_income_cents":0})).is_err());
    }
}
