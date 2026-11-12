//! Autonomous execution (phase 1) acceptance, synthetic data only. Runs start from
//! approved God's Eye / Nixon proposals, loop act → verify → adjust under budgets, and
//! never cross a hard gate even with a standing approval.
use axum::{body::Body, http::Request};
use nxt_os::agent::{Model, ModelError};
use nxt_os::autonomy::engine::{Deterministic, ModelPlanner, PlannedStep, Planner};
use nxt_os::autonomy::gateway::{self, GatewayConfig};
use nxt_os::nixon_finance::ai_gateway;
use nxt_os::autonomy::guard::{self, Tier};
use nxt_os::autonomy::{ActionClass, Autonomy, Budget, Config, Run, RunStatus, policy, tools};
use nxt_os::client_journey::capability::BoxFut;
use nxt_os::ledger::engine::Ledger;
use nxt_os::store::Store;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const REVIEWER: &str = "Synthetic Reviewer";
const ORVILLE: &str = "Synthetic Owner";

fn on() -> Config {
    Config { enabled: true, kill_env: false, budget: Budget::default(), ai: None }
}

fn proposal(id: &str, kind: &str, status: &str) -> Value {
    json!({"id": id, "finding_id": format!("f_{id}"), "kind": kind, "changes_books": false, "files_tax": false,
        "execution": "disabled", "applied": false, "status": status,
        "summary": "Synthetic: Q3 GST remittance not seen in the bank feed", "prepared_by": "Nixon", "payload": {}})
}

async fn seed(ledger: &Ledger, proposals: Vec<Value>) {
    let decisions: Vec<Value> = proposals
        .iter()
        .filter(|p| p["status"] == "approved_no_execution")
        .map(|p| json!({"proposal_id": p["id"], "actor_name": REVIEWER, "decision": "approve", "note": "", "at": "2026-11-12T07:00:00Z", "execution": "disabled", "verification_state": "not_verification"}))
        .collect();
    let findings: Vec<Value> = proposals.iter().map(|p| json!({"id": p["finding_id"], "title": "Missed remittance (synthetic)"})).collect();
    let review = json!({"id": "rv1", "correlation_id": "cor_1", "scope": "gods_eye", "proposals": proposals, "findings": findings, "decisions": decisions});
    ledger.store.set_json("nixon:review:rv1", &review).await.unwrap();
}

async fn set_status(ledger: &Ledger, pid: &str, status: &str) {
    let mut review: Value = ledger.store.get_json("nixon:review:rv1").await.unwrap().unwrap();
    for p in review["proposals"].as_array_mut().unwrap() {
        if p["id"] == pid {
            p["status"] = json!(status);
        }
    }
    ledger.store.set_json("nixon:review:rv1", &review).await.unwrap();
}

fn engine(config: Config) -> (Arc<Ledger>, Autonomy) {
    let ledger = Arc::new(Ledger::new(Store::memory()));
    (ledger.clone(), Autonomy::new(ledger, config))
}

/// Replays a fixed list of steps, then reports the plan complete.
struct Fixed(Mutex<VecDeque<PlannedStep>>);
impl Fixed {
    fn new(steps: Vec<(&str, Value)>) -> Arc<Self> {
        Arc::new(Self(Mutex::new(steps.into_iter().map(|(t, i)| PlannedStep { tool: t.into(), input: i, cost_micro_usd: 0 }).collect())))
    }
}
impl Planner for Fixed {
    fn name(&self) -> &'static str {
        "fixed"
    }
    fn next<'a>(&'a self, _run: &'a Run, _p: &'a Value) -> BoxFut<'a, Result<Option<PlannedStep>, String>> {
        Box::pin(async move { Ok(self.0.lock().unwrap().pop_front()) })
    }
}

fn outcomes(run: &Run) -> Vec<(String, String)> {
    run.steps.iter().map(|s| (s.tool.clone(), s.outcome.clone())).collect()
}

