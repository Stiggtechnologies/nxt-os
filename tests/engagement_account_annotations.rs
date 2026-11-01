//! Original NXT local-context binding acceptance. Synthetic memory, no network.
use axum::{
    body::Body,
    http::{HeaderMap, Request},
};
use nxt_os::{
    app_with,
    engagement::{
        EResult, Engagements, RevisionInput, account_annotations, analysis,
        models::{Annotation, EngagementInput},
    },
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn fixture() -> Value {
    json!({"name":"Synthetic account annotations","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","prior_period_start":"2024-01-01","prior_period_end":"2024-12-31","currency":"CAD","accounts":[{"id":"cash","code":"100","name":"Synthetic cash","kind":"asset","group_id":"assets","debit_cents":10000},{"id":"capital","code":"300","name":"Synthetic capital","kind":"equity","group_id":"equity","credit_cents":10000},{"id":"other","code":"101","name":"Unrelated asset","kind":"asset","group_id":"otherassets"},{"id":"otherliability","code":"201","name":"Unrelated liability","kind":"liability","group_id":"liabilities"}],"groups":[{"id":"assets","code":"A","name":"Synthetic assets","kind":"asset"},{"id":"equity","code":"E","name":"Synthetic equity","kind":"equity"},{"id":"otherassets","code":"O","name":"Other assets","kind":"asset"},{"id":"liabilities","code":"L","name":"Liabilities","kind":"liability"}],"adjustments":[{"id":"aje1","reference":"AJ1","date":"2025-11-01","description":"Touching selected account","lines":[{"account_id":"cash","debit_cents":25},{"account_id":"capital","credit_cents":25}]},{"id":"aje2","reference":"AJ2","date":"2025-12-01","lines":[{"account_id":"cash","debit_cents":10},{"account_id":"capital","credit_cents":10}]},{"id":"unrelated","reference":"AJ3","date":"2025-12-02","lines":[{"account_id":"other","debit_cents":5},{"account_id":"otherliability","credit_cents":5}]}],"workpapers":[{"id":"paper1","reference":"A1","title":"Synthetic manual support","body":"Synthetic text <evidence>.","required":true,"account_ids":["cash","capital"],"adjustment_ids":["aje1","aje2"]},{"id":"paper2","reference":"B1","title":"Unrelated manual support","body":"Unrelated text.","account_ids":["other","otherliability"],"adjustment_ids":["unrelated"]},{"id":"plan1","reference":"P1","title":"Pending","document_type":"placeholder","account_ids":["cash"]}]})
}
fn input(value: Value) -> EngagementInput {
    serde_json::from_value(value).unwrap()
}
fn context(value: &Value, owner: &str, account: &str) -> String {
    account_annotations::context_sha256(&input(value.clone()), owner, "paper1", account).unwrap()
}
fn mark(id: &str, owner: &str, account: &str, hash: &str) -> Value {
    json!({"id":id,"kind":"note","note":"Synthetic account note <script>unsafe</script>","operator_label":"Synthetic <reviewer>","body_sha256":digest(b"Synthetic text <evidence>."),"account_anchor":{"engagement_id":owner,"account_id":account,"context_sha256":hash}})
}
fn body_mark() -> Value {
    let mut value = mark("legacy1", "eng_synthetic", "cash", &"a".repeat(64));
    value.as_object_mut().unwrap().remove("account_anchor");
    value
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
        let start = at + 30 + name_len + extra;
        if &bytes[at + 30..at + 30 + name_len] == b"engagement.json" {
            return serde_json::from_slice(&bytes[start..start + len]).unwrap();
        }
        at = start + len;
    }
    panic!("Original bundle must retain engagement.json")
}
fn scoped_fixture() -> Value {
    let mut value = fixture();
    let hash = context(&value, "eng_synthetic", "cash");
    value["workpapers"][0]["annotations"] =
        json!([mark("account1", "eng_synthetic", "cash", &hash)]);
    value
}
fn has(result: &Value, code: &str) -> bool {
    result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == code)
}

