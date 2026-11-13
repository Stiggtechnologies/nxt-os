use nxt_os::{
    store::Store,
    tax::{calculations::{calculate, Calculation}, models::{ReturnInput, RevisionInput}, Tax},
};
use serde_json::{json, Value};

fn business(gross: i64) -> Value {
    json!({
        "scope_confirmed": true,
        "full_year_resident": true,
        "provincial_adjustments_not_applicable": true,
        "t2125_verified": true,
        "schedule8_part4_verified": true,
        "self_employment_gross_cents": gross
    })
}

fn line(result: &Calculation, code: &str) -> i64 {
    result.lines.iter().find(|line| line.code == code).unwrap().amount
}

#[test]
fn cpp_transfers_follow_part4_thresholds_and_balance() {
    for province in ["AB", "ON", "BC"] {
        for (gross, payable, credit, deduction) in [
            (350_000, 0, 0, 0),
            (350_010, 1, 1, 1),
            (5_000_000, 553_350, 230_175, 323_175),
            (7_130_000, 806_820, 335_610, 471_210),
            (8_120_000, 886_020, 335_610, 550_410),
            (9_000_000, 886_020, 335_610, 550_410),
        ] {
            let mut inputs = business(gross);
            inputs["instalments_cents"] = json!(100_000);
            let result = calculate("T1", 2025, province, &inputs).unwrap();
            assert!(result.supported, "{province}: {:?}", result.diagnostics);
            assert_eq!(line(&result, "22200"), deduction);
            assert_eq!(line(&result, "31000"), credit);
            assert_eq!(line(&result, "42100"), payable);
            assert_eq!(line(&result, "23600"), gross - deduction);
            assert_eq!(line(&result, "43500"), result.totals["income_tax"] + payable);
            assert_eq!(result.totals["balance"], line(&result, "43500") - 100_000);
        }
    }
}

#[test]
fn itemized_t2125_transfers_verified_expenses_and_cca_once() {
    let mut inputs = business(6_000_000);
    inputs["t2125_itemized_verified"] = json!(true);
    inputs["t2125_8521_cents"] = json!(100_000);
    inputs["t2125_8860_cents"] = json!(700_000);
    inputs["self_employment_cca_cents"] = json!(200_000);
    let result = calculate("T1", 2025, "AB", &inputs).unwrap();
    assert!(result.supported);
    assert_eq!(line(&result, "T2125:8521"), 100_000);
    assert_eq!(line(&result, "T2125:9368"), 1_000_000);
    assert_eq!(line(&result, "T2125:9946"), 5_000_000);
    assert_eq!(line(&result, "13500"), 5_000_000);
    assert_eq!(line(&result, "42100"), 553_350);
    inputs["t2125_itemized_verified"] = json!(false);
    assert!(!calculate("T1", 2025, "AB", &inputs).unwrap().supported);
    inputs["self_employment_expenses_cents"] = json!(800_000);
    assert!(calculate("T1", 2025, "AB", &inputs).is_err());
}

#[test]
fn prorated_cpp_transfers_use_the_part2_limits_and_keep_full_business_income() {
    for (months, gross, payable, credit, deduction) in [
        (1, 9_000_000, 73_835, 27_968, 45_868),
        (6, 5_000_000, 276_675, 115_088, 161_588),
        (6, 9_000_000, 443_010, 167_805, 275_205),
        (11, 9_000_000, 812_185, 307_643, 504_543),
        (12, 5_000_000, 553_350, 230_175, 323_175),
    ] {
        let mut inputs = business(gross);
        inputs["schedule8_contribution_months"] = json!(months);
        let result = calculate("T1", 2025, "AB", &inputs).unwrap();
        assert!(result.supported);
        assert_eq!(line(&result, "13500"), gross);
        assert_eq!(line(&result, "42100"), payable, "{months} months");
        assert_eq!(line(&result, "31000"), credit);
        assert_eq!(line(&result, "22200"), deduction);
        assert_eq!(line(&result, "23600"), gross - deduction);
    }
}

#[test]
fn cpp_months_are_bounded_and_require_a_confirmed_schedule8_path() {
    for invalid in [json!(0), json!(13), json!(5.5), json!("6"), Value::Null] {
        let mut inputs = business(5_000_000);
        inputs["schedule8_contribution_months"] = invalid;
        assert!(calculate("T1", 2025, "AB", &inputs).is_err());
    }
    let mut inputs = business(5_000_000);
    inputs["schedule8_contribution_months"] = json!(6);
    inputs["schedule8_part4_verified"] = json!(false);
    let result = calculate("T1", 2025, "AB", &inputs).unwrap();
    assert!(!result.supported);
    assert!(result.diagnostics.iter().any(|d| d.code == "schedule8_period_unverified"));
}

#[test]
fn unverified_or_conflicting_cpp_inputs_cannot_unlock_review() {
    for (key, value) in [
        ("employment_income_cents", json!(100)),
        ("cpp_base_credit_cents", json!(100)),
        ("se_cpp_credit_cents", json!(100)),
        ("born_1960_or_earlier", json!(true)),
    ] {
        let mut inputs = business(5_000_000);
        inputs[key] = value;
        assert!(calculate("T1", 2025, "AB", &inputs).is_err(), "{key}");
    }
    let mut inputs = business(5_000_000);
    inputs.as_object_mut().unwrap().remove("schedule8_part4_verified");
    let result = calculate("T1", 2025, "AB", &inputs).unwrap();
    assert!(!result.supported);
    assert!(!result.lines.iter().any(|line| line.code == "42100"));
}

#[tokio::test]
async fn saved_t4_blocks_part4_even_when_payroll_totals_are_zero() {
    let tax = Tax::new(Store::memory());
    let input: ReturnInput = serde_json::from_value(json!({
        "program": "T1",
        "tax_year": 2025,
        "jurisdiction": "AB",
        "name": "Synthetic business and payroll",
        "inputs": business(5_000_000),
        "slip_groups": [{"kind": "T4", "boxes": {"14": 0}}]
    })).unwrap();
    let record = tax.save(None, input).await.unwrap_or_else(|err| panic!("{}", err.1));
    let result = tax.calculate(&record.id, RevisionInput {
        expected_revision: record.revision, idempotency_key: None,
    }).await;
    assert!(result.err().unwrap().1.contains("saved T4"));
    assert!(tax.get(&record.id).await.unwrap_or_else(|err| panic!("{}", err.1)).calculation.is_none());
}
