//! Bounded 2026 payroll deductions for Alberta employees.
//!
//! This is an original estimate engine, not certified payroll software. It applies the
//! published 2026 federal and Alberta bracket rates, the 2026 CPP/CPP2 and EI maximums, and
//! non-refundable credits supplied as TD1 claim amounts. It does not model CPP/QPP outside
//! Alberta, Quebec, special payments, retroactive pay, multiple employers, TD1 additional
//! tax, garnishments, or any provincial payroll levy. Unsupported provinces are blocked
//! rather than approximated.
//!
//! Sources for the pinned constants (retrieved 2026-11-18):
//! - CRA current-year rates: <https://www.canada.ca/en/revenue-agency/services/tax/individuals/tax-rates-brackets/current-year.html>
//! - CRA indexation amounts: <https://www.canada.ca/en/revenue-agency/services/tax/individuals/frequently-asked-questions-individuals/adjustment-personal-income-tax-benefit-amounts.html>
//! - CRA CPP rates and maximums: <https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/payroll/payroll-deductions-contributions/canada-pension-plan-cpp/cpp-contribution-rates-maximums-exemptions.html>
//! - EI 2026 rate and maximum insurable earnings: <https://www.canada.ca/en/employment-social-development/news/2025/09/canada-employment-insurance-commission-sets-the-2026-employment-insurance-premium-rate.html>

use serde::{Deserialize, Serialize};

pub const YEAR: i32 = 2026;

/// Statutory 2026 amounts, in cents and basis points.
pub mod constants {
    /// CPP: year's maximum pensionable earnings and the basic exemption.
    pub const CPP_YMPE_CENTS: i64 = 7_460_000;
    pub const CPP_EXEMPTION_CENTS: i64 = 350_000;
    /// Employee base plus first additional contribution rate: 5.95%.
    pub const CPP_RATE_BP: i64 = 595;
    /// CPP2: employee rate on earnings between YMPE and YAMPE.
    pub const CPP2_YAMPE_CENTS: i64 = 8_500_000;
    pub const CPP2_RATE_BP: i64 = 400;
    /// EI: employee premium rate and maximum insurable earnings.
    pub const EI_RATE_BP: i64 = 163;
    pub const EI_MIE_CENTS: i64 = 6_890_000;
    /// Federal bracket ceilings and rates; the last rate applies with no ceiling.
    pub const FEDERAL_BRACKETS: &[(i64, i64)] = &[
        (5_852_300, 1_400),
        (11_704_500, 2_050),
        (18_144_000, 2_600),
        (25_848_200, 2_900),
    ];
    pub const FEDERAL_TOP_BP: i64 = 3_300;
    /// Alberta bracket ceilings and rates.
    pub const ALBERTA_BRACKETS: &[(i64, i64)] = &[
        (6_120_000, 800),
        (15_425_900, 1_000),
        (18_511_100, 1_200),
        (24_681_300, 1_300),
        (37_022_000, 1_400),
    ];
    pub const ALBERTA_TOP_BP: i64 = 1_500;
    /// Non-refundable credit amounts. Provincial basic personal amount must come from the
    /// employee's TD1; the 2026 federal maximum is published.
    pub const FEDERAL_BPA_MAX_CENTS: i64 = 1_645_200;
    pub const FEDERAL_BPA_MIN_CENTS: i64 = 1_482_900;
    pub const BPA_PHASE_START_CENTS: i64 = 18_144_000;
    pub const BPA_PHASE_END_CENTS: i64 = 25_848_200;
    pub const CANADA_EMPLOYMENT_CENTS: i64 = 150_100;
    /// Federal abatement for residents outside Quebec.
    pub const FEDERAL_ABATEMENT_BP: i64 = 1_000;
}

/// Read-only description of the pinned statutory amounts. Payroll settings are
/// deliberately not editable at runtime: a rate change ships with a ruleset release.
#[derive(Clone, Debug, Serialize)]
pub struct Settings {
    pub jurisdiction: String,
    pub year: i32,
    pub currency: String,
    pub editable: bool,
    pub cpp_ympe_cents: i64,
    pub cpp_exemption_cents: i64,
    pub cpp_rate_bp: i64,
    pub cpp2_yampe_cents: i64,
    pub cpp2_rate_bp: i64,
    pub ei_rate_bp: i64,
    pub ei_mie_cents: i64,
    pub federal_brackets: Vec<(i64, i64)>,
    pub federal_top_bp: i64,
    pub alberta_brackets: Vec<(i64, i64)>,
    pub alberta_top_bp: i64,
    pub federal_bpa_max_cents: i64,
    pub federal_bpa_min_cents: i64,
    pub canada_employment_cents: i64,
    pub federal_abatement_bp: i64,
    pub sources: Vec<String>,
    pub note: String,
}

