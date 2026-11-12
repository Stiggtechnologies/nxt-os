//! The autonomy planner's model client. It reuses God's Eye's
//! [`ai_gateway`](crate::nixon_finance::ai_gateway) routing, guard and request body, and
//! adds tool calling. It does not add a second gateway client or provider list.
//!
//! - Routes come from `ai_gateway::plan(Sensitivity::ClientData, …)`: DeepSeek on the
//!   zero-retention hosts God's Eye pins (never DeepSeek first-party), then the
//!   zero-retention Claude Sonnet 5.5 fallback. The NVIDIA free tier is never in a
//!   client-data plan, and `ai_gateway::guard` is re-checked before every send.
//! - Off by default. [`GatewayConfig::from_vars`] returns `None` unless
//!   `NXT_AUTONOMY_AI=on`, `NXT_AI_GATEWAY_PAID=confirmed` (a paid Vercel plan or gateway
//!   credits, which zero data retention needs) and a gateway credential are all set.

use super::guard::{Sensitivity, Tier, effective_tier};
use crate::agent::{Model, ModelError, outage_status};
use crate::client_journey::capability::BoxFut;
use crate::nixon_finance::ai_gateway::{self, Provider, Route};
use serde_json::{Value, json};
use std::time::Duration;

/// DeepSeek V4.1 Flash list price from the 2026-11-12 gateway plan ($0.30 in / $1.20 out
/// per 1M tokens), in tenths of a micro-USD per token. Used only for the cost budget.
pub const PRICE_IN_TENTHS: u64 = 3;
pub const PRICE_OUT_TENTHS: u64 = 12;

/// Routes for one planner request. Autonomy prompts always carry firm data, so the
/// sensitivity is at least client data whatever the text looks like.
pub fn routes_for(text: &str, env: &(dyn Fn(&str) -> Option<String> + Sync)) -> Vec<Route> {
    let tier = effective_tier(Tier::T2, text);
    let routes = ai_gateway::plan(tier.sensitivity(), env);
    routes.into_iter().filter(|r| ai_gateway::guard(r, Sensitivity::ClientData).is_ok()).collect()
}

/// God's Eye's OpenAI-style body (ZDR, no prompt training, host allow-list) plus tools.
pub fn tool_body(route: &Route, system: &str, user: &str, tools: &[Value]) -> Value {
    let mut b = ai_gateway::body(route, system, user);
    b["tools"] = json!(tools.iter().map(|t| json!({"type": "function", "function": {
        "name": t["name"], "description": t["description"], "parameters": t["input_schema"],
    }})).collect::<Vec<_>>());
    b
}

/// Converts an OpenAI-style reply into the Messages shape `ModelPlanner` reads.
pub fn to_messages_shape(v: &Value) -> Value {
    let msg = &v["choices"][0]["message"];
    let mut content = vec![];
    if let Some(call) = msg["tool_calls"].as_array().and_then(|c| c.first()) {
        let input: Value = call["function"]["arguments"].as_str().and_then(|a| serde_json::from_str(a).ok()).unwrap_or(json!({}));
        content.push(json!({"type": "tool_use", "id": call["id"], "name": call["function"]["name"], "input": input}));
    } else if let Some(t) = msg["content"].as_str() {
        content.push(json!({"type": "text", "text": t}));
    }
    json!({"content": content, "usage": {"input_tokens": v["usage"]["prompt_tokens"], "output_tokens": v["usage"]["completion_tokens"]}})
}

#[derive(Clone)]
pub struct GatewayConfig {
    credential: String,
}

impl std::fmt::Debug for GatewayConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayConfig").field("credential", &"[redacted]").finish()
    }
}

impl GatewayConfig {
    pub fn from_env() -> Option<Self> {
        Self::from_vars(ai_gateway::process_env)
    }

    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let on = |k: &str, v: &str| get(k).is_some_and(|x| x.trim().eq_ignore_ascii_case(v));
        if !on("NXT_AUTONOMY_AI", "on") || !on("NXT_AI_GATEWAY_PAID", "confirmed") {
            return None;
        }
        let credential = get("AI_GATEWAY_API_KEY").or_else(|| get("VERCEL_OIDC_TOKEN")).filter(|k| !k.trim().is_empty())?;
        Some(Self { credential })
    }
}

/// The gateway as an [`agent::Model`](crate::agent::Model), on zero-retention routes only.
pub struct GatewayModel {
    http: reqwest::Client,
    config: GatewayConfig,
}

impl GatewayModel {
    pub fn new(config: GatewayConfig) -> Self {
        Self { http: reqwest::Client::new(), config }
    }
}

impl Model for GatewayModel {
    fn respond<'a>(&'a self, system: &'a str, tools: &'a [Value], messages: &'a [Value]) -> BoxFut<'a, Result<Value, ModelError>> {
        Box::pin(async move {
            let user = super::guard::redact(&serde_json::to_string(messages).unwrap_or_default());
            let routes = routes_for(&user, &ai_gateway::process_env);
            let _ = rustls::crypto::ring::default_provider().install_default();
            let mut last = ModelError::Other("no allowed zero-retention route".into());
            for route in routes {
                ai_gateway::guard(&route, Sensitivity::ClientData).map_err(ModelError::Other)?;
                if route.provider != Provider::Gateway {
                    continue;
                }
                let resp = self
                    .http
                    .post(ai_gateway::GATEWAY_URL)
                    .bearer_auth(&self.config.credential)
                    .timeout(Duration::from_secs(60))
                    .json(&tool_body(&route, system, &user, tools))
                    .send()
                    .await;
                let resp = match resp {
                    Ok(r) => r,
                    Err(_) => {
                        last = ModelError::Outage(format!("gateway {} unreachable", route.model));
                        continue;
                    }
                };
                let status = resp.status();
                let v: Value = resp.json().await.unwrap_or(Value::Null);
                if status.is_success() {
                    return Ok(to_messages_shape(&v));
                }
                // Never echo the body: it can quote the request.
                last = if outage_status(status) { ModelError::Outage(format!("gateway {status}")) } else { ModelError::Other(format!("gateway {status}")) };
            }
            Err(last)
        })
    }
}