#[tokio::test]
async fn off_by_default_and_ai_off_without_paid_plan() {
    let (ledger, a) = engine(Config::off());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let e = a.start("rv1", "p1", REVIEWER).await.unwrap_err();
    assert_eq!(e.status, 409);
    assert_eq!(a.status()["autonomous_execution"], "off");
    assert_eq!(a.status()["ai_planner"], "off");
    let vars = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
    assert!(GatewayConfig::from_vars(vars(&[])).is_none());
    assert!(GatewayConfig::from_vars(vars(&[("NXT_AUTONOMY_AI", "on"), ("AI_GATEWAY_API_KEY", "synthetic")])).is_none(), "no paid-plan confirmation");
    assert!(GatewayConfig::from_vars(vars(&[("NXT_AUTONOMY_AI", "on"), ("NXT_AI_GATEWAY_PAID", "confirmed")])).is_none(), "no credential");
    let cfg = GatewayConfig::from_vars(vars(&[("NXT_AUTONOMY_AI", "on"), ("NXT_AI_GATEWAY_PAID", "confirmed"), ("AI_GATEWAY_API_KEY", "synthetic-key")])).unwrap();
    assert!(!format!("{cfg:?}").contains("synthetic-key"), "credential never printed");
}

#[tokio::test]
async fn approved_proposal_runs_internal_steps_then_waits_for_standing_approval() {
    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::NeedsHuman, "{}", run.reason);
    assert!(run.reason.contains("firm_record_update"), "{}", run.reason);
    assert_eq!(run.approved_by, REVIEWER);
    assert_eq!(
        outcomes(&run),
        vec![
            ("read_proposal".into(), "verified".into()),
            ("add_internal_note".into(), "verified".into()),
            ("create_internal_task".into(), "verified".into()),
            ("mark_finding_followed_up".into(), "refused".into()),
        ]
    );
    assert_eq!(run.notifications.len(), 1, "internal task is auto-with-notify");

    // Orville grants a standing approval for the class; the run resumes from memory.
    policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "follow-up marks are internal", 7, 20).await.unwrap();
    let run = a.resume(&run.id, REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::VerifiedDone, "{}", run.reason);
    let tools_run: Vec<_> = run.steps.iter().filter(|s| s.outcome == "verified").map(|s| s.tool.as_str()).collect();
    assert_eq!(tools_run, ["read_proposal", "add_internal_note", "create_internal_task", "mark_finding_followed_up", "recheck_proposal"]);

    // Records are internal-only; the proposal itself is untouched and the books unchanged.
    let notes = ledger.store.smembers("autonomy:notes:rv1:p1").await.unwrap();
    assert_eq!(notes.len(), 1);
    let note: Value = ledger.store.get_json(&notes[0]).await.unwrap().unwrap();
    assert_eq!(note["internal_only"], true);
    let review: Value = ledger.store.get_json("nixon:review:rv1").await.unwrap().unwrap();
    assert_eq!(review["proposals"][0]["status"], "approved_no_execution");
    assert_eq!(review["proposals"][0]["applied"], false);
    assert!(ledger.entries().await.unwrap().is_empty(), "no journal entry was posted");
    let audit = ledger.audit_log(500).await.unwrap();
    assert!(audit.iter().any(|e| e.action == "autonomy.run_started"));
}

#[tokio::test]
async fn gods_eye_staff_follow_up_proposals_run() {
    let (ledger, a) = engine(on());
    let mut p = proposal("p_ge_1", "staff_follow_up", "approved_no_execution");
    p["prepared_by"] = json!(nxt_os::nixon_finance::gods_eye::PREPARED_BY);
    p["payload"] = json!({"watcher": "missed_remittance", "effect": "none"});
    seed(&ledger, vec![p]).await;
    policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "follow-up marks are internal", 7, 20).await.unwrap();
    let run = a.start("rv1", "p_ge_1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::VerifiedDone, "{}", run.reason);
    assert_eq!(run.action_class, ActionClass::InternalTask);
}

