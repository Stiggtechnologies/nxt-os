//! End-to-end acceptance on synthetic data only: client journey -> engagement ->
//! company ledger -> tax calculate -> preparer/reviewer sign-off -> T183CORP
//! envelope signed in NXT Signatures -> filed status recorded.
//! No client data, no external send (test delivery mode), no CRA filing: "filed"
//! is the human record of a filing done outside NXT.
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

fn pdf(pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let mut kids = vec![];
    for p in 0..pages {
        let content = doc.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf 72 720 Td (Synthetic page {}) Tj ET", p + 1).into_bytes(),
        ));
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages_id,"Contents"=>content,
            "Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()]});
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages as i64}),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", catalog);
    let mut out = vec![];
    doc.save_to(&mut out).unwrap();
    out
}

async fn call(
    r: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("user-agent", "synthetic-e2e")
        .header("x-forwarded-for", "203.0.113.9");
    if auth {
        b = b.header("authorization", "Bearer k");
    }
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = r.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let st = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), 16 << 20)
        .await
        .unwrap();
    (
        st,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)})),
    )
}
async fn ok(r: &Router, m: &str, p: &str, b: Option<Value>) -> Value {
    let (s, v) = call(r, m, p, b, true).await;
    assert_eq!(s, 200, "{m} {p}: {v}");
    v
}
async fn public(r: &Router, m: &str, p: &str, b: Option<Value>) -> Value {
    let (s, v) = call(r, m, p, b, false).await;
    assert_eq!(s, 200, "{m} {p}: {v}");
    v
}
async fn fails(r: &Router, m: &str, p: &str, b: Option<Value>, want: u16) -> Value {
    let (s, v) = call(r, m, p, b, true).await;
    assert_eq!(s, want, "{m} {p}: {v}");
    v
}
fn rev(v: &Value) -> u64 {
    v["revision"].as_u64().expect("revision")
}
fn eng(v: &Value, s: &str) -> String {
    format!("/api/engagements/{}{s}", v["id"].as_str().unwrap())
}
fn tax(v: &Value, s: &str) -> String {
    format!("/api/tax/returns/{}{s}", v["id"].as_str().unwrap())
}
fn bot() -> Actor {
    Actor::Automation {
        executor: "e2e".into(),
    }
}
fn cpa() -> Actor {
    Actor::Human {
        name: "Synthetic reviewer".into(),
        role: "CPA".into(),
    }
}

fn router() -> (Arc<Signatures>, Router) {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::remove_var("NXT_ESIGN_DELIVERY");
    }
    let ledger = Arc::new(nxt_os::ledger::engine::Ledger::new(Store::memory()));
    let firm = Arc::new(nxt_os::firm::Firm::new(Store::memory(), ledger.clone()));
    let tax = Arc::new(nxt_os::tax::Tax::new(Store::memory()));
    let sigs = Arc::new(Signatures::new(Store::memory()));
    tax.require_signed_t183_from(sigs.clone()); // as wired by the production router
    let engagements = Arc::new(nxt_os::engagement::Engagements::new(
        Store::memory(),
        ledger.clone(),
        tax.clone(),
        firm.clone(),
    ));
    let r = Router::new()
        .merge(nxt_os::ledger::routes::router(ledger))
        .merge(nxt_os::firm::router(firm))
        .merge(nxt_os::tax::router(tax))
        .merge(nxt_os::engagement::router(engagements))
        .merge(signatures::router(sigs.clone()));
    (sigs, r)
}

fn advance(def: &JourneyDef, j: &mut ClientJourney, until: &str) {
    for s in def.steps.clone() {
        if s.key == until {
            break;
        }
        if j.status_of(&s.key) == Some(StepStatus::Completed) {
            continue;
        }
        j.start(def, &s.key, bot(), "t").unwrap();
        if j.finish_work(def, &s.key, bot(), "t").unwrap() == StepStatus::AwaitingApproval {
            j.approve(def, &s.key, cpa(), "t", "synthetic approval")
                .unwrap();
        }
    }
}

