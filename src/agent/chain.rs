//! Provider chain for Ask NXT and the firm agent: OpenAI first, then DeepSeek, then NVIDIA,
//! the last two through OpenRouter.
//!
//! Transferred from Stiggtechnologies/stiggWebsite3.0 `api/_lib/chatCore.ts` (PR #2 and #3,
//! merged 2026-11-12): `buildAttemptPlan` (a tier is skipped when its key is unset, model lists
//! are comma-separated env overrides) and `runWithFallback` (any error, rate limit, timeout or
//! empty reply moves to the next attempt; a per-attempt timeout inside a total budget). The
//! tier order is Orville's 2026-11-18 call: OpenAI, DeepSeek, NVIDIA via OpenRouter.
//!
//! Requests use the Chat Completions translation already in `openai.rs`. A refusal is an answer,
//! not a failure, so it is returned and never retried on another provider.

use super::openai::{from_openai, to_openai};
use super::{Model, ModelError, outage_status};
use crate::client_journey::capability::BoxFut;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// Default OpenAI model. The only OpenAI chat model id already used in Stigg code
/// (stiggWebsite3.0 `DEFAULT_GATEWAY_MODELS`). Set `NXT_OPENAI_MODELS` to change it.
pub const DEFAULT_OPENAI_MODELS: &[&str] = &["gpt-4o-mini"];
/// DeepSeek on OpenRouter. Ids checked against https://openrouter.ai/api/v1/models on
/// 2026-11-18; the first is God's Eye's default DeepSeek model, the second stiggWebsite3.0's.
pub const DEFAULT_DEEPSEEK_MODELS: &[&str] = &["deepseek/deepseek-v4.1-flash-1017", "deepseek/deepseek-v4-flash"];
/// NVIDIA Nemotron on OpenRouter, Nemotron first as in stiggWebsite3.0. Paid ids, not `:free`:
/// these prompts carry client books. Checked against the OpenRouter model list on 2026-11-18.
pub const DEFAULT_NVIDIA_MODELS: &[&str] = &["nvidia/nemotron-3.5-lightning", "nvidia/nemotron-3-super-120b-a12b"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// `openai` or `openrouter`.
    pub provider: &'static str,
    /// `openai`, `deepseek` or `nvidia`.
    pub tier: &'static str,
    pub model: String,
}

/// chatCore `parseModelList`.
pub fn parse_model_list(value: Option<&str>, fallback: &[&str]) -> Vec<String> {
    let list: Vec<String> = value.unwrap_or("").split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect();
    if list.is_empty() { fallback.iter().map(|m| m.to_string()).collect() } else { list }
}

/// chatCore `buildAttemptPlan`, with the tier order OpenAI, DeepSeek, NVIDIA.
pub fn build_attempt_plan(env: &dyn Fn(&str) -> Option<String>) -> Vec<Attempt> {
    let var = |k: &str| env(k).filter(|v| !v.trim().is_empty());
    let mut plan = vec![];
    if var("OPENAI_API_KEY").is_some() {
        for model in parse_model_list(var("NXT_OPENAI_MODELS").as_deref(), DEFAULT_OPENAI_MODELS) {
            plan.push(Attempt { provider: "openai", tier: "openai", model });
        }
    }
    if var("OPENROUTER_API_KEY").is_some() {
        for model in parse_model_list(var("OPENROUTER_DEEPSEEK_MODELS").as_deref(), DEFAULT_DEEPSEEK_MODELS) {
            plan.push(Attempt { provider: "openrouter", tier: "deepseek", model });
        }
        for model in parse_model_list(var("OPENROUTER_NVIDIA_MODELS").as_deref(), DEFAULT_NVIDIA_MODELS) {
            plan.push(Attempt { provider: "openrouter", tier: "nvidia", model });
        }
    }
    plan
}

fn process_env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// The model Ask NXT reports in its status card: the first attempt, if any.
pub fn primary_model() -> Option<String> {
    build_attempt_plan(&process_env).into_iter().next().map(|a| a.model)
}

