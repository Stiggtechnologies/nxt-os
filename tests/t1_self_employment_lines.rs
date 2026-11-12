//! Professional, commission, farming and fishing lines (T1 13699–14300). Synthetic inputs only.
use nxt_os::{
    store::Store,
    tax::{calculations::{calculate, Calculation}, catalog, models::{ReturnInput, RevisionInput}, Tax},
};
use serde_json::{json, Value};

fn base() -> Value {
    json!({"scope_confirmed": true, "full_year_resident": true, "provincial_adjustments_not_applicable": true})
}
fn line(result: &Calculation, code: &str) -> Option<i64> {
    result.lines.iter().find(|line| line.code == code).map(|line| line.amount)
}
fn blocking(result: &Calculation, code: &str) -> bool {
    result.diagnostics.iter().any(|d| d.code == code && d.blocks_review)
}

#[test]
fn each_activity_flows_gross_and_net_to_its_t1_line_and_total_income() {
    for (prefix, verify, gross_line, net_line) in [
        ("professional", "t2125_professional_verified", "13699", "13700"),
        ("commission", "t2125_commission_verified", "13899", "13900"),
        ("farming", "t2042_verified", "14099", "14100"),
        ("fishing", "t2121_verified", "14299", "14300"),
    ] {
        for province in ["AB", "ON", "BC"] {
            let mut inputs = base();
            inputs["interest_income_cents"] = json!(100_000);
            inputs[format!("{prefix}_gross_cents")] = json!(5_000_000);
            inputs[format!("{prefix}_expenses_cents")] = json!(1_000_000);
            inputs[format!("{prefix}_cca_cents")] = json!(200_000);
            let unverified = calculate("T1", 2025, province, &inputs).unwrap();
            assert!(unverified.diagnostics.iter().any(|d| d.code == "verification_required" && d.field.as_deref() == Some(verify)), "{prefix}");
            assert!(!unverified.supported);
            inputs[verify] = json!(true);
            let r = calculate("T1", 2025, province, &inputs).unwrap();
            assert_eq!(line(&r, gross_line), Some(5_000_000), "{prefix} {province}");
            assert_eq!(line(&r, net_line), Some(3_800_000), "{prefix} {province}");
            assert_eq!(line(&r, "15000"), Some(3_900_000), "{prefix} {province}");
            assert_eq!(line(&r, "13500"), None, "business line stays absent");
            // CPP payable is still required (Schedule 8 Part 4 or separate preparation).
            assert!(blocking(&r, "self_employed_cpp_payable_not_implemented"));
        }
    }
}

#[test]
fn schedule8_part4_uses_line_27_across_all_five_activities() {
    let mut split = base();
    for (key, value) in [
        ("t2125_verified", json!(true)), ("self_employment_gross_cents", json!(2_000_000)),
        ("t2125_professional_verified", json!(true)), ("professional_gross_cents", json!(1_500_000)),
        ("t2125_commission_verified", json!(true)), ("commission_gross_cents", json!(600_000)), ("commission_expenses_cents", json!(100_000)),
        ("t2042_verified", json!(true)), ("farming_gross_cents", json!(1_000_000)),
        ("t2121_verified", json!(true)), ("fishing_gross_cents", json!(500_000)),
        ("schedule8_part4_verified", json!(true)),
    ] { split[key] = value; }
    let mut single = base();
    single["t2125_verified"] = json!(true);
    single["schedule8_part4_verified"] = json!(true);
    single["self_employment_gross_cents"] = json!(5_500_000);
    for province in ["AB", "ON", "BC"] {
        let a = calculate("T1", 2025, province, &split).unwrap();
        let b = calculate("T1", 2025, province, &single).unwrap();
        assert!(a.supported, "{province}: {:?}", a.diagnostics);
        // 55,000.00 of earnings: base 5,150,000 x 9.9% = 509,850; first additional 2% = 103,000.
        assert_eq!(line(&a, "42100"), Some(612_850));
        assert_eq!(line(&a, "31000"), Some(254_925));
        assert_eq!(line(&a, "22200"), Some(357_925));
        for code in ["42100", "31000", "22200", "15000", "23600", "26000", "43500"] {
            assert_eq!(line(&a, code), line(&b, code), "{province} {code}");
        }
        assert_eq!(a.totals["balance"], b.totals["balance"]);
        assert_eq!(line(&a, "13900"), Some(500_000));
    }
}

