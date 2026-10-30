//! Persisted client journeys driven by capabilities.
//!
//! One record per client holds the journey state machine, the client's source
//! software profile and the latest capability assessment. `advance` walks the
//! journey forward as far as capabilities can prove the work is done; it stops
//! at the first step that is blocked, manual, or awaiting a human. Writes use
//! compare-and-swap on the record so concurrent advances cannot lose entries.

use super::capability::{Capabilities, Readiness, SourceProfile, StepContext};
use super::{Actor, ClientJourney, Gate, JourneyDef, JourneyError, StepStatus};
use crate::referral::routes::ApiError;
use crate::store::Store;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const INDEX: &str = "journey:index";
const MAX_JOURNEYS: usize = 2_000;

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub step: String,
    pub capability: Option<String>,
    pub at: String,
    pub readiness: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyRecord {
    pub journey: ClientJourney,
    #[serde(default)]
    pub sources: SourceProfile,
    pub created_at: String,
    #[serde(default)]
    pub last_assessment: Option<Assessment>,
}

pub struct Journeys {
    store: Store,
    def: JourneyDef,
    capabilities: Capabilities,
}

fn key(client: &str) -> String {
    format!("journey:client:{client}")
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn bad(m: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, m.into())
}
fn storage(m: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("journey storage: {m}"))
}
fn journey_error(e: JourneyError) -> ApiError {
    let m = match e {
        JourneyError::UnknownStep(s) => format!("unknown step {s}"),
        JourneyError::PriorStepIncomplete { step, blocked_by } => format!("{step} cannot start before {blocked_by} is done"),
        JourneyError::InvalidTransition { step, from, to } => format!("{step} cannot move from {from:?} to {to:?}"),
        JourneyError::HumanApprovalRequired(s) => format!("{s} needs approval by a human reviewer"),
        JourneyError::NotSkippable(s) => format!("{s} cannot be skipped"),
    };
    ApiError(StatusCode::CONFLICT, m)
}
pub fn valid_client_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id.chars().all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
}

/// What staff are recording for a step that is waiting on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaffAction {
    Complete,
    Approve,
    Skip,
    Fail,
}

impl Journeys {
    pub fn new(store: Store, capabilities: Capabilities) -> Self {
        let def = JourneyDef::load().expect("journey.json is validated by tests");
        Self { store, def, capabilities }
    }

    pub fn def(&self) -> &JourneyDef {
        &self.def
    }

    /// Which steps a capability can do, and which are still manual.
    pub fn coverage(&self) -> Value {
        let steps = self.def.steps.iter().map(|s| json!({
            "n": s.n, "step": s.key, "phase": s.phase, "gate": s.gate,
            "capability": self.capabilities.for_step(&s.key).map(|c| c.id()),
        })).collect::<Vec<_>>();
        let automated = steps.iter().filter(|s| !s["capability"].is_null()).count();
        json!({"steps": steps, "automated": automated, "total": self.def.steps.len()})
    }

    fn view(&self, r: &JourneyRecord) -> Value {
        json!({
            "client_id": r.journey.client_id,
            "progress": r.journey.progress(&self.def),
            "complete": r.journey.is_complete(&self.def),
            "status": r.journey.status,
            "sources": r.sources,
            "last_assessment": r.last_assessment,
            "ledger": r.journey.ledger,
            "created_at": r.created_at,
        })
    }

    async fn load(&self, client: &str) -> Result<(String, JourneyRecord)> {
        if !valid_client_id(client) {
            return Err(bad("invalid client id"));
        }
        let raw = self.store.get(&key(client)).await.map_err(storage)?
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no journey for this client".into()))?;
        let rec = serde_json::from_str(&raw).map_err(|_| storage("corrupt journey record"))?;
        Ok((raw, rec))
    }

    async fn save(&self, client: &str, expected: &str, rec: &JourneyRecord) -> Result<()> {
        let raw = serde_json::to_string(rec).map_err(storage)?;
        if !self.store.compare_exchange(&key(client), Some(expected), &raw).await.map_err(storage)? {
            return Err(ApiError(StatusCode::CONFLICT, "journey changed concurrently; retry".into()));
        }
        Ok(())
    }

    pub async fn create(&self, client: &str, sources: SourceProfile) -> Result<Value> {
        if !valid_client_id(client) {
            return Err(bad("invalid client id"));
        }
        if sources.dtmax_company.as_deref().is_some_and(|c| !valid_client_id(c)) {
            return Err(bad("invalid DT Max company scope"));
        }
        if self.store.smembers(INDEX).await.map_err(storage)?.len() >= MAX_JOURNEYS {
            return Err(ApiError(StatusCode::CONFLICT, "journey capacity reached".into()));
        }
        let rec = JourneyRecord { journey: ClientJourney::new(&self.def, client), sources, created_at: now(), last_assessment: None };
        let raw = serde_json::to_string(&rec).map_err(storage)?;
        if !self.store.set_nx(&key(client), &raw).await.map_err(storage)? {
            return Err(ApiError(StatusCode::CONFLICT, "a journey already exists for this client".into()));
        }
        self.store.sadd(INDEX, client).await.map_err(storage)?;
        Ok(self.view(&rec))
    }

    pub async fn get(&self, client: &str) -> Result<Value> {
        Ok(self.view(&self.load(client).await?.1))
    }

