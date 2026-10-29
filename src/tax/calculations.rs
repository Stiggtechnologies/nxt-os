//! Independent, bounded Canadian tax workpapers. These calculations do not
//! generate a complete CRA return, an electronic filing record, or certification.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const RULESET: &str = "nxt-ca-2025.1";
pub const T1_SCOPE: &str = "2025 federal and Alberta income-tax workpaper for a full-year Alberta resident, age 18–64, with ordinary Canadian T4 employment/interest only. Verified RRSP deduction and payroll credit/deduction amounts; no other income, deductions, credits, benefits, CPP/EI overpayments, elections, or special situations. Refundable benefits and a complete filing return are outside scope.";
pub const T1_ON_SCOPE: &str = "2025 federal and Ontario income-tax workpaper for a full-year Ontario resident, age 18–64, with ordinary Canadian T4 employment/interest only. Ontario tax follows the published ON428 brackets, 5.05% non-refundable credit rate, basic personal amount, optional spouse amount from a supplied net income, surtax, basic tax reduction, and Ontario health premium. Age, dependant, donation, dividend, LIFT, split-income, and minimum-tax amounts are outside scope. This is not a certified ON428 and does not file.";
pub const T1_PROV_SCOPE: &str = "2025 federal and provincial/territorial (BC, SK, MB, NB, NS, PE, NL, YT, NT, NU) income-tax workpaper for a full-year resident of that province at year-end, age 18-64, with ordinary Canadian T4 employment/interest only. Provincial tax uses the published 2025 brackets, the lowest-rate credit on the provincial basic personal amount plus verified CPP/EI (and the Canada employment amount for Yukon). Provincial reductions, surtaxes and other credits listed in the diagnostics are not calculated and must be confirmed not to apply. Quebec (TP-1) is not supported. This is not a certified provincial 428 and does not file.";
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
            source("CRA line 31270 home buyers' amount", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-31270-home-buyers-amount.html"),
            source("CRA line 31300 adoption expenses", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-31300-adoption-expenses.html"),
            source("CRA line 31900 interest paid on student loans", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-31900-interest-paid-on-your-student-loans.html"),
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

pub const T1_2026_SCOPE: &str = "2026 basic T1 income-tax workpaper (rate table ca-2026.1) for a full-year resident of AB, ON, BC, SK, MB, NB, NS, PE, NL, YT, NT or NU at year-end, with Canadian T4 employment and interest only, a verified RRSP deduction and verified payroll CPP/EI credit amounts. Federal and provincial basic personal amounts, the Canada employment amount and CPP/EI credits only. All other 2025-scope income, deductions and credits are rejected until their 2026 values are sourced. Not a filing.";
/// Fields allowed in the 2026 basic scope; any other non-zero/true 2025 field is rejected.
const T1_2026_FIELDS: &[&str] = &["scope_confirmed","other_tax_situations","employment_income_cents","interest_income_cents","rrsp_deduction_cents","rrsp_deduction_limit_cents","rrsp_verified","cpp_base_credit_cents","cpp_enhanced_deduction_cents","ei_credit_cents","payroll_verified","tax_withheld_cents","instalments_cents","full_year_resident","provincial_adjustments_not_applicable","born_1961_or_earlier","medical_expenses_cents","credits_verified","spouse_adjusted_net_income_cents","family_income_verified"];
/// 2026 basic T1: federal (CRA current-year brackets, 14% credit rate, BPA $16,452 phasing to $14,829
/// between $181,440 and $258,482, CEA $1,501) plus AB or a PROVINCES_2026 province. CPP/EI maxima
/// from T4127-JAN 2026 Tables 8.4-8.7. Anything not sourced for 2026 blocks instead of being guessed.
fn t1_2026(c: &mut Calculation, obj: &Map<String,Value>, code: &str) -> Result<(),String> {
    for (k,v) in obj { if !T1_2026_FIELDS.contains(&k.as_str()) && !(v.is_null() || v==&Value::Bool(false) || v.as_i64()==Some(0)) { return Err(format!("{k} is not supported in the 2026 basic scope (2026 values not yet sourced)")); } }
    if code=="ON" { return t1_2026_on(c,obj); }
    let prov=if code=="AB" { None } else { Some(super::rates::PROVINCES_2026.iter().find(|p| p.code==code).ok_or("No 2026 parameters for this province")?) };
    t1_2026_with(c,obj,prov)
}
fn t1_2026_with(c: &mut Calculation, obj: &Map<String,Value>, prov: Option<&super::rates::Province>) -> Result<(),String> {
    let t=&super::rates::CA_2026;
    let name=prov.map(|p| p.name).unwrap_or("Alberta");
    require(c,obj,"full_year_resident",&format!("Confirm full-year Canadian residence and {name} residence at December 31, 2026."))?;
    if prov.is_some() { require(c,obj,"provincial_adjustments_not_applicable",&format!("2026 {name} reductions, surtaxes and credits other than the basic personal amount and CPP/EI are not calculated. Confirm none apply."))?; }
    let employment=money(obj,"employment_income_cents")?; let interest=money(obj,"interest_income_cents")?;
    let rrsp=money(obj,"rrsp_deduction_cents")?; let limit=money(obj,"rrsp_deduction_limit_cents")?;
    let cpp=money(obj,"cpp_base_credit_cents")?; let enhanced=money(obj,"cpp_enhanced_deduction_cents")?; let ei=money(obj,"ei_credit_cents")?;
    if rrsp>limit { return Err("RRSP deduction exceeds the declared verified deduction limit".into()); }
    if rrsp>0 { require(c,obj,"rrsp_verified","Verify the 2026 RRSP deduction and deduction limit (Notice of Assessment).")?; }
    if cpp>351_945 || enhanced>112_700 || ei>112_307 { return Err("CPP/EI claim exceeds the 2026 employee maximum (T4127-JAN 2026)".into()); }
    if cpp>0 || enhanced>0 || ei>0 {
        require(c,obj,"payroll_verified","Enter only verified Schedule 8 base CPP/enhanced CPP deduction and eligible EI credit amounts.")?;
        if sum(&[cpp,enhanced,ei])?>employment { return Err("Declared payroll claims exceed employment income".into()); }
    }
    let income=sum(&[employment,interest])?; let deductions=sum(&[rrsp,enhanced])?;
    if deductions>income { return Err("Deductions exceed current income; loss and carryforward treatment is outside this scope".into()); }
    let taxable=income-deductions;
    let span=25_848_200-18_144_000;
    let reduction=rounded_ratio((taxable-18_144_000).clamp(0,span),t.federal_bpa_max_cents-t.federal_bpa_min_cents,span)?;
    let bpa=t.federal_bpa_max_cents-reduction; let cea=employment.min(t.canada_employment_amount_cents);
    let federal_before=progressive(taxable,t.federal_brackets)?;
    // CRA indexation page, 2026 column: age amount $9,208 reduced by 15% of net income over $46,432 (ITA 118(2));
    // medical expenses over the lesser of $2,890 and 3% of net income (ITA 118.2(1)).
    let aged=boolean(obj,"born_1961_or_earlier")?; let med=money(obj,"medical_expenses_cents")?;
    if aged || med>0 {
        require(c,obj,"credits_verified","Verify the birth year (1961 or earlier) and medical receipts (12-month period ending in 2026).")?;
        c.diagnostic("provincial_age_medical_2026_unsourced","2026 provincial age and medical expense credit amounts are not yet sourced; only the federal credits are calculated.",None,true);
    }
    let age_amount=if aged { (920_800-rounded_ratio((taxable-4_643_200).max(0),1500,10000)?).max(0) } else { 0 };
    let medical=(med-rounded_ratio(taxable,300,10000)?.min(289_000)).max(0);
    if aged { c.line("30100","Age amount (2026)",age_amount); } if med>0 { c.line("33099","Allowable medical expenses (2026)",medical); }
    let federal_credits=rounded_ratio(sum(&[bpa,cea,cpp,ei,age_amount,medical])?,t.federal_credit_rate_bps,10000)?;
    let federal=(federal_before-federal_credits).max(0);
    let (pbpa,pbefore,pcredits)=match prov {
        None => {
            let base=sum(&[t.alberta_bpa_cents,cpp,ei])?;
            if base>6_000_000 { c.diagnostic("ab_supplemental_2026_unsourced","Alberta credit amounts exceed $60,000; the 2026 Alberta supplemental tax credit is not yet sourced.",None,true); }
            (t.alberta_bpa_cents,progressive(taxable,t.alberta_brackets)?,rounded_ratio(base,800,10000)?)
        }
        Some(p) => {

            // 2025 low-income reductions end below $45,000 in every province; 2026 thresholds are not yet sourced.
            if ["NL","PE","NS","NB"].contains(&p.code) && taxable<6_000_000 { c.diagnostic("low_income_reduction_2026_unsourced",&format!("{} low-income tax reduction thresholds for 2026 are not yet sourced; incomes under $60,000 cannot be reviewed.",p.name),None,true); }
            let mut b=p.bpa_cents.unwrap_or(bpa);
            // T4127-JAN 2026 BPAMB: $15,780 reduced by (NI - $200,000) x 15,780/200,000, nil at $400,000.
            if p.code=="MB" { b-=rounded_ratio((taxable-20_000_000).clamp(0,20_000_000),b,20_000_000)?; } let ce=if p.employment_amount { cea } else { 0 };
            (b,progressive(taxable,p.brackets)?,rounded_ratio(sum(&[b,ce,cpp,ei])?,p.credit_rate_bps,10000)?)
        }
    };
    let pcode=prov.map(|p| p.code).unwrap_or("AB"); let lc=pcode.to_lowercase();
    let mut provincial=(pbefore-pcredits).max(0);
    if pcode=="BC" {
        // T4127-JAN 2026 BC factor S: $575 less 3.56% of net income over $25,570 (nil above $41,722), limited to BC tax.
        let r=(57_500-rounded_ratio((taxable-2_557_000).max(0),356,10000)?).clamp(0,provincial);
        c.line("BC61800","BC tax reduction (T4127-JAN 2026 factor S)",r); provincial-=r;
    }
    let tax=sum(&[federal,provincial])?; let paid=sum(&[money(obj,"tax_withheld_cents")?,money(obj,"instalments_cents")?])?;
    for (code,label,amount) in [
        ("10100".to_string(),"Employment income".to_string(),employment),("12100".into(),"Canadian interest income".into(),interest),("15000".into(),"Total income".into(),income),
        ("20800".into(),"Verified RRSP deduction".into(),rrsp),("22215".into(),"Verified enhanced CPP deduction".into(),enhanced),("23600".into(),"Net income".into(),taxable),("26000".into(),"Taxable income".into(),taxable),
        ("30000".into(),"Federal basic personal amount".into(),bpa),("30800".into(),"Verified base CPP credit amount".into(),cpp),("31200".into(),"Verified EI credit amount".into(),ei),("31260".into(),"Canada employment amount".into(),cea),
        ("federal_before_credits".into(),"Federal tax before credits".into(),federal_before),("35000".into(),"Scoped federal non-refundable credits".into(),federal_credits),("42000".into(),"Scoped net federal income tax".into(),federal),
        (format!("{pcode}58040"),format!("{name} basic personal amount"),pbpa),(format!("{lc}_before_credits"),format!("{name} tax on taxable income"),pbefore),
        (format!("{pcode}61500"),format!("Scoped {name} non-refundable credits"),pcredits),("42800".into(),format!("Scoped {name} income tax"),provincial),
    ] { c.line(&code,&label,amount); }
    c.total("taxable_income",taxable); c.total("federal_tax",federal); c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("payments",paid); c.total("balance",tax-paid);
    c.diagnostic("benefits_excluded","Balance covers only the stated 2026 income-tax workpaper and payments. Refundable benefits and all other return amounts are excluded. No return is filed.",None,false);
    Ok(())
}

/// Ontario 2026 (T4127-JAN 2026, 122nd ed.: Table 8.1 brackets 5.05%/9.15%/11.16%/12.16%/13.16% at
/// $53,891/$107,785/$150,000/$220,000; Table 8.2 BPA $12,989, surtax 20% over $5,818 and 36% over $7,446,
/// tax reduction basic amount $300; Chapter 6 Ontario V1/V2/S formulas incl. Ontario Health Premium).
/// LIFT credit and dependant tax-reduction amounts (Y) are not sourced for 2026 and must be confirmed not to apply.
const ON_2026_BRACKETS: &[(i64,i64)] = &[(5_389_100,505),(10_778_500,915),(15_000_000,1116),(22_000_000,1216),(i64::MAX,1316)];
pub fn on_health_premium_2026(taxable: i64) -> Result<i64,String> {
    let a=taxable;
    Ok(if a<=2_000_000 { 0 } else if a<=3_600_000 { rounded_ratio(a-2_000_000,600,10000)?.min(30_000) }
       else if a<=4_800_000 { (30_000+rounded_ratio(a-3_600_000,600,10000)?).min(45_000) }
       else if a<=7_200_000 { (45_000+rounded_ratio(a-4_800_000,2500,10000)?).min(60_000) }
       else if a<=20_000_000 { (60_000+rounded_ratio(a-7_200_000,2500,10000)?).min(75_000) }
       else { (75_000+rounded_ratio(a-20_000_000,2500,10000)?).min(90_000) })
}
fn t1_2026_on(c: &mut Calculation, obj: &Map<String,Value>) -> Result<(),String> {
    require(c,obj,"provincial_adjustments_not_applicable","2026 dependant amounts for the Ontario tax reduction and Ontario credits other than the basic personal amount and CPP/EI are not calculated. Confirm none apply.")?;
    let on=super::rates::Province { code:"ON", name:"Ontario", brackets:ON_2026_BRACKETS, credit_rate_bps:505, bpa_cents:Some(1_298_900), employment_amount:false, bracket_source:super::rates::CRA_T4127_JAN_2026, bpa_source:super::rates::CRA_T4127_JAN_2026, t4127_kp:&[], not_calculated:"" };
    t1_2026_with(c,obj,Some(&on))?;
    let basic=c.totals["provincial_tax"];
    let surtax=rounded_ratio((basic-581_800).max(0),2000,10000)?+rounded_ratio((basic-744_600).max(0),3600,10000)?;
    let reduction=(2*30_000-(basic+surtax)).clamp(0,basic+surtax);
    let ohp=on_health_premium_2026(c.totals["taxable_income"])?;
    // LIFT (ontario.ca "Low-Income Workers Tax Credit", 2022 onward): lesser of $875 and 5.05% of employment income,
    // less 5% of the greater of adjusted individual net income over $32,500 and adjusted family net income over $65,000.
    let employment=money(obj,"employment_income_cents")?; let spouse=money(obj,"spouse_adjusted_net_income_cents")?; let net=c.totals["taxable_income"];
    let mut lift=0;
    if employment>0 {
        require(c,obj,"family_income_verified","Confirm the spouse/common-law partner's adjusted net income (0 if none) and that you were not confined to a prison for more than six months in 2026 (LIFT credit).")?;
        let excess=(net-3_250_000).max(0).max(sum(&[net,spouse])?-6_500_000);
        lift=(87_500.min(rounded_ratio(employment,505,10000)?)-rounded_ratio(excess,500,10000)?).clamp(0,basic+surtax-reduction);
    }
    c.line("ON_lift","Ontario LIFT credit (ON428-A)",lift);
    let provincial=basic+surtax-reduction-lift+ohp;
    c.line("ON_surtax","Ontario surtax (T4127-JAN 2026 V1)",surtax); c.line("ON_tax_reduction","Ontario tax reduction, basic $300 only (T4127-JAN 2026 S)",reduction);
    c.lines.retain(|l| l.code!="42800"); c.line("ON_health_premium","Ontario Health Premium (T4127-JAN 2026 V2)",ohp); c.line("42800","Scoped Ontario income tax incl. health premium",provincial);
    let federal=c.totals["federal_tax"]; let paid=c.totals["payments"]; let tax=federal+provincial;
    c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("balance",tax-paid);
    Ok(())
}

pub fn calculate(program: &str, year: i32, jurisdiction: &str, inputs: &Value) -> Result<Calculation,String> {
    let scope = match (program, jurisdiction) {
        ("T1","AB") => T1_SCOPE,
        ("T1","ON") => T1_ON_SCOPE,
        ("T1", j) if super::rates::province(j).is_some() => T1_PROV_SCOPE,
        ("T1", _) => T1_SCOPE,
        ("T2", _) => T2_SCOPE,
        ("T3", _) => T3_SCOPE,
        ("T5013", _) => T5013_SCOPE,
        _ => return Err("Unknown tax program".into()),
    };
    let jurisdiction_ok = matches!((program, jurisdiction), ("T1","AB") | ("T1","ON") | ("T1","BC"|"SK"|"MB"|"NB"|"NS"|"PE"|"NL"|"YT"|"NT"|"NU") | ("T2","AB") | ("T3","CA-FED") | ("T5013","CA"));
    let expected = match program { "T1" => "AB, ON, BC, SK, MB, NB, NS, PE, NL, YT, NT or NU (not QC)", "T2" => "AB", "T3" => "CA-FED", "T5013" => "CA", _ => "" };
    let obj = inputs.as_object().ok_or("inputs must be an object")?;
    let mut c = Calculation { ruleset:RULESET.into(),program:program.into(),tax_year:year,jurisdiction:jurisdiction.into(),lines:vec![],totals:BTreeMap::new(),diagnostics:vec![],supported:true,scope:scope.into(),sources:sources(program),rate_table:super::rates::CA_2025.id.into(),line_validation:BTreeMap::new() };
    c.diagnostic("workpaper_only","Independent native workpaper; not a complete tax return, CRA-certified software, or filing authorization. Relationship parties are routing metadata; only explicit calculator inputs affect amounts.",None,false);
    if year == 2026 && program == "T1" && jurisdiction_ok {
        c.rate_table=super::rates::CA_2026.id.into(); c.scope=T1_2026_SCOPE.into();
        c.sources.push(source("CRA current-year (2026) tax rates and brackets",super::rates::CRA_CURRENT_YEAR));
        c.sources.push(source("CRA T4127-JAN 2026",super::rates::CRA_T4127_JAN_2026));
        c.sources.push(source("CRA indexation adjustment amounts",super::rates::CRA_INDEXATION));
        require(&mut c,obj,"scope_confirmed",T1_2026_SCOPE)?;
        if boolean(obj,"other_tax_situations")? { c.diagnostic("unsupported_situation","Additional income, credits, elections or special regimes need a separately implemented ruleset; this calculation cannot be reviewed as supported.",Some("other_tax_situations"),true); }
        t1_2026(&mut c,obj,jurisdiction)?; return Ok(c);
    }
    if year != 2025 { c.diagnostic("unsupported_year",if year==2026 { "2026 is implemented only for the basic T1 scope (not Quebec); T2/T3/T5013 2026 are not wired." } else { "Only the versioned 2025 calculation scope (and the basic 2026 T1 scope) is implemented." },None,true); }
    if !jurisdiction_ok { c.diagnostic("unsupported_jurisdiction",&format!("This program currently supports {expected} only."),None,true); }
    if year != 2025 || !jurisdiction_ok { return Ok(c); }
    require(&mut c,obj,"scope_confirmed",scope)?;
    if boolean(obj,"other_tax_situations")? { c.diagnostic("unsupported_situation","Additional income, credits, elections or special regimes need a separately implemented ruleset; this calculation cannot be reviewed as supported.",Some("other_tax_situations"),true); }
    match (program, jurisdiction) {
        ("T1","ON") => t1_ontario(&mut c,obj)?,
        ("T1", j) if super::rates::province(j).is_some() => t1_province(&mut c,obj,super::rates::province(j).unwrap())?,
        ("T1", _) => t1(&mut c,obj)?,
        ("T2", _) => t2(&mut c,obj)?,
        ("T3", _) => t3(&mut c,obj)?,
        ("T5013", _) => partnership(&mut c,obj)?,
        _ => unreachable!(),
    }
    Ok(c)
}

fn t1(c: &mut Calculation, obj: &Map<String,Value>) -> Result<(),String> {
    check_fields(obj,&["scope_confirmed","other_tax_situations","employment_income_cents","interest_income_cents","rrsp_deduction_cents","rrsp_deduction_limit_cents","rrsp_prior_year_earned_income_cents","rrsp_unused_room_cents","rrsp_pension_adjustment_cents","rrsp_par_cents","rrsp_net_pspa_cents","foreign_non_business_income_cents","foreign_non_business_tax_cents","foreign_tax_verified","cwb_may_apply","cpp_base_credit_cents","cpp_enhanced_deduction_cents","ei_credit_cents","tax_withheld_cents","instalments_cents","full_year_resident","payroll_verified","rrsp_verified","self_employment_gross_cents","self_employment_expenses_cents","self_employment_cca_cents","t2125_verified","se_cpp_deduction_cents","se_cpp_credit_cents","se_cpp_verified","capital_proceeds_cents","capital_acb_cents","capital_outlays_cents","schedule3_verified","rental_gross_cents","rental_expenses_cents","rental_cca_cents","t776_verified","pension_income_cents","pension_split_out_cents","pension_split_in_cents","t1032_verified","medical_expenses_cents","donations_cents","tuition_cents","credits_verified","eligible_dividends_cents","other_dividends_cents","dividends_verified","oas_pension_cents","oas_verified","spouse_net_income_cents","born_1960_or_earlier","disability_tax_credit_approved","home_buyers_amount_cents","adoption_expenses_cents","student_loan_interest_cents","provincial_adjustments_not_applicable"])?;
    require(c,obj,"full_year_resident","Confirm full-year Canadian residence and Alberta residence at year-end; newcomer, emigrant and deemed-resident rules are outside scope.")?;
    let employment=money(obj,"employment_income_cents")?; let interest=money(obj,"interest_income_cents")?;
    let rrsp=money(obj,"rrsp_deduction_cents")?; let limit=money(obj,"rrsp_deduction_limit_cents")?;
    let cpp=money(obj,"cpp_base_credit_cents")?; let enhanced=money(obj,"cpp_enhanced_deduction_cents")?; let ei=money(obj,"ei_credit_cents")?;
    if rrsp>limit { return Err("RRSP deduction exceeds the declared verified deduction limit".into()); }
    rrsp_limit_check(c,obj,limit)?;
    if rrsp>0 { require(c,obj,"rrsp_verified","Verify the RRSP deduction and available deduction limit against source documents; this engine does not calculate contribution room.")?; }
    if cpp>335_610 || enhanced>107_400 || ei>107_748 { return Err("CPP/EI claim exceeds the applicable 2025 employee maximum".into()); }
    if cpp>0 || enhanced>0 || ei>0 {
        require(c,obj,"payroll_verified","Enter only verified Schedule 8 base CPP/enhanced CPP deduction and eligible EI credit amounts; T4 box 16 is not itself the base CPP credit. No automatic contribution/overpayment calculation is performed.")?;
        if sum(&[cpp,enhanced,ei])?>employment { return Err("Declared payroll claims exceed employment income".into()); }
    }
    if ei>0 && employment<=200_000 { c.diagnostic("ei_refund_required","For this simple standard-insurable-pay scope, EI on earnings of $2,000 or less is an overpayment/refund, not a line 31200 credit; overpayments are outside scope.",Some("ei_credit_cents"),true); }
    let x=t1_income_extras(c,obj,employment)?;
    let income=sum(&[employment,interest,x.income])?; let deductions=sum(&[rrsp,enhanced,x.deductions])?;
    if deductions>income { return Err("Deductions exceed current income; loss and carryforward treatment is outside this scope".into()); }
    let net_before=income-deductions;
    // Line 23500 social benefits repayment (Worksheet 5000-D1): 15% over $93,454, capped at OAS received.
    let clawback=rounded_ratio((net_before-9_345_400).max(0),1500,10000)?.min(x.oas);
    let taxable=net_before-clawback;
    let reduction=rounded_ratio((taxable-17_788_200).clamp(0,7_553_200),159_100,7_553_200)?;
    let bpa=1_612_900-reduction; let employment_amount=employment.min(147_100);
    let federal_before=progressive(taxable,FEDERAL)?;
    let federal_credits=rounded_ratio(sum(&[bpa,employment_amount,cpp,ei])?,1450,10000)?;
    let (xcredit,pbase,pdon,xlines)=t1_credit_extras(c,obj,&x,taxable,bpa,"AB")?;
    let federal=(federal_before-federal_credits-xcredit).max(0);
    let federal=t1_foreign_and_flags(c,obj,federal,taxable)?;
    let provincial_before=progressive(taxable,ALBERTA)?;
    // AB428: credits at 8%, plus the 2025 Alberta supplemental tax credit of 2% on
    // credit amounts above $60,000 (CRA T4127-JUL 2025, factor K5P).
    let ab_credit_base=sum(&[2_232_300,cpp,ei,pbase])?;
    let provincial_credits=rounded_ratio(ab_credit_base,800,10000)?;
    let ab_supplemental=rounded_ratio((ab_credit_base-6_000_000).max(0),200,10000)?;
    let provincial=(provincial_before-provincial_credits-pdon-ab_supplemental).max(0);
    let tax=sum(&[federal,provincial,clawback])?; let paid=sum(&[money(obj,"tax_withheld_cents")?,money(obj,"instalments_cents")?])?;
    for (code,label,amount) in [
        ("10100","Employment income",employment),("12100","Canadian interest income",interest),("15000","Total income",income),
        ("20800","Verified RRSP deduction",rrsp),("22215","Verified enhanced CPP deduction",enhanced),("23600","Net income",taxable),("26000","Taxable income",taxable),
        ("30000","Federal basic personal amount",bpa),("30800","Verified base CPP credit amount",cpp),("31200","Verified EI credit amount",ei),("31260","Canada employment amount",employment_amount),
        ("federal_before_credits","Federal tax before credits",federal_before),("35000","Scoped federal non-refundable credits",federal_credits),("42000","Scoped net federal income tax",federal),
        ("AB58040","Alberta basic personal amount",2_232_300),("ab_before_credits","Alberta tax before credits",provincial_before),("AB61500","Scoped Alberta non-refundable credits",provincial_credits),("ab_supplemental_credit","Alberta supplemental tax credit (2% of credit amounts over $60,000)",ab_supplemental),("42800","Scoped Alberta income tax",provincial),
    ] { c.line(code,label,amount); }
    for (code,label,amount) in x.lines.iter().chain(xlines.iter()) { c.line(code,label,*amount); }
    if clawback>0 { c.line("23500","Social benefits repayment (OAS), also added on line 42200",clawback); }
    c.total("taxable_income",taxable); c.total("federal_tax",federal); c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("payments",paid); c.total("balance",tax-paid);
    c.diagnostic("benefits_excluded","Balance covers only the stated income-tax workpaper and payments. Refundable benefits, CWB, GST/HST credits, CPP/EI adjustments and all other return amounts are excluded.",None,false);
    Ok(())
}

fn t1_province(c: &mut Calculation, obj: &Map<String,Value>, p: &super::rates::Province) -> Result<(),String> {
    check_fields(obj,&["scope_confirmed","other_tax_situations","employment_income_cents","interest_income_cents","rrsp_deduction_cents","rrsp_deduction_limit_cents","rrsp_prior_year_earned_income_cents","rrsp_unused_room_cents","rrsp_pension_adjustment_cents","rrsp_par_cents","rrsp_net_pspa_cents","foreign_non_business_income_cents","foreign_non_business_tax_cents","foreign_tax_verified","cwb_may_apply","cpp_base_credit_cents","cpp_enhanced_deduction_cents","ei_credit_cents","tax_withheld_cents","instalments_cents","full_year_resident","payroll_verified","rrsp_verified","self_employment_gross_cents","self_employment_expenses_cents","self_employment_cca_cents","t2125_verified","se_cpp_deduction_cents","se_cpp_credit_cents","se_cpp_verified","capital_proceeds_cents","capital_acb_cents","capital_outlays_cents","schedule3_verified","rental_gross_cents","rental_expenses_cents","rental_cca_cents","t776_verified","pension_income_cents","pension_split_out_cents","pension_split_in_cents","t1032_verified","medical_expenses_cents","donations_cents","tuition_cents","credits_verified","eligible_dividends_cents","other_dividends_cents","dividends_verified","oas_pension_cents","oas_verified","spouse_net_income_cents","born_1960_or_earlier","disability_tax_credit_approved","home_buyers_amount_cents","adoption_expenses_cents","student_loan_interest_cents","provincial_adjustments_not_applicable"])?;
    require(c,obj,"full_year_resident",&format!("Confirm full-year Canadian residence and {} residence at December 31; part-year, multi-jurisdiction and business-income allocation are outside scope.",p.name))?;
    require(c,obj,"provincial_adjustments_not_applicable",&format!("The generic engine does not calculate: {}. Confirm none apply to this return, or prepare those amounts outside NXT.",p.not_calculated))?;
    let employment=money(obj,"employment_income_cents")?; let interest=money(obj,"interest_income_cents")?;
    let rrsp=money(obj,"rrsp_deduction_cents")?; let limit=money(obj,"rrsp_deduction_limit_cents")?;
    let cpp=money(obj,"cpp_base_credit_cents")?; let enhanced=money(obj,"cpp_enhanced_deduction_cents")?; let ei=money(obj,"ei_credit_cents")?;
    if rrsp>limit { return Err("RRSP deduction exceeds the declared verified deduction limit".into()); }
    rrsp_limit_check(c,obj,limit)?;
    if rrsp>0 { require(c,obj,"rrsp_verified","Verify the RRSP deduction and available deduction limit against source documents; this engine does not calculate contribution room.")?; }
    if cpp>335_610 || enhanced>107_400 || ei>107_748 { return Err("CPP/EI claim exceeds the applicable 2025 employee maximum".into()); }
    if cpp>0 || enhanced>0 || ei>0 {
        require(c,obj,"payroll_verified","Enter only verified Schedule 8 base CPP/enhanced CPP deduction and eligible EI credit amounts.")?;
        if sum(&[cpp,enhanced,ei])?>employment { return Err("Declared payroll claims exceed employment income".into()); }
    }
    if ei>0 && employment<=200_000 { c.diagnostic("ei_refund_required","EI on earnings of $2,000 or less is an overpayment/refund, not a line 31200 credit; overpayments are outside scope.",Some("ei_credit_cents"),true); }
    let x=t1_income_extras(c,obj,employment)?;
    let income=sum(&[employment,interest,x.income])?; let deductions=sum(&[rrsp,enhanced,x.deductions])?;
    if deductions>income { return Err("Deductions exceed current income; loss and carryforward treatment is outside this scope".into()); }
    let net_before=income-deductions;
    // Line 23500 social benefits repayment (Worksheet 5000-D1): 15% over $93,454, capped at OAS received.
    let clawback=rounded_ratio((net_before-9_345_400).max(0),1500,10000)?.min(x.oas);
    let taxable=net_before-clawback;
    let reduction=rounded_ratio((taxable-17_788_200).clamp(0,7_553_200),159_100,7_553_200)?;
    let bpa=1_612_900-reduction; let employment_amount=employment.min(147_100);
    let federal_before=progressive(taxable,FEDERAL)?;
    let federal_credits=rounded_ratio(sum(&[bpa,employment_amount,cpp,ei])?,1450,10000)?;
    let (xcredit,pbase,pdon,xlines)=t1_credit_extras(c,obj,&x,taxable,bpa,p.code)?;
    let federal=(federal_before-federal_credits-xcredit).max(0);
    let federal=t1_foreign_and_flags(c,obj,federal,taxable)?;
    if p.code=="MB" && taxable>20_000_000 { c.diagnostic("mb_bpa_phaseout","Manitoba phases out its basic personal amount for net income between $200,000 and $400,000 from 2025; this is not calculated.",None,true); }
    let prov_bpa=p.bpa_cents.unwrap_or(bpa);
    let prov_cea=if p.employment_amount { employment_amount } else { 0 };
    let provincial_before=progressive(taxable,p.brackets)?;
    let provincial_credits=sum(&[rounded_ratio(sum(&[prov_bpa,prov_cea,cpp,ei,pbase])?,p.credit_rate_bps,10000)?,pdon])?;
    let after_credits=(provincial_before-provincial_credits).max(0);
    let low_income=provincial_low_income_reduction(c,obj,p.code,taxable)?;
    let provincial=(after_credits-low_income).max(0);
    let tax=sum(&[federal,provincial,clawback])?; let paid=sum(&[money(obj,"tax_withheld_cents")?,money(obj,"instalments_cents")?])?;
    let lc=p.code.to_lowercase();
    for (code,label,amount) in [
        ("10100".to_string(),"Employment income".to_string(),employment),("12100".into(),"Canadian interest income".into(),interest),("15000".into(),"Total income".into(),income),
        ("20800".into(),"Verified RRSP deduction".into(),rrsp),("22215".into(),"Verified enhanced CPP deduction".into(),enhanced),("23600".into(),"Net income".into(),taxable),("26000".into(),"Taxable income".into(),taxable),
        ("30000".into(),"Federal basic personal amount".into(),bpa),("30800".into(),"Verified base CPP credit amount".into(),cpp),("31200".into(),"Verified EI credit amount".into(),ei),("31260".into(),"Canada employment amount".into(),employment_amount),
        ("federal_before_credits".into(),"Federal tax before credits".into(),federal_before),("35000".into(),"Scoped federal non-refundable credits".into(),federal_credits),("42000".into(),"Scoped net federal income tax".into(),federal),
        (format!("{}58040",p.code),format!("{} basic personal amount",p.name),prov_bpa),(format!("{lc}_before_credits"),format!("{} tax on taxable income",p.name),provincial_before),
        (format!("{}61500",p.code),format!("Scoped {} non-refundable credits",p.name),provincial_credits),(format!("{lc}_low_income_reduction"),format!("{} low-income tax reduction",p.name),low_income),("42800".into(),format!("Scoped {} income tax",p.name),provincial),
    ] { c.line(&code,&label,amount); }
    for (code,label,amount) in x.lines.iter().chain(xlines.iter()) { c.line(code,label,*amount); }
    if clawback>0 { c.line("23500","Social benefits repayment (OAS), also added on line 42200",clawback); }
    c.total("taxable_income",taxable); c.total("federal_tax",federal); c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("payments",paid); c.total("balance",tax-paid);
    c.diagnostic("provincial_scope",&format!("{} tax uses published 2025 brackets ({}) and the {}% credit rate on the basic personal amount and verified CPP/EI. Not calculated: {}.",p.name,p.bracket_source,p.credit_rate_bps as f64/100.0,p.not_calculated),None,false);
    c.diagnostic("benefits_excluded","Balance covers only the stated income-tax workpaper and payments. Refundable benefits and all other return amounts are excluded. No return is filed.",None,false);
    Ok(())
}
/// Additional T1 income and deductions (T2125, Schedule 3, T776, T1032). Every
/// amount must be verified by the preparer; nothing here derives CCA, ACB or
/// eligibility.
pub struct T1Extra { income: i64, deductions: i64, pension_for_credit: i64, se_cpp_credit: i64, elig_div: i64, other_div: i64, oas: i64, lines: Vec<(String,String,i64)> }
/// Federal foreign tax credit on non-business income only (Form T2209, line 40500), plus
/// unsupported-item flags for AMT (T691) and the Canada workers benefit (Schedule 6).
fn t1_foreign_and_flags(c: &mut Calculation, obj: &Map<String,Value>, federal: i64, net: i64) -> Result<i64,String> {
    let fi=money(obj,"foreign_non_business_income_cents")?; let ft=money(obj,"foreign_non_business_tax_cents")?;
    let mut ftc=0;
    if fi>0 || ft>0 {
        require(c,obj,"foreign_tax_verified","Verify foreign non-business income (in CAD, included in income above) and foreign tax paid on it. Business-income FTC, the provincial FTC (T2036), and the section 20(11)/20(12) deductions are not calculated.")?;
        // Tax on property income is limited to 15% of that income (excess handled under 20(11), not calculated).
        let paid=ft.min(rounded_ratio(fi,1500,10000)?);
        let limit=if net>0 { rounded_ratio(federal,fi.min(net),net)? } else { 0 };
        ftc=paid.min(limit);
        c.line("40500","Federal foreign tax credit (non-business, T2209)",ftc);
        if ft>0 { c.diagnostic("provincial_foreign_tax_credit_not_calculated","Provincial foreign tax credit (Form T2036) is not calculated.",Some("foreign_non_business_tax_cents"),false); }
    }
    c.diagnostic("amt_not_calculated","Alternative minimum tax (Form T691) is not calculated. Review if there are large capital gains, donations of securities, tax-shelter losses, or high income with significant credits.",None,false);
    if boolean(obj,"cwb_may_apply")? { c.diagnostic("canada_workers_benefit_not_calculated","Canada workers benefit (Schedule 6) is refundable and not calculated; prepare it outside NXT.",Some("cwb_may_apply"),false); }
    Ok((federal-ftc).max(0))
}

/// RRSP deduction limit cross-check (CRA "How contributions affect your RRSP deduction limit";
/// Guide T4040 Chart 3). When the prior-year inputs are supplied, recomputes the limit as
/// unused room + max(0, min(18% x prior-year earned income, annual dollar limit) - PA) + PAR - net PSPA
/// and blocks review if it disagrees with the declared (Notice of Assessment) limit.
fn rrsp_limit_check(c: &mut Calculation, obj: &Map<String,Value>, declared: i64) -> Result<(),String> {
    if !obj.contains_key("rrsp_prior_year_earned_income_cents") { return Ok(()); }
    let earned=money(obj,"rrsp_prior_year_earned_income_cents")?;
    let unused=match obj.get("rrsp_unused_room_cents") { None=>0, Some(v)=>v.as_i64().filter(|x| x.abs()<=100_000_000_000).ok_or("rrsp_unused_room_cents must be integer cents (may be negative)")? };
    let (pa,par,pspa)=(money(obj,"rrsp_pension_adjustment_cents")?,money(obj,"rrsp_par_cents")?,money(obj,"rrsp_net_pspa_cents")?);
    let new_room=(rounded_ratio(earned,1800,10000)?.min(3_249_000)-pa).max(0);
    let computed=unused+new_room+par-pspa;
    c.line("rrsp_limit_computed","RRSP deduction limit recomputed (T4040 Chart 3)",computed);
    if computed!=declared { c.diagnostic("rrsp_limit_mismatch",&format!("Declared RRSP deduction limit {declared} cents differs from recomputed {computed} cents; check the Notice of Assessment and inputs."),Some("rrsp_deduction_limit_cents"),true); }
    Ok(())
}

fn t1_income_extras(c: &mut Calculation, obj: &Map<String,Value>, employment: i64) -> Result<T1Extra,String> {
    let _ = employment;
    let mut lines: Vec<(String,String,i64)> = vec![];
    let (sg,se,scca)=(money(obj,"self_employment_gross_cents")?,money(obj,"self_employment_expenses_cents")?,money(obj,"self_employment_cca_cents")?);
    let mut se_net=0; let (mut se_ded,mut se_credit)=(0,0);
    if sg>0 || se>0 || scca>0 {
        require(c,obj,"t2125_verified","Verify T2125 gross business/professional income, deductible expenses and CCA (Schedule/Area A) from records. NXT does not calculate CCA, home-office, vehicle or GST/HST adjustments.")?;
        se_net=sg-se-scca;
        if scca>0 && sg-se-scca<0 { return Err("CCA cannot create or increase a business loss".into()); }
        lines.push(("13499".into(),"T2125 gross business income".into(),sg)); lines.push(("13500".into(),"T2125 net business income (loss)".into(),se_net));
        se_ded=money(obj,"se_cpp_deduction_cents")?; se_credit=money(obj,"se_cpp_credit_cents")?;
        if se_net>=500_000 && !boolean(obj,"se_cpp_verified")? { c.diagnostic("self_employed_cpp_not_calculated","Self-employment earnings of $5,000 or more need Schedule 8 CPP on self-employment earnings. NXT does not calculate it: enter verified lines 22200 and 31000 and confirm se_cpp_verified.",Some("se_cpp_verified"),true); }
        if se_ded>0 || se_credit>0 { require(c,obj,"se_cpp_verified","Verify Schedule 8 self-employed CPP amounts (lines 22200 and 31000).")?; }
        lines.push(("22200".into(),"Verified deduction for CPP on self-employment earnings".into(),se_ded)); lines.push(("31000".into(),"Verified CPP on self-employment earnings credit amount".into(),se_credit));
    }
    let (pr,acb,out)=(money(obj,"capital_proceeds_cents")?,money(obj,"capital_acb_cents")?,money(obj,"capital_outlays_cents")?);
    let mut taxable_gain=0;
    if pr>0 || acb>0 || out>0 {
        require(c,obj,"schedule3_verified","Verify Schedule 3 proceeds, ACB and outlays per disposition (T5008/T3/statements). NXT does not track ACB, superficial losses, principal residence, reserves, LCGE or allowable business investment losses.")?;
        let gain=pr-acb-out;
        // 2025 inclusion rate 1/2: the proposed 2/3 rate was cancelled (Department of Finance, 21 March 2025).
        taxable_gain=if gain>0 { rounded_ratio(gain,1,2)? } else { 0 };
        if gain<0 { c.diagnostic("net_capital_loss","A net capital loss is not deductible against other income this year; record 50% as a net capital loss carryforward (not applied by NXT).",None,false); }
        lines.push(("13199".into(),"Schedule 3 net capital gain (loss)".into(),gain)); lines.push(("12700".into(),"Taxable capital gains (50% inclusion)".into(),taxable_gain));
    }
    let (rg,re,rcca)=(money(obj,"rental_gross_cents")?,money(obj,"rental_expenses_cents")?,money(obj,"rental_cca_cents")?);
    let mut rental=0;
    if rg>0 || re>0 || rcca>0 {
        require(c,obj,"t776_verified","Verify T776 gross rents, current expenses (not capital) and CCA. NXT does not calculate CCA, co-ownership shares or personal-use adjustments.")?;
        if rcca>0 && rg-re-rcca<0 { return Err("CCA cannot create or increase a rental loss".into()); }
        rental=rg-re-rcca;
        lines.push(("12599".into(),"T776 gross rental income".into(),rg)); lines.push(("12600".into(),"T776 net rental income (loss)".into(),rental));
    }
    let (pen,out_split,in_split)=(money(obj,"pension_income_cents")?,money(obj,"pension_split_out_cents")?,money(obj,"pension_split_in_cents")?);
    if pen>0 || out_split>0 || in_split>0 {
        if out_split>0 || in_split>0 { require(c,obj,"t1032_verified","Verify the joint T1032 election: both spouses sign, the transferring spouse's eligible pension income, and the same amount on both returns.")?; }
        if out_split>rounded_ratio(pen,1,2)? { return Err("Split-pension deduction (line 21000) cannot exceed 50% of eligible pension income".into()); }
        lines.push(("11500".into(),"Verified eligible pension income".into(),pen)); lines.push(("11600".into(),"Elected split-pension amount received (T1032)".into(),in_split)); lines.push(("21000".into(),"Deduction for elected split-pension amount (T1032)".into(),out_split));
    }
    // Dividends: actual amounts (T5 box 24 eligible, box 10 other) grossed up 138% / 115% (line 12000/12010).
    let (ed,od)=(money(obj,"eligible_dividends_cents")?,money(obj,"other_dividends_cents")?);
    let (elig_div,other_div)=(rounded_ratio(ed,138,100)?,rounded_ratio(od,115,100)?);
    if ed>0 || od>0 {
        require(c,obj,"dividends_verified","Verify actual (not taxable) amounts of eligible and other-than-eligible dividends from taxable Canadian corporations (T5 boxes 24/10, T3 boxes 49/23). Foreign dividends do not qualify.")?;
        lines.push(("12010".into(),"Taxable amount of other than eligible dividends (115%)".into(),other_div)); lines.push(("12000".into(),"Taxable amount of dividends (eligible 138% + other 115%)".into(),sum(&[elig_div,other_div])?));
    }
    let oas=money(obj,"oas_pension_cents")?;
    if oas>0 { require(c,obj,"oas_verified","Verify T4A(OAS) box 18 OAS pension; recovery tax already withheld (box 22) must be entered as tax withheld. Net federal supplements and EI benefit repayments are not handled.")?; lines.push(("11300".into(),"Old age security pension".into(),oas)); }
    let income=sum(&[se_net,taxable_gain,rental,pen,in_split,elig_div,other_div,oas])?;
    Ok(T1Extra{income,deductions:sum(&[out_split,se_ded])?,pension_for_credit:(pen-out_split).max(0)+in_split,se_cpp_credit:se_credit,elig_div,other_div,oas,lines})
}
/// Federal credits for medical (33099), donations (34900), pension (31400), tuition (32300).
/// 2025 low-income tax reductions from the BC, NL, PE, NS and NB 428 forms. Family income is
/// the filer's net income plus the supplied spouse net income. Child/dependant add-ons and
/// spouse transfers of unused amounts are not handled (tax can only be overstated).
fn provincial_low_income_reduction(c: &mut Calculation, obj: &Map<String,Value>, code: &str, net: i64) -> Result<i64,String> {
    let spouse=optional_money(obj,"spouse_net_income_cents")?;
    let family=sum(&[net,spouse.unwrap_or(0).max(0)])?;
    let aged=obj.get("born_1960_or_earlier").is_some() && boolean(obj,"born_1960_or_earlier")?;
    let (pool,income,base,rate)=match code {
        "BC" => (56_200, net, 2_502_000, 356),
        "NL" => (if spouse.is_some() { 155_400 } else { 99_700 }, family, if spouse.is_some() { 4_046_000 } else { 2_392_800 }, 1600),
        "PE" => (35_000+if aged { 25_000 } else { 0 }+if spouse.is_some() { 35_000 } else { 0 }, family, 2_265_000, 500),
        "NS" => (if spouse.is_some() { 60_000 } else { 30_000 }, family, 1_500_000, 500),
        "NB" => (if spouse.is_some() { 160_400 } else { 80_200 }, family, 2_192_000, 300),
        _ => return Ok(0),
    };
    let r=(pool-rounded_ratio((income-base).max(0),rate,10000)?).max(0);
    if r>0 { c.diagnostic("low_income_reduction_scope",&format!("{code} low-income tax reduction uses the basic and spouse amounts only; eligible-dependant, dependent-child and spouse-age add-ons and transfers of unused amounts are not calculated (tax may be overstated)."),None,false); }
    Ok(r)
}
/// Returns (federal credit, provincial credit amounts at the provincial rate, provincial donation credit, lines).
fn t1_credit_extras(c: &mut Calculation, obj: &Map<String,Value>, x: &T1Extra, net: i64, bpa: i64, code: &str) -> Result<(i64,i64,i64,Vec<(String,String,i64)>),String> {
    let (med,don,tui)=(money(obj,"medical_expenses_cents")?,money(obj,"donations_cents")?,money(obj,"tuition_cents")?);
    let pension_amount=if x.pension_for_credit>0 { x.pension_for_credit.min(200_000) } else { 0 };
    if pension_amount>0 { c.diagnostic("pension_amount_eligibility","Line 31400 eligibility depends on age and pension type (age 65+ for most RRIF/annuity income). Verify that the pension income is eligible.",None,false); }
    if med>0 || don>0 || tui>0 { require(c,obj,"credits_verified","Verify medical receipts (12-month period), official donation receipts and the T2202 eligible tuition (current year only; carryforwards and transfers are not handled).")?; }
    let med_threshold=rounded_ratio(net,300,10000)?.min(283_400);
    let medical=(med-med_threshold).max(0);
    if don>rounded_ratio(net,7500,10000)? { return Err("Donations exceed 75% of net income; carryforward is outside scope".into()); }
    let first=don.min(20_000); let rest=don-first;
    let at33=rest.min((net-25_341_400).max(0));
    let donation_credit=sum(&[rounded_ratio(first,1450,10000)?,rounded_ratio(rest-at33,2900,10000)?,rounded_ratio(at33,3300,10000)?])?;
    // Age (30100), spouse (30300) and disability (31600) amounts; inputs are preparer-verified facts.
    let flag=|k:&str| -> Result<bool,String> { Ok(obj.get(k).is_some() && boolean(obj,k)?) };
    let aged=flag("born_1960_or_earlier")?; let disabled=flag("disability_tax_credit_approved")?;
    let spouse_net=optional_money(obj,"spouse_net_income_cents")?;
    if aged || disabled || spouse_net.is_some() { require(c,obj,"credits_verified","Verify the birth year (1960 or earlier), CRA-approved T2201 for an adult, and the spouse's line 23600 net income. Infirmity supplements, eligible dependant and caregiver amounts are not handled.")?; }
    let age_amount=if aged { (902_800-rounded_ratio((net-4_552_200).max(0),1500,10000)?).max(0) } else { 0 };
    let spouse_fed=spouse_net.map(|n| (bpa-n.max(0)).max(0)).unwrap_or(0);
    let disability=if disabled { 1_013_800 } else { 0 };
    // Home buyers' (31270, max $10,000, ITA 118.05), adoption (31300, 2025 max $19,580 per child, ITA 118.01),
    // student loan interest (31900, ITA 118.62). Amounts are preparer-verified.
    let (hb,adopt,sli)=(money(obj,"home_buyers_amount_cents")?,money(obj,"adoption_expenses_cents")?,money(obj,"student_loan_interest_cents")?);
    if hb>0 || adopt>0 || sli>0 { require(c,obj,"credits_verified","Verify home buyers' eligibility (qualifying home, first-time buyer or DTC exception; split total cannot exceed $10,000), adoption expenses net of reimbursements for the adoption period ending this year (one child), and student loan interest receipts (Canadian government student loans only; carryforwards not handled).")?; }
    if hb>1_000_000 { return Err("home_buyers_amount_cents exceeds the $10,000 maximum for 2025".into()); }
    if adopt>1_958_000 { return Err("adoption_expenses_cents exceeds the 2025 per-child maximum of $19,580; multiple children are outside scope".into()); }
    if adopt>0 || sli>0 { c.diagnostic("provincial_adoption_student_loan_not_calculated","Provincial adoption (58330) and student loan interest (58520) amounts are not calculated; provincial tax may be overstated.",None,false); }
    let credit_base=sum(&[medical,pension_amount,tui,x.se_cpp_credit,age_amount,spouse_fed,disability,hb,adopt,sli])?;
    let fed_dtc=sum(&[rounded_ratio(x.elig_div,150_198,1_000_000)?,rounded_ratio(x.other_div,90_301,1_000_000)?])?;
    let credit=sum(&[rounded_ratio(credit_base,1450,10000)?,donation_credit,fed_dtc])?;
    // Provincial counterparts from the 2025 428 form and worksheet (rates::PROV_CREDITS_2025).
    let p=super::rates::prov_credits_2025(code).ok_or("No 2025 provincial credit parameters")?;
    let p_medical=(med-rounded_ratio(net,300,10000)?.min(p.medical_max_cents)).max(0);
    let p_pension=x.pension_for_credit.max(0).min(p.pension_max_cents);
    let p_tuition=if p.current_year_tuition { tui } else { 0 };
    if tui>0 && !p.current_year_tuition { c.diagnostic("provincial_tuition_none",&format!("{} has no current-year tuition credit for 2025; only unused carryforward amounts (not handled) apply.",code),Some("tuition_cents"),false); }
    if tui>0 && p.current_year_tuition { c.diagnostic("provincial_education_amount","Provincial monthly education/textbook amounts and tuition carryforwards are not calculated; claim only reduces tax if they are nil.",Some("tuition_cents"),false); }
    let p_age=if !aged { 0 } else if p.age_max_cents==0 { age_amount } else { (p.age_max_cents-rounded_ratio((net-p.age_threshold_cents).max(0),1500,10000)?).max(0) };
    let p_spouse=match spouse_net { None=>0, Some(n) if p.spouse_base_cents==0 => { let _=n; spouse_fed }, Some(n) => { let floor=if code=="NS" { 87_400 } else { 0 }; (p.spouse_base_cents-n.max(floor)).clamp(0,p.spouse_max_cents) } };
    let p_disability=if !disabled { 0 } else if p.disability_cents==0 { disability } else { p.disability_cents };
    let p_base=sum(&[p_medical,p_pension,p_tuition,x.se_cpp_credit,p_age,p_spouse,p_disability])?;
    let p_top=match p.donation_top { Some((thr,_))=>rest.min((net-thr).max(0)), None=>0 };
    let p_don=sum(&[rounded_ratio(first,p.donation_low_bps,10000)?,rounded_ratio(rest-p_top,p.donation_high_bps,10000)?,match p.donation_top { Some((_,r))=>rounded_ratio(p_top,r,10000)?, None=>0 }])?;
    let p_dtc=sum(&[rounded_ratio(x.elig_div,p.dtc_ppm.0,1_000_000)?,rounded_ratio(x.other_div,p.dtc_ppm.1,1_000_000)?])?;
    let p_don=sum(&[p_don,p_dtc])?;
    let lc=code.to_lowercase();
    let mut lines=vec![(format!("{code}58080"),format!("{code} age amount"),p_age),(format!("{code}58120"),format!("{code} spouse or common-law partner amount"),p_spouse),(format!("{code}58440"),format!("{code} disability amount"),p_disability),(format!("{code}58360"),format!("{code} pension income amount"),p_pension),(format!("{code}58560"),format!("{code} current-year tuition amount"),p_tuition),
        (format!("{code}58689"),format!("{code} allowable medical expenses"),p_medical),(format!("{lc}_donation_credit"),format!("{code} donation credit"),p_don-p_dtc),(format!("{code}61520"),format!("{code} dividend tax credit"),p_dtc),(format!("{lc}_extra_credit_amounts"),format!("{code} additional credit amounts at the provincial rate"),p_base)];
    lines.retain(|l| l.2!=0);
    let mut out=vec![("31400".into(),"Pension income amount (max $2,000)".into(),pension_amount),("32300".into(),"Verified T2202 tuition amount (current year)".into(),tui),
        ("33099".into(),"Allowable medical expenses (less lesser of 3% net income and $2,834)".into(),medical),("34900".into(),"Donations (federal credit: 14.5% first $200, 29%/33% above)".into(),don),("donation_credit".into(),"Federal donation credit".into(),donation_credit),("40425".into(),"Federal dividend tax credit".into(),fed_dtc),("30100".into(),"Age amount".into(),age_amount),("30300".into(),"Spouse or common-law partner amount".into(),spouse_fed),("31600".into(),"Disability amount (18+)".into(),disability),("31270".into(),"Home buyers' amount".into(),hb),("31300".into(),"Adoption expenses".into(),adopt),("31900".into(),"Interest paid on student loans".into(),sli),("extra_federal_credits".into(),"Additional federal non-refundable credits".into(),credit)];
    out.extend(lines);
    Ok((credit,p_base,p_don,out))
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
    check_fields(obj,&["scope_confirmed","other_tax_situations","employment_income_cents","interest_income_cents","rrsp_deduction_cents","rrsp_deduction_limit_cents","rrsp_prior_year_earned_income_cents","rrsp_unused_room_cents","rrsp_pension_adjustment_cents","rrsp_par_cents","rrsp_net_pspa_cents","foreign_non_business_income_cents","foreign_non_business_tax_cents","foreign_tax_verified","cwb_may_apply","cpp_base_credit_cents","cpp_enhanced_deduction_cents","ei_credit_cents","tax_withheld_cents","instalments_cents","full_year_resident","payroll_verified","rrsp_verified","self_employment_gross_cents","self_employment_expenses_cents","self_employment_cca_cents","t2125_verified","se_cpp_deduction_cents","se_cpp_credit_cents","se_cpp_verified","capital_proceeds_cents","capital_acb_cents","capital_outlays_cents","schedule3_verified","rental_gross_cents","rental_expenses_cents","rental_cca_cents","t776_verified","pension_income_cents","pension_split_out_cents","pension_split_in_cents","t1032_verified","medical_expenses_cents","donations_cents","tuition_cents","credits_verified","eligible_dividends_cents","other_dividends_cents","dividends_verified","oas_pension_cents","oas_verified","spouse_net_income_cents","born_1960_or_earlier","disability_tax_credit_approved","home_buyers_amount_cents","adoption_expenses_cents","student_loan_interest_cents","provincial_adjustments_not_applicable"])?;
    require(c,obj,"full_year_resident","Confirm full-year Canadian residence and Ontario residence at year-end; newcomer, emigrant and deemed-resident rules are outside scope.")?;
    let employment=money(obj,"employment_income_cents")?; let interest=money(obj,"interest_income_cents")?;
    let rrsp=money(obj,"rrsp_deduction_cents")?; let limit=money(obj,"rrsp_deduction_limit_cents")?;
    let cpp=money(obj,"cpp_base_credit_cents")?; let enhanced=money(obj,"cpp_enhanced_deduction_cents")?; let ei=money(obj,"ei_credit_cents")?;
    if rrsp>limit { return Err("RRSP deduction exceeds the declared verified deduction limit".into()); }
    rrsp_limit_check(c,obj,limit)?;
    if rrsp>0 { require(c,obj,"rrsp_verified","Verify the RRSP deduction and available deduction limit against source documents; this engine does not calculate contribution room.")?; }
    if cpp>335_610 || enhanced>107_400 || ei>107_748 { return Err("CPP/EI claim exceeds the applicable 2025 employee maximum".into()); }
    if cpp>0 || enhanced>0 || ei>0 {
        require(c,obj,"payroll_verified","Enter only verified Schedule 8 base CPP/enhanced CPP deduction and eligible EI credit amounts; T4 box 16 is not itself the base CPP credit. No automatic contribution/overpayment calculation is performed.")?;
        if sum(&[cpp,enhanced,ei])?>employment { return Err("Declared payroll claims exceed employment income".into()); }
    }
    if ei>0 && employment<=200_000 { c.diagnostic("ei_refund_required","For this simple standard-insurable-pay scope, EI on earnings of $2,000 or less is an overpayment/refund, not a line 31200 credit; overpayments are outside scope.",Some("ei_credit_cents"),true); }
    let x=t1_income_extras(c,obj,employment)?;
    let income=sum(&[employment,interest,x.income])?; let deductions=sum(&[rrsp,enhanced,x.deductions])?;
    if deductions>income { return Err("Deductions exceed current income; loss and carryforward treatment is outside this scope".into()); }
    let net_before=income-deductions;
    // Line 23500 social benefits repayment (Worksheet 5000-D1): 15% over $93,454, capped at OAS received.
    let clawback=rounded_ratio((net_before-9_345_400).max(0),1500,10000)?.min(x.oas);
    let taxable=net_before-clawback;
    let reduction=rounded_ratio((taxable-17_788_200).clamp(0,7_553_200),159_100,7_553_200)?;
    let bpa=1_612_900-reduction; let employment_amount=employment.min(147_100);
    let federal_before=progressive(taxable,FEDERAL)?;
    let federal_credits=rounded_ratio(sum(&[bpa,employment_amount,cpp,ei])?,1450,10000)?;
    let (xcredit,pbase,pdon,xlines)=t1_credit_extras(c,obj,&x,taxable,bpa,"ON")?;
    let federal=(federal_before-federal_credits-xcredit).max(0);
    let federal=t1_foreign_and_flags(c,obj,federal,taxable)?;
    let ontario_before=progressive(taxable,ONTARIO)?;
    let ontario_credit_base=sum(&[1_274_700, cpp, ei, pbase])?;
    let ontario_credits=sum(&[rounded_ratio(ontario_credit_base,505,10000)?,pdon])?;
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
    let tax=sum(&[federal,provincial,clawback])?; let paid=sum(&[money(obj,"tax_withheld_cents")?,money(obj,"instalments_cents")?])?;
    for (code,label,amount) in [
        ("10100","Employment income",employment),("12100","Canadian interest income",interest),("15000","Total income",income),
        ("20800","Verified RRSP deduction",rrsp),("22215","Verified enhanced CPP deduction",enhanced),("23600","Net income",taxable),("26000","Taxable income",taxable),
        ("30000","Federal basic personal amount",bpa),("30800","Verified base CPP credit amount",cpp),("31200","Verified EI credit amount",ei),("31260","Canada employment amount",employment_amount),
        ("federal_before_credits","Federal tax before credits",federal_before),("35000","Scoped federal non-refundable credits",federal_credits),("42000","Scoped net federal income tax",federal),
        ("ON58040","Ontario basic personal amount",1_274_700),
        ("on_before_credits","Ontario tax on taxable income",ontario_before),("ON61500","Scoped Ontario non-refundable credits",ontario_credits),
        ("on_surtax","Ontario surtax",surtax),("on_tax_reduction","Ontario basic tax reduction",tax_reduction),("on_health_premium","Ontario health premium",health),("42800","Scoped Ontario income tax",provincial),
    ] { c.line(code,label,amount); }
    for (code,label,amount) in x.lines.iter().chain(xlines.iter()) { c.line(code,label,*amount); }
    if clawback>0 { c.line("23500","Social benefits repayment (OAS), also added on line 42200",clawback); }
    c.total("taxable_income",taxable); c.total("federal_tax",federal); c.total("provincial_tax",provincial); c.total("income_tax",tax); c.total("payments",paid); c.total("balance",tax-paid);
    c.diagnostic("ontario_scope","Ontario amount uses published 2025 ON428 brackets, the 5.05% credit rate, surtax, the basic reduction only, and the health-premium chart. Dependant reductions, eligible-dependant and caregiver amounts, dividends, LIFT, and minimum tax are not calculated. Cent rounding is half away from zero at each workpaper line, which can differ from dollar rounding on the paper form.",None,false);
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
    #[test]fn t1_generic_provinces_compute_from_published_brackets(){
        let base=json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"interest_income_cents":8_000_000});
        // BC on $80,000: 49,279*5.06% + 30,721*7.7% = 2,493.52 + 2,365.52 = 4,859.04; credits 12,932*5.06% = 654.36
        let bc=calc("T1","BC",base.clone()); assert!(bc.supported); assert_eq!(bc.totals["provincial_tax"],485_903-65_436);
        assert_eq!(bc.line_validation["bc_before_credits"],"validated"); assert_eq!(bc.line_validation["42800"],"unvalidated");
        // SK: 53,463*10.5% + 26,537*12.5% = 5,613.62 + 3,317.13 = 8,930.74 (half-up on the sum); credits 19,491*10.5% = 2,046.56
        assert_eq!(calc("T1","SK",base.clone()).totals["provincial_tax"],893_074-204_656);
        // YT uses the federal BPA (16,129) and CEA at 6.4%.
        let yt=calc("T1","YT",json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"interest_income_cents":8_000_000}));
        assert_eq!(yt.lines.iter().find(|l|l.code=="YT58040").unwrap().amount,1_612_900);
        // Missing confirmation blocks; MB over $200k blocks; QC unsupported.
        assert!(!calc("T1","NS",json!({"scope_confirmed":true,"full_year_resident":true})).supported);
        assert!(!calc("T1","MB",json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"interest_income_cents":25_000_000})).supported);
        assert!(!calc("T1","QC",json!({})).supported);
    }
    #[test]fn t2125_schedule3_t776_and_t1032_flow_into_net_income(){
        let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"t2125_verified":true,"self_employment_gross_cents":3_000_000,"self_employment_expenses_cents":1_000_000,"se_cpp_verified":true,
            "schedule3_verified":true,"capital_proceeds_cents":1_500_000,"capital_acb_cents":1_000_000,"capital_outlays_cents":100_000,"t776_verified":true,"rental_gross_cents":2_400_000,"rental_expenses_cents":1_400_000,"rental_cca_cents":200_000}));
        let l=|k:&str|c.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(l("13500"),2_000_000); assert_eq!(l("12700"),200_000); assert_eq!(l("12600"),800_000); assert_eq!(c.totals["taxable_income"],3_000_000);
        assert!(c.supported);
        assert!(calculate("T1",2025,"AB",&json!({"scope_confirmed":true,"full_year_resident":true,"t776_verified":true,"rental_gross_cents":100,"rental_expenses_cents":100,"rental_cca_cents":1})).is_err());
        // SE earnings >= $5,000 without verified Schedule 8 CPP block review.
        assert!(!calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"t2125_verified":true,"self_employment_gross_cents":1_000_000})).supported);
        // T1032: deduction capped at 50% of eligible pension income.
        assert!(calculate("T1",2025,"AB",&json!({"scope_confirmed":true,"full_year_resident":true,"t1032_verified":true,"pension_income_cents":2_000_000,"pension_split_out_cents":1_000_001})).is_err());
    }
    #[test]fn home_buyers_adoption_student_loan_federal_credits(){
        let c=calc("T1","ON",json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"employment_income_cents":5_000_000,"home_buyers_amount_cents":1_000_000,"adoption_expenses_cents":500_000,"student_loan_interest_cents":60_000}));
        let l=|k:&str|c.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(l("31270"),1_000_000); assert_eq!(l("31300"),500_000); assert_eq!(l("31900"),60_000);
        assert_eq!(l("extra_federal_credits"),rounded_ratio(1_560_000,1450,10000).unwrap());
        assert!(calculate("T1",2025,"ON",&json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"home_buyers_amount_cents":1_000_001})).is_err());
        assert!(calculate("T1",2025,"ON",&json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"adoption_expenses_cents":1_958_001})).is_err());
    }
    #[test]fn t1_2026_basic_scope_ab_bc_and_gates(){
        let base=json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":8_000_000});
        let c=calculate("T1",2026,"AB",&base).unwrap();
        assert_eq!(c.rate_table,"ca-2026.1"); assert!(c.supported,"{:?}",c.diagnostics);
        let l=|c:&Calculation,k:&str|c.lines.iter().find(|x|x.code==k).unwrap().amount;
        // Federal: 58,523*14% + 21,477*20.5% = 8,193.22 + 4,402.785 -> 12,596.01; credits (16,452+1,501)*14% = 2,513.42.
        assert_eq!(l(&c,"federal_before_credits"),819_322+440_279); assert_eq!(l(&c,"35000"),251_342);
        // Alberta: 61,200*8% + 18,800*10% = 4,896 + 1,880; credits 22,769*8% = 1,821.52.
        assert_eq!(l(&c,"ab_before_credits"),489_600+188_000); assert_eq!(l(&c,"AB61500"),182_152);
        let b=calculate("T1",2026,"BC",&base).unwrap(); assert!(b.supported,"{:?}",b.diagnostics);
        // BC: 50,363*5.6% + 29,637*7.7% = 2,820.33 + 2,282.05; credit 13,216*5.6% = 740.10.
        assert_eq!(l(&b,"bc_before_credits"),282_033+228_205); assert_eq!(l(&b,"BC61500"),74_010);
        // BPA phase-out midpoint and gates.
        let hi=calculate("T1",2026,"SK",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":25_848_200})).unwrap();
        assert_eq!(l(&hi,"30000"),1_482_900);
        assert!(calculate("T1",2026,"AB",&json!({"scope_confirmed":true,"donations_cents":100})).is_err());
        let mut ob=base.clone(); ob["family_income_verified"]=json!(true); let o=calculate("T1",2026,"ON",&ob).unwrap(); assert!(o.supported,"{:?}",o.diagnostics);
        // ON: 53,891*5.05% + 26,109*9.15% less 12,989*5.05%; below the $5,818 surtax threshold; OHP $750 at $80,000.
        assert_eq!(l(&o,"on_before_credits"),272_150+238_897); assert_eq!(l(&o,"ON61500"),65_594);
        assert_eq!(l(&o,"ON_surtax"),0); assert_eq!(l(&o,"ON_tax_reduction"),0); assert_eq!(l(&o,"ON_health_premium"),75_000);
        assert_eq!(l(&o,"ON_lift"),0);
        let w=calculate("T1",2026,"ON",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"family_income_verified":true,"employment_income_cents":3_400_000,"spouse_adjusted_net_income_cents":3_500_000})).unwrap();
        // min(875, 5.05% x 34,000=1,717) - 5% x max(1,500, 4,000) = 875 - 200 = 675.
        assert_eq!(l(&w,"ON_lift"),67_500); assert!(w.supported,"{:?}",w.diagnostics);
        assert_eq!(o.totals["provincial_tax"],272_150+238_897-65_594+75_000);
        for (a,p) in [(2_000_000,0),(3_000_000,30_000),(4_000_000,45_000),(5_000_000,60_000),(7_300_000,75_000),(25_000_000,90_000),(20_020_000,80_000)] { assert_eq!(on_health_premium_2026(a).unwrap(),p,"{a}"); }
        let lo=calculate("T1",2026,"ON",&json!({"scope_confirmed":true,"family_income_verified":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":1_500_000})).unwrap();
        assert_eq!(lo.totals["provincial_tax"],0);
        let rich=calculate("T1",2026,"ON",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"interest_income_cents":30_000_000})).unwrap();
        let b=rich.totals["provincial_tax"]-90_000+0; let basic=l(&rich,"on_before_credits")-l(&rich,"ON61500");
        assert_eq!(l(&rich,"ON_surtax"),rounded_ratio(basic-581_800,2000,10000).unwrap()+rounded_ratio(basic-744_600,3600,10000).unwrap()); assert_eq!(b,basic+l(&rich,"ON_surtax"));
        assert!(!calculate("T1",2026,"PE",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":3_000_000})).unwrap().supported);
        assert!(!calculate("T2",2026,"AB",&json!({})).unwrap().supported);
        let am=calculate("T1",2026,"AB",&json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"born_1961_or_earlier":true,"medical_expenses_cents":400_000,"interest_income_cents":5_000_000})).unwrap();
        // 9,208 - 15% x (50,000-46,432) = 8,672.80; medical 4,000 - 1,500 (3% < 2,890) = 2,500.
        assert_eq!(l(&am,"30100"),867_280); assert_eq!(l(&am,"33099"),250_000); assert!(!am.supported);
        assert_eq!(l(&am,"35000"),rounded_ratio(1_645_200+867_280+250_000,1400,10000).unwrap());
        assert!(calculate("T1",2026,"AB",&json!({"scope_confirmed":true,"full_year_resident":true,"medical_expenses_cents":100})).map(|c|!c.supported).unwrap_or(true));
        let bc=calculate("T1",2026,"BC",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":3_000_000})).unwrap();
        assert!(bc.supported,"{:?}",bc.diagnostics); assert_eq!(l(&bc,"BC61800"),41_729);
        let bc_hi=calculate("T1",2026,"BC",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":4_200_000})).unwrap(); assert_eq!(l(&bc_hi,"BC61800"),0);
        let mb=calculate("T1",2026,"MB",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"interest_income_cents":30_000_000})).unwrap();
        assert!(mb.supported,"{:?}",mb.diagnostics); assert_eq!(l(&mb,"MB58040"),789_000);
        let mb0=calculate("T1",2026,"MB",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"interest_income_cents":45_000_000})).unwrap(); assert_eq!(l(&mb0,"MB58040"),0);
    }
    #[test]fn medical_donation_pension_tuition_federal_credits(){
        // Net $50,000: medical threshold 1,500; 4,000 expenses -> 2,500. Pension 2,000 cap. Tuition 3,000. Donations 1,200 -> 200*14.5% + 1,000*29% = 29 + 290 = 319.
        let c=calc("T1","ON",json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"pension_income_cents":5_000_000,"medical_expenses_cents":400_000,"donations_cents":120_000,"tuition_cents":300_000}));
        let l=|k:&str|c.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(l("33099"),250_000); assert_eq!(l("31400"),200_000); assert_eq!(l("donation_credit"),31_900);
        assert_eq!(l("extra_federal_credits"),rounded_ratio(750_000,1450,10000).unwrap()+31_900);
        assert!(c.supported, "{:?}", c.diagnostics);
        // ON428 2025: medical cap $2,885 (3% = 1,500 applies); pension max $1,762; no current-year tuition; donations 5.05%/11.16%.
        assert_eq!(l("ON58689"),250_000); assert_eq!(l("ON58360"),176_200); assert!(c.lines.iter().all(|x|x.code!="ON58560"));
        assert_eq!(l("on_donation_credit"),1_010+11_160); assert_eq!(l("on_extra_credit_amounts"),426_200);
    }
    #[test]fn dividends_and_oas_recovery_2025(){
        // $1,000 eligible -> 1,380 taxable; federal DTC 15.0198% = 207.27; AB 8.12% = 112.06.
        let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"dividends_verified":true,"eligible_dividends_cents":100_000,"interest_income_cents":5_000_000}));
        assert!(c.supported, "{:?}", c.diagnostics);
        let l=|k:&str|c.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(l("12000"),138_000); assert_eq!(l("40425"),20_727); assert_eq!(l("AB61520"),11_206);
        // $1,000 other -> 1,150; ON 2.9863% = 34.34; federal 9.0301% = 103.85.
        let o=calc("T1","ON",json!({"scope_confirmed":true,"full_year_resident":true,"dividends_verified":true,"other_dividends_cents":100_000}));
        let lo=|k:&str|o.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(lo("12010"),115_000); assert_eq!(lo("40425"),10_385); assert_eq!(lo("ON61520"),3_434);
        // OAS $8,000 + interest $100,000: net before 108,000 -> 15% x 14,546 = 2,181.90 recovery.
        let r=calc("T1","BC",json!({"scope_confirmed":true,"full_year_resident":true,"oas_verified":true,"oas_pension_cents":800_000,"interest_income_cents":10_000_000}));
        let lr=|k:&str|r.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(lr("23500"),218_190); assert_eq!(lr("23600"),10_800_000-218_190);
        assert_eq!(r.totals["income_tax"],r.totals["federal_tax"]+r.totals["provincial_tax"]+218_190);
        assert!(!calc("T1","BC",json!({"scope_confirmed":true,"full_year_resident":true,"oas_pension_cents":800_000})).supported);
    }
    #[test]fn rrsp_limit_recomputed(){
        let base=|lim:i64|calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":8_000_000,"rrsp_deduction_cents":500_000,"rrsp_verified":true,"rrsp_deduction_limit_cents":lim,"rrsp_prior_year_earned_income_cents":10_000_000,"rrsp_unused_room_cents":200_000,"rrsp_pension_adjustment_cents":300_000}));
        // 2,000 + min(18,000, 32,490) - 3,000 = 17,000.
        let ok=base(1_700_000); assert!(ok.supported,"{:?}",ok.diagnostics);
        assert_eq!(ok.lines.iter().find(|l|l.code=="rrsp_limit_computed").unwrap().amount,1_700_000);
        assert!(!base(1_800_000).supported);
    }
    #[test]fn foreign_tax_credit_basic(){
        let c=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":8_000_000,"foreign_non_business_income_cents":1_000_000,"foreign_non_business_tax_cents":250_000,"foreign_tax_verified":true}));
        assert!(c.supported,"{:?}",c.diagnostics);
        // paid capped at 15% x 10,000 = 1,500; T2209 limit = basic federal tax x 10,000/80,000 = 1,327.35 binds.
        assert_eq!(c.lines.iter().find(|l|l.code=="40500").unwrap().amount,132_735);
        assert!(!calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"foreign_non_business_tax_cents":1})).supported);
    }
    #[test]fn low_income_reductions_2025(){
        let r=|j:&str,v:Value|{let c=calc("T1",j,v);c.lines.iter().find(|x|x.code==format!("{}_low_income_reduction",j.to_lowercase())).unwrap().amount};
        // BC428: $562 - 3.56% x ($30,000 - $25,020) = 384.71.
        assert_eq!(r("BC",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":3_000_000})),38_471);
        // NS428 with spouse (nil income): $600 - 5% x ($20,000 - $15,000) = 350.
        assert_eq!(r("NS",json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"interest_income_cents":2_000_000,"spouse_net_income_cents":0})),35_000);
        // NL428 single: $997 - 16% x ($25,000 - $23,928) = 825.48.
        assert_eq!(r("NL",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":2_500_000})),82_548);
        assert_eq!(r("NB",json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":6_000_000})),0);
    }
    #[test]fn provincial_credits_2025_bc_top_donation_tier_and_tuition(){
        // BC428 worksheet line 58969: $200 at 5.06%, part matched to taxable income over $259,829 at 20.5%, rest 16.8%.
        let c=calc("T1","BC",json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"interest_income_cents":30_000_000,"donations_cents":5_000_000,"tuition_cents":100_000}));
        let l=|k:&str|c.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(l("bc_donation_credit"),1_012+823_506+161_767);
        assert_eq!(l("BC58560"),100_000);
        for code in ["NL","PE","NS","NB","ON","MB","SK","AB","BC","YT","NT","NU"] {
            let p=super::super::rates::prov_credits_2025(code).unwrap();
            assert_eq!(super::super::rates::rate_status("ca-2025.1",&format!("{}_credits_2025",code.to_lowercase())),"sourced");
            assert!(p.medical_max_cents>0 && p.pension_max_cents>0 && p.donation_high_bps>p.donation_low_bps.min(p.donation_high_bps)-1);
        }
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
        assert!(!spouse.supported || spouse.diagnostics.iter().any(|d|d.code=="credits_verified"));
        // Age (Worksheet 5000-D1 / AB428): net $40,000 below both thresholds; spouse with nil income; DTC adult.
        let ab=calc("T1","AB",json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"interest_income_cents":4_000_000,"spouse_net_income_cents":0,"born_1960_or_earlier":true,"disability_tax_credit_approved":true}));
        assert!(ab.supported, "{:?}", ab.diagnostics);
        let l=|k:&str|ab.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(l("30100"),902_800); assert_eq!(l("30300"),1_612_900); assert_eq!(l("31600"),1_013_800);
        assert_eq!(l("AB58080"),622_100); assert_eq!(l("AB58120"),2_232_300); assert_eq!(l("AB58440"),1_721_900);
        // Age amount phases out at 15%: net $60,000 -> 9,028 - 0.15*(60,000-45,522) = 6,856.30.
        let ns=calc("T1","NS",json!({"scope_confirmed":true,"full_year_resident":true,"credits_verified":true,"interest_income_cents":6_000_000,"born_1960_or_earlier":true,"spouse_net_income_cents":50_000}));
        let n=|k:&str|ns.lines.iter().find(|x|x.code==k).unwrap().amount;
        assert_eq!(n("30100"),685_630); assert_eq!(n("NS58080"),573_400-437_580); assert_eq!(n("NS58120"),1_261_800-87_400);
    }
}
