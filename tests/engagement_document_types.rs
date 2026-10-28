//! Original NXT planning/evidence controls using synthetic data and memory only.
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

fn fixture() -> Value {
    json!({"name":"Synthetic typed workpapers","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","currency":"CAD","accounts":[{"id":"cash","code":"100","name":"Synthetic cash","kind":"asset","group_id":"assets","debit_cents":10000},{"id":"capital","code":"300","name":"Synthetic capital","kind":"equity","group_id":"equity","credit_cents":10000}],"groups":[{"id":"assets","code":"A","name":"Synthetic assets","kind":"asset"},{"id":"equity","code":"E","name":"Synthetic equity","kind":"equity"}],"adjustments":[{"id":"aje1","reference":"AJ1","date":"2025-12-31","lines":[{"account_id":"cash","debit_cents":5},{"account_id":"capital","credit_cents":5}]}],"workpapers":[{"id":"paper1","reference":"A1","title":"Synthetic manual support","body":"Synthetic evidence only.","required":true,"account_ids":["cash"],"adjustment_ids":["aje1"]},{"id":"plan1","reference":"P1","title":"Pending <script>unsafe</script>","document_type":"placeholder","required":false,"account_ids":["cash"],"adjustment_ids":["aje1"],"depends_on":["paper1"]}],"issues":[{"id":"issue1","title":"Synthetic planning follow-up","workpaper_id":"plan1","blocking":false}]})
}
fn input(v: Value) -> EngagementInput {
    serde_json::from_value(v).unwrap()
}
fn diagnostic(output: &Value, code: &str) -> Value {
    output["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == code)
        .unwrap()
        .clone()
}
#[test]
fn legacy_manual_preimages_and_strict_types_are_preserved() {
    let old = r#"{"id":"p1","reference":"A","title":"Legacy","folder":"","body":"Evidence","required":true,"account_ids":[],"adjustment_ids":[],"depends_on":[]}"#;
    let paper: Workpaper = serde_json::from_str(old).unwrap();
    assert_eq!(paper.document_type, WorkpaperType::Manual);
    assert_eq!(serde_json::to_string(&paper).unwrap(), old);
    let full = format!(
        r#"{{"name":"Legacy","client_ref":"","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","prior_period_start":null,"prior_period_end":null,"currency":"CAD","accounts":[],"groups":[],"adjustments":[],"workpapers":[{old}],"issues":[],"notes":"","expected_revision":null}}"#
    );
    let document: EngagementInput = serde_json::from_str(&full).unwrap();
    let raw = serde_json::to_vec(&document).unwrap();
    assert_eq!(raw, full.as_bytes());
    assert_eq!(Sha256::digest(&raw), Sha256::digest(full.as_bytes()));
    let mut explicit: Value = serde_json::from_str(&full).unwrap();
    explicit["workpapers"][0]["document_type"] = json!("manual");
    assert_eq!(serde_json::to_vec(&input(explicit)).unwrap(), raw);
    for invalid in [
        Value::Null,
        json!("unknown"),
        json!("Manual"),
        json!(""),
        json!(0),
        json!({}),
        json!({"manual":null}),
        json!({"placeholder":null}),
        json!([]),
        json!(1),
        json!(true),
    ] {
        let mut value = fixture();
        value["workpapers"][1]["document_type"] = invalid;
        assert!(serde_json::from_value::<EngagementInput>(value).is_err());
    }
    assert_eq!(
        serde_json::to_value(WorkpaperType::Placeholder).unwrap(),
        json!("placeholder")
    );
}
#[test]
fn pending_diagnostics_and_evidence_validation_are_type_specific() {
    let optional = input(fixture());
    assert!(analysis::validate(&optional).is_ok());
    let result = analysis::analyze(&optional);
    assert_eq!(result["supported"], true);
    assert_eq!(
        diagnostic(&result, "optional_placeholder_pending")["blocking"],
        false
    );
    let mut required = fixture();
    required["workpapers"][1]["required"] = json!(true);
    let result = analysis::analyze(&input(required));
    assert_eq!(result["supported"], false);
    assert_eq!(
        diagnostic(&result, "required_placeholder_workpaper")["blocking"],
        true
    );
    assert!(
        !result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "missing_required_workpaper")
    );
    for body in [" ", "\n", "evidence"] {
        let mut v = fixture();
        v["workpapers"][1]["body"] = json!(body);
        assert!(
            analysis::validate(&input(v))
                .unwrap_err()
                .contains("Placeholder")
        );
    }
    let mut mark = fixture();
    mark["workpapers"][1]["annotations"] = json!([{"id":"n1","kind":"note","note":"Synthetic","operator_label":"Synthetic","body_sha256":hex::encode(Sha256::digest(b""))}]);
    assert!(
        analysis::validate(&input(mark))
            .unwrap_err()
            .contains("Placeholder")
    );
    let mut manual = fixture();
    manual["workpapers"][0]["body"] = json!("");
    assert_eq!(
        diagnostic(
            &analysis::analyze(&input(manual)),
            "missing_required_workpaper"
        )["blocking"],
        true
    );
}

