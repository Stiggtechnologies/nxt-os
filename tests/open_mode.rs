use axum::{body::Body, http::Request};
use nxt_os::store::Store;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(router: &axum::Router, method: &str, uri: &str, key: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(key) = key { request = request.header("authorization", key); }
    if method == "POST" || method == "PUT" { request = request.header("content-type", "application/json"); }
    router.clone().oneshot(request.body(if method == "POST" || method == "PUT" { Body::from("{}") } else { Body::empty() }).unwrap()).await.unwrap()
}

/// One sequential test owns environment mutation in this executable. Other
/// integration suites run in distinct processes with explicitly configured keys.
#[tokio::test]
async fn missing_workspace_key_fails_closed_across_modules() {
    // Keep the engine Arcs so the test can inspect the exact stores served by
    // the routers without adding Clone or test-only APIs to production Store.
    let ledger = Arc::new(nxt_os::ledger::engine::Ledger::new(Store::memory()));
    let referrals = Arc::new(nxt_os::referral::engine::Engine::new(Store::memory()));
    let firm = Arc::new(nxt_os::firm::Firm::new(Store::memory(), ledger.clone()));
    let tax = Arc::new(nxt_os::tax::Tax::new(Store::memory()));
    let engagements = Arc::new(nxt_os::engagement::Engagements::new(Store::memory(), ledger.clone(), tax.clone(), firm.clone()));
    let signatures = Arc::new(nxt_os::signatures::Signatures::new(Store::memory()));
    let router = axum::Router::new()
        .merge(nxt_os::ledger::routes::router(ledger.clone()))
        .merge(nxt_os::firm::router(firm.clone()))
        .merge(nxt_os::tax::router(tax.clone()))
        .merge(nxt_os::engagement::router(engagements.clone()))
        .merge(nxt_os::migrations::router(Arc::new(nxt_os::migrations::Migrations::new(Store::memory()))))
        .merge(nxt_os::signatures::router(signatures.clone()))
        .merge(nxt_os::nixon_finance::routes::router(ledger.clone()))
        .merge(nxt_os::os::router(nxt_os::os::Os {
            ledger: ledger.clone(), referrals: referrals.clone(), firm: firm.clone(), tax: tax.clone(), engagements: engagements.clone(),
        }))
        .merge(nxt_os::referral::routes::app(referrals));
    for configured in [None, Some(""), Some(" \t ")] {
        unsafe {
            match configured {
                Some(key) => std::env::set_var("ADMIN_API_KEY", key),
                None => std::env::remove_var("ADMIN_API_KEY"),
            }
        }
        assert!(nxt_os::referral::routes::auth_required());
        for (method, uri) in [
            ("GET", "/api/os/status"),
            ("GET", "/api/os/insights"),
            ("GET", "/api/ledger/accounts"),
            ("GET", "/api/admin/settings"),
            ("GET", "/api/firm/summary"),
            ("GET", "/api/tax/summary"),
            ("GET", "/api/engagements/summary"),
            ("GET", "/api/migrations"),
            ("GET", "/api/migrations/unavailable/archive"),
            ("GET", "/api/nixon/status"),
            ("GET", "/api/nixon/reviews"),
            ("POST", "/api/nixon/reviews"),
            ("POST", "/api/nixon/actions"),
            ("GET", "/api/signatures"),
            ("GET", "/api/signatures/catalog"),
            ("GET", "/api/signatures/audit?path=signatures%2Faudit&offset=0&limit=1"),
            ("GET", "/api/signatures/unavailable"),
            ("GET", "/api/signatures/unavailable/package"),
            ("GET", "/api/signatures/templates/unavailable"),
            ("GET", "/api/firm/exports/unavailable/download"),
            ("GET", "/api/tax/returns/unavailable/package"),
            ("GET", "/api/engagements/unavailable/package"),
            ("POST", "/api/os/ask"),
            ("POST", "/api/os/sample-data"),
            ("POST", "/api/ledger/entries"),
            ("POST", "/api/track/purchase"),
            ("POST", "/api/firm/proposals"),
            ("POST", "/api/firm/workflows/tick"),
            ("POST", "/api/tax/returns"),
            ("POST", "/api/engagements"),
            ("POST", "/api/migrations/chunks"),
            ("POST", "/api/migrations/snapshots"),
            ("POST", "/api/signatures"),
            ("PUT", "/api/signatures/unavailable"),
            ("POST", "/api/signatures/unavailable/transition"),
            ("POST", "/api/signatures/unavailable/template"),
            ("POST", "/api/signatures/templates/unavailable/instantiate"),
        ] {
            for key in [None, Some("Bearer guessed-key")] {
                let response = call(&router, method, uri, key).await;
                assert_eq!(response.status().as_u16(), 503, "{configured:?}: {method} {uri}");
                assert_eq!(response.headers()["cache-control"], "private, no-store");
                assert_eq!(response.headers()["x-content-type-options"], "nosniff");
                let bytes = axum::body::to_bytes(response.into_body(), 2048).await.unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body, json!({"error": "Workspace sign-in is unavailable. Contact the administrator."}));
            }
        }
        // Public liveness and merchant offers expose no protected workspace state.
        assert_eq!(call(&router, "GET", "/api/health", None).await.status().as_u16(), 200);
        assert_eq!(call(&router, "GET", "/api/public/offer", None).await.status().as_u16(), 200);
        assert!(ledger.store.get("gl:seeded").await.unwrap().is_none(), "rejected account/sample-data requests must not initialize Ledger");
        assert!(firm.store.get("firm:state:v1").await.unwrap().is_none());
        assert!(tax.store.get("tax:state:v1").await.unwrap().is_none());
        assert!(engagements.store.get("engagements:state:v1").await.unwrap().is_none());
        assert!(signatures.store.get("signatures:state:v1").await.unwrap().is_none());
    }

    unsafe { std::env::set_var("ADMIN_API_KEY", "configured-test-key") };
    for supplied in [None, Some("Bearer wrong-key"), Some("Basic configured-test-key")] {
        for (method, uri) in [
            ("GET", "/api/os/status"),
            ("GET", "/api/nixon/status"),
            ("POST", "/api/nixon/reviews"),
            ("GET", "/api/signatures"),
            ("GET", "/api/signatures/catalog"),
            ("GET", "/api/signatures/audit"),
            ("GET", "/api/signatures/unavailable"),
            ("GET", "/api/signatures/unavailable/package"),
            ("GET", "/api/signatures/templates/unavailable"),
            ("POST", "/api/signatures"),
            ("PUT", "/api/signatures/unavailable"),
            ("POST", "/api/signatures/unavailable/transition"),
            ("POST", "/api/signatures/unavailable/template"),
            ("POST", "/api/signatures/templates/unavailable/instantiate"),
        ] {
            let response = call(&router, method, uri, supplied).await;
            assert_eq!(response.status().as_u16(), 401, "{method} {uri}");
            assert_eq!(response.headers()["cache-control"], "private, no-store");
        }
    }
    assert!(signatures.store.get("signatures:state:v1").await.unwrap().is_none());
    let response = call(&router, "GET", "/api/os/status", Some("Bearer configured-test-key")).await;
    assert_eq!(response.status().as_u16(), 200);
    let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
    let status: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status["auth_required"], true);
    assert_eq!(status["apps"].as_array().unwrap().len(), 8);
    assert!(status["apps"].as_array().unwrap().iter().any(|app| app["id"] == "signatures" && app["path"] == "/apps/signatures/"));
    assert!(status["apps"].as_array().unwrap().iter().any(|app| app["id"] == "nixon" && app["path"] == "/apps/nixon/"));
    unsafe { std::env::remove_var("ADMIN_API_KEY") };
    assert_eq!(call(&router, "GET", "/api/os/status", Some("Bearer configured-test-key")).await.status().as_u16(), 503);
}
