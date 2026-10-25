//! NXT OS: an AI-native business operating system. Hosts NXT Ledger
//! (accounting) and NXT Referrals (referral + loyalty), with the "Ask NXT"
//! assistant across both, served as one Rust function on Vercel.

pub mod ledger;
pub mod os;
pub mod referral;
pub mod store;

use axum::Router;
use std::sync::Arc;
use store::Store;

pub fn app(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>) -> Router {
    Router::new()
        .merge(ledger::routes::router(ledger.clone()))
        .merge(os::router(os::Os { ledger, referrals: referrals.clone() }))
        .merge(referral::routes::app(referrals))
}

/// Builds the app with storage chosen per-module by `make_store`.
pub fn app_with(make_store: impl Fn() -> Store) -> Router {
    app(Arc::new(ledger::engine::Ledger::new(make_store())), Arc::new(referral::engine::Engine::new(make_store())))
}
