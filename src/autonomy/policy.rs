//! Standing approvals: Orville (an admin) may let one action class run without a person
//! each time. Grants are time-boxed, capped per day, revocable, and refused in code for
//! hard-gated classes. They are checked before every step, so a revocation stops an
//! in-flight run at its next step.

use super::tools::{ActionClass, Level};
use crate::store::Store;
use serde::{Deserialize, Serialize};

pub const MAX_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandingApproval {
    pub action_class: ActionClass,
    pub granted_by: String,
    pub reason: String,
    pub granted_at: String,
    pub expires_at: String,
    pub max_runs_per_day: u32,
    #[serde(default)]
    pub revoked_at: Option<String>,
    #[serde(default)]
    pub revoked_by: Option<String>,
}

impl StandingApproval {
    pub fn is_active(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.revoked_at.is_none()
            && chrono::DateTime::parse_from_rfc3339(&self.expires_at).is_ok_and(|e| e.with_timezone(&chrono::Utc) > now)
    }
}

fn key(class: ActionClass) -> String {
    format!("autonomy:policy:{}", class.as_str())
}

pub async fn grant(store: &Store, class: ActionClass, by: &str, reason: &str, days: i64, max_runs_per_day: u32) -> Result<StandingApproval, (u16, String)> {
    if class.is_hard_gate() {
        return Err((403, format!("{} is a hard gate and cannot get a standing approval: {}", class.as_str(), class.gate_reason())));
    }
    if class.default_level() != Level::NeedsApproval {
        return Err((400, format!("{} already runs without a person; no standing approval is needed", class.as_str())));
    }
    if by.trim().chars().count() < 2 {
        return Err((400, "a named person is required".into()));
    }
    if reason.trim().is_empty() {
        return Err((400, "a reason is required".into()));
    }
    if !(1..=MAX_DAYS).contains(&days) {
        return Err((400, format!("days must be 1 to {MAX_DAYS}")));
    }
    if !(1..=500).contains(&max_runs_per_day) {
        return Err((400, "max_runs_per_day must be 1 to 500".into()));
    }
    let now = chrono::Utc::now();
    let policy = StandingApproval {
        action_class: class,
        granted_by: by.trim().into(),
        reason: super::guard::redact(reason.trim()).chars().take(500).collect(),
        granted_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        expires_at: (now + chrono::Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        max_runs_per_day,
        revoked_at: None,
        revoked_by: None,
    };
    store.set_json(&key(class), &policy).await.map_err(|m| (500, m))?;
    store.sadd("autonomy:policies", class.as_str()).await.map_err(|m| (500, m))?;
    Ok(policy)
}

pub async fn revoke(store: &Store, class: ActionClass, by: &str) -> Result<StandingApproval, (u16, String)> {
    let mut policy: StandingApproval = store.get_json(&key(class)).await.map_err(|m| (500, m))?.ok_or((404, "no standing approval for that class".to_string()))?;
    if policy.revoked_at.is_none() {
        policy.revoked_at = Some(crate::ledger::engine::now());
        policy.revoked_by = Some(by.trim().into());
        store.set_json(&key(class), &policy).await.map_err(|m| (500, m))?;
    }
    Ok(policy)
}

pub async fn get(store: &Store, class: ActionClass) -> Result<Option<StandingApproval>, String> {
    store.get_json(&key(class)).await
}

pub async fn list(store: &Store) -> Result<Vec<StandingApproval>, String> {
    let mut out = Vec::new();
    for c in store.smembers("autonomy:policies").await? {
        if let Some(class) = ActionClass::parse(&c) {
            if let Some(p) = get(store, class).await? {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| a.action_class.cmp(&b.action_class));
    Ok(out)
}

/// Counts one use of a class's standing approval today; false once the daily cap is hit.
pub async fn take_daily_use(store: &Store, policy: &StandingApproval) -> Result<bool, String> {
    let day = crate::ledger::engine::today();
    let used = store.incr_by(&format!("autonomy:policy-use:{}:{day}", policy.action_class.as_str()), 1).await?;
    Ok(used <= policy.max_runs_per_day as i64)
}
