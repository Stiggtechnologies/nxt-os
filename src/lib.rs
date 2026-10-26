//! NXT OS: an AI-native business operating system. Hosts NXT Ledger
//! (accounting) and NXT Referrals (referral + loyalty), with the "Ask NXT"
//! assistant across both, served as one Rust function on Vercel.

pub mod ledger;
pub mod firm;
pub mod os;
pub mod referral;
pub mod store;
pub mod tax;
pub mod engagement;

use axum::Router;
use std::sync::Arc;
use store::Store;

pub fn app(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>) -> Router {
    let firm = Arc::new(firm::Firm::new(Store::from_env(), ledger.clone()));
    app_modules(ledger, referrals, firm)
}

fn app_modules(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, firm: Arc<firm::Firm>) -> Router {
    let tax = Arc::new(tax::Tax::new(Store::from_env()));
    app_modules_with_tax(ledger, referrals, firm, tax)
}

fn app_modules_with_tax(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, firm: Arc<firm::Firm>, tax: Arc<tax::Tax>) -> Router {
    let engagements = Arc::new(engagement::Engagements::new(Store::from_env(),ledger.clone(),tax.clone(),firm.clone()));
    app_all(ledger,referrals,firm,tax,engagements)
}

fn app_all(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, firm: Arc<firm::Firm>, tax: Arc<tax::Tax>, engagements:Arc<engagement::Engagements>) -> Router {
    Router::new()
        .merge(ledger::routes::router(ledger.clone()))
        .merge(firm::router(firm.clone()))
        .merge(tax::router(tax.clone()))
        .merge(engagement::router(engagements.clone()))
        .merge(os::router(os::Os { ledger, firm, tax, engagements, referrals: referrals.clone() }))
        .merge(referral::routes::app(referrals))
}

/// Builds the app with storage chosen per-module by `make_store`.
pub fn app_with(make_store: impl Fn() -> Store) -> Router {
    let ledger = Arc::new(ledger::engine::Ledger::new(make_store()));
    let referrals = Arc::new(referral::engine::Engine::new(make_store()));
    let firm = Arc::new(firm::Firm::new(make_store(), ledger.clone()));
    let tax = Arc::new(tax::Tax::new(make_store()));
    let engagements=Arc::new(engagement::Engagements::new(make_store(),ledger.clone(),tax.clone(),firm.clone()));
    app_all(ledger, referrals, firm, tax, engagements)
}
