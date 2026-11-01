//! Firm agent: works a client's current journey step with Claude.
//!
//! The agent sees the step (action, outcome, gate), the client's source
//! software and the latest blockers, and may call only the actions the step's
//! capability exposes, plus `check_step` and `escalate_to_human`. Every call is
//! logged on the journey as evidence. The agent never moves the journey: after
//! it stops, the journey re-assesses the step and the autonomy policy decides
//! who closes it (evidence, Nixon review, or a person).

use crate::client_journey::capability::{BoxFut, Capability};
use crate::client_journey::service::{AgentAction, Journeys, WorkItem};
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const MAX_ROUNDS: usize = 6;
const MAX_RESULT_CHARS: usize = 20_000;

const SYSTEM_PROMPT: &str = "You are the NXT firm agent. You do the work of one step in an accounting \
firm's client journey, using only the tools given. Start by reading the blockers, then take the \
actions that clear them and call check_step to confirm. Never guess a client's identity, invent \
records or work around a blocker: if a blocker needs a person (missing staff confirmation, a system \
NXT cannot import yet, anything ambiguous), call escalate_to_human with a one-sentence reason and \
stop. You cannot approve, send, charge or file anything. Finish with a short plain summary of what \
you did and what is still blocked.";

/// A model that answers one Messages API request.
pub trait Model: Send + Sync {
    fn respond<'a>(&'a self, system: &'a str, tools: &'a [Value], messages: &'a [Value]) -> BoxFut<'a, Result<Value, String>>;
}

/// The Claude Messages API, configured like Ask NXT.
pub struct Claude {
    http: reqwest::Client,
    key: String,
}

impl Claude {
    pub fn from_env() -> Option<Self> {
        crate::os::api_key().map(|key| Self { http: reqwest::Client::new(), key })
    }
}

impl Model for Claude {
    fn respond<'a>(&'a self, system: &'a str, tools: &'a [Value], messages: &'a [Value]) -> BoxFut<'a, Result<Value, String>> {
        Box::pin(async move {
            let body = json!({
                "model": crate::os::model(),
                "max_tokens": 16000,
                "output_config": {"effort": "medium"},
                "fallbacks": "default",
                "system": system,
                "tools": tools,
                "messages": messages,
            });
            let resp = self.http.post(crate::os::api_url())
                .timeout(Duration::from_secs(120))
                .header("x-api-key", &self.key)
                .header("anthropic-version", "2026-10-21")
                .header("anthropic-beta", "server-side-fallback-2026-07-01")
                .json(&body)
                .send().await
                .map_err(|e| format!("couldn't reach the Claude API: {e}"))?;
            let status = resp.status();
            let v: Value = resp.json().await.map_err(|e| format!("invalid Claude API response: {e}"))?;
            if !status.is_success() {
                return Err(format!("Claude API error ({status}): {}", v["error"]["message"].as_str().unwrap_or("unknown error")));
            }
            Ok(v)
        })
    }
}

pub struct Agent {
    journeys: Arc<Journeys>,
    model: Arc<dyn Model>,
}

fn tools(cap: &dyn Capability, step: &str) -> Vec<Value> {
    let mut tools: Vec<Value> = cap.actions(step).into_iter()
        .map(|a| json!({"name": a.name, "description": a.description, "input_schema": a.input_schema}))
        .collect();
    tools.push(json!({"name": "check_step", "description": "Assess the step again and return whether it is ready, with its blockers and evidence.",
        "input_schema": {"type": "object", "properties": {}, "additionalProperties": false}}));
    tools.push(json!({"name": "escalate_to_human", "description": "Hand this step to a person, with a one-sentence reason. Stops your work on the step.",
        "input_schema": {"type": "object", "properties": {"reason": {"type": "string"}}, "required": ["reason"], "additionalProperties": false}}));
    tools
}

fn brief(item: &WorkItem) -> String {
    json!({
        "client_id": item.ctx.client_id,
        "step": {"key": item.step.key, "title": item.step.title, "action": item.step.action,
                 "outcome": item.step.outcome, "gate_check": item.step.gate_check},
        "sources": item.ctx.sources,
        "last_assessment": item.last_assessment.as_ref().map(|a| &a.readiness),
    }).to_string()
}

fn clip(v: &Value) -> String {
    let mut t = v.to_string();
    if t.len() > MAX_RESULT_CHARS {
        let mut cut = MAX_RESULT_CHARS;
        while !t.is_char_boundary(cut) { cut -= 1; }
        t.truncate(cut);
        t.push_str("…(truncated)");
    }
    t
}

impl Agent {
    pub fn new(journeys: Arc<Journeys>, model: Arc<dyn Model>) -> Self {
        Self { journeys, model }
    }

    /// Advances the journey, works the current step if a capability can, then advances again.
    pub async fn work(&self, client: &str) -> Result<Value, ApiError> {
        self.journeys.advance(client).await?;
        let Some(item) = self.journeys.work_item(client).await? else {
            let j = self.journeys.get(client).await?;
            return Ok(json!({"worked": false, "reason": "nothing for the agent: the step is manual, waiting on a person, or the journey is complete", "journey": j}));
        };
        let cap = item.capability.clone();
        let tools = tools(cap.as_ref(), &item.step.key);
        let mut messages = vec![json!({"role": "user", "content": brief(&item)})];
        let mut log: Vec<AgentAction> = Vec::new();
        let mut summary = String::new();
        let mut escalated = None;
        'rounds: for _ in 0..MAX_ROUNDS {
            let resp = self.model.respond(SYSTEM_PROMPT, &tools, &messages).await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
            let content = resp["content"].as_array().cloned().unwrap_or_default();
            let stop = resp["stop_reason"].as_str().unwrap_or("");
            summary = content.iter().filter(|c| c["type"] == "text").filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n\n");
            if stop != "tool_use" {
                break;
            }
            messages.push(json!({"role": "assistant", "content": content}));
            let mut results = Vec::new();
            for block in content.iter().filter(|c| c["type"] == "tool_use") {
                let name = block["name"].as_str().unwrap_or("").to_string();
                let input = block.get("input").cloned().unwrap_or(json!({}));
                let outcome = match name.as_str() {
                    "check_step" => Ok(serde_json::to_value(cap.assess(&item.ctx).await).unwrap_or(Value::Null)),
                    "escalate_to_human" => {
                        let reason = input["reason"].as_str().unwrap_or("").trim().to_string();
                        if reason.is_empty() { Err("a reason is required".to_string()) } else { escalated = Some(reason); Ok(json!({"escalated": true})) }
                    }
                    _ if cap.actions(&item.step.key).iter().any(|a| a.name == name) => cap.act(&item.ctx, &name, &input).await,
                    _ => Err(format!("unknown tool {name}")),
                };
                let (ok, result) = match outcome { Ok(v) => (true, v), Err(e) => (false, json!(e)) };
                results.push(json!({"type": "tool_result", "tool_use_id": block["id"], "content": clip(&result), "is_error": !ok}));
                log.push(AgentAction { at: chrono::Utc::now().to_rfc3339(), step: item.step.key.clone(), tool: name, input, ok, result });
            }
            if escalated.is_some() {
                break 'rounds;
            }
            messages.push(json!({"role": "user", "content": results}));
        }
        self.journeys.log_actions(client, &log).await?;
        if let Some(reason) = &escalated {
            self.journeys.escalate(client, &item.step.key, reason).await?;
        }
        let journey = self.journeys.advance(client).await?;
        Ok(json!({"worked": true, "step": item.step.key, "actions": log, "escalated": escalated, "summary": summary, "journey": journey}))
    }
}
