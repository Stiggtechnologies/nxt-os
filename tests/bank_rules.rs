//! Bank-rule management: explicit list/create/replace/delete over the rules the
//! bank feed uses to suggest a category.
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

fn suggestion(feed: &Value, description: &str) -> Value {
    feed.as_array()
        .unwrap()
        .iter()
        .find(|row| row["item"]["description"] == description)
        .unwrap_or_else(|| panic!("no feed row for {description}"))["suggestion"]
        .clone()
}

#[tokio::test]
async fn bank_rules_are_listed_saved_replaced_and_deleted() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    let (status, rules) = call(&r, "GET", "/api/ledger/rules", None).await;
    assert_eq!(status, 200);
    assert_eq!(rules, json!([]));

    // Keywords are trimmed and lowercased before they are stored.
    let (status, saved) = call(&r, "POST", "/api/ledger/rules", Some(json!({
        "keyword": "  ADOBE  ", "account_id": "acc_6300", "tax_code": "GST"
    }))).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["keyword"], "adobe");
    assert_eq!(saved["account_id"], "acc_6300");
    assert_eq!(saved["tax_code"], "GST");

    // A more specific rule wins for a description that contains both keywords.
    let (status, _) = call(&r, "POST", "/api/ledger/rules", Some(json!({
        "keyword": "adobe creative cloud", "account_id": "acc_6600"
    }))).await;
    assert_eq!(status, 200);

    let (status, imported) = call(&r, "POST", "/api/ledger/feed/import", Some(json!({
        "account_id": "acc_1000",
        "csv": "2026-03-01,ADOBE CREATIVE CLOUD,-56.49\n2026-03-02,ADOBE STOCK,-20.00"
    }))).await;
    assert_eq!(status, 200, "{imported}");
    assert_eq!(imported["added"], 2);

    let (_, feed) = call(&r, "GET", "/api/ledger/feed", None).await;
    assert_eq!(suggestion(&feed, "ADOBE CREATIVE CLOUD")["account_id"], "acc_6600");
    assert_eq!(suggestion(&feed, "ADOBE STOCK")["account_id"], "acc_6300");
    assert_eq!(suggestion(&feed, "ADOBE STOCK")["tax_code"], "GST");

    // Re-posting the same keyword replaces the rule instead of duplicating it.
    let (status, _) = call(&r, "POST", "/api/ledger/rules", Some(json!({
        "keyword": "adobe", "account_id": "acc_6000", "tax_code": "NONE"
    }))).await;
    assert_eq!(status, 200);
    let (_, rules) = call(&r, "GET", "/api/ledger/rules", None).await;
    assert_eq!(rules.as_array().unwrap().len(), 2);
    assert_eq!(rules[1]["account_id"], "acc_6000");

    // Deleting a rule requires an exact stored keyword and reports a missing one.
    let (status, deleted) = call(&r, "DELETE", "/api/ledger/rules/adobe%20creative%20cloud", None).await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(deleted["deleted"], "adobe creative cloud");
    let (_, feed) = call(&r, "GET", "/api/ledger/feed", None).await;
    assert_eq!(suggestion(&feed, "ADOBE CREATIVE CLOUD")["account_id"], "acc_6000");
    let (status, _) = call(&r, "DELETE", "/api/ledger/rules/never-matches", None).await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn bank_rule_input_is_validated_before_it_is_stored() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    for (body, why) in [
        (json!({"keyword": "   ", "account_id": "acc_6300"}), "blank keyword"),
        (json!({"keyword": "x", "account_id": "acc_9999"}), "unknown account"),
        (json!({"keyword": "x", "account_id": "acc_1000"}), "bank account"),
        (json!({"keyword": "x", "account_id": "acc_6300", "tax_code": "VAT20"}), "unknown tax code"),
    ] {
        let (status, value) = call(&r, "POST", "/api/ledger/rules", Some(body)).await;
        assert_eq!(status, 400, "{why}: {value}");
    }
    let (status, _) = call(&r, "POST", "/api/ledger/rules", Some(json!({
        "keyword": &"k".repeat(81), "account_id": "acc_6300"
    }))).await;
    assert_eq!(status, 400);
    let (_, rules) = call(&r, "GET", "/api/ledger/rules", None).await;
    assert_eq!(rules, json!([]));
}