pub fn settings() -> Settings {
    use constants::*;
    Settings {
        jurisdiction: "Alberta".into(),
        year: YEAR,
        currency: "CAD".into(),
        editable: false,
        cpp_ympe_cents: CPP_YMPE_CENTS,
        cpp_exemption_cents: CPP_EXEMPTION_CENTS,
        cpp_rate_bp: CPP_RATE_BP,
        cpp2_yampe_cents: CPP2_YAMPE_CENTS,
        cpp2_rate_bp: CPP2_RATE_BP,
        ei_rate_bp: EI_RATE_BP,
        ei_mie_cents: EI_MIE_CENTS,
        federal_brackets: FEDERAL_BRACKETS.to_vec(),
        federal_top_bp: FEDERAL_TOP_BP,
        alberta_brackets: ALBERTA_BRACKETS.to_vec(),
        alberta_top_bp: ALBERTA_TOP_BP,
        federal_bpa_max_cents: FEDERAL_BPA_MAX_CENTS,
        federal_bpa_min_cents: FEDERAL_BPA_MIN_CENTS,
        canada_employment_cents: CANADA_EMPLOYMENT_CENTS,
        federal_abatement_bp: FEDERAL_ABATEMENT_BP,
        sources: vec![
            "https://www.canada.ca/en/revenue-agency/services/tax/individuals/tax-rates-brackets/current-year.html".into(),
            "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/payroll/payroll-deductions-contributions/canada-pension-plan-cpp/cpp-contribution-rates-maximums-exemptions.html".into(),
            "https://www.canada.ca/en/employment-social-development/news/2025/09/canada-employment-insurance-commission-sets-the-2026-employment-insurance-premium-rate.html".into(),
        ],
        note: "Pinned 2026 Alberta amounts. They cannot be edited at runtime; a rate change ships with a ruleset release. This is an original estimate engine, not certified payroll software.".into(),
    }
}

/// Provinces this engine can compute. Anything else is blocked, never approximated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Province {
    Alberta,
}

impl Province {
    pub fn parse(value: &str) -> Option<Province> {
        match value.trim().to_ascii_uppercase().as_str() {
            "AB" | "ALBERTA" => Some(Province::Alberta),
            _ => None,
        }
    }
}

/// Per-employee election amounts, normally taken from the employee's TD1 forms.
#[derive(Clone, Debug)]
pub struct Options {
    pub province: Province,
    /// Federal TD1 claim amount, in cents. The engine adds the indexed federal BPA itself,
    /// so this carries any additional credits beyond the basic personal amount.
    pub federal_claim_cents: i64,
    /// Provincial TD1 claim amount, in cents. Carries the Alberta basic personal amount.
    pub provincial_claim_cents: i64,
    /// Canada employment amount for the year, in cents; 0 when not claimed.
    pub canada_employment_cents: i64,
}

impl Default for Options {
    fn default() -> Self {
        Options { province: Province::Alberta, federal_claim_cents: 0, provincial_claim_cents: 0, canada_employment_cents: constants::CANADA_EMPLOYMENT_CENTS }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    UnsupportedProvince,
    NegativeAmount,
    ExcessiveClaim,
}

impl Block {
    pub fn message(&self) -> &'static str {
        match self {
            Block::UnsupportedProvince => "Only Alberta employees are supported; other provinces and Quebec are blocked.",
            Block::NegativeAmount => "Annual employment income cannot be negative.",
            Block::ExcessiveClaim => "TD1 claim amounts cannot exceed the annual income.",
        }
    }
}

