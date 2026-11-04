//! Synthetic acceptance: a staff account sees and changes only the journeys of
//! clients it is assigned to; the shared admin key still sees every client.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

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

async fn preparer(r: &axum::Router, name: &str, clients: Value) -> String {
    ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":name,"display_name":"Synthetic Preparer","role":"preparer","clients":clients,"temporary_password":PW}))).await;
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":name,"password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn journeys_are_scoped_to_assigned_clients() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::set_var("NXT_AUTONOMY", r#"{"books_access":"human"}"#);
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let tok = preparer(&r, "scoped", json!(["client-a"])).await;
    // '*' is admin-only; a preparer assigned both clients sees both.
    let all = preparer(&r, "both", json!(["client-a", "client-b"])).await;

    let qbo = json!({"systems": ["quickbooks_online"]});
    for c in ["client-a", "client-b"] {
        ok(&r, "POST", "/api/journeys", "k", Some(json!({"client_id": c, "sources": {"systems": ["dtmax"], "dtmax_company": "firm-dtmax"}}))).await;
        let key = c.replace('-', "").to_uppercase();
        ok(&r, "POST", "/api/source-imports/dtmax/companies/firm-dtmax/clients", "k", Some(json!({"client_id": c, "dtmax_keys": [key]}))).await;
        let bytes = format!("synthetic dt max return {c}");
        ok(&r, "POST", "/api/source-imports/dtmax/companies/firm-dtmax/files", "k", Some(json!({
            "synthetic": true, "original_path": format!("Y:\\DTMax\\2023\\{}.t23", key.to_lowercase()), "year": 2023, "file_type": "t23",
            "sha256": hex::encode(Sha256::digest(bytes.as_bytes())), "content_hex": hex::encode(bytes.as_bytes()),
        }))).await;
        for step in ["enquiry_received", "discovery_call"] {
            ok(&r, "POST", &format!("/api/journeys/{c}/steps/{step}"), "k", Some(json!({"action": "complete"}))).await;
        }
        let j = ok(&r, "POST", &format!("/api/journeys/{c}/advance"), "k", None).await;
        assert_eq!(j["pending_review"]["step"], "books_access", "{j}");
    }

    let denied: [(&str, &str, Option<Value>); 6] = [
        ("GET", "/api/journeys/client-b", None),
        ("PUT", "/api/journeys/client-b/sources", Some(qbo.clone())),
        ("POST", "/api/journeys/client-b/advance", None),
        ("POST", "/api/journeys/client-b/work", None),
        ("POST", "/api/journeys/client-b/steps/books_access", Some(json!({"action": "skip"}))),
        ("POST", "/api/journeys", Some(json!({"client_id": "client-c"}))),
    ];
    for (m, uri, body) in denied {
        let (s, v) = call(&r, m, uri, &tok, body).await;
        assert_eq!(s, 403, "{m} {uri}: {v}");
    }
    let (s, _) = call(&r, "GET", "/api/journeys/no-such-client", &tok, None).await;
    assert_eq!(s, 403, "an unassigned id gets the same answer whether or not it exists");

    let list = ok(&r, "GET", "/api/journeys", &tok, None).await;
    assert_eq!(list["journeys"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["journeys"][0]["client_id"], "client-a");
    let queue = ok(&r, "GET", "/api/journeys/reviews", &tok, None).await;
    assert_eq!(queue["reviews"].as_array().unwrap().len(), 1, "{queue}");
    assert_eq!(queue["reviews"][0]["client_id"], "client-a");

    ok(&r, "GET", "/api/journeys/client-a", &tok, None).await;
    ok(&r, "PUT", "/api/journeys/client-a/sources", &tok, Some(qbo)).await;
    ok(&r, "POST", "/api/journeys/client-a/advance", &tok, None).await;
    assert_eq!(call(&r, "POST", "/api/journeys/client-a/work", &tok, None).await.0, 503, "assigned, but no model configured");

    for key in ["k", all.as_str()] {
        assert_eq!(ok(&r, "GET", "/api/journeys", key, None).await["journeys"].as_array().unwrap().len(), 2);
        assert_eq!(ok(&r, "GET", "/api/journeys/reviews", key, None).await["reviews"].as_array().unwrap().len(), 2);
        ok(&r, "GET", "/api/journeys/client-b", key, None).await;
    }
    ok(&r, "GET", "/api/journeys/coverage", &tok, None).await;
}
