//! Recurring / memorized journal templates: save, list, post and advance, delete.
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

fn template(next_date: &str, frequency: &str) -> Value {
    json!({
        "name": "Monthly bank fee", "frequency": frequency, "next_date": next_date, "memo": "Bank fee",
        "lines": [
            {"account_id": "acc_6100", "debit": 5000},
            {"account_id": "acc_1000", "credit": 5000}
        ]
    })
}

#[tokio::test]
async fn recurring_template_posts_and_advances_without_posting_automatically() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let (status, saved) = call(&r, "POST", "/api/ledger/recurring", Some(template("2026-12-01", "monthly"))).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["next_date"], "2026-12-01");
    assert_eq!(saved["last_posted"], Value::Null);

    // Saving a template does not touch the books.
    let (_, entries) = call(&r, "GET", "/api/ledger/entries", None).await;
    assert_eq!(entries.as_array().unwrap().len(), 0);

    let id = saved["id"].as_str().unwrap().to_string();
    let (status, entry) = call(&r, "POST", &format!("/api/ledger/recurring/{id}/post"), Some(json!({}))).await;
    assert_eq!(status, 200, "{entry}");
    assert_eq!(entry["date"], "2026-12-01");
    assert_eq!(entry["memo"], "Bank fee");
    assert_eq!(entry["source"], "journal");

    let (_, list) = call(&r, "GET", "/api/ledger/recurring", None).await;
    assert_eq!(list[0]["next_date"], "2027-01-01");
    assert_eq!(list[0]["last_posted"], "2026-12-01");

    let (status, _) = call(&r, "POST", &format!("/api/ledger/recurring/{id}/post"), Some(json!({}))).await;
    assert_eq!(status, 200);
    let (_, list) = call(&r, "GET", "/api/ledger/recurring", None).await;
    assert_eq!(list[0]["next_date"], "2027-02-01");
    let (_, entries) = call(&r, "GET", "/api/ledger/entries", None).await;
    assert_eq!(entries.as_array().unwrap().len(), 2);

    let (status, deleted) = call(&r, "DELETE", &format!("/api/ledger/recurring/{id}"), None).await;
    assert_eq!(status, 200, "{deleted}");
    let (_, list) = call(&r, "GET", "/api/ledger/recurring", None).await;
    assert_eq!(list, json!([]));
    let (status, _) = call(&r, "DELETE", &format!("/api/ledger/recurring/{id}"), None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn month_end_template_clamps_to_the_shorter_month() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, saved) = call(&r, "POST", "/api/ledger/recurring", Some(template("2027-01-31", "monthly"))).await;
    assert_eq!(status, 200, "{saved}");
    let id = saved["id"].as_str().unwrap().to_string();
    let (status, _) = call(&r, "POST", &format!("/api/ledger/recurring/{id}/post"), Some(json!({}))).await;
    assert_eq!(status, 200);
    let (_, list) = call(&r, "GET", "/api/ledger/recurring", None).await;
    assert_eq!(list[0]["next_date"], "2027-02-28");
    let (status, _) = call(&r, "POST", &format!("/api/ledger/recurring/{id}/post"), Some(json!({}))).await;
    assert_eq!(status, 200);
    let (_, list) = call(&r, "GET", "/api/ledger/recurring", None).await;
    assert_eq!(list[0]["next_date"], "2027-03-28");
}

#[tokio::test]
async fn recurring_template_input_is_validated_before_it_is_stored() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let mut unbalanced = template("2026-12-01", "monthly");
    unbalanced["lines"][1]["credit"] = json!(4000);
    let (status, value) = call(&r, "POST", "/api/ledger/recurring", Some(unbalanced)).await;
    assert_eq!(status, 400, "{value}");

    let mut unknown = template("2026-12-01", "monthly");
    unknown["lines"][0]["account_id"] = json!("acc_9999");
    let (status, _) = call(&r, "POST", "/api/ledger/recurring", Some(unknown)).await;
    assert_eq!(status, 400);

    let (status, _) = call(&r, "POST", "/api/ledger/recurring", Some(template("2020-01-01", "monthly"))).await;
    assert_eq!(status, 400, "past next_date");
    let (status, _) = call(&r, "POST", "/api/ledger/recurring", Some(template("2026-12-01", "fortnightly"))).await;
    assert_eq!(status, 400, "unknown frequency");
    let mut unnamed = template("2026-12-01", "monthly");
    unnamed["name"] = json!("   ");
    let (status, _) = call(&r, "POST", "/api/ledger/recurring", Some(unnamed)).await;
    assert_eq!(status, 400);

    let (_, list) = call(&r, "GET", "/api/ledger/recurring", None).await;
    assert_eq!(list, json!([]));
}
