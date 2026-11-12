//! 2025 Schedule 8, Parts 3 and 5, for full-year contributors using Canadian T4s.

use super::{
    schedule8::{BASIC_EXEMPTION, FIRST_CEILING, SECOND_CEILING},
    slips::Slip,
};

pub(super) const CONFIRMATION: &str = "schedule8_payroll_verified";
pub(super) const SOURCE_BOXES: &[&str] = &["T4 16", "T4 16A", "T4 26"];

const FIRST_BAND: i64 = FIRST_CEILING - BASIC_EXEMPTION;
const SECOND_BAND: i64 = SECOND_CEILING - FIRST_CEILING;

#[derive(Default)]
pub(super) struct Payroll {
    gross: i64,
    pensionable: i64,
    first: i64,
    second: i64,
}

impl Payroll {
    pub(super) fn from_slips(slips: &[Slip], employment: i64) -> Result<Self, String> {
        let mut payroll = Self::default();
        let mut found = false;
        for slip in slips.iter().filter(|slip| slip.kind == "T4") {
            found = true;
            let amount = |key| slip.boxes.get(key).copied().unwrap_or(0);
            if amount("17") != 0 || amount("17A") != 0 {
                return Err("QPP on a saved T4 requires Form RC381.".into());
            }
            let pensionable = slip.boxes.get("26").copied().unwrap_or_else(|| amount("14"));
            let first = amount("16");
            let second = amount("16A");
            if pensionable < 0 || !(0..=403_410).contains(&first) || !(0..=39_600).contains(&second)
                || pensionable == 0 && (first != 0 || second != 0)
            {
                return Err("Check CPP pensionable earnings and contribution amounts on the saved T4.".into());
            }
            payroll.pensionable += pensionable.min(SECOND_CEILING);
            payroll.gross += amount("14");
            payroll.first += first;
            payroll.second += second;
        }
        if !found {
            return Err("Save the T4 occurrences before calculating payroll CPP.".into());
        }
        if payroll.gross != employment {
            return Err("The saved T4 employment totals must match the workpaper before calculating payroll CPP.".into());
        }
        Ok(payroll)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Transfers {
    pub employment_credit: i64,
    pub employment_deduction: i64,
    pub overpayment: i64,
    pub business_credit: i64,
    pub business_deduction: i64,
    pub payable: i64,
}

// Each displayed form line rounds to cents, including the published five-decimal multiplier.
fn ratio(amount: i64, numerator: i64, denominator: i64) -> i64 {
    (amount * numerator + denominator / 2) / denominator
}

struct Employment {
    pensionable: i64,
    base: i64,
    first: i64,
    second: i64,
    base_delta: i64,
    first_delta: i64,
    second_delta: i64,
}

impl Employment {
    fn new(payroll: &Payroll) -> Self {
        let pensionable = payroll.pensionable.min(SECOND_CEILING);
        let second_band = (pensionable - FIRST_CEILING).max(0);
        let first_band = (pensionable - second_band - BASIC_EXEMPTION).max(0);
        let base = ratio(first_band, 495, 10_000);
        let first = ratio(first_band, 1, 100);
        let second = ratio(second_band, 4, 100);
        let actual_base = ratio(payroll.first, 831_933, 1_000_000);
        Self {
            pensionable, base, first, second,
            base_delta: actual_base - base,
            first_delta: payroll.first - actual_base - first,
            second_delta: payroll.second - second,
        }
    }

    fn first_delta(&self) -> i64 {
        self.base_delta + self.first_delta
    }

    fn delta(&self) -> i64 {
        self.first_delta() + self.second_delta
    }

    // Part 3b lines 30–48 and Part 5 lines 51–69 have the same allocation.
    fn credits(&self) -> (i64, i64) {
        let base_short = (-self.base_delta).max(0);
        let first_short = (-self.first_delta).max(0);
        let base_from_first = base_short.min(self.first_delta.max(0));
        let first_from_base = first_short.min(self.base_delta.max(0));
        let base_from_second = (base_short - base_from_first).min(self.second_delta.max(0));
        let first_from_second = (first_short - first_from_base)
            .min((self.second_delta - base_from_second).max(0));
        let second_from_first_band = (-self.second_delta).max(0).min(self.first_delta().max(0));
        let base = self.base - base_short + base_from_first + base_from_second;
        let enhanced = self.first - first_short + first_from_base + first_from_second
            + self.second + self.second_delta.min(0) + second_from_first_band;
        (base, enhanced)
    }
}

pub(super) fn full_year(payroll: &Payroll, net_business: i64) -> Transfers {
    let employment = Employment::new(payroll);
    let first_delta = employment.first_delta();
    let second_delta = employment.second_delta;

    // Part 5 lines 5–22: contributions applied to each earnings band.
    let first_paid = payroll.first - first_delta.max(0)
        + (-first_delta).max(0).min(second_delta.max(0));
    let second_paid = payroll.second - second_delta.max(0)
        + (-second_delta).max(0).min(first_delta.max(0));
    let first_remaining = (FIRST_BAND - ratio(first_paid, 1_680_672, 100_000)).max(0);
    let exemption_left = (BASIC_EXEMPTION - employment.pensionable).max(0);
    let business_above_first_band = (net_business - FIRST_BAND).max(0);
    let first_exemption = (exemption_left - business_above_first_band).max(0);
    let first_earnings = (net_business.min(first_remaining) - first_exemption).max(0);
    let second_earnings = if net_business + employment.pensionable > FIRST_CEILING {
        let remaining = SECOND_BAND - ratio(second_paid, 100, 4);
        remaining.min(net_business - exemption_left - first_earnings).max(0)
    } else {
        0
    };

    let (employment_credit, employment_deduction) = employment.credits();
    if first_earnings == 0 && second_earnings == 0 {
        let (employment_credit, employment_deduction) = if employment.delta() > 0 {
            (employment.base, employment.first + employment.second)
        } else {
            (employment_credit, employment_deduction)
        };
        return Transfers {
            employment_credit, employment_deduction,
            overpayment: employment.delta().max(0),
            business_credit: 0, business_deduction: 0, payable: 0,
        };
    }

    // Part 5 lines 43–50: apply any payroll excess against business CPP first.
    let base = ratio(first_earnings, 990, 10_000);
    let first = ratio(first_earnings, 2, 100);
    let second = ratio(second_earnings, 8, 100);
    let excess = employment.delta().max(0);
    let payable = base + first + second - 2 * excess;

    // Part 5 lines 70–88: apportion that offset between business deductions.
    let base_half = ratio(base, 1, 2);
    let first_half = ratio(first, 1, 2);
    let second_half = ratio(second, 1, 2);
    let offset = (base_half + first_half).min(excess);
    let base_offset = ratio(offset, 831_933, 1_000_000);
    let first_offset = offset - base_offset;
    let second_remaining = (second_half - (excess - base_half - first_half).max(0)).max(0);
    let deduction = base_half - base_offset + first_half + (first_half - first_offset)
        + second_half + second_remaining;
    Transfers {
        employment_credit, employment_deduction,
        overpayment: ratio((-payable).max(0), 1, 2),
        business_credit: base_half, business_deduction: deduction, payable: payable.max(0),
    }
}
