//! Nixon voice: a signed-in staff member can start (or reopen) a T1 draft for a named client.
//! Person-name rules match the Nixon decision rules; the shared key can't; nothing is filed.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, firm::staff, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.7");
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
async fn say(r: &axum::Router, tok: &str, text: &str) -> Value {
    ok(r, "POST", "/api/nixon/voice", tok, Some(json!({"transcript": text}))).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn staff_can_start_or_open_a_t1_by_voice_for_a_named_client() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
        std::env::remove_var("OPENAI_API_KEY");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let r = app_with(Store::memory);
    ok(&r, "POST", "/api/staff/users", "k", Some(json!({"username":"voiceadm","display_name":"Test Admin","role":"admin","clients":["*"],"temporary_password":PW}))).await;
    let tok = ok(&r, "POST", "/api/staff/login", "", Some(json!({"username":"voiceadm","password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(&r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(&r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(&r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;

    // The shared key is not a signed-in person.
    let a = say(&r, "k", "Start a tax return for Orville Test").await;
    assert_eq!(a["intent"], "start_tax_return");
    assert_eq!(a["effect"], "none");
    assert!(a["say"].as_str().unwrap().contains("Sign in"), "{a}");

    let a = say(&r, &tok, "Nixon, start a tax return for Orville Test please.").await;
    assert_eq!((a["intent"].as_str(), a["effect"].as_str(), a["filing"].as_str()), (Some("start_tax_return"), Some("tax_return_drafted"), Some("disabled")), "{a}");
    assert_eq!(a["open"]["app"], "tax");
    let view = a["open"]["view"].as_str().unwrap().to_string();
    let id = view.strip_prefix("return/").unwrap().to_string();
    let ret = ok(&r, "GET", &format!("/api/tax/returns/{id}"), "k", None).await;
    let ret = if ret["return"].is_object() { ret["return"].clone() } else { ret };
    assert_eq!((ret["program"].as_str(), ret["name"].as_str(), ret["status"].as_str(), ret["tax_year"].as_i64()), (Some("T1"), Some("Orville Test"), Some("draft"), Some(2025)), "{ret}");
    assert!(ret["notes"].as_str().unwrap().contains("Test Admin ["), "{ret}");

    // Saying it again opens the same draft instead of creating another.
    let again = say(&r, &tok, "open the T1 for orville test").await;
    assert_eq!(again["intent"], "start_tax_return");
    assert_eq!(again["open"]["view"], view.as_str());
    assert_eq!(again["effect"], "none");
    let list = ok(&r, "GET", "/api/tax/returns?program=T1", "k", None).await;
    let rows = list.as_array().cloned().or_else(|| list["returns"].as_array().cloned()).unwrap();
    assert_eq!(rows.iter().filter(|x| x["name"] == "Orville Test").count(), 1);

    // Person-name rules: no roles, no Nixon, no digits; a missing name asks whose.
    for bad in ["Start a tax return for Nixon", "start a tax return for admin", "start a T1 for client 42"] {
        let a = say(&r, &tok, bad).await;
        assert_eq!((a["intent"].as_str(), a["effect"].as_str()), (Some("start_tax_return"), Some("none")), "{bad}: {a}");
        assert!(a["open"].is_null(), "{bad}");
    }
    // Filing by voice is still refused.
    assert_eq!(say(&r, &tok, "file the tax return for Orville Test").await["intent"], "refuse_file");
}
