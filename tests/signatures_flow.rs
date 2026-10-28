//! Synthetic original NXT text only. Never contacts DocuSign or recipients.
use axum::{Router, body::Body, http::Request};
use nxt_os::{
    signatures::{self, Signatures},
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

async fn request(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, Value) {
    let mut request = Request::builder().method(method).uri(path);
    if auth {
        request = request.header("authorization", "Bearer k");
    }
    let body = if let Some(body) = body {
        request = request.header("content-type", "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };
    let response = router
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    assert!(
        response
            .headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)})),
    )
}
async fn call(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    expected: u16,
) -> Value {
    let (status, result) = request(router, method, path, body, true).await;
    assert_eq!(status, expected, "{method} {path}: {result}");
    result
}
fn fixture() -> Value {
    json!({"title":"Synthetic & <script>review</script>","email_subject":"Synthetic preparation only","email_message":"No message is sent.","context":{"module":"engagements","record_id":"synthetic-engagement","label":"Synthetic context, unverified"},"documents":[{"id":"doc1","name":"Original synthetic agreement","pages":["Original synthetic text.\n<script>remains plain text</script>","Second original page."]}],"recipients":[{"id":"role1","role_name":"Client signer","name":"Synthetic Signer","email":"signer@example.invalid","kind":"signer","routing_order":1},{"id":"role2","role_name":"Copy recipient","name":"Synthetic Copy","email":"copy@example.invalid","kind":"copy","routing_order":2}],"fields":[{"id":"field1","document_id":"doc1","recipient_id":"role1","page":2,"kind":"signature","label":"Unfilled signature placeholder","required":true,"x":1000,"y":7500,"width":3500,"height":600}]})
}
fn transition(record: &Value, action: &str) -> Value {
    json!({"expected_revision":record["envelope"]["revision"],"expected_content_hash":record["envelope"]["content_hash"],"operator_label":"Synthetic reviewer label","action":action,"note":"Synthetic internal review only. No verified identity or signature."})
}

