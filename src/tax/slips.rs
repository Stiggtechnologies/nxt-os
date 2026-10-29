//! Information-slip data entry (T4, T4A, T5, T3, T5008, RRSP receipts, T2202) and aggregation into the T1
//! calculation inputs the native engine supports. Amounts are integer cents.
//! Boxes the current engine cannot calculate are totalled and flagged, never
//! silently dropped or folded into another line.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const MAX: i64 = 100_000_000_000_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slip {
    /// T4, T4A or T5.
    pub kind: String,
    /// Payer label (no SIN/BN needed for aggregation).
    #[serde(default)] pub issuer: String,
    /// Box number -> integer cents, e.g. {"14": 5000000, "22": 800000}.
    pub boxes: BTreeMap<String, i64>,
}

/// Boxes the engine maps directly, as (slip, box, calculation input key, CRA line).
pub const MAPPED: &[(&str, &str, &str, &str)] = &[
    ("T4", "14", "employment_income_cents", "10100"),
    ("T4", "22", "tax_withheld_cents", "43700"),
    ("T4A", "22", "tax_withheld_cents", "43700"),
    ("T5", "13", "interest_income_cents", "12100"),
    ("T5008", "21", "capital_proceeds_cents", "S3-proceeds"),
    ("T5008", "20", "capital_acb_cents", "S3-acb"),
    ("T2202", "23", "tuition_cents", "32300"),
    ("T4A", "016", "pension_income_cents", "11500"),
];
/// Recorded boxes that are informational because the claim is entered separately
/// as a verified amount (they do not block review).
pub const NON_BLOCKING: &[(&str, &str)] = &[("T4", "16"), ("T4", "18"), ("RRSP", "contribution"), ("RRSP", "first60"), ("T2202", "24"), ("T2202", "25")];
/// Boxes recognised for entry and totalling but not applied by the 2025 engine.
pub const RECORDED: &[(&str, &str, &str)] = &[
    ("T4", "16", "CPP contributions (Schedule 8 base/enhanced split must be verified)"),
    ("T4", "18", "EI premiums (line 31200 must be verified)"),
    ("T4", "20", "RPP contributions (line 20700)"),
    ("T4", "44", "Union dues (line 21200)"),
    ("T3", "21", "Capital gains (Schedule 3 line 17600; enter via capital proceeds after review)"),
    ("T3", "23", "Actual amount of other-than-eligible dividends"),
    ("T3", "26", "Other income (line 13000)"),
    ("T3", "49", "Actual amount of eligible dividends"),
    ("T3", "50", "Taxable amount of eligible dividends (line 12000)"),
    ("T3", "32", "Taxable amount of other-than-eligible dividends (line 12010)"),
    ("T3", "25", "Foreign non-business income"),
    ("T3", "34", "Foreign non-business tax paid"),
    ("T5008", "15", "Type of security code (record only)"),
    ("T5008", "16", "Quantity (record only)"),
    ("RRSP", "contribution", "RRSP contribution receipt, Mar 3 - Dec 31 2025 (Schedule 7; deduction entered separately against verified limit)"),
    ("RRSP", "first60", "RRSP contribution receipt, Jan 1 - Mar 2 2026 (Schedule 7; deduction entered separately against verified limit)"),
    ("T2202", "24", "Number of months part-time"),
    ("T2202", "25", "Number of months full-time"),
    ("T4A", "018", "Lump-sum payments"),
    ("T4A", "020", "Self-employed commissions"),
    ("T4A", "028", "Other income (line 13000)"),
    ("T4A", "048", "Fees for services"),
    ("T5", "10", "Actual amount of other-than-eligible dividends"),
    ("T5", "11", "Taxable amount of other-than-eligible dividends (line 12010)"),
    ("T5", "18", "Capital gains dividends"),
    ("T5", "24", "Actual amount of eligible dividends"),
    ("T5", "25", "Taxable amount of eligible dividends (line 12000)"),
    ("T5", "15", "Foreign income"),
    ("T5", "16", "Foreign tax paid"),
];

pub fn validate(slips: &[Slip]) -> Result<(), String> {
    if slips.len() > 200 { return Err("at most 200 slips per return".into()); }
    for s in slips {
        if !["T4", "T4A", "T5", "T3", "T5008", "RRSP", "T2202"].contains(&s.kind.as_str()) { return Err(format!("unsupported slip type {}", s.kind)); }
        if s.issuer.len() > 200 || s.issuer.chars().any(char::is_control) { return Err("invalid issuer label".into()); }
        if s.boxes.len() > 60 { return Err("too many boxes on one slip".into()); }
        for (b, v) in &s.boxes {
            let known = MAPPED.iter().any(|m| m.0 == s.kind && m.1 == b) || RECORDED.iter().any(|r| r.0 == s.kind && r.1 == b);
            if !known { return Err(format!("{} box {} is not supported for entry", s.kind, b)); }
            if !(0..=MAX).contains(v) { return Err(format!("{} box {} must be non-negative integer cents", s.kind, b)); }
        }
    }
    Ok(())
}

