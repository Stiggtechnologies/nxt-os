//! Local save, calculate, and GIFI detail. WebTax is not opened and nothing is filed.
use nxt_os::{store::Store, tax::{models::{ReturnInput, RevisionInput}, Tax}};
use serde_json::json;

fn profile() -> serde_json::Value {
    json!({
        "legal_name":"Entirely synthetic tax test corporation", "corporation_type":"ccpc",
        "province":"AB", "address":"Synthetic address used only by acceptance tests",
        "officer_name":"Synthetic test officer", "officer_title":"Test preparer",
        "reconciliation_confirmed":true, "detail_rollup":true,
        "gifi":[
            {"section":"balance_sheet","code":"2599","current_cents":10_000_000,"prior_cents":8_000_000},
            {"section":"balance_sheet","code":"3499","current_cents":3_000_000,"prior_cents":2_000_000},
            {"section":"balance_sheet","code":"3620","current_cents":7_000_000,"prior_cents":6_000_000},
            {"section":"income_statement","code":"8000","current_cents":12_000_000,"prior_cents":9_000_000},
            {"section":"income_statement","code":"8120","current_cents":8_000_000,"prior_cents":6_000_000},
            {"section":"income_statement","code":"8299","current_cents":20_000_000,"prior_cents":15_000_000},
            {"section":"income_statement","code":"8521","current_cents":10_000_000,"prior_cents":8_000_000},
            {"section":"income_statement","code":"9368","current_cents":10_000_000,"prior_cents":8_000_000},
            {"section":"income_statement","code":"9970","current_cents":10_000_000,"prior_cents":7_000_000},
            {"section":"income_statement","code":"9990","current_cents":2_300_000,"prior_cents":1_610_000},
            {"section":"income_statement","code":"9999","current_cents":7_700_000,"prior_cents":5_390_000}
        ],
        "adjustments":[{"id":"tax-provision","label":"Verified income-tax provision addback","amount_cents":2_300_000}],
        "notes":"Synthetic reconciliation; no WebTax session and no filing claim."
    })
}

#[tokio::test]
async fn local_save_calculates_gifi_detail_without_opening_webtax() {
    let tax = Tax::new(Store::memory());
    let input: ReturnInput = serde_json::from_value(json!({
        "program":"T2","tax_year":2025,"jurisdiction":"AB","name":"Synthetic WebTax-shaped workpaper",
        "period_start":"2025-01-01","period_end":"2025-12-31",
        "inputs":{"scope_confirmed":true,"taxable_income_cents":10_000_000,"active_business_income_cents":10_000_000,"small_business_eligible":false},
        "corporate": profile()
    })).unwrap();
    let created = tax.save(None, input).await.ok().unwrap();
    assert_eq!(created.corporate.as_ref().unwrap().gifi.iter().any(|row| row.code == "8000"), true);
    let calculated = tax.calculate(&created.id, RevisionInput { expected_revision: created.revision, idempotency_key: Some("webtax-detail".into()) }).await.ok().unwrap();
    let analysis = &calculated.calculation.as_ref().unwrap().result["corporate_analysis"];
    assert_eq!(analysis["supported"], true);
    assert_eq!(analysis["totals"]["income_revenue_current"], 20_000_000);
    let codes: Vec<_> = analysis["diagnostics"].as_array().unwrap().iter().filter_map(|row| row["code"].as_str()).collect();
    assert!(!codes.contains(&"gifi_detail_mismatch"));
    assert_eq!(Tax::capabilities()["filing_enabled"], false);
    assert_eq!(calculated.status, "calculated");
}
