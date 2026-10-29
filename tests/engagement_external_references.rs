//! Original NXT HTTPS references; synthetic records, memory-only, no destination requests.
use axum::{body::Body, http::Request};
use nxt_os::{
    app_with,
    engagement::{
        Engagements, RevisionInput, SignoffInput, analysis,
        attachment_models::AttachmentUpload,
        models::{EngagementInput, Workpaper, WorkpaperType},
    },
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;
const STATE: &str = "engagements:state:v1";
fn fixture() -> Value {
    json!({"name":"Synthetic external references","kind":"internal","currency":"CAD","period_start":"2025-01-01","period_end":"2025-12-31","accounts":[{"id":"cash","code":"100","name":"Synthetic cash","kind":"asset","group_id":"assets","debit_cents":10000},{"id":"capital","code":"300","name":"Synthetic capital","kind":"equity","group_id":"equity","credit_cents":10000}],"groups":[{"id":"assets","code":"A","name":"Assets","kind":"asset"},{"id":"equity","code":"E","name":"Equity","kind":"equity"}],"workpapers":[{"id":"external","reference":"X1","title":"External <script>reference</script>","folder":"Planning","document_type":"external_reference","reference_url":"HTTPS://Example.INVALID:443/Exact?x=1&y=2#Part","body":"Native review note only.","required":true,"account_ids":["cash"]}]})
}
fn input(v: Value) -> EngagementInput {
    serde_json::from_value(v).unwrap()
}
fn engine() -> Engagements {
    let store = Store::memory();
    let ledger = Arc::new(nxt_os::ledger::engine::Ledger::new(Store::memory()));
    let tax = Arc::new(nxt_os::tax::Tax::new(Store::memory()));
    let firm = Arc::new(nxt_os::firm::Firm::new(Store::memory(), ledger.clone()));
    Engagements::new(store, ledger, tax, firm)
}
fn codes(value: &Value) -> Vec<&str> {
    value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect()
}
#[test]
fn raw_url_validation_is_strict_and_never_canonicalizes() {
    for url in [
        "https://example.invalid",
        "HTTPS://Example.INVALID:443/Exact?x=1&y=2#Part",
        "https://[::1]:8443/a",
        "https://éxample.invalid/path",
    ] {
        assert!(analysis::validate_reference_url(url).is_ok(), "{url}");
        let mut v = fixture();
        v["workpapers"][0]["reference_url"] = json!(url);
        let i = input(v);
        assert!(analysis::validate(&i).is_ok());
        assert_eq!(i.workpapers[0].reference_url.as_deref(), Some(url));
    }
    for url in [
        "",
        "http://example.invalid",
        "javascript:alert(1)",
        "data:text/html,x",
        "//example.invalid",
        "https:example.invalid",
        "https:///example.invalid",
        "https://",
        "https://@example.invalid",
        "https://u@example.invalid",
        "https://u:p@example.invalid",
        "https://example.invalid\\evil",
        "https://example.invalid/a b",
        " https://example.invalid",
        "https://example.invalid/\n",
        "https://example.invalid/\u{85}",
        "https://example.invalid/\u{2003}",
        "https://[bad]",
        "https://example.invalid:99999",
    ] {
        assert!(
            analysis::validate_reference_url(url).is_err(),
            "accepted {url:?}"
        );
    }
    let prefix = "https://example.invalid/";
    let exact = format!("{prefix}{}", "x".repeat(2048 - prefix.len()));
    assert!(analysis::validate_reference_url(&exact).is_ok());
    assert!(analysis::validate_reference_url(&(exact + "x")).is_err());
    assert!(analysis::validate_reference_url(&(prefix.to_string() + &"é".repeat(1024))).is_err());
}
#[test]
fn optional_url_serialization_preserves_legacy_manual_and_placeholder_preimages() {
    for old in [
        r#"{"id":"p1","reference":"A","title":"Legacy","folder":"","body":"Evidence","required":true,"account_ids":[],"adjustment_ids":[],"depends_on":[]}"#,
        r#"{"id":"p1","reference":"A","title":"Pending","folder":"","body":"","required":false,"account_ids":[],"adjustment_ids":[],"depends_on":[],"document_type":"placeholder"}"#,
    ] {
        let parsed: Workpaper = serde_json::from_str(old).unwrap();
        let encoded = serde_json::to_vec(&parsed).unwrap();
        assert_eq!(encoded, old.as_bytes());
        assert_eq!(Sha256::digest(&encoded), Sha256::digest(old.as_bytes()));
        assert!(parsed.reference_url.is_none());
    }
    for value in [Value::Null, json!(0), json!({}), json!([]), json!(true)] {
        let mut v = fixture();
        v["workpapers"][0]["reference_url"] = value;
        assert!(serde_json::from_value::<EngagementInput>(v).is_err());
    }
    for ty in ["manual", "placeholder"] {
        let mut v = fixture();
        v["workpapers"][0]["document_type"] = json!(ty);
        v["workpapers"][0]["body"] = json!("");
        assert!(analysis::validate(&input(v)).is_err());
    }
    let mut missing = fixture();
    missing["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("reference_url");
    assert!(analysis::validate(&input(missing)).is_err());
    assert_eq!(
        serde_json::to_value(WorkpaperType::ExternalReference).unwrap(),
        json!("external_reference")
    );
}
#[test]
fn native_note_pending_diagnostics_are_explicit_and_do_not_claim_retained_content() {
    let ready = analysis::analyze(&input(fixture()));
    assert_eq!(ready["supported"], true);
    assert!(codes(&ready).contains(&"external_content_unretained"));
    let notice = ready["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "external_content_unretained")
        .unwrap();
    assert_eq!(notice["blocking"], false);
    for required in [false, true] {
        let mut v = fixture();
        v["workpapers"][0]["body"] = json!(" \n");
        v["workpapers"][0]["required"] = json!(required);
        assert!(analysis::validate(&input(v.clone())).is_ok());
        let result = analysis::analyze(&input(v));
        let d = result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "missing_external_reference_note")
            .unwrap();
        assert_eq!(d["blocking"], required);
        assert!(!codes(&result).contains(&"missing_required_workpaper"));
    }
    let mut v = fixture();
    v["workpapers"][0]["annotations"] = json!([{"id":"a1","kind":"note","note":"Synthetic","operator_label":"Synthetic","body_sha256":hex::encode(Sha256::digest(b"Native review note only."))}]);
    assert!(
        analysis::validate(&input(v))
            .unwrap_err()
            .contains("External references")
    );
}
#[tokio::test]
async fn conversions_preserve_saved_notes_and_cannot_erase_saved_marks_or_urls() {
    let e = engine();
    let external = e
        .save(None, input(fixture()))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let before = e.store.get(STATE).await.unwrap().unwrap();
    for ty in [WorkpaperType::Manual, WorkpaperType::Placeholder] {
        let mut next = external.document.clone();
        next.expected_revision = Some(external.revision);
        next.workpapers[0].document_type = ty;
        next.workpapers[0].reference_url = None;
        if ty == WorkpaperType::Placeholder {
            next.workpapers[0].body.clear();
        }
        assert_eq!(e.save(Some(&external.id), next).await.unwrap_err().0, 409);
        assert_eq!(e.store.get(STATE).await.unwrap().unwrap(), before);
    }
    let mut manual = fixture();
    manual["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("reference_url");
    manual["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("document_type");
    manual["workpapers"][0]["annotations"] = json!([{"id":"a1","kind":"note","note":"Synthetic","operator_label":"Synthetic","body_sha256":hex::encode(Sha256::digest(b"Native review note only."))}]);
    let saved = e
        .save(None, input(manual))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let state = e.store.get(STATE).await.unwrap().unwrap();
    let mut next = saved.document.clone();
    next.expected_revision = Some(saved.revision);
    next.workpapers[0].document_type = WorkpaperType::ExternalReference;
    next.workpapers[0].reference_url = Some("https://example.invalid".into());
    next.workpapers[0].annotations.clear();
    assert_eq!(e.save(Some(&saved.id), next).await.unwrap_err().0, 409);
    assert_eq!(e.store.get(STATE).await.unwrap().unwrap(), state);
    let mut clear = saved.document.clone();
    clear.expected_revision = Some(saved.revision);
    clear.workpapers[0].annotations.clear();
    let clean = e
        .save(Some(&saved.id), clear)
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let mut changed = clean.document.clone();
    changed.expected_revision = Some(clean.revision);
    changed.workpapers[0].document_type = WorkpaperType::ExternalReference;
    changed.workpapers[0].reference_url = Some("https://example.invalid".into());
    changed.workpapers[0].body.clear();
    assert_eq!(e.save(Some(&clean.id), changed).await.unwrap_err().0, 409);
    let mut allowed = clean.document.clone();
    allowed.expected_revision = Some(clean.revision);
    allowed.workpapers[0].document_type = WorkpaperType::ExternalReference;
    allowed.workpapers[0].reference_url = Some("https://example.invalid".into());
    let converted = e
        .save(Some(&clean.id), allowed)
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    assert_eq!(
        converted.document.workpapers[0].body,
        clean.document.workpapers[0].body
    );
    assert_eq!(
        converted.document.workpapers[0].account_ids,
        clean.document.workpapers[0].account_ids
    );
    assert_eq!(
        converted.annotation_bindings.len(),
        clean.annotation_bindings.len()
    );
}
#[tokio::test]
async fn upload_or_file_history_cannot_attach_or_disappear_through_external_type() {
    let e = engine();
    let r = e
        .save(None, input(fixture()))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let before = e.store.get(STATE).await.unwrap().unwrap();
    let upload = |revision| AttachmentUpload {
        expected_revision: revision,
        upload_id: "synthetic_upload".into(),
        file_name: "synthetic.txt".into(),
        operator_label: "Synthetic".into(),
        note: "Synthetic only".into(),
        content_hex: hex::encode(b"Synthetic bytes"),
    };
    assert_eq!(
        e.upload_attachment(&r.id, "external", upload(r.revision))
            .await
            .unwrap_err()
            .0,
        409
    );
    assert_eq!(e.store.get(STATE).await.unwrap().unwrap(), before);
    let mut v = fixture();
    v["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("reference_url");
    v["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("document_type");
    let manual = e
        .save(None, input(v))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let saved = e
        .upload_attachment(&manual.id, "external", upload(manual.revision))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let before = e.store.get(STATE).await.unwrap().unwrap();
    let mut converted = saved.document.clone();
    converted.expected_revision = Some(saved.revision);
    converted.workpapers[0].document_type = WorkpaperType::ExternalReference;
    converted.workpapers[0].reference_url = Some("https://example.invalid".into());
    converted.workpapers[0].attachment_versions.clear();
    assert_eq!(e.save(Some(&saved.id), converted).await.unwrap_err().0, 409);
    assert_eq!(e.store.get(STATE).await.unwrap().unwrap(), before);
}
#[tokio::test]
async fn corrupt_current_snapshot_and_rollforward_origin_urls_fail_closed() {
    let e = engine();
    let r = e
        .save(None, input(fixture()))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r = e
        .calculate(
            &r.id,
            RevisionInput {
                expected_revision: r.revision,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let original = e.store.get(STATE).await.unwrap().unwrap();
    for location in ["current", "snapshot", "history"] {
        let mut corrupt: Value = serde_json::from_str(&original).unwrap();
        let doc = match location {
            "current" => &mut corrupt["engagements"][0]["document"],
            "snapshot" => &mut corrupt["engagements"][0]["snapshot"]["input_document"],
            _ => &mut corrupt["engagements"][0]["snapshots"][0]["input_document"],
        };
        doc["workpapers"][0]["reference_url"] = json!("javascript:alert(1)");
        e.store.set_json(STATE, &corrupt).await.unwrap();
        assert_eq!(e.get(&r.id).await.unwrap_err().0, 409);
        e.store.set(STATE, &original).await.unwrap();
    }
}
async fn raw(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (u16, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", "Bearer external-reference-test-key")
        .header("Content-Type", "application/json")
        .body(
            body.map(|v| Body::from(v.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status().as_u16();
    (
        status,
        axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}
async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    expected: u16,
) -> Value {
    let (status, bytes) = raw(app, method, path, body).await;
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)}));
    assert_eq!(status, expected, "{method} {path}: {value}");
    value
}
fn path(r: &Value, suffix: &str) -> String {
    format!("/api/engagements/{}{suffix}", r["id"].as_str().unwrap())
}
async fn revision(app: &axum::Router, r: &Value, suffix: &str) -> Value {
    call(
        app,
        "POST",
        &path(r, suffix),
        Some(json!({"expected_revision":r["revision"]})),
        200,
    )
    .await
}
async fn sign(app: &axum::Router, r: &Value, stage: &str) -> Value {
    call(app,"POST",&path(r,"/signoffs"),Some(json!({"expected_revision":r["revision"],"workpaper_id":"external","stage":stage,"operator":"Synthetic label","note":"Metadata only; no destination verification."})),200).await
}
async fn transition(app: &axum::Router, r: &Value, status: &str) -> Value {
    call(app,"POST",&path(r,"/transition"),Some(json!({"expected_revision":r["revision"],"status":status,"operator":"Synthetic label","note":"Synthetic internal transition."})),200).await
}
#[tokio::test]
async fn http_lifecycle_scope_print_package_review_invalidation_and_rollforward() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "external-reference-test-key");
    }
    let app = app_with(Store::memory);
    let r = call(&app, "POST", "/api/engagements", Some(fixture()), 200).await;
    let original_url = r["document"]["workpapers"][0]["reference_url"].clone();
    let r = revision(&app, &r, "/calculate").await;
    assert_eq!(r["snapshot"]["result"]["supported"], true);
    let snapshot = r["snapshot"].clone();
    let (status, html) = raw(&app, "GET", &path(&r, "/print"), None).await;
    assert_eq!(status, 200);
    let html = String::from_utf8(html).unwrap();
    assert!(
        html.contains(
            "target=\"_blank\" rel=\"noopener noreferrer\" referrerpolicy=\"no-referrer\""
        )
    );
    assert!(html.contains("not fetched, retained or verified"));
    assert!(html.contains("&amp;y=2"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
    let package = call(&app, "GET", &path(&r, "/package"), None, 200).await;
    assert_eq!(
        package["engagement"]["document"]["workpapers"][0]["reference_url"],
        original_url
    );
    let r = sign(&app, &r, "prepared").await;
    let mut edit = r["document"].clone();
    edit["expected_revision"] = r["revision"].clone();
    edit["workpapers"][0]["reference_url"] = json!("https://example.invalid/new");
    let changed = call(&app, "PUT", &path(&r, ""), Some(edit.clone()), 200).await;
    assert!(changed["snapshot"].is_null());
    assert_eq!(changed["signoffs"], json!([]));
    assert_eq!(changed["snapshots"][0], snapshot);
    assert_eq!(changed["signoff_history"].as_array().unwrap().len(), 1);
    call(&app, "PUT", &path(&changed, ""), Some(edit), 409).await;
    let r = revision(&app, &changed, "/calculate").await;
    let r = sign(&app, &r, "prepared").await;
    let r = transition(&app, &r, "in_review").await;
    let r = sign(&app, &r, "reviewed").await;
    let r = transition(&app, &r, "locked").await;
    let locked_snapshot = r["snapshot"].clone();
    let mut forbidden = r["document"].clone();
    forbidden["expected_revision"] = r["revision"].clone();
    forbidden["workpapers"][0]["body"] = json!("Changed after lock");
    call(&app, "PUT", &path(&r, ""), Some(forbidden), 409).await;
    let next=call(&app,"POST",&path(&r,"/rollforward"),Some(json!({"expected_revision":r["revision"],"period_start":"2026-01-01","period_end":"2026-12-31"})),200).await;
    let paper = &next["document"]["workpapers"][0];
    assert_eq!(paper["document_type"], "external_reference");
    assert_eq!(
        paper["reference_url"],
        r["document"]["workpapers"][0]["reference_url"]
    );
    assert_eq!(paper["body"], "");
    assert_eq!(paper["account_ids"], json!(["cash"]));
    assert_eq!(next["signoffs"], json!([]));
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"],
        locked_snapshot
    );
    assert!(
        next["rollforward_origin"]["copy_policy"]["preserved_fields"]
            .as_array()
            .unwrap()
            .contains(&json!("workpaper_reference_urls_unverified"))
    );
    let calculated = revision(&app, &next, "/calculate").await;
    assert_eq!(calculated["snapshot"]["result"]["supported"], false);
    let before = call(&app, "GET", "/api/engagements/audit", None, 200).await;
    call(&app,"POST",&path(&calculated,"/signoffs"),Some(json!({"expected_revision":calculated["revision"],"workpaper_id":"external","stage":"prepared","operator":"Synthetic","note":"Synthetic"})),409).await;
    assert_eq!(
        call(&app, "GET", "/api/engagements/audit", None, 200).await,
        before
    );
}

#[tokio::test]
async fn corrupt_origin_and_optional_empty_note_checkpoint_fail_closed_without_state_changes() {
    let e = engine();
    let mut v = fixture();
    v["workpapers"][0]["required"] = json!(false);
    v["workpapers"][0]["body"] = json!("");
    let empty = e
        .save(None, input(v))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let empty = e
        .calculate(
            &empty.id,
            RevisionInput {
                expected_revision: empty.revision,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let before = e.store.get(STATE).await.unwrap().unwrap();
    assert_eq!(
        e.signoff(
            &empty.id,
            SignoffInput {
                expected_revision: empty.revision,
                workpaper_id: "external".into(),
                stage: "prepared".into(),
                operator: "Synthetic".into(),
                note: "Synthetic".into()
            }
        )
        .await
        .unwrap_err()
        .0,
        409
    );
    assert_eq!(e.store.get(STATE).await.unwrap().unwrap(), before);
    let r = e
        .save(None, input(fixture()))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r = e
        .calculate(
            &r.id,
            RevisionInput {
                expected_revision: r.revision,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r = e
        .signoff(
            &r.id,
            SignoffInput {
                expected_revision: r.revision,
                workpaper_id: "external".into(),
                stage: "prepared".into(),
                operator: "Synthetic".into(),
                note: "Synthetic".into(),
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r=e.transition(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"status":"in_review","operator":"Synthetic","note":"Synthetic"})).unwrap()).await.unwrap_or_else(|e|panic!("{}",e.1));
    let r = e
        .signoff(
            &r.id,
            SignoffInput {
                expected_revision: r.revision,
                workpaper_id: "external".into(),
                stage: "reviewed".into(),
                operator: "Synthetic".into(),
                note: "Synthetic".into(),
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r=e.transition(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"status":"locked","operator":"Synthetic","note":"Synthetic"})).unwrap()).await.unwrap_or_else(|e|panic!("{}",e.1));
    let target=e.rollforward(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"period_start":"2026-01-01","period_end":"2026-12-31"})).unwrap()).await.unwrap_or_else(|e|panic!("{}",e.1));
    let original = e.store.get(STATE).await.unwrap().unwrap();
    let mut corrupt: Value = serde_json::from_str(&original).unwrap();
    let row = corrupt["engagements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == target.id)
        .unwrap();
    row["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["reference_url"] =
        json!("https://@example.invalid");
    e.store.set_json(STATE, &corrupt).await.unwrap();
    assert_eq!(e.get(&target.id).await.unwrap_err().0, 409);
    e.store.set(STATE, &original).await.unwrap();
    assert_eq!(
        e.get(&target.id)
            .await
            .unwrap_or_else(|e| panic!("{}", e.1))
            .revision,
        target.revision
    );
}
