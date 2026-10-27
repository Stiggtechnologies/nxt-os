//! GuardVoice: the phone layer for one NXT OS workspace.
//!
//! Twilio ConversationRelay, or speech `<Gather>` when there is no long-lived
//! WebSocket. Nixon (`crate::nixon`) is the assistant that knows the business
//! and takes actions. This module keeps the call, the handoff, the owner SMS,
//! and the approval record.
//!
//! Secrets come from the environment only. With no Twilio credentials the
//! process stays in mock mode and never dials or sends a real SMS.

mod config;
pub(crate) mod models;
mod routes;
mod session;
pub(crate) mod voice;

pub use config::{Autonomy, Config, Mode, SignatureMode, Transport};
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
