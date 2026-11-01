use nxt_os::engagement::{models::EngagementInput, reports, statements};
use serde_json::json;

pub fn fixture() -> EngagementInput {
    let rows = [
        ("cash", "1001", "asset", 50_000, 0, 30_000, 0),
        ("ar", "1060", "asset", 20_000, 0, 10_000, 0),
        ("equip", "1740", "asset", 100_000, 0, 80_000, 0),
        ("ap", "2620", "liability", 0, 15_000, 0, 10_000),
        ("ltd", "3140", "liability", 0, 40_000, 0, 30_000),
        ("shares", "3500", "equity", 0, 10_000, 0, 10_000),
        ("re", "3600", "equity", 0, 70_000, 0, 50_000),
        ("div", "3700", "equity", 5_000, 0, 0, 0),
        ("rev", "8000", "revenue", 0, 180_000, 0, 120_000),
        ("cogs", "8300", "expense", 80_000, 0, 60_000, 0),
        ("opex", "8520", "expense", 60_000, 0, 40_000, 0),
    ];
    let accounts: Vec<_> = rows.iter().map(|(id, code, kind, d, c, pd, pc)| json!({"id":id,"code":code,"name":id,"kind":kind,"group_id":format!("g{id}"),"debit_cents":d,"credit_cents":c,"prior_debit_cents":pd,"prior_credit_cents":pc})).collect();
    let groups: Vec<_> = rows.iter().map(|(id, code, kind, ..)| json!({"id":format!("g{id}"),"code":code,"name":id,"kind":kind,"gifi_code":code})).collect();
    serde_json::from_value(json!({
        "name":"Synthetic Co","kind":"compilation","period_start":"2025-01-01","period_end":"2025-12-31",
        "prior_period_start":"2024-01-01","prior_period_end":"2024-12-31",
        "accounts":accounts,"groups":groups,
        "adjustments":[{"id":"p1","reference":"PJE1","date":"2025-12-31","kind":"proposed_factual","description":"Unrecorded accrual","lines":[{"account_id":"opex","debit_cents":1000},{"account_id":"ap","credit_cents":1000}]}],
    })).unwrap()
}