fn rechain(value: &mut Value) {
    // Simulate structurally consistent audit corruption to test the separate
    // transition/current-record checks, not just a broken SHA-256 chain.
    let mut previous = None;
    for value in value["events"].as_array_mut().unwrap() {
        let mut event: signatures::models::AuditEvent =
            serde_json::from_value(value.clone()).unwrap();
        event.previous_event_hash = previous;
        event.event_hash.clear();
        event.event_hash = hex::encode(Sha256::digest(serde_json::to_vec(&event).unwrap()));
        previous = Some(event.event_hash.clone());
        *value = serde_json::to_value(event).unwrap();
    }
}
fn rebind_record_hash(value: &mut Value, index: usize) {
    let draft: signatures::models::Draft =
        serde_json::from_value(value["envelopes"][index]["draft"].clone()).unwrap();
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(&draft).unwrap()));
    value["envelopes"][index]["content_hash"] = json!(digest);
    value["envelopes"][index]["reviewed_content_hash"] = json!(digest);
    let record_id = value["envelopes"][index]["id"].clone();
    for event in value["events"].as_array_mut().unwrap() {
        if event["record_id"] == record_id {
            event["content_hash"] = json!(digest);
        }
    }
    rechain(value);
}
async fn corruption_checks(state: &Signatures, router: &Router, path: &str, template_path: &str) {
    const KEY: &str = "signatures:state:v1";
    let baseline = state.store.get(KEY).await.unwrap().unwrap();
    let good: Value = serde_json::from_str(&baseline).unwrap();
    let record_id = path.rsplit('/').next().unwrap();
    let index = good["envelopes"]
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r["id"] == record_id)
        .unwrap();
    let mut cases: Vec<(&str, Value)> = Vec::new();
    let mut bad = good.clone();
    bad["envelopes"][index]["draft"]["documents"][0]["pages"][0] =
        json!("Corrupted synthetic stored text.");
    cases.push(("modified approved text", bad.clone()));
    let draft: signatures::models::Draft =
        serde_json::from_value(bad["envelopes"][index]["draft"].clone()).unwrap();
    let changed_hash = hex::encode(Sha256::digest(serde_json::to_vec(&draft).unwrap()));
    bad["envelopes"][index]["content_hash"] = json!(changed_hash);
    bad["envelopes"][index]["reviewed_content_hash"] = json!(changed_hash);
    cases.push(("rehashing content cannot bypass audit binding", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["draft"]["fields"][0]["page"] = json!(3);
    rebind_record_hash(&mut bad, index);
    cases.push(("invalid native layout despite consistent hashes", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["draft"]["fields"][0]["required"] = json!(false);
    rebind_record_hash(&mut bad, index);
    cases.push((
        "approved readiness remains required with consistent hashes",
        bad,
    ));
    let mut bad = good.clone();
    bad["envelopes"][index]["status"] = json!("sent");
    cases.push(("unknown external status", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["revision"] = json!(0);
    cases.push(("zero revision", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["revision"] = json!(u64::MAX);
    cases.push(("overflow revision", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["reviewed_content_hash"] = Value::Null;
    cases.push(("missing approved content binding", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["reviewed_by_label"] = json!("Different synthetic label");
    cases.push(("reviewer label must match approval event", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["created_at"] = json!("invalid date");
    cases.push(("invalid record metadata", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["template_id"] = json!("missing-template");
    cases.push(("dangling template reference", bad));
    let mut bad = good.clone();
    let duplicate = bad["envelopes"][index].clone();
    bad["envelopes"].as_array_mut().unwrap().push(duplicate);
    cases.push(("duplicate envelope id", bad));
    let mut bad = good.clone();
    bad["envelopes"][index]["signature_value"] = json!("forged completion");
    cases.push(("unknown persisted record property", bad));
    let mut bad = good.clone();
    bad["external_send_enabled"] = json!(true);
    cases.push(("unknown persisted root property", bad));
    let mut bad = good.clone();
    bad["templates"][0]["draft"]["documents"][0]["pages"][0] = json!("Corrupted template text.");
    cases.push(("modified native template content", bad));
    let mut bad = good.clone();
    bad["templates"][0]["version"] = json!(2);
    cases.push(("unsupported template version", bad));
    let mut bad = good.clone();
    bad["templates"][0]["draft"]["recipients"][0]["email"] = json!("retained@example.invalid");
    let draft: signatures::models::Draft =
        serde_json::from_value(bad["templates"][0]["draft"].clone()).unwrap();
    let changed_hash = hex::encode(Sha256::digest(serde_json::to_vec(&draft).unwrap()));
    bad["templates"][0]["content_hash"] = json!(changed_hash);
    let template_id = bad["templates"][0]["id"].clone();
    for event in bad["events"].as_array_mut().unwrap() {
        if event["record_id"] == template_id {
            event["content_hash"] = json!(changed_hash);
        }
    }
    rechain(&mut bad);
    cases.push((
        "template roles cannot retain identity even with matching hashes",
        bad,
    ));
    let mut bad = good.clone();
    bad["events"][0]["note"] = json!("Changed synthetic audit note.");
    cases.push(("modified audit hash material", bad));
    let mut bad = good.clone();
    bad["events"][1]["previous_event_hash"] = Value::Null;
    cases.push(("broken global audit chain", bad));
    let mut bad = good.clone();
    bad["events"] = json!([]);
    cases.push(("missing audit history", bad));
    let mut bad = good.clone();
    bad["events"]
        .as_array_mut()
        .unwrap()
        .retain(|e| !(e["record_id"] == record_id && e["action"] == "approve"));
    rechain(&mut bad);
    cases.push(("removed approval with rehashed remaining history", bad));
    let mut bad = good.clone();
    for event in bad["events"].as_array_mut().unwrap() {
        if event["record_id"] == record_id && event["action"] == "approve" {
            event["action"] = json!("completed");
        }
    }
    rechain(&mut bad);
    cases.push(("unknown action with a valid hash chain", bad));
    let mut bad = good.clone();
    let last = bad["events"].as_array().unwrap().last().unwrap().clone();
    bad["events"].as_array_mut().unwrap().push(last);
    rechain(&mut bad);
    cases.push(("duplicate event identity", bad));
    let mut bad = good.clone();
    bad["events"] = Value::Array(vec![good["events"][0].clone(); 2001]);
    cases.push(("persisted event capacity", bad));
    for (name, bad) in cases {
        let corrupted = bad.to_string();
        state.store.set(KEY, &corrupted).await.unwrap();
        for endpoint in [
            path,
            &format!("{path}/package"),
            template_path,
            "/api/signatures",
            "/api/signatures/audit",
        ] {
            let (status, body) = request(router, "GET", endpoint, None, true).await;
            assert_eq!(
                status, 500,
                "corruption must fail closed: {name} {endpoint}: {body}"
            );
            assert_eq!(body["error"], "signature preparation storage unavailable");
        }
        call(
            router,
            "POST",
            "/api/signatures",
            Some(json!({"operator_label":"Synthetic author","draft":fixture()})),
            500,
        )
        .await;
        assert_eq!(
            state.store.get(KEY).await.unwrap().as_deref(),
            Some(corrupted.as_str()),
            "failed reads/writes must preserve evidence: {name}"
        );
        state.store.set(KEY, &baseline).await.unwrap();
        call(router, "GET", &format!("{path}/package"), None, 200).await;
    }
}

#[tokio::test]
async fn preparation_lifecycle_blocks_signature_claims_and_stale_changes() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
    }
    let state = Arc::new(Signatures::new(Store::memory()));
    let router = signatures::router(state.clone());
    assert_eq!(
        request(&router, "GET", "/api/signatures", None, false)
            .await
            .0,
        401
    );
    let catalog = call(&router, "GET", "/api/signatures/catalog", None, 200).await;
    assert_eq!(catalog["external_send_enabled"], false);
    assert_eq!(catalog["signing_enabled"], false);
    assert_eq!(catalog["connector"]["status"], "unconfigured");
    assert_eq!(catalog["source_parity"], "unverified");
    unsafe {
        std::env::set_var("ADMIN_API_KEY", " ");
    }
    assert_eq!(
        request(&router, "GET", "/api/signatures", None, true)
            .await
            .0,
        503
    );
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
    }

    let mut invalid = fixture();
    invalid["fields"][0]["page"] = json!(3);
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":invalid})),
        400,
    )
    .await;
    let mut invalid = fixture();
    invalid["fields"][0]["x"] = json!(9999);
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":invalid})),
        400,
    )
    .await;
    let mut invalid = fixture();
    invalid["fields"][0]["recipient_id"] = json!("role2");
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":invalid})),
        400,
    )
    .await;
    let mut invalid = fixture();
    invalid["recipients"][1]["id"] = json!("role1");
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":invalid})),
        400,
    )
    .await;
    let mut invalid = fixture();
    invalid["recipients"][0]["email"] = json!("bad\r\nemail@example.invalid");
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":invalid})),
        400,
    )
    .await;
    let mut invalid = fixture();
    invalid["documents"][0]["pages"][0] = json!("x".repeat(16001));
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":invalid})),
        400,
    )
    .await;
    let mut forged = fixture();
    forged["fields"][0]["signature_value"] = json!("fake signed value");
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":forged})),
        422,
    )
    .await;
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":fixture(),"status":"completed"})),
        422,
    )
    .await;
    call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","expected_revision":1,"draft":fixture()})),
        400,
    )
    .await;

    let incomplete = json!({"title":"Incomplete synthetic draft","email_subject":"","email_message":"","context":null,"documents":[],"recipients":[],"fields":[]});
    let partial = call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":incomplete})),
        200,
    )
    .await;
    assert_eq!(partial["readiness_findings"].as_array().unwrap().len(), 3);
    let partial_path = format!(
        "/api/signatures/{}",
        partial["envelope"]["id"].as_str().unwrap()
    );
    call(
        &router,
        "POST",
        &format!("{partial_path}/transition"),
        Some(transition(&partial, "submit_review")),
        409,
    )
    .await;

    let created = call(
        &router,
        "POST",
        "/api/signatures",
        Some(json!({"operator_label":"Synthetic author","draft":fixture()})),
        200,
    )
    .await;
    let path = format!(
        "/api/signatures/{}",
        created["envelope"]["id"].as_str().unwrap()
    );
    assert_eq!(created["readiness_findings"], json!([]));
    assert_eq!(created["envelope"]["draft"], fixture());
    call(&router, "GET", &format!("{path}/package"), None, 409).await;
    // An unsupported completion action cannot manufacture an external status.
    call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&created, "completed")),
        409,
    )
    .await;
    let review = call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&created, "submit_review")),
        200,
    )
    .await;
    assert_eq!(review["envelope"]["status"], "in_review");
    call(
        &router,
        "PUT",
        &path,
        Some(json!({"operator_label":"Synthetic author","expected_revision":2,"draft":fixture()})),
        409,
    )
    .await;
    call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&created, "approve")),
        409,
    )
    .await;
    let mut forged = transition(&review, "approve");
    forged["expected_content_hash"] = json!("0".repeat(64));
    call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(forged),
        409,
    )
    .await;
    let approved = call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&review, "approve")),
        200,
    )
    .await;
    assert_eq!(approved["envelope"]["status"], "approved");
    assert_eq!(
        approved["envelope"]["reviewed_content_hash"],
        approved["envelope"]["content_hash"]
    );
    let package = call(&router, "GET", &format!("{path}/package"), None, 200).await;
    assert_eq!(package["format"], "nxt-signature-preparation/v1");
    assert_eq!(package["contains_signatures"], false);
    assert_eq!(package["external_send_enabled"], false);
    assert_eq!(
        package["envelope"]["draft"]["documents"][0]["pages"][0],
        fixture()["documents"][0]["pages"][0]
    );
    assert!(
        package["identity_limit"]
            .as_str()
            .unwrap()
            .contains("not verified")
    );

    let template=call(&router,"POST",&format!("{path}/template"),Some(json!({"expected_revision":approved["envelope"]["revision"],"expected_content_hash":approved["envelope"]["content_hash"],"operator_label":"Synthetic author","name":"Synthetic reusable native template"})),200).await;
    assert_eq!(template["template"]["draft"]["context"], Value::Null);
    assert_eq!(template["template"]["draft"]["recipients"][0]["name"], "");
    assert_eq!(template["template"]["draft"]["recipients"][0]["email"], "");
    assert_eq!(
        template["template"]["draft"]["documents"],
        fixture()["documents"]
    );
    let template_path = format!(
        "/api/signatures/templates/{}",
        template["template"]["id"].as_str().unwrap()
    );
    call(&router, "GET", &template_path, None, 200).await;
    corruption_checks(&state, &router, &path, &template_path).await;
    let instantiate = json!({"expected_template_hash":template["template"]["content_hash"],"operator_label":"Synthetic new author","title":"New synthetic preparation","recipients":[{"id":"role1","name":"New Synthetic Signer","email":"new@example.invalid"}]});
    let mut wrong = instantiate.clone();
    wrong["expected_template_hash"] = json!("0".repeat(64));
    call(
        &router,
        "POST",
        &format!("{template_path}/instantiate"),
        Some(wrong),
        409,
    )
    .await;
    let mut wrong = instantiate.clone();
    wrong["recipients"][0]["id"] = json!("unknown-role");
    call(
        &router,
        "POST",
        &format!("{template_path}/instantiate"),
        Some(wrong),
        400,
    )
    .await;
    let mut wrong = instantiate.clone();
    let duplicate = wrong["recipients"][0].clone();
    wrong["recipients"].as_array_mut().unwrap().push(duplicate);
    call(
        &router,
        "POST",
        &format!("{template_path}/instantiate"),
        Some(wrong),
        400,
    )
    .await;
    let copied = call(
        &router,
        "POST",
        &format!("{template_path}/instantiate"),
        Some(instantiate),
        200,
    )
    .await;
    assert_eq!(copied["envelope"]["status"], "draft");
    assert_ne!(copied["envelope"]["id"], approved["envelope"]["id"]);
    assert_eq!(
        copied["envelope"]["draft"]["recipients"][0]["name"],
        "New Synthetic Signer"
    );
    assert!(!copied["readiness_findings"].as_array().unwrap().is_empty());
    let reopened = call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&approved, "return_to_draft")),
        200,
    )
    .await;
    assert_eq!(reopened["envelope"]["reviewed_content_hash"], Value::Null);
    call(&router, "GET", &format!("{path}/package"), None, 409).await;
    let mut changed = fixture();
    changed["documents"][0]["pages"][0] = json!("Changed original synthetic content.");
    let update = json!({"operator_label":"Synthetic author","expected_revision":reopened["envelope"]["revision"],"draft":changed});
    let ((a, av), (b, bv)) = tokio::join!(
        request(&router, "PUT", &path, Some(update.clone()), true),
        request(&router, "PUT", &path, Some(update), true)
    );
    assert!(
        (a == 200 && b == 409) || (a == 409 && b == 200),
        "concurrent revisions: {a} {b}"
    );
    let updated = if a == 200 { av } else { bv };
    assert_ne!(
        updated["envelope"]["content_hash"],
        created["envelope"]["content_hash"]
    );
    let cancelled = call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&updated, "cancel")),
        200,
    )
    .await;
    call(
        &router,
        "POST",
        &format!("{path}/transition"),
        Some(transition(&cancelled, "return_to_draft")),
        409,
    )
    .await;
    let audit = call(&router, "GET", "/api/signatures/audit?limit=200", None, 200).await;
    // The production wildcard rewrite adds path; accept only this audit route
    // and preserve every existing query and pagination validation rule.
    for rewrite_path in [
        "signatures%2Faudit",
        "%2Fsignatures%2Faudit",
        "api%2Fsignatures%2Faudit",
        "%2Fapi%2Fsignatures%2Faudit",
    ] {
        let url = format!("/api/signatures/audit?limit=200&path={rewrite_path}");
        assert_eq!(call(&router, "GET", &url, None, 200).await, audit);
    }
    for query in [
        "path=signatures%2Fother-record",
        "path=migrations%2Faudit",
        "path=signatures%2Faudit%2Fextra",
        "path=signatures%2Faudit&unexpected=1",
        "unexpected=1",
        "path=signatures%2Faudit&limit=0",
        "path=signatures%2Faudit&limit=201",
    ] {
        call(
            &router,
            "GET",
            &format!("/api/signatures/audit?{query}"),
            None,
            400,
        )
        .await;
    }
    let events = audit["events"].as_array().unwrap();
    assert_eq!(events.len(), 9);
    let mut previous = Value::Null;
    for event in events {
        assert_eq!(event["previous_event_hash"], previous);
        let mut typed: signatures::models::AuditEvent =
            serde_json::from_value(event.clone()).unwrap();
        typed.event_hash.clear();
        let expected = hex::encode(Sha256::digest(serde_json::to_vec(&typed).unwrap()));
        assert_eq!(event["event_hash"], expected);
        previous = event["event_hash"].clone();
    }
    call(&router, "GET", "/api/signatures/audit?limit=201", None, 400).await;
    call(&router, "GET", "/api/signatures/audit?limit=0", None, 400).await;
    let summary = call(&router, "GET", "/api/signatures", None, 200).await;
    assert_eq!(summary["envelopes"].as_array().unwrap().len(), 3);
    assert_eq!(summary["templates"].as_array().unwrap().len(), 1);
    assert_eq!(summary["templates"][0]["roles"][0]["email"], "");
    assert_eq!(summary["audit_events"], 9);
}
