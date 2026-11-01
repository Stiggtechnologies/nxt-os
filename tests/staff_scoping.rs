//! Synthetic acceptance: client scoping on signatures, firm projects and company books,
//! and the authenticated scheduled e-sign sweep. No real people or clients.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.1.0.1");
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
fn draft(title: &str) -> Value {
    json!({"title":title,"email_subject":"Synthetic","email_message":"No message is sent.","context":null,
        "documents":[{"id":"doc1","name":"Synthetic agreement","pages":["Synthetic text."]}],
        "recipients":[{"id":"role1","role_name":"Client signer","name":"Synthetic Signer","email":"signer@example.invalid","kind":"signer","routing_order":1}],
        "fields":[{"id":"field1","document_id":"doc1","recipient_id":"role1","page":1,"kind":"signature","label":"Signature","required":true,"x":1000,"y":7500,"width":3500,"height":600}]})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scoping_extends_to_signatures_firm_books_and_cron_is_authenticated() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::remove_var("CRON_SECRET");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);

    ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"scoped","display_name":"Sam Scoped","role":"preparer","clients":["client-a"],"temporary_password":PW}))).await;
    let tok = ok(&r, "POST", "/api/staff/login", "", Some(json!({"username":"scoped","password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(&r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(&r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    // must_change_password blocks work until the password is changed.
    assert_eq!(call(&r, "GET", "/api/signatures", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/firm/projects", &tok, None).await.0, 403);
    ok(&r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    let me = ok(&r, "GET", "/api/staff/me", &tok, None).await;
    assert_eq!(me["identity"]["must_change_password"], false);

    // Signatures: break-glass creates one envelope per client plus one without a client.
    let a = ok(&r, "POST", "/api/signatures", "k", Some(json!({"operator_label":"x","draft":draft("A"),"client_ref":"client-a"}))).await;
    let b = ok(&r, "POST", "/api/signatures", "k", Some(json!({"operator_label":"x","draft":draft("B"),"client_ref":"client-b"}))).await;
    ok(&r, "POST", "/api/signatures", "k", Some(json!({"operator_label":"x","draft":draft("None")}))).await;
    let id = |v: &Value| v["envelope"]["id"].as_str().or(v["id"].as_str()).unwrap().to_string();
    let (ida, idb) = (id(&a), id(&b));
    let list = ok(&r, "GET", "/api/signatures", &tok, None).await;
    let envs = list["envelopes"].as_array().unwrap();
    assert_eq!(envs.len(), 1, "{list}");
    assert_eq!(envs[0]["client_ref"], "client-a");
    assert_eq!(ok(&r, "GET", "/api/signatures", "k", None).await["envelopes"].as_array().unwrap().len(), 3);
    ok(&r, "GET", &format!("/api/signatures/{ida}"), &tok, None).await;
    assert_eq!(call(&r, "GET", &format!("/api/signatures/{idb}"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", &format!("/api/signatures/{idb}/esign"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/signatures/audit", &tok, None).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/signatures/esign/sweep", &tok, Some(json!({"operator_label":"x","dry_run":true}))).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/signatures", &tok, Some(json!({"operator_label":"x","draft":draft("C"),"client_ref":"client-b"}))).await.0, 403);
    ok(&r, "POST", "/api/signatures", &tok, Some(json!({"operator_label":"x","draft":draft("D"),"client_ref":"client-a"}))).await;
    // A write can't move an in-scope envelope to another client.
    let rev = ok(&r, "GET", &format!("/api/signatures/{ida}"), "k", None).await;
    let revision = rev["envelope"]["revision"].clone();
    assert_eq!(call(&r, "PUT", &format!("/api/signatures/{ida}"), &tok, Some(json!({"expected_revision":revision,"operator_label":"x","draft":draft("A"),"client_ref":"client-b"}))).await.0, 403);

    // Firm: only the caller's projects; other firm views are firm-wide.
    ok(&r, "POST", "/api/firm/projects", "k", Some(json!({"name":"Project A","client_ref":"client-a"}))).await;
    let pb = ok(&r, "POST", "/api/firm/projects", "k", Some(json!({"name":"Project B","client_ref":"client-b"}))).await;
    let projects = ok(&r, "GET", "/api/firm/projects", &tok, None).await;
    assert!(projects.as_array().unwrap().iter().all(|p| p["client_ref"] == "client-a") && projects.as_array().unwrap().len() == 1, "{projects}");
    let pbid = pb["id"].as_str().or(pb["project"]["id"].as_str()).unwrap();
    assert_eq!(call(&r, "GET", &format!("/api/firm/projects/{pbid}"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/firm/summary", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/firm/contacts", &tok, None).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/firm/projects", &tok, Some(json!({"name":"X","client_ref":"client-b"}))).await.0, 403);

    // Company books: company id is the client label; inventory and roster are firm-wide.
    assert_eq!(call(&r, "GET", "/api/company-books", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/company-books/roster", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/company-books/client-b/trial-balance", &tok, None).await.0, 403);
    assert_ne!(call(&r, "GET", "/api/company-books/client-a/trial-balance", &tok, None).await.0, 403);

    // Scheduled sweep: refuses without CRON_SECRET, refuses a wrong secret, runs dry-run by default.
    assert_eq!(call(&r, "GET", "/api/cron/esign-sweep", "", None).await.0, 503);
    unsafe { std::env::set_var("CRON_SECRET", "synthetic-cron-secret-0123456789") };
    assert_eq!(call(&r, "GET", "/api/cron/esign-sweep", "wrong", None).await.0, 401);
    assert_eq!(call(&r, "GET", "/api/cron/esign-sweep", "k", None).await.0, 401);
    let sw = ok(&r, "GET", "/api/cron/esign-sweep", "synthetic-cron-secret-0123456789", None).await;
    assert_eq!(sw["mode"], "dry_run");
    assert_eq!(sw["email_sent"], false);
}
