//! Synthetic acceptance: the client directory joins modules and honours staff client scope.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.3");
    if !tok.is_empty() { q = q.header("authorization", format!("Bearer {tok}")); }
    let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
async fn ok(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, m, uri, tok, body).await;
    assert_eq!(s, 200, "{m} {uri}: {v}");
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn directory_lists_every_module_and_filters_by_assignment() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);

    assert_eq!(call(&r, "GET", "/api/clients", "", None).await.0, 401);
    let empty = ok(&r, "GET", "/api/clients", "k", None).await;
    assert_eq!(empty["clients"], json!([]));
    assert_eq!(empty["live_cutover_allowed"], false);
    assert!(empty["not_loaded"].as_array().unwrap().iter().any(|n| n["record"].as_str().unwrap().contains("DT Max client list")));

    ok(&r, "POST", "/api/company-books/roster", "k", Some(json!({"expected_head":null,"capture":{"format":"nxt-source-company-roster/v1","source_system":"synthetic",
        "source_capture_sha256":"a".repeat(64),"observed_at":"2026-10-27T00:00:00Z","companies":[
        {"company_id":"client-a","company_name":"Synthetic A","source_navigation_ref":"row-1"},
        {"company_id":"client-b","company_name":"Synthetic B","source_navigation_ref":"row-2"}]}}))).await;
    ok(&r, "POST", "/api/firm/projects", "k", Some(json!({"name":"Year-end","client_ref":"CLIENT-A","tasks":[]}))).await;
    ok(&r, "POST", "/api/firm/projects", "k", Some(json!({"name":"Year-end","client_ref":"client-b","tasks":[]}))).await;
    ok(&r, "POST", "/api/firm/closes", "k", Some(json!({"name":"Close","period_start":"2025-12-01","period_end":"2025-12-31","client_ref":"client-c"}))).await;

    let all = ok(&r, "GET", "/api/clients", "k", None).await;
    let rows = all["clients"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "{all}");
    let a = rows.iter().find(|c| c["name"] == "Synthetic A").unwrap();
    assert_eq!(a["in_source_roster"], true);
    assert_eq!(a["projects"], 1, "case differs between modules but the row is one client");
    assert_eq!(rows.iter().find(|c| c["client_ref"] == "client-c").unwrap()["in_source_roster"], false);

    ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"scoped","display_name":"Sam Scoped","role":"preparer","clients":["client-a"],"temporary_password":"synthetic-pass-phrase-1"}))).await;
    let tok = ok(&r, "POST", "/api/staff/login", "", Some(json!({"username":"scoped","password":"synthetic-pass-phrase-1"}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(&r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(&r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(&r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":"synthetic-pass-phrase-1","new_password":"synthetic-pass-phrase-2"}))).await;

    let mine = ok(&r, "GET", "/api/clients", &tok, None).await;
    assert_eq!(mine["clients"].as_array().unwrap().len(), 1, "{mine}");
    assert_eq!(mine["scoped"], true);
    assert_eq!(mine["clients"][0]["name"], "Synthetic A");
    let detail = ok(&r, "GET", "/api/clients/client-a", &tok, None).await;
    assert_eq!(detail["client"]["projects"], 1);
    assert_eq!(detail["firm_work"]["projects"].as_array().unwrap().len(), 1);
    assert_eq!(call(&r, "GET", "/api/clients/client-b", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/clients/no-such", "k", None).await.0, 404);
}
