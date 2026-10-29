//! Native T2 schedule workpapers: Schedule 1 (net income for tax purposes), Schedule 8
//! (capital cost allowance, declining-balance classes only), Schedule 50 (shareholder
//! information) and Schedule 100/125 cross-checks. Line numbers and rates are copied from
//! the CRA forms and pages cited in [`SOURCES`]. Nothing here is CRA-certified or filed;
//! every result is a draft for preparer review.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::corporate::GifiRow;

pub const SOURCES: &[(&str, &str)] = &[
    ("T2 Schedule 1, Net Income (Loss) for Income Tax Purposes (t2sch1-25e)", "https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/t2sch1.html"),
    ("T2 Schedule 8, Capital Cost Allowance (t2sch8-26e), columns 10, 17, 19, 20, 21 and notes 12, 14, 15", "https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/t2sch8.html"),
    ("Income Tax Regulations (C.R.C., c. 945) s. 1100(1)(a) rates (incl. (xiv) Class 17, 8%), 1100(1)(b) Class 13, 1100(1)(c) Class 14, 1100(2) factors A/A.1 (incl. A(c.1)–(c.3), A(d), A.1(f)), 1100(2.01)/(2.011) straddle years, 1100(3) years under 12 months, Schedule III, 1100(0.1)–(0.3) immediate expensing", "https://laws-lois.justice.gc.ca/eng/regulations/C.R.C.,_c._945/page-16.html"),
    ("Income Tax Regulations (C.R.C., c. 945) s. 1104(3.1) designated immediate expensing property, (3.2) $1,500,000 limit, (3.3)–(3.4) associated allocation, (3.5) multiple and short years", "https://laws-lois.justice.gc.ca/eng/regulations/C.R.C.,_c._945/page-19.html"),
    ("CRA T4002 ch. 4, Schedule 8 columns 4, 9, 10 and 21 (immediate expensing)", "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/t4002/t4002-6.html"),
    ("CRA classes of depreciable property and CCA rates", "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/sole-proprietorships-partnerships/report-business-income-expenses/claiming-capital-cost-allowance/classes-depreciable-property.html"),
    ("T2 Schedule 50, Shareholder Information (t2sch50-19e)", "https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/t2sch50.html"),
    ("CRA GIFI (RC4088): Schedule 100/125 codes 2599, 3499, 3620, 3640, 8299, 8670, 9368, 9990, 9995, 9999", "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/rc4088/general-index-financial-information-gifi.html"),
];
pub const SCOPE: &str = "Draft T2 schedule workpapers. Schedule 1 totals reviewer-classified items against GIFI 9999. Schedule 8 covers the declining-balance classes listed with their CRA rates and the 50% (half-year) rule and the Reg 1100(3) short-year proration (days/365); the Reg 1100(2) AIIP/RIIP factors (A, A.1) including Classes 44/46/50 after April 15, 2024 and Class 43 after 2025, taxation years spanning two calendar years (Reg 1100(2.01)/(2.011)), Classes 54 to 56, Class 13 leaseholds (Schedule III) and Class 14 limited-life property (Reg 1100(1)(c)) are modelled, as is CCPC immediate expensing (Reg 1100(0.1), 1104(3.1)–(3.5)) for DIEP available before 2024; separate-class elections, the Class 14.1 pre-2017 additional allowance and per-vehicle cost caps are not. Schedule 50 is structurally validated only. Not CRA-certified; nothing is filed.";

/// Schedule 1 lines copied from t2sch1-25e. Additions: 101–199 on page 1 plus 201–296
/// (summed to amount D, line 199). Deductions: 401–499 plus 300–3xx (amount E, line 499).
const ADDITIONS: &[(&str, &str)] = &[
    ("101", "Provision for income taxes – current"), ("102", "Provision for income taxes – deferred"), ("103", "Interest and penalties on taxes"),
    ("104", "Amortization of tangible assets"), ("105", "Amortization of natural resource assets"), ("106", "Amortization of intangible assets"),
    ("107", "Recapture of capital cost allowance from Schedule 8"), ("110", "Loss in equity of subsidiaries and affiliates"), ("111", "Loss on disposal of assets"),
    ("112", "Charitable donations and gifts from Schedule 2"), ("113", "Taxable capital gains from Schedule 6"), ("114", "Political contributions"),
    ("115", "Holdbacks"), ("116", "Deferred and prepaid expenses"), ("117", "Depreciation in inventory – end of year"),
    ("118", "Scientific research expenditures deducted per financial statements"), ("119", "Capitalized interest"), ("120", "Non-deductible club dues and fees"),
    ("121", "Non-deductible meals and entertainment expenses"), ("122", "Non-deductible automobile expenses"), ("123", "Non-deductible life insurance premiums"),
    ("124", "Non-deductible company pension plans"), ("125", "Other reserves on lines 270 and 275 from Schedule 13"), ("126", "Reserves from financial statements – balance at the end of the year"),
    ("127", "Soft costs on construction and renovation of buildings"), ("128", "Non-deductible fines and penalties under section 67.6"),
    ("206", "Capital items expensed"), ("208", "Debt issue expense"), ("216", "Financing fees deducted in books"), ("226", "Non-deductible advertising"),
    ("227", "Non-deductible interest"), ("228", "Non-deductible legal and accounting fees"), ("230", "Other expenses from financial statements"),
    ("234", "Sales tax assessments"), ("235", "Share issue expense"), ("236", "Write-down of capital property"),
];
const DEDUCTIONS: &[(&str, &str)] = &[
    ("401", "Gain on disposal of assets per financial statements"), ("402", "Non-taxable dividends under section 83 from Schedule 3"),
    ("403", "Capital cost allowance from Schedule 8"), ("404", "Terminal loss from Schedule 8"), ("406", "Allowable business investment loss from Schedule 6"),
    ("408", "Holdbacks"), ("409", "Deferred and prepaid expenses"), ("410", "Depreciation in inventory – end of prior year"),
    ("413", "Other reserves on line 280 from Schedule 13"), ("414", "Reserves from financial statements – balance at the beginning of the year"),
    ("418", "Incorporation expenses under paragraph 20(1)(b)"), ("304", "Bad debt"), ("306", "Equity in income from subsidiaries or affiliates"),
    ("307", "Exempt income under section 81"), ("314", "Other income from financial statements"),
];
pub fn line_label(line: &str) -> Option<(&'static str, bool)> {
    ADDITIONS.iter().find(|(l, _)| *l == line).map(|(_, n)| (*n, true))
        .or_else(|| DEDUCTIONS.iter().find(|(l, _)| *l == line).map(|(_, n)| (*n, false)))
}