    pub async fn list(&self) -> Result<Value> {
        let mut ids = self.store.smembers(INDEX).await.map_err(storage)?;
        ids.sort();
        let mut out = Vec::new();
        for id in ids.iter().take(MAX_JOURNEYS) {
            if let Ok((_, r)) = self.load(id).await {
                out.push(json!({"client_id": id, "progress": r.journey.progress(&self.def), "last_assessment": r.last_assessment}));
            }
        }
        Ok(json!({"journeys": out}))
    }

    pub async fn set_sources(&self, client: &str, sources: SourceProfile) -> Result<Value> {
        if sources.dtmax_company.as_deref().is_some_and(|c| !valid_client_id(c)) {
            return Err(bad("invalid DT Max company scope"));
        }
        let (raw, mut rec) = self.load(client).await?;
        rec.sources = sources;
        self.save(client, &raw, &rec).await?;
        Ok(self.view(&rec))
    }

    /// Moves the journey forward while capabilities show the work is done.
    pub async fn advance(&self, client: &str) -> Result<Value> {
        let (raw, mut rec) = self.load(client).await?;
        let mut moved = Vec::new();
        let mut assessments = Vec::new();
        while let Some(step) = rec.journey.current_step(&self.def).cloned() {
            let status = rec.journey.status_of(&step.key).unwrap_or(StepStatus::Pending);
            if status == StepStatus::AwaitingApproval {
                break;
            }
            let Some(cap) = self.capabilities.for_step(&step.key) else {
                rec.last_assessment = Some(Assessment {
                    step: step.key.clone(), capability: None, at: now(),
                    readiness: json!({"state": "manual", "reasons": [format!("no capability for {} yet; staff record it", step.key)]}),
                });
                break;
            };
            let ctx = StepContext { client_id: client.to_string(), step: step.key.clone(), sources: rec.sources.clone() };
            let readiness = cap.assess(&ctx).await;
            let at = now();
            let actor = Actor::Automation { executor: cap.id().to_string() };
            let assessment = Assessment {
                step: step.key.clone(), capability: Some(cap.id().into()), at: at.clone(),
                readiness: serde_json::to_value(&readiness).unwrap_or(Value::Null),
            };
            assessments.push(assessment.clone());
            rec.last_assessment = Some(assessment);
            match readiness {
                Readiness::Ready { .. } => {
                    if status != StepStatus::InProgress {
                        rec.journey.start(&self.def, &step.key, actor.clone(), &at).map_err(journey_error)?;
                    }
                    rec.journey.finish_work(&self.def, &step.key, actor, &at).map_err(journey_error)?;
                    moved.push(step.key.clone());
                    if step.gate == Gate::Human {
                        break;
                    }
                }
                Readiness::Skip { reason } if step.skippable && status == StepStatus::Pending => {
                    rec.journey.skip(&self.def, &step.key, actor, &at, &reason).map_err(journey_error)?;
                    moved.push(step.key.clone());
                }
                _ => break,
            }
        }
        self.save(client, &raw, &rec).await?;
        let mut v = self.view(&rec);
        v["moved"] = json!(moved);
        v["assessments"] = json!(assessments);
        Ok(v)
    }

    /// Staff record a step that has no capability, or approve a human gate.
    pub async fn staff(&self, client: &str, step: &str, action: StaffAction, actor: Actor, reviewer: bool, note: &str) -> Result<Value> {
        let (raw, mut rec) = self.load(client).await?;
        let def = &self.def;
        let gate = def.step(step).ok_or_else(|| bad("unknown step"))?.gate;
        let at = now();
        let j = &mut rec.journey;
        let status = j.status_of(step).unwrap_or(StepStatus::Pending);
        match action {
            StaffAction::Complete => {
                if matches!(status, StepStatus::Pending | StepStatus::Failed) {
                    j.start(def, step, actor.clone(), &at).map_err(journey_error)?;
                }
                j.finish_work(def, step, actor, &at).map_err(journey_error)?;
            }
            StaffAction::Approve => {
                if gate != Gate::Human || status != StepStatus::AwaitingApproval {
                    return Err(ApiError(StatusCode::CONFLICT, "only a human-gated step awaiting approval can be approved".into()));
                }
                if !reviewer {
                    return Err(ApiError(StatusCode::FORBIDDEN, "approval needs a reviewer or admin".into()));
                }
                j.approve(def, step, actor, &at, if note.is_empty() { "approved" } else { note }).map_err(journey_error)?;
            }
            StaffAction::Skip => {
                if note.trim().is_empty() {
                    return Err(bad("a reason is required to skip a step"));
                }
                j.skip(def, step, actor, &at, note).map_err(journey_error)?;
            }
            StaffAction::Fail => {
                if note.trim().is_empty() {
                    return Err(bad("a reason is required to fail a step"));
                }
                j.fail(def, step, actor, &at, note).map_err(journey_error)?;
            }
        }
        self.save(client, &raw, &rec).await?;
        Ok(self.view(&rec))
    }

    /// Scheduled pass: advance every journey; one failure does not stop the rest.
    pub async fn tick(&self) -> Result<Value> {
        let mut ids = self.store.smembers(INDEX).await.map_err(storage)?;
        ids.sort();
        let mut results = Vec::new();
        for id in ids.iter().take(MAX_JOURNEYS) {
            match self.advance(id).await {
                Ok(v) => results.push(json!({"client_id": id, "moved": v["moved"], "current_step": v["progress"]["current_step"]})),
                Err(e) => results.push(json!({"client_id": id, "error": e.1})),
            }
        }
        Ok(json!({"journeys": results.len(), "results": results}))
    }
}