async fn signer(r: &Router, token: &str, name: &str) -> Value {
    let code = public(r, "POST", &format!("/api/sign/{token}/otp"), None).await["test_code"]
        .as_str()
        .unwrap()
        .to_string();
    public(
        r,
        "POST",
        &format!("/api/sign/{token}/verify"),
        Some(json!({"code":code})),
    )
    .await;
    public(
        r,
        "POST",
        &format!("/api/sign/{token}/consent"),
        Some(json!({"accept":true,"consent_version":esign::CONSENT_VERSION})),
    )
    .await;
    public(
        r,
        "POST",
        &format!("/api/sign/{token}/adopt"),
        Some(json!({"kind":"typed","text":name,"initials":"SO"})),
    )
    .await;
    public(
        r,
        "POST",
        &format!("/api/sign/{token}/sign"),
        Some(json!({"values":[{"field_id":"off_title","value":"Synthetic director"}]})),
    )
    .await
}

/// Prepares, reviews, approves and sends a T183CORP envelope linked to `record_id`; returns (id, signer token).
async fn t183corp(r: &Router, record_id: &str) -> (String, String) {
    let v = ok(r, "POST", "/api/signatures/esign/builtin/t183corp", Some(json!({"operator_label":"Synthetic preparer",
        "recipients":[{"id":"officer","name":"Synthetic Officer","email":"officer@example.invalid"}],
        "context":{"module":"tax","record_id":record_id,"label":"T183CORP 2025"}}))).await;
    let env = v["envelope"]["envelope"].clone();
    let id = env["id"].as_str().unwrap().to_string();
    let tr = |e: &Value, a: &str| json!({"expected_revision":e["revision"],"expected_content_hash":e["content_hash"],"operator_label":"Synthetic reviewer","action":a,"note":"Synthetic review"});
    ok(r, "POST", &format!("/api/signatures/{id}/esign/pdf"), Some(json!({"operator_label":"Synthetic preparer","expected_revision":env["revision"],"expected_content_hash":env["content_hash"],
        "document_id":env["draft"]["documents"][0]["id"],"pdf_base64":base64::engine::general_purpose::STANDARD.encode(pdf(2))}))).await;
    let reviewed = ok(
        r,
        "POST",
        &format!("/api/signatures/{id}/transition"),
        Some(tr(&env, "submit_review")),
    )
    .await["envelope"]
        .clone();
    let approved = ok(
        r,
        "POST",
        &format!("/api/signatures/{id}/transition"),
        Some(tr(&reviewed, "approve")),
    )
    .await["envelope"]
        .clone();
    let sent = ok(r, "POST", &format!("/api/signatures/{id}/esign/send"), Some(json!({"operator_label":"Synthetic preparer","expected_revision":approved["revision"],"expected_content_hash":approved["content_hash"]}))).await;
    assert_eq!(
        sent["delivery"]["email_sent"], false,
        "test delivery never emails"
    );
    let token = sent["delivery"]["links"][0]["url"]
        .as_str()
        .unwrap()
        .trim_start_matches("/sign/#")
        .to_string();
    (id, token)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_journey_through_books_tax_signoff_t183_and_filed() {
    let (sigs, r) = router();
    let def = JourneyDef::load().unwrap();
    let mut journey = ClientJourney::new(&def, "synthetic-client");
    advance(&def, &mut journey, "engagement_letter");

    // Company books in the NXT ledger (synthetic 2025 activity, all dated in-period).
    ok(&r, "PUT", "/api/ledger/company", Some(json!({"name":"Synthetic E2E Corp","currency":"CAD","province":"AB","default_tax_code":"GST"}))).await;
    for (date, memo, dr, cr, amt) in [
        (
            "2025-01-02",
            "Synthetic capital",
            "acc_1000",
            "acc_3000",
            1_000_000,
        ),
        (
            "2025-06-30",
            "Synthetic sales",
            "acc_1000",
            "acc_4000",
            900_000,
        ),
        (
            "2025-09-30",
            "Synthetic expense",
            "acc_6600",
            "acc_1000",
            700_000,
        ),
    ] {
        ok(&r, "POST", "/api/ledger/entries", Some(json!({"date":date,"memo":memo,"lines":[{"account_id":dr,"debit":amt},{"account_id":cr,"credit":amt}]}))).await;
    }

    // Engagement captures the ledger trial balance.
    let draft = json!({"name":"Synthetic E2E year-end","client_ref":"synthetic-only","kind":"internal","period_start":"2025-01-01","period_end":"2025-12-31",
        "currency":"CAD","accounts":[],"groups":[],"adjustments":[],"workpapers":[],"issues":[],"notes":"Synthetic end-to-end fixture."});
    let e = ok(&r, "POST", "/api/engagements", Some(draft)).await;
    let preview = ok(
        &r,
        "POST",
        &eng(&e, "/ledger/preview"),
        Some(json!({"expected_revision":rev(&e)})),
    )
    .await;
    assert_eq!(preview["provenance"]["source"], "nxt_ledger");
    assert_eq!(preview["provenance"]["ledger_writes"], false);
    let e = ok(
        &r,
        "POST",
        &eng(&e, "/import/commit"),
        Some(json!({"expected_revision":rev(&e),"preview_id":preview["id"]})),
    )
    .await;
    let cash = e["document"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "acc_1000")
        .unwrap()
        .clone();
    assert_eq!(
        cash["debit_cents"], 1_200_000,
        "ledger balances carried exactly"
    );

    // Map every captured account (including zero balances) to a lead group and add evidence.
    let mut doc = e["document"].clone();
    let mut kinds: Vec<String> = vec![];
    for a in doc["accounts"].as_array_mut().unwrap() {
        let k = a["kind"].as_str().unwrap().to_string();
        a["group_id"] = json!(format!("g_{k}"));
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    doc["groups"] = json!(kinds.iter().map(|k| json!({"id":format!("g_{k}"),"code":format!("lead_{k}"),"name":format!("Synthetic {k} lead"),"kind":k})).collect::<Vec<_>>());
    doc["workpapers"] = json!([{"id":"wp_lead","reference":"A","title":"Synthetic cash lead","folder":"Planning","body":"Synthetic bank agreement.","required":true,"account_ids":["acc_1000"]}]);
    doc["expected_revision"] = e["revision"].clone();
    let e = ok(&r, "PUT", &eng(&e, ""), Some(doc)).await;
    let e = ok(
        &r,
        "POST",
        &eng(&e, "/calculate"),
        Some(json!({"expected_revision":rev(&e)})),
    )
    .await;
    let so = |e: &Value, stage: &str| json!({"expected_revision":rev(e),"workpaper_id":"wp_lead","stage":stage,"operator":if stage=="prepared"{"Synthetic preparer"}else{"Synthetic reviewer"},"note":"Reviewed synthetic evidence at the saved revision."});
    let e = ok(&r, "POST", &eng(&e, "/signoffs"), Some(so(&e, "prepared"))).await;
    let e = ok(&r, "POST", &eng(&e, "/transition"), Some(json!({"expected_revision":rev(&e),"status":"in_review","operator":"Synthetic preparer","note":"Prepared."}))).await;
    let e = ok(&r, "POST", &eng(&e, "/signoffs"), Some(so(&e, "reviewed"))).await;
    let e = ok(&r, "POST", &eng(&e, "/transition"), Some(json!({"expected_revision":rev(&e),"status":"locked","operator":"Synthetic reviewer","note":"Reviewed and released."}))).await;
    advance(&def, &mut journey, "tax_preparation");

    // Engagement -> tax T2 draft (proposal only; never infers taxable income).
    let tp = ok(
        &r,
        "POST",
        &eng(&e, "/tax/preview"),
        Some(json!({"expected_revision":rev(&e),"jurisdiction":"AB"})),
    )
    .await;
    assert_eq!(tp["filing_enabled"], false);
    let t = ok(
        &r,
        "POST",
        "/api/tax/import/commit",
        Some(json!({"preview_id":tp["preview"]["id"]})),
    )
    .await;
    assert_eq!(
        (t["program"].as_str(), t["status"].as_str()),
        (Some("T2"), Some("draft"))
    );
    let mut edit = json!({});
    for f in [
        "program",
        "tax_year",
        "jurisdiction",
        "name",
        "client_ref",
        "period_start",
        "period_end",
        "inputs",
        "parties",
        "corporate",
        "notes",
    ] {
        edit[f] = t[f].clone();
    }
    edit["expected_revision"] = t["revision"].clone();
    edit["inputs"] = json!({"scope_confirmed":true,"taxable_income_cents":200_000,"active_business_income_cents":200_000,"small_business_eligible":false});
    edit["corporate"]["legal_name"] = json!("Synthetic E2E Corp");
    edit["corporate"]["corporation_type"] = json!("ccpc");
    edit["corporate"]["province"] = json!("AB");
    edit["corporate"]["reconciliation_confirmed"] = json!(true);
    let t = ok(&r, "PUT", &tax(&t, ""), Some(edit)).await;
    let t = ok(
        &r,
        "POST",
        &tax(&t, "/calculate"),
        Some(json!({"expected_revision":rev(&t),"idempotency_key":"e2e"})),
    )
    .await;
    assert_eq!(
        t["calculation"]["result"]["supported"], true,
        "{}",
        t["calculation"]["result"]
    );
    assert!(
        t["calculation"]["result"]["totals"]["income_tax"]
            .as_i64()
            .unwrap()
            > 0
    );

    // Preparer then a different reviewer sign off.
    let t = ok(&r, "POST", &tax(&t, "/transition"), Some(json!({"expected_revision":rev(&t),"status":"in_review","note":"","reviewer":"Synthetic preparer"}))).await;
    assert_eq!(t["preparer_signoff"]["by"], "Synthetic preparer");
    fails(&r, "POST", &tax(&t, "/transition"), Some(json!({"expected_revision":rev(&t),"status":"approved","note":"self review","reviewer":"synthetic PREPARER"})), 409).await;
    let t = ok(&r, "POST", &tax(&t, "/transition"), Some(json!({"expected_revision":rev(&t),"status":"approved","note":"Reviewed synthetic totals.","reviewer":"Synthetic reviewer"}))).await;
    let draft183 = ok(&r, "GET", &tax(&t, "/t183"), None).await;
    assert_eq!(draft183["context"]["record_id"], t["id"]);
    let tid = t["id"].as_str().unwrap().to_string();
    let filed = |envelope: &str, reference: &str| json!({"expected_revision":rev(&t),"status":"filed","note":"Filed outside NXT by a human","reviewer":"Synthetic operator","t183_envelope_id":envelope,"filing_reference":reference});

    // Filing is refused until a T183CORP for this exact return is fully signed.
    fails(
        &r,
        "POST",
        &tax(&t, "/transition"),
        Some(filed("env_missing", "SYNTH-REF")),
        409,
    )
    .await;
    let (other, other_token) = t183corp(&r, "tax_someotherreturn").await;
    signer(&r, &other_token, "Synthetic Officer").await;
    fails(
        &r,
        "POST",
        &tax(&t, "/transition"),
        Some(filed(&other, "SYNTH-REF")),
        409,
    )
    .await;
    let (env_id, token) = t183corp(&r, &tid).await;
    fails(
        &r,
        "POST",
        &tax(&t, "/transition"),
        Some(filed(&env_id, "SYNTH-REF")),
        409,
    )
    .await;
    let done = signer(&r, &token, "Synthetic Officer").await;
    assert_eq!(done["step"], "completed", "{done}");
    let ceremony = sigs.ceremony_record(&env_id).await.ok().unwrap();
    assert!(ceremony.verify_chain());

    // Journey human gates: filing authorization only via the verified ceremony.
    advance(&def, &mut journey, "filing_authorization");
    journey
        .start(&def, "filing_authorization", bot(), "t")
        .unwrap();
    journey
        .finish_work(&def, "filing_authorization", bot(), "t")
        .unwrap();
    assert!(esign::approve_journey_gate(&mut journey, &def, &ceremony, bot(), "t").is_err());
    esign::approve_journey_gate(&mut journey, &def, &ceremony, cpa(), "t")
        .ok()
        .unwrap();
    assert_eq!(
        journey.status_of("filing_authorization"),
        Some(StepStatus::Completed)
    );

    let t = ok(
        &r,
        "POST",
        &tax(&t, "/transition"),
        Some(filed(&env_id, "SYNTH-REF")),
    )
    .await;
    assert_eq!(t["status"], "filed");
    assert_eq!(t["filed_record"]["t183_envelope_id"], env_id.as_str());
    journey
        .start(&def, "filing_submission", bot(), "t")
        .unwrap();
    journey
        .finish_work(&def, "filing_submission", bot(), "t")
        .unwrap();
    journey
        .approve(
            &def,
            "filing_submission",
            cpa(),
            "t",
            "Filed outside NXT; reference recorded.",
        )
        .unwrap();
    assert_eq!(
        journey.status_of("filing_submission"),
        Some(StepStatus::Completed)
    );
    fails(
        &r,
        "POST",
        &tax(&t, "/transition"),
        Some(json!({"expected_revision":rev(&t),"status":"draft","note":"reopen"})),
        409,
    )
    .await;
    assert_eq!(
        ok(&r, "GET", "/api/tax/audit/verify", None).await["valid"],
        true
    );
}
