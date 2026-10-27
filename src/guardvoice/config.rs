//! Env-only configuration. Nothing here is read from a file, and nothing
//! secret is written back out.

use super::voice::{digits10, e164};
use crate::nixon::Engine;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Mock,
    Live,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// Twilio ConversationRelay over a WebSocket. Low latency. Needs a
    /// process that can hold the socket (not a Vercel request).
    Relay,
    /// Speech `<Gather>` over HTTPS. Works on the serverless deploy.
    Gather,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureMode {
    Off,
    Log,
    Enforce,
}

/// What Nixon may do without a person.
/// Consequential actions always wait, whichever level this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Autonomy {
    Read,
    Write,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: Mode,
    pub transport: Transport,
    pub signature_mode: SignatureMode,
    pub autonomy: Autonomy,
    /// How Nixon decides: scripted in tests and mock mode, Claude when asked.
    pub nixon: Engine,
    pub agent_name: String,
    pub company_fallback: String,
    pub timezone: String,
    pub owner_sms: Option<String>,
    pub from_number: Option<String>,
    pub transfer_number: Option<String>,
    pub transfer_person: String,
    pub loop_guard: Vec<String>,
    pub public_host: String,
    pub account_sid: String,
    pub auth_token: String,
    pub stream_secret: String,
    pub welcome: Option<String>,
    pub notes_webhook_url: Option<String>,
    pub notes_webhook_secret: Option<String>,
    pub language: String,
    pub tts_provider: String,
    pub tts_voice: String,
    pub stt_provider: String,
}

impl Config {
    /// Mock config for tests and the local simulator. The stream secret is a
    /// fixed dev value; set `GUARDVOICE_STREAM_SECRET` before any public host.
    pub fn mock() -> Self {
        Config {
            mode: Mode::Mock,
            transport: Transport::Relay,
            signature_mode: SignatureMode::Off,
            autonomy: Autonomy::Write,
            nixon: Engine::Scripted,
            agent_name: "Nixon".into(),
            company_fallback: "your business".into(),
            timezone: "America/Toronto".into(),
            owner_sms: Some("+15555550199".into()),
            from_number: Some("+15555550100".into()),
            transfer_number: Some("+15555550188".into()),
            transfer_person: "the owner".into(),
            loop_guard: vec![],
            public_host: "voice.test".into(),
            account_sid: String::new(),
            auth_token: String::new(),
            stream_secret: "gv-mock-stream-secret".into(),
            welcome: None,
            notes_webhook_url: None,
            notes_webhook_secret: None,
            language: "en-CA".into(),
            tts_provider: "Google".into(),
            tts_voice: "en-US-Neural2-F".into(),
            stt_provider: "Deepgram".into(),
        }
    }