#[test]
fn strict_anchor_objects_and_legacy_preimages_are_preserved() {
    let mut client_registry = fixture();
    client_registry["annotation_bindings"] = json!([]);
    assert!(serde_json::from_value::<EngagementInput>(client_registry).is_err());
    let old = format!(
        r#"{{"id":"legacy1","kind":"note","note":"Legacy","operator_label":"Synthetic","body_sha256":"{}"}}"#,
        "a".repeat(64)
    );
    let annotation: Annotation = serde_json::from_str(&old).unwrap();
    assert!(annotation.account_anchor.is_none());
    assert_eq!(serde_json::to_string(&annotation).unwrap(), old);
    assert_eq!(
        digest(&serde_json::to_vec(&annotation).unwrap()),
        digest(old.as_bytes())
    );
    let mut legacy = fixture();
    legacy["workpapers"][0]["annotations"] = json!([body_mark()]);
    let parsed = input(legacy.clone());
    let raw = serde_json::to_vec(&parsed).unwrap();
    assert!(!String::from_utf8_lossy(&raw).contains("account_anchor"));
    let again: EngagementInput = serde_json::from_slice(&raw).unwrap();
    assert_eq!(digest(&serde_json::to_vec(&again).unwrap()), digest(&raw));
    for bad in [
        Value::Null,
        json!([]),
        json!(["eng_synthetic", "cash", "a".repeat(64)]),
        json!(true),
        json!(1),
        json!("cash"),
        json!({}),
        json!({"engagement_id":"eng_synthetic","account_id":"cash"}),
        json!({"engagement_id":"eng_synthetic","account_id":"cash","context_sha256":"a".repeat(64),"extra":1}),
    ] {
        let mut value = scoped_fixture();
        value["workpapers"][0]["annotations"][0]["account_anchor"] = bad;
        assert!(serde_json::from_value::<EngagementInput>(value).is_err());
    }
    let duplicate = format!(
        r#"{{"id":"n1","kind":"note","note":"Synthetic","operator_label":"Synthetic","body_sha256":"{}","account_anchor":{{"engagement_id":"eng_synthetic","account_id":"cash","account_id":"capital","context_sha256":"{}"}}}}"#,
        "a".repeat(64),
        "b".repeat(64)
    );
    assert!(serde_json::from_str::<Annotation>(&duplicate).is_err());
    for (field, value) in [
        ("engagement_id", json!("bad.id")),
        ("engagement_id", json!("")),
        ("account_id", json!("unknown")),
        ("account_id", json!("other")),
        ("context_sha256", json!("A".repeat(64))),
        ("context_sha256", json!("0".repeat(63))),
        ("context_sha256", json!("g".repeat(64))),
    ] {
        let mut document = scoped_fixture();
        document["workpapers"][0]["annotations"][0]["account_anchor"][field] = value;
        assert!(analysis::validate(&input(document)).is_err(), "{field}");
    }
    let mut placeholder = scoped_fixture();
    placeholder["workpapers"][2]["annotations"] =
        placeholder["workpapers"][0]["annotations"].clone();
    placeholder["workpapers"][0]
        .as_object_mut()
        .unwrap()
        .remove("annotations");
    assert!(analysis::validate(&input(placeholder)).is_err());
}

