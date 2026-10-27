//! HTTP and WebSocket edge. Twilio webhooks are signature-checked.
//! The admin API uses the same workspace key as the rest of NXT OS.
//! The mock call API is how tests and the in-app simulator run a call
//! without a phone line.

use super::config::{SignatureMode, Transport};
use super::models::*;
use super::voice::{self, host_of, param};
use super::GuardVoice;
use crate::referral::routes::{require_admin, ApiError};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn router(gv: Arc<GuardVoice>) -> Router {
    let public = Router::new()
        .route("/api/guardvoice/health", get(health))
        .route("/api/guardvoice/twiml", post(twiml_inbound))
        .route("/api/guardvoice/twiml/gather", post(twiml_gather))
        .route("/api/guardvoice/twiml/outbound", post(twiml_outbound))
        .route("/api/guardvoice/twiml/relay-done", post(twiml_done))
        .route("/api/guardvoice/twiml/recording-status", post(twiml_recording))
        .route("/api/guardvoice/twiml/transfer-whisper", post(twiml_whisper))
        .route("/api/guardvoice/twiml/transfer-screen", post(twiml_screen))
        .route("/api/guardvoice/twiml/transfer-result", post(twiml_transfer_result))
        .route("/api/guardvoice/relay", post(relay_http).get(relay_ws));
    let admin = Router::new()
        .route("/api/guardvoice/status", get(status))
        .route("/api/guardvoice/summary", get(summary))
        .route("/api/guardvoice/activity", get(activity))
        .route("/api/guardvoice/calls", get(list_calls))
        .route("/api/guardvoice/calls/{id}", get(get_call))
        .route("/api/guardvoice/approvals", get(list_approvals))
        .route("/api/guardvoice/approvals/{id}/decide", post(decide))
        .route("/api/guardvoice/notifications", get(list_notes))
        .route("/api/guardvoice/notifications/{id}/read", post(read_note))
        .route("/api/guardvoice/handoffs", get(list_handoffs))
        .route("/api/guardvoice/handoffs/mine", post(mine))
        .route("/api/guardvoice/sms", get(list_sms))
        .route("/api/guardvoice/calls/outbound", post(outbound))
        .route("/api/guardvoice/calls/outbound/{id}", get(get_outbound))
        .route("/api/guardvoice/mock/calls", post(mock_start))
        .route("/api/guardvoice/mock/calls/{id}/say", post(mock_say))
        .route("/api/guardvoice/mock/calls/{id}/interrupt", post(mock_interrupt))
        .route("/api/guardvoice/mock/calls/{id}/hangup", post(mock_hangup))
        .layer(middleware::from_fn(require_admin));
    public.merge(admin).with_state(gv)
}

fn xml(body: String) -> Response {
    (StatusCode::OK, [("content-type", "text/xml; charset=utf-8")], body).into_response()
}

fn err_xml(status: StatusCode) -> Response {
    (status, [("content-type", "text/xml; charset=utf-8")], voice::say_hangup("Sorry, this line can't take the call right now.")).into_response()
}

async fn read_form(req: Request) -> (HeaderMap, String, Vec<(String, String)>) {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 64 * 1024).await.unwrap_or_default();
    let path = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or(parts.uri.path()).to_string();
    let params = voice::form_pairs(&bytes);
    (parts.headers, path, params)
}

fn check_sig(gv: &GuardVoice, headers: &HeaderMap, host: &str, path: &str, params: &[(String, String)]) -> Result<(), Response> {
    if gv.config.signature_mode == SignatureMode::Off || gv.config.auth_token.is_empty() && gv.config.signature_mode != SignatureMode::Enforce {
        if gv.config.signature_mode != SignatureMode::Enforce {
            return Ok(());
        }
    }
    if gv.config.signature_mode == SignatureMode::Off {
        return Ok(());
    }
    let given = headers.get("x-twilio-signature").and_then(|v| v.to_str().ok()).unwrap_or("");
    let url = voice::public_url(host, path);
    let ok = !gv.config.auth_token.is_empty() && voice::signature_matches(&gv.config.auth_token, &url, params, given);
    if ok {
        return Ok(());
    }
    if gv.config.signature_mode == SignatureMode::Log {
        eprintln!("guardvoice: Twilio signature did not match for {path}");
        return Ok(());
    }
    Err(err_xml(StatusCode::FORBIDDEN))
}

