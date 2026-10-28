use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", "Bearer k");
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn ok(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status, value) = call(r, method, uri, body).await;
    assert_eq!(status, 200, "{method} {uri} -> {value}");
    value
}

fn entry_count(entries: &Value) -> usize {
    entries.as_array().map(|a| a.len()).unwrap_or(0)
}

#[tokio::test]
async fn nixon_reviews_the_books_and_does_not_change_them() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let status = ok(&r, "GET", "/api/nixon/status", None).await;
    assert_eq!(status["posting"], "disabled");
    assert_eq!(status["filing"], "disabled");
    assert_eq!(status["surfaces"]["ledger"], "installed");
    assert_eq!(status["surfaces"]["firm"], "installed");
    assert_eq!(status["surfaces"]["tax"], "installed");
    assert_eq!(status["surfaces"]["migration"], "installed");

    let empty = ok(
        &r,
        "POST",
        "/api/nixon/reviews",
        Some(json!({ "idempotency_key": "empty-books" })),
    )
    .await;
    assert_eq!(empty["posting"], "disabled");
    assert!(
        empty["verifications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "cutover" && v["state"] == "not_run")
    );
    assert!(
        empty["verifications"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "firm_module" && v["state"] == "not_run")
    );
    let again = ok(
        &r,
        "POST",
        "/api/nixon/reviews",
        Some(json!({ "idempotency_key": "empty-books" })),
    )
    .await;
    assert_eq!(again["id"], empty["id"]);

    ok(&r, "POST", "/api/ledger/entries", Some(json!({"date":"2026-09-02","memo":"Owner investment","lines":[{"account_id":"acc_1000","debit":500000},{"account_id":"acc_3000","credit":500000}]}))).await;
    ok(&r, "POST", "/api/ledger/entries", Some(json!({"date":"2026-09-02","memo":"Owner investment","lines":[{"account_id":"acc_1000","debit":500000},{"account_id":"acc_3000","credit":500000}]}))).await;
    ok(&r, "POST", "/api/ledger/money/out", Some(json!({"account_id":"acc_1000","date":"2026-09-15","memo":"Rent","amounts_include_tax":false,"lines":[{"account_id":"acc_6500","amount":40000,"tax_code":"NONE"}]}))).await;
    ok(&r, "POST", "/api/ledger/money/out", Some(json!({"account_id":"acc_1000","date":"2026-10-15","memo":"Rent","amounts_include_tax":false,"lines":[{"account_id":"acc_6500","amount":120000,"tax_code":"NONE"}]}))).await;
    ok(&r, "POST", "/api/ledger/feed/import", Some(json!({"account_id":"acc_1000","csv":"Date,Description,Amount\n2026-10-20,ADOBE *CREATIVE CLOUD,-56.49\n"}))).await;

    let before = ok(&r, "GET", "/api/ledger/entries", None).await;
    let before_n = entry_count(&before);

    let (conflict, _) = call(
        &r,
        "POST",
        "/api/nixon/reviews",
        Some(json!({ "idempotency_key": "empty-books" })),
    )
    .await;
    assert_eq!(
        conflict, 409,
        "the empty-books key must not be reused for a new snapshot"
    );

    let review = ok(
        &r,
        "POST",
        "/api/nixon/reviews",
        Some(json!({
            "idempotency_key": "october-review",
            "cutover": {
                "source_class": "customer_operational",
                "as_of": "2026-10-28",
                "currency": "CAD",
                "period_lock": "2026-09-30",
                "prepared_by": "Casey Quinn",
                "label": "Source trial balance",
                "lines": [
                    {"account_code": "1000", "debit": 100, "credit": 0},
                    {"account_code": "9999", "debit": 0, "credit": 40},
                    {"account_code": "3000", "debit": 0, "credit": 50}
                ]
            },
            "surfaces": [{
                "surface": "tax",
                "source_class": "simulated",
                "as_of": "2026-10-28",
                "prepared_by": "Casey Quinn",
                "filing_requested": true,
                "items": [{"id": "t2", "label": "T2 workpaper", "state": "unreviewed"}]
            }, {
                "surface": "firm",
                "source_class": "simulated",
                "as_of": "2026-10-28",
                "prepared_by": "Casey Quinn",
                "items": [{"id": "close-bank", "label": "Bank reconciliation", "state": "open"}]
            }]
        })),
    )
    .await;

    assert_eq!(
        entry_count(&ok(&r, "GET", "/api/ledger/entries", None).await),
        before_n
    );
    let findings = review["findings"].as_array().unwrap();
    let cutover = findings.iter().find(|f| f["id"] == "f_cutover").unwrap();
    assert!(
        cutover["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["reference"] == "cutover:9999")
    );
    assert!(
        cutover["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["reference"] != "account:acc_9999")
    );
    let shift = findings
        .iter()
        .find(|f| f["id"] == "f_shift_screen")
        .unwrap();
    assert_eq!(shift["hypotheses"][0]["label"], "hypothesis");
    assert!(shift["hypotheses"][0]["confidence"].is_null());
    assert!(
        !shift["summary"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("root cause")
    );
    assert!(findings.iter().any(|f| f["id"] == "f_pending_feed"));
    assert!(findings.iter().any(|f| f["id"] == "f_possible_duplicate"));
    assert!(findings.iter().any(|f| f["capability"] == "cash_flow"));
    let filing = review["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["kind"] == "file_return")
        .unwrap();
    assert_eq!(filing["status"], "blocked");
    assert_eq!(filing["applied"], false);
    assert_eq!(filing["execution"], "disabled");

    let review_id = review["id"].as_str().unwrap();
    let cutover_proposal = review["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["finding_id"] == "f_cutover")
        .unwrap();
    let (denied, denied_body) = call(
        &r,
        "POST",
        &format!(
            "/api/nixon/reviews/{review_id}/proposals/{}/decide",
            cutover_proposal["id"].as_str().unwrap()
        ),
        Some(
            json!({ "actor_name": "Casey Quinn", "decision": "approve", "note": "I prepared it" }),
        ),
    )
    .await;
    assert_eq!(denied, 403, "{denied_body}");

    let (blocked, blocked_body) = call(
        &r,
        "POST",
        &format!(
            "/api/nixon/reviews/{review_id}/proposals/{}/decide",
            filing["id"].as_str().unwrap()
        ),
        Some(json!({ "actor_name": "Riley Morgan", "decision": "approve" })),
    )
    .await;
    assert_eq!(blocked, 409, "{blocked_body}");

    let feed = review["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["finding_id"] == "f_pending_feed")
        .unwrap();
    let decided = ok(
        &r,
        "POST",
        &format!("/api/nixon/reviews/{review_id}/proposals/{}/decide", feed["id"].as_str().unwrap()),
        Some(json!({ "actor_name": "Riley Morgan", "decision": "approve", "note": "I will categorize this in the bank feed." })),
    )
    .await;
    let decided_feed = decided["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == feed["id"])
        .unwrap();
    assert_eq!(decided_feed["status"], "approved_no_execution");
    assert_eq!(decided_feed["applied"], false);
    assert!(
        decided["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["verification_state"] == "not_verification")
    );
    assert_eq!(
        entry_count(&ok(&r, "GET", "/api/ledger/entries", None).await),
        before_n
    );

    let (exec, exec_body) = call(
        &r,
        "POST",
        &format!(
            "/api/nixon/reviews/{review_id}/proposals/{}/execute",
            feed["id"].as_str().unwrap()
        ),
        Some(json!({ "actor_name": "Riley Morgan" })),
    )
    .await;
    assert_eq!(exec, 403, "{exec_body}");
    assert_eq!(
        entry_count(&ok(&r, "GET", "/api/ledger/entries", None).await),
        before_n
    );

    let gate = ok(&r, "POST", "/api/nixon/actions", Some(json!({
        "kind": "post_journal",
        "actor_name": "Riley Morgan",
        "autonomy_level": "controlled",
        "note": "please book the difference",
        "payload": {"date":"2026-10-28","memo":"Nixon plug","lines":[{"account_id":"acc_1000","debit":1},{"account_id":"acc_3000","credit":1}]}
    }))).await;
    assert_eq!(gate["applied"], false);
    assert_eq!(gate["execution"], "disabled");
    assert_eq!(gate["status"], "blocked");
    assert_eq!(gate["autonomy_applied"], "advisory");
    assert_eq!(gate["autonomy_requested"], "controlled");
    let entries = ok(&r, "GET", "/api/ledger/entries", None).await;
    assert_eq!(entry_count(&entries), before_n);
    assert!(
        entries
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["memo"] != "Nixon plug")
    );

    let (role, _) = call(
        &r,
        "POST",
        "/api/nixon/actions",
        Some(json!({ "kind": "file_return", "actor_name": "Nixon" })),
    )
    .await;
    assert_eq!(role, 400);

    let audit = ok(&r, "GET", "/api/ledger/audit", None).await;
    let correlation = review["correlation_id"].as_str().unwrap();
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "nixon.review"
                && e["detail"].as_str().unwrap().contains(correlation))
    );
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "nixon.decision"
                && e["detail"].as_str().unwrap().contains("execution=disabled"))
    );
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "nixon.execution_refused")
    );
    assert!(audit.as_array().unwrap().iter().any(|e| {
        e["action"] == "nixon.action_blocked"
            && e["detail"]
                .as_str()
                .unwrap()
                .contains("autonomy_downgraded=true")
    }));
}

#[tokio::test]
async fn voice_can_ask_and_cannot_approve_or_post() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    ok(&r, "POST", "/api/ledger/entries", Some(json!({"date":"2026-10-02","memo":"Owner investment","lines":[{"account_id":"acc_1000","debit":500000},{"account_id":"acc_3000","credit":500000}]}))).await;
    let before = ok(&r, "GET", "/api/ledger/entries", None).await.as_array().unwrap().len();

    let cash = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"what is our cash position","channel":"push_to_talk"}))).await;
    assert_eq!(cash["intent"], "cash");
    assert_eq!(cash["posting"], "disabled");
    assert_eq!(cash["approval"], "not_by_voice");
    assert!(cash["say"].as_str().unwrap().contains('$'));
    assert!(!cash["say"].as_str().unwrap().to_lowercase().contains("root cause"));

    let gaps = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"show reconciliation gaps","channel":"open_mic"}))).await;
    assert_eq!(gaps["intent"], "reconciliation");
    assert!(gaps["say"].as_str().unwrap().to_lowercase().contains("invent") || gaps["say"].as_str().unwrap().to_lowercase().contains("gap") || gaps["say"].as_str().unwrap().to_lowercase().contains("reconcil"));

    let review = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"Nixon, run a review","channel":"push_to_talk"}))).await;
    assert_eq!(review["intent"], "run_review");
    assert_eq!(review["effect"], "review_recorded");
    assert!(review["review_id"].is_string());
    assert_eq!(ok(&r, "GET", "/api/ledger/entries", None).await.as_array().unwrap().len(), before);

    let explained = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"explain the finding","channel":"push_to_talk"}))).await;
    assert_eq!(explained["intent"], "explain");
    assert!(!explained["say"].as_str().unwrap().is_empty());

    let approve = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"approve the finding","channel":"push_to_talk"}))).await;
    assert_eq!(approve["intent"], "refuse_approve");
    assert_eq!(approve["effect"], "none");
    let stored = ok(&r, "GET", &format!("/api/nixon/reviews/{}", review["review_id"].as_str().unwrap()), None).await;
    assert!(stored["decisions"].as_array().unwrap().is_empty());

    let post = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"post a journal for the difference","channel":"push_to_talk"}))).await;
    assert_eq!(post["intent"], "refuse_post");
    let file = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript":"file the tax return","channel":"push_to_talk"}))).await;
    assert_eq!(file["intent"], "refuse_file");
    assert_eq!(ok(&r, "GET", "/api/ledger/entries", None).await.as_array().unwrap().len(), before);

    let echo = ok(&r, "POST", "/api/nixon/voice", Some(json!({"transcript": review["say"], "channel":"open_mic", "last_spoken": review["say"]}))).await;
    assert_eq!(echo["action"], "hold");
    assert_eq!(echo["intent"], "echo");

    let audit = ok(&r, "GET", "/api/ledger/audit", None).await;
    assert!(audit.as_array().unwrap().iter().any(|e| e["action"] == "nixon.voice" && e["detail"].as_str().unwrap().contains("approval=not_by_voice")));
}