#[test]
fn canonical_context_covers_complete_selected_inputs_without_unrelated_or_text_coupling() {
    let base = scoped_fixture();
    let parsed = input(base.clone());
    let owner = "eng_synthetic";
    let original = context(&base, owner, "cash");
    let expected = json!({"format":"nxt-account-annotation-context/v1","ruleset":"nxt-engagement-workpapers-v1","engagement_id":owner,"currency":"CAD","period_start":"2025-01-01","period_end":"2025-12-31","prior_period_start":"2024-01-01","prior_period_end":"2024-12-31","workpaper":{"id":"paper1","reference":"A1"},"account":parsed.accounts[0],"group":parsed.groups[0],"adjustments":[parsed.adjustments[0],parsed.adjustments[1]]});
    assert_eq!(original, digest(&serde_json::to_vec(&expected).unwrap()));
    assert_ne!(original, context(&base, "eng_other", "cash"));
    assert!(!has(
        &analysis::analyze(&parsed),
        "stale_account_annotation"
    ));
    let changes: Vec<Box<dyn Fn(&mut Value)>> = vec![
        Box::new(|v| v["accounts"][0]["debit_cents"] = json!(10001)),
        Box::new(|v| v["accounts"][0]["prior_debit_cents"] = json!(1)),
        Box::new(|v| v["accounts"][0]["code"] = json!("109")),
        Box::new(|v| v["accounts"][0]["name"] = json!("Changed selected account")),
        Box::new(|v| {
            v["accounts"][0]["kind"] = json!("expense");
            v["accounts"][0]["group_id"] = Value::Null;
        }),
        Box::new(|v| v["accounts"][0]["group_id"] = json!("otherassets")),
        Box::new(|v| v["groups"][0]["name"] = json!("Changed linked definition")),
        Box::new(|v| v["groups"][0]["code"] = json!("AX")),
        Box::new(|v| v["groups"][0]["gifi_code"] = json!("1000")),
        Box::new(|v| v["adjustments"][0]["description"] = json!("Changed touching entry")),
        Box::new(|v| v["adjustments"][0]["date"] = json!("2025-11-02")),
        Box::new(|v| v["adjustments"][0]["reference"] = json!("AJX")),
        Box::new(|v| v["adjustments"][0]["kind"] = json!("reclassifying")),
        Box::new(|v| v["adjustments"][0]["lines"][1]["credit_cents"] = json!(26)),
        Box::new(|v| {
            v["adjustments"][0]["lines"]
                .as_array_mut()
                .unwrap()
                .reverse()
        }),
        Box::new(|v| v["adjustments"].as_array_mut().unwrap().swap(0, 1)),
        Box::new(|v| {
            v["adjustments"].as_array_mut().unwrap().remove(1);
            v["workpapers"][0]["adjustment_ids"] = json!(["aje1"]);
        }),
        Box::new(|v| v["period_start"] = json!("2025-01-02")),
        Box::new(|v| v["period_end"] = json!("2025-12-30")),
        Box::new(|v| v["prior_period_start"] = json!("2024-01-02")),
        Box::new(|v| v["prior_period_end"] = json!("2024-12-30")),
        Box::new(|v| v["workpapers"][0]["reference"] = json!("A2")),
    ];
    for (index, change) in changes.into_iter().enumerate() {
        let mut changed = base.clone();
        change(&mut changed);
        assert_ne!(context(&changed, owner, "cash"), original, "change {index}");
        let parsed = input(changed);
        assert!(analysis::validate(&parsed).is_ok(), "change {index}");
        assert!(
            has(&analysis::analyze(&parsed), "stale_account_annotation"),
            "change {index}"
        );
    }
    let unrelated: Vec<Box<dyn Fn(&mut Value)>> = vec![
        Box::new(|v| v["accounts"][1]["name"] = json!("Counterpart definition is not selected")),
        Box::new(|v| {
            v["accounts"][2]["debit_cents"] = json!(2);
            v["accounts"][2]["group_id"] = json!("assets");
        }),
        Box::new(|v| v["groups"][2]["name"] = json!("Unrelated group definition")),
        Box::new(|v| v["adjustments"][2]["description"] = json!("Unrelated entry changed")),
        Box::new(|v| v["adjustments"].as_array_mut().unwrap().swap(1, 2)),
        Box::new(|v| v["workpapers"][1]["body"] = json!("Unrelated text changed")),
        Box::new(|v| v["workpapers"][0]["title"] = json!("Non-context planning title")),
        Box::new(|v| {
            v["workpapers"][0]["annotations"][0]["note"] =
                json!("Note does not recursively hash itself")
        }),
    ];
    for change in unrelated {
        let mut changed = base.clone();
        change(&mut changed);
        assert_eq!(context(&changed, owner, "cash"), original);
        assert!(!has(
            &analysis::analyze(&input(changed)),
            "stale_account_annotation"
        ));
    }
    let mut text = base.clone();
    text["workpapers"][0]["body"] = json!("Changed exact body");
    assert_eq!(context(&text, owner, "cash"), original);
    let result = analysis::analyze(&input(text));
    assert!(has(&result, "stale_workpaper_annotation"));
    assert!(!has(&result, "stale_account_annotation"));
    let mut currency = base;
    currency["currency"] = json!("USD");
    assert_ne!(context(&currency, owner, "cash"), original); // CAD remains the only supported calculation currency.
}

async fn raw(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, HeaderMap, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("Content-Type", "application/json");
    if auth {
        builder = builder.header("Authorization", "Bearer account-annotations-test-key");
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}
async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    expected: u16,
) -> Value {
    let (s, _, b) = raw(app, method, path, body, true).await;
    let v =
        serde_json::from_slice(&b).unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&b)}));
    assert_eq!(s, expected, "{method} {path}: {v}");
    v
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
async fn sign(app: &axum::Router, r: &Value, stage: &str, expected: u16) -> Value {
    call(app,"POST",&endpoint(r,"/signoffs"),Some(json!({"expected_revision":r["revision"],"workpaper_id":"paper1","stage":stage,"operator":if stage=="prepared"{"Synthetic preparer"}else{"Synthetic reviewer"},"note":"Synthetic only"})),expected).await
}
async fn stage(app: &axum::Router, r: &Value, status: &str) -> Value {
    call(app,"POST",&endpoint(r,"/transition"),Some(json!({"expected_revision":r["revision"],"status":status,"operator":"Synthetic reviewer","note":"Synthetic only"})),200).await
}

