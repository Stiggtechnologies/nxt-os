//! Firm agent on the journey spine, with a scripted model in place of Claude:
//! the agent works books_access through capability actions, Nixon reviews
//! before the step closes, and escalations and policy-reserved steps wait in
//! the human review queue. Synthetic data only.
use nxt_os::agent::{Agent, Model};
use nxt_os::client_journey::capability::{BoxFut, Capabilities, SourceProfile, SourceSystem};
use nxt_os::client_journey::service::{Journeys, StaffAction};
use nxt_os::client_journey::sources::SourceSoftwareImport;
use nxt_os::client_journey::Actor;
use nxt_os::source_import::{SourceImport, files::FileInput};
use nxt_os::store::Store;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Replays canned Messages API responses and records what it was offered.
struct Script {
    replies: Mutex<VecDeque<Value>>,
    tools_seen: Mutex<Vec<String>>,
}

impl Script {
    fn new(replies: Vec<Value>) -> Arc<Self> {
        Arc::new(Self { replies: Mutex::new(replies.into()), tools_seen: Mutex::new(vec![]) })
    }
}

impl Model for Script {
    fn respond<'a>(&'a self, _system: &'a str, tools: &'a [Value], _messages: &'a [Value]) -> BoxFut<'a, Result<Value, String>> {
        Box::pin(async move {
            *self.tools_seen.lock().unwrap() = tools.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect();
            self.replies.lock().unwrap().pop_front().ok_or_else(|| "script exhausted".to_string())
        })
    }
}

fn call(name: &str, input: Value) -> Value {
    json!({"stop_reason": "tool_use", "content": [{"type": "tool_use", "id": format!("t-{name}"), "name": name, "input": input}]})
}
fn done(text: &str) -> Value {
    json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": text}]})
}
fn cpa() -> Actor {
    Actor::Human { name: "Synthetic reviewer".into(), role: "Reviewer".into() }
}

struct Firm {
    journeys: Arc<Journeys>,
    source_import: Arc<SourceImport>,
}

fn firm(policy: &str) -> Firm {
    let source_import = Arc::new(SourceImport::new(Store::memory()));
    let caps = Capabilities::new().with(Arc::new(SourceSoftwareImport {
        company_books: Arc::new(nxt_os::company_books::CompanyBooks::new(Store::memory())),
        migrations: Arc::new(nxt_os::migrations::Migrations::new(Store::memory())),
        source_import: source_import.clone(),
    }));
    Firm { journeys: Arc::new(Journeys::new(Store::memory(), caps).with_policy(policy)), source_import }
}

fn dtmax(keys: &[&str]) -> SourceProfile {
    SourceProfile { systems: vec![SourceSystem::Dtmax], dtmax_company: Some("firm-dtmax".into()), dtmax_keys: keys.iter().map(|k| k.to_string()).collect() }
}

/// Starts a journey and has staff record the manual steps before books_access.
async fn at_books_access(f: &Firm, client: &str, sources: SourceProfile) {
    f.journeys.create(client, sources).await.map_err(|e| e.1).unwrap();
    for step in ["enquiry_received", "discovery_call"] {
        f.journeys.staff(client, step, StaffAction::Complete, cpa(), true, "").await.map_err(|e| e.1).unwrap();
    }
}

/// A preserved DT Max file that no client is linked to yet.
async fn unmatched_file(f: &Firm, stem: &str) {
    let bytes = format!("synthetic dt max return {stem}");
    f.source_import.preserve_file("firm-dtmax", FileInput {
        synthetic: true,
        original_path: format!("Y:\\DTMax\\2023\\{stem}.t23"),
        year: 2023,
        file_type: "t23".into(),
        sha256: hex::encode(Sha256::digest(bytes.as_bytes())),
        content_hex: hex::encode(bytes.as_bytes()),
    }).await.map_err(|e| e.1).unwrap();
}

#[tokio::test]
async fn agent_links_confirmed_dtmax_files_and_nixon_closes_the_step() {
    let f = firm("{}");
    at_books_access(&f, "smith-j", dtmax(&["SMITHJ"])).await;
    unmatched_file(&f, "smithj").await;
    unmatched_file(&f, "otherk").await;
    let model = Script::new(vec![
        call("link_dtmax_files_by_confirmed_keys", json!({})),
        call("check_step", json!({})),
        done("Linked the client's DT Max file; books access is ready."),
    ]);
    let v = Agent::new(f.journeys.clone(), model.clone()).work("smith-j").await.map_err(|e| e.1).unwrap();

    assert_eq!(v["step"], "books_access");
    let tools = model.tools_seen.lock().unwrap().clone();
    assert!(tools.contains(&"link_dtmax_files_by_confirmed_keys".into()) && tools.contains(&"escalate_to_human".into()));
    assert_eq!(v["actions"][0]["ok"], true);
    assert_eq!(v["actions"][0]["result"]["linked"].as_array().unwrap().len(), 1, "only the confirmed stem is linked");
    assert_eq!(v["actions"][1]["result"]["state"], "ready");
    let j = &v["journey"];
    assert_eq!(j["status"]["books_access"], "completed");
    assert_eq!(j["pending_review"], Value::Null);
    let review = &j["assessments"][0]["readiness"]["review"];
    assert_eq!(review["reviewer"], "nixon_review");
    assert_eq!(review["approved"], true);
    assert_eq!(j["agent_log"].as_array().unwrap().len(), 2);
    let other = f.source_import.unmatched_files("firm-dtmax").await.map_err(|e| e.1).unwrap();
    assert_eq!(other["files"][0]["linkage_key"], "OTHERK", "unconfirmed stems stay for staff");
}

