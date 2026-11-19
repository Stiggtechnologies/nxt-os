//! Straight-line schedules for fixed assets and prepaid expenses.
//!
//! A schedule is a record, not a posting: NXT computes what has been amortised as of a date
//! and what remains. Nothing is posted to the ledger and nothing is written back.

use serde::{Deserialize, Serialize};

/// One month of a schedule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Month {
    pub index: u32,
    pub amount_cents: i64,
    pub accumulated_cents: i64,
    pub remaining_cents: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    MonthsOutOfRange,
    NegativeTotal,
    BadStart,
}

impl ScheduleError {
    pub fn message(&self) -> &'static str {
        match self {
            ScheduleError::MonthsOutOfRange => "A schedule needs between 1 and 600 months",
            ScheduleError::NegativeTotal => "A schedule total must be positive",
            ScheduleError::BadStart => "A schedule needs a valid start date (YYYY-MM-DD)",
        }
    }
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let part = |start: usize, end: usize| value[start..end].parse::<u32>().ok();
    match (part(0, 4), part(5, 7), part(8, 10)) {
        (Some(year), Some(month), Some(day)) => (1900..=2200).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day),
        _ => false,
    }
}

/// Whole months from the start date to the as-of date; 0 when the start is later.
pub fn months_elapsed(start: &str, as_of: &str) -> u32 {
    let parse = |value: &str| -> (u32, u32) {
        let year = value[0..4].parse::<u32>().unwrap_or(0);
        let month = value[5..7].parse::<u32>().unwrap_or(0);
        (year, month)
    };
    let (start_year, start_month) = parse(start);
    let (as_year, as_month) = parse(as_of);
    if start_year == 0 || as_year == 0 {
        return 0;
    }
    let months = (as_year as i64 * 12 + as_month as i64) - (start_year as i64 * 12 + start_month as i64);
    months.max(0) as u32
}

/// Splits a total across months, holding the rounding remainder in the final month so the
/// schedule always adds back to the total.
pub fn monthly_amounts(total_cents: i64, months: u32) -> Vec<i64> {
    if months == 0 {
        return Vec::new();
    }
    let each = total_cents / months as i64;
    let mut amounts = vec![each; months as usize];
    let remainder = total_cents - each * months as i64;
    if let Some(last) = amounts.last_mut() {
        *last += remainder;
    }
    amounts
}

/// The schedule as of a date, stopping at the end of the term.
pub fn schedule(total_cents: i64, months: u32, start: &str, as_of: &str) -> Result<Vec<Month>, ScheduleError> {
    if total_cents < 0 {
        return Err(ScheduleError::NegativeTotal);
    }
    if !(1..=600).contains(&months) {
        return Err(ScheduleError::MonthsOutOfRange);
    }
    if !valid_date(start) || !valid_date(as_of) {
        return Err(ScheduleError::BadStart);
    }
    let amounts = monthly_amounts(total_cents, months);
    let elapsed = months_elapsed(start, as_of).min(months);
    let mut accumulated = 0_i64;
    let mut rows = Vec::new();
    for (index, amount) in amounts.iter().enumerate().take(elapsed as usize) {
        accumulated += amount;
        rows.push(Month { index: index as u32 + 1, amount_cents: *amount, accumulated_cents: accumulated, remaining_cents: total_cents - accumulated });
    }
    Ok(rows)
}

/// Totals an asset or prepaid schedule needs for display.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub total_cents: i64,
    pub months: u32,
    pub monthly_cents: i64,
    pub amortised_cents: i64,
    pub remaining_cents: i64,
    pub months_posted: u32,
}

pub fn summary(total_cents: i64, months: u32, start: &str, as_of: &str) -> Result<Summary, ScheduleError> {
    let rows = schedule(total_cents, months, start, as_of)?;
    let amortised = rows.iter().map(|row| row.amount_cents).sum();
    Ok(Summary {
        total_cents,
        months,
        monthly_cents: monthly_amounts(total_cents, months).first().copied().unwrap_or(0),
        amortised_cents: amortised,
        remaining_cents: total_cents - amortised,
        months_posted: rows.len() as u32,
    })
}

/// A stored schedule record. The kind distinguishes a fixed asset from a prepaid expense.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredSchedule {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub total_cents: i64,
    pub months: u32,
    pub start_date: String,
    pub account_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleInput {
    pub request_id: String,
    pub name: String,
    pub kind: String,
    pub total_cents: i64,
    pub months: u32,
    pub start_date: String,
    pub account_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_division_splits_evenly() {
        assert_eq!(monthly_amounts(1_200_000, 12), vec![100_000; 12]);
        assert_eq!(monthly_amounts(1_200_000, 12).iter().sum::<i64>(), 1_200_000);
    }

    #[test]
    fn a_remainder_lands_in_the_final_month() {
        // 10,000.00 over 12 months: 833.33 with 0.04 left over.
        let amounts = monthly_amounts(1_000_000, 12);
        assert_eq!(amounts[0], 83_333);
        assert_eq!(amounts[11], 83_337);
        assert_eq!(amounts.iter().sum::<i64>(), 1_000_000);
    }

    #[test]
    fn the_schedule_stops_at_the_as_of_date_and_never_overruns() {
        // January to June is five month boundaries; the sixth month starts in July.
        let rows = schedule(1_200_000, 12, "2026-01-15", "2026-06-30").unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4].accumulated_cents, 500_000);
        assert_eq!(rows[4].remaining_cents, 700_000);
        let after = schedule(1_200_000, 12, "2026-01-15", "2030-01-01").unwrap();
        assert_eq!(after.len(), 12, "the term caps the schedule");
    }

    #[test]
    fn nothing_is_amortised_before_the_start_date() {
        assert_eq!(schedule(1_200_000, 12, "2026-06-01", "2026-01-01").unwrap().len(), 0);
        assert_eq!(months_elapsed("2026-06-01", "2026-01-01"), 0);
    }

    #[test]
    fn summaries_add_up() {
        let s = summary(1_000_000, 12, "2026-01-01", "2026-06-30").unwrap();
        assert_eq!(s.months_posted, 5);
        assert_eq!(s.amortised_cents + s.remaining_cents, s.total_cents);
        assert_eq!(s.monthly_cents, 83_333);
    }

    #[test]
    fn bad_inputs_are_refused() {
        assert_eq!(schedule(100, 0, "2026-01-01", "2026-02-01").unwrap_err(), ScheduleError::MonthsOutOfRange);
        assert_eq!(schedule(-1, 12, "2026-01-01", "2026-02-01").unwrap_err(), ScheduleError::NegativeTotal);
        assert_eq!(schedule(100, 12, "2026-13-01", "2026-02-01").unwrap_err(), ScheduleError::BadStart);
        assert_eq!(schedule(100, 12, "not-a-date", "2026-02-01").unwrap_err(), ScheduleError::BadStart);
    }
}
