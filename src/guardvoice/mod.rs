//! GuardVoice: phone assistant for one NXT OS workspace.
//!
//! Voice (Twilio ConversationRelay, or speech `<Gather>` when there is no
//! long-lived WebSocket), intelligence (tools over Ledger and Referrals,
//! with permission levels), and a human gate (summaries, handoffs, SMS and
//! in-app escalation, approval before consequential actions, activity log).
//!
//! Secrets come from the environment only. With no Twilio credentials the
//! process stays in mock mode and never dials or sends a real SMS.

mod brain;
mod config;
mod models;
mod routes;
mod session;
mod tools;
mod voice;

pub use config::{Autonomy, BrainKind, Config, Mode, SignatureMode, Transport};
pub use routes::router;
pub use voice::twilio_signature;

use crate::ledger::engine::Ledger;
use crate::referral::engine::Engine as Referrals;
use crate::store::Store;
use std::sync::Arc;

pub struct GuardVoice {
    pub ledger: Arc<Ledger>,
    pub referrals: Arc<Referrals>,
    pub store: Store,
    pub config: Config,
}

impl GuardVoice {
    pub fn new(ledger: Arc<Ledger>, referrals: Arc<Referrals>, store: Store, config: Config) -> Self {
        GuardVoice { ledger, referrals, store, config }
    }
}
