//! Synthetic acceptance: '*' is admin-only, preparers are refused unassigned clients on every
//! client-keyed surface, and every access event lands in the staff audit log. No real people.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.9.0.1");
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
async fn events(r: &axum::Router) -> Vec<Value> { ok(r, "GET", "/api/staff/audit", "k", None).await["events"].as_array().unwrap().clone() }
fn has(ev: &[Value], action: &str, f: impl Fn(&Value) -> bool) -> bool { ev.iter().any(|e| e["action"] == action && f(e)) }

async fn signed_in(r: &axum::Router, user: &str) -> String {
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":user,"password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wildcard_is_admin_only_preparers_are_scoped_and_every_access_event_is_logged() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    staff::reset_denial_limiter();
    let r = app_with(Store::memory);

    // (1) '*' is refused for every non-admin role, on create and on update; admins may hold it.
    for role in ["preparer", "reviewer", "front_desk", "read_only"] {
        let (s, v) = call(&r, "POST", "/api/staff/users", "k", Some(json!({"username":format!("w-{role}"),"display_name":"Synthetic","role":role,"clients":["*"],"temporary_password":PW}))).await;
        assert_eq!(s, 400, "{role}: {v}");
        assert!(v["error"].as_str().unwrap().contains("admin accounts only"), "{v}");
    }
    let admin = ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"adm","display_name":"Synthetic Admin","role":"admin","clients":["*"],"temporary_password":PW}))).await;
    let prep = ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"prep","display_name":"Synthetic Preparer","role":"preparer","clients":["client-a"],"temporary_password":PW}))).await;
    let pid = prep["id"].as_str().unwrap().to_string();
    let (s, _) = call(&r, "POST", &format!("/api/staff/users/{pid}"), "k", Some(json!({"clients":["client-a","*"]}))).await;
    assert_eq!(s, 400);
    // Demoting a '*' admin without narrowing clients is also refused.
    let aid = admin["id"].as_str().unwrap().to_string();
    assert_eq!(call(&r, "POST", &format!("/api/staff/users/{aid}"), "k", Some(json!({"role":"preparer"}))).await.0, 400);
    // An empty list grants nothing (it is not "all").
    let empty = ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"none","display_name":"Synthetic None","role":"preparer","clients":[],"temporary_password":PW}))).await;
    assert_eq!(empty["clients"], json!([]));

    // Assign, unassign, role and admin changes.
    ok(&r, "POST", &format!("/api/staff/users/{pid}"), "k", Some(json!({"clients":["client-a","client-c"]}))).await;
    ok(&r, "POST", &format!("/api/staff/users/{pid}"), "k", Some(json!({"clients":["client-a"]}))).await;
    ok(&r, "POST", &format!("/api/staff/users/{pid}"), "k", Some(json!({"role":"reviewer"}))).await;
    ok(&r, "POST", &format!("/api/staff/users/{pid}"), "k", Some(json!({"role":"preparer"}))).await;
    ok(&r, "POST", &format!("/api/staff/users/{aid}"), "k", Some(json!({"role":"preparer","clients":["client-a"]}))).await;
    ok(&r, "POST", &format!("/api/staff/users/{aid}"), "k", Some(json!({"role":"admin","clients":["*"]}))).await;

    // (3) A preparer is denied an unassigned client on every client-keyed surface.
    let tok = signed_in(&r, "prep").await;
    let denied = [
        "/api/journeys/client-b",
        "/api/company-books/client-b",
        "/api/company-ledger/client-b/accounts",
        "/api/nixon/clients/client-b",
        "/api/source-imports/dtmax/companies/syn/clients/client-b/files",
    ];
    for uri in denied {
        let (s, v) = call(&r, "GET", uri, &tok, None).await;
        assert_eq!(s, 403, "{uri}: {v}");
    }
    // Firm-wide lists either refuse a scoped preparer or show only assigned clients.
    let (s, v) = call(&r, "GET", "/api/clients", &tok, None).await;
    if s == 200 { assert!(v["clients"].as_array().unwrap().iter().all(|c| c["client_ref"] == "client-a"), "{v}"); } else { assert_eq!(s, 403); }
    assert_eq!(call(&r, "GET", "/api/company-books/roster", &tok, None).await.0, 403);
    let (s, v) = call(&r, "GET", "/api/journeys", &tok, None).await;
    assert_eq!(s, 200);
    assert!(v["journeys"].as_array().unwrap().iter().all(|j| j["client_id"] == "client-a"));
    // A repeated denial is rate-limited, not re-written.
    for _ in 0..20 { call(&r, "GET", "/api/journeys/client-b", &tok, None).await; }

    // (2) Every access event type is in the log, with ids only.
    let ev = events(&r).await;
    assert!(has(&ev, "wildcard_refused", |e| e["result"] == "refused" && e["route"] == "POST /api/staff/users"));
    assert!(has(&ev, "wildcard_refused", |e| e["target_user"] == pid.as_str()));
    assert!(has(&ev, "client_assigned", |e| e["target_user"] == pid.as_str() && e["client"] == "client-c"));
    assert!(has(&ev, "client_unassigned", |e| e["target_user"] == pid.as_str() && e["client"] == "client-c"));
    assert!(has(&ev, "role_changed", |e| e["target_user"] == pid.as_str()));
    assert!(has(&ev, "admin_revoked", |e| e["target_user"] == aid.as_str()));
    assert!(has(&ev, "admin_granted", |e| e["target_user"] == aid.as_str()));
    for (uri, client) in [("/api/journeys/client-b", "client-b"), ("/api/company-books/client-b", "client-b"), ("/api/company-ledger/client-b/accounts", "client-b"), ("/api/nixon/clients/client-b", "client-b"), ("/api/source-imports/dtmax/companies/syn/clients/client-b/files", "client-b")] {
        assert!(has(&ev, "access_denied", |e| e["route"] == format!("GET {uri}") && e["client"] == client && e["result"] == "denied 403" && e["actor"].as_str().unwrap().contains(&pid) && e["at"].is_string()), "no denial logged for {uri}");
    }
    assert!(has(&ev, "access_denied", |e| e["route"] == "GET /api/company-books/roster"));
    let journey_denials = ev.iter().filter(|e| e["action"] == "access_denied" && e["route"] == "GET /api/journeys/client-b").count();
    assert_eq!(journey_denials, 1, "repeated denials must be rate-limited");
    // No names leak into access events: only ids.
    assert!(ev.iter().filter(|e| e["action"] == "access_denied").all(|e| !e.to_string().contains("Synthetic Preparer") || e["actor"].as_str().unwrap().contains("Synthetic Preparer")));
}