#[tokio::test]
async fn hard_gated_proposals_never_start() {
    let (ledger, a) = engine(on());
    let mut books = proposal("pb", "acknowledge", "approved_no_execution");
    books["changes_books"] = json!(true);
    let mut tax = proposal("pt", "acknowledge", "approved_no_execution");
    tax["files_tax"] = json!(true);
    seed(&ledger, vec![
        books,
        tax,
        proposal("pf", "file_return", "approved_no_execution"),
        proposal("pq", "quickbooks_post", "approved_no_execution"),
        proposal("pc", "notify_client", "approved_no_execution"),
        proposal("pe", "send_email", "approved_no_execution"),
        proposal("ps", "stripe_charge", "approved_no_execution"),
        proposal("pu", "something_new", "approved_no_execution"),
        proposal("pp", "acknowledge", "pending_approval"),
        proposal("pd", "acknowledge", "dismissed"),
    ]).await;
    for pid in ["pb", "pt", "pf", "pq", "pc", "pe", "ps", "pu", "pp", "pd"] {
        let e = a.start("rv1", pid, REVIEWER).await.unwrap_err();
        assert_eq!(e.status, 403, "{pid}: {}", e.message);
    }
    assert!(a.list_runs().await.unwrap().is_empty());
}

#[tokio::test]
async fn hard_gate_tools_are_refused_even_with_every_standing_approval() {
    // Standing approvals for hard-gated classes are refused in code.
    let store = Store::memory();
    for c in ActionClass::ALL.into_iter().filter(|c| c.is_hard_gate()) {
        let e = policy::grant(&store, c, ORVILLE, "try", 7, 5).await.unwrap_err();
        assert_eq!(e.0, 403, "{c:?}");
    }
    for tool in ["post_journal", "commit_company_books", "quickbooks_post", "file_return", "remit_tax", "notify_client", "send_email", "send_sms", "outbound_call", "stripe_charge", "make_payment", "reveal_identifier", "change_protected_repo"] {
        let (ledger, a) = engine(on());
        policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "allowed class", 7, 20).await.unwrap();
        let a = a.with_planner(Fixed::new(vec![("read_proposal", json!({})), (tool, json!({"to": "client@example.test"}))]));
        seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
        let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
        assert_eq!(run.status, RunStatus::Refused, "{tool}");
        assert_eq!(run.steps.last().unwrap().outcome, "refused");
        assert!(!tools::find(tool).unwrap().executable);
        let audit = a.audit_log(100).await.unwrap();
        assert!(audit.iter().any(|e| e["action"] == "hard_gate_refused"), "{tool}");
    }
}

#[tokio::test]
async fn standing_approval_rules_and_revocation_stop_the_next_step() {
    let store = Store::memory();
    assert_eq!(policy::grant(&store, ActionClass::Read, ORVILLE, "x", 7, 5).await.unwrap_err().0, 400);
    assert_eq!(policy::grant(&store, ActionClass::FirmRecordUpdate, ORVILLE, "x", 31, 5).await.unwrap_err().0, 400);
    assert_eq!(policy::grant(&store, ActionClass::FirmRecordUpdate, ORVILLE, "", 7, 5).await.unwrap_err().0, 400);

    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "ok", 7, 20).await.unwrap();
    policy::revoke(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE).await.unwrap();
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::NeedsHuman);
    assert_eq!(run.steps.last().unwrap().tool, "mark_finding_followed_up");

    // Daily cap: one use allowed, the second run waits.
    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution"), proposal("p2", "acknowledge", "approved_no_execution")]).await;
    policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "ok", 7, 1).await.unwrap();
    assert_eq!(a.start("rv1", "p1", REVIEWER).await.unwrap().status, RunStatus::VerifiedDone);
    let second = a.start("rv1", "p2", REVIEWER).await.unwrap();
    assert_eq!(second.status, RunStatus::NeedsHuman);
    assert!(second.reason.contains("daily cap"), "{}", second.reason);
}

