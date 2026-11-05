//! Stripe Checkout for guided T1 filing fee: mock Stripe HTTP, FeeFlag gate, ledger journal,
//! webhook HMAC + idempotency, staff refund. No live Stripe, no secrets printed.
use axum::{
    Router,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use hmac::{Hmac, KeyInit, Mac};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";
const SMS: &str = "246810";
const WHSEC: &str = "whsec_test_stripe_t1";
const FEE: i64 = 19_900;

#[derive(Default)]
struct MockStripe {
    sessions: Mutex<HashMap<String, Value>>,
    refunds: Mutex<Vec<Value>>,
    next: Mutex<u64>,
}

impl MockStripe {
    fn next_id(&self, prefix: &str) -> String {
        let mut n = self.next.lock().unwrap();
        *n += 1;
        format!("{prefix}{}", *n)
    }
}

fn parse_form(body: &str) -> HashMap<String, String> {
    body.split('&').filter_map(|pair| {
        let mut it = pair.splitn(2, '=');
        let k = it.next()?;
        let v = it.next().unwrap_or("");
        Some((url_decode(k), url_decode(v)))
    }).collect()
}
fn url_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = &s[i+1..i+3];
            if let Ok(v) = u8::from_str_radix(hex, 16) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn create_session(State(m): State<Arc<MockStripe>>, headers: HeaderMap, body: String) -> Response {
    if !headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").starts_with("Bearer sk_test_") {
        return (StatusCode::UNAUTHORIZED, "bad key").into_response();
    }
    let form: HashMap<String, String> = parse_form(&body);
    let rid = form.get("metadata[return_id]").cloned().unwrap_or_default();
    let amount: i64 = form.get("line_items[0][price_data][unit_amount]").and_then(|s| s.parse().ok()).unwrap_or(0);
    let currency = form.get("line_items[0][price_data][currency]").cloned().unwrap_or_else(|| "cad".into());
    let id = m.next_id("cs_test_");
    let pi = m.next_id("pi_test_");
    let session = json!({
        "id": id,
        "object": "checkout.session",
        "status": "open",
        "url": format!("https://checkout.test/pay/{id}"),
        "amount_total": amount,
        "currency": currency,
        "client_reference_id": rid,
        "metadata": {"return_id": rid, "product": "t1_filing_fee"},
        "payment_intent": pi,
    });
    m.sessions.lock().unwrap().insert(id.clone(), session.clone());
    (StatusCode::OK, axum::Json(session)).into_response()
}

async fn get_session(State(m): State<Arc<MockStripe>>, Path(id): Path<String>) -> Response {
    match m.sessions.lock().unwrap().get(&id).cloned() {
        Some(s) => (StatusCode::OK, axum::Json(s)).into_response(),
        None => (StatusCode::NOT_FOUND, "missing").into_response(),
    }
}

async fn get_pi(Path(id): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let _ = q;
    axum::Json(json!({
        "id": id,
        "object": "payment_intent",
        "latest_charge": {
            "id": "ch_test_1",
            "balance_transaction": {"id": "txn_1", "fee": 579, "currency": "cad"}
        }
    }))
    .into_response()
}

async fn create_refund(State(m): State<Arc<MockStripe>>, body: String) -> Response {
    let form: HashMap<String, String> = parse_form(&body);
    let pi = form.get("payment_intent").cloned().unwrap_or_default();
    let id = m.next_id("re_test_");
    let refund = json!({"id": id, "object": "refund", "payment_intent": pi, "status": "succeeded"});
    m.refunds.lock().unwrap().push(refund.clone());
    (StatusCode::OK, axum::Json(refund)).into_response()
}

async fn mock_stripe() -> (String, Arc<MockStripe>, tokio::task::JoinHandle<()>) {
    let mock = Arc::new(MockStripe::default());
    let app = Router::new()
        .route("/v1/checkout/sessions", post(create_session))
        .route("/v1/checkout/sessions/{id}", get(get_session))
        .route("/v1/payment_intents/{id}", get(get_pi))
        .route("/v1/refunds", post(create_refund))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), mock, handle)
}

fn sign(secret: &str, body: &[u8]) -> String {
    let ts = chrono::Utc::now().timestamp();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{ts}.").as_bytes());
    mac.update(body);
    format!("t={ts},v1={}", hex::encode(mac.finalize().into_bytes()))
}

async fn call(r: &axum::Router, m: &str, uri: &str, tok: Option<&str>, body: Option<Value>) -> (u16, Value) {
    call_raw(r, m, uri, tok, body.map(|v| (Some("application/json"), v.to_string())), None).await
}

async fn call_raw(
    r: &axum::Router,
    m: &str,
    uri: &str,
    tok: Option<&str>,
    body: Option<(Option<&str>, String)>,
    extra: Option<(&str, &str)>,
) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.0.0.42");
    if let Some(t) = tok {
        q = q.header("authorization", format!("Bearer {t}"));
    }
    if let Some((k, v)) = extra {
        q = q.header(k, v);
    }
    let q = match body {
        Some((Some(ct), b)) => q.header("content-type", ct).body(Body::from(b)),
        Some((None, b)) => q.body(Body::from(b)),
        None => q.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&b)})))
}

