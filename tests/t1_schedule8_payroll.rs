use nxt_os::{
    store::Store,
    tax::{Tax, models::{ReturnInput, RevisionInput, TaxReturn, TransitionInput}},
};
use serde_json::{Value, json};

fn input(province: &str, gross: i64, slips: Value) -> ReturnInput {
    serde_json::from_value(json!({
        "program": "T1", "tax_year": 2025, "jurisdiction": province, "name": "Synthetic CPP",
        "inputs": {
            "scope_confirmed": true, "full_year_resident": true,
            "provincial_adjustments_not_applicable": true,
            "t2125_verified": true, "self_employment_gross_cents": gross,
        },
        "slip_groups": slips,
    })).unwrap()
}

async fn confirm(tax: &Tax, draft: &TaxReturn) -> TaxReturn {
    let mut edit = ReturnInput::from(draft);
    edit.expected_revision = Some(draft.revision);
    edit.inputs["schedule8_payroll_verified"] = json!(true);
    tax.save(Some(&draft.id), edit).await.unwrap_or_else(|err| panic!("{}", err.1))
}

fn revision(record: &TaxReturn) -> RevisionInput {
    RevisionInput { expected_revision: record.revision, idempotency_key: None }
}

fn line(result: &Value, code: &str) -> i64 {
    result["lines"].as_array().unwrap().iter()
        .find(|line| line["code"] == code).unwrap_or_else(|| panic!("Missing line {code}"))
        ["amount"].as_i64().unwrap()
}

#[tokio::test]
async fn mixed_payroll_uses_both_cpp_bands_and_allows_normal_review() {
    for province in ["AB", "ON", "BC"] {
        let tax = Tax::new(Store::memory());
        let draft = tax.save(None, input(province, 3_000_000, json!([
            {"kind": "T4", "boxes": {"14": 5000000, "16": 276675, "16A": 0, "22": 100000, "26": 5000000}}
        ]))).await.unwrap_or_else(|err| panic!("{}", err.1));
        let held = tax.calculate(&draft.id, revision(&draft)).await.unwrap_or_else(|err| panic!("{}", err.1));
        assert_eq!(held.calculation.as_ref().unwrap().result["supported"], false);
        let confirmed = confirm(&tax, &held).await;
        let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
        let result = &calculated.calculation.as_ref().unwrap().result;
        assert_eq!(result["supported"], true, "{province}: {}", result["diagnostics"]);
        // Wages consume $46,500 of the first band. The business uses the remaining
        // $21,300 and $8,700 of CPP2 earnings, to cent precision after the CRA multiplier.
        for (code, amount) in [
            ("30800", 230_175), ("22215", 46_500), ("42100", 323_070),
            ("31000", 105_435), ("22200", 217_635), ("44800", 0), ("23600", 7_735_865),
        ] {
            assert_eq!(line(result, code), amount, "{province} {code}");
        }
        assert_eq!(line(result, "33500"),
            line(result, "30000") + line(result, "30800") + line(result, "31000") + line(result, "31260"));
        assert_eq!(result["totals"]["balance"], line(result, "43500") - 100_000);
        assert!(result["slip_group_summary"]["recorded_not_calculated"].get("T4 box 26").is_none());
        assert_eq!(result["slip_group_summary"]["applied_schedule8"]["T4 box 26"]["cents"], 5_000_000);
        let reviewed = tax.transition(&calculated.id, TransitionInput {
            expected_revision: calculated.revision, status: "in_review".into(),
            reviewer: "Synthetic preparer".into(), note: String::new(),
            filing_reference: String::new(), t183_envelope_id: String::new(),
        }).await.unwrap_or_else(|err| panic!("{}", err.1));
        assert_eq!(reviewed.status, "in_review");
    }
}

#[tokio::test]
async fn t4_fallback_and_payroll_excess_are_reconciled_once() {
    let tax = Tax::new(Store::memory());
    for (gross, overpayment, business) in [
        (0, 3_325, None),
        (10_000, 2_730, Some((0, 100))),
        (100_000, 0, Some((5_250, 3_625))),
    ] {
        let draft = tax.save(None, input("AB", gross, json!([
            {"kind": "T4", "boxes": {"14": 5000000, "16": 280000, "22": 100000}}
        ]))).await.unwrap_or_else(|err| panic!("{}", err.1));
        let confirmed = confirm(&tax, &draft).await;
        let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
        let result = &calculated.calculation.as_ref().unwrap().result;
        assert_eq!(result["supported"], true);
        assert_eq!(line(result, "30800"), 230_175);
        assert_eq!(line(result, "22215"), 46_500);
        assert_eq!(line(result, "44800"), overpayment);
        assert_eq!(result["totals"]["payments"], 100_000 + overpayment);
        if let Some((payable, deduction)) = business {
            assert_eq!(line(result, "42100"), payable);
            assert_eq!(line(result, "22200"), deduction);
        }
    }
}

#[tokio::test]
async fn multiple_t4s_shift_surplus_from_first_to_second_band_before_assessing_business_cpp() {
    let tax = Tax::new(Store::memory());
    let draft = tax.save(None, input("AB", 1_000_000, json!([
        {"kind": "T4", "boxes": {"14": 4000000, "16": 217175}},
        {"kind": "T4", "boxes": {"14": 4000000, "16": 217175}}
    ]))).await.unwrap_or_else(|err| panic!("{}", err.1));
    let confirmed = confirm(&tax, &draft).await;
    let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
    let result = &calculated.calculation.as_ref().unwrap().result;
    assert_eq!(result["supported"], true);
    assert_eq!(line(result, "30800"), 335_610);
    assert_eq!(line(result, "22215"), 98_740);
    assert_eq!(line(result, "42100"), 17_320);
    assert_eq!(line(result, "22200"), 17_320);
    assert_eq!(line(result, "44800"), 0);
}