#[tokio::test]
async fn kill_switch_blocks_new_runs_and_stops_waiting_ones() {
    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution"), proposal("p2", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::NeedsHuman);
    a.set_kill(true, ORVILLE, "synthetic drill").await.unwrap();
    assert_eq!(a.start("rv1", "p2", REVIEWER).await.unwrap_err().status, 423);
    policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "ok", 7, 20).await.unwrap();
    assert_eq!(a.resume(&run.id, REVIEWER).await.unwrap().status, RunStatus::Killed);

    let (ledger, a) = engine(Config { kill_env: true, ..on() });
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    assert_eq!(a.start("rv1", "p1", REVIEWER).await.unwrap_err().status, 423);
}

struct Slow;
impl Planner for Slow {
    fn name(&self) -> &'static str {
        "slow"
    }
    fn next<'a>(&'a self, run: &'a Run, p: &'a Value) -> BoxFut<'a, Result<Option<PlannedStep>, String>> {
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            Deterministic.next(run, p).await
        })
    }
}

#[tokio::test]
async fn step_time_and_cost_budgets_stop_the_run() {
    let (ledger, a) = engine(Config { budget: Budget { max_steps: 2, ..Budget::default() }, ..on() });
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::BudgetExhausted);
    assert_eq!(run.steps.len(), 2);

    let (ledger, a) = engine(Config { budget: Budget { max_wall_ms: 40, ..Budget::default() }, ..on() });
    let a = a.with_planner(Arc::new(Slow));
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::BudgetExhausted, "{}", run.reason);
    assert!(run.reason.contains("time budget"));

    let (ledger, a) = engine(Config { budget: Budget { max_cost_micro_usd: 100, ..Budget::default() }, ..on() });
    let script = Script::new(vec![tool_use("read_proposal", json!({}), 10_000, 1_000), tool_use("recheck_proposal", json!({}), 10, 10)]);
    let a = a.with_planner(Arc::new(ModelPlanner { model: script }));
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::BudgetExhausted, "{}", run.reason);
    assert!(run.reason.contains("cost budget"));
    assert!(run.spent.cost_micro_usd > 100);
}

#[tokio::test]
async fn runs_and_steps_are_idempotent() {
    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let first = a.start("rv1", "p1", REVIEWER).await.unwrap();
    let again = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(first.id, again.id, "one run per approved proposal");
    assert_eq!(a.list_runs().await.unwrap().len(), 1);

    let (ledger, a) = engine(on());
    let note = json!({"text": "Synthetic internal note"});
    let a = a.with_planner(Fixed::new(vec![("add_internal_note", note.clone()), ("add_internal_note", note)]));
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::VerifiedDone);
    assert_eq!(outcomes(&run)[1].1, "replayed");
    assert_eq!(ledger.store.smembers("autonomy:notes:rv1:p1").await.unwrap().len(), 1);
}

#[tokio::test]
async fn verify_failure_rolls_back_retries_then_hands_to_a_person() {
    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    a.inject_verify_faults("add_internal_note", 1);
    a.inject_verify_faults("create_internal_task", 2);
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::NeedsHuman, "{}", run.reason);
    let note = run.steps.iter().find(|s| s.tool == "add_internal_note").unwrap();
    assert_eq!((note.outcome.as_str(), note.attempts), ("verified", 2), "adjusted and retried");
    let task = run.steps.iter().find(|s| s.tool == "create_internal_task").unwrap();
    assert_eq!(task.outcome, "rolled_back");
    let key = &ledger.store.smembers("autonomy:tasks").await.unwrap()[0];
    let rec: Value = ledger.store.get_json(key).await.unwrap().unwrap();
    assert_eq!(rec["tombstone"], true, "compensating action left a tombstone");
    assert!(a.audit_log(200).await.unwrap().iter().any(|e| e["action"] == "rolled_back"));
}

#[tokio::test]
async fn a_dismissed_proposal_halts_the_run() {
    let (ledger, a) = engine(on());
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    set_status(&ledger, "p1", "dismissed").await;
    policy::grant(&ledger.store, ActionClass::FirmRecordUpdate, ORVILLE, "ok", 7, 20).await.unwrap();
    let run = a.resume(&run.id, REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::NeedsHuman);
    assert!(run.reason.contains("no longer approved"), "{}", run.reason);
}

