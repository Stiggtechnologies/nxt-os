//! Parallel AR/AP, classes, attachments, and native sync. No QuickBooks write-back.
use nxt_os::{
    company_books::{CompanyBooks, models::*},
    store::Store,
};
use serde_json::json;

fn chunk() -> Chunk {
    serde_json::from_value(json!({
        "accounts":[
            {"id":"cash","name":"Cash","kind":"asset"},
            {"id":"ar","name":"Accounts receivable","kind":"asset"},
            {"id":"ap","name":"Accounts payable","kind":"liability"},
            {"id":"revenue","name":"Revenue","kind":"income"},
            {"id":"expense","name":"Expense","kind":"expense"},
            {"id":"equity","name":"Equity","kind":"equity"}
        ],
        "entries":[{"id":"src-1","date":"2026-01-01","source_id":"archive-row:synthetic:1","memo":"Opening","lines":[
            {"account_id":"cash","debit_cents":"10000","credit_cents":"0","source_row":1,"memo":"Debit"},
            {"account_id":"equity","debit_cents":"0","credit_cents":"10000","source_row":2,"memo":"Credit"}
        ]}]
    })).unwrap()
}
fn preview(sha: &str) -> PreviewInput {
    serde_json::from_value(json!({
        "request_id":"preview-sub","expected_head":null,
        "company":{"name":"Synthetic","source_company_id":"realm","currency":"CAD","identity_verified":true,"currency_verified":true},
        "source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},
        "chunks":[sha],
        "trial_balance":[
            {"account_id":"cash","debit_cents":"10000","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},
            {"account_id":"equity","debit_cents":"0","credit_cents":"10000","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}},
            {"account_id":"ar","debit_cents":"0","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":3}},
            {"account_id":"ap","debit_cents":"0","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":4}},
            {"account_id":"revenue","debit_cents":"0","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":5}},
            {"account_id":"expense","debit_cents":"0","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":6}}
        ]
    })).unwrap()
}

#[tokio::test]
async fn parallel_subledger_posts_open_items_without_changing_source_rows() {
    let books = CompanyBooks::new(Store::memory());
    let uploaded = books.upload("co", chunk()).await.map_err(|e| format!("{} {}", e.0, e.1)).unwrap();
    let preview = books.preview("co", preview(uploaded["sha256"].as_str().unwrap())).await.map_err(|e| format!("{} {}", e.0, e.1)).unwrap();
    assert_eq!(preview["stage"], "preview_ready");
    let committed = books.commit("co", CommitInput { preview_id: preview["preview_id"].as_str().unwrap().into(), expected_head: None, request_id: "commit-sub".into(), reviewer_label: "Synthetic reviewer".into() }).await.map_err(|e| format!("{} {}", e.0, e.1)).unwrap();
    let head = committed["head_sha256"].as_str().unwrap().to_string();
    books.add_class("co", serde_json::from_value(json!({"request_id":"class-east","name":"East"})).unwrap()).await.ok().unwrap();
    let invoice = books.open_document("co", serde_json::from_value(json!({
        "request_id":"inv-1","expected_head":head,"operator_label":"Synthetic operator","kind":"invoice","contact":"Maple",
        "date":"2026-02-02","memo":"Synthetic invoice","control_account":"ar",
        "lines":[{"account_id":"revenue","amount_cents":2500,"class_id":"class-east"}]
    })).unwrap()).await.ok().unwrap();
    assert_eq!(invoice["open_cents"], 2500);
    assert_eq!(invoice["write_back"], false);
    let head = invoice["head_sha256"].as_str().unwrap().to_string();
    let paid = books.apply_payment("co", serde_json::from_value(json!({
        "request_id":"pay-1","expected_head":head,"operator_label":"Synthetic operator","document_id":"inv-1","cash_account":"cash","date":"2026-02-03","amount_cents":1000
    })).unwrap()).await.ok().unwrap();
    assert_eq!(paid["open_cents"], 1500);
    assert_eq!(paid["write_back"], false);
    let head = paid["head_sha256"].as_str().unwrap().to_string();
    assert_eq!(books.apply_payment("co", serde_json::from_value(json!({
        "request_id":"pay-over","expected_head":head,"operator_label":"Synthetic operator","document_id":"inv-1","cash_account":"cash","date":"2026-02-04","amount_cents":1501
    })).unwrap()).await.unwrap_err().0, 400);
    let source = books.entries("co", 0, 50, None).await.ok().unwrap();
    let source_id = source["rows"].as_array().unwrap().iter().find(|row| row["id"]=="src-1").unwrap()["id"].as_str().unwrap();
    assert_eq!(books.sparse_native("co", source_id, serde_json::from_value(json!({"expected_head":head,"expected_sync":0,"memo":"no"})).unwrap()).await.unwrap_err().0, 409);
    let native_id = invoice["journal_entry_id"].as_str().unwrap();
    let sparse = books.sparse_native("co", native_id, serde_json::from_value(json!({"expected_head":head,"expected_sync":0,"memo":"Classed copy","class_id":"class-east"})).unwrap()).await.ok().unwrap();
    assert_eq!(sparse["sync"], 1);
    assert_eq!(sparse["amounts_unchanged"], true);
    assert_eq!(books.sparse_native("co", native_id, serde_json::from_value(json!({"expected_head":head,"expected_sync":0,"memo":"stale"})).unwrap()).await.unwrap_err().0, 409);
    assert_eq!(books.delete_native("co", source_id, serde_json::from_value(json!({"request_id":"del-src","expected_head":head,"operator_label":"Synthetic operator","date":"2026-02-05","memo":"no"})).unwrap()).await.unwrap_err().0, 409);
    let removed = books.delete_native("co", native_id, serde_json::from_value(json!({"request_id":"del-inv","expected_head":head,"operator_label":"Synthetic operator","date":"2026-02-05","memo":"Parallel delete"})).unwrap()).await.ok().unwrap();
    assert_eq!(removed["status"], "deleted_in_parallel");
    assert_eq!(removed["source_bytes_unchanged"], true);
    assert_eq!(removed["write_back"], false);
    let attached = books.attach("co", serde_json::from_value(json!({"owner_id":"inv-1","filename":"note.txt","content_hex":"6869"})).unwrap()).await.ok().unwrap();
    assert_eq!(attached["content_executed"], false);
    assert_eq!(attached["byte_len"], 2);
    let view = books.subledger("co").await.ok().unwrap();
    assert_eq!(view["authoritative_system"], "quickbooks");
    assert_eq!(view["documents"][0]["open_cents"], 1500);
}
