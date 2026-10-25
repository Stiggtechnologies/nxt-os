//! HTTP acceptance coverage for the Firm Operations module.
//! All records here are synthetic; no external connector is invoked.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn raw(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    authenticated: bool,
) -> (u16, String, String) {
    let mut request = Request::builder().method(method).uri(uri);
    if authenticated {
        request = request.header("authorization", "Bearer k");
    }
    let request = match body {
        Some(value) => request.header("content-type", "application/json").body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, content_type, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let (status, _, text) = raw(router, method, uri, body, true).await;
    (status, serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text})))
}

async fn ok(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status, value) = call(router, method, uri, body).await;
    assert_eq!(status, 200, "{method} {uri}: {value}");
    value
}

fn id(value: &Value) -> &str {
    value["id"].as_str().expect("record has an id")
}

/// Keep module acceptance in one test so process-wide authentication setup
/// cannot race another test in this integration-test executable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn firm_operations_lifecycle() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);

    let (status, _, _) = raw(&router, "GET", "/api/firm/summary", None, false).await;
    assert_eq!(status, 401, "firm data must use workspace authentication");
    let (status, _, _) = raw(&router, "POST", "/api/firm/proposals", Some(json!({
        "name": "Unauthorized proposal", "idempotency_key": "unauthorized-event",
        "action": {"kind": "create_task", "title": "Must never be proposed"}
    })), false).await;
    assert_eq!(status, 401, "agent writes must also use workspace authentication");
    assert_eq!(ok(&router, "GET", "/api/firm/summary", None).await["clients_deferred"], true);
    assert_eq!(ok(&router, "GET", "/api/firm/projects", None).await, json!([]));

    // Reusable template -> project -> persisted task state and progress.
    let template = ok(&router, "POST", "/api/firm/templates", Some(json!({
        "name": "Acceptance bookkeeping",
        "description": "Synthetic firm checklist",
        "tasks": [
            {"title": "Review, reconcile and document", "assignee": "Preparer", "due_offset_days": -2},
            {"title": "Review exceptions", "assignee": "Reviewer", "due_offset_days": 0}
        ]
    }))).await;
    let project = ok(&router, "POST", &format!("/api/firm/templates/{}/instantiate", id(&template)), Some(json!({
        "name": "Synthetic monthly work", "owner": "Preparer", "due_date": "2026-11-30"
    }))).await;
    assert_eq!(project["template_id"], template["id"]);
    assert_eq!(project["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(project["tasks"][0]["due_date"], "2026-11-28");
    assert_eq!(project["status"], "open");
    let project_id = id(&project).to_string();
    for task in project["tasks"].as_array().unwrap() {
        ok(&router, "PUT", &format!("/api/firm/projects/{project_id}/tasks/{}", id(task)), Some(json!({"completed": true}))).await;
    }
    let finished = ok(&router, "GET", &format!("/api/firm/projects/{project_id}"), None).await;
    assert_eq!(finished["status"], "completed");
    assert!(finished["tasks"].as_array().unwrap().iter().all(|task| task["completed_at"].is_string()));
    let retained_tasks: Vec<Value> = finished["tasks"].as_array().unwrap().iter().map(|task| json!({
        "id": task["id"], "title": task["title"], "assignee": task["assignee"], "due_date": task["due_date"]
    })).collect();
    let edited = ok(&router, "PUT", &format!("/api/firm/projects/{project_id}"), Some(json!({
        "name": "Synthetic monthly work", "owner": "New project owner", "due_date": "2026-11-30", "tasks": retained_tasks
    }))).await;
    assert_eq!(edited["owner"], "New project owner");
    assert_eq!(edited["status"], "completed");
    assert_eq!(edited["tasks"], finished["tasks"], "metadata edit must retain task identities and completion history");
    let (status, _) = call(&router, "PUT", &format!("/api/firm/projects/{project_id}"), Some(json!({
        "name": "Foreign task identity", "tasks": [{"id": "task_not_in_this_project", "title": "Unknown"}]
    }))).await;
    assert_eq!(status, 400);
    let (status, _) = call(&router, "PUT", &format!("/api/firm/projects/{project_id}"), Some(json!({
        "name": "Duplicate task identity", "tasks": [
            {"id": finished["tasks"][0]["id"], "title": "First"},
            {"id": finished["tasks"][0]["id"], "title": "Duplicate"}
        ]
    }))).await;
    assert_eq!(status, 400);
    let (status, _) = call(&router, "POST", "/api/firm/projects", Some(json!({"name": "Invalid date", "due_date": "2026-02-30"}))).await;
    assert_eq!(status, 400);

    // Independent concurrent requests retain exactly-once operational effects.
    let concurrent_workflow = ok(&router, "POST", "/api/firm/workflows", Some(json!({
        "name": "Concurrent internal task", "trigger": "manual", "requires_approval": true,
        "action": {"kind": "create_task", "title": "One task despite concurrent retries"}
    }))).await;
    let concurrent_run_path = format!("/api/firm/workflows/{}/run", id(&concurrent_workflow));
    let router_a = router.clone();
    let router_b = router.clone();
    let path_a = concurrent_run_path.clone();
    let path_b = concurrent_run_path.clone();
    let (run_a, run_b) = tokio::join!(
        tokio::spawn(async move { ok(&router_a, "POST", &path_a, Some(json!({"idempotency_key": "concurrent-event"}))).await }),
        tokio::spawn(async move { ok(&router_b, "POST", &path_b, Some(json!({"idempotency_key": "concurrent-event"}))).await })
    );
    let run_a = run_a.unwrap();
    let run_b = run_b.unwrap();
    assert_eq!(run_a["id"], run_b["id"]);
    assert_eq!(run_a["approval_id"], run_b["approval_id"]);
    let projects_before = ok(&router, "GET", "/api/firm/projects", None).await.as_array().unwrap().len();
    let router_a = router.clone();
    let router_b = router.clone();
    let path_a = format!("/api/firm/approvals/{}/decide", run_a["approval_id"].as_str().unwrap());
    let path_b = path_a.clone();
    let (decision_a, decision_b) = tokio::join!(
        tokio::spawn(async move { ok(&router_a, "POST", &path_a, Some(json!({"decision": "approve"}))).await }),
        tokio::spawn(async move { ok(&router_b, "POST", &path_b, Some(json!({"decision": "approve"}))).await })
    );
    assert_eq!(decision_a.unwrap()["status"], "executed");
    assert_eq!(decision_b.unwrap()["status"], "executed");
    assert_eq!(ok(&router, "GET", "/api/firm/projects", None).await.as_array().unwrap().len(), projects_before + 1);

    // Proposed work stays inert until a human approves it. A repeated event
    // or repeated decision must not create a second task.
    let workflow = ok(&router, "POST", "/api/firm/workflows", Some(json!({
        "name": "Prepare exception review", "trigger": "manual", "enabled": true,
        "requires_approval": true,
        "action": {"kind": "create_task", "title": "Investigate synthetic exception", "project_id": project_id}
    }))).await;
    let workflow_id = id(&workflow).to_string();
    let run_path = format!("/api/firm/workflows/{workflow_id}/run");
    let run_count = ok(&router, "GET", "/api/firm/runs", None).await.as_array().unwrap().len();
    let preview = ok(&router, "POST", &run_path, Some(json!({"idempotency_key": "event-1", "dry_run": true}))).await;
    assert_eq!(preview["dry_run"], true);
    assert_eq!(ok(&router, "GET", "/api/firm/runs", None).await.as_array().unwrap().len(), run_count);
    let proposed = ok(&router, "POST", &run_path, Some(json!({"idempotency_key": "event-1"}))).await;
    assert_eq!(proposed["status"], "proposed");
    let replay = ok(&router, "POST", &run_path, Some(json!({"idempotency_key": "event-1"}))).await;
    assert_eq!(proposed["id"], replay["id"]);
    let pending_project = ok(&router, "GET", &format!("/api/firm/projects/{project_id}"), None).await;
    assert_eq!(pending_project["tasks"].as_array().unwrap().len(), 2);
    let approval_id = proposed["approval_id"].as_str().unwrap();
    // A subsequent workflow edit cannot silently change an already proposed action.
    ok(&router, "PUT", &format!("/api/firm/workflows/{workflow_id}"), Some(json!({
        "name": "Changed after proposal", "trigger": "manual", "enabled": true,
        "requires_approval": true,
        "action": {"kind": "create_task", "title": "Different unreviewed task", "project_id": project_id}
    }))).await;
    let decision_path = format!("/api/firm/approvals/{approval_id}/decide");
    let approved = ok(&router, "POST", &decision_path, Some(json!({"decision": "approve", "note": "Reviewed synthetic evidence"}))).await;
    assert_eq!(approved["status"], "executed");
    ok(&router, "POST", &decision_path, Some(json!({"decision": "approve"}))).await;
    let after_approval = ok(&router, "GET", &format!("/api/firm/projects/{project_id}"), None).await;
    assert_eq!(after_approval["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(after_approval["status"], "open");
    assert!(after_approval["tasks"].as_array().unwrap().iter().any(|task| task["title"] == "Investigate synthetic exception"));
    assert!(!after_approval["tasks"].as_array().unwrap().iter().any(|task| task["title"] == "Different unreviewed task"));
    let (status, _) = call(&router, "POST", &decision_path, Some(json!({"decision": "reject"}))).await;
    assert_eq!(status, 409);
    let rejected_run = ok(&router, "POST", &run_path, Some(json!({"idempotency_key": "event-2"}))).await;
    let rejected = ok(&router, "POST", &format!("/api/firm/approvals/{}/decide", rejected_run["approval_id"].as_str().unwrap()), Some(json!({"decision": "reject", "note": "Not needed"}))).await;
    assert_eq!(rejected["status"], "rejected");
    let after_rejection = ok(&router, "GET", &format!("/api/firm/projects/{project_id}"), None).await;
    assert_eq!(after_rejection["tasks"].as_array().unwrap().len(), 3);
    let (status, _) = call(&router, "POST", "/api/firm/workflows", Some(json!({
        "name": "Unsafe unattended task", "trigger": "manual", "requires_approval": false,
        "action": {"kind": "create_task", "title": "Should be rejected"}
    }))).await;
    assert_eq!(status, 400);

    let scheduled = ok(&router, "POST", "/api/firm/workflows", Some(json!({
        "name": "Paused report schedule", "trigger": "daily", "enabled": false,
        "due_at": "2026-10-01T12:00:00Z", "requires_approval": false,
        "action": {"kind": "generate_report", "report": "trial-balance", "to": "2026-10-31"}
    }))).await;
    let tick = ok(&router, "POST", "/api/firm/workflows/tick", Some(json!({"now": "2026-11-01T12:00:00Z"}))).await;
    assert!(!tick["runs"].as_array().unwrap().iter().any(|run| run["workflow_id"] == scheduled["id"]));
    let reports_before = ok(&router, "GET", "/api/firm/exports", None).await.as_array().unwrap().len();
    let automatic_report = ok(&router, "POST", "/api/firm/workflows", Some(json!({
        "name": "Daily native report", "trigger": "daily", "enabled": true,
        "due_at": "2026-11-01T12:00:00Z", "requires_approval": false,
        "action": {"kind": "generate_report", "report": "trial-balance", "to": "2026-10-31"}
    }))).await;
    let dispatched = ok(&router, "POST", "/api/firm/workflows/tick", Some(json!({"now": "2026-11-01T12:00:00Z"}))).await;
    let automatic_run = dispatched["runs"].as_array().unwrap().iter().find(|run| run["workflow_id"] == automatic_report["id"]).expect("due enabled workflow is dispatched");
    assert_eq!(automatic_run["status"], "executed");
    assert_eq!(ok(&router, "GET", "/api/firm/exports", None).await.as_array().unwrap().len(), reports_before + 1);

    // An OS agent can propose a bounded internal action, but cannot use this
    // endpoint to bypass review or create hidden enabled workflow definitions.
    let workflows_before = ok(&router, "GET", "/api/firm/workflows", None).await.as_array().unwrap().len();
    let agent_proposal = ok(&router, "POST", "/api/firm/proposals", Some(json!({
        "name": "Agent suggests exception follow-up", "idempotency_key": "agent-event-1",
        "action": {"kind": "create_task", "title": "Human-reviewed agent task", "project_id": project_id}
    }))).await;
    assert_eq!(agent_proposal["status"], "proposed");
    assert_eq!(agent_proposal["requires_approval"], true);
    let agent_replay = ok(&router, "POST", "/api/firm/proposals", Some(json!({
        "name": "Agent suggests exception follow-up", "idempotency_key": "agent-event-1",
        "action": {"kind": "create_task", "title": "Human-reviewed agent task", "project_id": project_id}
    }))).await;
    assert_eq!(agent_replay["id"], agent_proposal["id"]);
    assert_eq!(ok(&router, "GET", "/api/firm/workflows", None).await.as_array().unwrap().len(), workflows_before);
    assert_eq!(ok(&router, "GET", &format!("/api/firm/projects/{project_id}"), None).await["tasks"].as_array().unwrap().len(), 3);
    let agent_decision = ok(&router, "POST", &format!("/api/firm/approvals/{}/decide", agent_proposal["approval_id"].as_str().unwrap()), Some(json!({"decision": "approve", "note": "Human accepted the agent suggestion"}))).await;
    assert_eq!(agent_decision["status"], "executed");
    assert_eq!(ok(&router, "GET", &format!("/api/firm/projects/{project_id}"), None).await["tasks"].as_array().unwrap().len(), 4);
    let repeated_tick = ok(&router, "POST", "/api/firm/workflows/tick", Some(json!({"now": "2026-11-01T12:00:00Z"}))).await;
    assert!(!repeated_tick["runs"].as_array().unwrap().iter().any(|run| run["workflow_id"] == automatic_report["id"]));
    assert_eq!(ok(&router, "GET", "/api/firm/exports", None).await.as_array().unwrap().len(), reports_before + 1);

    // Human-in-the-loop close gates require responsibilities and a complete
    // checklist, then preserve a reason when an approved close is reopened.
    let close = ok(&router, "POST", "/api/firm/closes", Some(json!({
        "name": "Synthetic October close", "period_start": "2026-10-01", "period_end": "2026-10-31",
        "due_date": "2026-11-15", "preparer": "Preparer", "reviewer": "Reviewer", "approver": "Approver",
        "tasks": [{"title": "Reconcile synthetic balance"}]
    }))).await;
    let close_id = id(&close).to_string();
    let transition = format!("/api/firm/closes/{close_id}/transition");
    let (status, _) = call(&router, "POST", &transition, Some(json!({"status": "in_review"}))).await;
    assert_eq!(status, 409, "incomplete checklist must block review");
    for task in close["tasks"].as_array().unwrap() {
        ok(&router, "PUT", &format!("/api/firm/closes/{close_id}/tasks/{}", id(task)), Some(json!({"completed": true}))).await;
    }
    for state in ["in_review", "approved", "closed"] {
        let result = ok(&router, "POST", &transition, Some(json!({"status": state}))).await;
        assert_eq!(result["status"], state);
    }
    let (status, _) = call(&router, "PUT", &format!("/api/firm/closes/{close_id}/tasks/{}", id(&close["tasks"][0])), Some(json!({"completed": false}))).await;
    assert_eq!(status, 409, "closed checklist must be locked");
    let (status, _) = call(&router, "POST", &transition, Some(json!({"status": "open"}))).await;
    assert_eq!(status, 400, "reopening requires an explanation");
    let reopened = ok(&router, "POST", &transition, Some(json!({"status": "open", "note": "Synthetic late adjustment requires review"}))).await;
    assert_eq!(reopened["status"], "open");
    let no_owner = ok(&router, "POST", "/api/firm/closes", Some(json!({
        "name": "Missing preparer", "period_start": "2026-09-01", "period_end": "2026-09-30",
        "tasks": [{"title": "Completed task"}]
    }))).await;
    ok(&router, "PUT", &format!("/api/firm/closes/{}/tasks/{}", id(&no_owner), id(&no_owner["tasks"][0])), Some(json!({"completed": true}))).await;
    let (status, _) = call(&router, "POST", &format!("/api/firm/closes/{}/transition", id(&no_owner)), Some(json!({"status": "in_review"}))).await;
    assert_eq!(status, 400, "responsibility must be assigned before review");
    let assigned = ok(&router, "PUT", &format!("/api/firm/closes/{}", id(&no_owner)), Some(json!({
        "preparer": "Assigned preparer", "reviewer": "Assigned reviewer", "approver": "Assigned approver"
    }))).await;
    assert_eq!(assigned["tasks"][0]["id"], no_owner["tasks"][0]["id"]);
    assert_eq!(assigned["tasks"][0]["completed"], true, "assigning responsibility must preserve work already done");
    let in_review = ok(&router, "POST", &format!("/api/firm/closes/{}/transition", id(&no_owner)), Some(json!({"status": "in_review"}))).await;
    assert_eq!(in_review["status"], "in_review");
    let (status, _) = call(&router, "PUT", &format!("/api/firm/closes/{}", id(&no_owner)), Some(json!({"preparer": "Changed during review"}))).await;
    assert_eq!(status, 409, "reviewed close metadata must be locked until reopened");

    // Account templates are validated and versioned; preview never changes books.
    let (status, _) = call(&router, "POST", "/api/firm/account-templates", Some(json!({
        "name": "Duplicate codes", "accounts": [
            {"code": "1000", "name": "Cash", "kind": "asset"},
            {"code": "1000", "name": "Other cash", "kind": "asset"}
        ]
    }))).await;
    assert_eq!(status, 400);
    let (status, _) = call(&router, "POST", "/api/firm/account-templates", Some(json!({
        "name": "Missing parent", "accounts": [{"code": "1010", "name": "Cash", "kind": "asset", "parent_code": "9999"}]
    }))).await;
    assert_eq!(status, 400);
    let (status, _) = call(&router, "POST", "/api/firm/account-templates", Some(json!({
        "name": "Cyclic chart", "accounts": [
            {"code": "1000", "name": "Parent A", "kind": "asset", "parent_code": "1010"},
            {"code": "1010", "name": "Parent B", "kind": "asset", "parent_code": "1000"}
        ]
    }))).await;
    assert_eq!(status, 400, "cyclic account hierarchy must be rejected");
    let accounts_before = ok(&router, "GET", "/api/ledger/accounts", None).await;
    let chart = ok(&router, "POST", "/api/firm/account-templates", Some(json!({
        "name": "Synthetic chart", "accounts": [
            {"code": "1000", "name": "Cash", "kind": "asset"},
            {"code": "1010", "name": "Savings", "kind": "asset", "parent_code": "1000"}
        ]
    }))).await;
    let chart_v2 = ok(&router, "PUT", &format!("/api/firm/account-templates/{}", id(&chart)), Some(json!({
        "name": "Synthetic chart v2", "accounts": [{"code": "1000", "name": "Bank", "kind": "asset"}]
    }))).await;
    assert_eq!(chart_v2["version"].as_u64().unwrap(), chart["version"].as_u64().unwrap() + 1);
    let chart_preview = ok(&router, "POST", &format!("/api/firm/account-templates/{}/preview", id(&chart)), Some(json!({"existing_codes": ["1000"]}))).await;
    assert_eq!(chart_preview["application_deferred"], true);
    assert_eq!(chart_preview["can_apply"], false);
    assert!(!chart_preview["conflicts"].as_array().unwrap().is_empty());
    let historic_preview = ok(&router, "POST", &format!("/api/firm/account-templates/{}/preview", id(&chart)), Some(json!({"version": 1}))).await;
    assert_eq!(historic_preview["accounts"].as_array().unwrap().len(), 2);
    assert_eq!(ok(&router, "GET", "/api/ledger/accounts", None).await, accounts_before);

    // Native report exports contain actual balanced Ledger data. CSV quotes
    // embedded commas, quotes and newlines, and downloads stay authenticated.
    let empty_export = ok(&router, "POST", "/api/firm/exports", Some(json!({"report": "trial-balance", "to": "2026-12-31"}))).await;
    assert_eq!(empty_export["source"], "nxt_ledger");
    assert_eq!(empty_export["status"], "ready");
    let (status, _, empty_csv) = raw(&router, "GET", empty_export["download_url"].as_str().unwrap(), None, true).await;
    assert_eq!(status, 200);
    assert!(empty_csv.contains("debit_cents"));
    ok(&router, "POST", "/api/ledger/entries", Some(json!({
        "date": "2026-10-14", "memo": "Reviewed, \"receipt\"\nsecond line",
        "lines": [{"account_id": "acc_1000", "debit": 12345}, {"account_id": "acc_3000", "credit": 12345}]
    }))).await;
    let balance_export = ok(&router, "POST", "/api/firm/exports", Some(json!({"report": "trial-balance", "to": "2026-12-31"}))).await;
    let download = balance_export["download_url"].as_str().unwrap();
    assert_eq!(raw(&router, "GET", download, None, false).await.0, 401);
    let (status, content_type, csv) = raw(&router, "GET", download, None, true).await;
    assert_eq!(status, 200);
    assert!(content_type.starts_with("text/csv"));
    assert!(csv.lines().any(|line| line.starts_with("TOTAL,") && line.ends_with("12345,12345")), "balanced totals missing: {csv}");
    let journal = ok(&router, "POST", "/api/firm/exports", Some(json!({"report": "journal", "from": "2026-10-01", "to": "2026-10-31"}))).await;
    let (_, _, journal_csv) = raw(&router, "GET", journal["download_url"].as_str().unwrap(), None, true).await;
    assert!(journal_csv.contains("\"Reviewed, \"\"receipt\"\"\nsecond line\""), "journal memo is not CSV escaped: {journal_csv}");
    let (status, sync_type, sync_csv) = raw(&router, "GET", "/api/firm/spreadsheet-sync?report=trial-balance&to=2026-12-31", None, true).await;
    assert_eq!(status, 200);
    assert!(sync_type.starts_with("text/csv"));
    assert_eq!(sync_csv, csv, "read-only spreadsheet feed must use the same native report");
    ok(&router, "POST", "/api/ledger/entries", Some(json!({
        "date": "2026-10-15", "memo": "\n=SUM(1,1)",
        "lines": [{"account_id": "acc_1000", "debit": 1}, {"account_id": "acc_3000", "credit": 1}]
    }))).await;
    let safe_export = ok(&router, "POST", "/api/firm/exports", Some(json!({"report": "journal", "from": "2026-10-01", "to": "2026-10-31"}))).await;
    let (_, _, protected_csv) = raw(&router, "GET", safe_export["download_url"].as_str().unwrap(), None, true).await;
    // The Ledger normalizes surrounding memo whitespace; either retained or
    // trimmed whitespace must still leave an apostrophe before the formula.
    assert!(protected_csv.contains("\"'=SUM(1,1)\"") || protected_csv.contains("\"'\n=SUM(1,1)\""), "spreadsheet formula text must be inert: {protected_csv}");
    let (status, _) = call(&router, "POST", "/api/firm/exports", Some(json!({"report": "trial-balance", "from": "2026-12-31", "to": "2026-01-01"}))).await;
    assert_eq!(status, 400);

    let integrations = ok(&router, "GET", "/api/firm/integrations", None).await;
    let native = integrations.as_array().unwrap().iter().find(|item| item["id"] == "ledger").expect("native Ledger readiness");
    assert_eq!(native["configured"], true);
    assert_eq!(native["status"], "connected");
    for provider in ["quickbooks", "excel"] {
        let connection = integrations.as_array().unwrap().iter().find(|item| item["id"] == provider).expect("provider readiness is explicit");
        assert_eq!(connection["configured"], false);
        assert_eq!(connection["status"], "not_configured");
    }

    // A failed internal action remains reviewable and retryable. Fixing the
    // cause and approving the same proposal executes it once, not a new run.
    let full_tasks: Vec<Value> = (0..100).map(|index| json!({"title": format!("Synthetic task {index}")})).collect();
    let full_project = ok(&router, "POST", "/api/firm/projects", Some(json!({"name": "Bounded project", "tasks": full_tasks}))).await;
    let retry_proposal = ok(&router, "POST", "/api/firm/proposals", Some(json!({
        "name": "Retry after capacity recovery", "idempotency_key": "retry-capacity-event",
        "action": {"kind": "create_task", "title": "Approved retry task", "project_id": full_project["id"]}
    }))).await;
    let retry_path = format!("/api/firm/approvals/{}/decide", retry_proposal["approval_id"].as_str().unwrap());
    let (status, _) = call(&router, "POST", &retry_path, Some(json!({"decision": "approve"}))).await;
    assert_eq!(status, 409);
    let failed_runs = ok(&router, "GET", "/api/firm/runs", None).await;
    let failed_run = failed_runs.as_array().unwrap().iter().find(|run| run["id"] == retry_proposal["id"]).unwrap();
    assert_eq!(failed_run["status"], "proposed");
    assert_eq!(failed_run["attempts"], 1);
    assert!(failed_run["last_error"].is_string());
    ok(&router, "PUT", &format!("/api/firm/projects/{}", id(&full_project)), Some(json!({"name": "Bounded project", "tasks": []}))).await;
    let recovered = ok(&router, "POST", &retry_path, Some(json!({"decision": "approve", "note": "Capacity issue resolved"}))).await;
    assert_eq!(recovered["status"], "executed");
    let recovered_runs = ok(&router, "GET", "/api/firm/runs", None).await;
    let recovered_run = recovered_runs.as_array().unwrap().iter().find(|run| run["id"] == retry_proposal["id"]).unwrap();
    assert_eq!(recovered_run["attempts"], 2);
    assert!(recovered_run["last_error"].is_null());
    assert_eq!(ok(&router, "GET", &format!("/api/firm/projects/{}", id(&full_project)), None).await["tasks"].as_array().unwrap().len(), 1);
    let audit = ok(&router, "GET", "/api/firm/audit", None).await;
    assert!(!audit.as_array().unwrap().is_empty());
    assert!(audit.to_string().contains("Synthetic late adjustment requires review"), "reopen explanation must be audited");
}