#[tokio::test]
async fn identifiers_are_refused_and_never_logged() {
    let (ledger, a) = engine(on());
    let a = a.with_planner(Fixed::new(vec![("add_internal_note", json!({"text": "Client SIN 046 454 286 needs review"}))]));
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::NeedsHuman, "{}", run.reason);
    let everything = format!("{}{}", serde_json::to_string(&a.audit_log(500).await.unwrap()).unwrap(), serde_json::to_string(&a.get_run(&run.id).await.unwrap()).unwrap());
    let ledger_audit = format!("{:?}", ledger.audit_log(500).await.unwrap());
    for hay in [&everything, &ledger_audit] {
        assert!(!hay.contains("046 454 286") && !hay.contains("046454286"), "SIN leaked");
    }
    assert!(ledger.store.smembers("autonomy:notes:rv1:p1").await.unwrap().is_empty());
}

#[test]
fn sensitivity_guard_and_routing() {
    assert_eq!(guard::classify("Draft a friendly internal checklist title"), Tier::T1);
    assert_eq!(guard::classify("SIN 046-454-286"), Tier::T3);
    assert_eq!(guard::classify("acct 12345678"), Tier::T2);
    assert_eq!(guard::classify("email jane@example.test"), Tier::T2);
    assert_eq!(guard::classify("token sk_live_abc"), Tier::T3);
    assert_eq!(guard::classify("due 2026-04-30"), Tier::T1, "dates are not identifiers");
    assert_eq!(guard::redact("SIN 046 454 286, acct 12345678, on 2026-04-30"), "SIN •••286, acct •••678, on 2026-04-30");
    assert!(guard::redact("key sk_live_abcdef123 end").contains("[redacted-key]"));
    assert!(!guard::redact("key sk_live_abcdef123 end").contains("abcdef"));

    // Routing is God's Eye's ai_gateway: autonomy prompts are always client data, so the
    // plan is zero-retention gateway only, never NVIDIA free, never DeepSeek first-party.
    let env = |k: &str| match k {
        "NVIDIA_API_KEY" => Some("synthetic".to_string()),
        _ => None,
    };
    for text in ["Draft a meeting agenda title", "SIN 046 454 286"] {
        let routes = gateway::routes_for(text, &env);
        assert!(!routes.is_empty());
        for r in &routes {
            assert_eq!(r.provider, ai_gateway::Provider::Gateway, "{text}");
            assert!(r.zero_data_retention);
            assert!(!r.only.iter().any(|h| h == "deepseek"));
        }
        assert_eq!(routes[0].model, "deepseek/deepseek-v4.1-flash-1017");
        assert!(routes.iter().any(|r| r.model == "anthropic/claude-sonnet-5.5"), "zero-retention fallback present");
    }
    // A caller declaring non-sensitive text is overridden by the content classifier.
    assert_eq!(guard::effective_tier(Tier::T0, "acct 12345678").sensitivity(), guard::Sensitivity::ClientData);
    assert_eq!(guard::effective_tier(Tier::T0, "agenda").sensitivity(), guard::Sensitivity::NonSensitive);

    let route = &gateway::routes_for("x", &env)[0];
    let body = gateway::tool_body(route, "s", "u", &[json!({"name": "read_proposal", "description": "d", "input_schema": {"type": "object"}})]);
    assert_eq!(body["providerOptions"]["gateway"]["zeroDataRetention"], true);
    assert_eq!(body["tools"][0]["function"]["name"], "read_proposal");
    let reply = json!({"choices": [{"message": {"tool_calls": [{"id": "c1", "function": {"name": "add_internal_note", "arguments": "{\"text\":\"hi\"}"}}]}}], "usage": {"prompt_tokens": 5, "completion_tokens": 2}});
    let shaped = gateway::to_messages_shape(&reply);
    assert_eq!(shaped["content"][0]["type"], "tool_use");
    assert_eq!(shaped["content"][0]["input"]["text"], "hi");
}

