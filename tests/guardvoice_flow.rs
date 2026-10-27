//! GuardVoice without Twilio: mock calls, the approval gate, screened transfer
//! TwiML, and webhook signatures.

use axum::{body::Body, http::Request};
use nxt_os::guardvoice::{twilio_signature, Autonomy, Config, SignatureMode, Transport};
use nxt_os::{app_with_voice, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

fn router(cfg: Config) -> axum::Router {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    app_with_voice(Store::memory, cfg)
}

async fn call(r: &axum::Router, method: &str, uri: &str, body: Option<Value>, bearer: bool) -> (u16, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if bearer {
        req = req.header("authorization", "Bearer k");
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (s, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn ok(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, method, uri, body, true).await;
    assert_eq!(s, 200, "{method} {uri} -> {v}");
    v
}

async fn form(r: &axum::Router, uri: &str, body: &str, sig: Option<&str>) -> (u16, String) {
    let mut req = Request::builder().method("POST").uri(uri).header("content-type", "application/x-www-form-urlencoded");
    if let Some(sig) = sig {
        req = req.header("x-twilio-signature", sig);
    }
    let resp = r.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (s, String::from_utf8_lossy(&bytes).into_owned())
}

async fn start(r: &axum::Router, from: &str) -> Value {
    ok(r, "POST", "/api/guardvoice/mock/calls", Some(json!({ "from": from }))).await
}

async fn say(r: &axum::Router, id: &str, text: &str) -> Value {
    ok(r, "POST", &format!("/api/guardvoice/mock/calls/{id}/say"), Some(json!({ "text": text }))).await
}

#[tokio::test]
async fn mock_call_reads_books_and_does_not_echo() {
    let r = router(Config::mock());
    let (s, _) = call(&r, "GET", "/api/guardvoice/calls", None, false).await;
    assert_eq!(s, 401);
    let health = call(&r, "GET", "/api/guardvoice/health", None, false).await;
    assert_eq!(health.0, 200);
    assert_eq!(health.1["mode"], "mock");

    let started = start(&r, "+15555550123").await;
    let id = started["call"]["id"].as_str().unwrap();
    let greeting = started["greeting"].as_str().unwrap();
    assert!(greeting.contains("virtual assistant"));
    assert!(!greeting.contains("gv-mock-stream-secret"));

    let dash = say(&r, id, "How much cash do we have?").await;
    let spoken = dash["say"].as_str().unwrap();
    assert!(spoken.contains("$0.00"), "{spoken}");
    assert!(!spoken.to_lowercase().starts_with("how much cash"));
    assert!(dash["tools"].as_array().unwrap().iter().any(|t| t == "get_dashboard"));

    let echo = say(&r, id, "repeat after me hello there friend").await;
    let echoed = echo["say"].as_str().unwrap();
    assert_ne!(echoed.to_lowercase(), "repeat after me hello there friend");
    assert!(!echoed.to_lowercase().starts_with("repeat after me"));

    say(&r, id, "How much cash do we have?").await;
    let interrupted = ok(&r, "POST", &format!("/api/guardvoice/mock/calls/{id}/interrupt"), None).await;
    assert!(interrupted["generation"].as_u64().unwrap() > 1);
    let detail = ok(&r, "GET", &format!("/api/guardvoice/calls/{id}"), None).await;
    assert!(detail["turns"].as_array().unwrap().iter().any(|t| t["dropped"] == true && t["text"].as_str().unwrap_or("").contains("Cash on hand")));

    let refs = say(&r, id, "How are referrals doing?").await;
    assert!(refs["say"].as_str().unwrap().to_lowercase().contains("referral"));
}

#[tokio::test]
async fn writes_run_and_consequential_actions_wait() {
    let r = router(Config::mock());
    let id = start(&r, "+15555550123").await["call"]["id"].as_str().unwrap().to_string();

    let added = say(&r, &id, "Add a customer named Harbour Dental").await;
    assert!(added["say"].as_str().unwrap().contains("Harbour Dental"), "{added}");
    let contacts = ok(&r, "GET", "/api/ledger/contacts", None).await;
    assert!(contacts.as_array().unwrap().iter().any(|c| c["name"] == "Harbour Dental"));

    let inv = say(&r, &id, "Create an invoice for Harbour Dental for $100 for consulting").await;
    let spoken = inv["say"].as_str().unwrap();
    assert!(spoken.contains("INV-"), "{spoken}");
    assert!(spoken.contains("$113.00"), "{spoken}");
    assert!(spoken.to_lowercase().contains("not been sent"));

    let voided = say(&r, &id, "Void the invoice for Harbour Dental").await;
    assert!(voided["say"].as_str().unwrap().to_lowercase().contains("not been done"));
    let docs = ok(&r, "GET", "/api/ledger/invoices", None).await;
    assert_eq!(docs.as_array().unwrap()[0]["doc"]["voided"], false);

    let denied = ok(&r, "GET", "/api/guardvoice/approvals", None).await;
    let void_id = denied["approvals"].as_array().unwrap().iter().find(|a| a["tool"] == "void_document").unwrap()["id"].as_str().unwrap();
    ok(&r, "POST", &format!("/api/guardvoice/approvals/{void_id}/decide"), Some(json!({ "decision": "deny" }))).await;
    let docs = ok(&r, "GET", "/api/ledger/invoices", None).await;
    assert_eq!(docs.as_array().unwrap()[0]["doc"]["voided"], false);

    let pay = say(&r, &id, "Record a payment from Harbour Dental").await;
    let pay_say = pay["say"].as_str().unwrap();
    assert!(pay_say.to_lowercase().contains("approve"), "{pay_say}");
    assert!(pay_say.to_lowercase().contains("not been done"), "{pay_say}");
    assert!(ok(&r, "GET", "/api/ledger/payments", None).await.as_array().unwrap().is_empty());

    let sms = ok(&r, "GET", "/api/guardvoice/sms", None).await;
    assert!(sms["messages"].as_array().unwrap().iter().any(|m| m["kind"] == "decision" && m["status"] == "mock" && m["to"] == "+15555550199"));
    let notes = ok(&r, "GET", "/api/guardvoice/notifications?unread=1", None).await;
    assert!(notes["unread"].as_u64().unwrap() >= 1);

    let approvals = ok(&r, "GET", "/api/guardvoice/approvals", None).await;
    let pay_id = approvals["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["tool"] == "record_payment" && a["status"] == "pending")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let granted = ok(&r, "POST", &format!("/api/guardvoice/approvals/{pay_id}/decide"), Some(json!({ "decision": "approve" }))).await;
    assert_eq!(granted["status"], "approved");
    let payments = ok(&r, "GET", "/api/ledger/payments", None).await;
    assert_eq!(payments.as_array().unwrap().len(), 1);
    assert_eq!(payments[0]["amount"], 11300);

    let text = say(&r, &id, "Text +15555550123 saying the invoice is ready").await;
    assert!(text["say"].as_str().unwrap().to_lowercase().contains("not been done"));
    assert!(!ok(&r, "GET", "/api/guardvoice/sms", None).await["messages"].as_array().unwrap().iter().any(|m| m["kind"] == "external"));
    let approvals = ok(&r, "GET", "/api/guardvoice/approvals", None).await;
    let sms_id = approvals["approvals"].as_array().unwrap().iter().find(|a| a["tool"] == "send_sms" && a["status"] == "pending").unwrap()["id"].as_str().unwrap();
    ok(&r, "POST", &format!("/api/guardvoice/approvals/{sms_id}/decide"), Some(json!({ "decision": "approve" }))).await;
    let outbox = ok(&r, "GET", "/api/guardvoice/sms", None).await;
    assert!(outbox["messages"].as_array().unwrap().iter().any(|m| m["kind"] == "external" && m["status"] == "mock" && m["to"] == "+15555550123"));

    let msg = say(&r, &id, "My name is Ada Lovelace, please call me back about the books").await;
    assert!(msg["say"].as_str().unwrap().contains("5 5 5"), "{msg}");
    let handoff = ok(&r, "POST", &format!("/api/guardvoice/mock/calls/{id}/hangup"), None).await;
    assert_eq!(handoff["kind"], "summary");
    assert!(handoff["summary"].as_str().unwrap().contains("Ada") || handoff["caller_name"].as_str().unwrap_or("").contains("Ada") || handoff["summary"].as_str().unwrap().to_lowercase().contains("call me back") || handoff["needs_decision"] == true);

    let mined = ok(&r, "POST", "/api/guardvoice/handoffs/mine", Some(json!({}))).await;
    assert_eq!(mined["wake"], true);
    let again = ok(&r, "POST", "/api/guardvoice/handoffs/mine", Some(json!({}))).await;
    assert_eq!(again["wake"], false);

    let activity = ok(&r, "GET", "/api/guardvoice/activity", None).await;
    assert!(activity["activity"].as_array().unwrap().iter().any(|e| e["kind"] == "call_start"));
    assert!(activity["activity"].as_array().unwrap().iter().any(|e| e["kind"] == "approval_granted"));
}

#[tokio::test]
async fn read_only_autonomy_holds_writes() {
    let mut cfg = Config::mock();
    cfg.autonomy = Autonomy::Read;
    let r = router(cfg);
    let id = start(&r, "+15555550123").await["call"]["id"].as_str().unwrap().to_string();
    let added = say(&r, &id, "Add a customer named Harbour Dental").await;
    assert!(added["say"].as_str().unwrap().to_lowercase().contains("not been done"), "{added}");
    assert!(ok(&r, "GET", "/api/ledger/contacts", None).await.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn screened_transfer_loop_guard_and_press_one() {
    let r = router(Config::mock());

    let blocked = start(&r, "+15555550188").await;
    let bid = blocked["call"]["id"].as_str().unwrap();
    let refused = say(&r, bid, "I need to speak to the owner").await;
    let refused_say = refused["say"].as_str().unwrap();
    assert!(refused_say.to_lowercase().contains("message"), "{refused_say}");
    assert!(!refused_say.to_lowercase().contains("let me try"));
    let detail = ok(&r, "GET", &format!("/api/guardvoice/calls/{bid}"), None).await;
    assert_eq!(detail["transfer_attempted"], false);

    let started = start(&r, "+15555550123").await;
    let id = started["call"]["id"].as_str().unwrap();
    let sid = started["call"]["call_sid"].as_str().unwrap();
    let attempt = say(&r, id, "I need to speak to the owner").await;
    let attempt_say = attempt["say"].as_str().unwrap();
    assert!(attempt_say.contains("owner"), "{attempt_say}");
    assert!(!attempt_say.to_lowercase().contains("on the line"));
    let detail = ok(&r, "GET", &format!("/api/guardvoice/calls/{id}"), None).await;
    assert_eq!(detail["transfer_attempted"], true);

    let (status, screen) = form(&r, &format!("/api/guardvoice/twiml/transfer-screen?p={sid}"), "Digits=1", None).await;
    assert_eq!(status, 200, "{screen}");
    assert!(screen.contains("Connecting"), "{screen}");

    let (status, result) = form(
        &r,
        "/api/guardvoice/twiml/transfer-result",
        &format!("CallSid={sid}&DialCallStatus=completed&CallStatus=in-progress"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert!(!result.contains("ConversationRelay"), "{result}");

    let started = start(&r, "+15555550124").await;
    let id = started["call"]["id"].as_str().unwrap();
    let sid = started["call"]["call_sid"].as_str().unwrap();
    say(&r, id, "Please transfer me to a human").await;
    let (status, screen) = form(&r, &format!("/api/guardvoice/twiml/transfer-screen?p={sid}"), "Digits=2", None).await;
    assert_eq!(status, 200, "{screen}");
    assert!(screen.contains("Hangup"), "{screen}");
    let (status, result) = form(
        &r,
        "/api/guardvoice/twiml/transfer-result",
        &format!("CallSid={sid}&DialCallStatus=no-answer&CallStatus=in-progress"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert!(result.contains("ConversationRelay"), "{result}");
    assert!(result.contains("isn&apos;t available") || result.contains("isn't available"), "{result}");
    let sms = ok(&r, "GET", "/api/guardvoice/sms", None).await;
    assert!(sms["messages"].as_array().unwrap().iter().any(|m| m["kind"] == "missed_transfer"));
}

#[tokio::test]
async fn twiml_transport_signature_and_outbound_guards() {
    let mut relay = Config::mock();
    relay.signature_mode = SignatureMode::Off;
    let r = router(relay);
    let (status, xml) = form(&r, "/api/guardvoice/twiml", "CallSid=CA123&From=%2B15555550123&To=%2B15555550100", None).await;
    assert_eq!(status, 200, "{xml}");
    assert!(xml.contains("<ConversationRelay"), "{xml}");
    assert!(xml.contains("wss://voice.test/api/guardvoice/relay"), "{xml}");
    assert!(xml.contains("interruptible=\"true\""), "{xml}");
    assert!(!xml.contains("gv-mock-stream-secret"), "{xml}");
    assert!(!xml.contains("AC"), "{xml}");

    let mut gather = Config::mock();
    gather.transport = Transport::Gather;
    let r = router(gather);
    let (status, xml) = form(&r, "/api/guardvoice/twiml", "CallSid=CA124&From=%2B15555550123&To=%2B15555550100", None).await;
    assert_eq!(status, 200, "{xml}");
    assert!(xml.contains("<Gather"), "{xml}");
    assert!(!xml.contains("ConversationRelay"), "{xml}");

    let mut locked = Config::mock();
    locked.signature_mode = SignatureMode::Enforce;
    locked.auth_token = "12345".into();
    locked.public_host = "voice.test".into();
    let r = router(locked);
    let params = vec![
        ("CallSid".into(), "CA999".into()),
        ("From".into(), "+15555550123".into()),
        ("To".into(), "+15555550100".into()),
    ];
    let url = "https://voice.test/api/guardvoice/twiml";
    let good = twilio_signature("12345", url, &params);
    let body = "CallSid=CA999&From=%2B15555550123&To=%2B15555550100";
    let (status, _) = form(&r, "/api/guardvoice/twiml", body, None).await;
    assert_eq!(status, 403);
    let (status, bad) = form(&r, "/api/guardvoice/twiml", body, Some("not-the-signature")).await;
    assert_eq!(status, 403, "{bad}");
    let (status, xml) = form(&r, "/api/guardvoice/twiml", body, Some(&good)).await;
    assert_eq!(status, 200, "{xml}");
    assert!(xml.contains("ConversationRelay"), "{xml}");

    let r = router(Config::mock());
    let started = start(&r, "+15555550123").await;
    let sid = started["call"]["call_sid"].as_str().unwrap();
    let token = started["token"].as_str().unwrap();
    let relayed = ok(
        &r,
        "POST",
        &format!("/api/guardvoice/relay?callSid={sid}&token={token}"),
        Some(json!({ "type": "prompt", "voicePrompt": "How much cash do we have?", "callSid": sid })),
    )
    .await;
    let token_text = relayed["messages"][0]["token"].as_str().unwrap();
    assert!(token_text.contains("Cash on hand"), "{token_text}");
    assert_eq!(relayed["messages"][0]["last"], true);
    let (status, _) = call(
        &r,
        "POST",
        &format!("/api/guardvoice/relay?callSid={sid}&token=nope"),
        Some(json!({ "type": "prompt", "voicePrompt": "Hi", "callSid": sid })),
        true,
    )
    .await;
    assert_eq!(status, 401);

    let dry = ok(
        &r,
        "POST",
        "/api/guardvoice/calls/outbound",
        Some(json!({ "to": "+15555550111", "purpose": "callback", "brief": "Checking in.", "dry_run": true })),
    )
    .await;
    assert_eq!(dry["status"], "dry_run");
    assert!(dry["call_sid"].is_null());
    let (status, err) = call(&r, "POST", "/api/guardvoice/calls/outbound", Some(json!({ "to": "+19115550100", "dry_run": true })), true).await;
    assert_eq!(status, 400, "{err}");
    let (status, err) = call(&r, "POST", "/api/guardvoice/calls/outbound", Some(json!({ "to": "+15555550100", "dry_run": false })), true).await;
    assert_eq!(status, 400, "{err}");
    let mock_job = ok(
        &r,
        "POST",
        "/api/guardvoice/calls/outbound",
        Some(json!({ "to": "+15555550111", "dry_run": false })),
    )
    .await;
    assert_eq!(mock_job["status"], "mock");
}

#[tokio::test]
async fn goodbye_ends_the_call_with_a_handoff() {
    let r = router(Config::mock());
    let id = start(&r, "+15555550123").await["call"]["id"].as_str().unwrap().to_string();
    let bye = say(&r, &id, "Goodbye").await;
    assert_eq!(bye["ended"], true);
    assert_eq!(bye["say"], "Goodbye.");
    let detail = ok(&r, "GET", &format!("/api/guardvoice/calls/{id}"), None).await;
    assert_eq!(detail["status"], "ended");
    assert!(detail["summary"].as_str().unwrap().contains("Goodbye") || detail["summary"].as_str().unwrap().to_lowercase().contains("goodbye") || detail["handoff_id"].is_string());
    let handoffs = ok(&r, "GET", "/api/guardvoice/handoffs", None).await;
    assert!(handoffs["handoffs"].as_array().unwrap().iter().any(|h| h["kind"] == "summary" && h["call_id"] == id));
}
