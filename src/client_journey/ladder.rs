//! Value ladder: tiers with entry criteria, deliverables, placeholder pricing
//! (pricing TBD by Orville) and the trigger signals that recommend the next
//! rung. The journey surfaces next-rung offers at offer points; every offer is
//! drafted automatically but must be approved by a human before it is sent,
//! and sending is a stub. Offer events go to an append-only offer ledger.

use super::{Actor, ClientJourney, JourneyDef};
use serde::{Deserialize, Serialize};

pub const LADDER_JSON: &str = include_str!("ladder.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = ">=")]
    Gte,
    #[serde(rename = "<=")]
    Lte,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub signal: String,
    pub op: Op,
    pub threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tier {
    pub id: String,
    pub rung: u32,
    pub name: String,
    pub entry_criteria: String,
    pub deliverables: Vec<String>,
    pub pricing_model: String,
    pub pricing_tbd: bool,
    /// Signals that recommend moving a client up to THIS rung (any one fires).
    pub next_rung_triggers: Vec<Trigger>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ladder {
    pub ladder_id: String,
    pub version: u32,
    pub pricing_note: String,
    pub tiers: Vec<Tier>,
}

/// Client signals the firm measures (all optional; missing = not fired).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientSignals {
    pub monthly_transactions: Option<f64>,
    pub employees: Option<f64>,
    pub reconciliation_exceptions: Option<f64>,
    pub months_behind: Option<f64>,
    pub revenue_growth_pct: Option<f64>,
    pub annual_revenue: Option<f64>,
    pub months_to_year_end: Option<f64>,
    pub exit_horizon_years: Option<f64>,
}

impl ClientSignals {
    pub fn get(&self, name: &str) -> Option<f64> {
        match name {
            "monthly_transactions" => self.monthly_transactions,
            "employees" => self.employees,
            "reconciliation_exceptions" => self.reconciliation_exceptions,
            "months_behind" => self.months_behind,
            "revenue_growth_pct" => self.revenue_growth_pct,
            "annual_revenue" => self.annual_revenue,
            "months_to_year_end" => self.months_to_year_end,
            "exit_horizon_years" => self.exit_horizon_years,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recommendation {
    pub tier: String,
    pub reasons: Vec<String>,
}

impl Ladder {
    pub fn load() -> Result<Self, String> {
        let l: Ladder = serde_json::from_str(LADDER_JSON).map_err(|e| e.to_string())?;
        l.validate()?;
        Ok(l)
    }
    pub fn validate(&self) -> Result<(), String> {
        for (i, t) in self.tiers.iter().enumerate() {
            if t.rung as usize != i + 1 {
                return Err(format!("tier {} out of order", t.id));
            }
            if !t.pricing_tbd || !t.pricing_model.contains("TBD by Orville") {
                return Err(format!("tier {} pricing must stay marked TBD by Orville", t.id));
            }
            for tr in &t.next_rung_triggers {
                if !KNOWN_SIGNALS.contains(&tr.signal.as_str()) {
                    return Err(format!("tier {} uses unknown signal {}", t.id, tr.signal));
                }
            }
        }
        Ok(())
    }
    pub fn tier(&self, id: &str) -> Option<&Tier> {
        self.tiers.iter().find(|t| t.id == id)
    }
    /// Lowest rung above `current` whose triggers fire for these signals.
    pub fn recommend_next(&self, current: &str, signals: &ClientSignals) -> Option<Recommendation> {
        let cur = self.tier(current)?.rung;
        self.tiers.iter().filter(|t| t.rung > cur).find_map(|t| {
            let reasons: Vec<String> = t
                .next_rung_triggers
                .iter()
                .filter(|tr| match (signals.get(&tr.signal), tr.op) {
                    (Some(v), Op::Gte) => v >= tr.threshold,
                    (Some(v), Op::Lte) => v <= tr.threshold,
                    _ => false,
                })
                .map(|tr| format!("{} {} {}", tr.signal, if tr.op == Op::Gte { ">=" } else { "<=" }, tr.threshold))
                .collect();
            (!reasons.is_empty()).then(|| Recommendation { tier: t.id.clone(), reasons })
        })
    }
}

const KNOWN_SIGNALS: &[&str] = &[
    "monthly_transactions", "employees", "reconciliation_exceptions", "months_behind",
    "revenue_growth_pct", "annual_revenue", "months_to_year_end", "exit_horizon_years",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferStatus {
    Drafted,
    Approved,
    Declined,
    Sent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferEvent {
    pub seq: u64,
    pub offer_id: u64,
    pub step: String,
    pub tier: String,
    pub status: OfferStatus,
    pub actor: Actor,
    pub at: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfferError {
    NotAnOfferPoint(String),
    StepNotReached(String),
    NoRecommendation,
    UnknownOffer(u64),
    HumanApprovalRequired(u64),
    InvalidTransition { offer_id: u64, from: OfferStatus, to: OfferStatus },
}

/// Sends an approved offer to the client. Stub only.
pub trait OfferSender {
    fn send_offer(&self, client_id: &str, tier: &Tier) -> super::HookOutcome;
}
impl OfferSender for super::StubHooks {
    fn send_offer(&self, client_id: &str, tier: &Tier) -> super::HookOutcome {
        super::HookOutcome::StubOnly { would_do: format!("send {} offer to {client_id} (pricing TBD by Orville)", tier.name) }
    }
}

impl ClientJourney {
    pub fn offer_status(&self, offer_id: u64) -> Option<OfferStatus> {
        self.offers.iter().rev().find(|e| e.offer_id == offer_id).map(|e| e.status)
    }

    /// At an offer point the system drafts the next-rung offer. Draft only.
    pub fn draft_offer(&mut self, def: &JourneyDef, ladder: &Ladder, step: &str, signals: &ClientSignals, actor: Actor, at: &str) -> Result<u64, OfferError> {
        let s = def.step(step).ok_or_else(|| OfferError::NotAnOfferPoint(step.into()))?;
        let after = s.offer_point_after_tier.as_deref().ok_or_else(|| OfferError::NotAnOfferPoint(step.into()))?;
        if self.status_of(step) == Some(super::StepStatus::Pending) {
            return Err(OfferError::StepNotReached(step.into()));
        }
        let rec = ladder.recommend_next(after, signals).ok_or(OfferError::NoRecommendation)?;
        let offer_id = self.offers.iter().map(|e| e.offer_id).max().unwrap_or(0) + 1;
        self.push_offer(offer_id, step, &rec.tier, OfferStatus::Drafted, actor, at, &rec.reasons.join("; "));
        Ok(offer_id)
    }

    pub fn approve_offer(&mut self, offer_id: u64, approver: Actor, at: &str, note: &str) -> Result<(), OfferError> {
        self.offer_move(offer_id, OfferStatus::Drafted, OfferStatus::Approved, approver, at, note)
    }
    pub fn decline_offer(&mut self, offer_id: u64, approver: Actor, at: &str, note: &str) -> Result<(), OfferError> {
        self.offer_move(offer_id, OfferStatus::Drafted, OfferStatus::Declined, approver, at, note)
    }
    /// Only an Approved offer can be sent; the sender is a stub.
    pub fn send_offer(&mut self, offer_id: u64, ladder: &Ladder, sender: &dyn OfferSender, actor: Actor, at: &str) -> Result<super::HookOutcome, OfferError> {
        let tier_id = self.offers.iter().find(|e| e.offer_id == offer_id).map(|e| e.tier.clone()).ok_or(OfferError::UnknownOffer(offer_id))?;
        self.offer_move(offer_id, OfferStatus::Approved, OfferStatus::Sent, actor, at, "sent (stub)")?;
        let tier = ladder.tier(&tier_id).ok_or(OfferError::UnknownOffer(offer_id))?;
        Ok(sender.send_offer(&self.client_id, tier))
    }

    fn offer_move(&mut self, offer_id: u64, need: OfferStatus, to: OfferStatus, actor: Actor, at: &str, note: &str) -> Result<(), OfferError> {
        let from = self.offer_status(offer_id).ok_or(OfferError::UnknownOffer(offer_id))?;
        if from != need {
            if from == OfferStatus::Drafted && to == OfferStatus::Sent {
                return Err(OfferError::HumanApprovalRequired(offer_id));
            }
            return Err(OfferError::InvalidTransition { offer_id, from, to });
        }
        if matches!(to, OfferStatus::Approved | OfferStatus::Declined) && !matches!(actor, Actor::Human { .. }) {
            return Err(OfferError::HumanApprovalRequired(offer_id));
        }
        let first = self.offers.iter().find(|e| e.offer_id == offer_id).unwrap().clone();
        self.push_offer(offer_id, &first.step, &first.tier, to, actor, at, note);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_offer(&mut self, offer_id: u64, step: &str, tier: &str, status: OfferStatus, actor: Actor, at: &str, note: &str) {
        let seq = self.offers.len() as u64 + 1;
        self.offers.push(OfferEvent { seq, offer_id, step: step.into(), tier: tier.into(), status, actor, at: at.into(), note: note.into() });
    }
}
