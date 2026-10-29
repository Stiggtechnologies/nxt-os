//! Versioned CRA/provincial rate tables used by the native engine, and the
//! per-line validation register. Rates come from published CRA/provincial
//! sources (URLs below). "validated" means the line was reproduced against a
//! published CRA worked example recorded in `VALIDATIONS`; everything else is
//! "unvalidated". Nothing here is a claim of certified accuracy.
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RateTable {
    pub id: &'static str,
    pub tax_year: i32,
    /// (upper bound in cents, rate in basis points)
    pub federal_brackets: &'static [(i64, i64)],
    pub alberta_brackets: &'static [(i64, i64)],
    pub ontario_brackets: &'static [(i64, i64)],
    pub federal_credit_rate_bps: i64,
    pub federal_bpa_max_cents: i64,
    pub federal_bpa_min_cents: i64,
    pub canada_employment_amount_cents: i64,
    pub alberta_bpa_cents: i64,
    pub ontario_bpa_cents: i64,
    pub t2_federal_general_bps: i64,
    pub t2_federal_small_bps: i64,
    pub t2_alberta_general_bps: i64,
    pub t2_alberta_small_bps: i64,
    pub business_limit_cents: i64,
    pub sources: &'static [(&'static str, &'static str)],
}

pub const CA_2025: RateTable = RateTable {
    id: "ca-2025.1",
    tax_year: 2025,
    federal_brackets: &[(5_737_500,1450),(11_475_000,2050),(17_788_200,2600),(25_341_400,2900),(i64::MAX,3300)],
    alberta_brackets: &[(6_000_000,800),(15_123_400,1000),(18_148_100,1200),(24_197_400,1300),(36_296_100,1400),(i64::MAX,1500)],
    ontario_brackets: &[(5_288_600,505),(10_577_500,915),(15_000_000,1116),(22_000_000,1216),(i64::MAX,1316)],
    federal_credit_rate_bps: 1450,
    federal_bpa_max_cents: 1_612_900,
    federal_bpa_min_cents: 1_453_800,
    canada_employment_amount_cents: 147_100,
    alberta_bpa_cents: 2_232_300,
    ontario_bpa_cents: 1_274_700,
    t2_federal_general_bps: 1500,
    t2_federal_small_bps: 900,
    t2_alberta_general_bps: 800,
    t2_alberta_small_bps: 200,
    business_limit_cents: 50_000_000,
    sources: &[
        ("CRA tax rates and brackets", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/frequently-asked-questions-individuals/canadian-income-tax-rates-individuals-current-previous-years.html"),
        ("CRA line 30000 basic personal amount", "https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-30000-basic-personal-amount.html"),
        ("CRA corporation tax rates", "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/corporations/corporation-tax-rates.html"),
    ],
};

pub const TABLES: &[RateTable] = &[CA_2025];

pub const MB_FINANCE_2025: &str = "https://www.gov.mb.ca/finance/personal/ptaxes.html";

/// 2025 T1 provincial/territorial tables for the generic engine (AB and ON have
/// their own calculators). Brackets: CRA "previous year" rates page; basic personal
/// amounts: CRA T4127-JUL 2025 (Manitoba: Manitoba Finance, indexation frozen).
/// `not_calculated` lists provincial adjustments the generic engine does not apply.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Province {
    pub code: &'static str,
    pub name: &'static str,
    pub brackets: &'static [(i64, i64)],
    pub credit_rate_bps: i64,
    /// None = uses the federal BPA with its phase-out (Yukon).
    pub bpa_cents: Option<i64>,
    pub employment_amount: bool,
    pub bracket_source: &'static str,
    pub bpa_source: &'static str,
    /// T4127 Table 8.1 KP constants (dollars) the brackets must reproduce.
    pub t4127_kp: &'static [i64],
    pub not_calculated: &'static str,
}