/// Scripted Messages API responses with token usage.
struct Script {
    replies: Mutex<VecDeque<Value>>,
    offered: Mutex<Vec<String>>,
}
impl Script {
    fn new(replies: Vec<Value>) -> Arc<Self> {
        Arc::new(Self { replies: Mutex::new(replies.into()), offered: Mutex::new(vec![]) })
    }
}
impl Model for Script {
    fn respond<'a>(&'a self, _s: &'a str, tools: &'a [Value], messages: &'a [Value]) -> BoxFut<'a, Result<Value, ModelError>> {
        Box::pin(async move {
            assert!(!messages.iter().any(|m| m.to_string().contains("046454286")));
            *self.offered.lock().unwrap() = tools.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect();
            Ok(self.replies.lock().unwrap().pop_front().unwrap_or_else(|| json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": "DONE"}], "usage": {"input_tokens": 10, "output_tokens": 2}})))
        })
    }
}
fn tool_use(name: &str, input: Value, input_tokens: u64, output_tokens: u64) -> Value {
    json!({"stop_reason": "tool_use", "content": [{"type": "tool_use", "id": "t1", "name": name, "input": input}], "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens}})
}

#[tokio::test]
async fn model_planner_chains_tools_and_is_never_offered_hard_gates() {
    let (ledger, a) = engine(on());
    let script = Script::new(vec![
        tool_use("read_proposal", json!({}), 800, 40),
        tool_use("add_internal_note", json!({"text": "Checked the feed; remittance still missing"}), 900, 60),
        tool_use("recheck_proposal", json!({}), 900, 20),
    ]);
    let a = a.with_planner(Arc::new(ModelPlanner { model: script.clone() }));
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    let run = a.start("rv1", "p1", REVIEWER).await.unwrap();
    assert_eq!(run.status, RunStatus::VerifiedDone, "{}", run.reason);
    assert_eq!(run.planner, "model");
    assert_eq!(run.steps.len(), 3);
    assert!(run.spent.cost_micro_usd > 0);
    let offered = script.offered.lock().unwrap().clone();
    for gated in ["post_journal", "file_return", "send_email", "notify_client", "stripe_charge", "quickbooks_post", "reveal_identifier"] {
        assert!(!offered.contains(&gated.to_string()), "{gated} offered");
    }

    // A model that names a hard gate anyway is refused.
    let (ledger, a) = engine(on());
    let a = a.with_planner(Arc::new(ModelPlanner { model: Script::new(vec![tool_use("post_journal", json!({"amount": 1}), 10, 10)]) }));
    seed(&ledger, vec![proposal("p1", "acknowledge", "approved_no_execution")]).await;
    assert_eq!(a.start("rv1", "p1", REVIEWER).await.unwrap().status, RunStatus::Refused);
}

async fn call(r: &axum::Router, m: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.9").header("authorization", "Bearer autonomy-test-key");
    let q = match body {
        Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())),
        None => q.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_is_off_by_default_and_refuses_hard_gate_grants() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "autonomy-test-key");
    }
    let r = nxt_os::app_with(Store::memory);
    let (s, v) = call(&r, "GET", "/api/autonomy/status", None).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["autonomous_execution"], "off");
    assert_eq!(v["ai_planner"], "off");
    let (s, v) = call(&r, "POST", "/api/autonomy/runs", Some(json!({"review_id": "rv1", "proposal_id": "p1"}))).await;
    assert_eq!(s, 409, "{v}");
    let (s, v) = call(&r, "POST", "/api/autonomy/policies", Some(json!({"action_class": "cra_filing", "reason": "x", "days": 1, "max_runs_per_day": 1}))).await;
    assert_eq!(s, 403, "{v}");
    let (s, v) = call(&r, "GET", "/api/autonomy/tools", None).await;
    assert_eq!(s, 200);
    assert!(v.as_array().unwrap().iter().any(|t| t["name"] == "send_email" && t["executable"] == false));
}