fn engine() -> Engagements {
    let ledger = Arc::new(nxt_os::ledger::engine::Ledger::new(Store::memory()));
    let tax = Arc::new(nxt_os::tax::Tax::new(Store::memory()));
    let firm = Arc::new(nxt_os::firm::Firm::new(Store::memory(), ledger.clone()));
    Engagements::new(Store::memory(), ledger, tax, firm)
}
#[tokio::test]
async fn rejected_placeholder_operations_leave_exact_state_and_retained_shapes_unchanged() {
    let e = engine();
    let record = e
        .save(None, input(fixture()))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let before = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    let rejected = e
        .upload_attachment(
            &record.id,
            "plan1",
            AttachmentUpload {
                expected_revision: record.revision,
                upload_id: "placeholder_upload".into(),
                file_name: "synthetic.txt".into(),
                operator_label: "Synthetic".into(),
                note: "Synthetic only".into(),
                content_hex: hex::encode(b"Synthetic bytes only"),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(rejected.0, 409);
    assert!(rejected.1.contains("placeholder"));
    assert_eq!(
        e.store.get("engagements:state:v1").await.unwrap().unwrap(),
        before
    );
    let rejected = e
        .signoff(
            &record.id,
            SignoffInput {
                expected_revision: record.revision,
                workpaper_id: "plan1".into(),
                stage: "prepared".into(),
                operator: "Synthetic".into(),
                note: "Synthetic only".into(),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(rejected.0, 409);
    assert!(rejected.1.contains("Placeholder"));
    assert_eq!(
        e.store.get("engagements:state:v1").await.unwrap().unwrap(),
        before
    );
    let record = e
        .calculate(
            &record.id,
            RevisionInput {
                expected_revision: record.revision,
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let retained = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    let mut corrupt: Value = serde_json::from_str(&retained).unwrap();
    corrupt["engagements"][0]["snapshots"][0]["input_document"]["workpapers"][1]["body"] =
        json!("Forbidden evidence in retained placeholder");
    e.store
        .set_json("engagements:state:v1", &corrupt)
        .await
        .unwrap();
    assert_eq!(e.get(&record.id).await.unwrap_err().0, 409);
    e.store
        .set("engagements:state:v1", &retained)
        .await
        .unwrap();
    assert_eq!(
        e.get(&record.id)
            .await
            .unwrap_or_else(|e| panic!("{}", e.1))
            .revision,
        record.revision
    );
    // A genuine prior file version cannot be converted/deleted by a save.
    let uploaded = e
        .upload_attachment(
            &record.id,
            "paper1",
            AttachmentUpload {
                expected_revision: record.revision,
                upload_id: "manual_upload".into(),
                file_name: "synthetic.txt".into(),
                operator_label: "Synthetic".into(),
                note: "Synthetic only".into(),
                content_hex: hex::encode(b"Synthetic bytes only"),
            },
        )
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let state = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    let mut forged = uploaded.document.clone();
    forged.expected_revision = Some(uploaded.revision);
    forged.workpapers[0].document_type = WorkpaperType::Placeholder;
    forged.workpapers[0].body.clear();
    forged.workpapers[0].attachment_versions.clear();
    assert_eq!(e.save(Some(&uploaded.id), forged).await.unwrap_err().0, 409);
    assert_eq!(
        e.store.get("engagements:state:v1").await.unwrap().unwrap(),
        state
    );
    let mut invalid = uploaded.document.clone();
    invalid.workpapers[0].document_type = WorkpaperType::Placeholder;
    invalid.workpapers[0].body.clear();
    assert!(
        analysis::validate(&invalid)
            .unwrap_err()
            .contains("Placeholder")
    );
}

async fn raw(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (u16, Vec<u8>) {
    let builder = Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", "Bearer document-types-test-key")
        .header("Content-Type", "application/json");
    let req = builder
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
fn endpoint(r: &Value, suffix: &str) -> String {
    format!("/api/engagements/{}{suffix}", r["id"].as_str().unwrap())
}
fn edit(r: &Value) -> Value {
    let mut d = r["document"].clone();
    d["expected_revision"] = r["revision"].clone();
    d
}
async fn calculate(app: &axum::Router, r: &Value) -> Value {
    call(
        app,
        "POST",
        &endpoint(r, "/calculate"),
        Some(json!({"expected_revision":r["revision"]})),
        200,
    )
    .await
}
async fn stage(app: &axum::Router, r: &Value, status: &str) -> Value {
    call(app,"POST",&endpoint(r,"/transition"),Some(json!({"expected_revision":r["revision"],"status":status,"operator":"Synthetic reviewer","note":"Synthetic only"})),200).await
}
async fn sign(app: &axum::Router, r: &Value, stage: &str) -> Value {
    call(app,"POST",&endpoint(r,"/signoffs"),Some(json!({"expected_revision":r["revision"],"workpaper_id":"paper1","stage":stage,"operator":if stage=="prepared"{"Synthetic preparer"}else{"Synthetic reviewer"},"note":"Synthetic only"})),200).await
}
async fn lock(app: &axum::Router, r: &Value) -> Value {
    let r = sign(app, r, "prepared").await;
    let r = stage(app, &r, "in_review").await;
    let r = sign(app, &r, "reviewed").await;
    stage(app, &r, "locked").await
}
fn assert_planning_refs(r: &Value) {
    let p = &r["document"]["workpapers"][1];
    assert_eq!(p["id"], "plan1");
    assert_eq!(p["reference"], "P1");
    assert_eq!(p["account_ids"], json!(["cash"]));
    assert_eq!(p["adjustment_ids"], json!(["aje1"]));
    assert_eq!(p["depends_on"], json!(["paper1"]));
    assert_eq!(r["document"]["issues"][0]["workpaper_id"], "plan1");
}

#[tokio::test]
async fn api_conversion_review_lock_reopen_and_rollforward_preserve_planning_identity() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "document-types-test-key");
    }
    let app = app_with(Store::memory);
    let catalog = call(&app, "GET", "/api/engagements/catalog", None, 200).await;
    assert_eq!(
        catalog["workpaper_document_types"]["manual"]["default"],
        true
    );
    assert_eq!(
        catalog["workpaper_document_types"]["placeholder"]["signoffs"],
        "prohibited"
    );
    let r = call(&app, "POST", "/api/engagements", Some(fixture()), 200).await;
    for bad in [Value::Null, json!("file"), json!("Manual")] {
        let mut v = fixture();
        v["workpapers"][1]["document_type"] = bad;
        let (status, _) = raw(&app, "POST", "/api/engagements", Some(v)).await;
        assert!(status == 400 || status == 422);
    }
    assert_eq!(
        call(&app, "GET", "/api/engagements", None, 200)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let r = calculate(&app, &r).await;
    assert_eq!(r["snapshot"]["result"]["supported"], true);
    let before = call(&app, "GET", "/api/engagements/audit", None, 200).await;
    let rejected=call(&app,"POST",&endpoint(&r,"/signoffs"),Some(json!({"expected_revision":r["revision"],"workpaper_id":"plan1","stage":"prepared","operator":"Synthetic","note":"Synthetic"})),409).await;
    assert!(rejected.to_string().contains("Placeholder"));
    call(&app,"POST",&endpoint(&r,"/workpapers/plan1/attachments"),Some(json!({"expected_revision":r["revision"],"upload_id":"no_placeholder_upload","file_name":"synthetic.txt","operator_label":"Synthetic","note":"Synthetic only","content_hex":"61"})),409).await;
    assert_eq!(call(&app, "GET", &endpoint(&r, ""), None, 200).await, r);
    assert_eq!(
        call(&app, "GET", "/api/engagements/audit", None, 200).await,
        before
    );
    let package = call(&app, "GET", &endpoint(&r, "/package"), None, 200).await;
    assert_eq!(
        package["engagement"]["document"]["workpapers"][1]["document_type"],
        "placeholder"
    );
    assert_eq!(
        package["engagement"]["snapshot"]["input_document"]["workpapers"][1]["document_type"],
        "placeholder"
    );
    let (status, html) = raw(&app, "GET", &endpoint(&r, "/print"), None).await;
    assert_eq!(status, 200);
    let html = String::from_utf8(html).unwrap();
    assert!(html.contains("Placeholder · pending planning material, not evidence"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
    let snapshot = r["snapshot"].clone();
    let mut d = edit(&r);
    d["workpapers"][1]["document_type"] = json!("manual");
    d["workpapers"][1]["body"] = json!("New synthetic manual evidence.");
    let changed = call(&app, "PUT", &endpoint(&r, ""), Some(d.clone()), 200).await;
    assert_planning_refs(&changed);
    assert!(changed["snapshot"].is_null());
    assert_eq!(changed["snapshots"][0], snapshot);
    assert!(
        changed["document"]["workpapers"][1]
            .get("document_type")
            .is_none()
    );
    call(&app, "PUT", &endpoint(&r, ""), Some(d), 409).await;
    let r = calculate(&app, &changed).await;
    let r = sign(&app, &r, "prepared").await;
    let r = stage(&app, &r, "in_review").await;
    let r = sign(&app, &r, "reviewed").await;
    let mut illegal = edit(&r);
    illegal["workpapers"][1]["document_type"] = json!("placeholder");
    illegal["workpapers"][1]["body"] = json!("");
    call(&app, "PUT", &endpoint(&r, ""), Some(illegal), 409).await;
    assert_eq!(call(&app, "GET", &endpoint(&r, ""), None, 200).await, r);
    let mut clear = edit(&r);
    clear["workpapers"][1]["body"] = json!("");
    let cleared = call(&app, "PUT", &endpoint(&r, ""), Some(clear), 200).await;
    assert_eq!(cleared["status"], "draft");
    assert!(cleared["snapshot"].is_null());
    assert_eq!(cleared["signoffs"], json!([]));
    assert_eq!(cleared["signoff_history"].as_array().unwrap().len(), 2);
    let mut convert = edit(&cleared);
    convert["workpapers"][1]["document_type"] = json!("placeholder");
    let r = call(&app, "PUT", &endpoint(&cleared, ""), Some(convert), 200).await;
    assert_planning_refs(&r);
    let r = calculate(&app, &r).await;
    let locked = lock(&app, &r).await;
    call(
        &app,
        "PUT",
        &endpoint(&locked, ""),
        Some(edit(&locked)),
        409,
    )
    .await;
    let next=call(&app,"POST",&endpoint(&locked,"/rollforward"),Some(json!({"expected_revision":locked["revision"],"name":"Synthetic next period","period_start":"2026-01-01","period_end":"2026-12-31"})),200).await;
    assert_eq!(
        next["document"]["workpapers"][1]["document_type"],
        "placeholder"
    );
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"],
        locked["snapshot"]
    );
    assert!(
        next["rollforward_origin"]["copy_policy"]["preserved_fields"]
            .as_array()
            .unwrap()
            .contains(&json!("workpaper_document_types"))
    );
    for paper in next["document"]["workpapers"].as_array().unwrap() {
        assert_eq!(paper["body"], "");
        assert!(paper.get("annotations").is_none());
        assert!(paper.get("attachment_versions").is_none());
    }
    assert_eq!(
        call(&app, "GET", &endpoint(&locked, ""), None, 200).await,
        locked
    );
    let reopened = stage(&app, &locked, "draft").await;
    let mut d = edit(&reopened);
    d["workpapers"][1]["document_type"] = json!("manual");
    d["workpapers"][1]["body"] = json!("Synthetic reopened evidence");
    let reopened = call(&app, "PUT", &endpoint(&reopened, ""), Some(d), 200).await;
    assert_planning_refs(&reopened);
    let mut required = fixture();
    required["workpapers"][1]["required"] = json!(true);
    let required = call(&app, "POST", "/api/engagements", Some(required), 200).await;
    let required = calculate(&app, &required).await;
    assert_eq!(required["snapshot"]["result"]["supported"], false);
    assert_eq!(
        diagnostic(
            &required["snapshot"]["result"],
            "required_placeholder_workpaper"
        )["blocking"],
        true
    );
    call(&app,"POST",&endpoint(&required,"/transition"),Some(json!({"expected_revision":required["revision"],"status":"in_review","operator":"Synthetic","note":"Synthetic"})),409).await;
    assert_eq!(
        call(&app, "GET", "/api/ledger/entries", None, 200).await,
        json!([])
    );
}

#[tokio::test]
async fn optional_placeholder_dependencies_still_require_each_checkpoint_stage() {
    let e = engine();
    let mut source = fixture();
    source["workpapers"][0]["depends_on"] = json!(["plan1"]);
    source["workpapers"][1]["depends_on"] = json!([]);
    let r = e
        .save(None, input(source))
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
    assert_eq!(r.snapshot.as_ref().unwrap().result["supported"], true);
    let request = |revision, paper: &str, stage: &str| SignoffInput {
        expected_revision: revision,
        workpaper_id: paper.into(),
        stage: stage.into(),
        operator: if stage == "prepared" {
            "Synthetic preparer".into()
        } else {
            "Synthetic reviewer".into()
        },
        note: "Synthetic dependency QA only".into(),
    };
    let rejected = e
        .signoff(&r.id, request(r.revision, "paper1", "prepared"))
        .await
        .unwrap_err();
    assert_eq!(rejected.0, 409);
    assert!(rejected.1.contains("dependent"));
    let rejected = e
        .signoff(&r.id, request(r.revision, "plan1", "prepared"))
        .await
        .unwrap_err();
    assert_eq!(rejected.0, 409);
    assert!(rejected.1.contains("Placeholder"));
    let mut converted = r.document.clone();
    converted.expected_revision = Some(r.revision);
    converted.workpapers[1].document_type = WorkpaperType::Manual;
    converted.workpapers[1].body = "Synthetic dependency evidence".into();
    let r = e
        .save(Some(&r.id), converted)
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
    assert_eq!(
        e.signoff(&r.id, request(r.revision, "paper1", "prepared"))
            .await
            .unwrap_err()
            .0,
        409
    );
    let r = e
        .signoff(&r.id, request(r.revision, "plan1", "prepared"))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r = e
        .signoff(&r.id, request(r.revision, "paper1", "prepared"))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let transition=serde_json::from_value(json!({"expected_revision":r.revision,"status":"in_review","operator":"Synthetic reviewer","note":"Synthetic dependency QA only"})).unwrap();
    let r = e
        .transition(&r.id, transition)
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let rejected = e
        .signoff(&r.id, request(r.revision, "paper1", "reviewed"))
        .await
        .unwrap_err();
    assert_eq!(rejected.0, 409);
    assert!(rejected.1.contains("dependent"));
    let r = e
        .signoff(&r.id, request(r.revision, "plan1", "reviewed"))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let r = e
        .signoff(&r.id, request(r.revision, "paper1", "reviewed"))
        .await
        .unwrap_or_else(|e| panic!("{}", e.1));
    let transition=serde_json::from_value(json!({"expected_revision":r.revision,"status":"locked","operator":"Synthetic reviewer","note":"Synthetic dependency QA only"})).unwrap();
    assert_eq!(
        e.transition(&r.id, transition)
            .await
            .unwrap_or_else(|e| panic!("{}", e.1))
            .status,
        "locked"
    );
}
