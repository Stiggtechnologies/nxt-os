//! Firm client labels, directory export, chart apply, and internal playbooks.
//! QuickBooks stays authoritative. The ledger and spreadsheet write-back stay untouched.
use nxt_os::{firm::Firm, ledger::engine::Ledger, store::Store};
use serde_json::json;
use std::sync::Arc;

fn firm() -> (Firm, Arc<Ledger>) {
    let ledger = Arc::new(Ledger::new(Store::memory()));
    let firm = Firm::new(Store::memory(), ledger.clone());
    (firm, ledger)
}

#[tokio::test]
async fn firm_parallel_ops_stay_inside_nxt() {
    let (firm, ledger) = firm();
    let before = ledger.accounts().await.unwrap().len();
    let template = firm.save_template(None, serde_json::from_value(json!({
        "name": "Monthly close", "tasks": [{"title": "Reconcile cash"}]
    })).unwrap()).await.ok().unwrap();
    let project = firm.instantiate(&template.id, serde_json::from_value(json!({
        "name": "Maple close", "client_ref": "maple"
    })).unwrap()).await.ok().unwrap();
    assert_eq!(project.client_ref, "maple");

    let chart = firm.save_account_template(None, serde_json::from_value(json!({
        "name": "Synthetic chart",
        "accounts": [{"code": "1000", "name": "Cash", "kind": "asset"}]
    })).unwrap()).await.ok().unwrap();
    let preview = firm.preview_account_template(&chart.id, Default::default()).await.ok().unwrap();
    assert_eq!(preview["can_apply"], false);
    assert_eq!(preview["application_deferred"], true);
    let applied = firm.apply_account_template(&chart.id, serde_json::from_value(json!({"company_label": "maple"})).unwrap()).await.ok().unwrap();
    assert_eq!(applied["ledger_mutated"], false);
    assert_eq!(applied["write_back"], false);
    assert_eq!(applied["account_count"], 1);
    assert_eq!(ledger.accounts().await.unwrap().len(), before);

    for (kind, name) in [("customer", "Maple Co"), ("employee", "Synthetic Staff"), ("supplier", "North Supply")] {
        firm.save_contact(serde_json::from_value(json!({"kind": kind, "name": name})).unwrap()).await.ok().unwrap();
    }
    let export = firm.create_export(serde_json::from_value(json!({"report": "contact-list"})).unwrap()).await.ok().unwrap();
    assert_eq!(export.source, "nxt_firm_directory");
    let csv = firm.snapshot().await.ok().unwrap().exports.into_iter().find(|row| row.metadata.id == export.id).unwrap().csv;
    assert!(csv.contains("Maple Co"));
    assert!(csv.contains("Synthetic Staff"));
    assert!(csv.contains("North Supply"));
    assert!(csv.contains("nxt_parallel_not_quickbooks"));

    let playbook = firm.publish_playbook(serde_json::from_value(json!({
        "name": "Close pack", "template_id": template.id, "client_refs": ["maple", "cedar"]
    })).unwrap()).await.ok().unwrap();
    assert_eq!(playbook["external_publish"], false);
    assert_eq!(playbook["write_back"], false);
    assert_eq!(playbook["project_ids"].as_array().unwrap().len(), 2);
    let projects = firm.snapshot().await.ok().unwrap().projects;
    assert!(projects.iter().any(|row| row.client_ref == "cedar"));
}
