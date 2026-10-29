//! Native e-sign ceremony on synthetic PDFs. Test delivery only: no email,
//! no external service. Synthetic names on example.invalid addresses.
use axum::{Router, body::Body, http::Request};
use base64::Engine;
use lopdf::{Document, Object, Stream, dictionary};
use nxt_os::{
    client_journey::*,
    signatures::{self, Signatures, esign},
    store::Store,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

pub fn pdf(pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let mut kids = vec![];
    for p in 0..pages {
        let content = doc.add_object(Stream::new(dictionary! {}, format!("BT /F1 12 Tf 72 720 Td (Synthetic page {}) Tj ET", p + 1).into_bytes()));
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,"Contents"=>content,
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()]});
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages as i64}));
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", catalog);
    let mut out = vec![];
    doc.save_to(&mut out).ok().unwrap();
    out
}

async fn req(r: &Router, method: &str, path: &str, body: Option<Value>, auth: bool) -> (u16, Value, Vec<u8>) {
    let mut b = Request::builder().method(method).uri(path).header("user-agent", "synthetic-test-agent").header("x-forwarded-for", "203.0.113.7, 10.0.0.1");
    if auth { b = b.header("authorization", "Bearer k"); }
    let body = match body { Some(v) => { b = b.header("content-type", "application/json"); Body::from(v.to_string()) } None => Body::empty() };
    let res = r.clone().oneshot(b.body(body).unwrap()).await.ok().unwrap();
    let st = res.status().as_u16();
    assert!(res.headers().get("cache-control").unwrap().to_str().unwrap().contains("no-store"), "{path}");
    let bytes = axum::body::to_bytes(res.into_body(), 16 << 20).await.ok().unwrap().to_vec();
    (st, serde_json::from_slice(&bytes).unwrap_or(Value::Null), bytes)
}
async fn ok(r: &Router, m: &str, p: &str, b: Option<Value>, auth: bool) -> Value {
    let (s, v, _) = req(r, m, p, b, auth).await;
    assert_eq!(s, 200, "{m} {p}: {v}");
    v
}
fn tr(env: &Value, action: &str) -> Value {
    json!({"expected_revision":env["revision"],"expected_content_hash":env["content_hash"],"operator_label":"Synthetic staff","action":action,"note":"Synthetic review"})
}
fn setup() -> (Arc<Signatures>, Router) {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k"); }
    let s = Arc::new(Signatures::new(Store::memory()));
    (s.clone(), signatures::router(s))
}
async fn prepared(r: &Router, key: &str, recipients: Value, pages: usize) -> (String, Vec<(String, String)>) {
    let v = ok(r, "POST", &format!("/api/signatures/esign/builtin/{key}"), Some(json!({"operator_label":"Synthetic staff","recipients":recipients})), true).await;
    let env = v["envelope"]["envelope"].clone();
    let id = env["id"].as_str().unwrap().to_string();
    let doc = env["draft"]["documents"][0]["id"].clone();
    let pdf64 = base64::engine::general_purpose::STANDARD.encode(pdf(pages));
    ok(r, "POST", &format!("/api/signatures/{id}/esign/pdf"), Some(json!({"operator_label":"Synthetic staff","expected_revision":env["revision"],"expected_content_hash":env["content_hash"],"document_id":doc,"pdf_base64":pdf64})), true).await;
    let rev = ok(r, "POST", &format!("/api/signatures/{id}/transition"), Some(tr(&env, "submit_review")), true).await["envelope"].clone();
    let app = ok(r, "POST", &format!("/api/signatures/{id}/transition"), Some(tr(&rev, "approve")), true).await["envelope"].clone();
    let sent = ok(r, "POST", &format!("/api/signatures/{id}/esign/send"), Some(json!({"operator_label":"Synthetic staff","expected_revision":app["revision"],"expected_content_hash":app["content_hash"]})), true).await;
    assert_eq!(sent["delivery"]["email_sent"], false);
    assert_eq!(sent["delivery"]["mode"], "test");
    let links = sent["delivery"]["links"].as_array().unwrap().iter()
        .map(|l| (l["recipient_id"].as_str().unwrap().to_string(), l["url"].as_str().unwrap().trim_start_matches("/sign/#").to_string())).collect();
    (id, links)
}
async fn verify(r: &Router, t: &str) {
    let code = ok(r, "POST", &format!("/api/sign/{t}/otp"), None, false).await["test_code"].as_str().unwrap().to_string();
    ok(r, "POST", &format!("/api/sign/{t}/verify"), Some(json!({"code":code})), false).await;
}
async fn sign_as(r: &Router, t: &str, name: &str) -> Value {
    verify(r, t).await;
    ok(r, "POST", &format!("/api/sign/{t}/consent"), Some(json!({"accept":true,"consent_version":esign::CONSENT_VERSION})), false).await;
    ok(r, "POST", &format!("/api/sign/{t}/adopt"), Some(json!({"kind":"typed","text":name,"initials":"SS"})), false).await;
    ok(r, "POST", &format!("/api/sign/{t}/sign"), Some(json!({"values":[]})), false).await
}
fn human() -> Actor { Actor::Human { name: "R".into(), role: "CPA".into() } }