/// chatCore `runWithFallback`. Returns the first answer and the failures before it.
pub async fn run_with_fallback<'a, F>(plan: &'a [Attempt], per_attempt: Duration, total: Duration, run: F) -> Result<(Value, &'a Attempt, Vec<String>), ModelError>
where
    F: Fn(&'a Attempt) -> BoxFut<'a, Result<Value, ModelError>>,
{
    let started = Instant::now();
    let mut failures = vec![];
    let mut any_outage = false;
    for attempt in plan {
        let Some(remaining) = total.checked_sub(started.elapsed()).filter(|d| !d.is_zero()) else { break };
        let limit = per_attempt.min(remaining);
        let label = format!("{} {}", attempt.provider, attempt.model);
        match tokio::time::timeout(limit, run(attempt)).await {
            Ok(Ok(v)) if has_answer(&v) => return Ok((v, attempt, failures)),
            Ok(Ok(_)) => failures.push(format!("{label}: empty response")),
            Ok(Err(e)) => {
                any_outage |= matches!(e, ModelError::Outage(_));
                failures.push(format!("{label}: {}", e.message()));
            }
            Err(_) => {
                any_outage = true;
                failures.push(format!("{label}: timed out after {}s", limit.as_secs()));
            }
        }
    }
    let msg = if failures.is_empty() { "No AI provider is configured (set OPENAI_API_KEY or OPENROUTER_API_KEY)".to_string() } else { format!("All AI providers failed: {}", failures.join("; ")) };
    Err(if any_outage || failures.is_empty() { ModelError::Outage(msg) } else { ModelError::Other(msg) })
}

/// A reply with text, a tool call, or a refusal counts; an empty one moves on (chatCore "Empty response").
fn has_answer(v: &Value) -> bool {
    v["stop_reason"] == "refusal" || v["content"].as_array().is_some_and(|c| !c.is_empty())
}

/// OpenRouter routing preferences. `data_collection: deny` keeps client books away from
/// providers that store or train on prompts; set OPENROUTER_DATA_COLLECTION=allow to change it.
pub fn openrouter_body(mut body: Value, data_collection: &str) -> Value {
    if let Some(o) = body.as_object_mut() {
        if let Some(n) = o.remove("max_completion_tokens") { o.insert("max_tokens".into(), n); }
        o.insert("provider".into(), json!({"data_collection": if data_collection == "allow" { "allow" } else { "deny" }}));
    }
    body
}

pub struct Chain {
    http: reqwest::Client,
    plan: Vec<Attempt>,
    openai_key: Option<String>,
    openai_url: String,
    openrouter_key: Option<String>,
    openrouter_url: String,
    data_collection: String,
    per_attempt: Duration,
    total: Duration,
}

impl Chain {
    /// None when neither OPENAI_API_KEY nor OPENROUTER_API_KEY is set.
    pub fn from_env() -> Option<Self> {
        Self::from(&process_env)
    }

    pub fn from(env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let var = |k: &str| env(k).filter(|v| !v.trim().is_empty());
        let plan = build_attempt_plan(env);
        if plan.is_empty() {
            return None;
        }
        // Same rustls provider install as store.rs, ai_gateway.rs and stripe.rs.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let secs = |k: &str, d: u64| var(k).and_then(|v| v.trim().parse().ok()).filter(|n: &u64| *n > 0).unwrap_or(d);
        Some(Self {
            http: reqwest::Client::new(),
            plan,
            openai_key: var("OPENAI_API_KEY"),
            openai_url: format!("{}/v1/chat/completions", var("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com".into()).trim_end_matches('/')),
            openrouter_key: var("OPENROUTER_API_KEY"),
            openrouter_url: format!("{}/chat/completions", var("OPENROUTER_BASE_URL").unwrap_or_else(|| "https://openrouter.ai/api/v1".into()).trim_end_matches('/')),
            data_collection: var("OPENROUTER_DATA_COLLECTION").unwrap_or_else(|| "deny".into()).trim().to_lowercase(),
            per_attempt: Duration::from_secs(secs("NXT_AI_ATTEMPT_TIMEOUT_SECS", 45)),
            total: Duration::from_secs(secs("NXT_AI_TOTAL_BUDGET_SECS", 100)),
        })
    }

    pub fn plan(&self) -> &[Attempt] {
        &self.plan
    }

    async fn send(&self, attempt: &Attempt, system: &str, tools: &[Value], messages: &[Value]) -> Result<Value, ModelError> {
        let body = to_openai(&attempt.model, system, tools, messages);
        let (url, key, body) = if attempt.provider == "openai" {
            (&self.openai_url, self.openai_key.as_deref(), body)
        } else {
            (&self.openrouter_url, self.openrouter_key.as_deref(), openrouter_body(body, &self.data_collection))
        };
        let key = key.ok_or_else(|| ModelError::Other(format!("{} key is not set", attempt.provider)))?;
        let resp = self.http.post(url).bearer_auth(key).header("X-Title", "NXT OS").json(&body).send().await
            .map_err(|e| ModelError::Outage(format!("couldn't reach {}: {e}", attempt.provider)))?;
        let status = resp.status();
        let v: Value = resp.json().await.map_err(|e| ModelError::Other(format!("invalid {} response: {e}", attempt.provider)))?;
        if !status.is_success() {
            let msg = format!("{} error ({status}): {}", attempt.provider, v["error"]["message"].as_str().unwrap_or("unknown error"));
            return Err(if outage_status(status) { ModelError::Outage(msg) } else { ModelError::Other(msg) });
        }
        let mut out = from_openai(&v)?;
        out["provider"] = json!(attempt.provider);
        out["tier"] = json!(attempt.tier);
        if out["model"].is_null() { out["model"] = json!(attempt.model); }
        Ok(out)
    }
}

impl Model for Chain {
    fn respond<'a>(&'a self, system: &'a str, tools: &'a [Value], messages: &'a [Value]) -> BoxFut<'a, Result<Value, ModelError>> {
        Box::pin(async move {
            let (mut v, _, failures) = run_with_fallback(&self.plan, self.per_attempt, self.total, |a| Box::pin(self.send(a, system, tools, messages))).await?;
            if !failures.is_empty() { v["fallback_failures"] = json!(failures); }
            Ok(v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }
    fn tiers(plan: &[Attempt]) -> Vec<(&str, &str)> {
        plan.iter().map(|a| (a.provider, a.tier)).collect()
    }

    #[test]
    fn plan_is_openai_then_deepseek_then_nvidia() {
        let plan = build_attempt_plan(&env(&[("OPENAI_API_KEY", "x"), ("OPENROUTER_API_KEY", "y")]));
        assert_eq!(tiers(&plan), [("openai", "openai"), ("openrouter", "deepseek"), ("openrouter", "deepseek"), ("openrouter", "nvidia"), ("openrouter", "nvidia")]);
        assert_eq!(plan[0].model, "gpt-4o-mini");
        assert_eq!(plan[1].model, "deepseek/deepseek-v4.1-flash-1017");
        assert_eq!(plan[3].model, "nvidia/nemotron-3.5-lightning");
        assert!(plan.iter().all(|a| !a.model.ends_with(":free")), "no free tier for client data");
    }

    #[test]
    fn tiers_without_a_key_are_skipped_and_lists_can_be_overridden() {
        assert!(build_attempt_plan(&env(&[])).is_empty());
        assert!(build_attempt_plan(&env(&[("OPENAI_API_KEY", "  ")])).is_empty());
        assert!(build_attempt_plan(&env(&[("ANTHROPIC_API_KEY", "x")])).is_empty(), "Anthropic is not a tier");
        let only_router = build_attempt_plan(&env(&[("OPENROUTER_API_KEY", "y"), ("OPENROUTER_NVIDIA_MODELS", " , ")]));
        assert_eq!(only_router.first().map(|a| a.tier), Some("deepseek"));
        assert_eq!(only_router.iter().filter(|a| a.tier == "nvidia").count(), 2, "blank override keeps the defaults");
        let custom = build_attempt_plan(&env(&[("OPENAI_API_KEY", "x"), ("NXT_OPENAI_MODELS", "gpt-a, gpt-b")]));
        assert_eq!(custom.iter().map(|a| a.model.as_str()).collect::<Vec<_>>(), ["gpt-a", "gpt-b"]);
        assert!(Chain::from(&env(&[])).is_none());
        assert_eq!(Chain::from(&env(&[("OPENROUTER_API_KEY", "y")])).unwrap().plan().len(), 4);
    }

    #[test]
    fn openrouter_requests_deny_data_collection_by_default() {
        let body = openrouter_body(json!({"model": "m", "max_completion_tokens": 10}), "deny");
        assert_eq!(body["provider"]["data_collection"], "deny");
        assert_eq!(body["max_tokens"], 10);
        assert!(body.get("max_completion_tokens").is_none());
        assert_eq!(openrouter_body(json!({}), "anything")["provider"]["data_collection"], "deny");
        assert_eq!(openrouter_body(json!({}), "allow")["provider"]["data_collection"], "allow");
    }

    fn plan3() -> Vec<Attempt> {
        build_attempt_plan(&env(&[("OPENAI_API_KEY", "x"), ("OPENROUTER_API_KEY", "y"), ("OPENROUTER_DEEPSEEK_MODELS", "ds"), ("OPENROUTER_NVIDIA_MODELS", "nv")]))
    }
    fn text(t: &str) -> Value { json!({"content": [{"type": "text", "text": t}], "stop_reason": "end_turn"}) }

    #[tokio::test]
    async fn falls_through_errors_rate_limits_empty_replies_and_timeouts() {
        let plan = plan3();
        let calls = AtomicUsize::new(0);
        let (v, used, failures) = run_with_fallback(&plan, Duration::from_millis(200), Duration::from_secs(5), |a| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                match a.tier {
                    "openai" => Err(ModelError::Outage("openai error (429 Too Many Requests): rate limited".into())),
                    "deepseek" => Ok(json!({"content": [], "stop_reason": "end_turn"})),
                    _ => Ok(text("from nvidia")),
                }
            })
        }).await.map_err(|_| "chain failed").unwrap();
        assert_eq!(used.tier, "nvidia");
        assert_eq!(v["content"][0]["text"], "from nvidia");
        assert_eq!(failures.len(), 2);
        assert!(failures[0].contains("429") && failures[1].contains("empty"));
        assert_eq!(calls.load(Ordering::SeqCst), 3);

        let (_, used, failures) = run_with_fallback(&plan, Duration::from_millis(50), Duration::from_secs(5), |a| Box::pin(async move {
            if a.tier == "openai" { tokio::time::sleep(Duration::from_secs(2)).await; }
            Ok(text("ok"))
        })).await.map_err(|_| "chain failed").unwrap();
        assert_eq!(used.tier, "deepseek");
        assert!(failures[0].contains("timed out"));
    }

    #[tokio::test]
    async fn openai_answers_first_and_a_refusal_is_never_retried() {
        let plan = plan3();
        let calls = AtomicUsize::new(0);
        let (_, used, failures) = run_with_fallback(&plan, Duration::from_secs(1), Duration::from_secs(5), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(json!({"content": [], "stop_reason": "refusal"})) })
        }).await.map_err(|_| "chain failed").unwrap();
        assert_eq!(used.tier, "openai");
        assert!(failures.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a refusal does not go to DeepSeek or NVIDIA");
    }

    #[tokio::test]
    async fn all_failures_and_no_providers_fail_cleanly() {
        let plan = plan3();
        let err = run_with_fallback(&plan, Duration::from_secs(1), Duration::from_secs(5), |_| Box::pin(async { Err(ModelError::Other("bad request".into())) })).await.err().unwrap();
        let ModelError::Other(msg) = err else { panic!("a 4xx everywhere is not an outage") };
        assert!(msg.starts_with("All AI providers failed") && msg.matches("bad request").count() == 3, "{msg}");
        let err = run_with_fallback(&[], Duration::from_secs(1), Duration::from_secs(5), |_| Box::pin(async { Ok(text("x")) })).await.err().unwrap();
        let ModelError::Outage(msg) = err else { panic!("no provider") };
        assert!(msg.contains("OPENAI_API_KEY") && msg.contains("OPENROUTER_API_KEY"));
        let err = run_with_fallback(&plan, Duration::from_secs(1), Duration::ZERO, |_| Box::pin(async { Ok(text("x")) })).await.err().unwrap();
        assert!(matches!(err, ModelError::Outage(_)), "an exhausted budget is reported, not answered");
    }
}
