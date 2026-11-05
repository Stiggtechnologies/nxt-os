//! Synthetic acceptance for the Filing Desk and the internal-only gating of guided intake
//! and the desk. No real people, clients or CRA calls: the filing stub always refuses.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";
const SMS: &str = "246810";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: Option<&str>, body: Option<Value>) -> (u16, Value) { call_from(r, m, uri, tok, body, "10.0.0.9").await }
async fn call_from(r: &axum::Router, m: &str, uri: &str, tok: Option<&str>, body: Option<Value>, ip: &str) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", ip);
    if let Some(t) = tok { q = q.header("authorization", format!("Bearer {t}")); }
    let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&b)})))
}
async fn ok(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, m, uri, Some(tok), body).await;
    assert_eq!(s, 200, "{m} {uri}: {v}");
    v
}
async fn st(r: &axum::Router, m: &str, uri: &str, tok: Option<&str>, body: Option<Value>) -> u16 { call(r, m, uri, tok, body).await.0 }

/// Creates a staff user with a phone on file and signs in with password + SMS code.
async fn onboard(r: &axum::Router, username: &str, name: &str, role: &str, clients: Value) -> (String, String) {
    let u = ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":username,"display_name":name,"role":role,"clients":clients,"temporary_password":PW}))).await;
    let id = u["id"].as_str().unwrap().to_string();
    ok(r, "POST", &format!("/api/staff/users/{id}"), "k", Some(json!({"phone":"780-555-0100"}))).await;
    // Each synthetic user signs in from its own address so the per-network login limit isn't hit.
    let ip = format!("10.1.{}.{}", username.len(), username.bytes().map(u32::from).sum::<u32>() % 250);
    let (s1, v1) = call_from(r, "POST", "/api/staff/login/sms", None, Some(json!({"username":username,"password":PW})), &ip).await;
    assert_eq!(s1, 200, "{v1}");
    let (s2, login) = call_from(r, "POST", "/api/staff/login", None, Some(json!({"username":username,"password":PW,"sms_code":SMS})), &ip).await;
    assert_eq!(s2, 200, "{login}");
    let tok = login["token"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    (id, tok)
}

fn t1(client: &str) -> Value {
    json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic filer test","client_ref":client,
        "inputs":{"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000},"notes":"Synthetic only."})
}
fn path(id: &str, tail: &str) -> String { format!("/api/tax/returns/{id}{tail}") }

/// Preparer calculates and sends to review; reviewer approves. Returns the approved return.
async fn approve(r: &axum::Router, id: &str, prep: &str, rev: &str) -> Value {
    let t = ok(r, "GET", &path(id, ""), prep, None).await;
    let t = ok(r, "POST", &path(id, "/calculate"), prep, Some(json!({"expected_revision":t["revision"],"idempotency_key":format!("c{}", t["revision"])}))).await;
    let t = ok(r, "POST", &path(id, "/transition"), prep, Some(json!({"expected_revision":t["revision"],"status":"in_review","note":"","reviewer":"Riley Reviewer"}))).await;
    ok(r, "POST", &path(id, "/transition"), rev, Some(json!({"expected_revision":t["revision"],"status":"approved","note":"Checked","reviewer":"Riley Reviewer"}))).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filing_desk_is_internal_filer_only_two_person_append_only_and_launch_gated() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::set_var("NXT_SMS_MOCK_CODE", SMS);
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);

    // ---- Internal-only: no public or client access to guided intake or the Filing Desk. ----
    let internal = [("GET", "/api/tax/desk/me"), ("GET", "/api/tax/desk/queue"), ("GET", "/api/tax/desk/log"), ("GET", "/api/tax/desk/launch"),
        ("POST", "/api/tax/desk/launch"), ("POST", "/api/tax/desk/filers"), ("GET", "/api/tax/returns/x/desk"), ("POST", "/api/tax/returns/x/desk/confirm"),
        ("GET", "/api/tax/returns"), ("POST", "/api/tax/returns"), ("POST", "/api/tax/families"), ("POST", "/api/tax/t2/intake")];
    for (m, u) in internal {
        let body = (m == "POST").then(|| json!({}));
        assert_eq!(st(&r, m, u, None, body.clone()).await, 401, "public {m} {u}");
        // Client-portal style, bogus or forged-session bearers are not staff credentials.
        for bogus in ["client-portal-token", "nxs_forged", "nxc_client_session"] {
            assert_eq!(st(&r, m, u, Some(bogus), body.clone()).await, 401, "{bogus} {m} {u}");
        }
    }

    let all = json!(["TEST-0001"]);
    let (_admin_id, admin) = onboard(&r, "adm", "Ada Admin", "admin", json!(["*"])).await;
    let (prep_id, prep) = onboard(&r, "prep", "Pat Preparer", "preparer", all.clone()).await;
    let (_rev_id, rev) = onboard(&r, "rev", "Riley Reviewer", "reviewer", all.clone()).await;
    let (filer_id, filer) = onboard(&r, "fil", "Fran Filer", "reviewer", all.clone()).await;
    let (_, desk) = onboard(&r, "desk", "Dana Desk", "front_desk", all.clone()).await;
    let (_, ro) = onboard(&r, "robin", "Robin Readonly", "read_only", all.clone()).await;

    // The guided T1 Slips screen aggregates slips statelessly; scoped preparers must reach it.
    let agg = ok(&r, "POST", "/api/tax/slips/aggregate", &prep, Some(json!({"slips":[{"kind":"T4","boxes":{"14":58000}}]}))).await;
    assert!(agg.is_object(), "{agg}");
    assert_eq!(st(&r, "POST", "/api/tax/slips/aggregate", None, Some(json!({"slips":[]}))).await, 401);
    // Any staff may ask what they can do; only filers/admins reach the desk itself.
    let me = ok(&r, "GET", "/api/tax/desk/me", &filer, None).await;
    assert_eq!(me["filer"], false);
    assert_eq!(me["client_launch_enabled"], false);
    assert_eq!(st(&r, "GET", "/api/tax/desk/queue", Some(&filer), None).await, 403);
    assert_eq!(st(&r, "GET", "/api/tax/desk/queue", Some("k"), None).await, 403, "break-glass is not a filer");
    // Front desk / read-only can't change the fee flag (policy), non-admins can't grant filers.
    assert_eq!(st(&r, "POST", "/api/tax/desk/filers", Some(&rev), Some(json!({"user_id":filer_id,"granted":true}))).await, 403);
    assert_eq!(st(&r, "POST", "/api/tax/desk/filers", Some("k"), Some(json!({"user_id":filer_id,"granted":true}))).await, 403);
    ok(&r, "POST", "/api/tax/desk/filers", &admin, Some(json!({"user_id":filer_id,"granted":true}))).await;
    ok(&r, "POST", "/api/tax/desk/filers", &admin, Some(json!({"user_id":prep_id,"granted":true}))).await;
    assert_eq!(ok(&r, "GET", "/api/tax/desk/me", &filer, None).await["filer"], true);

    // ---- Queue: approved + fee paid, sorted by deadline. ----
    let t = ok(&r, "POST", "/api/tax/returns", &prep, Some(t1("TEST-0001"))).await;
    let tid = t["id"].as_str().unwrap().to_string();
    approve(&r, &tid, &prep, &rev).await;
    assert_eq!(ok(&r, "GET", "/api/tax/desk/queue", &filer, None).await["returns"].as_array().unwrap().len(), 0, "fee not paid yet");
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/fee"), Some(&desk), Some(json!({"paid":true}))).await, 403);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/fee"), Some(&ro), Some(json!({"paid":true}))).await, 403);
    ok(&r, "POST", &path(&tid, "/desk/fee"), &prep, Some(json!({"paid":true}))).await;
    let audit = ok(&r, "GET", "/api/tax/audit?limit=200", &admin, None).await;
    assert!(audit.to_string().contains("filing_fee_paid"), "{audit}");
    let q = ok(&r, "GET", "/api/tax/desk/queue", &filer, None).await;
    let row = &q["returns"][0];
    assert_eq!(row["id"], json!(tid));
    assert_eq!(row["deadline"], "2026-04-30");
    assert!(row["preparer"].as_str().unwrap().contains(&prep_id));
    assert!(row["money"]["kind"].is_string());
    assert_eq!(q["netfile_enabled"], false);
    assert!(q["banner"].as_str().unwrap().contains("NETFILE disabled. A human files."));

    // ---- Walkthrough in order. ----
    let d = ok(&r, "GET", &path(&tid, "/desk"), &filer, None).await;
    let fp = d["fingerprint"].as_str().unwrap().to_string();
    assert_eq!(d["can_file"], false);
    assert!(d["blockers"].as_array().unwrap().is_empty(), "{d}");
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/seen"), Some(&filer), Some(json!({"section":"slips","fingerprint":fp}))).await, 409, "out of order");
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/seen"), Some(&filer), Some(json!({"section":"summary","fingerprint":"stale"}))).await, 409);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/seen"), Some(&rev), Some(json!({"section":"summary","fingerprint":fp}))).await, 403, "non-filer");
    for sec in ["summary", "slips", "deductions", "final"] {
        ok(&r, "POST", &path(&tid, "/desk/seen"), &filer, Some(json!({"section":sec,"fingerprint":fp}))).await;
    }
    let d = ok(&r, "GET", &path(&tid, "/desk"), &filer, None).await;
    assert_eq!(d["can_file"], true, "{d}");

    // ---- Two-person rule: the preparer (also a filer) cannot file their own return. ----
    for sec in ["summary", "slips", "deductions", "final"] {
        ok(&r, "POST", &path(&tid, "/desk/seen"), &prep, Some(json!({"section":sec,"fingerprint":fp}))).await;
    }
    assert!(ok(&r, "GET", &path(&tid, "/desk"), &prep, None).await["two_person_rule"].is_string());
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&prep), Some(json!({"fingerprint":fp,"authorize":true,"password":PW2,"sms_code":SMS}))).await, 403);

    // ---- Confirm: checkbox, step-up, fingerprint. ----
    let conf = |a: bool, pw: &str, code: &str, f: &str| json!({"fingerprint":f,"authorize":a,"password":pw,"sms_code":code});
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(conf(false, PW2, SMS, &fp))).await, 400);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(conf(true, "wrong-password-x", SMS, &fp))).await, 401);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(conf(true, PW2, "000000", &fp))).await, 401);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(json!({"fingerprint":fp,"authorize":true,"password":PW2}))).await, 401, "password alone is not enough");
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(conf(true, PW2, SMS, "deadbeef"))).await, 409);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some("k"), Some(conf(true, PW2, SMS, &fp))).await, 403);
    let c = ok(&r, "POST", &path(&tid, "/desk/confirm"), &filer, Some(conf(true, PW2, SMS, &fp))).await;
    assert_eq!(c["confirmation"]["status"], "ready_to_submit");
    assert_eq!(c["filing_result"], "refused");
    assert_eq!(c["transmitted"], false);
    assert_eq!(c["log_entry"]["result"]["outcome"], "refused");
    assert_eq!(c["log_entry"]["result"]["http_status"], 409);
    assert_eq!(c["log_entry"]["fingerprint"], json!(fp));
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(conf(true, PW2, SMS, &fp))).await, 409, "already confirmed");
    // The return itself is not marked filed: a human files outside NXT.
    assert_eq!(ok(&r, "GET", &path(&tid, ""), &prep, None).await["status"], "approved");
    assert_eq!(ok(&r, "GET", "/api/tax/desk/queue", &filer, None).await["returns"].as_array().unwrap().len(), 0);

    // ---- Immutable log: filter by day and filer; no update or delete path. ----
    let day = c["log_entry"]["day"].as_str().unwrap().to_string();
    let l = ok(&r, "GET", "/api/tax/desk/log", &filer, None).await;
    assert_eq!(l["entries"].as_array().unwrap().len(), 1);
    assert_eq!(l["chain_verified"], true);
    assert_eq!(l["append_only"], true);
    assert_eq!(ok(&r, "GET", &format!("/api/tax/desk/log?day={day}"), &filer, None).await["entries"].as_array().unwrap().len(), 1);
    assert_eq!(ok(&r, "GET", "/api/tax/desk/log?day=2000-01-01", &filer, None).await["entries"].as_array().unwrap().len(), 0);
    assert_eq!(ok(&r, "GET", &format!("/api/tax/desk/log?filer={filer_id}"), &filer, None).await["entries"].as_array().unwrap().len(), 1);
    assert_eq!(ok(&r, "GET", &format!("/api/tax/desk/log?filer={prep_id}"), &filer, None).await["entries"].as_array().unwrap().len(), 0);
    for m in ["PUT", "PATCH", "DELETE"] {
        assert!([404u16, 405].contains(&st(&r, m, "/api/tax/desk/log", Some(&admin), Some(json!({}))).await), "{m} log");
    }

    // ---- Client-launch gate: default off, needs a recorded internal end-to-end test run. ----
    assert_eq!(ok(&r, "GET", "/api/tax/desk/launch", &filer, None).await["client_launch_enabled"], false);
    assert_eq!(st(&r, "POST", "/api/tax/desk/launch", Some(&admin), Some(json!({"enabled":true}))).await, 409, "no test run yet");
    assert_eq!(st(&r, "POST", "/api/tax/desk/launch/test-run", Some(&filer), Some(json!({"return_id":tid}))).await, 403);
    let run = ok(&r, "POST", "/api/tax/desk/launch/test-run", &admin, Some(json!({"return_id":tid}))).await;
    assert_eq!(run["fingerprint"], json!(fp));
    assert_eq!(st(&r, "POST", "/api/tax/desk/launch", Some(&filer), Some(json!({"enabled":true}))).await, 403);
    assert_eq!(st(&r, "POST", "/api/tax/desk/launch", Some("k"), Some(json!({"enabled":true}))).await, 403);
    assert_eq!(ok(&r, "POST", "/api/tax/desk/launch", &admin, Some(json!({"enabled":true}))).await["client_launch_enabled"], true);
    ok(&r, "POST", "/api/tax/desk/launch", &admin, Some(json!({"enabled":false}))).await;

    // ---- Any change after confirmation invalidates it (fingerprint mismatch) and re-queues. ----
    let t = ok(&r, "GET", &path(&tid, ""), &prep, None).await;
    ok(&r, "POST", &path(&tid, "/transition"), &prep, Some(json!({"expected_revision":t["revision"],"status":"draft","note":"client sent a late slip"}))).await;
    approve(&r, &tid, &prep, &rev).await;
    let d = ok(&r, "GET", &path(&tid, "/desk"), &filer, None).await;
    assert_ne!(d["fingerprint"], json!(fp));
    assert!(d["confirmation"].is_null());
    assert!(d["seen"].as_array().unwrap().is_empty(), "seen marks are bound to the old fingerprint");
    let q = ok(&r, "GET", "/api/tax/desk/queue", &filer, None).await;
    assert_eq!(q["returns"][0]["invalidated_confirmation"], true);
    assert_eq!(st(&r, "POST", &path(&tid, "/desk/confirm"), Some(&filer), Some(conf(true, PW2, SMS, &fp))).await, 409, "old fingerprint");
    // The old test run no longer clears the launch gate.
    assert_eq!(st(&r, "POST", "/api/tax/desk/launch", Some(&admin), Some(json!({"enabled":true}))).await, 409);

    // A non-synthetic client reference can never stand in for the launch test run.
    let other = ok(&r, "POST", "/api/tax/returns", &admin, Some(t1("client-real-ish"))).await;
    assert_eq!(st(&r, "POST", "/api/tax/desk/launch/test-run", Some(&admin), Some(json!({"return_id":other["id"]}))).await, 409);
}

#[tokio::test]
async fn append_only_store_has_no_rewrite_and_rejects_unknown_streams() {
    let s = Store::memory();
    s.append_only("tax-filing", &json!({"n":1})).await.unwrap();
    s.append_only("tax-filing", &json!({"n":2})).await.unwrap();
    let v = s.read_append_only("tax-filing", 10).await.unwrap();
    assert_eq!(v, vec![json!({"n":2}), json!({"n":1})], "newest first");
    assert!(s.append_only("anything-else", &json!({})).await.is_err());
}