#[test]
fn losses_cca_and_part4_limits_are_enforced() {
    let mut cca_loss = base();
    cca_loss["t2121_verified"] = json!(true);
    cca_loss["fishing_gross_cents"] = json!(100_000);
    cca_loss["fishing_expenses_cents"] = json!(90_000);
    cca_loss["fishing_cca_cents"] = json!(20_000);
    assert!(calculate("T1", 2025, "AB", &cca_loss).is_err());

    let mut farm_loss = base();
    farm_loss["employment_income_cents"] = json!(5_000_000);
    farm_loss["t2042_verified"] = json!(true);
    farm_loss["farming_gross_cents"] = json!(100_000);
    farm_loss["farming_expenses_cents"] = json!(300_000);
    let r = calculate("T1", 2025, "AB", &farm_loss).unwrap();
    assert_eq!(line(&r, "14100"), Some(-200_000));
    assert!(blocking(&r, "farming_loss_not_calculated"));
    assert!(!r.supported);

    let mut part4_loss = base();
    part4_loss["schedule8_part4_verified"] = json!(true);
    part4_loss["t2125_verified"] = json!(true);
    part4_loss["self_employment_gross_cents"] = json!(5_000_000);
    part4_loss["t2125_professional_verified"] = json!(true);
    part4_loss["professional_expenses_cents"] = json!(10_000);
    assert!(calculate("T1", 2025, "AB", &part4_loss).is_err());

    let mut employed = base();
    employed["employment_income_cents"] = json!(1_000_000);
    employed["schedule8_part4_verified"] = json!(true);
    employed["t2125_commission_verified"] = json!(true);
    employed["commission_gross_cents"] = json!(1_000_000);
    assert!(calculate("T1", 2025, "AB", &employed).is_err());
}

#[test]
fn new_fields_are_2025_only_and_rejected_elsewhere() {
    let mut inputs = base();
    inputs["professional_gross_cents"] = json!(100);
    inputs["t2125_professional_verified"] = json!(true);
    let r2026 = calculate("T1", 2026, "AB", &inputs);
    assert!(r2026.map(|r| !r.supported).unwrap_or(true));
    let mut unknown = base();
    unknown["professional_bonus_cents"] = json!(1);
    assert!(calculate("T1", 2025, "AB", &unknown).is_err());
}

#[test]
fn catalog_exposes_optional_fields_with_lines_forms_and_dtmax_reference() {
    let c = catalog::catalog();
    let p = &c["programs"][0];
    assert_eq!(p["field_catalog_version"], "nxt-tax-fields/T1/2025/4");
    let section = p["sections"].as_array().unwrap().iter().find(|s| s["id"] == "self-employment-totals").unwrap();
    let fields = section["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 16);
    for (key, form, gross, net, line) in [
        ("professional_gross_cents", "T2125", "GrossProfession", "ProfessionalInc", "13699"),
        ("commission_gross_cents", "T2125", "GrossCommission", "CommissionInc", "13899"),
        ("farming_gross_cents", "T2042", "GrossFarming", "FarmingInc", "14099"),
        ("fishing_gross_cents", "T2121", "GrossFishing", "FishingInc", "14299"),
    ] {
        let f = fields.iter().find(|f| f["key"] == key).unwrap();
        assert_eq!(f["optional"], true);
        assert_eq!(f["default_policy"], "omit");
        assert_eq!(f["tax_years"], json!([2025]));
        assert_eq!(f["source_form"], form);
        assert_eq!(f["dtmax_keywords"]["gross"], gross);
        assert_eq!(f["dtmax_keywords"]["net"], net);
        assert!(f["label"].as_str().unwrap().contains(line));
        assert!(p["defaults"].get(key).is_none());
        assert!(p["input_schema"]["properties"].get(key).is_some());
    }
    let business = p["sections"].as_array().unwrap().iter().flat_map(|s| s["fields"].as_array().unwrap())
        .find(|f| f["key"] == "self_employment_gross_cents").unwrap();
    assert_eq!(business["dtmax_keywords"]["net"], "BusinessInc");
}

#[tokio::test]
async fn saved_professional_workpaper_calculates_through_the_store() {
    let tax = Tax::new(Store::memory());
    let mut inputs = base();
    inputs["t2125_professional_verified"] = json!(true);
    inputs["professional_gross_cents"] = json!(4_000_000);
    inputs["schedule8_part4_verified"] = json!(true);
    let doc: ReturnInput = serde_json::from_value(json!({
        "program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "Synthetic professional", "client_ref": "SYN", "inputs": inputs
    })).unwrap();
    let saved = tax.save(None, doc).await.unwrap_or_else(|e| panic!("{}", e.1));
    let calculated = tax.calculate(&saved.id, RevisionInput { expected_revision: saved.revision, idempotency_key: None })
        .await.unwrap_or_else(|e| panic!("{}", e.1));
    let result = &calculated.calculation.as_ref().unwrap().result;
    let amount = |code: &str| result["lines"].as_array().unwrap().iter().find(|l| l["code"] == code).and_then(|l| l["amount"].as_i64());
    assert_eq!(amount("13700"), Some(4_000_000));
    assert_eq!(amount("42100"), Some(434_350));
}
