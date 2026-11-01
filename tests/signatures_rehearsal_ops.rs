//! Internal folders, edit lock, bulk rehearsal, resend, correct, void, and decline.
//! No email, certificate, or legal effect.
use nxt_os::{signatures::Signatures, store::Store};
use serde_json::{json, Value};

fn draft() -> Value {
    json!({
        "title":"Synthetic rehearsal","email_subject":"Synthetic preparation only","email_message":"No message is sent.",
        "documents":[{"id":"doc1","name":"Original synthetic agreement","pages":["Original synthetic text.","Second original page."]}],
        "recipients":[{"id":"role1","role_name":"Client signer","name":"Synthetic Signer","email":"signer@example.invalid","kind":"signer","routing_order":1}],
        "fields":[
            {"id":"field1","document_id":"doc1","recipient_id":"role1","page":2,"kind":"signature","label":"Unfilled signature placeholder","required":true,"x":1000,"y":7500,"width":3500,"height":600},
            {"id":"field2","document_id":"doc1","recipient_id":"role1","page":1,"kind":"text","label":"Answer","required":false,"x":100,"y":100,"width":400,"height":200}
        ]
    })
}

async fn save(sig: &Signatures) -> Value {
    sig.save(None, serde_json::from_value(json!({"operator_label":"Synthetic operator","draft":draft()})).unwrap()).await.ok().unwrap()
}
fn env<'a>(value: &'a Value) -> &'a Value { &value["envelope"] }
async fn step(sig: &Signatures, current: &Value, action: &str, extra: Value) -> Value {
    let mut body = json!({
        "expected_revision": env(current)["revision"],
        "expected_content_hash": env(current)["content_hash"],
        "operator_label": "Synthetic operator",
        "action": action,
        "note": "Synthetic rehearsal note"
    });
    if let Some(object) = extra.as_object() {
        for (key, value) in object { body[key] = value.clone(); }
    }
    sig.transition(env(current)["id"].as_str().unwrap(), serde_json::from_value(body).unwrap()).await.ok().unwrap()
}

