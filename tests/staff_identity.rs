//! Synthetic acceptance for individual staff accounts, roles, TOTP MFA, client scoping,
//! id-based segregation of duties and the break-glass key. No real people or clients.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: Option<&str>, body: Option<Value>, ip: &str) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", ip);
    if let Some(t) = tok { q = q.header("authorization", format!("Bearer {t}")); }
    let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&b)})))
}
async fn ok(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, m, uri, Some(tok), body, "10.0.0.1").await;
    assert_eq!(s, 200, "{m} {uri}: {v}");
    v
}
async fn status(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) { call(r, m, uri, Some(tok), body, "10.0.0.1").await }

/// Creates a user with the break-glass key, signs in, enrolls TOTP and returns (id, session).
async fn onboard(r: &axum::Router, username: &str, name: &str, role: &str, clients: Value) -> (String, String) {
    let u = ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":username,"display_name":name,"role":role,"clients":clients,"temporary_password":PW}))).await;
    let login = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":username,"password":PW}))).await;
    let tok = login["token"].as_str().unwrap().to_string();
    assert_eq!(login["mfa_enrollment_required"], true);
    // Before MFA the session is enrollment-only.
    assert_eq!(status(r, "GET", "/api/tax/returns", &tok, None).await.0, 403);
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    // The temporary password must be changed before any work.
    assert_eq!(status(r, "GET", "/api/tax/returns", &tok, None).await.0, 403);
    ok(r, "GET", "/api/staff/me", &tok, None).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    (u["id"].as_str().unwrap().to_string(), tok)
}