async fn ok(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, m, uri, Some(tok), body).await;
    assert_eq!(s, 200, "{m} {uri}: {v}");
    v
}

async fn onboard(r: &axum::Router, username: &str, name: &str, role: &str, clients: Value) -> (String, String) {
    let u = ok(
        r,
        "POST",
        "/api/staff/users",
        "k",
        Some(json!({"username":username,"display_name":name,"role":role,"clients":clients,"temporary_password":PW})),
    )
    .await;
    let id = u["id"].as_str().unwrap().to_string();
    ok(r, "POST", &format!("/api/staff/users/{id}"), "k", Some(json!({"phone":"780-555-0199"}))).await;
    let ip = format!("10.2.{}.{}", username.len(), username.bytes().map(u32::from).sum::<u32>() % 250);
    let mut q = Request::builder()
        .method("POST")
        .uri("/api/staff/login/sms")
        .header("x-forwarded-for", &ip)
        .header("content-type", "application/json")
        .body(Body::from(json!({"username":username,"password":PW}).to_string()))
        .unwrap();
    let _ = r.clone().oneshot(q).await.unwrap();
    q = Request::builder()
        .method("POST")
        .uri("/api/staff/login")
        .header("x-forwarded-for", &ip)
        .header("content-type", "application/json")
        .body(Body::from(json!({"username":username,"password":PW,"sms_code":SMS}).to_string()))
        .unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let b = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let login: Value = serde_json::from_slice(&b).unwrap();
    let tok = login["token"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    (id, tok)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stripe_checkout_webhook_marks_fee_paid_posts_journal_refunds_and_is_idempotent() {
    let (stripe_base, mock, _server) = mock_stripe().await;
    // Serialize env mutation against other tests that touch Stripe flags.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _env_guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::set_var("NXT_SMS_MOCK_CODE", SMS);
        std::env::remove_var("NXT_STRIPE_ENABLED");
        std::env::remove_var("STRIPE_SECRET_KEY");
        std::env::remove_var("STRIPE_WEBHOOK_SECRET");
        std::env::remove_var("NXT_STRIPE_LIVE_APPROVED");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);

    // Default off.
    let off = ok(&r, "GET", "/api/tax/stripe/quote", "k", None).await;
    assert_eq!(off["enabled"], false);
    let (ws, _) = call_raw(
        &r,
        "POST",
        "/api/tax/stripe/webhook",
        None,
        Some((Some("application/json"), json!({"id":"evt_off","type":"checkout.session.completed"}).to_string())),
        Some(("stripe-signature", "t=1,v1=x")),
    )
    .await;
    assert_eq!(ws, 503);

    unsafe {
        std::env::set_var("NXT_STRIPE_ENABLED", "true");
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_synthetic");
        std::env::set_var("STRIPE_WEBHOOK_SECRET", WHSEC);
        std::env::set_var("NXT_STRIPE_API_BASE", &stripe_base);
        std::env::set_var("NXT_STRIPE_FEE_CENTS", FEE.to_string());
        std::env::set_var("NXT_STRIPE_CURRENCY", "cad");
    }

    let quote = ok(&r, "GET", "/api/tax/stripe/quote", "k", None).await;
    assert_eq!(quote["enabled"], true);
    assert_eq!(quote["fee_cents"], FEE);

    let (_prep_id, prep) = onboard(&r, "stripe_prep", "Pat Preparer", "preparer", json!(["stripe-client-1"])).await;
    let (_adm_id, adm) = onboard(&r, "stripe_adm", "Ada Admin", "admin", json!(["*"])).await;

    let ret = ok(
        &r,
        "POST",
        "/api/tax/returns",
        &prep,
        Some(json!({
            "program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic Stripe Filer",
            "client_ref":"stripe-client-1",
            "inputs":{"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000},
            "notes":"Synthetic only."
        })),
    )
    .await;
    let rid = ret["id"].as_str().unwrap().to_string();

    let status0 = ok(&r, "GET", &format!("/api/tax/returns/{rid}/stripe"), &prep, None).await;
    assert_eq!(status0["fee_paid"], false);
    assert_eq!(status0["stripe_enabled"], true);

    let sess = ok(&r, "POST", &format!("/api/tax/returns/{rid}/stripe/checkout"), &prep, Some(json!({}))).await;
    let sid = sess["session_id"].as_str().unwrap().to_string();
    assert_eq!(sess["fee_cents"], FEE);
    assert!(sess["url"].as_str().unwrap().contains(&sid));
    assert_eq!(sess["reused"], false);

    // Reuse open session.
    let again = ok(&r, "POST", &format!("/api/tax/returns/{rid}/stripe/checkout"), &prep, Some(json!({}))).await;
    assert_eq!(again["session_id"], sid);
    assert_eq!(again["reused"], true);

    // Break-glass cannot refund; preparer cannot refund.
    assert_eq!(call(&r, "POST", &format!("/api/tax/returns/{rid}/stripe/refund"), Some("k"), Some(json!({}))).await.0, 403);
    assert_eq!(call(&r, "POST", &format!("/api/tax/returns/{rid}/stripe/refund"), Some(&prep), Some(json!({}))).await.0, 403);

    // Complete checkout via signed webhook.
    let session = mock.sessions.lock().unwrap().get(&sid).cloned().unwrap();
    let pi = session["payment_intent"].as_str().unwrap().to_string();
    // Mark session completed in mock for completeness.
    {
        let mut g = mock.sessions.lock().unwrap();
        let s = g.get_mut(&sid).unwrap();
        s["status"] = json!("complete");
    }
    let event = json!({
        "id": "evt_test_1",
        "type": "checkout.session.completed",
        "data": {"object": {
            "id": sid,
            "amount_total": FEE,
            "currency": "cad",
            "client_reference_id": rid,
            "metadata": {"return_id": rid, "product": "t1_filing_fee"},
            "payment_intent": pi
        }}
    });
    let body = event.to_string();
    let sig = sign(WHSEC, body.as_bytes());
    let (s1, v1) = call_raw(
        &r,
        "POST",
        "/api/tax/stripe/webhook",
        None,
        Some((Some("application/json"), body.clone())),
        Some(("stripe-signature", &sig)),
    )
    .await;
    assert_eq!(s1, 200, "{v1}");
    assert_eq!(v1["received"], true);
    assert_eq!(v1["duplicate"], Value::Null);

    // Idempotent replay.
    let (s2, v2) = call_raw(
        &r,
        "POST",
        "/api/tax/stripe/webhook",
        None,
        Some((Some("application/json"), body)),
        Some(("stripe-signature", &sig)),
    )
    .await;
    assert_eq!(s2, 200, "{v2}");
    assert_eq!(v2["duplicate"], true);

    // Bad signature refused.
    assert_eq!(
        call_raw(
            &r,
            "POST",
            "/api/tax/stripe/webhook",
            None,
            Some((Some("application/json"), json!({"id":"evt_bad","type":"checkout.session.completed","data":{"object":{}}}).to_string())),
            Some(("stripe-signature", "t=1,v1=dead")),
        )
        .await
        .0,
        401
    );

    let status1 = ok(&r, "GET", &format!("/api/tax/returns/{rid}/stripe"), &prep, None).await;
    assert_eq!(status1["fee_paid"], true);
    assert_eq!(status1["payment"]["status"], "paid");
    assert_eq!(status1["payment"]["stripe_fee_cents"], 579);
    let je = status1["payment"]["journal_entry_id"].as_str().unwrap().to_string();
    assert!(!je.is_empty());

    // Desk fee gate sees paid.
    let desk = ok(&r, "GET", &format!("/api/tax/returns/{rid}/desk"), &adm, None).await;
    assert_eq!(desk["fee_paid"], true);

    // Journal exists on the firm ledger.
    let entries = ok(&r, "GET", "/api/ledger/entries", "k", None).await;
    let list = entries.as_array().cloned().or_else(|| entries["entries"].as_array().cloned()).unwrap_or_default();
    let entry = list.iter().find(|e| e["id"] == je || e["entry"]["id"] == je || e["id"].as_str() == Some(je.as_str())).unwrap_or_else(|| panic!("missing journal {je} in {entries}"));
    let lines = entry["lines"].as_array().or_else(|| entry["entry"]["lines"].as_array()).unwrap();
    let debit: i64 = lines.iter().map(|l| l["debit"].as_i64().unwrap_or(0)).sum();
    let credit: i64 = lines.iter().map(|l| l["credit"].as_i64().unwrap_or(0)).sum();
    assert_eq!(debit, credit);
    assert_eq!(debit, FEE);

    // Admin refund reverses fee + journal.
    let refunded = ok(&r, "POST", &format!("/api/tax/returns/{rid}/stripe/refund"), &adm, Some(json!({}))).await;
    assert_eq!(refunded["fee_paid"], false);
    assert!(!refunded["refund_id"].as_str().unwrap().is_empty());

    let status2 = ok(&r, "GET", &format!("/api/tax/returns/{rid}/stripe"), &prep, None).await;
    assert_eq!(status2["fee_paid"], false);
    assert_eq!(status2["payment"]["status"], "refunded");

    // Second refund conflicts.
    assert_eq!(call(&r, "POST", &format!("/api/tax/returns/{rid}/stripe/refund"), Some(&adm), Some(json!({}))).await.0, 409);

    // Live key refused when enabled with sk_live_ and no approval.
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_live_should_refuse");
    }
    let bad = ok(&r, "GET", "/api/tax/stripe/quote", "k", None).await;
    assert_eq!(bad["enabled"], false);
    assert!(bad["disabled_reason"].as_str().unwrap().contains("LIVE_APPROVED"));
    unsafe {
        std::env::set_var("STRIPE_SECRET_KEY", "sk_test_synthetic");
    }
}