#[tokio::test]
async fn rehearsal_ops_stay_inside_nxt_without_delivery() {
    let sig = Signatures::new(Store::memory());
    let created = save(&sig).await;
    let review = step(&sig, &created, "submit_review", json!({})).await;
    let approved = step(&sig, &review, "approve", json!({})).await;
    let sent = step(&sig, &approved, "parallel_send", json!({})).await;
    assert_eq!(sent["external_delivery"], false);
    let resent = step(&sig, &sent, "parallel_resend", json!({})).await;
    assert_eq!(env(&resent)["status"], "parallel_sent");
    assert_eq!(env(&resent)["resend_count"], 1);
    assert_eq!(resent["external_delivery"], false);
    let rejected = sig.transition(env(&resent)["id"].as_str().unwrap(), serde_json::from_value(json!({
        "expected_revision": env(&resent)["revision"], "expected_content_hash": env(&resent)["content_hash"],
        "operator_label": "Synthetic operator", "action": "parallel_sign", "note": "Synthetic rehearsal note",
        "field_values": [{"field_id": "field1", "value": "not a signature"}]
    })).unwrap()).await.unwrap_err();
    assert_eq!(rejected.0, 409);
    let signed = step(&sig, &resent, "parallel_sign", json!({"field_values":[{"field_id":"field2","value":"Synthetic answer"}]})).await;
    assert_eq!(env(&signed)["status"], "parallel_signed");
    assert_eq!(env(&signed)["field_values"][0]["value"], "Synthetic answer");
    assert_eq!(signed["legal_effect"], false);
    assert_eq!(signed["completion_certificate"], false);
    assert!(sig.package(env(&signed)["id"].as_str().unwrap()).await.is_err());

    let second = save(&sig).await;
    let second = step(&sig, &second, "submit_review", json!({})).await;
    let second = step(&sig, &second, "approve", json!({})).await;
    let second = step(&sig, &second, "parallel_send", json!({})).await;
    let corrected = step(&sig, &second, "parallel_correct", json!({})).await;
    assert_eq!(env(&corrected)["status"], "draft");
    assert!(env(&corrected)["reviewed_content_hash"].is_null());

    let third = save(&sig).await;
    let third = step(&sig, &third, "submit_review", json!({})).await;
    let third = step(&sig, &third, "approve", json!({})).await;
    let third = step(&sig, &third, "parallel_send", json!({})).await;
    let voided = step(&sig, &third, "parallel_void", json!({})).await;
    assert_eq!(env(&voided)["status"], "parallel_voided");
    let fourth = save(&sig).await;
    let fourth = step(&sig, &fourth, "submit_review", json!({})).await;
    let fourth = step(&sig, &fourth, "approve", json!({})).await;
    let fourth = step(&sig, &fourth, "parallel_send", json!({})).await;
    let declined = step(&sig, &fourth, "parallel_decline", json!({})).await;
    assert_eq!(env(&declined)["status"], "parallel_declined");

    let folder = sig.create_folder("Synthetic reviews").await.ok().unwrap();
    assert_eq!(folder["mailbox_sharing"], false);
    let filed = sig.assign_folder(env(&signed)["id"].as_str().unwrap(), serde_json::from_value(json!({
        "expected_revision": env(&signed)["revision"], "expected_content_hash": env(&signed)["content_hash"],
        "operator_label": "Synthetic operator", "action": "assign_folder", "note": folder["id"]
    })).unwrap()).await.ok().unwrap();
    assert_eq!(env(&filed)["folder_id"], folder["id"]);

    let locked_source = save(&sig).await;
    let locked = sig.set_lock(env(&locked_source)["id"].as_str().unwrap(), serde_json::from_value(json!({
        "expected_revision": env(&locked_source)["revision"], "expected_content_hash": env(&locked_source)["content_hash"],
        "operator_label": "Synthetic operator", "action": "parallel_lock", "note": "Hold edits"
    })).unwrap(), true).await.ok().unwrap();
    assert!(env(&locked)["lock_token"].is_string());
    let edit = sig.save(Some(env(&locked)["id"].as_str().unwrap()), serde_json::from_value(json!({
        "expected_revision": env(&locked)["revision"], "operator_label": "Synthetic operator", "draft": draft()
    })).unwrap()).await.unwrap_err();
    assert_eq!(edit.0, 409);

    let template = sig.create_template(env(&corrected)["id"].as_str().unwrap(), serde_json::from_value(json!({
        "expected_revision": env(&corrected)["revision"], "expected_content_hash": env(&corrected)["content_hash"],
        "operator_label": "Synthetic operator", "name": "Synthetic favorite"
    })).unwrap()).await.ok().unwrap();
    let favorite = sig.favorite_template(template["template"]["id"].as_str().unwrap(), true).await.ok().unwrap();
    assert_eq!(favorite["template"]["favorite"], true);
    assert_eq!(favorite["external_share"], false);

    let bulk_source = save(&sig).await;
    let bulk_source = step(&sig, &bulk_source, "submit_review", json!({})).await;
    let bulk_source = step(&sig, &bulk_source, "approve", json!({})).await;
    let bulk = sig.bulk_rehearse(env(&bulk_source)["id"].as_str().unwrap(), serde_json::from_value(json!({
        "expected_revision": env(&bulk_source)["revision"], "expected_content_hash": env(&bulk_source)["content_hash"],
        "operator_label": "Synthetic operator", "action": "parallel_bulk", "note": "Synthetic bulk rehearsal"
    })).unwrap(), 2).await.ok().unwrap();
    assert_eq!(bulk["email_sent"], false);
    assert_eq!(bulk["legal_effect"], false);
    assert_eq!(bulk["ids"].as_array().unwrap().len(), 2);
    assert!(sig.package(bulk["ids"][0].as_str().unwrap()).await.is_err());
}
