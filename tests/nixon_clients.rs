//! Synthetic acceptance: Nixon's per-client view shows staff only the clients
//! they are assigned to, and stays read-only.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.4.0.1");
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
async fn each_account_sees_only_its_clients() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    let tok = preparer(&r, "scoped", json!(["client-a"])).await;
    for c in ["client-a", "client-b"] {
        ok(&r, "POST", "/api/journeys", "k", Some(json!({"client_id": c}))).await;
    }

    let mine = ok(&r, "GET", "/api/nixon/clients", &tok, None).await;
    assert_eq!(mine["clients"].as_array().unwrap().len(), 1, "{mine}");
    assert_eq!(mine["clients"][0]["client_id"], "client-a");
    assert_eq!(ok(&r, "GET", "/api/nixon/clients", "k", None).await["clients"].as_array().unwrap().len(), 2);

    let v = ok(&r, "GET", "/api/nixon/clients/client-a", &tok, None).await;
    assert_eq!(v["read_only"], true);
    assert!(v["findings"].to_string().contains("No company books"), "{v}");

    assert_eq!(call(&r, "GET", "/api/nixon/clients/client-b", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/nixon/clients/no-such-client", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/nixon/clients/no-such-client", "k", None).await.0, 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn staff_see_every_assigned_client_and_its_ledger() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let shared = Store::memory();
    let r = app_with(move || shared.clone());
    let tok = preparer(&r, "assigned", json!(["client-a", "client-new"])).await;
    for (client, amount) in [("client-a", 10000), ("client-b", 7700)] {
        let base = format!("/api/company-ledger/{client}");
        ok(&r, "POST", &format!("{base}/open"), "k", Some(json!({"name": format!("Synthetic {client}")}))).await;
        let accounts = ok(&r, "GET", &format!("{base}/accounts"), "k", None).await;
        let income = accounts.as_array().unwrap().iter().find(|a| a["kind"] == "income").unwrap()["id"].clone();
        let customer = ok(&r, "POST", &format!("{base}/contacts"), "k", Some(json!({"kind":"customer","name":"Maple"}))).await;
        ok(&r, "POST", &format!("{base}/invoices"), "k", Some(json!({"contact_id":customer["id"],"date":"2026-02-02","lines":[{"account_id":income,"description":"Work","qty":1.0,"rate":amount}]}))).await;
    }

    // A client with a ledger and no journey or books is listed, and so is an assigned client with nothing yet.
    let mine = ok(&r, "GET", "/api/nixon/clients", &tok, None).await;
    let rows = mine["clients"].as_array().unwrap();
    let ids: Vec<&str> = rows.iter().map(|c| c["client_id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["client-a", "client-new"], "{mine}");
    assert_eq!(rows[0]["ledger_open"], true);
    assert_eq!(rows[1]["ledger_open"], false);
    assert_eq!(ok(&r, "GET", "/api/nixon/clients", "k", None).await["clients"].as_array().unwrap().len(), 2);

    let v = ok(&r, "GET", "/api/nixon/clients/client-a", &tok, None).await;
    let l = &v["ledger"];
    assert_eq!(l["name"], "Synthetic client-a");
    assert_eq!(l["trial_balance"]["balanced"], true);
    assert_eq!(l["recent_entries"].as_array().unwrap().len(), 1);
    assert_eq!(l["ar_aging"]["rows"][0]["contact"], "Maple");
    assert!(l["ar_aging"]["totals"]["total"].as_i64().unwrap() >= 10000, "{l}");
    assert!(!v.to_string().contains("client-b"));

    let new = ok(&r, "GET", "/api/nixon/clients/client-new", &tok, None).await;
    assert!(new["ledger"].is_null());
    assert!(new["findings"].to_string().contains("No client ledger"), "{new}");

    assert_eq!(call(&r, "GET", "/api/nixon/clients/client-b", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/company-ledger/client-b/entries", &tok, None).await.0, 403);
}
