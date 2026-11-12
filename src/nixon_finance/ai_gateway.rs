//! The one AI client God's Eye may use, with a sensitivity guard.
//!
//! AI drafting is **off** unless `GODS_EYE_AI=on`. Every watcher is
//! deterministic and works with no model and no key.
//!
//! Routing, in order:
//! 1. **Primary:** DeepSeek through the Vercel AI Gateway, pinned to
//!    zero-data-retention hosts serving DeepSeek (Fireworks, Together, Baseten),
//!    never DeepSeek first-party. Every gateway request sets
//!    `providerOptions.gateway.zeroDataRetention` and `disallowPromptTraining`.
//! 2. **Sensitive backup:** a zero-retention gateway model (Claude Sonnet 5.5 on
//!    Anthropic or Bedrock). Otherwise it fails cleanly.
//! 3. **Non-sensitive backup only:** NVIDIA NIM free models (same list as
//!    stiggWebsite3.0 PR #3). Client or company data never reaches this tier,
//!    and there is no OpenRouter tier here.
//!
//! The guard is checked per attempt, not only when the plan is built, so an
//! env override cannot route client data to a free or retaining provider.

use serde_json::{Value, json};
use std::time::Duration;

pub const GATEWAY_URL: &str = "https://ai-gateway.vercel.sh/v1/chat/completions";
pub const NVIDIA_URL: &str = "https://integrate.api.nvidia.com/v1/chat/completions";

pub const DEFAULT_MODELS: &[&str] = &["deepseek/deepseek-v4.1-flash-1017"];
pub const DEFAULT_ZDR_PROVIDERS: &[&str] = &["fireworks", "togetherai", "baseten"];
pub const DEFAULT_SENSITIVE_FALLBACK_MODELS: &[&str] = &["anthropic/claude-sonnet-5.5"];
pub const DEFAULT_SENSITIVE_FALLBACK_PROVIDERS: &[&str] = &["anthropic", "bedrock"];
pub const DEFAULT_NVIDIA_MODELS: &[&str] = &[
    "nvidia/nemotron-3.5-lightning-30b-a3b",
    "nvidia/nemotron-3-super-120b-a12b",
    "openai/gpt-oss-20b",
];

/// Hosts that Vercel's AI Gateway docs list with a zero-data-retention agreement
/// (`/docs/ai-gateway/security-and-compliance/zdr`, read 2026-11-12), less the
/// first-party labs excluded below. Anything else is refused for client data.
const ZDR_HOSTS: &[&str] = &[
    "anthropic", "azure", "baseten", "bedrock", "claudeaws", "digitalocean", "fireworks",
    "vertex", "vertexAnthropic", "groq", "mistral", "modal", "nebius", "openai", "parasail",
    "togetherai",
];
/// Never used for Canadian client or company data, even when ZDR-listed.
const EXCLUDED_HOSTS: &[&str] = &["deepseek", "alibaba", "moonshotai", "zai", "deepinfra", "novita", "cerebras"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    /// Static help text or watcher descriptions: no client, company or tax data.
    NonSensitive,
    /// Anything built from books, tax returns, names or amounts.
    ClientData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Gateway,
    NvidiaFree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub provider: Provider,
    pub model: String,
    /// Gateway `only` provider list. Empty for NVIDIA.
    pub only: Vec<String>,
    pub zero_data_retention: bool,
}

type Env<'a> = &'a (dyn Fn(&str) -> Option<String> + Sync);

