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
pub fn for_year(year: i32) -> Option<&'static RateTable> { TABLES.iter().find(|t| t.tax_year == year) }

/// (table id, line code, CRA example reference). Empty until a line has been
/// reproduced against a published CRA example and the test is in this repo.
pub const VALIDATIONS: &[(&str, &str, &str)] = &[];

pub fn line_status(table: &str, code: &str) -> &'static str {
    if VALIDATIONS.iter().any(|v| v.0 == table && v.1 == code) { "validated" } else { "unvalidated" }
}

pub fn catalog() -> Value {
    json!({"tables": TABLES, "validations": VALIDATIONS.iter().map(|v| json!({"table": v.0, "line": v.1, "example": v.2})).collect::<Vec<_>>(),
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
    }
}