async fn health(State(gv): State<Arc<GuardVoice>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "mode": gv.config.status()["mode"],
        "transport": gv.config.status()["transport"],
    }))
}

async fn status(State(gv): State<Arc<GuardVoice>>) -> Json<Value> {
    let mut s = gv.config.status();
    s["storage"] = json!(gv.store.backend_name());
    Json(s)
}

async fn summary(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    gv.summary_counts().await.map(Json).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn activity(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    let v = gv.activity(200).await.map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "activity": v })))
}

fn call_view(c: &Call, full: bool) -> Value {
    let mut v = json!({
        "id": c.id,
        "call_sid": c.call_sid,
        "direction": c.direction,
        "from": c.from,
        "to": c.to,
        "status": c.status,
        "role": c.role,
        "started_at": c.started_at,
        "ended_at": c.ended_at,
        "summary": c.summary,
        "tools_used": c.tools_used,
        "transfer_attempted": c.transfer_attempted,
        "company": c.company,
        "turn_count": c.turns.len(),
    });
    if full {
        v["turns"] = json!(c.turns);
        v["transfer"] = json!(c.transfer);
        v["handoff_id"] = json!(c.handoff_id);
        v["purpose"] = json!(c.purpose);
        v["brief"] = json!(c.brief);
    }
    v
}

async fn list_calls(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    let calls = gv.list_calls().await.map_err(internal)?;
    Ok(Json(json!({ "calls": calls.iter().map(|c| call_view(c, false)).collect::<Vec<_>>() })))
}

async fn get_call(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let c = gv.call_by_id(&id).await.map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    Ok(Json(call_view(&c, true)))
}

async fn list_approvals(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "approvals": gv.approvals().await.map_err(internal)? })))
}

#[derive(Deserialize)]
struct DecideBody {
    decision: String,
}

async fn decide(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>, Json(b): Json<DecideBody>) -> Result<Json<Value>, ApiError> {
    let approve = matches!(b.decision.as_str(), "approve" | "approved" | "yes");
    if !approve && !matches!(b.decision.as_str(), "deny" | "denied" | "no") {
        return Err(ApiError(StatusCode::BAD_REQUEST, "decision must be approve or deny".into()));
    }
    let a = gv.decide(&id, approve).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!(a)))
}

async fn list_notes(State(gv): State<Arc<GuardVoice>>, Query(q): Query<std::collections::HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let mut notes = gv.notifications().await.map_err(internal)?;
    let unread = notes.iter().filter(|n| !n.read).count();
    if q.get("unread").map(|s| s.as_str()) == Some("1") {
        notes.retain(|n| !n.read);
    }
    Ok(Json(json!({ "notifications": notes, "unread": unread })))
}

async fn read_note(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let n = gv.mark_read(&id).await.map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    Ok(Json(json!(n)))
}

async fn list_handoffs(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "handoffs": gv.handoffs().await.map_err(internal)? })))
}

async fn mine(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    gv.mine().await.map(Json).map_err(internal)
}

async fn list_sms(State(gv): State<Arc<GuardVoice>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "messages": gv.sms_outbox().await.map_err(internal)? })))
}

#[derive(Deserialize)]
struct OutboundBody {
    to: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    brief: String,
    #[serde(default)]
    dry_run: bool,
}

async fn outbound(State(gv): State<Arc<GuardVoice>>, Json(b): Json<OutboundBody>) -> Result<Json<Value>, ApiError> {
    let purpose = if b.purpose.is_empty() { "callback".into() } else { b.purpose };
    let brief = if b.brief.is_empty() { "Calling as requested.".into() } else { b.brief };
    // The admin endpoint is the human. dry_run never dials. Mock mode never dials.
    let job = gv.place_outbound(&b.to, &purpose, &brief, b.dry_run).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!(job)))
}

