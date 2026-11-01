//! Synthetic acceptance: per-client Firm work view, client-labelled closes, scoping on both,
//! and the gated hourly workflow-trigger cron. No real clients.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.1");
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
async fn client_work_view_closes_scoping_and_gated_cron() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::remove_var("CRON_SECRET");
        std::env::remove_var("NXT_FIRM_CRON");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);

    ok(&r, "POST", "/api/firm/projects", "k", Some(json!({"name":"Year-end A","client_ref":"client-a","tasks":[
        {"title":"Collect slips","due_date":"2026-01-10"},{"title":"Prepare T1","due_date":"2026-04-01"}]}))).await;
    ok(&r, "POST", "/api/firm/projects", "k", Some(json!({"name":"Year-end B","client_ref":"client-b","tasks":[{"title":"Collect slips","due_date":"2026-01-10"}]}))).await;
    let c = ok(&r, "POST", "/api/firm/closes", "k", Some(json!({"name":"Dec close A","period_start":"2025-12-01","period_end":"2025-12-31","due_date":"2026-01-15","client_ref":"client-a"}))).await;
    assert_eq!(c["client_ref"], "client-a");
    ok(&r, "POST", "/api/firm/closes", "k", Some(json!({"name":"Dec close B","period_start":"2025-12-01","period_end":"2025-12-31","client_ref":"client-b"}))).await;

    let w = ok(&r, "GET", "/api/firm/clients/client-a/work?as_of=2026-02-01", "k", None).await;
    assert_eq!(w["projects"].as_array().unwrap().len(), 1, "{w}");
    assert_eq!(w["closes"].as_array().unwrap().len(), 1);
    assert_eq!(w["projects"][0]["overdue_tasks"], 1);
    assert_eq!(w["projects"][0]["next_due"], "2026-01-10");
    assert_eq!(w["totals"]["open_tasks"], 5); // 2 project tasks + 3 default close tasks
    assert_eq!(w["totals"]["overdue_tasks"], 4); // 1 project task + 3 close tasks due 2026-01-15
    assert_eq!(w["source"], "nxt_parallel_not_quickbooks");
    assert_eq!(call(&r, "GET", "/api/firm/clients/client-a/work?as_of=bad", "k", None).await.0, 400);

    // Scoped account: own client's work and closes only.
    ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"scoped","display_name":"Sam Scoped","role":"preparer","clients":["client-a"],"temporary_password":"synthetic-pass-phrase-1"}))).await;
    let tok = ok(&r, "POST", "/api/staff/login", "", Some(json!({"username":"scoped","password":"synthetic-pass-phrase-1"}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(&r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(&r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(&r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":"synthetic-pass-phrase-1","new_password":"synthetic-pass-phrase-2"}))).await;
    ok(&r, "GET", "/api/firm/clients/client-a/work", &tok, None).await;
    assert_eq!(call(&r, "GET", "/api/firm/clients/client-b/work", &tok, None).await.0, 403);
    let closes = ok(&r, "GET", "/api/firm/closes", &tok, None).await;
    assert!(closes.as_array().unwrap().iter().all(|c| c["client_ref"] == "client-a") && closes.as_array().unwrap().len() == 1, "{closes}");
    assert_eq!(call(&r, "POST", "/api/firm/closes", &tok, Some(json!({"name":"X","period_start":"2025-12-01","period_end":"2025-12-31","client_ref":"client-b"}))).await.0, 403);

    // Cron: 503 unconfigured, 401 wrong secret, skipped unless NXT_FIRM_CRON=on, then ticks.
    assert_eq!(call(&r, "GET", "/api/cron/firm-tick", "", None).await.0, 503);
    unsafe { std::env::set_var("CRON_SECRET", "synthetic-cron-secret-0123456789") };
    assert_eq!(call(&r, "GET", "/api/cron/firm-tick", "k", None).await.0, 401);
    assert_eq!(ok(&r, "GET", "/api/cron/firm-tick", "synthetic-cron-secret-0123456789", None).await["skipped"], true);
    unsafe { std::env::set_var("NXT_FIRM_CRON", "on") };
    let t = ok(&r, "GET", "/api/cron/firm-tick", "synthetic-cron-secret-0123456789", None).await;
    assert!(t.get("skipped").is_none(), "{t}");
}
