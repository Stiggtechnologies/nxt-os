//! Synthetic T5013 CCA fixtures only.
use nxt_os::tax::{t2_schedules::CcaClass, t5013_cca::*};

fn input() -> T5013CcaInput {
    T5013CcaInput {
        fiscal_period_start: "2025-01-01".into(), fiscal_period_end: "2025-12-31".into(),
        cca: vec![CcaClass { class: "8".into(), opening_ucc_cents: 10_000_01, ..Default::default() }],
        partners: vec![
            Partner { partner_id: "A".into(), name: "A Ltd".into(), share_bp: 3_333 },
            Partner { partner_id: "B".into(), name: "B".into(), share_bp: 3_333 },
            Partner { partner_id: "C".into(), name: "C".into(), share_bp: 3_334 },
        ],
        rental_or_leasing_property: Some(false),
    }
}
fn blocks(v: &serde_json::Value, code: &str) -> bool { v["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == code && d["blocks_review"] == true) }

#[test]
fn line_250_allocated_to_box_040() {
    let v = build(&input()).unwrap();
    assert_eq!(v["line_250_total_cca_cents"], 2_000_00); // 20% × 10,000.01 = 2,000.002 → 2,000.00
    let a: Vec<i64> = v["allocations"].as_array().unwrap().iter().map(|x| x["box_040_cca_cents"].as_i64().unwrap()).collect();
    assert_eq!(a.iter().sum::<i64>(), 2_000_00);
    assert_eq!(a, vec![666_60, 666_60, 666_80]);
    assert_eq!(v["supported"], true, "{}", v["diagnostics"]);
    assert_eq!(v["filed"], false);
}

#[test]
fn allocation_remainders_and_blocks() {
    assert_eq!(allocate(100, &[3_333, 3_333, 3_334]), vec![33, 33, 34]);
    assert_eq!(allocate(101, &[5_000, 5_000]), vec![51, 50]);
    let mut i = input(); i.partners[2].share_bp = 3_000;
    assert!(blocks(&build(&i).unwrap(), "t5013_partner_shares"));
    let mut i = input(); i.rental_or_leasing_property = None;
    assert!(blocks(&build(&i).unwrap(), "t5013_rental_property"));
    let mut i = input(); i.cca[0].additions_cents = 1_000_00; i.cca[0].diep_cents = 1_000_00; i.cca[0].immediate_expensing_cents = 1_000_00;
    let v = build(&i).unwrap(); assert!(blocks(&v, "t5013_immediate_expensing"));
    assert!(!v["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "cca_immediate_expensing"));
    let mut i = input(); i.partners[1].partner_id = "A".into(); assert!(build(&i).is_err());
    // Short fiscal period: Reg 1100(3) proration applies (CRA partnership CCA page).
    let mut i = input(); i.fiscal_period_start = "2025-07-01".into();
    assert_eq!(build(&i).unwrap()["line_250_total_cca_cents"], 1_008_21); // 2,000.00 × 184/365, engine truncates
}