/// Returns calculation inputs, per-line totals and review flags.
pub fn aggregate(slips: &[Slip]) -> Result<Value, String> {
    validate(slips)?;
    let mut inputs: BTreeMap<&str, i64> = BTreeMap::new();
    let mut lines: BTreeMap<&str, i64> = BTreeMap::new();
    let mut recorded: BTreeMap<String, Value> = BTreeMap::new();
    let mut flags = vec![];
    for s in slips {
        for (b, v) in &s.boxes {
            if let Some(m) = MAPPED.iter().find(|m| m.0 == s.kind && m.1 == b) {
                *inputs.entry(m.2).or_default() = inputs.get(m.2).copied().unwrap_or(0).checked_add(*v).ok_or("slip total overflow")?;
                *lines.entry(m.3).or_default() += v;
            } else if let Some(r) = RECORDED.iter().find(|r| r.0 == s.kind && r.1 == b) {
                let key = format!("{} box {}", r.0, r.1);
                let prior = recorded.get(&key).and_then(|x| x["cents"].as_i64()).unwrap_or(0);
                recorded.insert(key, json!({"cents": prior + v, "meaning": r.2}));
                if *v > 0 && !flags.iter().any(|f: &Value| f["box"] == format!("{} {}", r.0, r.1)) {
                    flags.push(json!({"box": format!("{} {}", r.0, r.1), "blocks_review": !NON_BLOCKING.contains(&(r.0, r.1)),
                        "message": format!("{} is recorded but not calculated by the native engine; verify and enter the claim manually or mark other_tax_situations.", r.2)}));
                }
            }
        }
    }
    let mut obj = Map::new();
    for (k, v) in &inputs { obj.insert((*k).into(), json!(v)); }
    Ok(json!({"inputs": obj, "lines": lines, "recorded_not_calculated": recorded, "flags": flags, "slip_count": slips.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn slip(kind: &str, b: &[(&str, i64)]) -> Slip { Slip { kind: kind.into(), issuer: "Synthetic payer".into(), boxes: b.iter().map(|(k, v)| (k.to_string(), *v)).collect() } }
    #[test] fn sums_multiple_slips_into_engine_inputs() {
        let out = aggregate(&[slip("T4", &[("14", 5_000_000), ("22", 700_000), ("16", 250_000)]), slip("T4", &[("14", 1_000_000), ("22", 100_000)]), slip("T5", &[("13", 12_345)])]).unwrap();
        assert_eq!(out["inputs"]["employment_income_cents"], 6_000_000);
        assert_eq!(out["inputs"]["tax_withheld_cents"], 800_000);
        assert_eq!(out["inputs"]["interest_income_cents"], 12_345);
        assert_eq!(out["lines"]["10100"], 6_000_000);
        assert_eq!(out["flags"][0]["blocks_review"], false);
    }
    #[test] fn t3_t5008_rrsp_t2202_slips() {
        let out = aggregate(&[slip("T5008", &[("21", 1_500_000), ("20", 1_000_000)]), slip("T2202", &[("23", 300_000), ("25", 8)]), slip("RRSP", &[("contribution", 500_000)]), slip("T3", &[("26", 2_000)]), slip("T4A", &[("016", 100)])]).unwrap();
        assert_eq!(out["inputs"]["capital_proceeds_cents"], 1_500_000);
        assert_eq!(out["inputs"]["capital_acb_cents"], 1_000_000);
        assert_eq!(out["inputs"]["tuition_cents"], 300_000);
        assert_eq!(out["inputs"]["pension_income_cents"], 100);
        let flags = out["flags"].as_array().unwrap();
        assert!(flags.iter().any(|f| f["box"] == "T3 26" && f["blocks_review"] == true));
        assert!(flags.iter().any(|f| f["box"] == "RRSP contribution" && f["blocks_review"] == false));
        assert!(flags.iter().any(|f| f["box"] == "T2202 25" && f["blocks_review"] == false));
    }
    #[test] fn unsupported_boxes_are_flagged_or_rejected() {
        let out = aggregate(&[slip("T5", &[("25", 13_800)])]).unwrap();
        assert_eq!(out["flags"][0]["blocks_review"], true);
        assert!(aggregate(&[slip("T4", &[("99", 1)])]).is_err());
        assert!(aggregate(&[slip("T4", &[("14", -1)])]).is_err());
        assert!(aggregate(&[slip("T2125", &[])]).is_err());
    }
}
