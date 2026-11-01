//! OpenAI backup model, used only during a Claude outage (see `Agent::work`).
//!
//! The agent speaks the Claude Messages format, so this adapter translates
//! requests and responses at the edge. A refusal maps to `refusal`, which ends
//! the run like a Claude refusal: the backup is never a way around one.

use super::{Model, ModelError, outage_status};
use crate::client_journey::capability::BoxFut;
use serde_json::{Value, json};
use std::time::Duration;

pub struct OpenAi {
    http: reqwest::Client,
    key: String,
    model: String,
    url: String,
}

impl OpenAi {
    /// Needs NXT_JOURNEY_BACKUP=on, OPENAI_API_KEY and NXT_BACKUP_MODEL.
    pub fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        if !var("NXT_JOURNEY_BACKUP").is_some_and(|v| v.trim().eq_ignore_ascii_case("on")) {
            return None;
        }
        let base = var("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com".into());
        Some(Self {
            http: reqwest::Client::new(),
            key: var("OPENAI_API_KEY")?,
            model: var("NXT_BACKUP_MODEL")?,
            url: format!("{}/v1/chat/completions", base.trim_end_matches('/')),
        })
    }
}

/// Claude Messages request to Chat Completions request.
pub fn to_openai(model: &str, system: &str, tools: &[Value], messages: &[Value]) -> Value {
    let mut out = vec![json!({"role": "system", "content": system})];
    for m in messages {
        let Some(blocks) = m["content"].as_array() else {
            out.push(json!({"role": m["role"], "content": m["content"]}));
            continue;
        };
        if m["role"] == "assistant" {
            let text: Vec<&str> = blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
            let calls: Vec<Value> = blocks.iter().filter(|b| b["type"] == "tool_use").map(|b| json!({
                "id": b["id"], "type": "function",
                "function": {"name": b["name"], "arguments": b["input"].to_string()},
            })).collect();
            out.push(json!({"role": "assistant", "content": text.join("\n\n"), "tool_calls": calls}));
        } else {
            for b in blocks.iter().filter(|b| b["type"] == "tool_result") {
                out.push(json!({"role": "tool", "tool_call_id": b["tool_use_id"], "content": b["content"]}));
            }
        }
    }
    let tools: Vec<Value> = tools.iter().map(|t| json!({
        "type": "function",
        "function": {"name": t["name"], "description": t["description"], "parameters": t["input_schema"]},
    })).collect();
    json!({"model": model, "max_completion_tokens": 16000, "messages": out, "tools": tools})
}

/// Chat Completions response to Claude Messages response.
pub fn from_openai(resp: &Value) -> Result<Value, ModelError> {
    let choice = &resp["choices"][0];
    let msg = &choice["message"];
    if !choice.is_object() {
        return Err(ModelError::Other("OpenAI response has no choices".into()));
    }
    let mut content = Vec::new();
    if let Some(text) = msg["content"].as_str().filter(|t| !t.is_empty()) {
        content.push(json!({"type": "text", "text": text}));
    }
    for call in msg["tool_calls"].as_array().into_iter().flatten() {
        let input: Value = serde_json::from_str(call["function"]["arguments"].as_str().unwrap_or("{}"))
            .map_err(|_| ModelError::Other("OpenAI returned invalid tool arguments".into()))?;
        content.push(json!({"type": "tool_use", "id": call["id"], "name": call["function"]["name"], "input": input}));
    }
    let refused = msg["refusal"].as_str().is_some_and(|r| !r.is_empty());
    let stop = match choice["finish_reason"].as_str().unwrap_or("") {
        _ if refused => "refusal",
        "content_filter" => "refusal",
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        _ => "end_turn",
    };
    Ok(json!({"content": content, "stop_reason": stop, "model": resp["model"]}))
}

impl Model for OpenAi {
    fn respond<'a>(&'a self, system: &'a str, tools: &'a [Value], messages: &'a [Value]) -> BoxFut<'a, Result<Value, ModelError>> {
        Box::pin(async move {
            let resp = self.http.post(&self.url)
                .timeout(Duration::from_secs(120))
                .bearer_auth(&self.key)
                .json(&to_openai(&self.model, system, tools, messages))
                .send().await
                .map_err(|e| ModelError::Outage(format!("couldn't reach the OpenAI API: {e}")))?;
            let status = resp.status();
            let v: Value = resp.json().await.map_err(|e| ModelError::Other(format!("invalid OpenAI API response: {e}")))?;
            if !status.is_success() {
                let msg = format!("OpenAI API error ({status}): {}", v["error"]["message"].as_str().unwrap_or("unknown error"));
                return Err(if outage_status(status) { ModelError::Outage(msg) } else { ModelError::Other(msg) });
            }
            from_openai(&v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_tool_turns_both_ways() {
        let tools = [json!({"name": "check_step", "description": "d", "input_schema": {"type": "object"}})];
        let messages = [
            json!({"role": "user", "content": "brief"}),
            json!({"role": "assistant", "content": [{"type": "text", "text": "ok"}, {"type": "tool_use", "id": "t1", "name": "check_step", "input": {"a": 1}}]}),
            json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ready", "is_error": false}]}),
        ];
        let req = to_openai("m", "sys", &tools, &messages);
        assert_eq!(req["messages"][0]["role"], "system");
        assert_eq!(req["messages"][2]["tool_calls"][0]["function"]["arguments"], r#"{"a":1}"#);
        assert_eq!(req["messages"][3]["role"], "tool");
        assert_eq!(req["messages"][3]["tool_call_id"], "t1");
        assert_eq!(req["tools"][0]["function"]["parameters"]["type"], "object");

        let resp = json!({"model": "m", "choices": [{"finish_reason": "tool_calls", "message": {"content": null,
            "tool_calls": [{"id": "t2", "function": {"name": "check_step", "arguments": "{\"b\":2}"}}]}}]});
        let v = from_openai(&resp).map_err(|_| "bad").unwrap();
        assert_eq!(v["stop_reason"], "tool_use");
        assert_eq!(v["content"][0]["id"], "t2");
        assert_eq!(v["content"][0]["input"]["b"], 2);
    }

    #[test]
    fn refusals_and_filters_end_the_run() {
        let v = from_openai(&json!({"choices": [{"finish_reason": "stop", "message": {"refusal": "no"}}]})).map_err(|_| "bad").unwrap();
        assert_eq!(v["stop_reason"], "refusal");
        let v = from_openai(&json!({"choices": [{"finish_reason": "content_filter", "message": {}}]})).map_err(|_| "bad").unwrap();
        assert_eq!(v["stop_reason"], "refusal");
    }
}
