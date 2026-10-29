//! Information-slip data entry (T4, T4A, T5) and aggregation into the T1
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
];
/// Boxes recognised for entry and totalling but not applied by the 2025 engine.
pub const RECORDED: &[(&str, &str, &str)] = &[
    ("T4", "16", "CPP contributions (Schedule 8 base/enhanced split must be verified)"),
    ("T4", "18", "EI premiums (line 31200 must be verified)"),
    ("T4", "20", "RPP contributions (line 20700)"),
    ("T4", "44", "Union dues (line 21200)"),
    ("T4A", "016", "Pension or superannuation (line 11500)"),
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
        if !["T4", "T4A", "T5"].contains(&s.kind.as_str()) { return Err(format!("unsupported slip type {}", s.kind)); }
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
                    flags.push(json!({"box": format!("{} {}", r.0, r.1), "blocks_review": !(r.1 == "16" || r.1 == "18") || r.0 != "T4",
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
    #[test] fn unsupported_boxes_are_flagged_or_rejected() {
        let out = aggregate(&[slip("T5", &[("25", 13_800)])]).unwrap();
        assert_eq!(out["flags"][0]["blocks_review"], true);
        assert!(aggregate(&[slip("T4", &[("99", 1)])]).is_err());
        assert!(aggregate(&[slip("T4", &[("14", -1)])]).is_err());
        assert!(aggregate(&[slip("T3", &[])]).is_err());
    }
}
