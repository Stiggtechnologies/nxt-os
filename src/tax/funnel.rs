//! Staff-only Filing funnel: durable append-only step transitions plus a read-only
//! summary that counts clients at each step and lists stalled ones.
//!
//! Steps (ordered): intake_started → documents_uploaded → interview_done →
//! fee_paid → desk_review → filed. Live hooks write to the `filing-funnel`
//! append-only stream; on read we backfill missing steps from existing return /
//! desk timestamps without mutating client or book data. No client contact,
//! emails, notifications, or public links.
use super::{Tax, TResult, bad, now, models::TaxReturn, filing_desk::{DeskState, fee_paid_at}};
use crate::firm::staff::{Identity, Role};
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub const STREAM: &str = "filing-funnel";
pub const BANNER: &str = "Internal staff funnel. No client access. No notifications.";

/// Ordered funnel steps. Index is the step number (1-based in the API).
pub const STEPS: [&str; 6] = [
    "intake_started",
    "documents_uploaded",
    "interview_done",
    "fee_paid",
    "desk_review",
    "filed",
];

/// Default stall thresholds in whole days per step (filed has no stall).
pub fn default_thresholds() -> HashMap<&'static str, i64> {
    HashMap::from([
        ("intake_started", 3),
        ("documents_uploaded", 7),
        ("interview_done", 5),
        ("fee_paid", 3),
        ("desk_review", 2),
        ("filed", 0),
    ])
}

fn forbidden(m: &str) -> ApiError { ApiError(StatusCode::FORBIDDEN, m.into()) }
fn unavailable(e: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("funnel store unavailable: {e}"))
}

fn parse_at(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
        .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ").ok()
            .map(|n| DateTime::<Utc>::from_naive_utc_and_offset(n, Utc)))
}

fn step_index(step: &str) -> Option<usize> { STEPS.iter().position(|s| *s == step) }

fn sees(who: &Identity, client_ref: &str) -> bool {
    who.all_clients() || who.can_client(client_ref)
}

/// Whether inputs look like documents / slips have been uploaded.
pub fn has_documents(inputs: &Value) -> bool {
    let Some(obj) = inputs.as_object() else { return false; };
    for (k, v) in obj {
        let kl = k.to_ascii_lowercase();
        let slipish = ["slip", "income", "employment", "t4", "t5", "t2202", "t5008", "pension",
            "dividend", "interest", "revenue", "sales", "gain", "withheld"].iter().any(|w| kl.contains(w));
        if !slipish { continue; }
        match v {
            Value::Number(n) if n.as_i64().unwrap_or(0).abs() > 0 => return true,
            Value::String(s) if !s.trim().is_empty() => return true,
            Value::Bool(true) => return true,
            Value::Array(a) if !a.is_empty() => return true,
            Value::Object(o) if !o.is_empty() => return true,
            _ => {}
        }
    }
    false
}

/// Derive step → earliest timestamp from existing records (read-only).
pub fn derive_steps(r: &TaxReturn, desk: &DeskState) -> Vec<(String, String)> {
    let mut out = Vec::new();
    out.push(("intake_started".into(), r.created_at.clone()));

    if has_documents(&r.inputs) {
        // Exact first-upload time is not stored on the return; use created_at when the
        // return still has a single input revision, otherwise updated_at.
        let at = if r.input_revision <= 1 || r.created_at == r.updated_at {
            r.created_at.clone()
        } else {
            r.updated_at.clone()
        };
        out.push(("documents_uploaded".into(), at));
    }

    // Interview done: calculated (guided review complete enough to compute) or later.
    if let Some(c) = &r.calculation {
        if ["calculated", "in_review", "approved", "filed"].contains(&r.status.as_str()) {
            out.push(("interview_done".into(), c.created_at.clone()));
        }
    } else if let Some(p) = &r.preparer_signoff {
        out.push(("interview_done".into(), p.at.clone()));
    }

    if let Some(at) = fee_paid_at(desk, &r.id) {
        out.push(("fee_paid".into(), at));
    }

    // Filing Desk queue entry: approved + fee paid.
    if r.status == "approved" || r.status == "filed" {
        if let Some(fee_at) = fee_paid_at(desk, &r.id) {
            let review_at = r.review.as_ref().map(|v| v.at.as_str()).unwrap_or(r.updated_at.as_str());
            let desk_at = match (parse_at(review_at), parse_at(&fee_at)) {
                (Some(a), Some(b)) => if a > b { review_at.to_string() } else { fee_at },
                _ => fee_at,
            };
            out.push(("desk_review".into(), desk_at));
        }
    }

    if r.status == "filed" {
        let at = r.filed_record.as_ref().map(|f| f.at.clone()).unwrap_or_else(|| r.updated_at.clone());
        out.push(("filed".into(), at));
    }

    out
}