/// Declining-balance CCA rates (percent), each checked against Income Tax Regulations s. 1100(1)(a)
/// (laws-lois.justice.gc.ca, C.R.C., c. 945): Class 17 is 1100(1)(a)(xiv), 8 per cent. The CRA classes page agrees for the others.
const RATES: &[(&str, u32)] = &[
    ("1", 4), ("3", 5), ("6", 10), ("8", 20), ("10", 30), ("10.1", 30), ("12", 100), ("14.1", 5),
    ("16", 40), ("17", 8), ("43", 30), ("45", 45), ("46", 30), ("50", 55), ("53", 50),
    ("43.1", 30), ("43.2", 50), ("44", 25), ("54", 30), ("55", 40), ("56", 30),
];
pub fn rate(class: &str) -> Option<u32> { RATES.iter().find(|(c, _)| *c == class).map(|(_, r)| *r) }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Schedule1Item { pub line: String, pub amount_cents: i64, pub source_ref: String }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CcaClass {
    pub class: String,
    pub opening_ucc_cents: i64,
    pub additions_cents: i64,
    /// Lesser of proceeds and capital cost of property disposed of (column 7).
    pub proceeds_cents: i64,
    /// Signed column 6 adjustments and transfers.
    pub adjustments_cents: i64,
    /// Legacy catch-all for additions outside the 50% rule. Blocks: use the specific fields below.
    pub accelerated_additions_cents: i64,
    /// Part of `additions_cents` that is accelerated investment incentive property (or, for
    /// Classes 54–56, property acquired before 2025): element D of Reg 1100(2).
    pub aiip_additions_cents: i64,
    /// Part of `additions_cents` that is reaccelerated investment incentive property (or, for
    /// Classes 54–56, property acquired after 2024): element D.1 of Reg 1100(2).
    pub riip_additions_cents: i64,
    /// Part of `additions_cents` the preparer has classified as excluded from element F of
    /// Reg 1100(2) (e.g. Class 12 paragraphs (a)–(c), (e)–(i), (k), (l), (p)–(s)): no 50% rule.
    pub half_year_exempt_additions_cents: i64,
    /// Taxation years spanning two calendar years only: the part of `aiip_additions_cents` /
    /// `riip_additions_cents` that became available for use in the later calendar year. The rest is
    /// taken as available in the earlier year; each part takes its own year's factor, as the
    /// blended factor (A(B) + C(D))/(B + D) of Reg 1100(2.01)/(2.011) works out to.
    pub aiip_second_year_cents: Option<i64>,
    pub riip_second_year_cents: Option<i64>,
    /// Classes 44, 46 and 50 AIIP available after 2023: was the property both acquired and
    /// available for use after April 15, 2024 (Reg 1100(2) A(c.1)–(c.3))?
    pub acquired_after_april_15_2024: Option<bool>,
    /// Class 43 acquired after 2025: would it have been Class 53 if acquired in 2025
    /// (Reg 1100(2) A(d) and A.1(f))?
    pub would_have_been_class_53: Option<bool>,
    /// Class 13 only: each part of the capital cost of a leasehold interest (Schedule III).
    pub leaseholds: Vec<LeaseholdPart>,
    /// Class 14 only: each limited-life patent, franchise, concession or licence (Reg 1100(1)(c)).
    pub limited_life: Vec<LimitedLifeProperty>,
    pub no_property_left: bool,
    /// Optional lower claim (column 21 "or a lower amount").
    pub claim_cents: Option<i64>,
    /// Part of `additions_cents` that is designated immediate expensing property (column 4,
    /// Reg 1104(3.1)) and the immediate expensing amount claimed on it (column 9, Reg 1100(0.1)).
    pub diep_cents: i64,
    pub immediate_expensing_cents: i64,
}

/// Reg 1100(0.1)–(0.3) and 1104(3.1)–(3.5) facts for immediate expensing (CCPC only here).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImmediateExpensing {
    /// Associated in the year with another eligible person or partnership (1104(3.2)).
    pub associated: Option<bool>,
    /// Associated only: this corporation's percentage under a 1104(3.3) agreement, and the total
    /// of all percentages in the agreement (basis points; nil limit above 100%).
    pub agreement_percent_bp: Option<u32>,
    pub agreement_total_bp: Option<u32>,
    /// Another taxation year of this corporation ends in the same calendar year (1104(3.5)(a)).
    pub other_tax_year_ends_same_calendar_year: Option<bool>,
    /// The property was designated on the prescribed form (1104(3.1) DIEP (c)).
    pub designated_on_prescribed_form: bool,
    /// Each DIEP was acquired after April 18, 2021, became available for use before 2024, is
    /// not in Class 1–6, 14.1, 17, 47, 49 or 51, and was not used or acquired for use before
    /// acquisition, and not acquired non-arm's-length or on a listed rollover (1104(3.1)).
    pub diep_conditions_confirmed: bool,
}

