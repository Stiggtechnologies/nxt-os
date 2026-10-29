//! Synthetic composition with current-main metadata, lockdown and opaque files.
use nxt_os::{
    engagement::{Engagements, models::EngagementInput},
    firm::Firm,
    ledger::engine::Ledger,
    store::Store,
    tax::Tax,
};
use serde_json::{Value, json};
use std::sync::Arc;

fn engine() -> Engagements {
    let ledger = Arc::new(Ledger::new(Store::memory()));
    let tax = Arc::new(Tax::new(Store::memory()));
    let firm = Arc::new(Firm::new(Store::memory(), ledger.clone()));
    Engagements::new(Store::memory(), ledger, tax, firm)
}
fn document() -> EngagementInput {
    serde_json::from_value(json!({
        "name":"Synthetic external composition","kind":"internal","currency":"CAD",
        "period_start":"2025-01-01","period_end":"2025-12-31",
        "accounts":[
            {"id":"cash","code":"1000","name":"Synthetic cash","kind":"asset","group_id":"assets","debit_cents":10000},
            {"id":"capital","code":"3000","name":"Synthetic capital","kind":"equity","group_id":"equity","credit_cents":10000}
        ],
        "groups":[{"id":"assets","code":"A","name":"Assets","kind":"asset","gifi_code":"1000"},{"id":"equity","code":"E","name":"Equity","kind":"equity","gifi_code":"3500"}],
        "workpapers":[{"id":"lead","reference":"A","title":"Synthetic lead","body":"\nFirst 😀 é\r\nSecond\rLast","required":true,"account_ids":["cash"],"tags":["review","tax"],"link_label":"Prior year lead"}]
    })).unwrap()
}
fn metadata(paper: &nxt_os::engagement::models::Workpaper) {
    assert_eq!(paper.tags, ["review", "tax"]);
    assert_eq!(paper.link_label.as_deref(), Some("Prior year lead"));
}

#[tokio::test]
async fn conversion_history_and_rollforward_keep_tags_labels_and_lockdown() {
    let eng = engine();
    let mut record = eng.save(None, document()).await.ok().unwrap();
    let exact_body = record.document.workpapers[0].body.clone();
    record = eng
        .calculate(
            &record.id,
            serde_json::from_value(json!({"expected_revision":record.revision})).unwrap(),
        )
        .await
        .ok()
        .unwrap();
    let manual_snapshot = serde_json::to_value(record.snapshot.as_ref().unwrap()).unwrap();
    let mut converted = record.document.clone();
    converted.expected_revision = Some(record.revision);
    converted.workpapers[0].document_type =
        nxt_os::engagement::models::WorkpaperType::ExternalReference;
    converted.workpapers[0].reference_url =
        Some("HTTPS://Example.INVALID:443/Exact?x=1&y=2#Part".into());
    record = eng.save(Some(&record.id), converted).await.ok().unwrap();
    metadata(&record.document.workpapers[0]);
    assert_eq!(record.document.workpapers[0].body, exact_body);
    assert_eq!(
        serde_json::to_value(&record.snapshots[0]).unwrap(),
        manual_snapshot
    );
    let mut invalid = record.document.clone();
    invalid.expected_revision = Some(record.revision);
    invalid.workpapers[0].link_label = Some("https://example.invalid/not-a-label".into());
    let before = serde_json::to_value(eng.get(&record.id).await.ok().unwrap()).unwrap();
    assert!(eng.save(Some(&record.id), invalid).await.is_err());
    assert_eq!(
        serde_json::to_value(eng.get(&record.id).await.ok().unwrap()).unwrap(),
        before
    );
    let mut invalid = record.document.clone();
    invalid.expected_revision = Some(record.revision);
    invalid.workpapers[0].reference_url = Some(" https://example.invalid".into());
    assert!(eng.save(Some(&record.id), invalid).await.is_err());
    record = eng
        .calculate(
            &record.id,
            serde_json::from_value(json!({"expected_revision":record.revision})).unwrap(),
        )
        .await
        .ok()
        .unwrap();
    record = eng.signoff(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"workpaper_id":"lead","stage":"prepared","operator":"Synthetic preparer","note":"Native note checkpoint"})).unwrap()).await.ok().unwrap();
    record = eng.transition(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"status":"in_review","operator":"Synthetic reviewer","note":"Internal review"})).unwrap()).await.ok().unwrap();
    record = eng.signoff(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"workpaper_id":"lead","stage":"reviewed","operator":"Synthetic reviewer","note":"Saved metadata reviewed"})).unwrap()).await.ok().unwrap();
    let today = chrono::Utc::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    record = eng.set_lockdown(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"spacing_days":14,"reference_date":today,"operator":"Synthetic reviewer"})).unwrap()).await.ok().unwrap();
    assert!(eng.transition(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"status":"locked","operator":"Synthetic reviewer","note":"Too early"})).unwrap()).await.is_err());
    record = eng.set_lockdown(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"spacing_days":45,"reference_date":"2020-01-01","operator":"Synthetic reviewer"})).unwrap()).await.ok().unwrap();
    record = eng.transition(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"status":"locked","operator":"Synthetic reviewer","note":"Spacing elapsed"})).unwrap()).await.ok().unwrap();
    let source_before = serde_json::to_value(&record).unwrap();
    let locked_snapshot = serde_json::to_value(record.snapshot.as_ref().unwrap()).unwrap();
    let next = eng.rollforward(&record.id, serde_json::from_value(json!({"expected_revision":record.revision,"period_start":"2026-01-01","period_end":"2026-12-31"})).unwrap()).await.ok().unwrap();
    metadata(&next.document.workpapers[0]);
    assert_eq!(next.document.workpapers[0].body, "");
    assert_eq!(
        next.document.workpapers[0].reference_url,
        record.document.workpapers[0].reference_url
    );
    assert_eq!(
        serde_json::to_value(&next.rollforward_origin.as_ref().unwrap().source_snapshot).unwrap(),
        locked_snapshot
    );
    assert_eq!(
        serde_json::to_value(eng.get(&record.id).await.ok().unwrap()).unwrap(),
        source_before
    );
    assert_eq!(
        serde_json::to_value(&record.snapshots[0]).unwrap(),
        manual_snapshot
    );
    let package: Value = eng.package(&record.id).await.ok().unwrap();
    assert_eq!(
        package["engagement"]["document"]["workpapers"][0]["tags"],
        json!(["review", "tax"])
    );
    assert_eq!(
        package["engagement"]["document"]["workpapers"][0]["link_label"],
        "Prior year lead"
    );
    assert_eq!(
        eng.gifi_lines(&record.id).await.ok().unwrap()["taxcycle_file"],
        false
    );
}

