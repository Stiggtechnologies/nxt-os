//! Autonomy policy: who may close a step once its work is shown done.
//!
//! - `auto`: the capability's evidence closes the step.
//! - `review`: Nixon re-checks the work first and escalates to a human when in doubt.
//! - `human`: a reviewer or admin always closes it.
//!
//! Every step defaults to `review`. Steps the profession or the client
//! relationship reserve for people are locked to `human` and cannot be loosened:
//! the journey's human gates (engagement letter, filing authorization, e-file)
//! plus anything that prices work or moves money. `NXT_AUTONOMY` (JSON, e.g.
//! `{"books_access":"auto"}`) overrides the rest; invalid entries are reported.

use super::{Gate, JourneyDef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    Auto,
    Review,
    Human,
}

/// Auto-gated steps that still need a person: they price work or move money.
pub const LOCKED_HUMAN: [&str; 3] = ["proposal", "invoicing", "payment"];

#[derive(Debug, Clone)]
pub struct Policy {
    modes: BTreeMap<String, Autonomy>,
    pub rejected: Vec<String>,
}

impl Policy {
    pub fn new(def: &JourneyDef, overrides: Option<&str>) -> Self {
        let locked = |key: &str| def.step(key).is_some_and(|s| s.gate == Gate::Human) || LOCKED_HUMAN.contains(&key);
        let mut modes: BTreeMap<String, Autonomy> = def.steps.iter()
            .map(|s| (s.key.clone(), if locked(&s.key) { Autonomy::Human } else { Autonomy::Review }))
            .collect();
        let mut rejected = Vec::new();
        if let Some(raw) = overrides.filter(|r| !r.trim().is_empty()) {
            match serde_json::from_str::<BTreeMap<String, Autonomy>>(raw) {
                Ok(map) => {
                    for (key, mode) in map {
                        if def.step(&key).is_none() {
                            rejected.push(format!("{key}: unknown step"));
                        } else if locked(&key) && mode != Autonomy::Human {
                            rejected.push(format!("{key}: locked to human"));
                        } else {
                            modes.insert(key, mode);
                        }
                    }
                }
                Err(_) => rejected.push("NXT_AUTONOMY is not a JSON object of step to auto|review|human".into()),
            }
        }
        Self { modes, rejected }
    }

    pub fn from_env(def: &JourneyDef) -> Self {
        Self::new(def, std::env::var("NXT_AUTONOMY").ok().as_deref())
    }

    pub fn mode(&self, step: &str) -> Autonomy {
        self.modes.get(step).copied().unwrap_or(Autonomy::Human)
    }

    pub fn view(&self) -> Value {
        json!({"modes": self.modes, "locked_human": LOCKED_HUMAN, "rejected_overrides": self.rejected})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_review_and_locks_human_steps() {
        let def = JourneyDef::load().unwrap();
        let p = Policy::new(&def, Some(r#"{"books_access":"auto","engagement_letter":"auto","payment":"review","nope":"auto"}"#));
        assert_eq!(p.mode("books_access"), Autonomy::Auto);
        assert_eq!(p.mode("data_migration"), Autonomy::Review);
        assert_eq!(p.mode("engagement_letter"), Autonomy::Human);
        assert_eq!(p.mode("payment"), Autonomy::Human);
        assert_eq!(p.rejected.len(), 3);
    }
}
