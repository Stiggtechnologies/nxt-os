//! God's Eye with real staff sessions (synthetic accounts and books only):
//! - a preparer or any client-scoped account gets 403 on every God's Eye route and on decide;
//! - the stored approver is the signed-in account's stamped label, never request-body text;
//! - AI wording is refused on a decided proposal.
//! One test in its own binary because the staff registry is process-global.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.4.0.7");
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
async fn account(r: &axum::Router, username: &str, display: &str, role: &str, clients: Value) -> String {
    ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":username,"display_name":display,"role":role,"clients":clients,"temporary_password":PW}))).await;
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":username,"password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gods_eye_is_firm_wide_staff_only_and_records_the_signed_in_approver() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        // AI on with no gateway auth: no request can leave the process.
        std::env::set_var("GODS_EYE_AI", "on");
        std::env::remove_var("AI_GATEWAY_API_KEY");
        std::env::remove_var("VERCEL_OIDC_TOKEN");
        std::env::remove_var("NVIDIA_API_KEY");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let shared = Store::memory();
    let r = app_with(move || shared.clone());

    // Synthetic company ledger with an invoice long past due, so the run has a finding.
    ok(&r, "POST", "/api/company-ledger/synthetic-co/open", "k", Some(json!({"name":"Synthetic Co Ltd"}))).await;
    let c = ok(&r, "POST", "/api/company-ledger/synthetic-co/contacts", "k", Some(json!({"kind":"customer","name":"Synthetic Buyer","terms_days":30}))).await;
    for date in ["2026-05-01", "2026-08-15"] {
        ok(&r, "POST", "/api/company-ledger/synthetic-co/invoices", "k", Some(json!({"contact_id": c["id"], "date": date,
            "lines": [{"description":"Synthetic goods","qty":1,"rate":250000,"account_id":"acc_4100","tax_code":"HST13"}]}))).await;
    }
    ok(&r, "POST", "/api/tax/returns", "k", Some(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic T1","client_ref":"synthetic-co","inputs":{},"parties":[]}))).await;

    let run = ok(&r, "POST", "/api/nixon/gods-eye/runs", "k", Some(json!({"as_of":"2026-11-12"}))).await;
    let id = run["id"].as_str().unwrap().to_string();
    let pids: Vec<String> = run["proposals"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect();
    assert!(pids.len() >= 3, "{run}");
    let decide = |pid: &str| format!("/api/nixon/reviews/{id}/proposals/{pid}/decide");
    let ai = |pid: &str| format!("/api/nixon/gods-eye/runs/{id}/proposals/{pid}/ai-draft");

    // Preparer scoped to the very client in the findings, and a reviewer scoped to one client.
    let preparer = account(&r, "ge-preparer", "Synthetic Preparer", "preparer", json!(["synthetic-co"])).await;
    let reviewer = account(&r, "ge-reviewer", "Synthetic Reviewer", "reviewer", json!(["synthetic-co"])).await;
    for tok in [&preparer, &reviewer] {
        assert_eq!(call(&r, "GET", "/api/nixon/gods-eye?as_of=2026-11-12", tok, None).await.0, 403);
        assert_eq!(call(&r, "POST", "/api/nixon/gods-eye/runs", tok, Some(json!({"as_of":"2026-11-12"}))).await.0, 403);
        assert_eq!(call(&r, "POST", &ai(&pids[0]), tok, None).await.0, 403);
        assert_eq!(call(&r, "POST", &decide(&pids[0]), tok, Some(json!({"actor_name":"Casey Quinn","decision":"approve","note":""}))).await.0, 403);
        assert_eq!(call(&r, "POST", &decide(&pids[0]), tok, Some(json!({"actor_name":"Casey Quinn","decision":"dismiss","note":""}))).await.0, 403);
    }
    let still = ok(&r, "GET", &format!("/api/nixon/reviews/{id}"), "k", None).await;
    assert!(still["proposals"].as_array().unwrap().iter().all(|p| p["status"] == "pending_approval"), "scoped accounts changed nothing");
    assert_eq!(still["decisions"].as_array().unwrap().len(), 0);

    // A firm-wide admin decides. The body names someone else; the stored approver is the session's label.
    let admin = account(&r, "ge-admin", "Synthetic Admin Person", "admin", json!(["*"])).await;
    let me = ok(&r, "GET", "/api/staff/me", &admin, None).await;
    let uid = me["identity"]["user_id"].as_str().unwrap_or_else(|| panic!("{me}")).to_string();
    let label = format!("Synthetic Admin Person [{uid}]");
    assert_eq!(me["label"], label.as_str());
    assert_eq!(call(&r, "GET", "/api/nixon/gods-eye?as_of=2026-11-12", &admin, None).await.0, 200);
    let decided = ok(&r, "POST", &decide(&pids[0]), &admin, Some(json!({"actor_name":"Casey Quinn","decision":"approve","note":"Synthetic check"}))).await;
    assert_eq!(decided["decisions"][0]["actor_name"], label.as_str(), "{decided}");
    assert!(!decided.to_string().contains("Casey Quinn"));
    // Even an empty body name works: the server stamps the person.
    let d2 = ok(&r, "POST", &decide(&pids[1]), &admin, Some(json!({"actor_name":"","decision":"dismiss","note":""}))).await;
    assert_eq!(d2["decisions"][1]["actor_name"], label.as_str());

    // AI wording is refused on a decided proposal (before any model call).
    let (s, v) = call(&r, "POST", &ai(&pids[0]), &admin, None).await;
    assert_eq!(s, 409, "{v}");
    assert!(v.to_string().contains("already has a decision"), "{v}");
    // On an undecided one, with no gateway auth configured, it fails cleanly and saves nothing.
    let (s, v) = call(&r, "POST", &ai(&pids[2]), &admin, None).await;
    assert_eq!(s, 502, "{v}");
    let after = ok(&r, "GET", &format!("/api/nixon/reviews/{id}"), "k", None).await;
    assert!(after["proposals"].as_array().unwrap().iter().all(|p| p["payload"].get("ai_draft").is_none()));
}
