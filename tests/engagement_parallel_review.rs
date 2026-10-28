//! Lockdown spacing, a non-TaxCycle GIFI list, and file metadata without rendering.
use nxt_os::{
    engagement::{models::EngagementInput, Engagements},
    firm::Firm,
    ledger::engine::Ledger,
    store::Store,
    tax::Tax,
};
use serde_json::json;
use std::sync::Arc;

fn engine() -> Engagements {
    let ledger = Arc::new(Ledger::new(Store::memory()));
    let tax = Arc::new(Tax::new(Store::memory()));
    let firm = Arc::new(Firm::new(Store::memory(), ledger.clone()));
    Engagements::new(Store::memory(), ledger, tax, firm)
}

fn document() -> EngagementInput {
    serde_json::from_value(json!({
        "name":"Synthetic engagement","client_ref":"synthetic-only","kind":"internal",
        "period_start":"2025-01-01","period_end":"2025-12-31","currency":"CAD",
        "accounts":[
            {"id":"a_cash","code":"1000","name":"Cash","kind":"asset","group_id":"g_cash","debit_cents":1700000,"credit_cents":0},
            {"id":"a_ap","code":"2000","name":"Payables","kind":"liability","group_id":"g_ap","debit_cents":0,"credit_cents":300000},
            {"id":"a_eq","code":"3000","name":"Capital","kind":"equity","group_id":"g_eq","debit_cents":0,"credit_cents":1200000},
            {"id":"a_rev","code":"4000","name":"Revenue","kind":"revenue","group_id":"g_rev","debit_cents":0,"credit_cents":900000},
            {"id":"a_exp","code":"5000","name":"Expense","kind":"expense","group_id":"g_exp","debit_cents":700000,"credit_cents":0}
        ],
        "groups":[
            {"id":"g_cash","code":"1000","name":"Cash","kind":"asset","gifi_code":"1000"},
            {"id":"g_ap","code":"2000","name":"Payables","kind":"liability","gifi_code":"2620"},
            {"id":"g_eq","code":"3000","name":"Capital","kind":"equity","gifi_code":"3500"},
            {"id":"g_rev","code":"4000","name":"Revenue","kind":"revenue","gifi_code":"8000"},
            {"id":"g_exp","code":"5000","name":"Expense","kind":"expense","gifi_code":"9270"}
        ],
        "workpapers":[{"id":"wp1","reference":"A","title":"Lead","body":"Synthetic evidence.","required":true,"account_ids":["a_cash"]}]
    })).unwrap()
}

#[tokio::test]
async fn lockdown_gifi_export_and_file_metadata_do_not_fetch_vendor_files() {
    let eng = engine();
    let mut record = eng.save(None, document()).await.ok().unwrap();
    let pdf = b"%PDF-1.4\n1 0 obj << /Type /Page >> endobj\n%%EOF";
    record = eng.upload_attachment(&record.id, "wp1", serde_json::from_value(json!({
        "expected_revision": record.revision, "upload_id": "up_pdf", "file_name": "page.pdf",
        "operator_label": "Synthetic QA", "note": "Synthetic pdf", "content_hex": hex::encode(pdf)
    })).unwrap()).await.ok().unwrap();
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    png.extend(b"IHDR");
    png.extend(&8u32.to_be_bytes());
    png.extend(&4u32.to_be_bytes());
    record = eng.upload_attachment(&record.id, "wp1", serde_json::from_value(json!({
        "expected_revision": record.revision, "upload_id": "up_png", "file_name": "mark.png",
        "operator_label": "Synthetic QA", "note": "Synthetic png", "content_hex": hex::encode(png)
    })).unwrap()).await.ok().unwrap();
    let pdf_id = record.document.workpapers[0].attachment_versions.iter().find(|v| v.file_name == "page.pdf").unwrap().id.clone();
    let png_id = record.document.workpapers[0].attachment_versions.iter().find(|v| v.file_name == "mark.png").unwrap().id.clone();
    let described = eng.describe_attachment(&record.id, "wp1", &pdf_id).await.ok().unwrap();
    assert_eq!(described["kind"], "pdf");
    assert_eq!(described["page_count"], 1);
    assert_eq!(described["rendered"], false);
    assert_eq!(described["scanned"], false);
    assert_eq!(described["executed"], false);
    let image = eng.describe_attachment(&record.id, "wp1", &png_id).await.ok().unwrap();
    assert_eq!(image["kind"], "png");
    assert_eq!(image["width"], 8);
    assert_eq!(image["height"], 4);
    assert_eq!(image["rendered"], false);
    let mark = eng.add_file_tickmark(&record.id, serde_json::from_value(json!({
        "expected_revision": record.revision, "workpaper_id": "wp1", "attachment_id": pdf_id, "page": 1, "code": "T1", "note": "Synthetic"
    })).unwrap()).await.ok().unwrap();
    assert_eq!(mark["rendered"], false);
    record = eng.get(&record.id).await.ok().unwrap();

    let lines = eng.gifi_lines(&record.id).await.ok().unwrap();
    assert_eq!(lines["taxcycle_file"], false);
    assert_eq!(lines["format"], "nxt-gifi-lines/v1");
    assert!(lines["rows"].as_array().unwrap().iter().any(|row| row["gifi"] == "8000" && row["credit_cents"] == 900000));

    record = eng.calculate(&record.id, serde_json::from_value(json!({"expected_revision": record.revision})).unwrap()).await.ok().unwrap();
    assert_eq!(record.snapshot.as_ref().unwrap().result["supported"], true);
    record = eng.signoff(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "workpaper_id": "wp1", "stage": "prepared", "operator": "Synthetic preparer", "note": "Prepared synthetic evidence."})).unwrap()).await.ok().unwrap();
    record = eng.transition(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "status": "in_review", "operator": "Synthetic reviewer", "note": "Send synthetic file to review."})).unwrap()).await.ok().unwrap();
    record = eng.signoff(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "workpaper_id": "wp1", "stage": "reviewed", "operator": "Synthetic reviewer", "note": "Reviewed synthetic evidence."})).unwrap()).await.ok().unwrap();
    let today = chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string();
    record = eng.set_lockdown(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "spacing_days": 14, "reference_date": today, "operator": "Synthetic reviewer"})).unwrap()).await.ok().unwrap();
    assert_eq!(record.lockdown.as_ref().unwrap().spacing_days, 14);
    assert_eq!(eng.transition(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "status": "locked", "operator": "Synthetic reviewer", "note": "Too early."})).unwrap()).await.unwrap_err().0, 409);
    record = eng.set_lockdown(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "spacing_days": 45, "reference_date": "2020-01-01", "operator": "Synthetic reviewer"})).unwrap()).await.ok().unwrap();
    record = eng.transition(&record.id, serde_json::from_value(json!({"expected_revision": record.revision, "status": "locked", "operator": "Synthetic reviewer", "note": "Spacing elapsed."})).unwrap()).await.ok().unwrap();
    assert_eq!(record.status, "locked");
    assert_eq!(record.lockdown.as_ref().unwrap().spacing_days, 45);
}
