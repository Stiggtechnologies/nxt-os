use super::accounting_service_procedures::run;
use chrono::NaiveDate;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn date() -> NaiveDate {
    "2024-10-01".parse().unwrap()
}

pub(super) fn fixtures() -> [(&'static str, Value); 20] {
    let ids = json!({"ids":["a","b"]});
    let movements = json!({"opening_cents":100,"additions_cents":[25],"removals_cents":[10],"reported_closing_cents":115});
    let balance = json!({"balance_cents":100});
    let hash = "a".repeat(64);
    [
        (
            "Transaction intake and coding",
            json!({"source_manifest":ids,"transaction_register":ids}),
        ),
        (
            "Accounts payable and expenses",
            json!({"invoice_register":{"invoices":[{"supplier":"vendor","number":"INV1","cents":100}]},"payee_verification":{"verified_supplier_ids":["vendor"]}}),
        ),
        (
            "Accounts receivable and collections",
            json!({"aging_schedule":{"invoices":[{"due_on":"2024-09-15","open_cents":100}]},"cash_application":balance}),
        ),
        (
            "Treasury and disbursements",
            json!({"cash_forecast":balance,"payment_register":{"amounts_cents":[25]}}),
        ),
        (
            "Fixed assets, leases, and inventory",
            json!({"asset_register":movements,"inventory_rollforward":movements}),
        ),
        (
            "Revenue recognition and contract accounting",
            json!({"contract_register":{"entries":[{"earned_on":"2024-09-25","cents":100},{"earned_on":"2024-11-01","cents":20}]},"cutoff_schedule":balance}),
        ),
        (
            "Consolidation, intercompany, and FX",
            json!({"entity_trial_balances":{"by_pair_cents":{"a_b":100}},"elimination_register":{"by_pair_cents":{"a_b":-100}}}),
        ),
        (
            "Industry and fund accounting",
            json!({"restriction_terms":ids,"allocation_schedule":{"ids":["a"]}}),
        ),
        (
            "Payroll processing and exception review",
            json!({"payroll_register":{"gross_cents":125,"deductions_cents":[10,15],"net_cents":100}}),
        ),
        (
            "Payroll reconciliation and corrections",
            json!({"payroll_register":balance,"bank_settlement":balance,"ledger_clearing":balance}),
        ),
        (
            "Tax intake and carryforwards",
            json!({"prior_returns":balance,"carryforward_schedule":{"prior_closing_cents":100,"additions_cents":[20],"applied_cents":[5],"reported_closing_cents":115}}),
        ),
        (
            "Canadian corporate tax",
            json!({"corporate_profile":balance,"book_to_tax_schedule":{"increase_cents":[20],"decrease_cents":[5],"reported_taxable_income_cents":115}}),
        ),
        (
            "US business and pass-through tax",
            json!({"entity_classification":balance,"allocation_register":{"by_pair_cents":{"part1":50,"part2":50}}}),
        ),
        (
            "Indirect tax",
            json!({"taxable_sales_register":{"collected_cents":100,"adjustments_cents":[20]},"remittance_reconciliation":{"claimed_credits_cents":40,"reported_net_cents":80}}),
        ),
        (
            "Tax filing and authorization coordination",
            json!({"frozen_return":{"artifact_sha256":hash},"filing_authorization":{"authorized_artifact_sha256":hash,"obtained_on":"2024-09-01","expires_on":"2024-11-01"},"filing_diagnostics":{"blockers":0}}),
        ),
        (
            "Evidence, confirmations, and sampling",
            json!({"population_manifest":ids,"sample_register":{"ids":["a"]},"confirmation_log":{"ids":["a"]}}),
        ),
        (
            "Substantive testing",
            json!({"assertion_map":ids,"testing_workpapers":ids,"exceptions_register":{"blockers":0}}),
        ),
        (
            "Audit completion and reporting",
            json!({"misstatement_register":{"uncorrected_cents":[40,-20]},"completion_checklist":{"escalation_cents":100}}),
        ),
        (
            "Agreed-upon procedures",
            json!({"agreed_procedures":ids,"procedure_evidence":ids,"factual_findings":ids}),
        ),
        (
            "Engagement quality review coordination",
            json!({"reviewer_eligibility":{"engagement_lead_id":"person1","reviewer_id":"person2","eligible":true},"review_points":{"blockers":0}}),
        ),
    ]
}

#[test]
fn twenty_service_procedures_compute_from_archived_inputs() {
    for (role, inputs) in fixtures() {
        let result = run(role, date(), &sources(inputs), true)
            .unwrap_or_else(|error| panic!("{role}: {}", error.1));
        assert_eq!(result["passes"], true, "{role}: {result}");
        assert_eq!(result["release_authorized"], false);
    }
}

fn sources(inputs: Value) -> BTreeMap<String, String> {
    inputs
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), json!({"procedure_inputs":value}).to_string()))
        .collect()
}

#[test]
fn service_procedures_flag_duplicate_invoices_wrong_filing_and_offsetting_misstatements() {
    let duplicate = sources(json!({
        "invoice_register":{"invoices":[{"supplier":"vendor","number":"INV1","cents":100},{"supplier":"Vendor","number":" inv1 ","cents":100}]},
        "payee_verification":{"verified_supplier_ids":["vendor"]}
    }));
    let result = run("Accounts payable and expenses", date(), &duplicate, true)
        .ok()
        .unwrap();
    assert_eq!(result["findings"]["exception_rows"], json!([2]));
    assert_eq!(result["passes"], false);
    let wrong = sources(json!({
        "frozen_return":{"artifact_sha256":"a".repeat(64)},
        "filing_authorization":{"authorized_artifact_sha256":"b".repeat(64),"obtained_on":"2024-09-01","expires_on":"2024-11-01"},
        "filing_diagnostics":{"blockers":0}
    }));
    assert_eq!(
        run(
            "Tax filing and authorization coordination",
            date(),
            &wrong,
            true
        )
        .ok()
        .unwrap()["passes"],
        false
    );
    let material = sources(json!({
        "misstatement_register":{"uncorrected_cents":[70,-70]},"completion_checklist":{"escalation_cents":100}
    }));
    let result = run("Audit completion and reporting", date(), &material, true)
        .ok()
        .unwrap();
    assert_eq!(result["findings"]["aggregate_signed_cents"], "0");
    assert_eq!(result["passes"], false);
    assert!(run("Valuation", date(), &BTreeMap::new(), true).is_err());
    assert!(run("Canadian corporate tax", date(), &BTreeMap::new(), true).is_err());
}