/// Annual totals for one employee. All amounts are cents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Annual {
    pub year: i32,
    pub gross_cents: i64,
    pub pensionable_cents: i64,
    pub insurable_cents: i64,
    pub cpp_cents: i64,
    pub cpp2_cents: i64,
    pub ei_cents: i64,
    pub federal_tax_cents: i64,
    pub provincial_tax_cents: i64,
    pub net_cents: i64,
}

fn round_bp(amount_cents: i64, bp: i64) -> i64 {
    // Half-up on the cent, computed in i128 so large salaries cannot overflow.
    let raw = (amount_cents as i128) * (bp as i128);
    let rounded = (raw + 5_000) / 10_000;
    rounded as i64
}

fn bracket_tax(taxable: i64, brackets: &[(i64, i64)], top_bp: i64) -> i64 {
    let mut tax = 0_i64;
    let mut floor = 0_i64;
    for (ceiling, rate_bp) in brackets {
        let ceiling = *ceiling;
        if taxable <= floor {
            break;
        }
        let slice = taxable.min(ceiling) - floor;
        tax += round_bp(slice, *rate_bp);
        floor = ceiling;
    }
    if taxable > floor {
        tax += round_bp(taxable - floor, top_bp);
    }
    tax
}

/// The federal basic personal amount, phased out above the 29% bracket threshold.
pub fn federal_bpa_cents(annual_income_cents: i64) -> i64 {
    use constants::*;
    if annual_income_cents <= BPA_PHASE_START_CENTS {
        return FEDERAL_BPA_MAX_CENTS;
    }
    if annual_income_cents >= BPA_PHASE_END_CENTS {
        return FEDERAL_BPA_MIN_CENTS;
    }
    let span = BPA_PHASE_END_CENTS - BPA_PHASE_START_CENTS;
    let decline = FEDERAL_BPA_MAX_CENTS - FEDERAL_BPA_MIN_CENTS;
    let over = annual_income_cents - BPA_PHASE_START_CENTS;
    FEDERAL_BPA_MAX_CENTS - ((decline as i128 * over as i128) / span as i128) as i64
}

/// Computes one employee's annual deductions and net pay.
pub fn annual(gross_cents: i64, options: &Options) -> Result<Annual, Block> {
    use constants::*;
    if options.province != Province::Alberta {
        return Err(Block::UnsupportedProvince);
    }
    if gross_cents < 0 {
        return Err(Block::NegativeAmount);
    }
    if options.federal_claim_cents < 0 || options.provincial_claim_cents < 0 || options.canada_employment_cents < 0 {
        return Err(Block::NegativeAmount);
    }

    // CPP: employee contribution on earnings above the exemption, capped at the YMPE.
    let base_pensionable = (gross_cents.min(CPP_YMPE_CENTS) - CPP_EXEMPTION_CENTS).max(0);
    let cpp = round_bp(base_pensionable, CPP_RATE_BP);
    // CPP2: earnings between the YMPE and the YAMPE.
    let second_pensionable = (gross_cents.min(CPP2_YAMPE_CENTS) - CPP_YMPE_CENTS).max(0);
    let cpp2 = round_bp(second_pensionable, CPP2_RATE_BP);
    // EI: capped at the maximum insurable earnings.
    let insurable = gross_cents.min(EI_MIE_CENTS);
    let ei = round_bp(insurable, EI_RATE_BP);

    let pensionable = gross_cents.min(CPP_YMPE_CENTS);
    let taxable = (gross_cents - cpp - cpp2 - ei).max(0);

    // Non-refundable credits at the lowest bracket rate, plus the employee's TD1 amounts.
    let federal_credits = federal_bpa_cents(taxable) + options.canada_employment_cents + options.federal_claim_cents + cpp + cpp2 + ei;
    let provincial_credits = options.provincial_claim_cents + cpp + cpp2;
    let federal_credit_value = round_bp(federal_credits, FEDERAL_BRACKETS[0].1);
    let provincial_credit_value = round_bp(provincial_credits, ALBERTA_BRACKETS[0].1);

    let federal_before_abatement = (bracket_tax(taxable, FEDERAL_BRACKETS, FEDERAL_TOP_BP) - federal_credit_value).max(0);
    let federal_tax = federal_before_abatement - round_bp(federal_before_abatement, FEDERAL_ABATEMENT_BP);
    let provincial_tax = (bracket_tax(taxable, ALBERTA_BRACKETS, ALBERTA_TOP_BP) - provincial_credit_value).max(0);

    let net = gross_cents - cpp - cpp2 - ei - federal_tax - provincial_tax;
    Ok(Annual {
        year: YEAR,
        gross_cents,
        pensionable_cents: pensionable,
        insurable_cents: insurable,
        cpp_cents: cpp,
        cpp2_cents: cpp2,
        ei_cents: ei,
        federal_tax_cents: federal_tax,
        provincial_tax_cents: provincial_tax,
        net_cents: net,
    })
}

