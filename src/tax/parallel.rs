//! Compare an NXT workpaper with lines supplied from DT Max or WebTax.
//! The old system stays authoritative. This does not file, transmit, or overwrite either result.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelSource {
    pub source_system: String,
    pub source_artifact_sha256: Option<String>,
    pub program: String,
    pub tax_year: i32,
    pub jurisdiction: String,
    pub lines: Vec<SourceLine>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLine {
    pub code: String,
    pub amount_cents: i64,
}

pub fn compare(nxt: &BTreeMap<String, i64>, source: &ParallelSource) -> Result<Value, String> {
    if !matches!(source.source_system.as_str(), "dtmax" | "webtax") {
        return Err("source_system must be dtmax or webtax".into());
    }
    if source.lines.is_empty() || source.lines.len() > 400 {
        return Err("supply between 1 and 400 source lines".into());
    }
    let mut source_lines = BTreeMap::new();
    for line in &source.lines {
        if line.code.is_empty() || line.code.len() > 40 || !line.code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
            return Err("source line codes must be 1–40 ASCII letters, digits, underscores or dots".into());
        }
        if source_lines.insert(line.code.clone(), line.amount_cents).is_some() {
            return Err(format!("duplicate source line {}", line.code));
        }
    }
    let paired_bytes = match &source.source_artifact_sha256 {
        None => false,
        Some(value) if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) => true,
        Some(_) => return Err("source_artifact_sha256 must be a lowercase hex SHA-256 of the operator-held source file".into()),
    };
    let mut codes: Vec<String> = nxt.keys().chain(source_lines.keys()).cloned().collect();
    codes.sort();
    codes.dedup();
    let mut rows = Vec::new();
    let mut matches = 0_usize;
    let mut differs = 0_usize;
    for code in codes {
        let nxt_amount = nxt.get(&code).copied();
        let source_amount = source_lines.get(&code).copied();
        let status = match (nxt_amount, source_amount) {
            (Some(a), Some(b)) if a == b => "match",
            (Some(_), Some(_)) => "differ",
            (Some(_), None) => "only_nxt",
            (None, Some(_)) => "only_source",
            (None, None) => continue,
        };
        if status == "match" { matches += 1; } else { differs += 1; }
        let delta = nxt_amount.unwrap_or(0) - source_amount.unwrap_or(0);
        rows.push(json!({
            "code": code,
            "nxt_cents": nxt_amount,
            "source_cents": source_amount,
            "delta_cents": delta,
            "status": status,
        }));
    }
    Ok(json!({
        "format": "nxt-tax-parallel-compare/v1",
        "authoritative_system": source.source_system,
        "parallel_only": true,
        "filing_enabled": false,
        "certification": "not_claimed",
        "write_back": false,
        "paired_to_vendor_bytes": paired_bytes,
        "sample_status": if paired_bytes { "operator_sha256_only" } else { "lines_without_vendor_file" },
        "matches": matches,
        "differences": differs,
        "rows": rows,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplied_lines_diff_without_claiming_a_vendor_file_or_a_filing() {
        let mut nxt = BTreeMap::new();
        nxt.insert("26000".into(), 100);
        nxt.insert("42800".into(), 40);
        let source = ParallelSource {
            source_system: "dtmax".into(),
            source_artifact_sha256: None,
            program: "T1".into(),
            tax_year: 2025,
            jurisdiction: "ON".into(),
            lines: vec![
                SourceLine { code: "26000".into(), amount_cents: 100 },
                SourceLine { code: "42800".into(), amount_cents: 41 },
                SourceLine { code: "ON_HEALTH".into(), amount_cents: 9 },
            ],
        };
        let report = compare(&nxt, &source).unwrap();
        assert_eq!(report["authoritative_system"], "dtmax");
        assert_eq!(report["filing_enabled"], false);
        assert_eq!(report["certification"], "not_claimed");
        assert_eq!(report["write_back"], false);
        assert_eq!(report["paired_to_vendor_bytes"], false);
        assert_eq!(report["matches"], 1);
        assert_eq!(report["differences"], 2);
        let rejected = ParallelSource { source_system: "quickbooks".into(), source_artifact_sha256: None, program: "T1".into(), tax_year: 2025, jurisdiction: "ON".into(), lines: vec![SourceLine { code: "26000".into(), amount_cents: 1 }] };
        assert!(compare(&nxt, &rejected).is_err());
    }
}
