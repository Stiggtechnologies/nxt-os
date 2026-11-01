//! Synthetic original NXT UTF-8 range/copy/lifetime acceptance; no network.
use axum::{body::Body, http::Request};
use nxt_os::{
    app_with,
    engagement::{
        EResult, EngagementRecord, Engagements, RevisionInput, account_annotations,
        models::{Annotation, EngagementInput},
    },
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;
const TEXT: &str = "\ré😀e\u{301}\r\nB\rC\n<script>x</script>";
fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}
fn fixture() -> Value {
    json!({"name":"Synthetic recorded text","period_start":"2025-01-01","period_end":"2025-12-31","accounts":[{"id":"cash","code":"100","name":"Synthetic cash","kind":"asset","group_id":"assets","debit_cents":10000},{"id":"capital","code":"300","name":"Synthetic capital","kind":"equity","group_id":"equity","credit_cents":10000}],"groups":[{"id":"assets","code":"A","name":"Assets","kind":"asset"},{"id":"equity","code":"E","name":"Equity","kind":"equity"}],"workpapers":[{"id":"paper1","reference":"A1","title":"Synthetic","body":TEXT,"required":true,"account_ids":["cash"]},{"id":"plan1","reference":"P1","title":"Pending","document_type":"placeholder"}]})
}
fn input(v: Value) -> EngagementInput {
    serde_json::from_value(v).unwrap()
}
fn mark(owner: &str, id: &str, start: usize, selection: &str) -> Value {
    json!({"id":id,"kind":"note","note":"Synthetic <note>","operator_label":"Synthetic unverified","body_sha256":sha(TEXT),"text_range_anchor":{"engagement_id":owner,"start_utf8_byte":start,"end_utf8_byte":start+selection.len(),"selection_sha256":sha(selection),"selected_text":selection}})
}
fn valid(v: Value) -> bool {
    serde_json::from_value::<EngagementInput>(v)
        .is_ok_and(|d| nxt_os::engagement::analysis::validate(&d).is_ok())
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
async fn save(e: &Engagements, r: &EngagementRecord, mut d: Value) -> EResult<EngagementRecord> {
    d["expected_revision"] = json!(r.revision);
    e.save(Some(&r.id), input(d)).await
}

#[test]
fn utf8_exact_copy_boundaries_shapes_controls_and_stale_short_bodies() {
    let mut maximum = fixture();
    let body = "x".repeat(16384);
    maximum["workpapers"][0]["body"] = json!(body);
    let mut boundary = mark("eng_fixed", "max", 12288, &"x".repeat(4096));
    boundary["body_sha256"] = json!(sha(&body));
    maximum["workpapers"][0]["annotations"] = json!([boundary]);
    assert!(valid(maximum));
    let mut middle = fixture();
    middle["workpapers"][0]["annotations"] =
        json!([mark("eng_fixed", "middle", 2, "é😀e\u{301}\r\n")]);
    assert!(!valid(middle));
    let selected = "é😀e\u{301}\r\n";
    let mut source = fixture();
    source["workpapers"][0]["annotations"] = json!([mark("eng_fixed", "range1", 1, selected)]);
    assert!(valid(source.clone()));
    for (start, selection) in [
        (0, "\r"),
        (1 + selected.len(), "B"),
        (1 + selected.len() + 1, "\r"),
        (TEXT.find("\r\n").unwrap(), "\r\n"),
        (TEXT.find("<script>").unwrap(), "<script>x</script>"),
    ] {
        let mut v = fixture();
        v["workpapers"][0]["annotations"] = json!([mark("eng_fixed", "range1", start, selection)]);
        assert!(valid(v));
    }
    for bad in [
        Value::Null,
        json!([]),
        json!("x"),
        json!({}),
        json!({"engagement_id":"eng_fixed","start_utf8_byte":0,"end_utf8_byte":1,"selection_sha256":sha("\r"),"selected_text":"\r","extra":1}),
    ] {
        let mut v = source.clone();
        v["workpapers"][0]["annotations"][0]["text_range_anchor"] = bad;
        assert!(serde_json::from_value::<EngagementInput>(v).is_err());
    }
    for offset in [
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        Value::Null,
        json!(true),
    ] {
        let mut v = source.clone();
        v["workpapers"][0]["annotations"][0]["text_range_anchor"]["start_utf8_byte"] = offset;
        assert!(serde_json::from_value::<EngagementInput>(v).is_err());
    }
    for (key, value) in [
        ("start_utf8_byte", json!(2)),
        ("end_utf8_byte", json!(16385)),
        ("end_utf8_byte", json!(1)),
        ("selected_text", json!("")),
        ("selected_text", json!("x".repeat(4097))),
        ("selection_sha256", json!("A".repeat(64))),
        ("selection_sha256", json!("0".repeat(64))),
        ("engagement_id", json!("bad.id")),
    ] {
        let mut v = source.clone();
        v["workpapers"][0]["annotations"][0]["text_range_anchor"][key] = value;
        assert!(!valid(v), "{key}");
    }
    let mut wrong_slice = fixture();
    wrong_slice["workpapers"][0]["annotations"] = json!([mark("eng_fixed", "range1", 0, "X")]);
    assert!(!valid(wrong_slice));
    let mut control = source.clone();
    control["workpapers"][0]["annotations"][0] = mark("eng_fixed", "range1", 0, "\u{0}");
    assert!(!valid(control));
    let mut both = source.clone();
    both["workpapers"][0]["annotations"][0]["account_anchor"] =
        json!({"engagement_id":"eng_fixed","account_id":"cash","context_sha256":"a".repeat(64)});
    assert!(!valid(both));
    let mut pending = source.clone();
    pending["workpapers"][1]["annotations"] = pending["workpapers"][0]["annotations"].clone();
    pending["workpapers"][0]["annotations"] = json!([]);
    assert!(!valid(pending));
    for changed in ["", "Q", "Different shorter text"] {
        let mut stale = source.clone();
        stale["workpapers"][0]["body"] = json!(changed);
        assert!(valid(stale.clone()));
        let result = nxt_os::engagement::analysis::analyze(&input(stale));
        assert_eq!(result["supported"], false);
        assert!(
            result["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == "stale_workpaper_annotation")
        );
    }
    let duplicate = format!(
        "{{\"id\":\"a\",\"kind\":\"note\",\"note\":\"n\",\"operator_label\":\"o\",\"body_sha256\":\"{}\",\"text_range_anchor\":{{\"engagement_id\":\"eng_fixed\",\"start_utf8_byte\":0,\"start_utf8_byte\":1,\"end_utf8_byte\":1,\"selection_sha256\":\"{}\",\"selected_text\":\"x\"}}}}",
        sha(TEXT),
        sha("x")
    );
    assert!(serde_json::from_str::<Annotation>(&duplicate).is_err());
}

#[tokio::test]
async fn legacy_account_annotation_input_and_receipt_preimages_and_binding_digest_unchanged() {
    const BODY: &str = "7f08c180cb869da8f738f77724ca4cebc195744aaee6b04e21c245ab8caf9fad";
    const BINDING: &str = "285b94840ae39770cd726c81e4bbe9dedef218af1e0eac447f66f0f83e59bca3";
    let old = format!(
        r#"{{"id":"a1","kind":"note","note":"Legacy","operator_label":"Synthetic","body_sha256":"{BODY}","account_anchor":{{"engagement_id":"eng_fixed","account_id":"cash","context_sha256":"{}"}}}}"#,
        "c".repeat(64)
    );
    let annotation: Annotation = serde_json::from_str(&old).unwrap();
    assert!(annotation.text_range_anchor.is_none());
    assert_eq!(serde_json::to_string(&annotation).unwrap(), old);
    let old_input = format!(
        r#"{{"name":"Legacy","client_ref":"","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31","prior_period_start":null,"prior_period_end":null,"currency":"CAD","accounts":[{{"id":"cash","code":"100","name":"Cash","kind":"asset","group_id":null,"debit_cents":0,"credit_cents":0,"prior_debit_cents":0,"prior_credit_cents":0}}],"groups":[],"adjustments":[],"workpapers":[{{"id":"p1","reference":"A1","title":"Legacy","folder":"","body":"Legacy text.","required":false,"account_ids":["cash"],"adjustment_ids":[],"depends_on":[],"annotations":[{old}]}}],"issues":[],"notes":"","expected_revision":null}}"#
    );
    let document: EngagementInput = serde_json::from_str(&old_input).unwrap();
    assert_eq!(serde_json::to_string(&document).unwrap(), old_input);
    assert_eq!(
        sha(&serde_json::to_string(&document).unwrap()),
        sha(&old_input)
    );
    let receipt = format!(r#"{{"annotation_id":"a1","account_binding_sha256":"{BINDING}"}}"#);
    let parsed: nxt_os::engagement::AnnotationBindingReceipt =
        serde_json::from_str(&receipt).unwrap();
    assert!(parsed.text_range_binding_sha256.is_none());
    assert_eq!(serde_json::to_string(&parsed).unwrap(), receipt);
    let record = json!({"id":"eng_fixed","revision":1,"input_revision":1,"status":"draft","document":document,"snapshot":null,"snapshots":[],"signoffs":[],"signoff_history":[],"lock_history":[],"import_history":[],"created_at":"synthetic","updated_at":"synthetic","rollforward_from":null,"annotation_bindings":[parsed]});
    let e = engine();
    e.store
        .set_json(
            "engagements:state:v1",
            &json!({"engagements":[record],"previews":[],"audit":[]}),
        )
        .await
        .unwrap();
    let read = must(e.get("eng_fixed").await);
    assert_eq!(serde_json::to_value(&read).unwrap(), record);
    assert_eq!(
        read.annotation_bindings[0]
            .account_binding_sha256
            .as_deref(),
        Some(BINDING)
    ); // Load recomputes old canonical binding and checks this pinned baseline.
}

#[tokio::test]
async fn range_lifetime_registry_scope_changes_corruption_and_atomic_capacity() {
    for history in [false, true] {
        let e = engine();
        let r = must(e.save(None, input(fixture())).await);
        let original = mark(&r.id, "range1", 1, "é😀e\u{301}\r\n");
        let mut d = serde_json::to_value(&r.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([original.clone()]);
        let r = must(save(&e, &r, d).await);
        let r = if history {
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
        let receipts = serde_json::to_value(&r.annotation_bindings).unwrap();
        assert!(r.annotation_bindings[0].account_binding_sha256.is_none());
        assert!(r.annotation_bindings[0].text_range_binding_sha256.is_some());
        let mut d = serde_json::to_value(&r.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([]);
        let removed = must(save(&e, &r, d).await);
        for mode in ["body", "range", "account", "kind", "custom", "paper"] {
            let mut changed = original.clone();
            let mut d = serde_json::to_value(&removed.document).unwrap();
            match mode {
                "body" => {
                    changed.as_object_mut().unwrap().remove("text_range_anchor");
                }
                "range" => {
                    changed = mark(&r.id, "range1", 0, "\r");
                }
                "account" => {
                    changed.as_object_mut().unwrap().remove("text_range_anchor");
                    changed["account_anchor"] = json!({"engagement_id":r.id,"account_id":"cash","context_sha256":"a".repeat(64)});
                }
                "kind" => changed["kind"] = json!("agreed"),
                "custom" => {
                    changed["kind"] = json!("custom");
                    changed["custom_mark"] =
                        json!({"id":"m1","code":"QA","label":"Copied","description":"Synthetic"});
                }
                _ => {
                    d["workpapers"][1]["document_type"] = json!("manual");
                    d["workpapers"][1]["body"] = json!(TEXT);
                }
            }
            let paper = if mode == "paper" { 1 } else { 0 };
            d["workpapers"][paper]["annotations"] = json!([changed]);
            let before = e.store.get("engagements:state:v1").await.unwrap();
            assert_eq!(save(&e, &removed, d).await.unwrap_err().0, 409, "{mode}");
            assert_eq!(e.store.get("engagements:state:v1").await.unwrap(), before);
        }
        let mut d = serde_json::to_value(&removed.document).unwrap();
        d["workpapers"][0]["annotations"] = json!([original]);
        let restored = must(save(&e, &removed, d).await);
        assert_eq!(
            serde_json::to_value(&restored.annotation_bindings).unwrap(),
            receipts
        );
        let raw = e.store.get("engagements:state:v1").await.unwrap().unwrap();
        for mode in ["missing", "both", "changed", "copy", "owner"] {
            let mut state: Value = serde_json::from_str(&raw).unwrap();
            let record = &mut state["engagements"][0];
            match mode {
                "missing" => record["annotation_bindings"] = json!([]),
                "both" => {
                    record["annotation_bindings"][0]["account_binding_sha256"] =
                        json!("a".repeat(64))
                }
                "changed" => {
                    record["annotation_bindings"][0]["text_range_binding_sha256"] =
                        json!("0".repeat(64))
                }
                "copy" => {
                    record["document"]["workpapers"][0]["annotations"][0]["text_range_anchor"]["selected_text"] =
                        json!("corrupt")
                }
                _ => {
                    record["document"]["workpapers"][0]["annotations"][0]["text_range_anchor"]["engagement_id"] =
                        json!("eng_other")
                }
            };
            e.store
                .set_json("engagements:state:v1", &state)
                .await
                .unwrap();
            assert_eq!(e.get(&r.id).await.unwrap_err().0, 409);
            e.store.set("engagements:state:v1", &raw).await.unwrap();
        }
    }
    // Fill a legal synthetic server metadata fixture to 1999 IDs; adding two
    // new ranges would append once then fail. The entire Store must stay exact.
    let e = engine();
    let r = must(e.save(None, input(fixture())).await);
    let mut state: Value =
        serde_json::from_str(&e.store.get("engagements:state:v1").await.unwrap().unwrap()).unwrap();
    state["engagements"][0]["annotation_bindings"] = json!(
        (0..1999)
            .map(|n| json!({"annotation_id":format!("reserved{n}")}))
            .collect::<Vec<_>>()
    );
    e.store
        .set_json("engagements:state:v1", &state)
        .await
        .unwrap();
    let r = must(e.get(&r.id).await);
    let before = e.store.get("engagements:state:v1").await.unwrap();
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] =
        json!([mark(&r.id, "new1", 0, "\r"), mark(&r.id, "new2", 0, "\r")]);
    assert_eq!(save(&e, &r, d).await.unwrap_err().0, 409);
    assert_eq!(e.store.get("engagements:state:v1").await.unwrap(), before);
    // A registered body-only ID cannot be removed then upgraded to a range.
    let e = engine();
    let r = must(e.save(None, input(fixture())).await);
    let mut plain = mark(&r.id, "body1", 0, "\r");
    plain.as_object_mut().unwrap().remove("text_range_anchor");
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([plain]);
    let r = must(save(&e, &r, d).await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([]);
    let r = must(save(&e, &r, d).await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([mark(&r.id, "body1", 0, "\r")]);
    assert_eq!(save(&e, &r, d).await.unwrap_err().0, 409);
}

async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, Value, axum::http::HeaderMap) {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("Content-Type", "application/json");
    if auth {
        b = b.header("Authorization", "Bearer text-range-test-key");
    }
    let response = app
        .clone()
        .oneshot(
            b.body(
                body.map(|v| Body::from(v.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let raw = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let v = serde_json::from_slice(&raw)
        .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&raw)}));
    (status, v, headers)
}
#[tokio::test]
async fn http_saved_owner_cas_recorded_copy_print_stale_review_and_rollforward() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "text-range-test-key");
    }
    let app = app_with(Store::memory);
    let (status, _, _) = call(&app, "POST", "/api/engagements", Some(fixture()), false).await;
    assert_eq!(status, 401);
    let (status, r, _) = call(&app, "POST", "/api/engagements", Some(fixture()), true).await;
    assert_eq!(status, 200);
    let path = format!("/api/engagements/{}", r["id"].as_str().unwrap());
    let selected = "<script>x</script>";
    let original = mark(
        r["id"].as_str().unwrap(),
        "range1",
        TEXT.find(selected).unwrap(),
        selected,
    );
    let mut d = r["document"].clone();
    d["expected_revision"] = r["revision"].clone();
    d["workpapers"][0]["annotations"] = json!([
        original.clone(),
        mark(
            r["id"].as_str().unwrap(),
            "range_lf",
            TEXT.find('\n').unwrap(),
            "\n"
        ),
        mark(
            r["id"].as_str().unwrap(),
            "range_crlf",
            TEXT.find("\r\n").unwrap(),
            "\r\n"
        ),
        mark(r["id"].as_str().unwrap(), "range_bare_cr", 0, "\r")
    ]);
    let (status, r, headers) = call(&app, "PUT", &path, Some(d.clone()), true).await;
    assert_eq!(status, 200);
    assert_eq!(headers["cache-control"], "private, no-store");
    assert_eq!(call(&app, "PUT", &path, Some(d), true).await.0, 409);
    let mut foreign = r["document"].clone();
    foreign["expected_revision"] = r["revision"].clone();
    foreign["workpapers"][0]["annotations"][0]["text_range_anchor"]["engagement_id"] =
        json!("eng_foreign");
    assert_eq!(call(&app, "PUT", &path, Some(foreign), true).await.0, 409);
    let (status, r, _) = call(
        &app,
        "POST",
        &(path.clone() + "/calculate"),
        Some(json!({"expected_revision":r["revision"]})),
        true,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(r["snapshot"]["result"]["supported"], true);
    let mut d = r["document"].clone();
    d["expected_revision"] = r["revision"].clone();
    d["workpapers"][0]["body"] = json!("Q");
    let (_, r, _) = call(&app, "PUT", &path, Some(d), true).await;
    let (_, r, _) = call(
        &app,
        "POST",
        &(path.clone() + "/calculate"),
        Some(json!({"expected_revision":r["revision"]})),
        true,
    )
    .await;
    assert_eq!(r["snapshot"]["result"]["supported"], false);
    let (status, print, _) = call(&app, "GET", &(path.clone() + "/print"), None, true).await;
    assert_eq!(status, 200);
    let html = print["text"].as_str().unwrap();
    assert!(html.contains("Recorded selected text"));
    assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("<pre><span>\n</span></pre>"));
    assert!(html.contains("<pre><span>&#13;\n</span></pre>"));
    assert!(html.contains("<pre><span>&#13;<br></span></pre>"));
    assert!(!html.contains("&#13;<br>\n"));
    assert_eq!(call(&app,"POST",&(path.clone()+"/signoffs"),Some(json!({"expected_revision":r["revision"],"workpaper_id":"paper1","stage":"prepared","operator":"Synthetic","note":"Synthetic"})),true).await.0,409);
    let mut d = r["document"].clone();
    d["expected_revision"] = r["revision"].clone();
    let mut fresh = mark(r["id"].as_str().unwrap(), "range2", 0, "Q");
    fresh["body_sha256"] = json!(sha("Q"));
    d["workpapers"][0]["annotations"] = json!([fresh]);
    let (_, r, _) = call(&app, "PUT", &path, Some(d), true).await;
    let (_, mut r, _) = call(
        &app,
        "POST",
        &(path.clone() + "/calculate"),
        Some(json!({"expected_revision":r["revision"]})),
        true,
    )
    .await;
    assert_eq!(r["snapshot"]["result"]["supported"], true);
    for (route, fields) in [
        (
            "signoffs",
            json!({"workpaper_id":"paper1","stage":"prepared","operator":"Synthetic preparer","note":"Synthetic"}),
        ),
        (
            "transition",
            json!({"status":"in_review","operator":"Synthetic reviewer","note":"Synthetic"}),
        ),
        (
            "signoffs",
            json!({"workpaper_id":"paper1","stage":"reviewed","operator":"Synthetic reviewer","note":"Synthetic"}),
        ),
        (
            "transition",
            json!({"status":"locked","operator":"Synthetic reviewer","note":"Synthetic"}),
        ),
    ] {
        let mut v = fields;
        v["expected_revision"] = r["revision"].clone();
        let (s, next, _) = call(&app, "POST", &(path.clone() + "/" + route), Some(v), true).await;
        assert_eq!(s, 200, "{next}");
        r = next;
    }
    let (_, package, _) = call(&app, "GET", &(path.clone() + "/package"), None, true).await;
    assert_eq!(package["engagement"], r);
    assert_eq!(r["annotation_bindings"].as_array().unwrap().len(), 5);
    let(status,next,_)=call(&app,"POST",&(path.clone()+"/rollforward"),Some(json!({"expected_revision":r["revision"],"period_start":"2026-01-01","period_end":"2026-12-31"})),true).await;
    assert_eq!(status, 200);
    assert!(next.get("annotation_bindings").is_none());
    assert!(
        next["document"]["workpapers"][0]
            .get("annotations")
            .is_none()
    );
    assert_eq!(next["rollforward_origin"]["source_snapshot"], r["snapshot"]);
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["annotations"]
            [0]["text_range_anchor"]["engagement_id"],
        r["id"]
    );
    assert_eq!(
        call(&app, "GET", "/api/ledger/entries", None, true).await.1,
        json!([])
    );
}

#[tokio::test]
async fn old_account_ids_cannot_become_ranges_and_retained_range_shapes_use_source_owner() {
    let e = engine();
    let r = must(e.save(None, input(fixture())).await);
    let context =
        account_annotations::context_sha256(&r.document, &r.id, "paper1", "cash").unwrap();
    let mut account = mark(&r.id, "old_account", 0, "\r");
    account.as_object_mut().unwrap().remove("text_range_anchor");
    account["account_anchor"] =
        json!({"engagement_id":r.id,"account_id":"cash","context_sha256":context});
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([account]);
    let r = must(save(&e, &r, d).await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([]);
    let r = must(save(&e, &r, d).await);
    let mut d = serde_json::to_value(&r.document).unwrap();
    d["workpapers"][0]["annotations"] = json!([mark(&r.id, "old_account", 0, "\r")]);
    assert_eq!(save(&e, &r, d.clone()).await.unwrap_err().0, 409);
    d["workpapers"][0]["annotations"][0]["id"] = json!("new_range");
    let r = must(save(&e, &r, d).await);
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
    for field in ["engagement_id", "selection_sha256"] {
        let mut state: Value = serde_json::from_str(&raw).unwrap();
        state["engagements"][0]["snapshots"][0]["input_document"]["workpapers"][0]["annotations"]
            [0]["text_range_anchor"][field] = if field == "engagement_id" {
            json!("eng_foreign")
        } else {
            json!("0".repeat(64))
        };
        e.store
            .set_json("engagements:state:v1", &state)
            .await
            .unwrap();
        assert_eq!(e.get(&r.id).await.unwrap_err().0, 409);
        e.store.set("engagements:state:v1", &raw).await.unwrap();
    }
    let mut r = r;
    for (route, fields) in [
        (
            "sign",
            json!({"workpaper_id":"paper1","stage":"prepared","operator":"Synthetic preparer","note":"Synthetic"}),
        ),
        (
            "transition",
            json!({"status":"in_review","operator":"Synthetic reviewer","note":"Synthetic"}),
        ),
        (
            "sign",
            json!({"workpaper_id":"paper1","stage":"reviewed","operator":"Synthetic reviewer","note":"Synthetic"}),
        ),
        (
            "transition",
            json!({"status":"locked","operator":"Synthetic reviewer","note":"Synthetic"}),
        ),
    ] {
        let mut v = fields;
        v["expected_revision"] = json!(r.revision);
        r = if route == "sign" {
            must(e.signoff(&r.id, serde_json::from_value(v).unwrap()).await)
        } else {
            must(
                e.transition(&r.id, serde_json::from_value(v).unwrap())
                    .await,
            )
        };
    }
    let target=must(e.rollforward(&r.id,serde_json::from_value(json!({"expected_revision":r.revision,"period_start":"2026-01-01","period_end":"2026-12-31"})).unwrap()).await);
    assert!(target.annotation_bindings.is_empty());
    assert_eq!(
        target
            .rollforward_origin
            .as_ref()
            .unwrap()
            .source_snapshot
            .input_document
            .workpapers[0]
            .annotations[0]
            .text_range_anchor
            .as_ref()
            .unwrap()
            .engagement_id,
        r.id
    );
    let raw = e.store.get("engagements:state:v1").await.unwrap().unwrap();
    for field in ["engagement_id", "selection_sha256", "end_utf8_byte"] {
        let mut state: Value = serde_json::from_str(&raw).unwrap();
        let record = state["engagements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|v| v["id"] == target.id)
            .unwrap();
        let value = match field {
            "engagement_id" => json!(target.id),
            "end_utf8_byte" => json!(16385),
            _ => json!("0".repeat(64)),
        };
        record["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["annotations"]
            [0]["text_range_anchor"][field] = value;
        e.store
            .set_json("engagements:state:v1", &state)
            .await
            .unwrap();
        assert_eq!(e.get(&target.id).await.unwrap_err().0, 409);
        e.store.set("engagements:state:v1", &raw).await.unwrap();
    }
    assert_eq!(must(e.get(&target.id).await).id, target.id);
}