/// Splits an annual figure across pay periods, putting the remainder on the last period so
/// the periods always add back to the annual total.
pub fn per_period(annual: &Annual, periods: u32) -> Result<Vec<Annual>, Block> {
    if periods == 0 || periods > 52 {
        return Err(Block::NegativeAmount);
    }
    let split = |total: i64| -> Vec<i64> {
        let each = total / periods as i64;
        let mut parts = vec![each; periods as usize];
        let remainder = total - each * periods as i64;
        if let Some(last) = parts.last_mut() {
            *last += remainder;
        }
        parts
    };
    let gross = split(annual.gross_cents);
    let cpp = split(annual.cpp_cents);
    let cpp2 = split(annual.cpp2_cents);
    let ei = split(annual.ei_cents);
    let federal = split(annual.federal_tax_cents);
    let provincial = split(annual.provincial_tax_cents);
    Ok((0..periods as usize)
        .map(|index| Annual {
            year: annual.year,
            gross_cents: gross[index],
            pensionable_cents: annual.pensionable_cents,
            insurable_cents: annual.insurable_cents,
            cpp_cents: cpp[index],
            cpp2_cents: cpp2[index],
            ei_cents: ei[index],
            federal_tax_cents: federal[index],
            provincial_tax_cents: provincial[index],
            net_cents: gross[index] - cpp[index] - cpp2[index] - ei[index] - federal[index] - provincial[index],
        })
        .collect())
}

/// T4 box totals derived from the annual amounts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct T4 {
    pub year: i32,
    /// Box 14: total employment income.
    pub box14_employment_income_cents: i64,
    /// Box 16: employee's CPP contributions (base and first additional).
    pub box16_cpp_cents: i64,
    /// Box 16A: employee's second additional CPP contributions.
    pub box16a_cpp2_cents: i64,
    /// Box 18: employee's EI premiums.
    pub box18_ei_cents: i64,
    /// Box 24: insurable earnings.
    pub box24_insurable_cents: i64,
    /// Box 26: pensionable earnings.
    pub box26_pensionable_cents: i64,
}