pub const PROVINCES_2025: &[Province] = &[
    Province { code:"BC", name:"British Columbia", brackets:&[(4_927_900,506),(9_856_000,770),(11_315_800,1050),(13_740_700,1229),(18_630_600,1470),(25_982_900,1680),(i64::MAX,2050)], credit_rate_bps:506, bpa_cents:Some(1_293_200), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_301,4_061,6_086,9_398,13_310,22_924], not_calculated:"BC tax reduction (low-income), BC428 credits other than BPA/CPP/EI" },
    Province { code:"SK", name:"Saskatchewan", brackets:&[(5_346_300,1050),(15_275_000,1250),(i64::MAX,1450)], credit_rate_bps:1050, bpa_cents:Some(1_949_100), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_069,4_124], not_calculated:"SK428 credits other than BPA/CPP/EI (e.g. dependant, age, pension, dividend)" },
    Province { code:"MB", name:"Manitoba", brackets:&[(4_700_000,1080),(10_000_000,1275),(i64::MAX,1740)], credit_rate_bps:1080, bpa_cents:Some(1_578_000), employment_amount:false, bracket_source:MB_FINANCE_2025, bpa_source:MB_FINANCE_2025, t4127_kp:&[917,5_567], not_calculated:"Manitoba BPA phase-out for net income $200,000-$400,000 (blocked above $200,000), Family Tax Benefit, MB428 credits other than BPA/CPP/EI" },
    Province { code:"NB", name:"New Brunswick", brackets:&[(5_130_600,940),(10_261_400,1400),(19_006_000,1600),(i64::MAX,1950)], credit_rate_bps:940, bpa_cents:Some(1_339_600), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[2_360,4_412,11_064], not_calculated:"NB low-income tax reduction, NB428 credits other than BPA/CPP/EI" },
    Province { code:"NS", name:"Nova Scotia", brackets:&[(3_050_700,879),(6_101_500,1495),(9_588_300,1667),(15_465_000,1750),(i64::MAX,2100)], credit_rate_bps:879, bpa_cents:Some(1_174_400), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_879,2_929,3_725,9_137], not_calculated:"NS low-income tax reduction, age amount supplement, NS428 credits other than BPA/CPP/EI" },
    Province { code:"PE", name:"Prince Edward Island", brackets:&[(3_332_800,950),(6_465_600,1347),(10_500_000,1660),(14_000_000,1762),(i64::MAX,1900)], credit_rate_bps:950, bpa_cents:Some(1_465_000), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_323,3_347,4_418,6_350], not_calculated:"PE low-income tax reduction, PE428 credits other than BPA/CPP/EI" },
    Province { code:"NL", name:"Newfoundland and Labrador", brackets:&[(4_419_200,870),(8_838_200,1450),(15_779_200,1580),(22_091_000,1780),(28_221_400,1980),(56_442_900,2080),(112_885_800,2130),(i64::MAX,2180)], credit_rate_bps:870, bpa_cents:Some(1_106_700), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[2_563,3_712,6_868,11_286,14_108,16_930,22_575], not_calculated:"NL low-income tax reduction, NL428 credits other than BPA/CPP/EI" },
    Province { code:"YT", name:"Yukon", brackets:&[(5_737_500,640),(11_475_000,900),(17_788_200,1090),(50_000_000,1280),(i64::MAX,1500)], credit_rate_bps:640, bpa_cents:None, employment_amount:true, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_492,3_672,7_052,18_052], not_calculated:"YT428 credits other than BPA/CEA/CPP/EI, Yukon low-income family credit" },
    Province { code:"NT", name:"Northwest Territories", brackets:&[(5_196_400,590),(10_393_000,860),(16_896_700,1220),(i64::MAX,1405)], credit_rate_bps:590, bpa_cents:Some(1_784_200), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_403,5_145,8_270], not_calculated:"NT428 credits other than BPA/CPP/EI, cost of living credits" },
    Province { code:"NU", name:"Nunavut", brackets:&[(5_470_700,400),(10_941_300,700),(17_788_100,900),(i64::MAX,1150)], credit_rate_bps:400, bpa_cents:Some(1_927_400), employment_amount:false, bracket_source:CRA_LAST_YEAR, bpa_source:CRA_T4127_JUL_2025, t4127_kp:&[1_641,3_829,8_276], not_calculated:"NU428 credits other than BPA/CPP/EI, cost of living credit" },
];
pub fn province(code: &str) -> Option<&'static Province> { PROVINCES_2025.iter().find(|p| p.code == code) }
pub fn for_year(year: i32) -> Option<&'static RateTable> { TABLES.iter().find(|t| t.tax_year == year) }

pub const CRA_LAST_YEAR: &str = "https://www.canada.ca/en/revenue-agency/services/tax/individuals/tax-rates-brackets/last-year.html";
pub const CRA_T4127_JUL_2025: &str = "https://www.canada.ca/en/revenue-agency/services/forms-publications/payroll/payroll-deductions-t4127-payroll-deductions-formulas/t4127-jul-121st-edition-effective-july-1-2025/t4127-jul-payroll-deductions-formulas.html";
pub const CRA_INDEXATION: &str = "https://www.canada.ca/en/revenue-agency/services/tax/individuals/frequently-asked-questions-individuals/adjustment-personal-income-tax-benefit-amounts.html";
pub const CRA_RRSP_LIMITS: &str = "https://www.canada.ca/en/revenue-agency/services/tax/registered-plans-administrators/pspa/mp-rrsp-dpsp-tfsa-limits-ympe.html";

