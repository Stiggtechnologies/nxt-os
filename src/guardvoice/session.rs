//! Call records, the activity log, notifications, SMS outbox, and the
//! end-of-call handoff. Tool execution lives in `tools`.

use super::config::Mode;
use hmac::{KeyInit, Mac};
use super::models::*;
use super::voice::{self, finalize_speech, gate_speech, mask_number};
use super::GuardVoice;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Value, json};

const CALLS: usize = 200;
const NOTES: usize = 200;
const APPROVALS: usize = 200;
const HANDOFFS: usize = 200;
const SMS: usize = 200;
const JOBS: usize = 100;
const ACTIVITY: usize = 500;

impl GuardVoice {
    pub(super) async fn log(&self, kind: &str, call_id: Option<&str>, summary: &str, detail: Value) -> Result<(), String> {
        let ev = json!({
            "at": now(),
            "kind": kind,
            "call_id": call_id,
            "summary": summary,
            "detail": detail,
        });
        self.store.lpush_capped("gv:activity", &ev.to_string(), ACTIVITY).await
    }

    async fn push_id(&self, list: &str, id: &str, cap: usize) -> Result<(), String> {
        self.store.lpush_capped(list, id, cap).await
    }

    async fn load_many<T: DeserializeOwned>(&self, list: &str, prefix: &str, limit: usize) -> Result<Vec<T>, String> {
        let ids = self.store.lrange(list, limit).await?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(v) = self.store.get_json::<T>(&format!("{prefix}{id}")).await? {
                out.push(v);
            }
        }
        Ok(out)
    }

    async fn put<T: Serialize>(&self, key: &str, v: &T) -> Result<(), String> {
        self.store.set_json(key, v).await
    }

    pub(super) async fn company_name(&self) -> String {
        match self.ledger.company().await {
            Ok(c) if !c.name.trim().is_empty() => c.name,
            _ => self.config.company_fallback.clone(),
        }
    }

    pub(super) fn greeting(&self, company: &str, role: &str) -> String {
        if role == "transfer-failed" {
            return format!(
                "Sorry, {} isn't available right now. I can take a message and make sure they get it.",
                self.config.transfer_person
            );
        }
        if let Some(w) = &self.config.welcome {
            if role == "direct" {
                return w.clone();
            }
        }
        format!(
            "Hi, thanks for calling {company}. This is {}, the virtual assistant. How can I help?",
            self.config.agent_name
        )
    }

    pub(super) async fn start_call(&self, new: NewCall) -> Result<Call, String> {
        if let Some(existing) = self.call_by_sid(&new.call_sid).await? {
            return Ok(existing);
        }
        let company = self.company_name().await;
        let greet = if new.direction == "outbound" {
            None
        } else {
            Some(self.greeting(&company, &new.role))
        };
        let mut call = Call {
            id: new_id("call"),
            call_sid: new.call_sid,
            direction: new.direction,
            from: new.from,
            to: new.to,
            status: "active".into(),
            role: new.role,
            started_at: now(),
            ended_at: None,
            turns: Vec::new(),
            generation: 1,
            speaking: false,
            tools_used: Vec::new(),
            transfer_attempted: false,
            transfer: None,
            summary: None,
            handoff_id: None,
            purpose: new.purpose,
            brief: new.brief,
            company,
            owner_notified: false,
        };
        if let Some(g) = greet {
            call.speaking = true;
            call.turns.push(Turn { role: "agent".into(), text: g, at: now(), generation: call.generation, dropped: false });
        }
        self.put(&format!("gv:call:{}", call.id), &call).await?;
        self.store.set(&format!("gv:sid:{}", call.call_sid), &call.id).await?;
        self.push_id("gv:call_ids", &call.id, CALLS).await?;
        self.log(
            "call_start",
            Some(&call.id),
            &format!("{} call from {}", call.direction, mask_number(&call.from)),
            json!({ "call_sid": call.call_sid, "role": call.role }),
        )
        .await?;
        Ok(call)
    }

    pub(super) async fn call_by_id(&self, id: &str) -> Result<Call, String> {
        self.store
            .get_json(&format!("gv:call:{id}"))
            .await?
            .ok_or_else(|| format!("call {id} not found"))
    }

    pub(super) async fn call_by_sid(&self, sid: &str) -> Result<Option<Call>, String> {
        let Some(id) = self.store.get(&format!("gv:sid:{sid}")).await? else {
            return Ok(None);
        };
        Ok(Some(self.call_by_id(&id).await?))
    }

    pub(super) async fn save_call(&self, call: &Call) -> Result<(), String> {
        self.put(&format!("gv:call:{}", call.id), call).await
    }

    pub(super) async fn patch_call(&self, id: &str, f: impl FnOnce(&mut Call)) -> Result<Call, String> {
        let mut call = self.call_by_id(id).await?;
        f(&mut call);
        self.save_call(&call).await?;
        Ok(call)
    }

    pub(super) async fn list_calls(&self) -> Result<Vec<Call>, String> {
        self.load_many("gv:call_ids", "gv:call:", CALLS).await
    }

    /// Caller finished a phrase. The spoken reply is dropped if they barge in
    /// (generation changes) before we commit it, and a leading echo of their
    /// words is stripped.
    pub(super) async fn on_caller_text(&self, call_id: &str, text: &str) -> Result<TurnOutcome, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(TurnOutcome { say: None, ended: false, approvals: vec![], tools: vec![], generation: 0 });
        }
        let text: String = text.chars().take(2000).collect();
        let mut call = self.call_by_id(call_id).await?;
        if call.status != "active" {
            return Err("that call has ended".into());
        }
        if call.speaking {
            call.speaking = false;
        }
        let generation = call.generation;
        call.turns.push(Turn { role: "caller".into(), text: text.clone(), at: now(), generation, dropped: false });
        self.save_call(&call).await?;

        let thought = self.think(&call, &text).await?;
        let mut call = self.call_by_id(call_id).await?;
        let spoken = gate_speech(generation, call.generation, &text, &thought.say);
        match &spoken {
            None => {
                call.turns.push(Turn {
                    role: "agent".into(),
                    text: finalize_speech(&text, &thought.say),
                    at: now(),
                    generation,
                    dropped: true,
                });
                self.log("barge_in", Some(call_id), "Dropped a reply the caller interrupted.", json!({ "generation": generation }))
                    .await?;
            }
            Some(say) => {
                call.speaking = true;
                call.turns.push(Turn { role: "agent".into(), text: say.clone(), at: now(), generation: call.generation, dropped: false });
                for t in &thought.tools {
                    if !call.tools_used.contains(t) {
                        call.tools_used.push(t.clone());
                    }
                }
            }
        }
        let ended = thought.end_call && spoken.is_some();
        self.save_call(&call).await?;
        let say = spoken;
        if ended {
            self.hangup(call_id, "assistant ended the call").await?;
        }
        Ok(TurnOutcome {
            say,
            ended,
            approvals: thought.approval_ids,
            tools: thought.tools,
            generation: self.call_by_id(call_id).await?.generation,
        })
    }

    pub(super) async fn interrupt(&self, call_id: &str) -> Result<Call, String> {
        let call = self
            .patch_call(call_id, |c| {
                c.generation = c.generation.saturating_add(1);
                if c.speaking {
                    if let Some(t) = c.turns.iter_mut().rev().find(|t| t.role == "agent" && !t.dropped) {
                        t.dropped = true;
                    }
                    c.speaking = false;
                }
            })
            .await?;
        self.log("interrupt", Some(call_id), "Caller interrupted playback.", json!({ "generation": call.generation })).await?;
        Ok(call)
    }

    pub(super) async fn hangup(&self, call_id: &str, reason: &str) -> Result<Handoff, String> {
        let mut call = self.call_by_id(call_id).await?;
        if call.status == "ended" {
            if let Some(id) = &call.handoff_id {
                if let Some(h) = self.store.get_json(&format!("gv:handoff:{id}")).await? {
                    return Ok(h);
                }
            }
        }
        call.status = "ended".into();
        call.ended_at = Some(now());
        call.speaking = false;
        let transcript = transcript_text(&call);
        let hints = score_transcript(&transcript);
        let pending = self.pending_for(call_id).await?;
        let needs = !pending.is_empty()
            || hints.actionable
            || call.transfer.as_ref().is_some_and(|t| t.outcome.as_deref() == Some("reconnect") || t.declined);
        let summary = build_summary(&call, &hints, &pending, reason);
        call.summary = Some(summary.clone());
        let handoff = Handoff {
            id: new_id("hof"),
            call_id: call.id.clone(),
            call_sid: call.call_sid.clone(),
            kind: "summary".into(),
            at: now(),
            from: call.from.clone(),
            caller_name: hints.name.clone(),
            callback_phone: hints.phone.clone().or_else(|| Some(call.from.clone())),
            summary: summary.clone(),
            urgency: if needs { "soon".into() } else { "normal".into() },
            needs_decision: needs,
            status: "open".into(),
        };
        call.handoff_id = Some(handoff.id.clone());
        self.put(&format!("gv:handoff:{}", handoff.id), &handoff).await?;
        self.push_id("gv:handoff_ids", &handoff.id, HANDOFFS).await?;
        self.save_call(&call).await?;
        self.log("call_end", Some(call_id), &summary, json!({ "reason": reason, "needs_decision": needs })).await?;
        self.notify(
            if needs { "warn" } else { "info" },
            if needs { "Call needs you" } else { "Call summary" },
            &summary,
            "handoffs",
            Some(call_id),
        )
        .await?;
        if needs && !call.owner_notified {
            self.owner_sms(&format!("[NXT GuardVoice] {summary}"), Some(call_id), "summary").await?;
            self.patch_call(call_id, |c| c.owner_notified = true).await?;
        }
        let note = json!({
            "call_id": call.id,
            "call_sid": call.call_sid,
            "from": call.from,
            "to": call.to,
            "direction": call.direction,
            "company": call.company,
            "summary": summary,
            "needs_decision": needs,
            "tools": call.tools_used,
        });
        self.dispatch_note(&note).await;
        Ok(handoff)
    }

    pub(super) async fn notify(&self, level: &str, title: &str, body: &str, href: &str, call_id: Option<&str>) -> Result<Notification, String> {
        let n = Notification {
            id: new_id("ntf"),
            at: now(),
            level: level.into(),
            title: title.into(),
            body: body.chars().take(600).collect(),
            href: href.into(),
            read: false,
            call_id: call_id.map(str::to_string),
        };
        self.put(&format!("gv:note:{}", n.id), &n).await?;
        self.push_id("gv:note_ids", &n.id, NOTES).await?;
        Ok(n)
    }

    pub(super) async fn notifications(&self) -> Result<Vec<Notification>, String> {
        self.load_many("gv:note_ids", "gv:note:", NOTES).await
    }

    pub(super) async fn mark_read(&self, id: &str) -> Result<Notification, String> {
        let mut n: Notification = self.store.get_json(&format!("gv:note:{id}")).await?.ok_or_else(|| format!("notification {id} not found"))?;
        n.read = true;
        self.put(&format!("gv:note:{id}"), &n).await?;
        Ok(n)
    }

    /// Texts the owner. In mock mode (or without Twilio credentials) the
    /// message is stored in the outbox and nothing leaves the process.
    pub(super) async fn owner_sms(&self, body: &str, call_id: Option<&str>, kind: &str) -> Result<(), String> {
        let Some(to) = self.config.owner_sms.clone() else {
            self.log("sms_skipped", call_id, "Owner SMS number is not configured.", json!({ "kind": kind })).await?;
            return Ok(());
        };
        self.deliver_sms(&to, body, call_id, kind).await?;
        if let Some(id) = call_id {
            let _ = self.patch_call(id, |c| c.owner_notified = true).await;
        }
        Ok(())
    }

    pub(super) async fn deliver_sms(&self, to: &str, body: &str, call_id: Option<&str>, kind: &str) -> Result<SmsMessage, String> {
        let to = voice::e164(to).ok_or_else(|| format!("not an E.164 number: {to}"))?;
        if voice::is_blocked_service(&to) {
            return Err("that number can't be texted".into());
        }
        let from = self.config.from_number.clone().unwrap_or_default();
        let body: String = body.chars().take(600).collect();
        let mut msg = SmsMessage {
            id: new_id("sms"),
            at: now(),
            to: to.clone(),
            from: from.clone(),
            body: body.clone(),
            status: "mock".into(),
            kind: kind.into(),
            call_id: call_id.map(str::to_string),
        };
        if self.config.mode == Mode::Live && !self.config.account_sid.is_empty() && !self.config.auth_token.is_empty() && !from.is_empty() {
            match self.twilio_form(&format!("/2010-04-01/Accounts/{}/Messages.json", self.config.account_sid), &[("To", to.as_str()), ("From", from.as_str()), ("Body", body.as_str())]).await {
                Ok(_) => msg.status = "sent".into(),
                Err(e) => {
                    msg.status = "failed".into();
                    self.log("sms_failed", call_id, &e, json!({ "to": mask_number(&to) })).await?;
                }
            }
        }
        self.put(&format!("gv:sms:{}", msg.id), &msg).await?;
        self.push_id("gv:sms_ids", &msg.id, SMS).await?;
        self.log("sms", call_id, &format!("{kind} SMS {status} to {to}", status = msg.status, to = mask_number(&to)), json!({ "sms_id": msg.id }))
            .await?;
        Ok(msg)
    }

    pub(super) async fn sms_outbox(&self) -> Result<Vec<SmsMessage>, String> {
        self.load_many("gv:sms_ids", "gv:sms:", SMS).await
    }

    pub(super) async fn activity(&self, limit: usize) -> Result<Vec<Value>, String> {
        self.store.list_json("gv:activity", limit.min(ACTIVITY)).await
    }

    pub(super) async fn handoffs(&self) -> Result<Vec<Handoff>, String> {
        self.load_many("gv:handoff_ids", "gv:handoff:", HANDOFFS).await
    }

    pub(super) async fn save_handoff(&self, h: &Handoff) -> Result<(), String> {
        self.put(&format!("gv:handoff:{}", h.id), h).await?;
        self.push_id("gv:handoff_ids", &h.id, HANDOFFS).await
    }

    /// Structured stand-in for the reference handoff miner: new actionable
    /// cards since the cursor, then the cursor moves forward.
    pub(super) async fn mine(&self) -> Result<Value, String> {
        let cursor = self.store.get("gv:mine_cursor").await?.unwrap_or_default();
        let all = self.handoffs().await?;
        let fresh: Vec<Handoff> = all
            .into_iter()
            .filter(|h| h.at.as_str() > cursor.as_str())
            .filter(|h| h.needs_decision || h.kind == "message" || h.kind == "followup" || h.urgency != "normal")
            .collect();
        let at = now();
        self.store.set("gv:mine_cursor", &at).await?;
        Ok(json!({
            "wake": !fresh.is_empty(),
            "cursor": at,
            "handoffs": fresh,
        }))
    }

    pub(super) async fn approvals(&self) -> Result<Vec<Approval>, String> {
        self.load_many("gv:approval_ids", "gv:approval:", APPROVALS).await
    }

    pub(super) async fn pending_for(&self, call_id: &str) -> Result<Vec<Approval>, String> {
        let all = self.approvals().await?;
        Ok(all.into_iter().filter(|a| a.call_id == call_id && a.status == "pending").collect())
    }

    pub(super) async fn enqueue_approval(&self, call_id: &str, tool: &str, input: Value, summary: String) -> Result<Approval, String> {
        let a = Approval {
            id: new_id("apr"),
            call_id: call_id.into(),
            tool: tool.into(),
            input,
            summary: summary.clone(),
            status: "pending".into(),
            created_at: now(),
            decided_at: None,
            result: None,
            error: None,
        };
        self.put(&format!("gv:approval:{}", a.id), &a).await?;
        self.push_id("gv:approval_ids", &a.id, APPROVALS).await?;
        self.log("approval_queued", Some(call_id), &summary, json!({ "approval_id": a.id, "tool": tool })).await?;
        self.notify("warn", "Approval needed", &summary, "approvals", Some(call_id)).await?;
        self.owner_sms(&format!("[NXT GuardVoice] Decision needed: {summary}"), Some(call_id), "decision").await?;
        Ok(a)
    }

    pub(super) async fn summary_counts(&self) -> Result<Value, String> {
        let notes = self.notifications().await?;
        let approvals = self.approvals().await?;
        let handoffs = self.handoffs().await?;
        Ok(json!({
            "unread_notifications": notes.iter().filter(|n| !n.read).count(),
            "pending_approvals": approvals.iter().filter(|a| a.status == "pending").count(),
            "open_handoffs": handoffs.iter().filter(|h| h.status == "open").count(),
            "mode": match self.config.mode { Mode::Mock => "mock", Mode::Live => "live" },
        }))
    }

    pub(super) async fn save_job(&self, job: &OutboundJob) -> Result<(), String> {
        let fresh = self.store.get(&format!("gv:job:{}", job.id)).await?.is_none();
        self.put(&format!("gv:job:{}", job.id), job).await?;
        if fresh {
            self.push_id("gv:job_ids", &job.id, JOBS).await?;
        }
        Ok(())
    }

    pub(super) async fn job(&self, id: &str) -> Result<OutboundJob, String> {
        self.store.get_json(&format!("gv:job:{id}")).await?.ok_or_else(|| format!("outbound job {id} not found"))
    }

    async fn dispatch_note(&self, note: &Value) {
        let Some(url) = self.config.notes_webhook_url.clone() else {
            return;
        };
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return;
        }
        let body = serde_json::to_vec(note).unwrap_or_default();
        let mut req = reqwest::Client::new().post(&url).timeout(std::time::Duration::from_secs(4)).header("content-type", "application/json");
        if let Some(sec) = &self.config.notes_webhook_secret {
            if !sec.is_empty() {
                let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(sec.as_bytes()).expect("hmac");
                mac.update(&body);
                req = req.header("x-guardvoice-signature", format!("sha256={}", hex::encode(mac.finalize().into_bytes())));
            }
        }
        match req.body(body).send().await {
            Ok(r) => {
                let _ = self.log("notes_webhook", None, "Posted the call note.", json!({ "status": r.status().as_u16() })).await;
            }
            Err(e) => {
                let _ = self.log("notes_webhook", None, "Call note webhook failed.", json!({ "error": e.to_string() })).await;
            }
        }
    }

    pub(super) async fn twilio_form(&self, path: &str, fields: &[(&str, &str)]) -> Result<Value, String> {
        if self.config.mode != Mode::Live {
            return Err("mock mode does not call Twilio".into());
        }
        let url = format!("https://api.twilio.com{path}");
        let body = form_body(fields);
        let resp = reqwest::Client::new()
            .post(url)
            .basic_auth(&self.config.account_sid, Some(&self.config.auth_token))
            .timeout(std::time::Duration::from_secs(15))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("Twilio request failed: {e}"))?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(json!({}));
        if !status.is_success() {
            let msg = v["message"].as_str().unwrap_or("Twilio rejected the request");
            return Err(msg.to_string());
        }
        Ok(v)
    }
}

