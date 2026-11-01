//! Nixon review gate for `review` steps.
//!
//! Before a step closes without a person, Nixon re-checks it independently:
//! it assesses the step again and requires the same ready evidence, and it
//! reads what the agent did on this step. Any failed action, any agent request
//! for a human, or any change between the two assessments escalates the step
//! to the human review queue. Nixon only approves or escalates; it never acts.

use super::capability::{Capability, Readiness, StepContext};
use super::service::AgentAction;
use serde::Serialize;
use serde_json::Value;

pub const REVIEWER: &str = "nixon_review";

#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub reviewer: &'static str,
    pub approved: bool,
    pub reasons: Vec<String>,
}

pub async fn review(cap: &dyn Capability, ctx: &StepContext, evidence: &Value, actions: &[AgentAction]) -> Verdict {
    let mut reasons = Vec::new();
    match cap.assess(ctx).await {
        Readiness::Ready { evidence: again } if &again == evidence => {}
        Readiness::Ready { .. } => reasons.push("evidence changed between assessment and review".into()),
        other => reasons.push(format!("independent re-assessment is not ready: {}", serde_json::to_value(&other).unwrap_or(Value::Null))),
    }
    for a in actions.iter().filter(|a| a.step == ctx.step) {
        if a.tool == "escalate_to_human" {
            reasons.push(format!("agent asked for a human: {}", a.input["reason"].as_str().unwrap_or("no reason given")));
        } else if !a.ok {
            reasons.push(format!("agent action {} failed: {}", a.tool, a.result.as_str().unwrap_or("error")));
        }
    }
    Verdict { reviewer: REVIEWER, approved: reasons.is_empty(), reasons }
}
