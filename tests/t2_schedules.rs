//! Synthetic T2 schedule fixtures only. The SIN below is a fabricated Luhn-valid test number.
use nxt_os::tax::{corporate::GifiRow, t2_schedules::*};

fn row(section: &str, code: &str, cents: i64) -> GifiRow {
    GifiRow { section: section.into(), code: code.into(), label: String::new(), current_cents: cents, prior_cents: 0 }
}
fn base() -> T2Input {
    T2Input {
        corporation_type: "ccpc".into(),
        gifi: vec![
            row("balance_sheet", "2599", 50_000_00), row("balance_sheet", "3499", 20_000_00),
            row("balance_sheet", "3620", 30_000_00), row("balance_sheet", "3640", 50_000_00),
            row("balance_sheet", "3500", 100_00),
            row("income_statement", "8299", 120_000_00), row("income_statement", "9368", 90_000_00),
            row("income_statement", "8670", 5_000_00), row("income_statement", "9990", 4_000_00),
            row("income_statement", "9999", 26_000_00),
        ],
        schedule1: vec![
            Schedule1Item { line: "101".into(), amount_cents: 4_000_00, source_ref: "TAX-1".into() },
            Schedule1Item { line: "104".into(), amount_cents: 5_000_00, source_ref: "TAX-2".into() },
            Schedule1Item { line: "403".into(), amount_cents: 6_000_00, source_ref: "S8".into() },
        ],
        cca: vec![CcaClass { class: "8".into(), opening_ucc_cents: 20_000_00, additions_cents: 10_000_00, ..Default::default() }],
        shareholders: vec![Shareholder { name: "Synthetic Holder".into(), kind: "individual".into(), identifier: "046454286".into(), common_bp: 10_000, preferred_bp: 0 }],
        tax_year_start: "2024-01-01".into(), tax_year_end: "2024-12-31".into(),
    }
}

#[test]
fn schedule1_8_50_reconcile() {
    let out = build(&base()).unwrap();
    assert_eq!(out["filed"], false);
    // Class 8 @20%: (20,000 + 10,000 − 5,000 half-year) × 20% = 5,000
    assert_eq!(out["schedule_8"]["schedule1_line_403_cca_cents"], 5_000_00);
    // 26,000 + 9,000 − 6,000 = 29,000 on line 300
    assert_eq!(out["schedule_1"]["t2_line_300_cents"], 29_000_00);
    // 403 (6,000) ≠ Schedule 8 (5,000) blocks
    assert_eq!(out["supported"], false);
    let mut fixed = base();
    fixed.schedule1[2].amount_cents = 5_000_00;
    let out = build(&fixed).unwrap();
    assert_eq!(out["supported"], true, "{}", out["diagnostics"]);
    assert_eq!(out["schedule_50"]["shareholders"][0]["identifier_masked"], "***286");
    assert!(!out.to_string().contains("046454286"));
}

#[test]
fn blocking_rules() {
    let mut i = base();
    i.schedule1[2].amount_cents = 5_000_00;
    i.shareholders[0].identifier = "046454287".into();
    i.cca[0].no_property_left = true;
    i.gifi[1].current_cents = 1;
    let out = build(&i).unwrap();
    let codes: Vec<String> = out["diagnostics"].as_array().unwrap().iter().map(|d| d["code"].as_str().unwrap().to_string()).collect();
    for c in ["schedule50_identifier", "s100_assets_equal_liabilities_and_equity", "s1_404_matches_schedule8"] { assert!(codes.iter().any(|x| x == c), "{c} {codes:?}"); }
    assert_eq!(out["schedule_8"]["schedule1_line_404_terminal_loss_cents"], 30_000_00);
    let mut i = base();
    i.shareholders.clear();
    i.cca = vec![CcaClass { class: "99".into(), ..Default::default() }];
    let out = build(&i).unwrap();
    let s = out["diagnostics"].to_string();
    assert!(s.contains("schedule50_required") && s.contains("cca_class_unsupported"));
    let mut i = base();
    i.schedule1[0].amount_cents = -1;
    assert!(build(&i).is_err());
    assert_eq!(rate("10"), Some(30));
}

