//! Versioned availability of bounded calculation paths and verified browser-entry schemas.
//! A calculation path is not complete scenario coverage, certification or filing readiness.
use serde_json::{json, Value};
use super::calculations::{self, supports_scope};

pub const SCHEMA: &str = "nxt-tax-coverage/v1";
pub const VERSION: &str = "2026-11-09.1";
pub const YEARS: &[i32] = &[2025, 2026];
pub const PROGRAMS: &[&str] = &["T1", "T2", "T3", "T5013"];
pub const JURISDICTIONS: &[&str] = &["AB", "BC", "MB", "NB", "NL", "NS", "NT", "NU", "ON", "PE", "QC", "SK", "YT", "CA", "CA-FED"];

/// Only the existing 2025 baseline templates have an accepted browser field/default contract.
/// Never attach a different year's defaults merely because an engine branch exists.
pub fn entry_schema_ready(program: &str, year: i32, jurisdiction: &str) -> bool {
    year == 2025 && supports_scope(program, year, jurisdiction)
}
pub fn entry_schema_id(program: &str) -> String { format!("nxt-tax-entry/{program}/2025/v1") }
pub fn entry_years(program: &str) -> Vec<i32> {
    YEARS.iter().copied().filter(|y| JURISDICTIONS.iter().any(|j| entry_schema_ready(program, *y, j))).collect()
}
pub fn entry_jurisdictions(program: &str, year: i32) -> Vec<&'static str> {
    JURISDICTIONS.iter().copied().filter(|j| entry_schema_ready(program, year, j)).collect()
}
pub fn engine_years() -> Vec<i32> {
    YEARS.iter().copied().filter(|y| PROGRAMS.iter().any(|p| JURISDICTIONS.iter().any(|j| supports_scope(p, *y, j)))).collect()
}
/// Qualify rate metadata at the API boundary without changing any rate table or formula.
pub fn rate_metadata() -> Value {
    let mut value=super::rates::catalog();
    value["calculation_years"]=json!(engine_years());
    value["browser_entry_years"]=json!([2025]);
    value["coverage_schema"]=json!(SCHEMA);value["coverage_version"]=json!(VERSION);
    value["year_availability_statement"]=json!("Calculation years identify limited engine paths; use the versioned coverage catalog for program/jurisdiction and browser-entry limits.");
    value
}
fn entry_scope(program: &str, jurisdiction: &str) -> &'static str {
    match (program, jurisdiction) {
        ("T1", "AB") => calculations::T1_SCOPE,
        ("T1", "ON") => calculations::T1_ON_SCOPE,
        ("T1", _) => calculations::T1_PROV_SCOPE,
        ("T2", _) => calculations::T2_SCOPE,
        ("T3", _) => calculations::T3_SCOPE,
        ("T5013", _) => calculations::T5013_SCOPE,
        _ => "No browser entry schema.",
    }
}

pub fn catalog() -> Value {
    let mut scopes=Vec::new();
    for &program in PROGRAMS { for &year in YEARS { for &jurisdiction in JURISDICTIONS {
        let available=supports_scope(program, year, jurisdiction);
        let ready=entry_schema_ready(program, year, jurisdiction);
        scopes.push(json!({
            "program":program,"tax_year":year,"jurisdiction":jurisdiction,
            "engine_scope_available":available,"scenario_validation_required":true,
            "browser_entry_available":ready,
            "entry_state":if ready {"verified_baseline_schema"} else if available {"year_specific_schema_pending"} else {"unsupported_scope"},
            "entry_schema_id":ready.then(||entry_schema_id(program)),
            "entry_schema_year":ready.then_some(2025),
            "entry_scope":ready.then(||entry_scope(program,jurisdiction)),
            "filing_supported":false,"complete_return_supported":false,
        }));
    } } }
    json!({
        "schema":SCHEMA,"version":VERSION,"years":YEARS,"programs":PROGRAMS,"jurisdictions":JURISDICTIONS,
        "engine_years":engine_years(),"browser_entry_years":[2025],"scopes":scopes,
        "statement":"Availability identifies a bounded calculation path, not support for every tax situation. Each saved calculation must pass its own scope confirmations and diagnostics. No complete return, certification or filing is claimed.",
        "browser_entry_statement":"New-return templates and the T1 interview use verified 2025 entry forms. Limited 2026 engine paths are listed read-only until their entry forms are verified. Profile-only Rollforward and T2 intake may create other-year draft containers; those drafts do not enable an editor or calculation scope and do not reuse 2025 defaults.",
        "profile_only_draft_paths":["rollforward","t2_intake"],"draft_container_is_not_entry_or_calculation_readiness":true,
        "unlisted_scope_available":false,"source_years_are_not_calculation_coverage":true,"source_values_auto_mapped":false,
        "certified":false,"filing_supported":false,"full_DTMax_parity":false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_rows_match_the_actual_scope_guard_and_year_specific_entry_boundary() {
        let c=catalog();let rows=c["scopes"].as_array().unwrap();assert_eq!(rows.len(),120);
        let mut available=0;let mut ready=0;
        for r in rows {
            let p=r["program"].as_str().unwrap();let y=r["tax_year"].as_i64().unwrap() as i32;let j=r["jurisdiction"].as_str().unwrap();
            let a=supports_scope(p,y,j);assert_eq!(r["engine_scope_available"],a);assert_eq!(r["browser_entry_available"],entry_schema_ready(p,y,j));
            available+=usize::from(a);ready+=usize::from(r["browser_entry_available"]==true);
            if y==2026 || !a { assert!(r["entry_schema_id"].is_null());assert!(r["entry_schema_year"].is_null());assert!(r["entry_scope"].is_null()); }
            assert_eq!(r["filing_supported"],false);assert_eq!(r["complete_return_supported"],false);
        }
        assert_eq!((available,ready),(31,15));
        for y in 2000..=2100 { for &p in PROGRAMS { for &j in JURISDICTIONS { if !YEARS.contains(&y) { assert!(!supports_scope(p,y,j)); } } } }
        assert!(!entry_schema_ready("T1",2025,"QC"));assert!(!entry_schema_ready("T1",2026,"AB"));assert!(supports_scope("T1",2026,"QC"));
    }
}
