//! Universal Nixon entry points ship disabled: with the flag off the start URL and source
//! recording refuse; with it on (internal only) every channel lands on the tax-request start
//! and the source is recorded on the synthetic intake. Unauthenticated callers never get in.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.9");
    if !tok.is_empty() { q = q.header("authorization", format!("Bearer {tok}")); }
    let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

#[tokio::test]
async fn entry_points_are_disabled_by_default_and_record_source_when_enabled() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::remove_var("NXT_NIXON_ENTRY_ENABLED");
    }
    let r = app_with(Store::memory);
    let (s, _) = call(&r, "GET", "/api/nixon/start?source=google", "", None).await;
    assert_eq!(s, 401, "no public access");
    let (s, v) = call(&r, "GET", "/api/nixon/start?source=google", "k", None).await;
    assert_eq!(s, 403, "{v}");
    let (s, _) = call(&r, "POST", "/api/journeys", "k", Some(json!({"client_id":"TEST-ORVILLE-OFF","entry":{"source":"google"}}))).await;
    assert_eq!(s, 403);
    // Without entry fields intake creation is unchanged.
    let (s, v) = call(&r, "POST", "/api/journeys", "k", Some(json!({"client_id":"TEST-ORVILLE-PLAIN"}))).await;
    assert_eq!((s, v["entry"].clone()), (200, Value::Null));

    unsafe { std::env::set_var("NXT_NIXON_ENTRY_ENABLED", "true"); }
    for (q, ch) in [("source=google&utm_campaign=t1-2025", "google"), ("utm_source=facebook&utm_medium=social", "facebook"),
                    ("utm_source=ig", "instagram"), ("source=linkedin", "linkedin"), ("utm_source=twitter", "x"),
                    ("utm_medium=cpc&utm_source=newsletter", "ads"), ("", "link")] {
        let (s, v) = call(&r, "GET", &format!("/api/nixon/start?{q}"), "k", None).await;
        assert_eq!(s, 200, "{q}: {v}");
        assert_eq!(v["entry"]["channel"], ch, "{q}");
        assert_eq!(v["brand"], "Nixon");
        assert_eq!(v["entry"]["landing"], "tax_request_start");
    }
    let (s, _) = call(&r, "GET", "/api/nixon/start?source=%3Cscript%3E", "k", None).await;
    assert_eq!(s, 400);
    let (s, v) = call(&r, "POST", "/api/journeys", "k", Some(json!({"client_id":"TEST-ORVILLE","entry":{"utm_source":"google","utm_campaign":"t1-2025","gclid":"synthetic123"}}))).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["entry"]["channel"], "google");
    assert_eq!(v["entry"]["click_ids"], json!(["gclid"]));
    assert!(!v.to_string().contains("synthetic123"), "click ids are not stored");
    let (_, d) = call(&r, "GET", "/api/journeys/TEST-ORVILLE", "k", None).await;
    assert_eq!(d["entry"]["utm_campaign"], "t1-2025");
    unsafe { std::env::remove_var("NXT_NIXON_ENTRY_ENABLED"); }
}