/// none | aiip | riip
fn accel_ok(v: &str) -> bool { matches!(v, "" | "none" | "aiip" | "riip") }

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LeaseholdPart {
    pub cost_cents: i64,
    /// Schedule III s. 2(b): 12-month periods (1 to 40) from the start of the year the cost was
    /// incurred to the end of the lease, with renewals counted per s. 3(b).
    pub periods_12m: u32,
    /// Amounts claimed and deductible for this part in previous years (s. 3(c)).
    pub claimed_before_cents: i64,
    /// Capital cost incurred (available for use) in this taxation year (Reg 1100(1)(b)(i)).
    pub incurred_in_year: bool,
    pub acceleration: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitedLifeProperty {
    pub cost_cents: i64,
    /// Life remaining when the cost was incurred, in days, and the days of that life in this year.
    pub life_days: u32,
    pub days_in_year: u32,
    pub available_in_year: bool,
    pub acceleration: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shareholder {
    pub name: String,
    /// individual | corporation | partnership | trust
    pub kind: String,
    /// One of: a BN/partnership account (9 digits, 2 letters, 4 digits) or "NR", a 9-digit SIN, or a trust number (T + 8 digits).
    pub identifier: String,
    /// Basis points: 10000 = 100%.
    pub common_bp: u32,
    pub preferred_bp: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct T2Input {
    /// Schedule 100/125 rows (current column used).
    pub gifi: Vec<GifiRow>,
    pub schedule1: Vec<Schedule1Item>,
    pub cca: Vec<CcaClass>,
    pub shareholders: Vec<Shareholder>,
    /// ccpc | other_private | public | ... (as in the corporate profile). Private types require Schedule 50.
    pub corporation_type: String,
    /// Taxation year (YYYY-MM-DD). Needed whenever accelerated additions are claimed: the
    /// Reg 1100(2) factors depend on the year property became available for use.
    pub tax_year_start: String,
    pub tax_year_end: String,
    /// Required when any class claims immediate expensing.
    pub immediate_expensing: Option<ImmediateExpensing>,
}

const MAX: i64 = 100_000_000_000_000;
fn diag(out: &mut Vec<Value>, code: &str, msg: impl Into<String>, blocking: bool) {
    out.push(json!({"code":code,"severity":if blocking {"error"} else {"warning"},"message":msg.into(),"blocks_review":blocking}));
}
fn gifi(rows: &[GifiRow], section: &str, code: &str) -> Option<i64> {
    rows.iter().find(|r| r.section == section && r.code == code).map(|r| r.current_cents)
}

pub fn validate(input: &T2Input) -> Result<(), String> {
    if input.schedule1.len() > 300 || input.cca.len() > 60 || input.shareholders.len() > 200 || input.gifi.len() > 400 { return Err("too many rows".into()); }
    for i in &input.schedule1 {
        if !(0..=MAX).contains(&i.amount_cents) { return Err(format!("Schedule 1 line {} amount must be 0 to {MAX} cents; the line decides add or deduct", i.line)); }
        if i.source_ref.len() > 120 { return Err("source_ref is limited to 120 characters".into()); }
    }
    for c in &input.cca {
        for v in [c.opening_ucc_cents, c.additions_cents, c.proceeds_cents, c.accelerated_additions_cents] { if !(0..=MAX).contains(&v) { return Err(format!("Class {} amounts must be 0 to {MAX} cents", c.class)); } }
        if !(-MAX..=MAX).contains(&c.adjustments_cents) || c.claim_cents.is_some_and(|v| !(0..=MAX).contains(&v)) { return Err(format!("Class {} amounts out of range", c.class)); }
        if c.accelerated_additions_cents > c.additions_cents { return Err(format!("Class {} accelerated additions exceed additions", c.class)); }
        for v in [c.aiip_additions_cents, c.riip_additions_cents, c.half_year_exempt_additions_cents] { if !(0..=MAX).contains(&v) { return Err(format!("Class {} amounts must be 0 to {MAX} cents", c.class)); } }
        if c.aiip_additions_cents + c.riip_additions_cents + c.half_year_exempt_additions_cents > c.additions_cents { return Err(format!("Class {}: AIIP, RIIP and half-year-exempt parts exceed additions", c.class)); }
        for v in [c.aiip_second_year_cents, c.riip_second_year_cents] { if v.is_some_and(|v| v < 0) { return Err(format!("Class {} second-year parts cannot be negative", c.class)); } }
        if c.aiip_second_year_cents.unwrap_or(0) > c.aiip_additions_cents || c.riip_second_year_cents.unwrap_or(0) > c.riip_additions_cents { return Err(format!("Class {}: second-year parts exceed the AIIP or RIIP additions", c.class)); }
        if !(0..=MAX).contains(&c.diep_cents) || !(0..=c.diep_cents).contains(&c.immediate_expensing_cents) || c.diep_cents > c.additions_cents { return Err(format!("Class {}: DIEP must be within additions and the immediate expensing amount within DIEP", c.class)); }
        if c.aiip_additions_cents + c.riip_additions_cents + c.half_year_exempt_additions_cents + c.immediate_expensing_cents > c.additions_cents { return Err(format!("Class {}: AIIP, RIIP and half-year-exempt parts are taken from the additions remaining after immediate expensing (column 10)", c.class)); }
        if c.leaseholds.len() > 100 || c.limited_life.len() > 100 { return Err("too many rows".into()); }
        if (!c.leaseholds.is_empty() && c.class != "13") || (!c.limited_life.is_empty() && c.class != "14") { return Err(format!("Class {}: leaseholds are Class 13 and limited-life property is Class 14", c.class)); }
        for l in &c.leaseholds {
            if !(0..=MAX).contains(&l.cost_cents) || !(0..=l.cost_cents).contains(&l.claimed_before_cents) || !(1..=40).contains(&l.periods_12m) || !accel_ok(&l.acceleration) { return Err("leasehold: cost 0..max, claimed_before ≤ cost, periods_12m 1..40, acceleration none|aiip|riip".into()); }
        }
        for l in &c.limited_life {
            if !(0..=MAX).contains(&l.cost_cents) || l.life_days == 0 || l.days_in_year > 366 || l.days_in_year > l.life_days || !accel_ok(&l.acceleration) { return Err("limited-life property: cost 0..max, life_days > 0, days_in_year ≤ 366 and ≤ life_days, acceleration none|aiip|riip".into()); }
        }
    }
    let mut classes = BTreeSet::new();
    for c in &input.cca { if !classes.insert(c.class.as_str()) { return Err(format!("duplicate CCA class {}", c.class)); } }
    for s in &input.shareholders {
        if s.name.trim().is_empty() || s.name.len() > 175 || s.name.chars().any(char::is_control) { return Err("each shareholder needs a name of up to 175 characters".into()); }
        if s.common_bp > 10_000 || s.preferred_bp > 10_000 { return Err("share percentages cannot exceed 100%".into()); }
    }
    Ok(())
}

/// Schedule 1: A (GIFI 9999) + line 500 − line 510 = C (line 300 of the T2).
pub fn schedule1(net_income_9999: i64, items: &[Schedule1Item], diagnostics: &mut Vec<Value>) -> Value {
    let mut lines: BTreeMap<String, i64> = BTreeMap::new();
    for i in items {
        match line_label(&i.line) {
            Some(_) => *lines.entry(i.line.clone()).or_default() += i.amount_cents,
            None => diag(diagnostics, "schedule1_line_unsupported", format!("Line {} is not one of the Schedule 1 lines this workpaper carries; classify it on a listed line or enter it manually in the return.", i.line), true),
        }
    }
    let sum = |add: bool| lines.iter().filter(|(l, _)| line_label(l).is_some_and(|(_, a)| a == add)).map(|(_, v)| *v).sum::<i64>();
    let d = lines.iter().filter(|(l, _)| l.starts_with('2')).map(|(_, v)| *v).sum::<i64>();
    let e = lines.iter().filter(|(l, _)| l.starts_with('3')).map(|(_, v)| *v).sum::<i64>();
    let (l500, l510) = (sum(true), sum(false));
    let b = net_income_9999 + l500;
    let c = b - l510;
    let rows: Vec<Value> = lines.iter().map(|(l, v)| { let (label, add) = line_label(l).unwrap(); json!({"line":l,"label":label,"addition":add,"amount_cents":v}) }).collect();
    json!({"amount_a_gifi_9999_cents":net_income_9999,"lines":rows,"amount_d_line_199_cents":d,"amount_e_line_499_cents":e,
        "line_500_cents":l500,"amount_b_cents":b,"line_510_cents":l510,"amount_c_net_income_for_tax_cents":c,"t2_line_300_cents":c})
}

type Frac = (i128, i128);
const NIL: Frac = (0, 1);

/// Reg 1100(2) factor A (AIIP, or Classes 54–56 acquired before 2025) or A.1 (RIIP, or
/// Classes 54–56 acquired after 2024) for property available for use in calendar year `y`.
/// Err = the factor depends on facts this workpaper does not have.
fn factor(c: &CcaClass, riip: bool, y: i32) -> Result<Frac, String> {
    let tier = |a: Frac, b: Frac, c: Frac, t1: i32, t2: i32| if y < t1 { a } else if y <= t2 { b } else { c };
    let class = c.class.as_str();
    Ok(match (class, riip) {
        ("12" | "13" | "14" | "15" | "59", _) => NIL,
        ("43.1", false) => tier((7, 3), (3, 2), (5, 6), 2024, 2025),
        ("43.1", true) => tier((7, 3), (3, 2), (5, 6), 2030, 2031),
        ("43.2", false) => tier((1, 1), (1, 2), (1, 10), 2024, 2025),
        ("53", false) => tier((1, 1), (1, 2), (1, 10), 2024, 2025),
        ("53", true) => (1, 1),
        ("54" | "56", false) => tier((7, 3), (3, 2), (5, 6), 2024, 2025),
        ("54" | "56", true) => tier((7, 3), (3, 2), (5, 6), 2030, 2031),
        ("55", false) => tier((3, 2), (7, 8), (3, 8), 2024, 2025),
        ("55", true) => tier((3, 2), (7, 8), (3, 8), 2030, 2031),
        // A(c.1)–(c.3): 3, 2 1/3, 9/11 if acquired and available after April 15, 2024 and before
        // 2027; otherwise A(a)(ii) nil after 2023; nil after 2026.
        ("44" | "46" | "50", false) if (2024..=2026).contains(&y) => match c.acquired_after_april_15_2024 {
            None => return Err(format!("Class {class} AIIP available for use in {y}: set acquired_after_april_15_2024 (Reg 1100(2) A(c.1)–(c.3)).")),
            Some(false) => NIL,
            Some(true) => match class { "44" => (3, 1), "46" => (7, 3), _ => (9, 11) },
        },
        ("44", true) => if y < 2027 { (3, 1) } else { NIL },
        ("46", true) => if y < 2027 { (7, 3) } else { NIL },
        ("50", true) => if y < 2027 { (9, 11) } else { NIL },
        // A(d)(iii) / A.1(f)(ii)–(iv): Class 43 acquired after 2025 that would have been Class 53.
        ("43", _) if y >= 2026 => match c.would_have_been_class_53 {
            None => return Err(format!("Class 43 {} available for use in {y}: set would_have_been_class_53 (Reg 1100(2) A(d), A.1(f)).", if riip { "RIIP" } else { "AIIP" })),
            Some(true) => if riip { tier((7, 3), (3, 2), (5, 6), 2030, 2031) } else { (5, 6) },
            Some(false) => if riip { if y < 2030 { (1, 2) } else { NIL } } else { NIL },
        },
        (_, false) => if y < 2024 { (1, 2) } else { NIL },
        (_, true) => if y < 2030 { (1, 2) } else { NIL },
    })
}
fn mul(v: i64, f: Frac) -> i128 { i128::from(v) * f.0 / f.1 }

/// Calendar years (start, end) of the taxation year for the 1100(2) factors, or a blocking reason.
fn factor_year(input: &T2Input) -> Result<(i32, i32), String> {
    use chrono::{Datelike, NaiveDate};
    let p = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    let (Some(a), Some(b)) = (p(&input.tax_year_start), p(&input.tax_year_end)) else {
        return Err("Accelerated additions need tax_year_start and tax_year_end (YYYY-MM-DD): the Reg 1100(2) factors depend on the year.".into());
    };
    if b < a { return Err("tax_year_end is before tax_year_start.".into()); }
    if b.year() - a.year() > 1 { return Err("Taxation year spans more than two calendar years.".into()); }
    Ok((a.year(), b.year()))
}

/// Reg 1100(3): Some(days) when the taxation year is less than 12 months.
fn short_year_days(input: &T2Input) -> Result<Option<i64>, String> {
    use chrono::{Days, Months, NaiveDate};
    let p = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    let (Some(a), Some(b)) = (p(&input.tax_year_start), p(&input.tax_year_end)) else {
        return Err("Schedule 8 needs tax_year_start and tax_year_end (YYYY-MM-DD): a year under 12 months is prorated under Reg 1100(3).".into());
    };
    if b < a { return Err("tax_year_end is before tax_year_start.".into()); }
    let twelve = a.checked_add_months(Months::new(12)).ok_or("tax year out of range")?;
    let next = b.checked_add_days(Days::new(1)).ok_or("tax year out of range")?;
    Ok(if next < twelve { Some((b - a).num_days() + 1) } else { None })
}

/// Schedule 8. Declining-balance classes use Reg 1100(1)(a) rates and the Reg 1100(2) formula
/// A(B) + A.1(B.1) − 0.5(C); Class 13 follows Reg 1100(1)(b) and Schedule III; Class 14 follows
/// Reg 1100(1)(c). CCA is floored to the cent so it never exceeds the maximum.
pub fn schedule8(input: &T2Input, diagnostics: &mut Vec<Value>) -> Value {
    let classes = &input.cca;
    let (mut recapture, mut terminal, mut cca) = (0i64, 0i64, 0i64);
    let mut rows = vec![];
    let short = short_year_days(input);
    if !classes.is_empty() {
        match &short {
            Err(m) => diag(diagnostics, "cca_tax_year", m.clone(), true),
            Ok(Some(d)) => diag(diagnostics, "cca_short_year", format!("Taxation year of {d} days (less than 12 months): Reg 1100(3) limits the maximum to {d}/365 (Class 14 under 1100(1)(c) is not prorated)."), false),
            Ok(None) => {}
        }
    }
    let short = short.ok().flatten();
    let needs_year = classes.iter().any(|c| c.aiip_additions_cents + c.riip_additions_cents > 0
        || c.leaseholds.iter().any(|l| l.incurred_in_year && l.acceleration != "none" && !l.acceleration.is_empty())
        || c.limited_life.iter().any(|l| l.available_in_year && l.acceleration != "none" && !l.acceleration.is_empty()));
    let years = if needs_year { match factor_year(input) { Ok(y) => Some(y), Err(m) => { diag(diagnostics, "cca_tax_year", m, true); None } } } else { None };
    // Classes 13 and 14 apply the acceleration to the year the cost is incurred; a straddle year is not split there.
    let year = match years { Some((a, b)) if a == b => Some(b), Some(_) if classes.iter().any(|c| (c.class == "13" || c.class == "14") && (c.leaseholds.iter().any(|l| l.incurred_in_year && !matches!(l.acceleration.as_str(), "" | "none")) || c.limited_life.iter().any(|l| l.available_in_year && !matches!(l.acceleration.as_str(), "" | "none")))) => { diag(diagnostics, "cca_tax_year", "Class 13/14 accelerated property in a taxation year spanning two calendar years is not modelled.", true); None }, _ => None };
    let ie_total: i64 = classes.iter().map(|c| c.immediate_expensing_cents).sum();
    let ie_ok = ie_total == 0 || immediate_expensing_check(input, ie_total, diagnostics);
    for c in classes {
        let straight = c.class == "13" || c.class == "14";
        let rate = rate(&c.class);
        if rate.is_none() && !straight {
            diag(diagnostics, "cca_class_unsupported", format!("Class {} is not modelled here (no Reg 1100(1)(a) rate carried, or a special method).", c.class), true);
            continue;
        }
        if c.accelerated_additions_cents > 0 {
            diag(diagnostics, "cca_accelerated_not_modelled", format!("Class {}: accelerated_additions_cents is a legacy field; split it into aiip_additions_cents, riip_additions_cents or half_year_exempt_additions_cents.", c.class), true);
        }
        let zev = matches!(c.class.as_str(), "54" | "55" | "56");
        let ie = if ie_ok { c.immediate_expensing_cents } else { 0 };
        if c.immediate_expensing_cents > 0 && matches!(c.class.as_str(), "1" | "2" | "3" | "4" | "5" | "6" | "14.1" | "17" | "47" | "49" | "51") {
            diag(diagnostics, "cca_immediate_expensing_class", format!("Class {} property is excluded from immediate expensing property (Reg 1104(3.1)).", c.class), true);
        }
        let regular = c.additions_cents - ie - c.aiip_additions_cents - c.riip_additions_cents - c.half_year_exempt_additions_cents;
        if zev && regular > 0 {
            diag(diagnostics, "cca_zev_acquisition_date", format!("Class {}: enter additions as aiip_additions_cents (acquired before 2025) or riip_additions_cents (acquired after 2024).", c.class), true);
        }
        let col10 = c.opening_ucc_cents + c.additions_cents + c.adjustments_cents - c.proceeds_cents;
        let mut row = json!({"class":c.class,"column_10_ucc_cents":col10});
        if c.diep_cents > 0 || ie > 0 { row["column_4_diep_cents"] = json!(c.diep_cents); row["column_9_immediate_expensing_cents"] = json!(ie); }
        if ie > col10.max(0) { diag(diagnostics, "cca_immediate_expensing_exceeds_ucc", format!("Class {}: the immediate expensing amount exceeds the UCC (Reg 1100(0.1)(b)).", c.class), true); }
        let (mut rec, mut term, mut claim) = (0, 0, 0);
        if col10 < 0 {
            if c.class == "10.1" { diag(diagnostics, "cca_class_10_1_negative", "Class 10.1 has no recapture; a negative UCC means the inputs need review.", true); }
            else { rec = -col10; }
        } else if c.no_property_left && col10 > 0 {
            if c.class == "10.1" { diag(diagnostics, "cca_class_10_1_disposed", "Class 10.1 has no terminal loss; the special half-year rule on disposal is not modelled.", true); }
            else { term = col10; }
        } else {
            let ie = ie.min(col10);
            // Reg 1100(0.2): the 1100(0.1) amount is deducted before the other CCA of the class.
            let col10 = col10 - ie;
            let full = if straight {
                straight_line_max(c, year, col10, &mut row, diagnostics)
            } else {
                let r = rate.unwrap();
                // Reg 1100(2): F = regular additions (none for the classes listed in F(b)(ii)), G = proceeds.
                let f = if zev { 0 } else { regular.max(0) };
                let g = c.proceeds_cents;
                let accel = c.aiip_additions_cents + c.riip_additions_cents;
                let mut adj: i128 = -(i128::from((f - g).max(0)) / 2);
                if accel > 0 {
                    if g > f { diag(diagnostics, "cca_proceeds_exceed_regular_additions", format!("Class {}: proceeds exceed regular additions alongside accelerated additions; the E reduction of Reg 1100(2) is not modelled.", c.class), true); }
                    if let Some((y1, y2)) = years {
                        for (amt, second, riip) in [(c.aiip_additions_cents, c.aiip_second_year_cents, false), (c.riip_additions_cents, c.riip_second_year_cents, true)] {
                            if amt == 0 { continue; }
                            let split = if y1 == y2 { vec![(amt, y2)] } else if let Some(s2) = second { vec![(amt - s2, y1), (s2, y2)] } else {
                                diag(diagnostics, "cca_straddle_split", format!("Class {}: the taxation year spans {y1} and {y2}; set {} (the part available for use in {y2}) for the Reg 1100(2.01)/(2.011) blend.", c.class, if riip { "riip_second_year_cents" } else { "aiip_second_year_cents" }), true);
                                continue;
                            };
                            let mut shown = vec![];
                            for (part, y) in split {
                                if part == 0 { continue; }
                                match factor(c, riip, y) {
                                    Ok(fac) => { adj += mul(part, fac); shown.push(if y1 == y2 { format!("{}/{}", fac.0, fac.1) } else { format!("{y}: {}/{}", fac.0, fac.1) }); }
                                    Err(m) => diag(diagnostics, "cca_factor_unsourced", m, true),
                                }
                            }
                            row[if riip { "factor_a1" } else { "factor_a" }] = json!(shown.join(", "));
                        }
                    }
                }
                row["rate_percent"] = json!(r);
                row["reg_1100_2_adjustment_cents"] = json!(adj as i64);
                row["column_17_half_year_cents"] = json!((f - g).max(0) / 2);
                (((i128::from(col10) + adj).max(0) * i128::from(r)) / 100) as i64
            };
            // Reg 1100(3): days in the year / 365 of the maximum, except 1100(1)(c) (Class 14).
            let max = match short { Some(d) if c.class != "14" => { row["short_year_proration"] = json!(format!("{d}/365")); (i128::from(full) * i128::from(d) / 365) as i64 } _ => full };
            // Column 21 maximum = column 19 × column 20 + column 9 (T4002 ch. 4). 1100(3) does not prorate 1100(0.1).
            let max = max + ie;
            claim = match c.claim_cents { Some(v) if v > max => { diag(diagnostics, "cca_claim_exceeds_maximum", format!("Class {} claim exceeds the maximum {max} cents.", c.class), true); max } Some(v) => v, None => max };
            row["maximum_cca_cents"] = json!(max);
        }
        let closing = col10 + rec - term - claim;
        row["column_19_recapture_cents"] = json!(rec); row["column_20_terminal_loss_cents"] = json!(term);
        row["column_21_cca_cents"] = json!(claim); row["ucc_end_of_year_cents"] = json!(if rec > 0 || term > 0 { 0 } else { closing });
        recapture += rec; terminal += term; cca += claim;
        rows.push(row);
    }
    json!({"classes":rows,"schedule1_line_107_recapture_cents":recapture,"schedule1_line_404_terminal_loss_cents":terminal,"schedule1_line_403_cca_cents":cca})
}

/// Reg 1100(0.1) and 1104(3.1)–(3.5) gate. Returns false (and blocks) when the claim cannot be sourced.
fn immediate_expensing_check(input: &T2Input, total: i64, diagnostics: &mut Vec<Value>) -> bool {
    use chrono::NaiveDate;
    let mut b = |m: String| { diag(diagnostics, "cca_immediate_expensing", m, true); false };
    if input.corporation_type != "ccpc" { return b("Immediate expensing is modelled for CCPCs only (eligible person, Reg 1104(3.1)); other corporations need the 1100(0.1)(c) income limit and the post-2023 rules.".into()); }
    let Some(ie) = &input.immediate_expensing else { return b("Set immediate_expensing (association, designation and DIEP conditions under Reg 1104(3.1)–(3.5)).".into()) };
    if !ie.designated_on_prescribed_form { return b("DIEP must be designated on the prescribed form (Reg 1104(3.1)).".into()); }
    if !ie.diep_conditions_confirmed { return b("Confirm each DIEP meets Reg 1104(3.1): acquired after April 18, 2021, available for use before 2024 (CCPC), not Class 1–6, 14.1, 17, 47, 49 or 51, not previously used, not acquired non-arm's-length or by rollover.".into()); }
    let p = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    let (Some(a), Some(e)) = (p(&input.tax_year_start), p(&input.tax_year_end)) else { return b("Immediate expensing needs tax_year_start and tax_year_end.".into()) };
    if e < a { return b("tax_year_end is before tax_year_start.".into()); }
    if a >= NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() { return b("A CCPC's DIEP must become available for use before 2024 (Reg 1104(3.1)); this year starts after 2023.".into()); }
    if e <= NaiveDate::from_ymd_opt(2021, 4, 18).unwrap() { return b("DIEP must be acquired after April 18, 2021 (Reg 1104(3.1)).".into()); }
    match ie.other_tax_year_ends_same_calendar_year { None => return b("Set other_tax_year_ends_same_calendar_year (Reg 1104(3.5)(a)).".into()), Some(true) => return b("Several taxation years ending in one calendar year share the limit under Reg 1104(3.5)(a); not modelled.".into()), _ => {} }
    let mut limit: i128 = 150_000_000;
    match ie.associated {
        None => return b("Set associated (Reg 1104(3.2)): the $1.5M limit is nil for associated persons without a 1104(3.3) agreement.".into()),
        Some(true) => match (ie.agreement_percent_bp, ie.agreement_total_bp) {
            (Some(pct), Some(tot)) if pct <= tot => { if tot > 10_000 { limit = 0; } else { limit = limit * i128::from(pct) / 10_000; } }
            _ => return b("Associated: enter the 1104(3.3) agreement percentage and the agreement total; without an agreement the limit is nil unless the Minister allocates it under 1104(3.4).".into()),
        },
        Some(false) => {}
    }
    let days = (e - a).num_days() + 1;
    if days < 357 { limit = limit * i128::from(days) / 365; }
    if i128::from(total) > limit { return b(format!("Immediate expensing claimed {total} cents exceeds the immediate expensing limit {limit} cents (Reg 1104(3.2), (3.5)(b)).")); }
    diag(diagnostics, "cca_immediate_expensing_limit", format!("Immediate expensing limit {limit} cents (Reg 1104(3.2){}); claimed {total} cents.", if days < 357 { format!(", prorated {days}/365 under (3.5)(b)") } else { String::new() }), false);
    true
}

/// Class 13 (Reg 1100(1)(b), Schedule III ss. 1–3) and Class 14 (Reg 1100(1)(c)), capped at UCC.
fn straight_line_max(c: &CcaClass, year: Option<i32>, ucc: i64, row: &mut Value, diagnostics: &mut Vec<Value>) -> i64 {
    let mut total: i128 = 0;
    let mut parts = vec![];
    if c.class == "13" {
        if c.leaseholds.is_empty() && ucc > 0 { diag(diagnostics, "cca_class13_parts", "Class 13 needs each leasehold part (cost, 12-month periods, prior claims).", true); }
        for l in &c.leaseholds {
            // Schedule III s. 2: lesser of 1/5 of the cost and cost ÷ 12-month periods (max 40); s. 3(c): not more than what is left.
            let base = (i128::from(l.cost_cents) / 5).min(i128::from(l.cost_cents) / i128::from(l.periods_12m)).min(i128::from(l.cost_cents - l.claimed_before_cents));
            let amt = if !l.incurred_in_year { base } else {
                let accel = match (l.acceleration.as_str(), year) {
                    ("aiip", Some(y)) => Some(y < 2024),
                    ("riip", Some(y)) => Some(y < 2030),
                    ("aiip" | "riip", None) => None,
                    _ => Some(false),
                };
                match (l.acceleration.as_str(), accel) {
                    (_, None) => 0,
                    ("aiip" | "riip", Some(true)) => base * 3 / 2,   // 1100(1)(b)(i)(A): 150% (capped at UCC below)
                    ("aiip" | "riip", Some(false)) => base,          // 1100(1)(b)(ii): any other case
                    _ => base / 2,                                     // 1100(1)(b)(i)(B): 50%
                }
            };
            parts.push(json!({"cost_cents":l.cost_cents,"periods_12m":l.periods_12m,"amount_cents":amt as i64}));
            total += amt;
        }
    } else {
        if c.limited_life.is_empty() && ucc > 0 { diag(diagnostics, "cca_class14_parts", "Class 14 needs each limited-life property (cost, life in days, days in the year).", true); }
        for l in &c.limited_life {
            let base = i128::from(l.cost_cents) * i128::from(l.days_in_year) / i128::from(l.life_days);
            let extra = if !l.available_in_year { 0 } else {
                match (l.acceleration.as_str(), year) {
                    ("aiip", Some(y)) => if y < 2024 { base / 2 } else { base / 4 },
                    ("riip", Some(y)) => if y < 2030 { base / 2 } else { base / 4 },
                    _ => 0,
                }
            };
            parts.push(json!({"cost_cents":l.cost_cents,"life_days":l.life_days,"days_in_year":l.days_in_year,"amount_cents":(base + extra) as i64}));
            total += base + extra;
        }
        if !c.limited_life.is_empty() { diag(diagnostics, "cca_class14_day_basis", "Class 14 apportions each cost over its remaining life by days; confirm this matches the property's terms.", false); }
    }
    row["method"] = json!(if c.class == "13" { "Reg 1100(1)(b) and Schedule III" } else { "Reg 1100(1)(c)" });
    row["parts"] = Value::Array(parts);
    total.min(i128::from(ucc.max(0))) as i64
}

fn identifier_ok(s: &Shareholder) -> bool {
    let id = s.identifier.trim();
    let digits = |v: &str| v.bytes().all(|b| b.is_ascii_digit());
    match s.kind.as_str() {
        "individual" => id.len() == 9 && digits(id) && luhn(id),
        "trust" => id.len() == 9 && id.starts_with('T') && digits(&id[1..]),
        "corporation" | "partnership" => id == "NR" || (id.len() == 15 && digits(&id[..9]) && id[9..11].bytes().all(|b| b.is_ascii_uppercase()) && digits(&id[11..])),
        _ => false,
    }
}
fn luhn(d: &str) -> bool {
    let sum: u32 = d.bytes().rev().enumerate().map(|(i, b)| { let n = u32::from(b - b'0'); if i % 2 == 1 { let x = n * 2; x / 10 + x % 10 } else { n } }).sum();
    sum % 10 == 0
}

/// Schedule 50. Identifiers are checked but never echoed: only a masked last-three suffix is returned.
pub fn schedule50(input: &T2Input, diagnostics: &mut Vec<Value>) -> Value {
    let private = matches!(input.corporation_type.as_str(), "ccpc" | "other_private");
    if private && input.shareholders.is_empty() {
        diag(diagnostics, "schedule50_required", "Private corporations must list every shareholder holding 10% or more of common or preferred shares.", true);
    }
    let (mut common, mut preferred) = (0u32, 0u32);
    let mut rows = vec![];
    for s in &input.shareholders {
        common += s.common_bp; preferred += s.preferred_bp;
        if s.common_bp < 1000 && s.preferred_bp < 1000 { diag(diagnostics, "schedule50_below_threshold", format!("{} holds under 10% of both share types; Schedule 50 lists only holders of 10% or more.", s.name), false); }
        if !identifier_ok(s) { diag(diagnostics, "schedule50_identifier", format!("{}: enter one identifier matching the shareholder type (SIN for an individual, BN/partnership account or NR, or T + 8 digits for a trust).", s.name), true); }
        let tail: String = s.identifier.chars().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect();
        rows.push(json!({"name":s.name,"kind":s.kind,"identifier_masked":format!("***{tail}"),"common_percent":f64::from(s.common_bp)/100.0,"preferred_percent":f64::from(s.preferred_bp)/100.0}));
    }
    if common > 10_000 || preferred > 10_000 { diag(diagnostics, "schedule50_over_100", "Listed holdings exceed 100% of a share type.", true); }
    json!({"required":private,"shareholders":rows,"listed_common_percent":f64::from(common)/100.0,"listed_preferred_percent":f64::from(preferred)/100.0})
}

/// Schedule 100/125 consistency and the links into Schedule 1.
pub fn cross_checks(input: &T2Input, s1: &Value, s8: &Value, diagnostics: &mut Vec<Value>) -> Value {
    let bs = |c| gifi(&input.gifi, "balance_sheet", c);
    let is = |c| gifi(&input.gifi, "income_statement", c);
    let mut checks = vec![];
    let mut check = |name: &str, ok: Option<bool>, blocking: bool, msg: &str, diags: &mut Vec<Value>| {
        if ok == Some(false) { diag(diags, name, msg, blocking); }
        checks.push(json!({"check":name,"passed":ok}));
    };
    check("s100_assets_equal_liabilities_and_equity", bs("2599").zip(bs("3499")).zip(bs("3620")).map(|((a, l), e)| a == l + e), true, "Schedule 100: GIFI 2599 must equal 3499 plus 3620.", diagnostics);
    check("s100_total_3640", bs("3640").zip(bs("2599")).map(|(t, a)| t == a), true, "Schedule 100: GIFI 3640 must equal total assets 2599.", diagnostics);
    check("s125_required_totals", Some(is("8299").is_some() && is("9368").is_some() && is("9999").is_some()), true, "Schedule 125 needs 8299, 9368 and 9999.", diagnostics);
    check("s1_amount_a_is_gifi_9999", is("9999").map(|v| Some(v) == s1["amount_a_gifi_9999_cents"].as_i64()), true, "Schedule 1 amount A must be GIFI 9999.", diagnostics);
    let line = |l: &str| s1["lines"].as_array().into_iter().flatten().find(|r| r["line"] == l).and_then(|r| r["amount_cents"].as_i64()).unwrap_or(0);
    check("s1_101_matches_gifi_9990", is("9990").map(|v| v == line("101")), false, "Line 101 usually adds back current income tax GIFI 9990.", diagnostics);
    check("s1_102_matches_gifi_9995", is("9995").map(|v| v == line("102")), false, "Line 102 usually adds back deferred income tax GIFI 9995.", diagnostics);
    check("s1_104_matches_gifi_8670", is("8670").map(|v| v == line("104")), false, "Line 104 usually adds back amortization of tangible assets GIFI 8670.", diagnostics);
    if !input.cca.is_empty() {
        for (l, k) in [("403", "schedule1_line_403_cca_cents"), ("404", "schedule1_line_404_terminal_loss_cents"), ("107", "schedule1_line_107_recapture_cents")] {
            check(&format!("s1_{l}_matches_schedule8"), Some(line(l) == s8[k].as_i64().unwrap_or(0)), true, &format!("Schedule 1 line {l} must equal the Schedule 8 total."), diagnostics);
        }
    }
    check("s50_share_capital_has_holders", bs("3500").map(|v| v == 0 || !input.shareholders.is_empty()), false, "Share capital (GIFI 3500) is reported but no shareholders are listed.", diagnostics);
    Value::Array(checks)
}

pub fn build(input: &T2Input) -> Result<Value, String> {
    validate(input)?;
    let mut diagnostics = vec![];
    let ni = gifi(&input.gifi, "income_statement", "9999").unwrap_or(0);
    let s1 = schedule1(ni, &input.schedule1, &mut diagnostics);
    let s8 = schedule8(input, &mut diagnostics);
    let s50 = schedule50(input, &mut diagnostics);
    let checks = cross_checks(input, &s1, &s8, &mut diagnostics);
    let supported = !diagnostics.iter().any(|d| d["blocks_review"] == true);
    Ok(json!({"format":"nxt-t2-schedules/v1","scope":SCOPE,"supported":supported,"filed":false,
        "schedule_1":s1,"schedule_8":s8,"schedule_50":s50,"cross_checks":checks,"diagnostics":diagnostics,
        "sources":SOURCES.iter().map(|(t,u)| json!({"title":t,"url":u})).collect::<Vec<_>>()}))
}