async fn get_outbound(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let job = gv.job(&id).await.map_err(|e| ApiError(StatusCode::NOT_FOUND, e))?;
    Ok(Json(json!(job)))
}

#[derive(Deserialize)]
struct MockStart {
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    direction: String,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    brief: Option<String>,
}

async fn mock_start(State(gv): State<Arc<GuardVoice>>, Json(b): Json<MockStart>) -> Result<Json<Value>, ApiError> {
    let sid = format!("CAmock{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let direction = if b.direction == "outbound" { "outbound" } else { "inbound" };
    let call = gv
        .start_call(NewCall {
            call_sid: sid,
            direction: direction.into(),
            from: if b.from.is_empty() { "+15555550123".into() } else { b.from },
            to: if b.to.is_empty() { gv.config.from_number.clone().unwrap_or_else(|| "+15555550100".into()) } else { b.to },
            role: if direction == "outbound" { "outbound".into() } else { "direct".into() },
            purpose: b.purpose,
            brief: b.brief,
        })
        .await
        .map_err(internal)?;
    let greeting = call.turns.iter().find(|t| t.role == "agent").map(|t| t.text.clone());
    Ok(Json(json!({
        "call": call_view(&call, true),
        "greeting": greeting,
        "token": voice::stream_token(&gv.config.stream_secret, &call.call_sid),
    })))
}

#[derive(Deserialize)]
struct SayBody {
    text: String,
}

async fn mock_say(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>, Json(b): Json<SayBody>) -> Result<Json<Value>, ApiError> {
    let out = gv.on_caller_text(&id, &b.text).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!({
        "say": out.say,
        "ended": out.ended,
        "approvals": out.approvals,
        "tools": out.tools,
        "generation": out.generation,
    })))
}

async fn mock_interrupt(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let call = gv.interrupt(&id).await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!({ "generation": call.generation, "speaking": call.speaking })))
}

async fn mock_hangup(State(gv): State<Arc<GuardVoice>>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let h = gv.hangup(&id, "caller hung up").await.map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(json!(h)))
}