    pub fn from_env() -> Self {
        let mut c = Config::mock();
        c.owner_sms = None;
        c.from_number = None;
        c.transfer_number = None;
        c.public_host.clear();
        c.stream_secret.clear();

        let sid = env("TWILIO_ACCOUNT_SID");
        let token = env("TWILIO_AUTH_TOKEN");
        c.account_sid = sid.clone();
        c.auth_token = token.clone();
        let twilio_ready = !sid.is_empty() && !token.is_empty();
        c.mode = match env("GUARDVOICE_MODE").to_ascii_lowercase().as_str() {
            "live" => Mode::Live,
            "mock" | "test" => Mode::Mock,
            _ => {
                if twilio_ready {
                    Mode::Live
                } else {
                    Mode::Mock
                }
            }
        };
        c.transport = match env("GUARDVOICE_TRANSPORT").to_ascii_lowercase().as_str() {
            "gather" | "serverless" => Transport::Gather,
            _ => Transport::Relay,
        };
        c.signature_mode = match env("GUARDVOICE_SIGNATURE_MODE").to_ascii_lowercase().as_str() {
            "off" => SignatureMode::Off,
            "log" => SignatureMode::Log,
            "enforce" => SignatureMode::Enforce,
            _ => {
                if c.mode == Mode::Mock {
                    SignatureMode::Off
                } else {
                    SignatureMode::Enforce
                }
            }
        };
        c.autonomy = match env("GUARDVOICE_AUTONOMY").to_ascii_lowercase().as_str() {
            "read" | "readonly" => Autonomy::Read,
            _ => Autonomy::Write,
        };
        c.nixon = match env("NIXON_ENGINE").to_ascii_lowercase().as_str() {
            "scripted" | "mock" => Engine::Scripted,
            "claude" | "anthropic" => Engine::Claude,
            _ => {
                if c.mode == Mode::Mock {
                    Engine::Scripted
                } else {
                    Engine::Claude
                }
            }
        };
        if let Some(v) = env_opt("GUARDVOICE_AGENT_NAME") {
            c.agent_name = v;
        }
        if let Some(v) = env_opt("GUARDVOICE_COMPANY_NAME") {
            c.company_fallback = v;
        }
        if let Some(v) = env_opt("GUARDVOICE_TIMEZONE") {
            c.timezone = v;
        }
        if let Some(v) = env_opt("GUARDVOICE_TRANSFER_NAME") {
            c.transfer_person = v;
        }
        c.owner_sms = phone("GUARDVOICE_OWNER_SMS");
        c.from_number = phone("GUARDVOICE_FROM_NUMBER").or_else(|| phone("TWILIO_PHONE_NUMBER"));
        c.transfer_number = phone("GUARDVOICE_TRANSFER_NUMBER");
        c.loop_guard = env("GUARDVOICE_LOOP_GUARD")
            .split(',')
            .filter_map(|n| e164(n.trim()))
            .collect();
        c.public_host = env("GUARDVOICE_PUBLIC_HOST")
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string();
        if c.public_host == "auto" {
            c.public_host.clear();
        }
        c.stream_secret = env_opt("GUARDVOICE_STREAM_SECRET").unwrap_or_else(|| {
            if c.mode == Mode::Mock {
                "gv-mock-stream-secret".into()
            } else {
                uuid::Uuid::new_v4().to_string()
            }
        });
        c.welcome = env_opt("GUARDVOICE_WELCOME");
        c.notes_webhook_url = env_opt("GUARDVOICE_NOTES_WEBHOOK_URL");
        c.notes_webhook_secret = env_opt("GUARDVOICE_NOTES_WEBHOOK_SECRET");
        if let Some(v) = env_opt("GUARDVOICE_LANGUAGE") {
            c.language = v;
        }
        if let Some(v) = env_opt("GUARDVOICE_TTS_PROVIDER") {
            c.tts_provider = v;
        }
        if let Some(v) = env_opt("GUARDVOICE_TTS_VOICE") {
            c.tts_voice = v;
        }
        if let Some(v) = env_opt("GUARDVOICE_STT_PROVIDER") {
            c.stt_provider = v;
        }
        c
    }

    pub fn twilio_ready(&self) -> bool {
        self.mode == Mode::Live && !self.account_sid.is_empty() && !self.auth_token.is_empty() && self.from_number.is_some()
    }

    pub fn status(&self) -> Value {
        json!({
            "mode": match self.mode { Mode::Mock => "mock", Mode::Live => "live" },
            "transport": match self.transport { Transport::Relay => "relay", Transport::Gather => "gather" },
            "signature_mode": match self.signature_mode { SignatureMode::Off => "off", SignatureMode::Log => "log", SignatureMode::Enforce => "enforce" },
            "autonomy": match self.autonomy { Autonomy::Read => "read", Autonomy::Write => "write" },
            "nixon": match self.nixon { Engine::Scripted => "scripted", Engine::Claude => "claude" },
            "assistant": "Nixon",
            "agent_name": self.agent_name,
            "timezone": self.timezone,
            "owner_sms_configured": self.owner_sms.is_some(),
            "from_number_configured": self.from_number.is_some(),
            "transfer_enabled": self.transfer_target_digits().is_some(),
            "twilio_ready": self.twilio_ready(),
            "ai_enabled": std::env::var("ANTHROPIC_API_KEY").ok().is_some_and(|k| !k.trim().is_empty()),
            "notes_webhook_configured": self.notes_webhook_url.is_some(),
            "public_host_configured": !self.public_host.is_empty(),
            "stream_secret_from_env": std::env::var("GUARDVOICE_STREAM_SECRET").ok().is_some_and(|s| !s.trim().is_empty()),
        })
    }

    /// Digits of a transfer target that is safe to dial, or none.
    pub fn transfer_target_digits(&self) -> Option<String> {
        let n = self.transfer_number.as_deref()?;
        let d = digits10(n);
        if d.len() != 10 || self.blocked_digits().contains(&d) {
            None
        } else {
            Some(d)
        }
    }

    pub fn blocked_digits(&self) -> Vec<String> {
        let mut v = Vec::new();
        for n in self.from_number.iter().chain(self.owner_sms.iter()).chain(self.loop_guard.iter()) {
            let d = digits10(n);
            if !d.is_empty() {
                v.push(d);
            }
        }
        v
    }
}

fn env(name: &str) -> String {
    std::env::var(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_default()
}

fn env_opt(name: &str) -> Option<String> {
    let s = env(name);
    if s.is_empty() { None } else { Some(s) }
}

fn phone(name: &str) -> Option<String> {
    let raw = env_opt(name)?;
    match e164(&raw) {
        Some(n) => Some(n),
        None => {
            eprintln!("guardvoice: {name} is not E.164 and will be ignored");
            None
        }
    }
}
