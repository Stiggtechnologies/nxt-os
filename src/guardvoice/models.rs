use serde::{Deserialize, Serialize};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub role: String,
    pub text: String,
    pub at: String,
    pub generation: u64,
    #[serde(default)]
    pub dropped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub reason: String,
    pub caller_name: String,
    pub accepted: bool,
    pub declined: bool,
    pub outcome: Option<String>,
    pub target_masked: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub call_sid: String,
    pub direction: String,
    pub from: String,
    pub to: String,
    pub status: String,
    pub role: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub turns: Vec<Turn>,
    pub generation: u64,
    pub speaking: bool,
    pub tools_used: Vec<String>,
    pub transfer_attempted: bool,
    pub transfer: Option<Transfer>,
    pub summary: Option<String>,
    pub handoff_id: Option<String>,
    pub purpose: Option<String>,
    pub brief: Option<String>,
    pub company: String,
    #[serde(default)]
    pub owner_notified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub id: String,
    pub call_id: String,
    pub call_sid: String,
    pub kind: String,
    pub at: String,
    pub from: String,
    pub caller_name: Option<String>,
    pub callback_phone: Option<String>,
    pub summary: String,
    pub urgency: String,
    pub needs_decision: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub id: String,
    pub call_id: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub at: String,
    pub level: String,
    pub title: String,
    pub body: String,
    pub href: String,
    pub read: bool,
    pub call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsMessage {
    pub id: String,
    pub at: String,
    pub to: String,
    pub from: String,
    pub body: String,
    pub status: String,
    pub kind: String,
    pub call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundJob {
    pub id: String,
    pub to: String,
    pub from: String,
    pub purpose: String,
    pub brief: String,
    pub status: String,
    pub created_at: String,
    pub call_sid: Option<String>,
    pub error: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct NewCall {
    pub call_sid: String,
    pub direction: String,
    pub from: String,
    pub to: String,
    pub role: String,
    pub purpose: Option<String>,
    pub brief: Option<String>,
}