#[tokio::test]
async fn blank_pensionable_box_uses_box_14_but_explicit_zero_is_preserved() {
    let tax = Tax::new(Store::memory());
    for (boxes, payable) in [
        (json!({"14": 100000}), 29_750),
        (json!({"14": 100000, "26": 0}), 17_850),
    ] {
        let draft = tax.save(None, input("AB", 500_000, json!([
            {"kind": "T4", "boxes": boxes}
        ]))).await.unwrap_or_else(|err| panic!("{}", err.1));
        let confirmed = confirm(&tax, &draft).await;
        let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
        let result = &calculated.calculation.as_ref().unwrap().result;
        assert_eq!(result["supported"], true);
        assert_eq!(line(result, "42100"), payable);
    }
}

#[tokio::test]
async fn prorated_mixed_cpp_leaves_actual_t4_earnings_unscaled() {
    let tax = Tax::new(Store::memory());
    let mut annual = input("AB", 4_000_000, json!([
        {"kind": "T4", "boxes": {"14": 2000000, "16": 108588, "26": 2000000}}
    ]));
    annual.inputs["schedule8_contribution_months"] = json!(6);
    let draft = tax.save(None, annual).await.unwrap_or_else(|err| panic!("{}", err.1));
    let confirmed = confirm(&tax, &draft).await;
    let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
    let result = &calculated.calculation.as_ref().unwrap().result;
    assert_eq!(result["supported"], true);
    // $20,000 of pensionable T4 earnings and $20,000 of prorated business income.
    assert_eq!(line(result, "10100"), 2_000_000);
    assert_eq!(line(result, "13500"), 4_000_000);
    assert_eq!(line(result, "30800"), 90_338);
    assert_eq!(line(result, "22215"), 18_250);
    assert_eq!(line(result, "42100"), 221_035);
}

#[tokio::test]
async fn business_loss_reduces_income_without_reducing_employment_cpp() {
    let tax = Tax::new(Store::memory());
    let mut annual = input("AB", 100_000, json!([
        {"kind": "T4", "boxes": {"14": 5000000, "16": 280000, "26": 5000000}}
    ]));
    annual.inputs["self_employment_expenses_cents"] = json!(150_000);
    let draft = tax.save(None, annual).await.unwrap_or_else(|err| panic!("{}", err.1));
    let confirmed = confirm(&tax, &draft).await;
    let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
    let result = &calculated.calculation.as_ref().unwrap().result;
    assert_eq!(result["supported"], true);
    assert_eq!(line(result, "13500"), -50_000);
    assert_eq!(line(result, "15000"), 4_950_000);
    assert_eq!(line(result, "30800"), 230_175);
    assert_eq!(line(result, "22215"), 46_500);
    assert_eq!(line(result, "42100"), 0);
    assert_eq!(line(result, "44800"), 3_325);
}

#[tokio::test]
async fn no_contribution_months_reconcile_erroneous_t4_cpp_as_an_overpayment() {
    let tax = Tax::new(Store::memory());
    let mut annual = input("AB", 500_000, json!([
        {"kind": "T4", "boxes": {"14": 5000000, "16": 100000, "16A": 5000}}
    ]));
    annual.inputs["schedule8_contribution_months"] = json!(0);
    let draft = tax.save(None, annual).await.unwrap_or_else(|err| panic!("{}", err.1));
    let confirmed = confirm(&tax, &draft).await;
    let calculated = tax.calculate(&confirmed.id, revision(&confirmed)).await.unwrap_or_else(|err| panic!("{}", err.1));
    let result = &calculated.calculation.as_ref().unwrap().result;
    assert_eq!(result["supported"], true);
    assert_eq!(line(result, "13500"), 500_000);
    for code in ["22200", "22215", "30800", "31000", "42100"] {
        assert_eq!(line(result, code), 0, "{code}");
    }
    assert_eq!(line(result, "44800"), 105_000);
}

#[tokio::test]
async fn edited_t4_requires_new_confirmation_and_qpp_remains_blocked() {
    let tax = Tax::new(Store::memory());
    let draft = tax.save(None, input("AB", 100_000, json!([
        {"kind": "T4", "boxes": {"14": 5000000, "16": 276675}}
    ]))).await.unwrap_or_else(|err| panic!("{}", err.1));
    let confirmed = confirm(&tax, &draft).await;
    let mut edit = ReturnInput::from(&confirmed);
    edit.expected_revision = Some(confirmed.revision);
    edit.slip_groups.as_mut().unwrap()[0].boxes.insert("17A".into(), 100);
    let edited = tax.save(Some(&confirmed.id), edit).await.unwrap_or_else(|err| panic!("{}", err.1));
    assert_eq!(edited.inputs["schedule8_payroll_verified"], false);
    let qpp = confirm(&tax, &edited).await;
    assert!(tax.calculate(&qpp.id, revision(&qpp)).await.err().unwrap().1.contains("RC381"));
    let mut manual = ReturnInput::from(&confirmed);
    manual.expected_revision = Some(qpp.revision);
    manual.slip_groups = Some(vec![]);
    let no_t4 = tax.save(Some(&confirmed.id), manual).await.unwrap_or_else(|err| panic!("{}", err.1));
    let no_t4 = confirm(&tax, &no_t4).await;
    assert!(tax.calculate(&no_t4.id, revision(&no_t4)).await.is_err());
}