pub fn t4(annual: &Annual) -> T4 {
    T4 {
        year: annual.year,
        box14_employment_income_cents: annual.gross_cents,
        box16_cpp_cents: annual.cpp_cents,
        box16a_cpp2_cents: annual.cpp2_cents,
        box18_ei_cents: annual.ei_cents,
        box24_insurable_cents: annual.insurable_cents,
        box26_pensionable_cents: annual.pensionable_cents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Federal and Alberta basic personal amounts are supplied as TD1 claims, mirroring how
    /// an employer receives them.
    fn options() -> Options {
        Options { province: Province::Alberta, federal_claim_cents: 0, provincial_claim_cents: 2_276_900, canada_employment_cents: constants::CANADA_EMPLOYMENT_CENTS }
    }

    #[test]
    fn cpp_and_ei_stop_at_their_maximums() {
        let high = annual(12_000_000, &options()).unwrap();
        // 71,100 pensionable dollars at 5.95% is the published maximum of 4,230.45.
        assert_eq!(high.cpp_cents, 423_045);
        // 10,400 dollars between the YMPE and YAMPE at 4% is 416.00.
        assert_eq!(high.cpp2_cents, 41_600);
        // 68,900 insurable dollars at 1.63% is 1,123.07.
        assert_eq!(high.ei_cents, 112_307);
        assert_eq!(high.pensionable_cents, constants::CPP_YMPE_CENTS);

        let low = annual(4_000_000, &options()).unwrap();
        assert_eq!(low.cpp_cents, round_bp(4_000_000 - 350_000, 595));
        assert_eq!(low.cpp2_cents, 0, "no CPP2 below the YMPE");
        assert_eq!(low.ei_cents, round_bp(4_000_000, 163));
    }

    #[test]
    fn net_pay_is_gross_less_every_deduction() {
        let result = annual(8_500_000, &options()).unwrap();
        assert!(result.federal_tax_cents > 0 && result.provincial_tax_cents > 0);
        let deducted = result.cpp_cents + result.cpp2_cents + result.ei_cents + result.federal_tax_cents + result.provincial_tax_cents;
        assert_eq!(result.net_cents, result.gross_cents - deducted);
        assert!(result.net_cents < result.gross_cents);
    }

    #[test]
    fn bracket_tax_is_progressive() {
        // 100,000 taxable: 58,523 at 14%, then 41,477 at 20.5%.
        let expected = round_bp(5_852_300, 1_400) + round_bp(10_000_000 - 5_852_300, 2_050);
        assert_eq!(bracket_tax(10_000_000, constants::FEDERAL_BRACKETS, constants::FEDERAL_TOP_BP), expected);
        // Alberta: 61,200 at 8%, then the rest at 10%.
        let ab = round_bp(6_120_000, 800) + round_bp(10_000_000 - 6_120_000, 1_000);
        assert_eq!(bracket_tax(10_000_000, constants::ALBERTA_BRACKETS, constants::ALBERTA_TOP_BP), ab);
    }

    #[test]
    fn the_federal_bpa_phases_out_between_the_two_thresholds() {
        assert_eq!(federal_bpa_cents(10_000_000), constants::FEDERAL_BPA_MAX_CENTS);
        assert_eq!(federal_bpa_cents(constants::BPA_PHASE_START_CENTS), constants::FEDERAL_BPA_MAX_CENTS);
        assert_eq!(federal_bpa_cents(constants::BPA_PHASE_END_CENTS), constants::FEDERAL_BPA_MIN_CENTS);
        let middle = federal_bpa_cents((constants::BPA_PHASE_START_CENTS + constants::BPA_PHASE_END_CENTS) / 2);
        assert!(middle < constants::FEDERAL_BPA_MAX_CENTS && middle > constants::FEDERAL_BPA_MIN_CENTS);
    }

    #[test]
    fn pay_periods_add_back_to_the_annual_totals() {
        let result = annual(9_100_000, &options()).unwrap();
        for periods in [12_u32, 24, 26, 52] {
            let rows = per_period(&result, periods).unwrap();
            assert_eq!(rows.len() as u32, periods);
            assert_eq!(rows.iter().map(|row| row.gross_cents).sum::<i64>(), result.gross_cents);
            assert_eq!(rows.iter().map(|row| row.cpp_cents).sum::<i64>(), result.cpp_cents);
            assert_eq!(rows.iter().map(|row| row.federal_tax_cents).sum::<i64>(), result.federal_tax_cents);
            assert_eq!(rows.iter().map(|row| row.net_cents).sum::<i64>(), result.net_cents);
        }
    }

    #[test]
    fn t4_boxes_carry_the_annual_totals() {
        let result = annual(9_000_000, &options()).unwrap();
        let slip = t4(&result);
        assert_eq!(slip.box14_employment_income_cents, 9_000_000);
        assert_eq!(slip.box16_cpp_cents, result.cpp_cents);
        assert_eq!(slip.box16a_cpp2_cents, result.cpp2_cents);
        assert_eq!(slip.box18_ei_cents, result.ei_cents);
        assert_eq!(slip.box24_insurable_cents, 6_890_000, "insurable earnings are capped");
        assert_eq!(slip.box26_pensionable_cents, 7_460_000, "pensionable earnings are capped");
    }

    #[test]
    fn unsupported_provinces_and_negative_amounts_are_blocked() {
        let mut quebec = options();
        quebec.province = Province::Alberta;
        assert!(annual(-1, &quebec).is_err());
        assert_eq!(Province::parse("QC"), None);
        assert_eq!(Province::parse("QC").is_none(), true);
        assert_eq!(Province::parse("AB"), Some(Province::Alberta));
        assert_eq!(Province::parse(" alberta "), Some(Province::Alberta));
        assert!(per_period(&annual(1_000_000, &options()).unwrap(), 0).is_err());
        assert!(per_period(&annual(1_000_000, &options()).unwrap(), 53).is_err());
    }
}
