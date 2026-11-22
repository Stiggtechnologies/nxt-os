//! Declared custom fields and validated per-contact values.
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

#[tokio::test]
async fn fields_are_declared_and_contact_values_validate_against_them() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    let (status, field) = call(&r, "POST", "/api/ledger/custom-fields", Some(json!({"key": "region", "label": "Region", "kind": "text"}))).await;
    assert_eq!(status, 200, "{field}");
    assert_eq!(field["kind"], "text");
    call(&r, "POST", "/api/ledger/custom-fields", Some(json!({"key": "seats", "label": "Seats", "kind": "number"}))).await;
    let (_, fields) = call(&r, "GET", "/api/ledger/custom-fields", None).await;
    assert_eq!(fields.as_array().unwrap().len(), 2);
    assert_eq!(fields[0]["label"], "Region");

    let (_, cust) = call(&r, "POST", "/api/ledger/contacts", Some(json!({"kind": "customer", "name": "Maple Co"}))).await;
    let cid = cust["id"].as_str().unwrap().to_string();
    let (status, values) = call(&r, "PUT", &format!("/api/ledger/contacts/{cid}/custom"),
        Some(json!({"values": {"region": "West", "seats": 12}}))).await;
    assert_eq!(status, 200, "{values}");
    assert_eq!(values["region"], "West");
    let (_, detail) = call(&r, "GET", &format!("/api/ledger/contacts/{cid}"), None).await;
    assert_eq!(detail["custom"]["region"], "West");
    assert_eq!(detail["custom"]["seats"], 12);

    for (body, why) in [
        (json!({"values": {"region": 5}}), "wrong kind"),
        (json!({"values": {"undeclared": "x"}}), "unknown key"),
    ] {
        let (status, value) = call(&r, "PUT", &format!("/api/ledger/contacts/{cid}/custom"), Some(body)).await;
        assert_eq!(status, 400, "{why}: {value}");
    }

    // An empty map clears the contact's values.
    let (status, _) = call(&r, "PUT", &format!("/api/ledger/contacts/{cid}/custom"), Some(json!({"values": {}}))).await;
    assert_eq!(status, 200);
    let (_, detail) = call(&r, "GET", &format!("/api/ledger/contacts/{cid}"), None).await;
    assert_eq!(detail["custom"], json!({}));
}

#[tokio::test]
async fn field_definitions_are_validated_and_deletable() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);
    for (body, why) in [
        (json!({"key": "Bad Key", "label": "Bad", "kind": "text"}), "bad key"),
        (json!({"key": "x", "label": "X", "kind": "money"}), "bad kind"),
        (json!({"key": "y", "label": "  ", "kind": "text"}), "blank label"),
    ] {
        let (status, value) = call(&r, "POST", "/api/ledger/custom-fields", Some(body)).await;
        assert_eq!(status, 400, "{why}: {value}");
    }
    call(&r, "POST", "/api/ledger/custom-fields", Some(json!({"key": "region", "label": "Region", "kind": "text"}))).await;
    let (status, value) = call(&r, "POST", "/api/ledger/custom-fields", Some(json!({"key": "zone", "label": "region", "kind": "text"}))).await;
    assert_eq!(status, 400, "duplicate label: {value}");

    let (status, _) = call(&r, "DELETE", "/api/ledger/custom-fields/region", None).await;
    assert_eq!(status, 200);
    let (status, _) = call(&r, "DELETE", "/api/ledger/custom-fields/region", None).await;
    assert_eq!(status, 404);
}