#[tokio::test]
async fn external_reference_is_isolated_from_manual_attachment_description_and_tickmarks() {
    let eng = engine();
    let mut input = document();
    let mut external = input.workpapers[0].clone();
    external.id = "external".into();
    external.reference = "X".into();
    external.required = false;
    external.document_type = nxt_os::engagement::models::WorkpaperType::ExternalReference;
    external.reference_url = Some("https://example.invalid/native-note".into());
    input.workpapers.push(external);
    let mut record = eng.save(None, input).await.ok().unwrap();
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    png.extend(b"IHDR");
    png.extend(&8u32.to_be_bytes());
    png.extend(&4u32.to_be_bytes());
    let upload = |rev| json!({"expected_revision":rev,"upload_id":"synthetic_png","file_name":"synthetic.png","operator_label":"Synthetic","note":"Opaque bytes only","content_hex":hex::encode(&png)});
    record = eng
        .upload_attachment(
            &record.id,
            "lead",
            serde_json::from_value(upload(record.revision)).unwrap(),
        )
        .await
        .ok()
        .unwrap();
    let attachment_id = record.document.workpapers[0].attachment_versions[0]
        .id
        .clone();
    let description = eng
        .describe_attachment(&record.id, "lead", &attachment_id)
        .await
        .ok()
        .unwrap();
    assert_eq!(
        (
            description["kind"].as_str(),
            description["width"].as_u64(),
            description["height"].as_u64()
        ),
        (Some("png"), Some(8), Some(4))
    );
    for field in ["rendered", "scanned", "executed"] {
        assert_eq!(description[field], false);
    }
    eng.add_file_tickmark(&record.id,serde_json::from_value(json!({"expected_revision":record.revision,"workpaper_id":"lead","attachment_id":attachment_id,"page":1,"code":"T1","note":"Synthetic page anchor"})).unwrap()).await.ok().unwrap();
    record = eng.get(&record.id).await.ok().unwrap();
    assert_eq!(record.file_tickmarks.len(), 1);
    metadata(&record.document.workpapers[0]);
    metadata(&record.document.workpapers[1]);
    let before = serde_json::to_value(&record).unwrap();
    let mut external_upload = upload(record.revision);
    external_upload["upload_id"] = json!("synthetic_external");
    assert!(
        eng.upload_attachment(
            &record.id,
            "external",
            serde_json::from_value(external_upload).unwrap()
        )
        .await
        .is_err()
    );
    assert!(eng.add_file_tickmark(&record.id,serde_json::from_value(json!({"expected_revision":record.revision,"workpaper_id":"external","attachment_id":attachment_id,"page":1,"code":"T1","note":"Foreign attachment must reject"})).unwrap()).await.is_err());
    let mut forbidden = record.document.clone();
    forbidden.expected_revision = Some(record.revision);
    forbidden.workpapers[0].document_type =
        nxt_os::engagement::models::WorkpaperType::ExternalReference;
    forbidden.workpapers[0].reference_url =
        Some("https://example.invalid/cannot-erase-file".into());
    forbidden.workpapers[0].attachment_versions.clear();
    assert!(eng.save(Some(&record.id), forbidden).await.is_err());
    assert_eq!(
        serde_json::to_value(eng.get(&record.id).await.ok().unwrap()).unwrap(),
        before
    );
    assert_eq!(
        eng.attachment(&record.id, "lead", &attachment_id)
            .await
            .ok()
            .unwrap()
            .1,
        png
    );
}
