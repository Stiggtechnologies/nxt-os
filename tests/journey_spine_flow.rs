//! The client journey as the firm's spine, through the production router:
//! persisted journeys, staff-recorded manual steps, human gates, and the
//! source-software capability doing `books_access` and `data_migration` from
//! what the importers hold. Synthetic data only.
use axum::{Router, body::Body, http::Request};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

fn app() -> Router {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::remove_var("CRON_SECRET");
    }
    nxt_os::app_with(nxt_os::store::Store::memory)
}

async fn call(r: &Router, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let mut b = Request::builder().method(method).uri(path)
        .header("authorization", "Bearer k")
        .header("user-agent", "journey-test")
        .header("x-forwarded-for", "203.0.113.7");
    let body = match body {
        Some(v) => { b = b.header("content-type", "application/json"); Body::from(v.to_string()) }
        None => Body::empty(),
    };
    let res = r.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), 4 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn ok(r: &Router, method: &str, path: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, method, path, body).await;
    assert_eq!(s, 200, "{method} {path}: {v}");
    v
}

async fn staff(r: &Router, client: &str, step: &str, action: &str) -> Value {
    ok(r, "POST", &format!("/api/journeys/{client}/steps/{step}"), Some(json!({"action": action, "note": "synthetic"}))).await
}

/// Staff record every step from the current one up to (not including) `until`.
async fn staff_through(r: &Router, client: &str, until: &str) {
    loop {
        let v = ok(r, "GET", &format!("/api/journeys/{client}"), None).await;
        let step = v["progress"]["current_step"].as_str().unwrap().to_string();
        if step == until { return; }
        let after = staff(r, client, &step, "complete").await;
        if after["status"][&step] == "awaiting_approval" {
            staff(r, client, &step, "approve").await;
        }
    }
}

/// Per-system states from this advance's assessment of the source-import step.
fn systems(v: &Value) -> Vec<(String, String)> {
    let a = v["assessments"].as_array().unwrap().iter()
        .find(|a| a["capability"] == "source_software_import").expect("source import was assessed");
    a["readiness"]["evidence"]["systems"].as_array().unwrap().iter()
        .map(|s| (s["system"].as_str().unwrap().to_string(), s["state"].as_str().unwrap().to_string())).collect()
}

#[tokio::test]
async fn coverage_names_the_source_import_capability() {
    let r = app();
    let v = ok(&r, "GET", "/api/journeys/coverage", None).await;
    let cap = |k: &str| v["steps"].as_array().unwrap().iter().find(|s| s["step"] == k).unwrap()["capability"].clone();
    assert_eq!(cap("books_access"), "source_software_import");
    assert_eq!(cap("data_migration"), "source_software_import");
    assert_eq!(cap("enquiry_received"), Value::Null);
    assert_eq!(v["automated"], 2);
}

#[tokio::test]
async fn journey_persists_and_stops_at_manual_steps() {
    let r = app();
    ok(&r, "POST", "/api/journeys", Some(json!({"client_id": "acme-co"}))).await;
    let (s, _) = call(&r, "POST", "/api/journeys", Some(json!({"client_id": "acme-co"}))).await;
    assert_eq!(s, 409, "one journey per client");
    let v = ok(&r, "POST", "/api/journeys/acme-co/advance", None).await;
    assert_eq!(v["moved"], json!([]));
    assert_eq!(v["last_assessment"]["readiness"]["state"], "manual");
    assert_eq!(v["progress"]["current_step"], "enquiry_received");
    let listed = ok(&r, "GET", "/api/journeys", None).await;
    assert_eq!(listed["journeys"][0]["client_id"], "acme-co");
}

