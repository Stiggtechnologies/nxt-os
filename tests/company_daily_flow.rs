//! Parallel departments, items, employees, estimates, purchase orders, time, and budgets.
//! Sales-tax preference follows the published ignore-or-balance rule. No QuickBooks write-back.
use nxt_os::{company_books::{CompanyBooks, models::*}, store::Store};
use serde_json::json;

fn chunk() -> Chunk {
    serde_json::from_value(json!({
        "accounts":[
            {"id":"cash","name":"Cash","kind":"asset"},
            {"id":"ar","name":"Accounts receivable","kind":"asset"},
            {"id":"revenue","name":"Revenue","kind":"income"},
            {"id":"hst","name":"HST payable","kind":"liability"},
            {"id":"equity","name":"Equity","kind":"equity"}
        ],
        "entries":[{"id":"src-1","date":"2026-01-01","source_id":"archive-row:synthetic:1","memo":"Opening","lines":[
            {"account_id":"cash","debit_cents":"10000","credit_cents":"0","source_row":1,"memo":"Debit"},
            {"account_id":"equity","debit_cents":"0","credit_cents":"10000","source_row":2,"memo":"Credit"}
        ]}]
    })).unwrap()
}
fn preview(sha: &str) -> PreviewInput {
    let row = |account, n| json!({"account_id":account,"debit_cents":"0","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":n}});
    serde_json::from_value(json!({
        "request_id":"preview-daily","expected_head":null,
        "company":{"name":"Synthetic","source_company_id":"realm","currency":"CAD","identity_verified":true,"currency_verified":true},
        "source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},
        "chunks":[sha],
        "trial_balance":[
            {"account_id":"cash","debit_cents":"10000","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},
            {"account_id":"equity","debit_cents":"0","credit_cents":"10000","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}},
            row("ar", 3), row("revenue", 4), row("hst", 5)
        ]
    })).unwrap()
}

#[tokio::test]
async fn parallel_daily_lists_and_sales_tax_preference_do_not_write_back() {
    let books = CompanyBooks::new(Store::memory());
    let uploaded = books.upload("co", chunk()).await.ok().unwrap();
    let preview = books.preview("co", preview(uploaded["sha256"].as_str().unwrap())).await.ok().unwrap();
    assert_eq!(preview["stage"], "preview_ready");
    let committed = books.commit("co", CommitInput { preview_id: preview["preview_id"].as_str().unwrap().into(), expected_head: None, request_id: "commit-daily".into(), reviewer_label: "Synthetic reviewer".into(), needs_review: false, review_blockers: vec![], suspense_account_id: None }).await.ok().unwrap();
    let head = committed["head_sha256"].as_str().unwrap().to_string();
    let before = books.entries("co", 0, 50, None).await.ok().unwrap()["rows"].as_array().unwrap().len();

    books.add_department("co", serde_json::from_value(json!({"request_id":"dept-1","name":"East"})).unwrap()).await.ok().unwrap();
    books.add_item("co", serde_json::from_value(json!({"request_id":"item-1","name":"Advisory","income_account":"revenue"})).unwrap()).await.ok().unwrap();
    books.add_employee("co", serde_json::from_value(json!({"request_id":"emp-1","name":"Synthetic Staff","active":true})).unwrap()).await.ok().unwrap();
    let estimate = books.add_estimate("co", serde_json::from_value(json!({"request_id":"est-1","contact":"Maple","document_number":"EST-1","lines":[{"item_id":"item-1","amount_cents":4000}]})).unwrap()).await.ok().unwrap();
    assert_eq!(estimate["posts_to_ledger"], false);
    let order = books.add_purchase_order("co", serde_json::from_value(json!({"request_id":"po-1","contact":"Vendor","document_number":"PO-1","lines":[{"item_id":"item-1","amount_cents":1500}]})).unwrap()).await.ok().unwrap();
    assert_eq!(order["posts_to_ledger"], false);
    let time = books.add_time("co", serde_json::from_value(json!({"request_id":"time-1","employee_id":"emp-1","date":"2026-02-02","minutes":90})).unwrap()).await.ok().unwrap();
    assert_eq!(time["posts_payroll"], false);
    let budget = books.add_budget("co", serde_json::from_value(json!({"request_id":"bud-1","account_id":"revenue","period":"2026-02","amount_cents":8000})).unwrap()).await.ok().unwrap();
    assert_eq!(budget["posts_to_ledger"], false);
    assert_eq!(books.entries("co", 0, 50, None).await.ok().unwrap()["rows"].as_array().unwrap().len(), before);
    assert_eq!(books.payroll_run("co").await.unwrap_err().0, 409);

    let ignored = books.open_document("co", serde_json::from_value(json!({
        "request_id":"inv-off","expected_head":head,"operator_label":"Synthetic operator","kind":"invoice","contact":"Maple",
        "date":"2026-02-02","memo":"Tax off","control_account":"ar","document_number":"INV-OFF",
        "lines":[{"account_id":"revenue","amount_cents":1000}],
        "tax":{"tax_cents":130,"lines":[{"account_id":"hst","amount_cents":50}]}
    })).unwrap()).await.ok().unwrap();
    assert_eq!(ignored["tax_detail_ignored"], true);
    assert_eq!(ignored["tax_cents"], 0);
    assert_eq!(ignored["total_cents"], 1000);
    assert_eq!(ignored["document_number"], "INV-OFF");
    assert_eq!(ignored["write_back"], false);
    let head = ignored["head_sha256"].as_str().unwrap().to_string();

    books.set_sales_tax("co", serde_json::from_value(json!({"enabled":true})).unwrap()).await.ok().unwrap();
    assert_eq!(books.open_document("co", serde_json::from_value(json!({
        "request_id":"inv-bad","expected_head":head,"operator_label":"Synthetic operator","kind":"invoice","contact":"Maple",
        "date":"2026-02-03","memo":"Tax bad","control_account":"ar",
        "lines":[{"account_id":"revenue","amount_cents":1000}],
        "tax":{"tax_cents":130,"lines":[{"account_id":"hst","amount_cents":50}]}
    })).unwrap()).await.unwrap_err().0, 400);
    let taxed = books.open_document("co", serde_json::from_value(json!({
        "request_id":"inv-on","expected_head":head,"operator_label":"Synthetic operator","kind":"invoice","contact":"Maple",
        "date":"2026-02-03","memo":"Tax on","control_account":"ar","document_number":"INV-ON",
        "lines":[{"account_id":"revenue","amount_cents":1000}],
        "tax":{"tax_cents":130,"lines":[{"account_id":"hst","amount_cents":130}]}
    })).unwrap()).await.ok().unwrap();
    assert_eq!(taxed["tax_detail_ignored"], false);
    assert_eq!(taxed["tax_cents"], 130);
    assert_eq!(taxed["total_cents"], 1130);
    assert_eq!(taxed["write_back"], false);

    let view = books.daily("co").await.ok().unwrap();
    assert_eq!(view["departments"].as_array().unwrap().len(), 1);
    assert_eq!(view["items"].as_array().unwrap().len(), 1);
    assert_eq!(view["employees"].as_array().unwrap().len(), 1);
    assert_eq!(view["estimates"].as_array().unwrap().len(), 1);
    assert_eq!(view["purchase_orders"].as_array().unwrap().len(), 1);
    assert_eq!(view["time"].as_array().unwrap().len(), 1);
    assert_eq!(view["budgets"].as_array().unwrap().len(), 1);
    assert_eq!(view["payroll_enabled"], false);
    assert_eq!(view["multi_currency"], false);
    assert_eq!(view["authoritative_system"], "quickbooks");
    let source = books.entries("co", 0, 50, None).await.ok().unwrap();
    assert!(source["rows"].as_array().unwrap().iter().any(|row| row["id"] == "src-1"));
}