/// Per-rate source register: (table id, rate key, value as published, source URL, status).
/// status is "sourced" only when the value was compared to the cited official page
/// (checked 2026-10-29). "unvalidated" means no official source was matched.
pub const RATE_SOURCES: &[(&str, &str, &str, &str, &str)] = &[
    ("ca-2025.1","federal_brackets","14.5% to $57,375; 20.5% to $114,750; 26% to $177,882; 29% to $253,414; 33% above",CRA_LAST_YEAR,"sourced"),
    ("ca-2025.1","federal_credit_rate_bps","14.5% full-year 2025 (15% Jan-Jun, 14% Jul-Dec)",CRA_LAST_YEAR,"sourced"),
    ("ca-2025.1","federal_bpa_max_cents","$16,129 (net income <= $177,882)",CRA_INDEXATION,"sourced"),
    ("ca-2025.1","federal_bpa_min_cents","$14,538 (net income >= $253,414)",CRA_INDEXATION,"sourced"),
    ("ca-2025.1","canada_employment_amount_cents","$1,471",CRA_INDEXATION,"sourced"),
    ("ca-2025.1","alberta_brackets","8% to $60,000; 10% to $151,234; 12% to $181,481; 13% to $241,974; 14% to $362,961; 15% above",CRA_LAST_YEAR,"sourced"),
    ("ca-2025.1","alberta_credit_rate_bps","8% (lowest Alberta rate; T4127 Option 2 K1P = 0.0800 x TCP)",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","alberta_supplemental_credit","2% of non-refundable credit amounts above $60,000 (T4127 K5P = (K1P+K2P-$4,800) x 0.02/0.08)",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","alberta_bpa_cents","$22,323",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","ontario_brackets","5.05% to $52,886; 9.15% to $105,775; 11.16% to $150,000; 12.16% to $220,000; 13.16% above",CRA_LAST_YEAR,"sourced"),
    ("ca-2025.1","ontario_bpa_cents","$12,747",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","ontario_surtax","20% of basic ON tax over $5,710; 36% over $7,307 (T4127 Table 8.2)",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","ontario_tax_reduction_basic","$294 x 2 (T4127 Table 8.2 S2)",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","ontario_health_premium","Chart thresholds $20,000-$200,600, max $900",CRA_T4127_JUL_2025,"unvalidated"),
    ("ca-2025.1","ontario_spouse_amount","$10,823 max, reduced by spouse net income over $1,082",CRA_T4127_JUL_2025,"unvalidated"),
    ("ca-2025.1","cpp_base_max_cents","$3,356.10 base CPP credit maximum",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","ei_max_cents","$1,077.48 EI premium maximum",CRA_T4127_JUL_2025,"sourced"),
    ("ca-2025.1","cpp_enhanced_max_cents","$1,074.00 (first + second additional CPP deduction maximum, derived)",CRA_T4127_JUL_2025,"unvalidated"),
    ("ca-2025.1","medical_threshold_cents","$2,834 (3% of net income ceiling)",CRA_INDEXATION,"sourced"),
    ("ca-2025.1","rrsp_dollar_limit_cents","$32,490",CRA_RRSP_LIMITS,"sourced"),
    ("ca-2025.1","t2_rates","Federal 15%/9%, Alberta 8%/2%, $500,000 business limit","https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/corporations/corporation-tax-rates.html","unvalidated"),
];

pub fn rate_status(table: &str, key: &str) -> &'static str {
    RATE_SOURCES.iter().find(|r| r.0 == table && r.1 == key).map(|r| r.4).unwrap_or("unvalidated")
}

/// (table id, line code, published reference reproduced by a test in this repo).
/// A line is "validated" only when every input rate is sourced and a test reproduces
/// a figure CRA publishes (T4127 bracket constants K/KP, indexation table values).
/// These are not full CRA T1 worked examples; see docs/TAX_MODULE.md.
pub const VALIDATIONS: &[(&str, &str, &str)] = &[
    ("ca-2025.1","federal_before_credits","T4127-JUL 2025 Option 2 federal constants K = 3,443 / 9,754 / 15,090 / 25,227 (test rates::tests::federal_brackets_reproduce_t4127_constants)"),
    ("ca-2025.1","ab_before_credits","T4127-JUL 2025 Option 2 Alberta constants KP = 1,200 / 4,225 / 6,039 / 8,459 / 12,089 (test rates::tests::alberta_brackets_reproduce_t4127_constants)"),
    ("ca-2025.1","on_before_credits","T4127-JUL 2025 Table 8.1 Ontario constants KP = 2,168 / 4,294 / 5,794 / 7,994 (test rates::tests::ontario_brackets_reproduce_t4127_constants)"),
    ("ca-2025.1","30000","CRA indexation table: BPA $16,129 max / $14,538 min, phase-out $177,882-$253,414 (test calculations::tests::federal_brackets_and_bpa_phaseout_have_exact_boundaries)"),
    ("ca-2025.1","31260","CRA indexation table: Canada employment amount $1,471"),
    ("ca-2025.1","AB58040","T4127-JUL 2025: Alberta basic personal amount $22,323"),
    ("ca-2025.1","ON58040","T4127-JUL 2025: Ontario basic personal amount $12,747"),
];

/// Generic-engine provinces are validated on `{xx}_before_credits` because a test
/// reproduces every T4127 Table 8.1 KP constant from the brackets.
pub fn line_status(table: &str, code: &str) -> &'static str {
    if table == "ca-2025.1" && let Some(p) = code.strip_suffix("_before_credits") && province(&p.to_uppercase()).is_some() { return "validated"; }
    if VALIDATIONS.iter().any(|v| v.0 == table && v.1 == code) { "validated" } else { "unvalidated" }
}

pub fn catalog() -> Value {
    json!({"tables": TABLES, "validations": VALIDATIONS.iter().map(|v| json!({"table": v.0, "line": v.1, "example": v.2})).collect::<Vec<_>>(),
        "provinces": PROVINCES_2025,
        "rate_sources": RATE_SOURCES.iter().map(|r| json!({"table": r.0, "key": r.1, "value": r.2, "url": r.3, "status": r.4})).collect::<Vec<_>>(),
        "statement": "Rates are transcribed from published CRA/provincial sources. Lines are 'unvalidated' unless reproduced against a published CRA example. Not certified."})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn brackets_are_ordered_and_terminated() {
        for t in TABLES { for b in [t.federal_brackets, t.alberta_brackets, t.ontario_brackets] {
            assert!(b.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1)); assert_eq!(b.last().unwrap().0, i64::MAX);
        } }
        assert!(for_year(2025).is_some()); assert!(for_year(2019).is_none());
        assert_eq!(line_status("ca-2025.1", "42000"), "unvalidated");
        assert_eq!(line_status("ca-2025.1", "federal_before_credits"), "validated");
    }
    /// Tax on bracket ceiling c at rate r equals r*c - K where K is CRA's published
    /// constant. We check (top_rate * A - tax(A)) rounded to dollars equals K.
    fn constants(brackets: &[(i64, i64)]) -> Vec<i64> {
        let mut out = vec![]; let mut prev = 0_i64; let mut tax_num = 0_i128;
        for w in brackets.windows(2) {
            let (ceil, rate) = w[0]; let next_rate = w[1].1;
            tax_num += i128::from(ceil - prev) * i128::from(rate); prev = ceil;
            // K at threshold = next_rate*ceil - tax(ceil), in cents*bps -> dollars
            let k = i128::from(ceil) * i128::from(next_rate) - tax_num;
            out.push(((k + 500_000) / 1_000_000) as i64);
        }
        out
    }
    #[test] fn federal_brackets_reproduce_t4127_constants() {
        assert_eq!(constants(CA_2025.federal_brackets), vec![3_443, 9_754, 15_090, 25_227]);
    }
    #[test] fn alberta_brackets_reproduce_t4127_constants() {
        assert_eq!(constants(CA_2025.alberta_brackets), vec![1_200, 4_225, 6_039, 8_459, 12_089]);
    }
    #[test] fn ontario_brackets_reproduce_t4127_constants() {
        assert_eq!(constants(CA_2025.ontario_brackets), vec![2_168, 4_294, 5_794, 7_994]);
    }
    #[test] fn all_province_brackets_reproduce_t4127_kp_constants() {
        for p in PROVINCES_2025 {
            assert_eq!(constants(p.brackets), p.t4127_kp.to_vec(), "{}", p.code);
            assert_eq!(p.credit_rate_bps, p.brackets[0].1, "{} credit rate = lowest rate", p.code);
            assert_eq!(p.brackets.last().unwrap().0, i64::MAX);
            assert!(p.bracket_source.starts_with("https://www."));
        }
        assert_eq!(line_status("ca-2025.1","bc_before_credits"),"validated");
        assert_eq!(line_status("ca-2025.1","qc_before_credits"),"unvalidated");
    }
    #[test] fn every_validated_line_has_only_sourced_inputs_and_urls_are_official() {
        for r in RATE_SOURCES { assert!(r.3.starts_with("https://www.canada.ca/"), "{}", r.1); assert!(["sourced","unvalidated"].contains(&r.4)); }
        for k in ["federal_brackets","alberta_brackets","ontario_brackets","federal_bpa_max_cents","alberta_bpa_cents","ontario_bpa_cents","alberta_credit_rate_bps"] { assert_eq!(rate_status("ca-2025.1", k), "sourced", "{k}"); }
        assert_eq!(rate_status("ca-2025.1", "missing"), "unvalidated");
    }
}
