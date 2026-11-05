//! NXT OS: an AI-native operating system for running an accounting firm,
//! served as one Rust function on Vercel. The client journey
//! (`client_journey`) is the spine: each step is done by a module capability
//! (`client_journey::capability`) or recorded by staff, and human gates stay human.

pub mod agent;
pub mod guardvoice;
pub mod ledger;
pub mod nixon;
pub mod firm;
pub mod nixon_finance;
pub mod os;
pub mod referral;
pub mod store;
pub mod tax;
pub mod engagement;
pub mod migrations;
pub mod signatures;
pub mod company_books;
pub mod source_import;
pub mod import_parsers;
pub mod client_journey;
pub mod client_directory;
pub mod client_alias;

use axum::Router;
use std::sync::Arc;
use store::Store;

pub fn app(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>) -> Router {
    let voice = voice_router(ledger.clone(), referrals.clone(), guardvoice::Config::from_env(), Store::from_env());
    let firm = Arc::new(firm::Firm::new(Store::from_env(), ledger.clone()));
    app_modules(ledger, referrals, firm).merge(voice)
}

fn voice_router(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, voice: guardvoice::Config, store: Store) -> Router {
    guardvoice::router(Arc::new(guardvoice::GuardVoice::new(ledger, referrals, store, voice)))
}

fn app_modules(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, firm: Arc<firm::Firm>) -> Router {
    let tax = Arc::new(tax::Tax::new(Store::from_env()));
    app_modules_with_tax(ledger, referrals, firm, tax)
}

fn app_modules_with_tax(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, firm: Arc<firm::Firm>, tax: Arc<tax::Tax>) -> Router {
    let engagements = Arc::new(engagement::Engagements::new(Store::from_env(),ledger.clone(),tax.clone(),firm.clone()));
    let migrations = Arc::new(migrations::Migrations::new(Store::from_env()));
    let signatures = Arc::new(signatures::Signatures::new(Store::from_env()));
    let company_books = Arc::new(company_books::CompanyBooks::new(Store::from_env()));
    let source_lists = Arc::new(company_books::source_lists::SourceLists::new(Store::from_env(), migrations.clone()));
    let source_import = Arc::new(source_import::SourceImport::new(Store::from_env()));
    app_all(ledger,referrals,firm,tax,engagements,migrations,signatures,company_books,source_lists,source_import,Store::from_env())
}

fn app_all(ledger: Arc<ledger::engine::Ledger>, referrals: Arc<referral::engine::Engine>, firm: Arc<firm::Firm>, tax: Arc<tax::Tax>, engagements:Arc<engagement::Engagements>, migrations:Arc<migrations::Migrations>, signatures:Arc<signatures::Signatures>, company_books:Arc<company_books::CompanyBooks>, source_lists:Arc<company_books::source_lists::SourceLists>, source_import: Arc<source_import::SourceImport>, journey_store: Store) -> Router {
    tax.require_signed_t183_from(signatures.clone());
    let capabilities = client_journey::capability::Capabilities::new()
        .with(Arc::new(client_journey::sources::SourceSoftwareImport {
            company_books: company_books.clone(),
            migrations: migrations.clone(),
            source_import: source_import.clone(),
        }))
        .with(Arc::new(client_journey::ledger::LedgerClose::new(company_books.clone())));
    let journeys = Arc::new(client_journey::service::Journeys::new(journey_store, capabilities));
    let backup = agent::openai::OpenAi::from_env().map(|m| Arc::new(m) as Arc<dyn agent::Model>);
    let agent = agent::Claude::from_env().map(|m| agent::Agent::new(journeys.clone(), Arc::new(m)).with_backup(backup));
    let directory = Arc::new(client_directory::Directory {
        company_books: company_books.clone(),
        migrations: migrations.clone(),
        tax: tax.clone(),
        engagements: engagements.clone(),
        signatures: signatures.clone(),
        firm: firm.clone(),
    });
    let resolver = Arc::new(client_alias::Resolver::new(directory.clone(), journeys.clone(), ledger.store.clone()));
    let inner = Router::new()
        .merge(ledger::routes::router(ledger.clone()))
        .merge(ledger::routes::company_router(ledger.store.clone(), company_books.clone()))
        .merge(firm::router(firm.clone()))
        .merge(tax::router(tax.clone()))
        .merge(engagement::router(engagements.clone()))
        .merge(migrations::router(migrations.clone()))
        .merge(signatures::router(signatures.clone()))
        .merge(company_books::router(company_books.clone()))
        .merge(company_books::source_lists::router(source_lists))
        .merge(client_directory::router(directory.clone()))
        .merge(crate::nixon_finance::routes::router(crate::nixon_finance::voice::Scope {
            ledger: ledger.clone(),
            firm: firm.clone(),
            tax: tax.clone(),
            engagements: engagements.clone(),
            signatures: signatures.clone(),
            migrations: migrations.clone(),
            company_books: company_books.clone(),
            referrals: referrals.clone(),
            journeys: journeys.clone(),
        }))
        .merge(tax::stripe::router(std::sync::Arc::new(tax::stripe::StripePay::new(tax.clone(), ledger.clone()))))
        .merge(os::router(os::Os { ledger, firm, tax, engagements, referrals: referrals.clone() }))
        .merge(referral::routes::app(referrals))
        .merge(source_import::router(source_import))
        .merge(client_journey::router(journeys, agent));
    // Company alias resolution runs before routing, so a moved path reaches its own handler.
    Router::new().fallback_service(inner).layer(axum::middleware::from_fn_with_state(resolver, client_alias::rewrite))
}

/// Builds the app with storage chosen per-module by `make_store`.
pub fn app_with(make_store: impl Fn() -> Store) -> Router {
    app_with_voice(make_store, guardvoice::Config::from_env())
}

/// Same as `app_with`, with an explicit GuardVoice config (tests, mock mode).
pub fn app_with_voice(make_store: impl Fn() -> Store, voice: guardvoice::Config) -> Router {
    let ledger = Arc::new(ledger::engine::Ledger::new(make_store()));
    let referrals = Arc::new(referral::engine::Engine::new(make_store()));
    let firm = Arc::new(firm::Firm::new(make_store(), ledger.clone()));
    let tax = Arc::new(tax::Tax::new(make_store()));
    let engagements=Arc::new(engagement::Engagements::new(make_store(),ledger.clone(),tax.clone(),firm.clone()));
    let migrations = Arc::new(migrations::Migrations::new(make_store()));
    let signatures = Arc::new(signatures::Signatures::new(make_store()));
    let company_books = Arc::new(company_books::CompanyBooks::new(make_store()));
    let source_lists = Arc::new(company_books::source_lists::SourceLists::new(make_store(), migrations.clone()));
    let source_import = Arc::new(source_import::SourceImport::new(make_store()));
    let voice = voice_router(ledger.clone(), referrals.clone(), voice, make_store());
    app_all(ledger, referrals, firm, tax, engagements,migrations,signatures,company_books,source_lists,source_import,make_store()).merge(voice)
}