#[tokio::test]
async fn books_access_reports_each_current_system_and_blocks_on_gaps() {
    let r = app();
    ok(&r, "POST", "/api/journeys", Some(json!({"client_id": "beta-co", "sources": {"systems": ["quickbooks_online", "caseware"]}}))).await;
    staff_through(&r, "beta-co", "books_access").await;
    let v = ok(&r, "POST", "/api/journeys/beta-co/advance", None).await;
    assert_eq!(v["progress"]["current_step"], "books_access");
    assert_eq!(v["last_assessment"]["readiness"]["state"], "blocked");
    assert_eq!(systems(&v), vec![("quickbooks_online".into(), "missing".into()), ("caseware".into(), "not_automated".into())]);
    assert_eq!(v["last_assessment"]["readiness"]["reasons"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn dtmax_files_carry_the_client_through_books_access_and_data_migration() {
    let r = app();
    let client = "smith-j";
    ok(&r, "POST", "/api/journeys", Some(json!({"client_id": client, "sources": {"systems": ["dtmax"], "dtmax_company": "firm-dtmax"}}))).await;
    staff_through(&r, client, "books_access").await;
    let v = ok(&r, "POST", &format!("/api/journeys/{client}/advance"), None).await;
    assert_eq!(systems(&v), vec![("dtmax".into(), "missing".into())]);

    ok(&r, "POST", "/api/source-imports/dtmax/companies/firm-dtmax/clients", Some(json!({"client_id": client, "dtmax_keys": ["SMITHJ"]}))).await;
    let bytes = b"synthetic dt max return";
    ok(&r, "POST", "/api/source-imports/dtmax/companies/firm-dtmax/files", Some(json!({
        "synthetic": true, "original_path": "Y:\\DTMax\\2023\\smithj.t23", "year": 2023, "file_type": "t23",
        "sha256": hex::encode(Sha256::digest(bytes)), "content_hex": hex::encode(bytes),
    }))).await;

    let v = ok(&r, "POST", &format!("/api/journeys/{client}/advance"), None).await;
    assert_eq!(v["moved"], json!(["books_access"]));
    assert_eq!(v["status"]["books_access"], "completed");
    assert_eq!(v["progress"]["current_step"], "books_health_check");
    let entry = v["ledger"].as_array().unwrap().iter().rev().find(|e| e["step"] == "books_access").unwrap();
    assert_eq!(entry["actor"], json!({"kind": "automation", "executor": "source_software_import"}));

    staff_through(&r, client, "data_migration").await;
    let v = ok(&r, "POST", &format!("/api/journeys/{client}/advance"), None).await;
    assert_eq!(v["moved"], json!(["data_migration"]));
    assert_eq!(systems(&v), vec![("dtmax".into(), "imported".into())]);
    assert_eq!(v["progress"]["current_step"], "catch_up_cleanup");
}

#[tokio::test]
async fn no_prior_software_skips_data_migration() {
    let r = app();
    ok(&r, "POST", "/api/journeys", Some(json!({"client_id": "new-co"}))).await;
    let (s, _) = call(&r, "POST", "/api/journeys/new-co/steps/discovery_call", Some(json!({"action": "complete"}))).await;
    assert_eq!(s, 409, "steps run in order");
    staff_through(&r, "new-co", "data_migration").await;
    let v = ok(&r, "POST", "/api/journeys/new-co/advance", None).await;
    assert_eq!(v["status"]["data_migration"], "skipped");
}

#[tokio::test]
async fn human_gates_and_cron_stay_guarded() {
    let r = app();
    ok(&r, "POST", "/api/journeys", Some(json!({"client_id": "gate-co"}))).await;
    let (s, _) = call(&r, "POST", "/api/journeys/gate-co/steps/enquiry_received", Some(json!({"action": "approve"}))).await;
    assert_eq!(s, 409, "only human-gated steps are approved");
    staff_through(&r, "gate-co", "engagement_letter").await;
    let v = staff(&r, "gate-co", "engagement_letter", "complete").await;
    assert_eq!(v["status"]["engagement_letter"], "awaiting_approval");
    let v = ok(&r, "POST", "/api/journeys/gate-co/advance", None).await;
    assert_eq!(v["moved"], json!([]), "advance never passes a human gate");
    let (s, _) = call(&r, "GET", "/api/cron/journey-tick", None).await;
    assert_eq!(s, 503, "cron refuses without CRON_SECRET");
}
