//! Synthetic original NXT text-tickmark acceptance; no source product/data/network.
use axum::{body::Body, http::Request};
use nxt_os::{
    app_with,
    engagement::{analysis, models::EngagementInput},
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
fn body_hash(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}
fn definition() -> Value {
    json!({"id":"mark_review","code":"QA","label":"Original <b>label</b>","description":"Original <script>alert(1)</script> & meaning"})
}
fn fixture() -> Value {
    json!({"name":"Synthetic custom text marks","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","currency":"CAD","accounts":[{"id":"cash","code":"100","name":"Synthetic cash","kind":"asset","group_id":"assets","debit_cents":10000},{"id":"capital","code":"300","name":"Synthetic capital","kind":"equity","group_id":"equity","credit_cents":10000}],"groups":[{"id":"assets","code":"A","name":"Synthetic assets","kind":"asset"},{"id":"equity","code":"E","name":"Synthetic equity","kind":"equity"}],"workpapers":[{"id":"paper1","reference":"A1","title":"Synthetic support","body":"Synthetic evidence text.","required":true,"account_ids":["cash"]}]})
}
fn custom_fixture() -> Value {
    let mut v = fixture();
    let d = definition();
    v["tickmark_catalogue"] = json!([d.clone()]);
    v["workpapers"][0]["annotations"] = json!([{"id":"custom1","kind":"custom","note":"Saved custom meaning.","operator_label":"Synthetic preparer","body_sha256":body_hash("Synthetic evidence text."),"custom_mark":d},{"id":"builtin1","kind":"note","note":"Existing builtin note.","operator_label":"Synthetic preparer","body_sha256":body_hash("Synthetic evidence text.")}]);
    v
}
fn valid(v: Value) -> bool {
    let input: EngagementInput = serde_json::from_value(v).unwrap();
    analysis::validate(&input).is_ok()
}

#[test]
fn custom_catalogue_boundaries_and_invalid_snapshots_are_checked() {
    assert!(valid(custom_fixture()));
    let mut boundary = custom_fixture();
    boundary["tickmark_catalogue"][0]["label"] = json!("é".repeat(64));
    boundary["tickmark_catalogue"][0]["description"] = json!("é".repeat(512));
    assert!(valid(boundary));
    let mut fifty = fixture();
    fifty["tickmark_catalogue"]=json!((0..50).map(|n|json!({"id":format!("mark_{n}"),"code":format!("Q{n}"),"label":"Synthetic","description":"Synthetic only"})).collect::<Vec<_>>());
    assert!(valid(fifty.clone()));
    fifty["tickmark_catalogue"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"mark50","code":"Q50","label":"Synthetic","description":"Synthetic"}));
    assert!(!valid(fifty));
    for code in [
        "",
        "ABCDEFGHIJKLM",
        "lower",
        "QA!",
        "Q A",
        "NOTE",
        "AG",
        "RC",
        "TR",
        "IN",
        "CF",
        "ＮＸＴ",
    ] {
        let mut v = fixture();
        let mut d = definition();
        d["code"] = json!(code);
        v["tickmark_catalogue"] = json!([d]);
        assert!(!valid(v), "{code}");
    }
    let mut twelve = fixture();
    let mut d = definition();
    d["code"] = json!("ABCDEFGHIJKL");
    twelve["tickmark_catalogue"] = json!([d]);
    assert!(valid(twelve));
    for (key, value) in [
        ("id", "bad.id".to_string()),
        ("id", "x".repeat(65)),
        ("label", "".into()),
        ("label", " ".into()),
        ("label", "é".repeat(65)),
        ("description", "".into()),
        ("description", "x".repeat(1025)),
        ("label", "line\nnext".into()),
        ("description", "line\tnext".into()),
        ("description", "line\u{2028}next".into()),
        ("label", "line\u{2029}next".into()),
    ] {
        let mut v = fixture();
        let mut d = definition();
        d[key] = json!(value);
        v["tickmark_catalogue"] = json!([d]);
        assert!(!valid(v), "{key}");
    }
    let mut duplicate = fixture();
    duplicate["tickmark_catalogue"] = json!([definition(), definition()]);
    assert!(!valid(duplicate.clone()));
    duplicate["tickmark_catalogue"][1]["id"] = json!("other_id");
    assert!(!valid(duplicate.clone()));
    duplicate["tickmark_catalogue"][1]["code"] = json!("OTHER");
    duplicate["tickmark_catalogue"][1]["id"] = json!("mark_review");
    assert!(!valid(duplicate));
    let mut no_copy = custom_fixture();
    no_copy["workpapers"][0]["annotations"][0]
        .as_object_mut()
        .unwrap()
        .remove("custom_mark");
    assert!(!valid(no_copy));
    let mut builtin_copy = custom_fixture();
    builtin_copy["workpapers"][0]["annotations"][0]["kind"] = json!("note");
    assert!(!valid(builtin_copy));
    let mut invalid_copy = custom_fixture();
    invalid_copy["workpapers"][0]["annotations"][0]["custom_mark"]["code"] = json!("AG");
    assert!(!valid(invalid_copy));
    let mut retired = custom_fixture();
    retired["tickmark_catalogue"] = json!([]);
    assert!(
        valid(retired),
        "Copied meaning has no live catalogue dependency"
    );
}
async fn raw(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (u16, Vec<u8>) {
    let r = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", "Bearer custom-tickmark-test-key");
    let r = match body {
        Some(v) => r
            .header("content-type", "application/json")
            .body(Body::from(v.to_string())),
        None => r.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(r).await.unwrap();
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    (status, bytes.to_vec())
}
async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    expected: u16,
) -> Value {
    let (s, b) = raw(app, method, path, body).await;
    let v =
        serde_json::from_slice(&b).unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&b)}));
    assert_eq!(s, expected, "{method} {path}: {v}");
    v
}
fn endpoint(v: &Value, suffix: &str) -> String {
    format!("/api/engagements/{}{suffix}", v["id"].as_str().unwrap())
}
fn edit(v: &Value) -> Value {
    let mut d = v["document"].clone();
    d["expected_revision"] = v["revision"].clone();
    d
}
async fn calculate(app: &axum::Router, v: &Value) -> Value {
    call(
        app,
        "POST",
        &endpoint(v, "/calculate"),
        Some(json!({"expected_revision":v["revision"]})),
        200,
    )
    .await
}
async fn stage(app: &axum::Router, v: &Value, status: &str) -> Value {
    call(app,"POST",&endpoint(v,"/transition"),Some(json!({"expected_revision":v["revision"],"status":status,"operator":"Synthetic reviewer","note":"Synthetic internal review only."})),200).await
}
async fn checkpoint(app: &axum::Router, v: &Value, stage: &str) -> Value {
    call(app,"POST",&endpoint(v,"/signoffs"),Some(json!({"expected_revision":v["revision"],"workpaper_id":"paper1","stage":stage,"operator":if stage=="prepared"{"Synthetic preparer"}else{"Synthetic reviewer"},"note":"Synthetic only, not authenticated identity."})),200).await
}
async fn lock(app: &axum::Router, v: &Value) -> Value {
    let v = checkpoint(app, v, "prepared").await;
    let v = stage(app, &v, "in_review").await;
    let v = checkpoint(app, &v, "reviewed").await;
    stage(app, &v, "locked").await
}
fn bundled_json(bytes: &[u8]) -> Value {
    let mut at = 0;
    while bytes.get(at..at + 4) == Some(b"PK\x03\x04") {
        let u16at = |n| u16::from_le_bytes(bytes[n..n + 2].try_into().unwrap()) as usize;
        let u32at = |n| u32::from_le_bytes(bytes[n..n + 4].try_into().unwrap()) as usize;
        assert_eq!(u16at(at + 8), 0);
        let len = u32at(at + 18);
        let name_len = u16at(at + 26);
        let extra = u16at(at + 28);
        let name = &bytes[at + 30..at + 30 + name_len];
        let start = at + 30 + name_len + extra;
        if name == b"engagement.json" {
            return serde_json::from_slice(&bytes[start..start + len]).unwrap();
        }
        at = start + len;
    }
    panic!("Original NXT bundle must retain engagement.json")
}
#[tokio::test]
async fn custom_meanings_stay_frozen_across_catalogue_changes_and_rollforward() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "custom-tickmark-test-key") };
    let app = app_with(Store::memory);
    let catalog = call(&app, "GET", "/api/engagements/catalog", None, 200).await;
    assert!(
        catalog["annotation_kinds"]
            .as_array()
            .unwrap()
            .contains(&json!("custom"))
    );
    assert_eq!(
        catalog["custom_tickmark_catalogue"]["maximum_definitions"],
        50
    );
    assert!(
        catalog["annotation_scope"]
            .as_str()
            .unwrap()
            .contains("unverified")
    );
    let original = definition();
    let created = call(
        &app,
        "POST",
        "/api/engagements",
        Some(custom_fixture()),
        200,
    )
    .await;
    let created=call(&app,"POST",&endpoint(&created,"/workpapers/paper1/attachments"),Some(json!({"expected_revision":created["revision"],"upload_id":"synthetic_tickmark_bundle","file_name":"synthetic.txt","operator_label":"Synthetic preparer","note":"Tiny original NXT fixture for the ZIP package check only.","content_hex":hex::encode(b"Synthetic test bytes only.")})),200).await;
    let calculated = calculate(&app, &created).await;
    assert_eq!(calculated["snapshot"]["result"]["supported"], true);
    let old_snapshot = calculated["snapshot"].clone();
    let mut current = original.clone();
    current["code"] = json!("NEW");
    current["label"] = json!("Current <i>label</i>");
    current["description"] = json!("Current meaning");
    let mut change = edit(&calculated);
    change["tickmark_catalogue"] = json!([current.clone()]);
    let changed = call(
        &app,
        "PUT",
        &endpoint(&calculated, ""),
        Some(change.clone()),
        200,
    )
    .await;
    assert_eq!(
        changed["document"]["workpapers"][0]["annotations"][0]["custom_mark"],
        original
    );
    assert_eq!(changed["snapshots"][0], old_snapshot);
    call(&app, "PUT", &endpoint(&changed, ""), Some(change), 409).await;
    let mut redefine = edit(&changed);
    redefine["workpapers"][0]["annotations"][0]["custom_mark"] = current.clone();
    call(&app, "PUT", &endpoint(&changed, ""), Some(redefine), 409).await;
    let mut reinterpret = edit(&changed);
    reinterpret["workpapers"][0]["annotations"][0]["kind"] = json!("note");
    reinterpret["workpapers"][0]["annotations"][0]
        .as_object_mut()
        .unwrap()
        .remove("custom_mark");
    call(&app, "PUT", &endpoint(&changed, ""), Some(reinterpret), 409).await;
    let mut reinterpret = edit(&changed);
    reinterpret["workpapers"][0]["annotations"][1]["kind"] = json!("custom");
    reinterpret["workpapers"][0]["annotations"][1]["custom_mark"] = current.clone();
    call(&app, "PUT", &endpoint(&changed, ""), Some(reinterpret), 409).await;
    let mut remove = edit(&changed);
    remove["tickmark_catalogue"] = json!([]);
    let removed = call(&app, "PUT", &endpoint(&changed, ""), Some(remove), 200).await;
    let removed = calculate(&app, &removed).await;
    assert_eq!(removed["snapshot"]["result"]["supported"], true);
    assert_eq!(
        removed["snapshot"]["input_document"]["workpapers"][0]["annotations"][0]["custom_mark"],
        original
    );
    let (status, html) = raw(&app, "GET", &endpoint(&removed, "/print"), None).await;
    assert_eq!(status, 200);
    let html = String::from_utf8(html).unwrap();
    assert!(
        html.contains("Original &lt;b&gt;label&lt;/b&gt;")
            && html.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; meaning")
    );
    assert!(!html.contains("<script>alert(1)</script>"));
    let package = call(&app, "GET", &endpoint(&removed, "/package"), None, 200).await;
    assert_eq!(
        package["engagement"]["document"]["workpapers"][0]["annotations"][0]["custom_mark"],
        original
    );
    let mut stale = edit(&removed);
    stale["workpapers"][0]["body"] = json!("Changed synthetic text.");
    let stale = call(&app, "PUT", &endpoint(&removed, ""), Some(stale), 200).await;
    let stale = calculate(&app, &stale).await;
    assert_eq!(stale["snapshot"]["result"]["supported"], false);
    assert!(
        stale["snapshot"]["result"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "stale_workpaper_annotation")
    );
    call(&app,"POST",&endpoint(&stale,"/transition"),Some(json!({"expected_revision":stale["revision"],"status":"in_review","operator":"Synthetic reviewer","note":"Cannot accept stale text."})),409).await;
    let mut repaired = edit(&stale);
    repaired["tickmark_catalogue"] = json!([current.clone()]);
    for a in repaired["workpapers"][0]["annotations"]
        .as_array_mut()
        .unwrap()
    {
        a["body_sha256"] = json!(body_hash("Changed synthetic text."));
    }
    let repaired = call(&app, "PUT", &endpoint(&stale, ""), Some(repaired), 200).await;
    let mut replace = edit(&repaired);
    let annotations = replace["workpapers"][0]["annotations"]
        .as_array_mut()
        .unwrap();
    annotations.retain(|a| a["id"] != "custom1");
    annotations.push(json!({"id":"custom2","kind":"custom","custom_mark":current.clone(),"note":"A deliberate new meaning uses a new annotation ID.","operator_label":"Synthetic preparer","body_sha256":body_hash("Changed synthetic text.")}));
    let replace = call(&app, "PUT", &endpoint(&repaired, ""), Some(replace), 200).await;
    let replace = calculate(&app, &replace).await;
    assert_eq!(replace["snapshot"]["result"]["supported"], true);
    let locked = lock(&app, &replace).await;
    let mut locked_edit = edit(&locked);
    locked_edit["tickmark_catalogue"] = json!([]);
    call(&app, "PUT", &endpoint(&locked, ""), Some(locked_edit), 409).await;
    let (status, bundle) = raw(
        &app,
        "GET",
        &endpoint(&locked, "/attachments/package"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let bundled = bundled_json(&bundle);
    assert_eq!(
        bundled["engagement"]["document"]["tickmark_catalogue"],
        json!([current.clone()])
    );
    assert_eq!(bundled["engagement"]["snapshots"][0], old_snapshot);
    let next=call(&app,"POST",&endpoint(&locked,"/rollforward"),Some(json!({"expected_revision":locked["revision"],"period_start":"2026-01-01","period_end":"2026-12-31"})),200).await;
    assert_eq!(next["document"]["tickmark_catalogue"], json!([current]));
    assert!(
        next["document"]["workpapers"][0]
            .get("annotations")
            .is_none()
    );
    assert!(
        next["rollforward_origin"]["copy_policy"]["preserved_fields"]
            .as_array()
            .unwrap()
            .contains(&json!("tickmark_catalogue"))
    );
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"],
        locked["snapshot"]
    );
    assert_eq!(
        call(&app, "GET", &endpoint(&locked, ""), None, 200).await,
        locked
    );
    let legacy = call(&app, "POST", "/api/engagements", Some(fixture()), 200).await;
    let legacy = calculate(&app, &legacy).await;
    let legacy = lock(&app, &legacy).await;
    let legacy_next=call(&app,"POST",&endpoint(&legacy,"/rollforward"),Some(json!({"expected_revision":legacy["revision"],"period_start":"2026-01-01","period_end":"2026-12-31"})),200).await;
    assert!(legacy_next["document"].get("tickmark_catalogue").is_none());
    assert_eq!(
        legacy_next["rollforward_origin"]["copy_policy"]["preserved_fields"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        call(&app, "GET", "/api/ledger/entries", None, 200).await,
        json!([])
    );
}