#[tokio::test]
async fn agent_escalates_when_identity_is_unconfirmed() {
    let f = firm("{}");
    at_books_access(&f, "jones-k", dtmax(&[])).await;
    unmatched_file(&f, "jonesk").await;
    let model = Script::new(vec![
        call("link_dtmax_files_by_confirmed_keys", json!({})),
        call("escalate_to_human", json!({"reason": "staff must confirm which DT Max stems belong to this client"})),
    ]);
    let v = Agent::new(f.journeys.clone(), model).work("jones-k").await.map_err(|e| e.1).unwrap();
    assert_eq!(v["actions"][0]["ok"], false);
    assert_eq!(v["escalated"], "staff must confirm which DT Max stems belong to this client");
    assert_eq!(v["journey"]["status"]["books_access"], "pending");
    assert_eq!(v["journey"]["pending_review"]["mode"], "human");

    let queue = f.journeys.reviews().await.map_err(|e| e.1).unwrap();
    assert_eq!(queue["reviews"][0]["client_id"], "jones-k");
    let again = Agent::new(f.journeys.clone(), Script::new(vec![])).work("jones-k").await.map_err(|e| e.1).unwrap();
    assert_eq!(again["worked"], false, "the agent leaves an escalated step to people");

    let err = f.journeys.staff("jones-k", "books_access", StaffAction::Approve, cpa(), false, "").await.unwrap_err();
    assert_eq!(err.0.as_u16(), 403);
    let j = f.journeys.staff("jones-k", "books_access", StaffAction::Approve, cpa(), true, "confirmed by phone").await.map_err(|e| e.1).unwrap();
    assert_eq!(j["status"]["books_access"], "completed");
    assert_eq!(j["pending_review"], Value::Null);
}

#[tokio::test]
async fn nixon_escalates_ready_work_after_a_failed_agent_action() {
    let f = firm("{}");
    at_books_access(&f, "lee-m", dtmax(&["LEEM"])).await;
    unmatched_file(&f, "leem").await;
    let model = Script::new(vec![
        call("delete_everything", json!({})),
        call("link_dtmax_files_by_confirmed_keys", json!({})),
        done("Linked."),
    ]);
    let v = Agent::new(f.journeys.clone(), model).work("lee-m").await.map_err(|e| e.1).unwrap();
    assert_eq!(v["actions"][0]["ok"], false, "tools outside the capability are refused");
    let j = &v["journey"];
    assert_eq!(j["status"]["books_access"], "in_progress");
    let reasons = j["pending_review"]["reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|r| r.as_str().unwrap().contains("delete_everything")));
}

#[tokio::test]
async fn policy_reserved_steps_wait_for_a_person_and_manual_steps_skip_the_model() {
    let f = firm(r#"{"books_access":"human"}"#);
    f.journeys.create("park-s", dtmax(&["PARKS"])).await.map_err(|e| e.1).unwrap();
    let idle = Agent::new(f.journeys.clone(), Script::new(vec![])).work("park-s").await.map_err(|e| e.1).unwrap();
    assert_eq!(idle["worked"], false, "enquiry_received has no capability, so no model call");

    for step in ["enquiry_received", "discovery_call"] {
        f.journeys.staff("park-s", step, StaffAction::Complete, cpa(), true, "").await.map_err(|e| e.1).unwrap();
    }
    unmatched_file(&f, "parks").await;
    let model = Script::new(vec![call("link_dtmax_files_by_confirmed_keys", json!({})), done("Linked.")]);
    let v = Agent::new(f.journeys.clone(), model).work("park-s").await.map_err(|e| e.1).unwrap();
    assert_eq!(v["journey"]["pending_review"]["mode"], "human");
    assert_eq!(v["journey"]["status"]["books_access"], "in_progress");
    let j = f.journeys.staff("park-s", "books_access", StaffAction::Approve, cpa(), true, "").await.map_err(|e| e.1).unwrap();
    assert_eq!(j["status"]["books_access"], "completed");
    let coverage = f.journeys.coverage();
    assert_eq!(coverage["policy"]["modes"]["books_access"], "human");
    assert_eq!(coverage["policy"]["modes"]["invoicing"], "human");
}