#[tokio::test]
async fn engagement_letter_full_ceremony_routing_flatten_certificate_and_gate() {
    let (s, r) = setup();
    let (id, links) = prepared(&r, "engagement_letter", json!([{"id":"client","name":"Sam Synthetic","email":"client@example.invalid"},{"id":"professional","name":"Pat Preparer","email":"pro@example.invalid"}]), 1).await;
    let pro = &links.iter().find(|l| l.0 == "professional").unwrap().1;
    let client = &links.iter().find(|l| l.0 == "client").unwrap().1;
    assert_eq!(req(&r, "POST", &format!("/api/sign/{pro}/otp"), None, false).await.0, 409, "routing order");
    assert_eq!(req(&r, "GET", &format!("/api/sign/{client}/document"), None, false).await.0, 403, "no PDF before identity check");
    assert_eq!(req(&r, "GET", &format!("/api/signatures/{id}/esign"), None, false).await.0, 401);
    sign_as(&r, client, "Sam Synthetic").await;
    let done = sign_as(&r, pro, "Pat Preparer").await;
    assert_eq!(done["step"], "completed", "{done}");
    let c = s.ceremony_record(&id).await.ok().unwrap();
    assert!(c.verify_chain());
    assert!(c.events.iter().any(|e| e.ip == "203.0.113.7" && e.user_agent == "synthetic-test-agent"));
    for which in ["original", "signed", "certificate"] {
        let (st, _, bytes) = req(&r, "GET", &format!("/api/signatures/{id}/esign/document/{which}"), None, true).await;
        assert_eq!(st, 200);
        assert!(bytes.starts_with(b"%PDF-"));
        Document::load_mem(&bytes).ok().unwrap();
    }
    let signed = s.completed_pdf(&id, "signed").await.ok().unwrap();
    assert_eq!(hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&signed)), c.signed_pdf_sha256.clone().unwrap());
    let d = Document::load_mem(&signed).ok().unwrap();
    let text = String::from_utf8_lossy(&d.get_page_content(*d.get_pages().get(&1).unwrap()).ok().unwrap()).to_string();
    assert!(text.contains("Sam Synthetic") && text.contains("Pat Preparer") && text.contains(&id), "flattened values present");
    let cert = s.completed_pdf(&id, "certificate").await.ok().unwrap();
    let cd = Document::load_mem(&cert).ok().unwrap();
    let all: String = cd.get_pages().values().map(|p| String::from_utf8_lossy(&cd.get_page_content(*p).ok().unwrap()).to_string()).collect();
    assert!(all.contains("203.0.113.7") && all.contains("signed"));
    assert_eq!(req(&r, "POST", &format!("/api/sign/{client}/otp"), None, false).await.0, 409);

    let def = JourneyDef::load().ok().unwrap();
    let bot = || Actor::Automation { executor: "test".into() };
    let mut j = ClientJourney::new(&def, "synthetic");
    for k in ["website_visit", "lead_capture", "needs_discovery", "service_scoping", "proposal_pricing", "engagement_letter"] {
        j.start(&def, k, bot(), "t").ok().unwrap(); j.finish_work(&def, k, bot(), "t").ok().unwrap();
    }
    assert!(esign::approve_journey_gate(&mut j, &def, &c, bot(), "t").is_err());
    let mut tampered = c.clone();
    tampered.events[1].detail = "altered".into();
    assert!(esign::approve_journey_gate(&mut j, &def, &tampered, human(), "t").is_err());
    esign::approve_journey_gate(&mut j, &def, &c, human(), "t").ok().unwrap();
    assert_eq!(j.status_of("engagement_letter"), Some(StepStatus::Completed));
}