pub fn process_env(k: &str) -> Option<String> {
    std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// AI drafting is off unless `GODS_EYE_AI` is exactly `on`.
pub fn enabled(env: Env) -> bool {
    env("GODS_EYE_AI").as_deref() == Some("on")
}

fn list(env: Env, key: &str, default: &[&str]) -> Vec<String> {
    match env(key) {
        Some(v) => v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        None => default.iter().map(|s| s.to_string()).collect(),
    }
}

fn is_free_model(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.ends_with(":free") || m.ends_with("-free") || m.starts_with("openrouter/")
}

/// Refuses any attempt that could send client data to a free or retaining provider.
pub fn guard(route: &Route, sensitivity: Sensitivity) -> Result<(), String> {
    if sensitivity == Sensitivity::NonSensitive {
        return Ok(());
    }
    if route.provider != Provider::Gateway {
        return Err(format!("client data cannot go to the free tier ({})", route.model));
    }
    if !route.zero_data_retention {
        return Err("client data needs zero data retention".into());
    }
    if is_free_model(&route.model) {
        return Err(format!("client data cannot use a free model ({})", route.model));
    }
    if route.only.is_empty() {
        return Err("client data needs an explicit zero-retention host list".into());
    }
    for host in &route.only {
        if EXCLUDED_HOSTS.contains(&host.as_str()) {
            return Err(format!("host {host} is excluded for client data"));
        }
        if !ZDR_HOSTS.contains(&host.as_str()) {
            return Err(format!("host {host} is not on the zero-retention list"));
        }
    }
    Ok(())
}

/// The ordered attempts for one request. Routes the guard refuses are dropped here
/// and checked again before each send.
pub fn plan(sensitivity: Sensitivity, env: Env) -> Vec<Route> {
    let gw = |models: Vec<String>, hosts: Vec<String>| -> Vec<Route> {
        models.into_iter().map(|model| Route { provider: Provider::Gateway, model, only: hosts.clone(), zero_data_retention: true }).collect()
    };
    let mut out = gw(list(env, "GODS_EYE_MODELS", DEFAULT_MODELS), list(env, "GODS_EYE_ZDR_PROVIDERS", DEFAULT_ZDR_PROVIDERS));
    match sensitivity {
        Sensitivity::ClientData => out.extend(gw(
            list(env, "GODS_EYE_SENSITIVE_FALLBACK_MODELS", DEFAULT_SENSITIVE_FALLBACK_MODELS),
            list(env, "GODS_EYE_SENSITIVE_FALLBACK_PROVIDERS", DEFAULT_SENSITIVE_FALLBACK_PROVIDERS),
        )),
        Sensitivity::NonSensitive => {
            if env("NVIDIA_API_KEY").is_some() {
                out.extend(list(env, "NVIDIA_MODELS", DEFAULT_NVIDIA_MODELS).into_iter().map(|model| Route {
                    provider: Provider::NvidiaFree, model, only: vec![], zero_data_retention: false,
                }));
            }
        }
    }
    out.retain(|r| guard(r, sensitivity).is_ok());
    out
}

/// OpenAI-compatible chat body. Gateway requests always carry the ZDR options.
pub fn body(route: &Route, system: &str, user: &str) -> Value {
    let mut b = json!({
        "model": route.model,
        "max_tokens": 700,
        "temperature": 0.2,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
    });
    if route.provider == Provider::Gateway {
        b["providerOptions"] = json!({"gateway": {
            "zeroDataRetention": true,
            "disallowPromptTraining": true,
            "only": route.only,
        }});
    }
    b
}

/// Masks every run of seven or more digits (SINs, bank and card numbers) to its last
/// three digits. Amounts in cents shorter than that are kept.
pub fn redact(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let date_at = |i: usize| -> bool {
        i + 10 <= n
            && (0..10).all(|k| if k == 4 || k == 7 { c[i + k] == '-' } else { c[i + k].is_ascii_digit() })
            && (i + 10 == n || !c[i + 10].is_ascii_digit())
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < n {
        if !c[i].is_ascii_digit() {
            out.push(c[i]);
            i += 1;
            continue;
        }
        if date_at(i) {
            out.extend(&c[i..i + 10]);
            i += 10;
            continue;
        }
        // One number, allowing single spaces or dashes between digit groups (046 454 286).
        let mut j = i;
        let mut digits = String::new();
        while j < n {
            if c[j].is_ascii_digit() {
                digits.push(c[j]);
                j += 1;
            } else if (c[j] == ' ' || c[j] == '-') && j + 1 < n && c[j + 1].is_ascii_digit() && !date_at(j + 1) && digits.len() < 19 {
                j += 1;
            } else {
                break;
            }
        }
        if digits.len() >= 7 {
            out.push_str("•••");
            out.push_str(&digits[digits.len() - 3..]);
        } else {
            out.extend(&c[i..j]);
        }
        i = j;
    }
    out
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub text: String,
    pub provider: &'static str,
    pub model: String,
    pub zero_data_retention: bool,
}

/// Sends the prompt along the plan. A clean error when AI is off, nothing is
/// configured, or every allowed route fails. The prompt is redacted first.
pub async fn draft(sensitivity: Sensitivity, system: &str, user: &str, env: Env<'_>) -> Result<Draft, String> {
    if !enabled(env) {
        return Err("ai_drafting_disabled".into());
    }
    let user = redact(user);
    let gateway_auth = env("AI_GATEWAY_API_KEY").or_else(|| env("VERCEL_OIDC_TOKEN"));
    let nvidia_auth = env("NVIDIA_API_KEY");
    let routes = plan(sensitivity, env);
    if routes.is_empty() {
        return Err("no allowed model route".into());
    }
    let any_auth = routes.iter().any(|r| match r.provider {
        Provider::Gateway => gateway_auth.is_some(),
        Provider::NvidiaFree => nvidia_auth.is_some(),
    });
    if !any_auth {
        return Err("ai gateway is not configured".into());
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::new();
    let mut last = String::from("no route was configured");
    for route in routes {
        guard(&route, sensitivity)?;
        let (url, key, label) = match route.provider {
            Provider::Gateway => (GATEWAY_URL, gateway_auth.as_deref(), "vercel-ai-gateway"),
            Provider::NvidiaFree => (NVIDIA_URL, nvidia_auth.as_deref(), "nvidia-nim"),
        };
        let Some(key) = key else {
            last = format!("{label} is not configured");
            continue;
        };
        let resp = http
            .post(url)
            .bearer_auth(key)
            .timeout(Duration::from_secs(25))
            .json(&body(&route, system, &user))
            .send()
            .await;
        let resp = match resp {
            Ok(r) => r,
            Err(_) => {
                last = format!("{label} {} unreachable or timed out", route.model);
                continue;
            }
        };
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            last = format!("{label} {} answered {}", route.model, status.as_u16());
            continue;
        }
        let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
        if text.is_empty() {
            last = format!("{label} {} returned no text", route.model);
            continue;
        }
        return Ok(Draft { text: redact(&text), provider: label, model: route.model, zero_data_retention: route.zero_data_retention });
    }
    Err(last)
}

/// Routing as reported by the status route. Names only, never values.
pub fn describe(env: Env) -> Value {
    let show = |s: Sensitivity| -> Vec<Value> {
        plan(s, env).iter().map(|r| json!({
            "provider": match r.provider { Provider::Gateway => "vercel-ai-gateway", Provider::NvidiaFree => "nvidia-nim" },
            "model": r.model, "only": r.only, "zero_data_retention": r.zero_data_retention,
        })).collect()
    };
    json!({
        "enabled": enabled(env),
        "flag": "GODS_EYE_AI=on",
        "gateway_auth_configured": env("AI_GATEWAY_API_KEY").is_some() || env("VERCEL_OIDC_TOKEN").is_some(),
        "nvidia_configured": env("NVIDIA_API_KEY").is_some(),
        "client_data_routes": show(Sensitivity::ClientData),
        "non_sensitive_routes": show(Sensitivity::NonSensitive),
        "rule": "Client and company data uses zero-retention gateway hosts only. NVIDIA free models get non-sensitive prompts only. Watchers never need AI.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k: &str| m.get(k).cloned()
    }

    #[test]
    fn off_by_default() {
        let e = env_of(&[]);
        assert!(!enabled(&e));
        let e = env_of(&[("GODS_EYE_AI", "true")]);
        assert!(!enabled(&e));
    }

    #[test]
    fn client_data_plan_is_zero_retention_gateway_only() {
        let e = env_of(&[("NVIDIA_API_KEY", "x")]);
        let p = plan(Sensitivity::ClientData, &e);
        assert_eq!(p[0].model, "deepseek/deepseek-v4.1-flash-1017");
        assert_eq!(p[0].only, vec!["fireworks", "togetherai", "baseten"]);
        assert_eq!(p.last().unwrap().model, "anthropic/claude-sonnet-5.5");
        assert!(p.iter().all(|r| r.provider == Provider::Gateway && r.zero_data_retention));
        assert!(p.iter().all(|r| !r.only.iter().any(|h| h == "deepseek")));
    }

    #[test]
    fn non_sensitive_plan_adds_nvidia_only_with_a_key() {
        let p = plan(Sensitivity::NonSensitive, &env_of(&[]));
        assert!(p.iter().all(|r| r.provider == Provider::Gateway));
        let p = plan(Sensitivity::NonSensitive, &env_of(&[("NVIDIA_API_KEY", "x")]));
        let nv: Vec<_> = p.iter().filter(|r| r.provider == Provider::NvidiaFree).map(|r| r.model.as_str()).collect();
        assert_eq!(nv, DEFAULT_NVIDIA_MODELS);
    }

    #[test]
    fn overrides_cannot_route_client_data_to_free_or_retaining_hosts() {
        let e = env_of(&[
            ("GODS_EYE_MODELS", "nvidia/nemotron-3.5-lightning-free,deepseek/deepseek-v4.1-flash-1017"),
            ("GODS_EYE_ZDR_PROVIDERS", "deepseek,fireworks"),
            ("GODS_EYE_SENSITIVE_FALLBACK_MODELS", "openrouter/free"),
        ]);
        assert!(plan(Sensitivity::ClientData, &e).is_empty());
        let e = env_of(&[("GODS_EYE_ZDR_PROVIDERS", "novita")]);
        assert!(plan(Sensitivity::ClientData, &e).iter().all(|r| r.model.starts_with("anthropic/")));
        let nv = Route { provider: Provider::NvidiaFree, model: "openai/gpt-oss-20b".into(), only: vec![], zero_data_retention: false };
        assert!(guard(&nv, Sensitivity::ClientData).is_err());
        assert!(guard(&nv, Sensitivity::NonSensitive).is_ok());
        let no_zdr = Route { provider: Provider::Gateway, model: "x/y".into(), only: vec!["fireworks".into()], zero_data_retention: false };
        assert!(guard(&no_zdr, Sensitivity::ClientData).is_err());
        let no_hosts = Route { provider: Provider::Gateway, model: "x/y".into(), only: vec![], zero_data_retention: true };
        assert!(guard(&no_hosts, Sensitivity::ClientData).is_err());
    }

    #[test]
    fn gateway_body_always_carries_zero_retention() {
        let r = &plan(Sensitivity::ClientData, &env_of(&[]))[0];
        let b = body(r, "s", "u");
        assert_eq!(b["providerOptions"]["gateway"]["zeroDataRetention"], true);
        assert_eq!(b["providerOptions"]["gateway"]["disallowPromptTraining"], true);
        assert_eq!(b["providerOptions"]["gateway"]["only"], json!(["fireworks", "togetherai", "baseten"]));
    }

    #[test]
    fn redaction_masks_long_digit_runs() {
        assert_eq!(redact("SIN 123 456 789 on file"), "SIN •••789 on file");
        assert_eq!(redact("acct 12345678901"), "acct •••901");
        assert_eq!(redact("owes 125000 cents"), "owes 125000 cents");
        assert_eq!(redact("Q3 2026-07-01 to 2026-09-30"), "Q3 2026-07-01 to 2026-09-30");
        assert_eq!(redact("SIN 123-456-789"), "SIN •••789");
    }

    #[tokio::test]
    async fn draft_refuses_when_off_and_never_calls_out() {
        let e = env_of(&[("AI_GATEWAY_API_KEY", "x")]);
        assert_eq!(draft(Sensitivity::ClientData, "s", "u", &e).await.unwrap_err(), "ai_drafting_disabled");
        let e = env_of(&[("GODS_EYE_AI", "on")]);
        let err = draft(Sensitivity::ClientData, "s", "u", &e).await.unwrap_err();
        assert!(err.contains("not configured"), "{err}");
    }
}
