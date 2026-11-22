//! Firm shift roster: planning records, not time entries or payroll.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k");
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn shift(staff: &str, date: &str, start: &str, end: &str) -> Value {
    json!({"staff": staff, "date": date, "start": start, "end": end, "note": "Front desk"})
}

#[tokio::test]
async fn shifts_are_saved_sorted_replaced_and_deleted() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, saved) = call(&r, "POST", "/api/firm/shifts", Some(shift("Dana", "2026-09-02", "09:00", "17:00"))).await;
    assert_eq!(status, 200, "{saved}");
    call(&r, "POST", "/api/firm/shifts", Some(shift("Ari", "2026-09-01", "08:00", "12:00"))).await;
    let (_, list) = call(&r, "GET", "/api/firm/shifts", None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["staff"], "Ari", "sorted by date then start");
    assert_eq!(list[1]["staff"], "Dana");

    // Re-posting with the same id replaces the shift instead of duplicating it.
    let mut edited = shift("Dana", "2026-09-02", "10:00", "18:00");
    edited["id"] = saved["id"].clone();
    let (status, _) = call(&r, "POST", "/api/firm/shifts", Some(edited)).await;
    assert_eq!(status, 200);
    let (_, list) = call(&r, "GET", "/api/firm/shifts", None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[1]["start"], "10:00");

    let (status, _) = call(&r, "DELETE", &format!("/api/firm/shifts/{}", saved["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&r, "DELETE", &format!("/api/firm/shifts/{}", saved["id"].as_str().unwrap()), None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn shift_input_is_validated() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    for (body, why) in [
        (shift("Dana", "2026-09-31", "09:00", "17:00"), "bad date"),
        (shift("Dana", "2026-09-01", "9:00", "17:00"), "short time"),
        (shift("Dana", "2026-09-01", "25:00", "26:00"), "invalid hour"),
        (shift("Dana", "2026-09-01", "17:00", "09:00"), "ends before it starts"),
        (shift("   ", "2026-09-01", "09:00", "17:00"), "blank staff"),
    ] {
        let (status, value) = call(&r, "POST", "/api/firm/shifts", Some(body)).await;
        assert_eq!(status, 400, "{why}: {value}");
    }
    let (_, list) = call(&r, "GET", "/api/firm/shifts", None).await;
    assert_eq!(list, json!([]));
}