#[test]
fn recapture_goes_to_line_107() {
    let mut i = base();
    i.cca = vec![CcaClass { class: "10".into(), opening_ucc_cents: 1_000_00, proceeds_cents: 3_000_00, ..Default::default() }];
    let out = build(&i).unwrap();
    assert_eq!(out["schedule_8"]["schedule1_line_107_recapture_cents"], 2_000_00);
    assert_eq!(out["schedule_8"]["classes"][0]["ucc_end_of_year_cents"], 0);
}

fn codes(out: &serde_json::Value) -> Vec<String> {
    out["diagnostics"].as_array().unwrap().iter().map(|d| d["code"].as_str().unwrap().to_string()).collect()
}

#[test]
fn reg_1100_2_factors_and_zev() {
    let mut i = base();
    // Class 54 AIIP in 2024: A = 3/2 (Reg 1100(2) A(d)(ii)); (10,000 + 15,000) × 30% = 7,500
    i.cca = vec![CcaClass { class: "54".into(), additions_cents: 10_000_00, aiip_additions_cents: 10_000_00, ..Default::default() }];
    let out = build(&i).unwrap();
    assert_eq!(out["schedule_8"]["classes"][0]["factor_a"], "3/2");
    assert_eq!(out["schedule_8"]["schedule1_line_403_cca_cents"], 7_500_00);
    // Class 8 RIIP in 2024: A.1 = 1/2, no half-year: (10,000 + 5,000) × 20% = 3,000
    i.cca = vec![CcaClass { class: "8".into(), additions_cents: 10_000_00, riip_additions_cents: 10_000_00, ..Default::default() }];
    assert_eq!(build(&i).unwrap()["schedule_8"]["schedule1_line_403_cca_cents"], 3_000_00);
    // Regular ZEV additions, straddle years and 44 AIIP after 2023 block
    i.cca = vec![CcaClass { class: "55".into(), additions_cents: 1_00, ..Default::default() }];
    assert!(codes(&build(&i).unwrap()).contains(&"cca_zev_acquisition_date".to_string()));
    i.cca = vec![CcaClass { class: "44".into(), additions_cents: 1_00, aiip_additions_cents: 1_00, ..Default::default() }];
    assert!(codes(&build(&i).unwrap()).contains(&"cca_factor_unsourced".to_string()));
    i.tax_year_start = "2024-06-01".into(); i.tax_year_end = "2025-05-31".into();
    i.cca[0].class = "8".into();
    assert!(codes(&build(&i).unwrap()).contains(&"cca_tax_year".to_string()));
    i.tax_year_start = "2024-06-01".into(); i.tax_year_end = "2024-12-31".into();
    assert!(codes(&build(&i).unwrap()).contains(&"cca_short_year".to_string()));
}

#[test]
fn straight_line_classes_13_14() {
    let mut i = base();
    // Schedule III: min(10,000/5, 10,000/10) = 1,000; incurred in year, not accelerated: × 50% = 500
    i.cca = vec![
        CcaClass { class: "13".into(), additions_cents: 10_000_00, leaseholds: vec![LeaseholdPart { cost_cents: 10_000_00, periods_12m: 10, incurred_in_year: true, ..Default::default() }], ..Default::default() },
        // Reg 1100(1)(c): 3,650 × 365 / 3,650 = 365
        CcaClass { class: "14".into(), opening_ucc_cents: 3_650_00, limited_life: vec![LimitedLifeProperty { cost_cents: 3_650_00, life_days: 3650, days_in_year: 365, ..Default::default() }], ..Default::default() },
    ];
    let out = build(&i).unwrap();
    assert_eq!(out["schedule_8"]["classes"][0]["column_21_cca_cents"], 500_00, "{}", out["schedule_8"]);
    assert_eq!(out["schedule_8"]["classes"][1]["column_21_cca_cents"], 365_00);
    // AIIP leasehold incurred in 2024 is outside 1100(1)(b)(i)(A) and (B): (ii) full amount
    i.cca.truncate(1);
    i.cca[0].leaseholds[0].acceleration = "aiip".into();
    assert_eq!(build(&i).unwrap()["schedule_8"]["schedule1_line_403_cca_cents"], 1_000_00);
    i.tax_year_start = "2023-01-01".into(); i.tax_year_end = "2023-12-31".into();
    assert_eq!(build(&i).unwrap()["schedule_8"]["schedule1_line_403_cca_cents"], 1_500_00);
    i.cca[0].leaseholds.clear();
    assert!(codes(&build(&i).unwrap()).contains(&"cca_class13_parts".to_string()));
}
