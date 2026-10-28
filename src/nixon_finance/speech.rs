//! Server-side OpenAI Realtime and cloud TTS.
//!
//! The provider key stays on the server. When it is unset, both routes
//! refuse and the page keeps using browser speech. A daily character cap
//! is reserved before any speech request is sent.

use super::service::NixonError;
use super::voice::{strip_for_speech, SPEECH_MAX_CHARS};
use crate::ledger::engine::Ledger;
use serde::Deserialize;

const REALTIME_URL: &str = "https://api.openai.com/v1/realtime/calls";
const SPEECH_URL: &str = "https://api.openai.com/v1/audio/speech";
const DEFAULT_DAILY_CHARS: i64 = 420 * 40;

#[derive(Debug, Deserialize)]
pub struct RealtimeOffer {
    pub sdp: String,
}

pub fn openai_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY").ok().filter(|k| !k.trim().is_empty())
}

fn daily_cap() -> i64 {
    std::env::var("NIXON_TTS_DAILY_CHARS").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0 && *n < DEFAULT_DAILY_CHARS).unwrap_or(DEFAULT_DAILY_CHARS)
}

pub fn realtime_session() -> serde_json::Value {
    serde_json::json!({
        "type": "realtime",
        "model": std::env::var("NIXON_REALTIME_MODEL").ok().filter(|m| !m.is_empty()).unwrap_or_else(|| "gpt-realtime-2.1".into()),
        "instructions": "You are Nixon, the finance voice in NXT OS. Speak short Canadian English. For any request about the books, a module, an account, an invoice, a client, an engagement, cash, a review, or opening a screen, call nixon_command with the user's words and then say only the say field it returns. If it returns choices, read the numbered options and wait. Never approve, post, file, or claim a screen opened unless the tool result includes an open target. Do not invent balances.",
        "audio": { "output": { "voice": "marin" } },
        "tools": [{
            "type": "function",
            "name": "nixon_command",
            "description": "Run one read-only Nixon command. The result say text is what you speak. open is a screen the app will show. choices mean you must ask which one.",
            "parameters": {
                "type": "object",
                "additionalProperties": false,
                "properties": { "transcript": { "type": "string" } },
                "required": ["transcript"]
            }
        }],
        "tool_choice": "auto"
    })
}

pub async fn negotiate(ledger: &Ledger, offer: RealtimeOffer) -> Result<serde_json::Value, NixonError> {
    if offer.sdp.trim().is_empty() {
        return Err(NixonError::new(400, "sdp is required"));
    }
    if offer.sdp.len() > 65_536 {
        return Err(NixonError::new(413, "sdp is too large"));
    }
    let Some(key) = openai_key() else {
        return Err(NixonError::new(503, "realtime_unconfigured"));
    };
    let day = crate::ledger::engine::today();
    let used = ledger.store.incr_by(&format!("nixon:realtime:{day}"), 1).await.map_err(|m| NixonError::new(500, m))?;
    if used > 20 {
        return Err(NixonError::new(429, "realtime_quota"));
    }
    let boundary = "nixonrealtimeboundary";
    let session = realtime_session().to_string();
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"sdp\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"session\"\r\n\r\n{session}\r\n--{boundary}--\r\n",
        offer.sdp
    );
    let response = reqwest::Client::new()
        .post(REALTIME_URL)
        .bearer_auth(key)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| NixonError::new(502, "voice_provider_unavailable"))?;
    let status = response.status();
    let answer = response.text().await.map_err(|_| NixonError::new(502, "voice_provider_unavailable"))?;
    if !status.is_success() {
        return Err(NixonError::new(502, "voice_provider_unavailable"));
    }
    ledger.audit("nixon.realtime", format!("session=negotiated posting=disabled filing=disabled day={day}")).await?;
    Ok(serde_json::json!({
        "session": { "id": "realtime" },
        "model": "gpt-realtime-2.1",
        "fallback": "none",
        "transport": { "type": "webrtc", "sdp": answer },
    }))
}

#[derive(Debug, Deserialize)]
pub struct SpeakRequest {
    pub text: String,
}

pub async fn synthesize(ledger: &Ledger, request: SpeakRequest) -> Result<Vec<u8>, NixonError> {
    let text = strip_for_speech(&request.text);
    if text.is_empty() {
        return Err(NixonError::new(400, "nothing to speak"));
    }
    if text.chars().count() > SPEECH_MAX_CHARS + 1 {
        return Err(NixonError::new(400, "speech is too long"));
    }
    let Some(key) = openai_key() else {
        return Err(NixonError::new(503, "tts_unconfigured"));
    };
    let day = crate::ledger::engine::today();
    let used = ledger.store.incr_by(&format!("nixon:tts:{day}"), text.chars().count() as i64).await.map_err(|m| NixonError::new(500, m))?;
    if used > daily_cap() {
        return Err(NixonError::new(429, "tts_quota"));
    }
    let voice = std::env::var("NIXON_TTS_VOICE").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "marin".into());
    let response = reqwest::Client::new()
        .post(SPEECH_URL)
        .bearer_auth(key)
        .json(&serde_json::json!({
            "model": "gpt-4o-mini-tts",
            "voice": voice,
            "input": text,
            "instructions": "Speak in a warm, clear Canadian English voice. Never read markdown. Do not add facts that are not in the text.",
        }))
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|_| NixonError::new(502, "voice_provider_unavailable"))?;
    if !response.status().is_success() {
        return Err(NixonError::new(502, "voice_provider_unavailable"));
    }
    response.bytes().await.map(|b| b.to_vec()).map_err(|_| NixonError::new(502, "voice_provider_unavailable"))
}