#[test]
fn draft_aspe_statements_tie_out() {
    let s = statements::generate(&fixture());
    let cur = &s["current"];
    assert_eq!(cur["income_statement"]["net_income_cents"], 40_000);
    assert_eq!(cur["income_statement"]["gross_profit_cents"], 100_000);
    assert_eq!(cur["retained_earnings"]["closing_cents"], 105_000);
    assert_eq!(cur["balance_sheet"]["total_assets_cents"], 170_000);
    assert_eq!(cur["balance_sheet"]["difference_cents"], 0);
    assert_eq!(s["prior"]["retained_earnings"]["closing_cents"], 70_000);
    let cf = &s["cash_flow"];
    assert_eq!(cf["operating_cents"], 35_000);
    assert_eq!(cf["investing_cents"], -20_000);
    assert_eq!(cf["financing_cents"], 5_000);
    assert_eq!(cf["net_change_in_cash_cents"], 20_000);
    assert!(!s["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "cash_flow_unreconciled"));
    assert!(s["review_notice"].as_str().unwrap().contains("CPA review"));
    assert_eq!(s["notes"].as_array().unwrap().len(), 2, "compilation notes limited to basis of accounting");
}

#[test]
fn proposed_entries_stay_out_of_adjusted_basis() {
    let s = statements::generate(&fixture());
    assert_eq!(s["current"]["income_statement"]["operating_expenses_cents"], 60_000);
    assert_eq!(s["uncorrected_misstatements"]["total_net_income_effect_cents"], -1000);
    let opex = s["leadsheets"].as_array().unwrap().iter().find(|l| l["group_id"] == "gopex").unwrap();
    assert_eq!(opex["adjustments_cents"], 0);
    assert_eq!(opex["change_cents"], 20_000);
}

#[test]
fn unbalanced_proposed_entry_still_blocks() {
    let mut input = fixture();
    input.adjustments[0].lines[1].credit_cents = 999;
    let s = statements::generate(&input);
    assert!(s["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "unbalanced_adjustment"));
}

#[test]
fn unknown_entry_kind_rejected() {
    let mut input = fixture();
    input.adjustments[0].kind = "bogus".into();
    assert_eq!(statements::generate(&input)["supported"], false);
}

#[test]
fn templates_are_flagged_for_cpa_review() {
    let input = fixture();
    let report = reports::template(&input, "compilation_report").unwrap();
    assert!(report[0].contains("CPA REVIEW REQUIRED"));
    assert!(report.iter().any(|l| l.contains("Synthetic Co") && l.contains("2025-12-31")));
    let mut review = fixture();
    review.kind = "review".into();
    assert!(reports::template(&review, "compilation_report").is_err());
    assert!(reports::template(&review, "engagement_letter").unwrap().iter().any(|l| l.contains("CSRE 2400")));
    assert!(reports::template(&input, "nope").is_err());
}

#[test]
fn pdf_export_is_well_formed() {
    let bytes = reports::pdf(&reports::statement_lines(&fixture()));
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.starts_with("%PDF-1.4"));
    assert!(text.trim_end().ends_with("%%EOF"));
    assert!(text.contains("NET INCOME"));
    let xref: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
    assert_eq!(&bytes[xref..xref + 4], b"xref");
}

#[test]
fn esign_handoff_matches_signatures_draft_and_is_not_submitted() {
    let v = reports::esign_handoff(&fixture(), "eng-1").unwrap();
    assert_eq!(v["submitted"], false);
    let draft: nxt_os::signatures::models::Draft = serde_json::from_value(v["draft"].clone()).unwrap();
    assert_eq!(draft.documents.len(), 1);
    assert!(draft.recipients.is_empty());
}

#[test]
fn gifi_csv_matches_tax_import_columns() {
    let csv = reports::gifi_csv(&fixture());
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("code,current,prior,description"));
    assert!(csv.contains("1001,\"500.00\",\"300.00\""));
    assert!(csv.contains("8000,\"1,800.00\",\"1,200.00\""));
}

#[test]
fn gifi_csv_includes_totals_for_tax_import_checks() {
    let csv = reports::gifi_csv(&fixture());
    assert!(csv.contains("2599,\"1,700.00\""));
    assert!(csv.contains("8299,\"1,800.00\""));
    assert!(csv.contains("9368,\"1,400.00\""));
}

#[test]
fn amortization_is_added_back_and_tax_entries_stay_off_book() {
    let mut input = fixture();
    input.groups.iter_mut().find(|g| g.id == "gopex").unwrap().gifi_code = Some("8670".into());
    input.adjustments.push(serde_json::from_value(json!({"id":"t1","reference":"TJE1","date":"2025-12-31","kind":"tax","description":"CCA","lines":[{"account_id":"opex","debit_cents":500},{"account_id":"equip","credit_cents":500}]})).unwrap());
    let s = statements::generate(&input);
    assert!(!s["diagnostics"].to_string().contains("cash_flow_unreconciled"), "{}", s["diagnostics"]);
    assert_eq!(s["cash_flow"]["amortization_cents"], 60_000);
    assert_eq!(s["cash_flow"]["operating_cents"], 95_000);
    assert_eq!(s["cash_flow"]["investing_cents"], -80_000);
    assert_eq!(s["current"]["income_statement"]["net_income_cents"], 40_000);
    assert_eq!(s["tax_entries"].as_array().unwrap().len(), 1);
}

#[test]
fn gifi_csv_passes_tax_import_script() {
    let path = std::env::temp_dir().join(format!("nxt-eng-gifi-{}.csv", std::process::id()));
    std::fs::write(&path, reports::gifi_csv(&fixture())).unwrap();
    let Ok(out) = std::process::Command::new("python3").arg("scripts/import-gifi-csv.py").arg(&path).output() else { return };
    let _ = std::fs::remove_file(&path);
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
