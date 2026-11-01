//! Operator prior-year columns on a T1. They do not change the current calculation or file.
use nxt_os::{store::Store, tax::{models::{PriorLine, ReturnInput, RevisionInput}, Tax}};
use serde_json::json;

#[tokio::test]
async fn prior_year_columns_do_not_transfer_or_file() {
    let tax = Tax::new(Store::memory());
    let input: ReturnInput = serde_json::from_value(json!({
        "program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic prior-year column",
        "inputs":{"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000}
    })).unwrap();
    let created = tax.save(None, input).await.ok().unwrap();
    let calculated = tax.calculate(&created.id, RevisionInput { expected_revision: created.revision, idempotency_key: Some("prior-column".into()) }).await.ok().unwrap();
    let calc_id = calculated.calculation.as_ref().unwrap().id.clone();
    let stored = tax.set_prior_lines(&calculated.id, calculated.revision, vec![PriorLine { code: "10100".into(), cents: 5_000_000 }]).await.ok().unwrap();
    assert_eq!(stored["transfers_credits"], false);
    assert_eq!(stored["filing_enabled"], false);
    assert_eq!(stored["keywords_loaded"], false);
    assert_eq!(stored["write_back"], false);
    assert_eq!(stored["authoritative_system"], "dtmax");
    assert_eq!(stored["return"]["input_revision"], calculated.input_revision);
    assert_eq!(stored["return"]["inputs"]["employment_income_cents"], 5_000_000);
    assert_eq!(stored["return"]["calculation"]["id"], calc_id);
    assert_eq!(stored["return"]["prior_lines"][0]["cents"], 5_000_000);
    let t2: ReturnInput = serde_json::from_value(json!({
        "program":"T2","tax_year":2025,"jurisdiction":"AB","name":"Synthetic corporate",
        "period_start":"2025-01-01","period_end":"2025-12-31","inputs":{"scope_confirmed":true}
    })).unwrap();
    let corporate = tax.save(None, t2).await.ok().unwrap();
    assert_eq!(tax.set_prior_lines(&corporate.id, corporate.revision, vec![PriorLine { code: "10100".into(), cents: 1 }]).await.unwrap_err().0, 400);
}