/// Current step = highest derived step that has a timestamp.
pub fn current_step(derived: &[(String, String)]) -> Option<(String, String)> {
    derived.iter().rev().find(|(s, _)| step_index(s).is_some()).cloned()
}

/// Days the client has been on `step` (from entered_at to `now_at`).
pub fn stall_days(entered_at: &str, now_at: DateTime<Utc>) -> Option<i64> {
    let start = parse_at(entered_at)?;
    Some((now_at - start).num_days().max(0))
}

pub fn is_stalled(step: &str, days: i64, thresholds: &HashMap<&str, i64>) -> bool {
    if step == "filed" { return false; }
    let t = thresholds.get(step).copied().unwrap_or(7);
    t > 0 && days >= t
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunnelEvent {
    pub return_id: String,
    pub client_ref: String,
    pub program: String,
    pub step: String,
    pub at: String,
    pub source: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preparer: Option<String>,
}

impl FunnelEvent {
    pub fn to_json(&self) -> Value {
        json!({
            "return_id": self.return_id,
            "client_ref": self.client_ref,
            "program": self.program,
            "step": self.step,
            "at": self.at,
            "source": self.source,
            "actor": self.actor,
            "preparer": self.preparer,
        })
    }
}

fn preparer_of(r: &TaxReturn) -> Option<String> {
    r.preparer_signoff.as_ref().map(|s| s.by.clone())
}

#[derive(Default, Deserialize)]
pub struct FunnelFilter {
    pub program: Option<String>,
    pub preparer: Option<String>,
    /// Only list clients stalled more than this many days (overrides per-step default floor).
    pub stalled_days: Option<i64>,
}

impl Tax {
    /// Staff (not FrontDesk/ReadOnly break-glass-only) may read the funnel.
    fn funnel_who(&self, who: &Identity) -> TResult<()> {
        if who.break_glass {
            return Err(forbidden("sign in with a staff account to open the Filing funnel"));
        }
        match who.role {
            Role::Admin | Role::Preparer | Role::Reviewer => Ok(()),
            Role::FrontDesk | Role::ReadOnly => Err(forbidden("the Filing funnel is for preparer, reviewer, and admin staff")),
        }
    }

    /// Append one step transition if that (return_id, step) is not already recorded.
    pub async fn funnel_emit(
        &self,
        r: &TaxReturn,
        step: &str,
        source: &str,
        actor: &str,
        at: Option<&str>,
    ) -> TResult<()> {
        if step_index(step).is_none() { return Err(bad(format!("unknown funnel step: {step}"))); }
        let existing = self.store.read_append_only(STREAM, 5000).await.map_err(unavailable)?;
        if existing.iter().any(|e| e["return_id"] == r.id && e["step"] == step) {
            return Ok(());
        }
        let ev = FunnelEvent {
            return_id: r.id.clone(),
            client_ref: r.client_ref.clone(),
            program: r.program.clone(),
            step: step.into(),
            at: at.unwrap_or(&now()).to_string(),
            source: source.into(),
            actor: actor.into(),
            preparer: preparer_of(r),
        };
        self.store.append_only(STREAM, &ev.to_json()).await.map_err(unavailable)?;
        Ok(())
    }

    /// Record every derived step that is missing from the log (backfill, no client/book writes).
    pub async fn funnel_backfill_one(&self, r: &TaxReturn, desk: &DeskState) -> TResult<usize> {
        let derived = derive_steps(r, desk);
        let existing = self.store.read_append_only(STREAM, 5000).await.map_err(unavailable)?;
        let have: HashSet<(String, String)> = existing.iter()
            .filter_map(|e| Some((e["return_id"].as_str()?.to_string(), e["step"].as_str()?.to_string())))
            .collect();
        let mut n = 0;
        for (step, at) in derived {
            if have.contains(&(r.id.clone(), step.clone())) { continue; }
            let ev = FunnelEvent {
                return_id: r.id.clone(),
                client_ref: r.client_ref.clone(),
                program: r.program.clone(),
                step: step.clone(),
                at,
                source: "backfill".into(),
                actor: "system".into(),
                preparer: preparer_of(r),
            };
            self.store.append_only(STREAM, &ev.to_json()).await.map_err(unavailable)?;
            n += 1;
        }
        Ok(n)
    }

    /// Read-only funnel summary: counts at each step + stalled clients. Respects preparer scoping.
    pub async fn funnel_summary(&self, who: &Identity, filter: FunnelFilter) -> TResult<Value> {
        self.funnel_who(who)?;
        let s = self.load().await?;
        // Backfill from existing records on read (tracking table only).
        for r in s.returns.iter().filter(|r| ["T1", "T2"].contains(&r.program.as_str())) {
            if sees(who, &r.client_ref) {
                let _ = self.funnel_backfill_one(r, &s.desk).await;
            }
        }
        let events = self.store.read_append_only(STREAM, 5000).await.map_err(unavailable)?;
        let thresholds = default_thresholds();
        let now_at = Utc::now();
        let min_stall = filter.stalled_days;
        let program_f = filter.program.as_deref().map(|p| p.to_ascii_uppercase());
        let preparer_f = filter.preparer.as_deref().map(|p| p.trim().to_string()).filter(|p| !p.is_empty());

        // Latest event per (return_id, step), then current step per return.
        let mut by_return: HashMap<String, HashMap<String, Value>> = HashMap::new();
        for e in events.iter().rev() {
            // events are newest-first from store; walk oldest-first via rev for first-wins at
            let rid = e["return_id"].as_str().unwrap_or("").to_string();
            let step = e["step"].as_str().unwrap_or("").to_string();
            if rid.is_empty() || step_index(&step).is_none() { continue; }
            by_return.entry(rid).or_default().entry(step).or_insert_with(|| e.clone());
        }
        // Rebuild properly: prefer earliest `at` for each step from all events.
        by_return.clear();
        for e in &events {
            let rid = e["return_id"].as_str().unwrap_or("").to_string();
            let step = e["step"].as_str().unwrap_or("").to_string();
            if rid.is_empty() || step_index(&step).is_none() { continue; }
            let map = by_return.entry(rid).or_default();
            match map.get(&step) {
                None => { map.insert(step, e.clone()); }
                Some(prev) => {
                    let pa = prev["at"].as_str().unwrap_or("");
                    let ea = e["at"].as_str().unwrap_or("");
                    if parse_at(ea).zip(parse_at(pa)).is_some_and(|(a, b)| a < b) || (pa.is_empty() && !ea.is_empty()) {
                        map.insert(step, e.clone());
                    }
                }
            }
        }

        let mut counts = [0usize; STEPS.len()];
        let mut stalled = Vec::new();
        let mut tracked = 0usize;

        for r in s.returns.iter().filter(|r| ["T1", "T2"].contains(&r.program.as_str())) {
            if !sees(who, &r.client_ref) { continue; }
            if let Some(ref p) = program_f {
                if r.program != *p { continue; }
            }
            let prep = preparer_of(r).unwrap_or_default();
            if let Some(ref pf) = preparer_f {
                let match_id = crate::firm::staff::label_user_id(&prep).unwrap_or("");
                if !prep.to_ascii_lowercase().contains(&pf.to_ascii_lowercase())
                    && match_id != pf.as_str()
                {
                    continue;
                }
            }
            tracked += 1;

            // Prefer live derivation for current position (authoritative).
            let derived = derive_steps(r, &s.desk);
            let Some((step, entered)) = current_step(&derived) else { continue; };
            if let Some(i) = step_index(&step) { counts[i] += 1; }

            let days = stall_days(&entered, now_at).unwrap_or(0);
            // If stalled_days filter is set, use it as the minimum; else per-step default.
            let stalled_now = if step == "filed" {
                false
            } else if let Some(n) = min_stall {
                days >= n
            } else {
                is_stalled(&step, days, &thresholds)
            };
            if stalled_now {
                stalled.push(json!({
                    "return_id": r.id,
                    "client_ref": r.client_ref,
                    "name": r.name,
                    "program": r.program,
                    "tax_year": r.tax_year,
                    "step": step,
                    "step_number": step_index(&step).map(|i| i + 1),
                    "entered_at": entered,
                    "days_stalled": days,
                    "threshold_days": if let Some(n) = min_stall { n } else { thresh },
                    "preparer": prep,
                    "status": r.status,
                }));
            }
        }

        stalled.sort_by(|a, b| {
            b["days_stalled"].as_i64().cmp(&a["days_stalled"].as_i64())
                .then(a["client_ref"].as_str().cmp(&b["client_ref"].as_str()))
        });

        let steps_out: Vec<Value> = STEPS.iter().enumerate().map(|(i, s)| json!({
            "step": s,
            "step_number": i + 1,
            "label": step_label(s),
            "count": counts[i],
            "threshold_days": thresholds.get(s).copied().unwrap_or(0),
        })).collect();

        Ok(json!({
            "banner": BANNER,
            "steps": steps_out,
            "stalled": stalled,
            "tracked": tracked,
            "filters": {
                "program": program_f,
                "preparer": preparer_f,
                "stalled_days": min_stall,
            },
            "thresholds": { let mut m = serde_json::Map::new(); for (k,v) in &thresholds { m.insert((*k).into(), json!(*v)); } Value::Object(m) },
            "client_access": false,
            "notifications": false,
        }))
    }

    /// Best-effort live hook: never fails the parent write if the funnel log is unavailable.
    pub async fn funnel_hook(&self, r: &TaxReturn, step: &str, source: &str, actor: &str, at: Option<&str>) {
        if let Err(e) = self.funnel_emit(r, step, source, actor, at).await {
            eprintln!("filing-funnel hook skipped ({step}/{source}): {}", e.1);
        }
    }
}

fn step_label(step: &str) -> &'static str {
    match step {
        "intake_started" => "Intake started",
        "documents_uploaded" => "Documents uploaded",
        "interview_done" => "Interview done",
        "fee_paid" => "Fee paid",
        "desk_review" => "In review at Filing Desk",
        "filed" => "Filed",
        _ => step,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tax::models::*;
    use crate::tax::filing_desk::FeeFlag;
    use serde_json::json;

    fn ret(status: &str, created: &str, updated: &str, inputs: Value) -> TaxReturn {
        TaxReturn {
            id: "tax_1".into(), program: "T1".into(), tax_year: 2025, jurisdiction: "AB".into(),
            name: "Test".into(), client_ref: "c1".into(), period_start: None, period_end: None,
            inputs, parties: vec![], corporate: None, notes: String::new(), revision: 1, input_revision: 1,
            status: status.into(), calculation: None, calculations: vec![], review: None, reviews: vec![],
            rollforward_from: None, created_at: created.into(), updated_at: updated.into(),
            parallel_comparisons: vec![], prior_lines: vec![], preparer_signoff: None, filed_record: None,
        }
    }

    #[test]
    fn documents_detected_from_slip_inputs() {
        assert!(!has_documents(&json!({})));
        assert!(!has_documents(&json!({"scope_confirmed": true})));
        assert!(has_documents(&json!({"employment_income_cents": 100})));
        assert!(has_documents(&json!({"t4_slips": [{"box14": 1}]})));
    }

    #[test]
    fn derive_intake_only() {
        let r = ret("draft", "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z", json!({}));
        let d = derive_steps(&r, &DeskState::default());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "intake_started");
        assert_eq!(current_step(&d).unwrap().0, "intake_started");
    }

    #[test]
    fn derive_through_filed_and_stall() {
        let mut r = ret("filed", "2026-01-01T00:00:00Z", "2026-02-01T00:00:00Z",
            json!({"employment_income_cents": 5_000_000, "scope_confirmed": true}));
        r.calculation = Some(CalculationSnapshot {
            id: "c1".into(), input_revision: 1, request_revision: 1, idempotency_key: None,
            input_document: None, input_hash: "h".into(), output_hash: "o".into(),
            created_at: "2026-01-10T00:00:00Z".into(), result: json!({"supported": true}),
        });
        r.review = Some(Review {
            at: "2026-01-15T00:00:00Z".into(), reviewer: "Rev [usr_2]".into(), note: "ok".into(),
            input_hash: "h".into(), output_hash: "o".into(),
        });
        r.filed_record = Some(FiledRecord {
            by: "Filer [usr_3]".into(), at: "2026-01-20T00:00:00Z".into(),
            reference: "CRA-1".into(), t183_envelope_id: "env_1".into(),
        });
        let mut desk = DeskState::default();
        desk.fees.push(FeeFlag {
            return_id: "tax_1".into(), paid: true, by: "stripe".into(),
            at: "2026-01-12T00:00:00Z".into(), stripe_session_id: None,
        });
        let d = derive_steps(&r, &desk);
        let names: Vec<_> = d.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(names, STEPS);
        assert_eq!(current_step(&d).unwrap().0, "filed");
        let days = stall_days("2026-01-01T00:00:00Z", parse_at("2026-01-10T00:00:00Z").unwrap()).unwrap();
        assert_eq!(days, 9);
        assert!(is_stalled("intake_started", 9, &default_thresholds()));
        assert!(!is_stalled("filed", 100, &default_thresholds()));
    }

    #[test]
    fn step_order_is_stable() {
        assert_eq!(STEPS.len(), 6);
        assert_eq!(step_index("fee_paid"), Some(3));
        assert_eq!(step_index("desk_review"), Some(4));
    }
}