#[tokio::test]
async fn saved_options_immutable_scope_stale_gates_and_rollforward_are_owner_bound() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "account-annotations-test-key");
    }
    let app = app_with(Store::memory);
    let mut source = fixture();
    source["workpapers"][0]["annotations"] = json!([body_mark()]);
    let r = call(&app, "POST", "/api/engagements", Some(source), 200).await;
    let r=call(&app,"POST",&endpoint(&r,"/workpapers/paper1/attachments"),Some(json!({"expected_revision":r["revision"],"upload_id":"synthetic_account_file","file_name":"synthetic.txt","operator_label":"Synthetic","note":"Synthetic only","content_hex":"61"})),200).await;
    let options_path = endpoint(&r, "/workpapers/paper1/account-anchors");
    assert_eq!(raw(&app, "GET", &options_path, None, false).await.0, 401);
    assert_eq!(raw(&app, "POST", &options_path, None, true).await.0, 405);
    let audit = call(&app, "GET", "/api/engagements/audit", None, 200).await;
    let (status, headers, bytes) = raw(&app, "GET", &options_path, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(headers["cache-control"], "private, no-store");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let options: Value = serde_json::from_slice(&bytes).unwrap();
    let keys: Vec<_> = options
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        vec![
            "accounts",
            "body_sha256",
            "engagement_id",
            "expected_revision",
            "format",
            "input_hash",
            "input_revision",
            "scope",
            "workpaper_id"
        ]
    );
    assert_eq!(options["format"], "nxt-account-annotation-options/v1");
    assert_eq!(options["engagement_id"], r["id"]);
    assert_eq!(options["expected_revision"], r["revision"]);
    assert_eq!(options["input_revision"], r["input_revision"]);
    assert_eq!(options["workpaper_id"], "paper1");
    assert_eq!(
        options["body_sha256"],
        digest(b"Synthetic text <evidence>.")
    );
    assert_eq!(
        options["input_hash"],
        digest(&serde_json::to_vec(&input(r["document"].clone())).unwrap())
    );
    assert!(
        options["scope"]
            .as_str()
            .unwrap()
            .contains("group definition")
    );
    assert_eq!(options["accounts"].as_array().unwrap().len(), 2);
    for a in options["accounts"].as_array().unwrap() {
        assert_eq!(a.as_object().unwrap().len(), 2);
        assert_eq!(
            a["context_sha256"],
            context(
                &r["document"],
                r["id"].as_str().unwrap(),
                a["account_id"].as_str().unwrap()
            )
        );
    }
    assert_eq!(call(&app, "GET", &endpoint(&r, ""), None, 200).await, r);
    assert_eq!(
        call(&app, "GET", "/api/engagements/audit", None, 200).await,
        audit
    );
    call(
        &app,
        "GET",
        &endpoint(&r, "/workpapers/plan1/account-anchors"),
        None,
        409,
    )
    .await;
    call(
        &app,
        "GET",
        &endpoint(&r, "/workpapers/missing/account-anchors"),
        None,
        404,
    )
    .await;
    let hash = options["accounts"][0]["context_sha256"].as_str().unwrap();
    let owner = r["id"].as_str().unwrap();
    let account_mark = mark("account1", owner, "cash", hash);
    let mut d = edit(&r);
    d["workpapers"][0]["annotations"]
        .as_array_mut()
        .unwrap()
        .push(account_mark.clone());
    let marked = call(&app, "PUT", &endpoint(&r, ""), Some(d.clone()), 200).await;
    call(&app, "PUT", &endpoint(&r, ""), Some(d), 409).await;
    let r = calculate(&app, &marked).await;
    assert_eq!(r["snapshot"]["result"]["supported"], true);
    for mode in [
        "remove_scope",
        "change_scope",
        "change_digest",
        "change_text",
        "add_scope",
    ] {
        let mut changed = edit(&r);
        match mode {
            "remove_scope" => {
                changed["workpapers"][0]["annotations"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("account_anchor");
            }
            "change_scope" => {
                changed["workpapers"][0]["annotations"][1]["account_anchor"]["account_id"] =
                    json!("capital");
                changed["workpapers"][0]["annotations"][1]["account_anchor"]["context_sha256"] =
                    options["accounts"][1]["context_sha256"].clone();
            }
            "change_digest" => {
                changed["workpapers"][0]["annotations"][1]["account_anchor"]["context_sha256"] =
                    json!("0".repeat(64))
            }
            "change_text" => {
                changed["workpapers"][0]["annotations"][1]["body_sha256"] = json!("0".repeat(64))
            }
            _ => {
                changed["workpapers"][0]["annotations"][0]["account_anchor"] =
                    account_mark["account_anchor"].clone()
            }
        }
        call(&app, "PUT", &endpoint(&r, ""), Some(changed), 409).await;
    }
    let other = call(&app, "POST", "/api/engagements", Some(fixture()), 200).await;
    let mut replay = edit(&other);
    replay["workpapers"][0]["annotations"] = json!([account_mark.clone()]);
    call(&app, "PUT", &endpoint(&other, ""), Some(replay), 409).await;
    let other_options = call(
        &app,
        "GET",
        &endpoint(&other, "/workpapers/paper1/account-anchors"),
        None,
        200,
    )
    .await;
    assert_ne!(
        other_options["accounts"][0]["context_sha256"],
        options["accounts"][0]["context_sha256"]
    );
    let mut replay = fixture();
    replay["workpapers"][0]["annotations"] = json!([account_mark]);
    call(&app, "POST", "/api/engagements", Some(replay), 409).await;
    // Legacy body-only anchors keep their existing editable behavior.
    let mut body_only = edit(&r);
    body_only["workpapers"][0]["annotations"][0]["body_sha256"] = json!("0".repeat(64));
    let body_only = call(&app, "PUT", &endpoint(&r, ""), Some(body_only), 200).await;
    let mut body_only_fix = edit(&body_only);
    body_only_fix["workpapers"][0]["annotations"][0]["body_sha256"] =
        json!(digest(b"Synthetic text <evidence>."));
    let r = call(
        &app,
        "PUT",
        &endpoint(&body_only, ""),
        Some(body_only_fix),
        200,
    )
    .await;
    let mut changed = edit(&r);
    changed["accounts"][0]["code"] = json!("109");
    let stale = call(&app, "PUT", &endpoint(&r, ""), Some(changed), 200).await;
    let stale = calculate(&app, &stale).await;
    assert!(has(
        &stale["snapshot"]["result"],
        "stale_account_annotation"
    ));
    assert_eq!(stale["snapshot"]["result"]["supported"], false);
    sign(&app, &stale, "prepared", 409).await;
    let fresh = call(
        &app,
        "GET",
        &endpoint(&stale, "/workpapers/paper1/account-anchors"),
        None,
        200,
    )
    .await;
    let mut reanchor = edit(&stale);
    reanchor["workpapers"][0]["annotations"][1]["account_anchor"]["context_sha256"] =
        fresh["accounts"][0]["context_sha256"].clone();
    call(
        &app,
        "PUT",
        &endpoint(&stale, ""),
        Some(reanchor.clone()),
        409,
    )
    .await;
    reanchor["workpapers"][0]["annotations"][1]["id"] = json!("account2");
    let fixed = call(&app, "PUT", &endpoint(&stale, ""), Some(reanchor), 200).await;
    let r = calculate(&app, &fixed).await;
    assert_eq!(r["snapshot"]["result"]["supported"], true);
    let r = sign(&app, &r, "prepared", 200).await;
    let r = stage(&app, &r, "in_review").await;
    let r = sign(&app, &r, "reviewed", 200).await;
    let locked = stage(&app, &r, "locked").await;
    call(
        &app,
        "PUT",
        &endpoint(&locked, ""),
        Some(edit(&locked)),
        409,
    )
    .await;
    let package = call(&app, "GET", &endpoint(&locked, "/package"), None, 200).await;
    assert_eq!(
        package["engagement"]["annotation_bindings"],
        locked["annotation_bindings"]
    );
    assert_eq!(
        package["engagement"]["document"]["workpapers"][0]["annotations"][1]["account_anchor"],
        locked["document"]["workpapers"][0]["annotations"][1]["account_anchor"]
    );
    let (status, _, html) = raw(&app, "GET", &endpoint(&locked, "/print"), None, true).await;
    assert_eq!(status, 200);
    let html = String::from_utf8(html).unwrap();
    assert!(html.contains("Saved account target / context SHA-256"));
    assert!(html.contains(locked["id"].as_str().unwrap()));
    assert!(html.contains(fresh["accounts"][0]["context_sha256"].as_str().unwrap()));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
    let next=call(&app,"POST",&endpoint(&locked,"/rollforward"),Some(json!({"expected_revision":locked["revision"],"period_start":"2026-01-01","period_end":"2026-12-31"})),200).await;
    assert!(
        next["document"]["workpapers"][0]
            .get("annotations")
            .is_none()
    );
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"],
        locked["snapshot"]
    );
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["annotations"]
            [1]["account_anchor"]["engagement_id"],
        locked["id"]
    );
    let (s, _, bytes) = raw(
        &app,
        "GET",
        &endpoint(&next, "/attachments/package"),
        None,
        true,
    )
    .await;
    assert_eq!(s, 200);
    assert!(bytes.starts_with(b"PK"));
    let zip_package = bundled_json(&bytes);
    assert!(next.get("annotation_bindings").is_none());
    assert_eq!(zip_package["engagement"], next);
    assert_eq!(
        zip_package["engagement"]["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"]
            [0]["annotations"][1]["account_anchor"]["engagement_id"],
        locked["id"]
    );
    assert_eq!(
        call(&app, "GET", "/api/ledger/entries", None, 200).await,
        json!([])
    );
}