fn tax_input(client: &str) -> Value {
    json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic staff test","client_ref":client,
        "inputs":{"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000},"notes":"Synthetic only."})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn staff_accounts_roles_mfa_scoping_and_break_glass() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);

    // Login is public but wrong credentials fail; staff admin endpoints need a credential.
    assert_eq!(call(&r, "POST", "/api/staff/login", None, Some(json!({"username":"nobody","password":PW})), "10.0.0.1").await.0, 401);
    assert_eq!(call(&r, "GET", "/api/staff/users", None, None, "10.0.0.1").await.0, 401);

    let (prep_id, prep) = onboard(&r, "prep", "Pat Preparer", "preparer", json!(["*"])).await;
    let (_rev_id, rev) = onboard(&r, "rev", "Riley Reviewer", "reviewer", json!(["*"])).await;
    let (_, desk) = onboard(&r, "desk", "Dana Desk", "front_desk", json!(["*"])).await;
    let (_, ro) = onboard(&r, "robin", "Robin Readonly", "read_only", json!(["*"])).await;
    let (_, scoped) = onboard(&r, "scoped", "Sam Scoped", "preparer", json!(["client-a"])).await;

    let me = ok(&r, "GET", "/api/staff/me", &prep, None).await;
    assert_eq!(me["identity"]["user_id"], json!(prep_id));

    // MFA is required at login once enrolled.
    assert_eq!(call(&r, "POST", "/api/staff/login", None, Some(json!({"username":"prep","password":PW})), "10.0.0.2").await.0, 401);

    // Role policy.
    assert_eq!(status(&r, "POST", "/api/tax/returns", &ro, Some(tax_input("client-a"))).await.0, 403);
    assert_eq!(status(&r, "POST", "/api/tax/returns", &desk, Some(tax_input("client-a"))).await.0, 403);
    assert_eq!(status(&r, "GET", "/api/staff/users", &prep, None).await.0, 403);
    ok(&r, "GET", "/api/tax/returns", &ro, None).await;

    // Preparer creates and sends to review; label is stamped with the verified account.
    let t = ok(&r, "POST", "/api/tax/returns", &prep, Some(tax_input("client-a"))).await;
    let tid = t["id"].as_str().unwrap().to_string();
    let t = ok(&r, "POST", &format!("/api/tax/returns/{tid}/calculate"), &prep, Some(json!({"expected_revision":t["revision"],"idempotency_key":"c1"}))).await;
    let t = ok(&r, "POST", &format!("/api/tax/returns/{tid}/transition"), &prep, Some(json!({"expected_revision":t["revision"],"status":"in_review","note":"","reviewer":"Someone Else"}))).await;
    assert!(t["preparer_signoff"]["by"].as_str().unwrap().contains(&prep_id), "{t}");
    // Preparer role can't approve; break-glass can't review a staff-prepared return.
    assert_eq!(status(&r, "POST", &format!("/api/tax/returns/{tid}/transition"), &prep, Some(json!({"expected_revision":t["revision"],"status":"approved","note":"ok","reviewer":"Riley Reviewer"}))).await.0, 403);
    assert_eq!(status(&r, "POST", &format!("/api/tax/returns/{tid}/transition"), "k", Some(json!({"expected_revision":t["revision"],"status":"approved","note":"ok","reviewer":"Riley Reviewer"}))).await.0, 409);
    let a = ok(&r, "POST", &format!("/api/tax/returns/{tid}/transition"), &rev, Some(json!({"expected_revision":t["revision"],"status":"approved","note":"Checked","reviewer":"Pat Preparer"}))).await;
    assert!(a["review"]["reviewer"].as_str().unwrap().contains("Riley Reviewer ["), "typed name must be replaced: {a}");

    // Segregation of duties by id: a reviewer-role account cannot approve its own preparation.
    let t2 = ok(&r, "POST", "/api/tax/returns", &rev, Some(tax_input("client-b"))).await;
    let id2 = t2["id"].as_str().unwrap().to_string();
    let t2 = ok(&r, "POST", &format!("/api/tax/returns/{id2}/calculate"), &rev, Some(json!({"expected_revision":t2["revision"],"idempotency_key":"c2"}))).await;
    let t2 = ok(&r, "POST", &format!("/api/tax/returns/{id2}/transition"), &rev, Some(json!({"expected_revision":t2["revision"],"status":"in_review","note":"","reviewer":"x"}))).await;
    let (s, v) = status(&r, "POST", &format!("/api/tax/returns/{id2}/transition"), &rev, Some(json!({"expected_revision":t2["revision"],"status":"approved","note":"self","reviewer":"A Different Name"}))).await;
    assert_eq!(s, 409, "{v}");

    // Client scoping: list filtered, other client's record and firm-wide views refused.
    let list = ok(&r, "GET", "/api/tax/returns", &scoped, None).await;
    assert!(list.as_array().unwrap().iter().all(|x| x["client_ref"] == "client-a") && !list.as_array().unwrap().is_empty());
    assert_eq!(status(&r, "GET", &format!("/api/tax/returns/{id2}"), &scoped, None).await.0, 403);
    ok(&r, "GET", &format!("/api/tax/returns/{tid}"), &scoped, None).await;
    assert_eq!(status(&r, "GET", "/api/tax/summary", &scoped, None).await.0, 403);
    assert_eq!(status(&r, "POST", "/api/tax/returns", &scoped, Some(tax_input("client-b"))).await.0, 403);
    assert_eq!(status(&r, "GET", "/api/engagements/summary", &scoped, None).await.0, 403);

    // Staff audit records break-glass actions; logout ends the session.
    let audit = ok(&r, "GET", "/api/staff/audit", "k", None).await;
    assert!(audit.to_string().contains("user_created"));
    ok(&r, "POST", "/api/staff/logout", &ro, Some(json!({}))).await;
    assert_eq!(status(&r, "GET", "/api/tax/returns", &ro, None).await.0, 401);

    // Lockout after repeated failures.
    for _ in 0..5 { call(&r, "POST", "/api/staff/login", None, Some(json!({"username":"desk","password":"wrong-password-xx"})), "10.0.0.3").await; }
    assert_eq!(call(&r, "POST", "/api/staff/login", None, Some(json!({"username":"desk","password":PW})), "10.0.0.3").await.0, 429);

    // Public signing pages are rate limited per IP.
    let mut last = 0;
    for _ in 0..12 { last = call(&r, "POST", "/api/sign/nosuchtoken/otp", None, None, "10.9.9.9").await.0; }
    assert_eq!(last, 429);
    assert_ne!(call(&r, "POST", "/api/sign/nosuchtoken/otp", None, None, "10.9.9.10").await.0, 429);

    // E-sign sweep runs for staff (front desk allowed) and is a no-op here.
    let sw = ok(&r, "POST", "/api/signatures/esign/sweep", &desk, Some(json!({"operator_label":"x","dry_run":true}))).await;
    assert_eq!(sw["email_sent"], false);

    // Break-glass can be switched off by config.
    unsafe { std::env::set_var("NXT_BREAK_GLASS", "off") };
    assert_eq!(status(&r, "GET", "/api/tax/returns", "k", None).await.0, 401);
    ok(&r, "GET", "/api/tax/returns", &prep, None).await;
    unsafe { std::env::remove_var("NXT_BREAK_GLASS") };
}
