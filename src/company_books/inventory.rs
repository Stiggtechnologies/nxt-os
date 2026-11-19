//! Item stock movements.
//!
//! Quantities are stored in thousandths of a unit (milli-units) so nothing depends on
//! floating point. On-hand is the sum of signed movements. Valuation uses the unit cost on
//! the most recent movement; this is not weighted average or FIFO and says so.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Movement {
    pub id: String,
    pub item_id: String,
    /// receipt, issue or adjustment.
    pub kind: String,
    /// Positive for receipt and issue, signed for adjustment.
    pub quantity_milli: i64,
    pub unit_cost_cents: i64,
    pub date: String,
    pub memo: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementInput {
    pub request_id: String,
    pub item_id: String,
    pub kind: String,
    pub quantity_milli: i64,
    #[serde(default)] pub unit_cost_cents: i64,
    pub date: String,
    #[serde(default)] pub memo: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MovementError {
    UnknownKind,
    ZeroQuantity,
    NegativeCost,
    BadDate,
}

impl MovementError {
    pub fn message(&self) -> &'static str {
        match self {
            MovementError::UnknownKind => "A movement is a receipt, an issue or an adjustment",
            MovementError::ZeroQuantity => "A movement needs a non-zero quantity",
            MovementError::NegativeCost => "A unit cost cannot be negative",
            MovementError::BadDate => "A movement needs a valid date (YYYY-MM-DD)",
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

/// Signed quantity a movement contributes. An adjustment carries its own sign.
pub fn signed_quantity(kind: &str, quantity_milli: i64) -> Result<i64, MovementError> {
    match kind {
        "receipt" => Ok(quantity_milli.abs()),
        "issue" => Ok(-quantity_milli.abs()),
        "adjustment" => Ok(quantity_milli),
        _ => Err(MovementError::UnknownKind),
    }
}

/// Checks a movement before it is stored.
pub fn validate(input: &MovementInput) -> Result<i64, MovementError> {
    let signed = signed_quantity(&input.kind, input.quantity_milli)?;
    if signed == 0 {
        return Err(MovementError::ZeroQuantity);
    }
    if input.unit_cost_cents < 0 {
        return Err(MovementError::NegativeCost);
    }
    if !valid_date(&input.date) {
        return Err(MovementError::BadDate);
    }
    Ok(signed)
}

/// On-hand quantity per item, and the cost of the latest movement per item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ItemStock {
    pub item_id: String,
    pub on_hand_milli: i64,
    pub unit_cost_cents: i64,
    pub value_cents: i64,
    pub movements: usize,
}

/// Display value in cents: on-hand quantity times the latest unit cost.
pub fn value_cents(on_hand_milli: i64, unit_cost_cents: i64) -> i64 {
    let raw = (on_hand_milli as i128) * (unit_cost_cents as i128);
    // milli-units times cents, back to cents, rounded half-up.
    let rounded = if raw >= 0 { (raw + 500) / 1000 } else { (raw - 500) / 1000 };
    rounded as i64
}

/// Rolls movements up per item, ordered by movement date then id so the latest cost wins.
pub fn stock(movements: &[Movement]) -> Vec<ItemStock> {
    let mut ordered: Vec<&Movement> = movements.iter().collect();
    ordered.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));
    let mut totals: Vec<ItemStock> = Vec::new();
    for movement in ordered {
        let signed = signed_quantity(&movement.kind, movement.quantity_milli).unwrap_or(0);
        match totals.iter_mut().find(|row| row.item_id == movement.item_id) {
            Some(row) => {
                row.on_hand_milli += signed;
                row.movements += 1;
                if movement.unit_cost_cents > 0 {
                    row.unit_cost_cents = movement.unit_cost_cents;
                }
            }
            None => totals.push(ItemStock {
                item_id: movement.item_id.clone(),
                on_hand_milli: signed,
                unit_cost_cents: movement.unit_cost_cents,
                value_cents: 0,
                movements: 1,
            }),
        }
    }
    for row in totals.iter_mut() {
        row.value_cents = value_cents(row.on_hand_milli, row.unit_cost_cents);
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movement(id: &str, item: &str, kind: &str, quantity: i64, cost: i64, date: &str) -> Movement {
        Movement { id: id.into(), item_id: item.into(), kind: kind.into(), quantity_milli: quantity, unit_cost_cents: cost, date: date.into(), memo: String::new() }
    }

    #[test]
    fn receipts_and_issues_have_opposite_signs() {
        assert_eq!(signed_quantity("receipt", 500).unwrap(), 500);
        assert_eq!(signed_quantity("issue", 500).unwrap(), -500);
        assert_eq!(signed_quantity("adjustment", -250).unwrap(), -250);
        assert_eq!(signed_quantity("receipt", -500).unwrap(), 500, "a receipt is positive regardless of input sign");
        assert_eq!(signed_quantity("donation", 1), Err(MovementError::UnknownKind));
    }

    #[test]
    fn validation_blocks_zero_quantities_bad_dates_and_negative_costs() {
        let base = MovementInput { request_id: "m1".into(), item_id: "i1".into(), kind: "receipt".into(), quantity_milli: 0, unit_cost_cents: 0, date: "2026-01-01".into(), memo: String::new() };
        assert_eq!(validate(&base), Err(MovementError::ZeroQuantity));
        assert_eq!(validate(&MovementInput { quantity_milli: 100, ..base.clone() }).unwrap(), 100);

        let bad_date = MovementInput { quantity_milli: 100, date: "2026-13-01".into(), ..base.clone() };
        assert_eq!(validate(&bad_date), Err(MovementError::BadDate));
        let bad_cost = MovementInput { quantity_milli: 100, unit_cost_cents: -1, ..base };
        assert_eq!(validate(&bad_cost), Err(MovementError::NegativeCost));
    }

    #[test]
    fn on_hand_is_the_sum_of_signed_movements() {
        let rows = stock(&[
            movement("a", "i1", "receipt", 10_000, 500, "2026-01-01"),
            movement("b", "i1", "issue", 4_000, 0, "2026-01-05"),
            movement("c", "i1", "adjustment", -1_000, 0, "2026-01-06"),
        ]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].on_hand_milli, 5_000);
        assert_eq!(rows[0].movements, 3);
    }

    #[test]
    fn value_uses_the_latest_unit_cost() {
        let rows = stock(&[
            movement("a", "i1", "receipt", 10_000, 500, "2026-01-01"),
            movement("b", "i1", "receipt", 10_000, 700, "2026-02-01"),
        ]);
        assert_eq!(rows[0].unit_cost_cents, 700, "the later cost wins");
        // 20 units at 7.00 is 140.00, i.e. 14,000 cents.
        assert_eq!(rows[0].value_cents, 14_000);
    }

    #[test]
    fn zero_unit_cost_yields_zero_value_without_hiding_quantity() {
        let rows = stock(&[movement("a", "i1", "receipt", 3_000, 0, "2026-01-01")]);
        assert_eq!(rows[0].on_hand_milli, 3_000);
        assert_eq!(rows[0].value_cents, 0);
    }

    #[test]
    fn per_item_rollup_is_separate() {
        let rows = stock(&[
            movement("a", "i1", "receipt", 1_000, 100, "2026-01-01"),
            movement("b", "i2", "receipt", 2_000, 200, "2026-01-02"),
        ]);
        assert_eq!(rows.len(), 2);
        let first = rows.iter().find(|row| row.item_id == "i1").unwrap();
        assert_eq!(first.on_hand_milli, 1_000);
        assert_eq!(first.value_cents, 100);
    }
}