fn must<T>(r: EResult<T>) -> T {
    r.unwrap_or_else(|e| panic!("{} {}", e.0, e.1))
}
fn engine() -> Engagements {
    let ledger = Arc::new(nxt_os::ledger::engine::Ledger::new(Store::memory()));
    let tax = Arc::new(nxt_os::tax::Tax::new(Store::memory()));
    let firm = Arc::new(nxt_os::firm::Firm::new(Store::memory(), ledger.clone()));
    Engagements::new(Store::memory(), ledger, tax, firm)
}
async fn save_document(
    e: &Engagements,
    r: &nxt_os::engagement::EngagementRecord,
    mut document: Value,
) -> nxt_os::engagement::EngagementRecord {
    document["expected_revision"] = json!(r.revision);
    must(e.save(Some(&r.id), input(document)).await)
}

#[tokio::test]
async fn lifetime_account_bindings_survive_remove_save_reuse_with_and_without_calculation() {
    for calculated in [false, true] {
        let e = engine();
        let r = must(e.save(None, input(fixture())).await);
        assert!(
            serde_json::to_value(&r)
                .unwrap()
                .get("annotation_bindings")
                .is_none()
        );
        let options = must(e.account_anchor_options(&r.id, "paper1").await);
        let mut original = mark(
            "lifetime1",
            &r.id,
            "cash",
            options["accounts"][0]["context_sha256"].as_str().unwrap(),
        );
        original["kind"] = json!("custom");
        original["custom_mark"] = json!({"id":"meaning1","code":"QA","label":"Original copied meaning","description":"Original synthetic definition"});
        let mut d = serde_json::to_value(&r.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([original.clone()]);
        let r = save_document(&e, &r, d).await;
        let r = if calculated {
            must(
                e.calculate(
                    &r.id,
                    RevisionInput {
                        expected_revision: r.revision,
                    },
                )
                .await,
            )
        } else {
            r
        };
        assert_eq!(r.snapshots.len(), usize::from(calculated));
        assert_eq!(r.annotation_bindings.len(), 1);
        let registry = serde_json::to_value(&r.annotation_bindings).unwrap();
        let mut d = serde_json::to_value(&r.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([]);
        let removed = save_document(&e, &r, d).await;
        assert_eq!(
            serde_json::to_value(&removed.annotation_bindings).unwrap(),
            registry
        );
        for mode in [
            "body_only",
            "body_sha",
            "context",
            "account",
            "kind",
            "custom_meaning",
            "paper",
        ] {
            let mut d = serde_json::to_value(&removed.document).unwrap();
            let mut changed = original.clone();
            match mode {
                "body_only" => {
                    changed.as_object_mut().unwrap().remove("account_anchor");
                }
                "body_sha" => changed["body_sha256"] = json!("0".repeat(64)),
                "context" => changed["account_anchor"]["context_sha256"] = json!("0".repeat(64)),
                "account" => {
                    changed["account_anchor"]["account_id"] = json!("capital");
                    changed["account_anchor"]["context_sha256"] =
                        options["accounts"][1]["context_sha256"].clone();
                }
                "kind" => {
                    changed["kind"] = json!("note");
                    changed.as_object_mut().unwrap().remove("custom_mark");
                }
                "custom_meaning" => {
                    changed["custom_mark"]["label"] = json!("Changed copied meaning")
                }
                _ => {
                    d["workpapers"][1]["account_ids"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("cash"));
                }
            }
            let paper = if mode == "paper" { 1 } else { 0 };
            d["workpapers"][paper]["annotations"] = json!([changed]);
            d["expected_revision"] = json!(removed.revision);
            let before = e.store.get("engagements:state:v1").await.unwrap();
            let rejected = e.save(Some(&removed.id), input(d)).await.unwrap_err();
            assert_eq!(rejected.0, 409, "{mode}");
            assert!(rejected.1.contains("lifetime"), "{mode}: {}", rejected.1);
            assert_eq!(e.store.get("engagements:state:v1").await.unwrap(), before);
        }
        let mut d = serde_json::to_value(&removed.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([original.clone()]);
        let restored = save_document(&e, &removed, d).await;
        assert_eq!(
            serde_json::to_value(&restored.annotation_bindings).unwrap(),
            registry
        );
        let mut d = serde_json::to_value(&restored.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([]);
        d["accounts"][0]["code"] = json!("109");
        d["workpapers"][0]["body"] = json!("Changed synthetic text after removal");
        let changed = save_document(&e, &restored, d).await;
        let fresh = must(e.account_anchor_options(&changed.id, "paper1").await);
        let mut reanchored = original;
        reanchored["body_sha256"] = fresh["body_sha256"].clone();
        reanchored["account_anchor"]["context_sha256"] =
            fresh["accounts"][0]["context_sha256"].clone();
        let mut d = serde_json::to_value(&changed.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([reanchored.clone()]);
        d["expected_revision"] = json!(changed.revision);
        assert_eq!(
            e.save(Some(&changed.id), input(d.clone()))
                .await
                .unwrap_err()
                .0,
            409
        );
        reanchored["id"] = json!("lifetime2");
        d["workpapers"][0]["annotations"] = json!([reanchored]);
        let saved = save_document(&e, &changed, d).await;
        assert_eq!(saved.annotation_bindings.len(), 2);
    }
}

#[tokio::test]
async fn registry_corruption_and_legacy_body_only_upgrade_attempts_fail_closed() {
    let plain = engine();
    let mut d = fixture();
    d["workpapers"][0]["annotations"] = json!([body_mark()]);
    let r = must(plain.save(None, input(d)).await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([]);
    let r = save_document(&plain, &r, d).await;
    assert!(r.snapshots.is_empty());
    let options = must(plain.account_anchor_options(&r.id, "paper1").await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["expected_revision"] = json!(r.revision);
    d["workpapers"][0]["annotations"] = json!([mark(
        "legacy1",
        &r.id,
        "cash",
        options["accounts"][0]["context_sha256"].as_str().unwrap()
    )]);
    assert_eq!(plain.save(Some(&r.id), input(d)).await.unwrap_err().0, 409);
    let e = engine();
    let r = must(e.save(None, input(fixture())).await);
    let options = must(e.account_anchor_options(&r.id, "paper1").await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([mark(
        "receipt1",
        &r.id,
        "cash",
        options["accounts"][0]["context_sha256"].as_str().unwrap()
    )]);
    let r = save_document(&e, &r, d).await;
    let r = must(
        e.calculate(
            &r.id,
            RevisionInput {
                expected_revision: r.revision,
            },
        )
        .await,
    );
    let retained = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    for mode in ["missing", "empty", "changed", "duplicate"] {
        let mut state: Value = serde_json::from_str(&retained).unwrap();
        let record = &mut state["engagements"][0];
        match mode {
            "missing" => {
                record
                    .as_object_mut()
                    .unwrap()
                    .remove("annotation_bindings");
            }
            "empty" => record["annotation_bindings"] = json!([]),
            "changed" => {
                record["annotation_bindings"][0]["account_binding_sha256"] = json!("0".repeat(64))
            }
            _ => {
                let duplicate = record["annotation_bindings"][0].clone();
                record["annotation_bindings"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
        }
        e.store
            .set_json("engagements:state:v1", &state)
            .await
            .unwrap();
        assert_eq!(e.get(&r.id).await.unwrap_err().0, 409, "{mode}");
        e.store
            .set("engagements:state:v1", &retained)
            .await
            .unwrap();
    }
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([]);
    let removed = save_document(&e, &r, d).await;
    let valid = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    let mut corrupt: Value = serde_json::from_str(&valid).unwrap();
    corrupt["engagements"][0]
        .as_object_mut()
        .unwrap()
        .remove("annotation_bindings");
    e.store
        .set_json("engagements:state:v1", &corrupt)
        .await
        .unwrap();
    assert_eq!(e.get(&removed.id).await.unwrap_err().0, 409);
    e.store.set("engagements:state:v1", &valid).await.unwrap();
    // A pre-feature body-only ID remains editable body-only, but may not become
    // account-scoped even after it has been removed or retained only in history.
    let legacy = engine();
    let mut old = fixture();
    old["workpapers"][0]["annotations"] = json!([body_mark()]);
    let r = must(legacy.save(None, input(old)).await);
    let r = must(
        legacy
            .calculate(
                &r.id,
                RevisionInput {
                    expected_revision: r.revision,
                },
            )
            .await,
    );
    let mut state: Value = serde_json::from_str(
        &legacy
            .store
            .get("engagements:state:v1")
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    state["engagements"][0]
        .as_object_mut()
        .unwrap()
        .remove("annotation_bindings");
    state["engagements"][0]["document"]["workpapers"][0]["annotations"] = json!([]);
    state["engagements"][0]["snapshot"] = Value::Null;
    legacy
        .store
        .set_json("engagements:state:v1", &state)
        .await
        .unwrap();
    let r = must(legacy.get(&r.id).await);
    let options = must(legacy.account_anchor_options(&r.id, "paper1").await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["expected_revision"] = json!(r.revision);
    d["workpapers"][0]["annotations"] = json!([mark(
        "legacy1",
        &r.id,
        "cash",
        options["accounts"][0]["context_sha256"].as_str().unwrap()
    )]);
    let before = legacy.store.get("engagements:state:v1").await.unwrap();
    assert_eq!(legacy.save(Some(&r.id), input(d)).await.unwrap_err().0, 409);
    assert_eq!(
        legacy.store.get("engagements:state:v1").await.unwrap(),
        before
    );
    let mut d = serde_json::to_value(&r.document).unwrap();
    let mut old = body_mark();
    old["body_sha256"] = json!("0".repeat(64));
    d["workpapers"][0]["annotations"] = json!([old]);
    let r = save_document(&legacy, &r, d).await;
    assert!(
        r.annotation_bindings
            .iter()
            .any(|b| b.annotation_id == "legacy1" && b.account_binding_sha256.is_none())
    );
}

#[tokio::test]
async fn lifetime_registry_capacity_rejects_atomically_without_inflating_other_bounds() {
    assert_eq!(account_annotations::REGISTRY_MAX, 2000);
    let e = engine();
    let bulk = |batch: usize| json!({"name":"Synthetic lifetime capacity","period_start":"2025-01-01","period_end":"2025-12-31","workpapers":(0..10).map(|paper|json!({"id":format!("paper{paper}"),"reference":format!("P{paper}"),"title":"Synthetic","body":"Synthetic","annotations":(0..50).map(|n|json!({"id":format!("id{}",batch*500+paper*50+n),"kind":"note","note":"Synthetic","operator_label":"Synthetic","body_sha256":digest(b"Synthetic")})).collect::<Vec<_>>()})).collect::<Vec<_>>()});
    let mut r = must(e.save(None, input(bulk(0))).await);
    for batch in 1..4 {
        r = save_document(&e, &r, bulk(batch)).await;
    }
    assert_eq!(r.annotation_bindings.len(), 2000);
    let before = e.store.get("engagements:state:v1").await.unwrap();
    let mut extra = bulk(4);
    extra["expected_revision"] = json!(r.revision);
    let error = e.save(Some(&r.id), input(extra)).await.unwrap_err();
    assert_eq!(error.0, 409);
    assert!(error.1.contains("2000"));
    assert_eq!(e.store.get("engagements:state:v1").await.unwrap(), before);
    let record = must(e.get(&r.id).await);
    assert_eq!(record.annotation_bindings.len(), 2000);
    assert!(record.snapshots.is_empty());
}
#[tokio::test]
async fn stored_current_historical_and_origin_owner_checks_fail_closed_without_legacy_changes() {
    let e = engine();
    let r = must(e.save(None, input(fixture())).await);
    let before = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    let options = must(e.account_anchor_options(&r.id, "paper1").await);
    assert_eq!(
        e.store.get("engagements:state:v1").await.unwrap().unwrap(),
        before
    );
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["expected_revision"] = json!(r.revision);
    d["workpapers"][0]["annotations"] = json!([mark(
        "account1",
        &r.id,
        "cash",
        options["accounts"][0]["context_sha256"].as_str().unwrap()
    )]);
    let r = must(e.save(Some(&r.id), input(d)).await);
    let r = must(
        e.calculate(
            &r.id,
            RevisionInput {
                expected_revision: r.revision,
            },
        )
        .await,
    );
    let raw = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    for scope in ["current", "history"] {
        let mut corrupt: Value = serde_json::from_str(&raw).unwrap();
        let doc = if scope == "current" {
            &mut corrupt["engagements"][0]["document"]
        } else {
            &mut corrupt["engagements"][0]["snapshots"][0]["input_document"]
        };
        doc["workpapers"][0]["annotations"][0]["account_anchor"]["engagement_id"] =
            json!("eng_wrong_owner");
        e.store
            .set_json("engagements:state:v1", &corrupt)
            .await
            .unwrap();
        let error = e.get(&r.id).await.unwrap_err();
        assert_eq!(error.0, 409);
        assert!(error.1.contains("owner"));
        e.store.set("engagements:state:v1", &raw).await.unwrap();
    }
    let sign=serde_json::from_value(json!({"expected_revision":r.revision,"workpaper_id":"paper1","stage":"prepared","operator":"Synthetic preparer","note":"Synthetic owner QA"})).unwrap();
    let r = must(e.signoff(&r.id, sign).await);
    let r=must(e.transition(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"status":"in_review","operator":"Synthetic reviewer","note":"Synthetic owner QA"})).unwrap()).await);
    let r=must(e.signoff(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"workpaper_id":"paper1","stage":"reviewed","operator":"Synthetic reviewer","note":"Synthetic owner QA"})).unwrap()).await);
    let r=must(e.transition(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"status":"locked","operator":"Synthetic reviewer","note":"Synthetic owner QA"})).unwrap()).await);
    let target=must(e.rollforward(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"period_start":"2026-01-01","period_end":"2026-12-31"})).unwrap()).await);
    assert_eq!(
        must(e.get(&target.id).await)
            .rollforward_origin
            .as_ref()
            .unwrap()
            .source_snapshot
            .input_document
            .workpapers[0]
            .annotations[0]
            .account_anchor
            .as_ref()
            .unwrap()
            .engagement_id,
        r.id
    );
    let retained = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    let mut corrupt: Value = serde_json::from_str(&retained).unwrap();
    let target_record = corrupt["engagements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"] == target.id)
        .unwrap();
    target_record["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["annotations"]
        [0]["account_anchor"]["engagement_id"] = json!(target.id);
    e.store
        .set_json("engagements:state:v1", &corrupt)
        .await
        .unwrap();
    assert_eq!(e.get(&target.id).await.unwrap_err().0, 409);
    e.store
        .set("engagements:state:v1", &retained)
        .await
        .unwrap();
    assert_eq!(must(e.get(&target.id).await).id, target.id);
}
