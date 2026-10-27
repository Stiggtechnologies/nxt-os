//! NXT OS: an AI-native business operating system. Hosts NXT Ledger
//! (accounting), NXT Referrals (referral + loyalty), GuardVoice (the phone
//! line), and Nixon (the assistant that knows the business and takes actions).
//! Ask NXT answers questions and stays read-only. Served as one Rust function
//! on Vercel.

pub mod guardvoice;
pub mod ledger;
pub mod nixon;
pub mod os;
pub mod referral;
pub mod store;

use axum::Router;
use std::sync::Arc;
use store::Store;

pub fn app(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>) -> Router {
    app_parts(ledger, referrals, guardvoice::Config::from_env(), Store::from_env)
}

/// Builds the app with storage chosen per-module by `make_store`.
pub fn app_with(make_store: impl Fn() -> Store) -> Router {
    app_with_voice(make_store, guardvoice::Config::from_env())
}

/// Same as `app_with`, with an explicit GuardVoice config (tests, mock mode).
pub fn app_with_voice(make_store: impl Fn() -> Store, voice: guardvoice::Config) -> Router {
    app_parts(
        Arc::new(ledger::engine::Ledger::new(make_store())),
        Arc::new(referral::engine::Engine::new(make_store())),
        voice,
        make_store,
    )
}

fn app_parts(
    ledger: Arc<ledger::engine::Ledger>,
    referrals: Arc<referral::engine::Engine>,
    voice: guardvoice::Config,
    make_store: impl Fn() -> Store,
) -> Router {
    let gv = Arc::new(guardvoice::GuardVoice::new(ledger.clone(), referrals.clone(), make_store(), voice));
    Router::new()
        .merge(ledger::routes::router(ledger.clone()))
        .merge(os::router(os::Os { ledger, referrals: referrals.clone() }))
        .merge(guardvoice::router(gv))
        .merge(referral::routes::app(referrals))
}