pub(super) struct TurnOutcome {
    pub say: Option<String>,
    pub ended: bool,
    pub approvals: Vec<String>,
    pub tools: Vec<String>,
    pub generation: u64,
}

fn form_body(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn transcript_text(call: &Call) -> String {
    call.turns
        .iter()
        .filter(|t| !t.dropped)
        .map(|t| format!("{}: {}", t.role, t.text))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) struct Hints {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub actionable: bool,
}

pub(super) fn score_transcript(text: &str) -> Hints {
    let low = text.to_lowercase();
    let keys = ["call me back", "callback", "call back", "follow up", "follow-up", "schedule", "meeting", "message", "have them call", "have him call", "have her call"];
    let actionable = keys.iter().any(|k| low.contains(k));
    let name = low.split("my name is ").nth(1).and_then(|rest| {
        let words: Vec<&str> = rest.split(|c: char| !c.is_ascii_alphabetic() && c != ' ' && c != '\'').next().unwrap_or("").split_whitespace().take(4).collect();
        let s = words.join(" ");
        let s = s.trim().to_string();
        if s.len() >= 2 { Some(title_case(&s)) } else { None }
    });
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    let phone = if digits.len() >= 10 {
        voice::e164(&digits[digits.len() - 10..])
    } else {
        None
    };
    Hints { name, phone, actionable }
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_summary(call: &Call, hints: &Hints, pending: &[Approval], reason: &str) -> String {
    let asked = call
        .turns
        .iter()
        .find(|t| t.role == "caller" && !t.dropped)
        .map(|t| t.text.chars().take(160).collect::<String>())
        .unwrap_or_else(|| "no request".into());
    let tools = if call.tools_used.is_empty() {
        "no tools".into()
    } else {
        call.tools_used.join(", ")
    };
    let who = hints.name.clone().unwrap_or_else(|| mask_number(&call.from));
    let pending_txt = if pending.is_empty() {
        "nothing waiting on approval".into()
    } else {
        format!("{} waiting on approval", pending.len())
    };
        format!(
        "{dir} call with {who}. They said: {asked}. Tools: {tools}. {pending_txt}. Ended: {reason}.",
        dir = call.direction
    )
}