#[tokio::test]
async fn otp_lockout_decline_void_and_reissue() {
    let (s, r) = setup();
    let (id, links) = prepared(&r, "t183", json!([{"id":"taxpayer","name":"Sam Synthetic","email":"tp@example.invalid"}]), 2).await;
    let t = &links[0].1;
    assert_eq!(req(&r, "GET", "/api/sign/notatoken", None, false).await.0, 410);
    assert_eq!(req(&r, "POST", &format!("/api/sign/{t}/sign"), Some(json!({"values":[]})), false).await.0, 403);
    ok(&r, "POST", &format!("/api/sign/{t}/otp"), None, false).await;
    for _ in 0..5 { assert_ne!(req(&r, "POST", &format!("/api/sign/{t}/verify"), Some(json!({"code":"000000x"})), false).await.0, 200); }
    assert_ne!(req(&r, "POST", &format!("/api/sign/{t}/verify"), Some(json!({"code":"123456"})), false).await.0, 200, "locked out");
    let new = ok(&r, "POST", &format!("/api/signatures/{id}/esign/link"), Some(json!({"operator_label":"Synthetic staff","recipient_id":"taxpayer"})), true).await;
    let t2 = new["delivery"]["links"][0]["url"].as_str().unwrap().trim_start_matches("/sign/#").to_string();
    assert_eq!(req(&r, "GET", &format!("/api/sign/{t}"), None, false).await.0, 410);
    verify(&r, &t2).await;
    let declined = ok(&r, "POST", &format!("/api/sign/{t2}/decline"), Some(json!({"reason":"Synthetic decline"})), false).await;
    assert_eq!(declined["step"], "declined", "{declined}");
    assert!(s.ceremony_record(&id).await.ok().unwrap().verify_chain());

    let (s2, r2) = setup();
    let (id2, l2) = prepared(&r2, "t183corp", json!([{"id":"officer","name":"Pat Officer","email":"o@example.invalid"}]), 2).await;
    ok(&r2, "POST", &format!("/api/signatures/{id2}/esign/void"), Some(json!({"operator_label":"Synthetic staff","reason":"Synthetic void"})), true).await;
    assert_eq!(req(&r2, "POST", &format!("/api/sign/{}/otp", l2[0].1), None, false).await.0, 409);
    assert_eq!(s2.ceremony_record(&id2).await.ok().unwrap().status, "voided");
}

#[tokio::test]
async fn refuses_unsafe_inputs() {
    let (_s, r) = setup();
    let v = ok(&r, "POST", "/api/signatures/esign/builtin/t183", Some(json!({"operator_label":"Synthetic staff","recipients":[{"id":"taxpayer","name":"Sam Synthetic","email":"tp@example.invalid"}]})), true).await;
    let env = v["envelope"]["envelope"].clone();
    let id = env["id"].as_str().unwrap();
    let up = |b64: String| json!({"operator_label":"Synthetic staff","expected_revision":env["revision"],"expected_content_hash":env["content_hash"],"document_id":"doc","pdf_base64":b64});
    let e = base64::engine::general_purpose::STANDARD;
    assert_eq!(req(&r, "POST", &format!("/api/signatures/{id}/esign/pdf"), Some(up(e.encode(b"not a pdf"))), true).await.0, 400);
    assert_eq!(req(&r, "POST", &format!("/api/signatures/{id}/esign/pdf"), Some(up(e.encode(pdf(1)))), true).await.0, 400, "fields on page 2");
    ok(&r, "POST", &format!("/api/signatures/{id}/esign/pdf"), Some(up(e.encode(pdf(2)))), true).await;
    assert_eq!(req(&r, "POST", &format!("/api/signatures/{id}/esign/send"), Some(json!({"operator_label":"Synthetic staff","expected_revision":env["revision"],"expected_content_hash":env["content_hash"]})), true).await.0, 409, "not approved");
    let cat = ok(&r, "GET", "/api/signatures/esign/catalog", None, true).await;
    assert!(cat["templates"].to_string().contains("t183corp"));
}