async fn twiml_inbound(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    if host.is_empty() {
        return xml(voice::say_hangup("This line is not configured with a public host yet."));
    }
    let sid = {
        let s = param(&params, "CallSid");
        if s.is_empty() { format!("CAin{}", &uuid::Uuid::new_v4().simple().to_string()[..10]) } else { s.to_string() }
    };
    let call = match gv
        .start_call(NewCall {
            call_sid: sid,
            direction: "inbound".into(),
            from: param(&params, "From").to_string(),
            to: param(&params, "To").to_string(),
            role: "direct".into(),
            purpose: None,
            brief: None,
        })
        .await
    {
        Ok(c) => c,
        Err(_) => return err_xml(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let greet = call.turns.iter().find(|t| t.role == "agent").map(|t| t.text.clone()).unwrap_or_else(|| gv.greeting(&call.company, "direct"));
    xml(voice::inbound_twiml(&gv.config, &host, &call.call_sid, &greet))
}

async fn twiml_gather(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let sid = param(&params, "CallSid").to_string();
    let speech = param(&params, "SpeechResult");
    let Some(call) = gv.call_by_sid(&sid).await.ok().flatten() else {
        return xml(voice::say_hangup("Sorry, I lost the call."));
    };
    if speech.trim().is_empty() {
        return xml(voice::gather_twiml(&host, "I didn't catch that. What do you need?", false));
    }
    match gv.on_caller_text(&call.id, speech).await {
        Ok(out) => {
            let say = out.say.unwrap_or_else(|| "Okay.".into());
            xml(voice::continue_twiml(&gv.config, &host, &call.call_sid, &say, out.ended))
        }
        Err(_) => xml(voice::say_hangup("Sorry, something went wrong. Goodbye.")),
    }
}

async fn twiml_outbound(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let job_id = req_query_job(&path);
    let sid = param(&params, "CallSid").to_string();
    let brief = if let Ok(job) = gv.job(&job_id).await { job.brief } else { "Calling as requested.".into() };
    let purpose = if let Ok(job) = gv.job(&job_id).await { Some(job.purpose) } else { None };
    let call = match gv
        .start_call(NewCall {
            call_sid: if sid.is_empty() { format!("CAout{}", &uuid::Uuid::new_v4().simple().to_string()[..8]) } else { sid },
            direction: "outbound".into(),
            from: param(&params, "From").to_string(),
            to: param(&params, "To").to_string(),
            role: "outbound".into(),
            purpose,
            brief: Some(brief),
        })
        .await
    {
        Ok(c) => c,
        Err(_) => return err_xml(StatusCode::INTERNAL_SERVER_ERROR),
    };
    // Outbound waits for the other person to speak. Empty welcome keeps ConversationRelay quiet.
    let greet = "";
    if gv.config.transport == Transport::Gather {
        return xml(voice::gather_twiml(&host, "Hello.", false));
    }
    xml(voice::relay_twiml(&gv.config, &host, &call.call_sid, greet))
}

fn req_query_job(path: &str) -> String {
    path.split("job=").nth(1).unwrap_or("").split('&').next().unwrap_or("").to_string()
}

async fn twiml_done(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let sid = param(&params, "CallSid");
    if let Ok(Some(call)) = gv.call_by_sid(sid).await {
        if call.status == "active" {
            let _ = gv.hangup(&call.id, "relay disconnected").await;
        }
    }
    xml(voice::empty_response())
}

async fn twiml_recording(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let _ = gv
        .log(
            "recording_status",
            None,
            param(&params, "RecordingStatus"),
            json!({ "call_sid": param(&params, "CallSid") }),
        )
        .await;
    xml(voice::empty_response())
}

async fn twiml_whisper(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let q = query_map(&path);
    let parent = q.get("p").cloned().unwrap_or_default();
    let reason = q.get("r").cloned().unwrap_or_default();
    let name = q.get("n").cloned().unwrap_or_default();
    let label = gv.company_name().await;
    xml(voice::whisper_twiml(&host, &parent, &label, &name, &reason))
}

async fn twiml_screen(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let parent = query_map(&path).get("p").cloned().unwrap_or_else(|| param(&params, "ParentCallSid").to_string());
    let digit = param(&params, "Digits");
    match gv.apply_screen(&parent, digit).await {
        Ok(body) => xml(body),
        Err(_) => xml(voice::screen_twiml("")),
    }
}

async fn twiml_transfer_result(State(gv): State<Arc<GuardVoice>>, req: Request) -> Response {
    let (headers, path, params) = read_form(req).await;
    let host = host_of(&gv.config, &headers);
    if let Err(r) = check_sig(&gv, &headers, &host, &path, &params) {
        return r;
    }
    let sid = param(&params, "CallSid");
    match gv.apply_transfer_result(sid, param(&params, "DialCallStatus"), param(&params, "CallStatus"), &host).await {
        Ok(body) => xml(body),
        Err(_) => xml(voice::say_hangup("Goodbye.")),
    }
}

fn query_map(path: &str) -> std::collections::HashMap<String, String> {
    let q = path.split('?').nth(1).unwrap_or("");
    voice::form_pairs(q.as_bytes()).into_iter().collect()
}

#[derive(Deserialize)]
struct RelayQuery {
    #[serde(default, rename = "callSid")]
    call_sid: String,
    #[serde(default)]
    token: String,
}

async fn relay_http(State(gv): State<Arc<GuardVoice>>, Query(q): Query<RelayQuery>, Json(msg): Json<Value>) -> Result<Json<Value>, ApiError> {
    let frames = handle_relay(&gv, &q, msg).await.map_err(|e| ApiError(StatusCode::UNAUTHORIZED, e))?;
    Ok(Json(json!({ "messages": frames })))
}

async fn relay_ws(ws: WebSocketUpgrade, State(gv): State<Arc<GuardVoice>>, Query(q): Query<RelayQuery>) -> Response {
    if q.call_sid.is_empty() || !voice::token_ok(&gv.config.stream_secret, &q.call_sid, &q.token) {
        return ApiError(StatusCode::UNAUTHORIZED, "bad stream token".into()).into_response();
    }
    ws.on_upgrade(move |socket| relay_loop(gv, q, socket))
}

async fn relay_loop(gv: Arc<GuardVoice>, q: RelayQuery, mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        match handle_relay(&gv, &q, v).await {
            Ok(frames) => {
                for f in frames {
                    if socket.send(Message::Text(f.to_string().into())).await.is_err() {
                        return;
                    }
                }
            }
            Err(_) => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
        }
    }
}

async fn handle_relay(gv: &GuardVoice, q: &RelayQuery, msg: Value) -> Result<Vec<Value>, String> {
    let kind = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let sid = msg.get("callSid").and_then(|s| s.as_str()).filter(|s| !s.is_empty()).unwrap_or(q.call_sid.as_str());
    if sid.is_empty() || !voice::token_ok(&gv.config.stream_secret, sid, &q.token) {
        // setup may carry the token in customParameters when the socket URL had it stripped
        let token = msg.pointer("/customParameters/token").and_then(|t| t.as_str()).unwrap_or("");
        if !voice::token_ok(&gv.config.stream_secret, sid, token) && !voice::token_ok(&gv.config.stream_secret, sid, &q.token) {
            return Err("bad stream token".into());
        }
    }
    match kind {
        "setup" | "connected" => {
            let from = msg.get("from").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let to = msg.get("to").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let role = msg.pointer("/customParameters/role").and_then(|s| s.as_str()).unwrap_or("direct");
            let _ = gv
                .start_call(NewCall {
                    call_sid: sid.to_string(),
                    direction: if role == "outbound" { "outbound".into() } else { "inbound".into() },
                    from,
                    to,
                    role: role.into(),
                    purpose: None,
                    brief: None,
                })
                .await?;
            Ok(vec![])
        }
        "prompt" => {
            let text = msg.get("voicePrompt").and_then(|s| s.as_str()).unwrap_or("");
            let Some(call) = gv.call_by_sid(sid).await? else {
                return Err("unknown call".into());
            };
            let out = gv.on_caller_text(&call.id, text).await?;
            let mut frames = Vec::new();
            if let Some(say) = out.say {
                frames.push(json!({ "type": "text", "token": say, "last": true }));
            }
            if out.ended {
                frames.push(json!({ "type": "end", "handoffData": "ended" }));
            }
            Ok(frames)
        }
        "interrupt" => {
            if let Some(call) = gv.call_by_sid(sid).await? {
                gv.interrupt(&call.id).await?;
            }
            Ok(vec![])
        }
        "dtmf" => {
            let digit = msg.get("digit").and_then(|s| s.as_str()).unwrap_or("");
            let Some(call) = gv.call_by_sid(sid).await? else {
                return Ok(vec![]);
            };
            let phrase = if digit == "0" { "I need to speak to the owner" } else { "" };
            if phrase.is_empty() {
                return Ok(vec![json!({ "type": "text", "token": "I can look up the books or take a message. What do you need?", "last": true })]);
            }
            let out = gv.on_caller_text(&call.id, phrase).await?;
            let say = out.say.unwrap_or_else(|| "Okay.".into());
            Ok(vec![json!({ "type": "text", "token": say, "last": true })])
        }
        "stop" | "close" => {
            if let Some(call) = gv.call_by_sid(sid).await? {
                if call.status == "active" {
                    gv.hangup(&call.id, "relay closed").await?;
                }
            }
            Ok(vec![])
        }
        "error" => {
            let _ = gv.log("relay_error", None, msg.get("description").and_then(|s| s.as_str()).unwrap_or("relay error"), json!({})).await;
            Ok(vec![])
        }
        _ => Ok(vec![]),
    }
}

fn internal(e: String) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e)
}
