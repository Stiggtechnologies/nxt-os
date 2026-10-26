//! Synthetic HTTP acceptance for independent NXT engagement workpapers.
//! No Caseware file, private client record or external connector is used.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

async fn raw(router: &axum::Router, method: &str, uri: &str, body: Option<Value>, auth: bool) -> (u16, String, String, String) {
    let mut request = Request::builder().method(method).uri(uri);
    if auth { request = request.header("authorization", "Bearer k"); }
    let request = match body {
        Some(body) => request.header("content-type", "application/json").body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }.unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let content_type = response.headers().get("content-type").and_then(|v|v.to_str().ok()).unwrap_or("").to_string();
    let cache = response.headers().get("cache-control").and_then(|v|v.to_str().ok()).unwrap_or("").to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, content_type, cache, String::from_utf8(bytes.to_vec()).unwrap())
}
async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let (status, _, _, body) = raw(router, method, uri, body, true).await;
    (status, serde_json::from_str(&body).unwrap_or_else(|_|json!({"raw":body})))
}
async fn ok(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status,value) = call(router,method,uri,body).await;
    assert_eq!(status,200,"{method} {uri}: {value}"); value
}
async fn fails(router: &axum::Router, method: &str, uri: &str, body: Option<Value>, expected: u16) {
    let (status,value) = call(router,method,uri,body).await;
    assert_eq!(status,expected,"{method} {uri}: {value}");
}
fn id(record: &Value) -> &str { record["id"].as_str().expect("engagement id") }
fn revision(record: &Value) -> u64 { record["revision"].as_u64().expect("revision") }
fn endpoint(record: &Value, suffix: &str) -> String { format!("/api/engagements/{}{suffix}",id(record)) }
fn editable(record: &Value) -> Value {
    let mut input = record["document"].clone(); input["expected_revision"] = record["revision"].clone(); input
}
async fn calculate(router: &axum::Router, record: &Value) -> Value {
    ok(router,"POST",&endpoint(record,"/calculate"),Some(json!({"expected_revision":revision(record)}))).await
}
async fn transition(router: &axum::Router, record: &Value, status: &str, note: &str) -> Value {
    ok(router,"POST",&endpoint(record,"/transition"),Some(json!({"expected_revision":revision(record),"status":status,"operator":"Synthetic reviewer","note":note}))).await
}
async fn signoff(router: &axum::Router, record: &Value, workpaper: &str, stage: &str) -> Value {
    ok(router,"POST",&endpoint(record,"/signoffs"),Some(json!({"expected_revision":revision(record),"workpaper_id":workpaper,"stage":stage,"operator":if stage=="prepared"{"Synthetic preparer"}else{"Synthetic reviewer"},"note":"Reviewed synthetic evidence at the saved revision."}))).await
}

fn fixture() -> Value {
    let mut accounts = Vec::new(); let mut groups = Vec::new();
    for (key,kind,debit,credit) in [
        ("cash","asset",1_000_000,0),("receivables","asset",200_000,0),("equipment","asset",500_000,0),
        ("payables","liability",0,300_000),("capital","equity",0,1_200_000),
        ("revenue","revenue",0,900_000),("expense","expense",700_000,0),
    ] {
        let group = format!("g_{key}");
        accounts.push(json!({"id":format!("a_{key}"),"code":format!("code_{key}"),"name":format!("Synthetic {key}"),"kind":kind,"group_id":group,"debit_cents":debit,"credit_cents":credit,"prior_debit_cents":0,"prior_credit_cents":0}));
        groups.push(json!({"id":group,"code":format!("map_{key}"),"name":format!("Synthetic {key} lead"),"kind":kind}));
    }
    json!({
        "name":"Synthetic engagement acceptance","client_ref":"synthetic-only","kind":"internal",
        "period_start":"2025-01-01","period_end":"2025-12-31","currency":"CAD","accounts":accounts,"groups":groups,
        "adjustments":[{"id":"aje1","reference":"AJE1","date":"2025-12-31","kind":"adjusting","description":"Synthetic accrued expense",
            "lines":[{"account_id":"a_expense","debit_cents":100_000},{"account_id":"a_payables","credit_cents":100_000}]}],
        "workpapers":[
            {"id":"wp_lead","reference":"A","title":"Synthetic lead","folder":"Planning","body":"Synthetic evidence body.","required":true,"account_ids":["a_cash"],"adjustment_ids":["aje1"]},
            {"id":"wp_review","reference":"B","title":"Synthetic dependent review","folder":"Completion","body":"Synthetic review body.","required":true,"depends_on":["wp_lead"]}
        ],"issues":[],"notes":"Internal workpaper fixture; no source-system data."
    })
}

/// One test avoids process-wide authentication environment races in this binary.
#[tokio::test(flavor="multi_thread", worker_threads=2)]
async fn engagement_workpaper_lifecycle() {
    unsafe { std::env::set_var("ADMIN_API_KEY","k") };
    let router = app_with(Store::memory);
    for uri in ["/api/engagements","/api/engagements/catalog","/api/engagements/summary","/api/engagements/audit"] {
        assert_eq!(raw(&router,"GET",uri,None,false).await.0,401);
    }
    assert_eq!(raw(&router,"POST","/api/engagements",Some(fixture()),false).await.0,401);
    assert_eq!(ok(&router,"GET","/api/engagements",None).await,json!([]));
    let (_,_,cache,_) = raw(&router,"GET","/api/engagements/catalog",None,true).await;
    assert!(cache.contains("no-store"));
    let catalog = ok(&router,"GET","/api/engagements/catalog",None).await;
    assert_eq!(catalog["status"],"validation_only");
    let summary = ok(&router,"GET","/api/engagements/summary",None).await;
    assert_eq!(summary["auth_required"],true);
    assert!(summary["access_model"].as_str().unwrap().contains("shared"));
    let ledger_before = ok(&router,"GET","/api/ledger/entries",None).await;
    let tax_before = ok(&router,"GET","/api/tax/returns",None).await;
    let created = ok(&router,"POST","/api/engagements",Some(fixture())).await;
    assert_eq!(created["status"],"draft"); assert_eq!(created["revision"],1); assert_eq!(created["input_revision"],1);
    assert!(created["snapshot"].is_null());
    fails(&router,"POST",&endpoint(&created,"/transition"),Some(json!({"expected_revision":1,"status":"locked","operator":"Synthetic reviewer","note":"Cannot bypass calculation."})),409).await;
    let calculated = calculate(&router,&created).await;
    assert_eq!(calculated["snapshot"]["result"]["supported"],true,"{calculated}");
    assert_eq!(calculated["snapshot"]["result"]["ruleset"],"nxt-engagement-workpapers-v1");
    let totals = &calculated["snapshot"]["result"]["totals"];
    for (key,expected) in [("unadjusted_debit_cents",2_400_000),("unadjusted_credit_cents",2_400_000),("adjustment_debit_cents",100_000),("adjustment_credit_cents",100_000),("adjusted_debit_cents",2_500_000),("adjusted_credit_cents",2_500_000),("adjusted_difference_cents",0)] {
        assert_eq!(totals[key],expected,"{key}");
    }
    assert_eq!(calculated["snapshot"]["result"]["financial_statements"]["current"],json!({
        "assets_cents":1_700_000,"liabilities_cents":400_000,"equity_cents":1_200_000,"revenue_cents":900_000,"expenses_cents":800_000,"net_income_cents":100_000,"balance_sheet_difference_cents":0
    }));
    let mut expected_document = created["document"].clone(); expected_document["expected_revision"] = Value::Null;
    assert_eq!(calculated["snapshot"]["input_document"],expected_document);
    for key in ["input_hash","output_hash"] {
        let digest = calculated["snapshot"][key].as_str().unwrap(); assert_eq!(digest.len(),64); assert!(digest.chars().all(|c|c.is_ascii_hexdigit()));
    }
    assert_eq!(ok(&router,"GET","/api/ledger/entries",None).await,ledger_before,"Engagement AJE must not post Ledger entries");
    assert_eq!(ok(&router,"GET","/api/tax/returns",None).await,tax_before,"Engagement AJE must not create or alter Tax workpapers");

    // Dependencies, distinct preparation/review stages and an exact fresh
    // calculation form the internal review gate; labels do not establish IAM.
    fails(&router,"POST",&endpoint(&calculated,"/signoffs"),Some(json!({"expected_revision":revision(&calculated),"workpaper_id":"wp_review","stage":"prepared","operator":"Synthetic preparer","note":"Dependency not ready."})),409).await;
    fails(&router,"POST",&endpoint(&calculated,"/signoffs"),Some(json!({"expected_revision":revision(&calculated),"workpaper_id":"wp_lead","stage":"reviewed","operator":"Synthetic reviewer","note":"Not prepared."})),409).await;
    let prepared_a = signoff(&router,&calculated,"wp_lead","prepared").await;
    let prepared = signoff(&router,&prepared_a,"wp_review","prepared").await;
    let in_review = transition(&router,&prepared,"in_review","Begin internal workpaper review.").await;
    fails(&router,"POST",&endpoint(&in_review,"/transition"),Some(json!({"expected_revision":revision(&in_review),"status":"locked","operator":"Synthetic reviewer","note":"Reviews incomplete."})),409).await;
    let reviewed_a = signoff(&router,&in_review,"wp_lead","reviewed").await;
    let reviewed = signoff(&router,&reviewed_a,"wp_review","reviewed").await;
    let locked = transition(&router,&reviewed,"locked","Synthetic evidence reviewed and released internally.").await;
    assert_eq!(locked["status"],"locked");
    assert!(!locked["lock_history"].as_array().unwrap().is_empty());
    fails(&router,"PUT",&endpoint(&locked,""),Some(editable(&locked)),409).await;
    fails(&router,"POST",&endpoint(&locked,"/calculate"),Some(json!({"expected_revision":revision(&locked)})),409).await;
    fails(&router,"POST",&endpoint(&locked,"/transition"),Some(json!({"expected_revision":revision(&locked),"status":"draft","operator":"Synthetic reviewer"})),400).await;
    let reopen_note="Reopen synthetic engagement for a correction.\nSynthetic evidence follow-up:\tretain prior review.";
    let reopened = transition(&router,&locked,"draft",reopen_note).await;
    assert_eq!(reopened["lock_history"].as_array().unwrap().last().unwrap()["note"],reopen_note);
    let historical_snapshot = locked["snapshot"].clone();
    let mut edit = editable(&reopened); edit["notes"] = json!("Corrected synthetic notes.");
    let changed = ok(&router,"PUT",&endpoint(&reopened,""),Some(edit.clone())).await;
    assert_eq!(changed["status"],"draft"); assert!(changed["snapshot"].is_null());
    assert_eq!(changed["signoffs"],json!([]));
    assert!(changed["snapshots"].as_array().unwrap().contains(&historical_snapshot));
    assert_eq!(changed["snapshots"][0]["input_document"]["notes"],created["document"]["notes"]);
    assert!(!changed["signoff_history"].as_array().unwrap().is_empty());
    fails(&router,"PUT",&endpoint(&changed,""),Some(edit),409).await;
    fails(&router,"POST",&endpoint(&changed,"/transition"),Some(json!({"expected_revision":revision(&changed),"status":"in_review","operator":"Synthetic reviewer","note":"Stale result."})),409).await;

    // Preview preserves saved content and mapping; the exact preview commits
    // once, and imported identity remains local to this engagement.
    let mut accounts = changed["document"]["accounts"].as_array().unwrap().clone();
    for account in &mut accounts { account.as_object_mut().unwrap().remove("group_id"); }
    accounts[0]["debit_cents"] = json!(1_000_001); accounts[4]["credit_cents"] = json!(1_200_001);
    let preview = ok(&router,"POST",&endpoint(&changed,"/import/preview"),Some(json!({"expected_revision":revision(&changed),"document":{"format":"nxt-engagement-tb/v1","accounts":accounts}}))).await;
    assert_eq!(preview["accounts"][0]["debit_cents"],1_000_001);
    assert_eq!(preview["accounts"][0]["group_id"],"g_cash");
    assert_eq!(preview["checks"]["supported"],true);
    assert_eq!(preview["checks"]["totals"]["adjusted_debit_cents"],2_500_001);
    assert_eq!(preview["checks"]["totals"]["adjusted_credit_cents"],2_500_001);
    assert_eq!(ok(&router,"GET",&endpoint(&changed,""),None).await,changed,"Preview cannot alter saved workpaper or review state");
    let import_request = json!({"expected_revision":revision(&changed),"preview_id":id(&preview)});
    let imported = ok(&router,"POST",&endpoint(&changed,"/import/commit"),Some(import_request.clone())).await;
    assert_eq!(imported["document"]["accounts"][0]["id"],"a_cash");
    assert_eq!(imported["document"]["accounts"][0]["group_id"],"g_cash");
    assert_eq!(imported["document"]["accounts"][0]["debit_cents"],1_000_001);
    assert!(imported["snapshot"].is_null()); assert_eq!(imported["signoffs"],json!([]));
    assert_eq!(imported["import_history"].as_array().unwrap().len(),1);
    assert_eq!(imported["import_history"][0]["source_hash"],preview["source_hash"]);
    assert_eq!(imported["import_history"][0]["canonical_hash"],preview["canonical_hash"]);
    assert_eq!(imported["import_history"][0]["preview_id"],preview["id"]);
    assert_eq!(ok(&router,"POST",&endpoint(&changed,"/import/commit"),Some(import_request)).await,imported,"Commit replay cannot create a second import effect");
    let calculated = calculate(&router,&imported).await;
    let prepared_a = signoff(&router,&calculated,"wp_lead","prepared").await;
    let prepared = signoff(&router,&prepared_a,"wp_review","prepared").await;
    let in_review = transition(&router,&prepared,"in_review","Review the new imported input revision.").await;
    let reviewed_a = signoff(&router,&in_review,"wp_lead","reviewed").await;
    let reviewed = signoff(&router,&reviewed_a,"wp_review","reviewed").await;
    let calculated = transition(&router,&reviewed,"locked","Lock reviewed comparatives before rollforward.").await;

    // Year close here means a new comparative workpaper, never a hidden Ledger
    // closing entry, carried approval, or automatic tax carryforward.
    let next = ok(&router,"POST",&endpoint(&calculated,"/rollforward"),Some(json!({"expected_revision":revision(&calculated),"period_start":"2026-01-01","period_end":"2026-12-31"}))).await;
    assert_ne!(next["id"],calculated["id"]); assert_eq!(next["status"],"draft");
    assert!(next["snapshot"].is_null()); assert_eq!(next["snapshots"],json!([])); assert_eq!(next["signoffs"],json!([]));
    assert_eq!(next["document"]["adjustments"],json!([]));
    assert_eq!(next["document"]["prior_period_start"],"2025-01-01");
    assert_eq!(next["document"]["prior_period_end"],"2025-12-31");
    for account in next["document"]["accounts"].as_array().unwrap() { assert_eq!(account["debit_cents"],0); assert_eq!(account["credit_cents"],0); }
    assert!(next["document"]["workpapers"].as_array().unwrap().iter().all(|p|p["body"]=="" && p["adjustment_ids"]==json!([])));
    let expense = next["document"]["accounts"].as_array().unwrap().iter().find(|a|a["id"]=="a_expense").unwrap();
    let payable = next["document"]["accounts"].as_array().unwrap().iter().find(|a|a["id"]=="a_payables").unwrap();
    assert_eq!(expense["prior_debit_cents"],800_000); assert_eq!(payable["prior_credit_cents"],400_000);
    assert_eq!(ok(&router,"GET",&endpoint(&calculated,""),None).await,calculated);
    fails(&router,"POST",&endpoint(&calculated,"/rollforward"),Some(json!({"expected_revision":revision(&calculated),"period_start":"2026-01-01","period_end":"2026-12-31"})),409).await;

    // Schema and reference errors fail instead of creating plausible results.
    for variant in ["duplicate_account","duplicate_code","dangling_group","dangling_adjustment_account","dependency_cycle","bad_date"] {
        let mut bad = fixture();
        match variant {
            "duplicate_account" => bad["accounts"][1]["id"] = bad["accounts"][0]["id"].clone(),
            "duplicate_code" => bad["accounts"][1]["code"] = bad["accounts"][0]["code"].clone(),
            "dangling_group" => bad["accounts"][0]["group_id"] = json!("missing_group"),
            "dangling_adjustment_account" => bad["adjustments"][0]["lines"][0]["account_id"] = json!("missing_account"),
            "dependency_cycle" => bad["workpapers"][0]["depends_on"] = json!(["wp_review"]),
            "bad_date" => bad["period_start"] = json!("2025-02-30"),
            _ => unreachable!(),
        }
        fails(&router,"POST","/api/engagements",Some(bad),400).await;
    }
    let mut fractional = fixture(); fractional["accounts"][0]["debit_cents"] = json!(1.25);
    let (status,_) = call(&router,"POST","/api/engagements",Some(fractional)).await;
    assert!([400,422].contains(&status),"Fractional cents must be rejected");
    let mut oversized = fixture(); oversized["workpapers"][0]["body"] = json!("x".repeat(16 * 1024 + 1));
    fails(&router,"POST","/api/engagements",Some(oversized),400).await;
    let mut unsupported_currency = fixture(); unsupported_currency["currency"] = json!("USD");
    fails(&router,"POST","/api/engagements",Some(unsupported_currency),400).await;
    let mut oversized_request = fixture(); oversized_request["notes"] = json!("x".repeat(1_100_000));
    fails(&router,"POST","/api/engagements",Some(oversized_request),413).await;
    for reason in ["unbalanced","unmapped","blank_required_workpaper","open_blocking_issue"] {
        let mut blocked = fixture();
        match reason {
            "unbalanced" => blocked["accounts"][0]["debit_cents"] = json!(1_000_001),
            "unmapped" => blocked["accounts"][0]["group_id"] = Value::Null,
            "blank_required_workpaper" => blocked["workpapers"][0]["body"] = json!(""),
            "open_blocking_issue" => blocked["issues"] = json!([{"id":"issue1","title":"Synthetic blocker","blocking":true,"status":"open","workpaper_id":"wp_lead"}]),
            _ => unreachable!(),
        }
        let blocked = ok(&router,"POST","/api/engagements",Some(blocked)).await;
        let blocked = calculate(&router,&blocked).await;
        assert_eq!(blocked["snapshot"]["result"]["supported"],false,"{reason}: {blocked}");
        fails(&router,"POST",&endpoint(&blocked,"/transition"),Some(json!({"expected_revision":revision(&blocked),"status":"in_review","operator":"Synthetic reviewer","note":"Must not accept blockers."})),409).await;
    }

    let mut injection = editable(&next); injection["name"] = json!("Synthetic <script>alert('name')</script>");
    injection["workpapers"][0]["body"] = json!("<img src=x onerror=alert('body')>");
    let injection = ok(&router,"PUT",&endpoint(&next,""),Some(injection)).await;
    let mut edit_a = editable(&injection); edit_a["notes"] = json!("Concurrent synthetic change A");
    let mut edit_b = editable(&injection); edit_b["notes"] = json!("Concurrent synthetic change B");
    let edit_path = endpoint(&injection,"");
    let (a,b) = tokio::join!(call(&router,"PUT",&edit_path,Some(edit_a)),call(&router,"PUT",&edit_path,Some(edit_b)));
    assert!((a.0==200 && b.0==409)||(a.0==409 && b.0==200),"Exactly one stale competing edit must commit: {a:?} {b:?}");
    let injection = if a.0==200 {a.1}else{b.1};
    let (status,content_type,cache,html) = raw(&router,"GET",&endpoint(&injection,"/print"),None,true).await;
    assert_eq!(status,200); assert!(content_type.starts_with("text/html")); assert!(cache.contains("no-store"));
    assert!(html.contains("&lt;script&gt;")); assert!(html.contains("&lt;img")); assert!(!html.contains("<script>")); assert!(!html.contains("<img src=x"));
    let (status,content_type,cache,package) = raw(&router,"GET",&endpoint(&calculated,"/package"),None,true).await;
    assert_eq!(status,200);
    assert!(content_type.starts_with("application/json")); assert!(cache.contains("no-store"));
    let package:Value = serde_json::from_str(&package).unwrap();
    assert!(package.is_object()); assert!(package.to_string().contains(calculated["snapshot"]["input_hash"].as_str().unwrap()));
    let (status,_,_,locked_html)=raw(&router,"GET",&endpoint(&calculated,"/print"),None,true).await;
    assert_eq!(status,200);
    assert!(locked_html.contains("Trial balance control totals"));
    assert!(locked_html.contains("Current financial summary"));
    assert!(locked_html.contains("Prior financial summary"));
    assert!(locked_html.contains("<td>adjusted debit cents</td><td>25000.01</td>"));
    assert!(locked_html.contains("<td>adjusted credit cents</td><td>25000.01</td>"));
    assert!(locked_html.contains("<td>net income cents</td><td>1000.00</td>"));
    let archived = transition(&router,&calculated,"archived","Retain the synthetic engagement history.").await;
    assert_eq!(archived["status"],"archived");
    fails(&router,"PUT",&endpoint(&archived,""),Some(editable(&archived)),409).await;
    fails(&router,"DELETE",&endpoint(&archived,""),None,405).await;
    assert_eq!(ok(&router,"GET","/api/ledger/entries",None).await,ledger_before);
    assert_eq!(ok(&router,"GET","/api/tax/returns",None).await,tax_before);
    let audit = ok(&router,"GET","/api/engagements/audit",None).await;
    assert!(!audit.as_array().unwrap().is_empty());
    assert!(audit.to_string().contains("Reopen synthetic engagement for a correction."));
    fails(&router,"GET","/api/engagements/audit?limit=201",None,400).await;
    let listed = ok(&router,"GET","/api/engagements",None).await;
    for row in listed.as_array().unwrap() {
        for field in ["document","snapshot","snapshots","signoff_history","lock_history","import_history"] {assert!(row.get(field).is_none(),"List projection must not embed {field}");}
    }
    engagement_bridge_acceptance().await;
    engagement_rollforward_origin_acceptance().await;
    engagement_annotation_acceptance().await;
    engagement_preview_binding_acceptance().await;
}

async fn engagement_preview_binding_acceptance() {
    use nxt_os::{engagement,firm,ledger,tax};
    let ledger=Arc::new(ledger::engine::Ledger::new(Store::memory()));
    let tax=Arc::new(tax::Tax::new(Store::memory()));
    let firm=Arc::new(firm::Firm::new(Store::memory(),ledger.clone()));
    let engagements=Arc::new(engagement::Engagements::new(Store::memory(),ledger.clone(),tax,firm));
    let router=engagement::router(engagements.clone()).merge(ledger::routes::router(ledger));
    let ledger_before=ok(&router,"GET","/api/ledger/entries",None).await;
    let created=ok(&router,"POST","/api/engagements",Some(fixture())).await;
    let preview=ok(&router,"POST",&endpoint(&created,"/import/preview"),Some(json!({"expected_revision":revision(&created),"document":{"format":"nxt-engagement-tb/v1","accounts":created["document"]["accounts"]}}))).await;
    assert_eq!(preview["binding_sha256"].as_str().unwrap().len(),64);
    let request=json!({"expected_revision":revision(&created),"preview_id":preview["id"]});
    let baseline=engagements.store.get("engagements:state:v1").await.unwrap().unwrap();
    for field in ["source_hash","canonical_hash","target_input_sha256","created_at","expires_at","expected_revision","summary","warnings","checks","provenance","accounts_and_canonical_hash","binding_sha256","legacy_unbound"] {
        let mut stored:Value=serde_json::from_str(&baseline).unwrap();
        let p=&mut stored["previews"][0];
        match field {
            "expected_revision"=>p[field]=json!(2),
            "summary"=>p[field]["accounts"]=json!(999),
            "warnings"=>p[field]=json!([]),
            "checks"=>p[field]["totals"]["adjusted_debit_cents"]=json!(1),
            "provenance"=>p[field]=json!({"source":"altered_synthetic_origin"}),
            "accounts_and_canonical_hash"=>{
                p["accounts"][0]["debit_cents"]=json!(999);
                let accounts:Vec<engagement::models::Account>=serde_json::from_value(p["accounts"].clone()).unwrap();
                p["canonical_hash"]=json!(hex::encode(Sha256::digest(serde_json::to_vec(&accounts).unwrap())));
            },
            "legacy_unbound"=>{p.as_object_mut().unwrap().remove("binding_sha256");},
            _=>p[field]=json!("b".repeat(64))
        }
        let corrupt=stored.to_string();
        engagements.store.set("engagements:state:v1",&corrupt).await.unwrap();
        fails(&router,"POST",&endpoint(&created,"/import/commit"),Some(request.clone()),409).await;
        assert_eq!(engagements.store.get("engagements:state:v1").await.unwrap().unwrap(),corrupt,"{field}: rejection must make no record, receipt or audit write");
        assert_eq!(ok(&router,"GET",&endpoint(&created,""),None).await,created);
    }
    let mut target_drift:Value=serde_json::from_str(&baseline).unwrap();
    target_drift["engagements"][0]["document"]["notes"]=json!("Synthetic target drift without a revision increment");
    let drift_bytes=target_drift.to_string();
    engagements.store.set("engagements:state:v1",&drift_bytes).await.unwrap();
    fails(&router,"POST",&endpoint(&created,"/import/commit"),Some(request.clone()),409).await;
    assert_eq!(engagements.store.get("engagements:state:v1").await.unwrap().unwrap(),drift_bytes);
    engagements.store.set("engagements:state:v1",&baseline).await.unwrap();
    let imported=ok(&router,"POST",&endpoint(&created,"/import/commit"),Some(request.clone())).await;
    assert_eq!(imported["import_history"][0]["preview_binding_sha256"],preview["binding_sha256"]);
    assert_eq!(ok(&router,"POST",&endpoint(&created,"/import/commit"),Some(request.clone())).await,imported);
    let committed=engagements.store.get("engagements:state:v1").await.unwrap().unwrap();
    for variant in ["missing_receipt","source","canonical","provenance","binding","duplicate_receipt","commit_marker"] {
        let mut stored:Value=serde_json::from_str(&committed).unwrap();
        match variant {
            "missing_receipt"=>stored["engagements"][0]["import_history"]=json!([]),
            "source"=>stored["engagements"][0]["import_history"][0]["source_hash"]=json!("c".repeat(64)),
            "canonical"=>stored["engagements"][0]["import_history"][0]["canonical_hash"]=json!("c".repeat(64)),
            "provenance"=>stored["engagements"][0]["import_history"][0]["provenance"]=json!({"source":"changed"}),
            "binding"=>stored["engagements"][0]["import_history"][0]["preview_binding_sha256"]=json!("c".repeat(64)),
            "duplicate_receipt"=>{let receipt=stored["engagements"][0]["import_history"][0].clone();stored["engagements"][0]["import_history"].as_array_mut().unwrap().push(receipt);},
            "commit_marker"=>stored["previews"][0]["committed_revision"]=json!(999),
            _=>unreachable!()
        }
        let corrupt=stored.to_string();
        engagements.store.set("engagements:state:v1",&corrupt).await.unwrap();
        fails(&router,"POST",&endpoint(&created,"/import/commit"),Some(request.clone()),409).await;
        assert_eq!(engagements.store.get("engagements:state:v1").await.unwrap().unwrap(),corrupt);
    }
    let mut legacy:Value=serde_json::from_str(&committed).unwrap();
    legacy["previews"][0].as_object_mut().unwrap().remove("binding_sha256");
    legacy["previews"][0].as_object_mut().unwrap().remove("target_input_sha256");
    legacy["engagements"][0]["import_history"][0].as_object_mut().unwrap().remove("preview_binding_sha256");
    let legacy_bytes=legacy.to_string();
    engagements.store.set("engagements:state:v1",&legacy_bytes).await.unwrap();
    let retained=ok(&router,"POST",&endpoint(&created,"/import/commit"),Some(request)).await;
    assert!(retained["import_history"][0].get("preview_binding_sha256").is_none());
    assert_eq!(serde_json::from_str::<Value>(&engagements.store.get("engagements:state:v1").await.unwrap().unwrap()).unwrap(),legacy,"Legacy committed replay must not fabricate a new receipt or binding");
    engagements.store.set("engagements:state:v1",&baseline).await.unwrap();
    let second=ok(&router,"POST","/api/engagements",Some(fixture())).await;
    let mut crossed:Value=serde_json::from_str(&engagements.store.get("engagements:state:v1").await.unwrap().unwrap()).unwrap();
    crossed["previews"][0]["engagement_id"]=second["id"].clone();
    let crossed_bytes=crossed.to_string();
    engagements.store.set("engagements:state:v1",&crossed_bytes).await.unwrap();
    fails(&router,"POST",&endpoint(&second,"/import/commit"),Some(json!({"expected_revision":revision(&second),"preview_id":preview["id"]})),409).await;
    assert_eq!(engagements.store.get("engagements:state:v1").await.unwrap().unwrap(),crossed_bytes);
    assert_eq!(ok(&router,"GET","/api/ledger/entries",None).await,ledger_before);
}

async fn engagement_annotation_acceptance() {
    let router = app_with(Store::memory);
    let ledger_before = ok(&router, "GET", "/api/ledger/entries", None).await;
    let mut input = fixture();
    let body = "Synthetic caf\u{00e9} evidence\r\nReviewed \u{1f4ce}";
    input["workpapers"][0]["body"] = json!(body);
    let anchor = hex::encode(Sha256::digest(body.as_bytes()));
    let annotation = json!({"id":"mark_1","kind":"agreed","note":"<img src=x onerror=alert(1)> Synthetic source agreed.","operator_label":"Synthetic preparer <label>","body_sha256":anchor});
    input["workpapers"][0]["annotations"] = json!([annotation]);
    for variant in [
        "kind",
        "hash",
        "label",
        "duplicate",
        "per_paper_limit",
        "total_limit",
    ] {
        let mut invalid = input.clone();
        match variant {
            "kind" => invalid["workpapers"][0]["annotations"][0]["kind"] = json!("signed"),
            "hash" => {
                invalid["workpapers"][0]["annotations"][0]["body_sha256"] = json!("A".repeat(64))
            }
            "label" => invalid["workpapers"][0]["annotations"][0]["operator_label"] = json!(" "),
            "duplicate" => {
                invalid["workpapers"][1]["annotations"] =
                    input["workpapers"][0]["annotations"].clone()
            }
            "per_paper_limit" => {
                invalid["workpapers"][0]["annotations"] = json!(vec![annotation.clone(); 51])
            }
            "total_limit" => {
                let template = input["workpapers"][0].clone();
                invalid["workpapers"] = json!(
                    (0..11)
                        .map(|paper| {
                            let mut p = template.clone();
                            p["id"] = json!(format!("p{paper}"));
                            p["reference"] = json!(format!("P{paper}"));
                            p["annotations"] = json!(
                                (0..50)
                                    .map(|mark| {
                                        let mut a = annotation.clone();
                                        a["id"] = json!(format!("m{paper}_{mark}"));
                                        a
                                    })
                                    .collect::<Vec<_>>()
                            );
                            p
                        })
                        .collect::<Vec<_>>()
                );
            }
            _ => unreachable!(),
        }
        fails(&router, "POST", "/api/engagements", Some(invalid), 400).await;
    }
    assert_eq!(
        ok(&router, "GET", "/api/engagements", None).await,
        json!([])
    );
    let record = ok(&router, "POST", "/api/engagements", Some(input)).await;
    assert!(
        record["document"]["workpapers"][1]
            .get("annotations")
            .is_none(),
        "Legacy empty workpapers keep their signed input representation"
    );
    let calculated = calculate(&router, &record).await;
    assert_eq!(calculated["snapshot"]["result"]["supported"], true);
    assert_eq!(
        calculated["snapshot"]["input_document"]["workpapers"][0]["annotations"][0],
        annotation
    );
    let printed = raw(&router, "GET", &endpoint(&calculated, "/print"), None, true)
        .await
        .3;
    assert!(printed.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(!printed.contains("<img src=x"));
    assert!(printed.contains(&anchor));
    let prepared = signoff(&router, &calculated, "wp_lead", "prepared").await;
    let mut changed = editable(&prepared);
    changed["workpapers"][0]["body"] = json!("Changed evidence requires a new review.");
    let edited = ok(
        &router,
        "PUT",
        &endpoint(&prepared, ""),
        Some(changed.clone()),
    )
    .await;
    assert!(edited["snapshot"].is_null());
    assert_eq!(edited["signoffs"], json!([]));
    fails(&router, "PUT", &endpoint(&edited, ""), Some(changed), 409).await;
    let blocked = calculate(&router, &edited).await;
    assert_eq!(blocked["snapshot"]["result"]["supported"], false);
    assert!(
        blocked["snapshot"]["result"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "stale_workpaper_annotation" && d["blocking"] == true)
    );
    fails(&router,"POST",&endpoint(&blocked,"/signoffs"),Some(json!({"expected_revision":revision(&blocked),"workpaper_id":"wp_lead","stage":"prepared","operator":"Synthetic","note":"Must fail stale anchor"})),409).await;
    let mut reviewed_text = editable(&blocked);
    reviewed_text["workpapers"][0]["annotations"][0]["body_sha256"] =
        json!(hex::encode(Sha256::digest(
            reviewed_text["workpapers"][0]["body"]
                .as_str()
                .unwrap()
                .as_bytes()
        )));
    let saved = ok(&router, "PUT", &endpoint(&blocked, ""), Some(reviewed_text)).await;
    let mut ready = calculate(&router, &saved).await;
    assert_eq!(ready["snapshot"]["result"]["supported"], true);
    ready = signoff(&router, &ready, "wp_lead", "prepared").await;
    ready = signoff(&router, &ready, "wp_review", "prepared").await;
    ready = transition(
        &router,
        &ready,
        "in_review",
        "Synthetic text anchor reviewed",
    )
    .await;
    ready = signoff(&router, &ready, "wp_lead", "reviewed").await;
    ready = signoff(&router, &ready, "wp_review", "reviewed").await;
    let locked = transition(
        &router,
        &ready,
        "locked",
        "Synthetic annotated workpapers locked",
    )
    .await;
    fails(
        &router,
        "PUT",
        &endpoint(&locked, ""),
        Some(editable(&locked)),
        409,
    )
    .await;
    let next = ok(&router,"POST",&endpoint(&locked,"/rollforward"),Some(json!({"expected_revision":revision(&locked),"period_start":"2026-01-01","period_end":"2026-12-31"}))).await;
    assert!(
        next["document"]["workpapers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("annotations").is_none())
    );
    assert_eq!(
        next["rollforward_origin"]["source_snapshot"]["input_document"]["workpapers"][0]["annotations"],
        locked["document"]["workpapers"][0]["annotations"]
    );
    assert!(
        next["rollforward_origin"]["copy_policy"]["cleared_fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "workpaper_annotations")
    );
    assert_eq!(
        ok(&router, "GET", "/api/ledger/entries", None).await,
        ledger_before
    );
}

async fn engagement_rollforward_origin_acceptance() {
    // A separate in-memory app makes preservation assertions independent from
    // other acceptance scenarios. These are synthetic HTTP calls, not services.
    let router = app_with(Store::memory);
    ok(&router,"PUT","/api/ledger/company",Some(json!({"name":"Synthetic origin sentinel","currency":"CAD","province":"AB","default_tax_code":"GST"}))).await;
    ok(&router,"GET","/api/ledger/accounts",None).await;
    ok(&router,"POST","/api/ledger/entries",Some(json!({"date":"2025-01-02","memo":"Synthetic origin preservation sentinel","lines":[{"account_id":"acc_1000","debit":10_000},{"account_id":"acc_3000","credit":10_000}]}))).await;
    let tax_sentinel = ok(&router,"POST","/api/tax/returns",Some(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic origin Tax sentinel","client_ref":"synthetic-only","inputs":{},"parties":[],"notes":"Preserve this draft exactly."}))).await;
    let firm_sentinel = ok(&router,"POST","/api/firm/projects",Some(json!({"name":"Synthetic origin Firm sentinel","tasks":[{"title":"Preserve this task exactly"}]}))).await;
    let ledger_before = ok(&router,"GET","/api/ledger/entries",None).await;
    let tax_before = ok(&router,"GET","/api/tax/returns",None).await;
    let firm_before = ok(&router,"GET","/api/firm/projects",None).await;
    assert_eq!(ledger_before.as_array().unwrap().len(),1);
    assert_eq!(tax_before.as_array().unwrap().len(),1);
    assert_eq!(firm_before.as_array().unwrap().len(),1);

    let source = ok(&router,"POST","/api/engagements",Some(fixture())).await;
    assert!(source.get("rollforward_origin").is_none_or(Value::is_null),"An ordinary new engagement has no fabricated rollforward origin");
    let source = calculate(&router,&source).await;
    let source = signoff(&router,&source,"wp_lead","prepared").await;
    let source = signoff(&router,&source,"wp_review","prepared").await;
    let source = transition(&router,&source,"in_review","Review synthetic origin source.").await;
    let source = signoff(&router,&source,"wp_lead","reviewed").await;
    let source = signoff(&router,&source,"wp_review","reviewed").await;
    let source = transition(&router,&source,"locked","Freeze the exact synthetic comparative source.").await;
    let source_snapshot = source["snapshot"].clone();
    let next_period = json!({"expected_revision":revision(&source),"period_start":"2026-01-01","period_end":"2026-12-31","name":"Synthetic immutable origin target"});

    let before_rollforward = ok(&router,"GET","/api/engagements",None).await;
    let mut stale = next_period.clone(); stale["expected_revision"] = json!(revision(&source)-1);
    fails(&router,"POST",&endpoint(&source,"/rollforward"),Some(stale),409).await;
    assert_eq!(ok(&router,"GET","/api/engagements",None).await,before_rollforward,"A stale rollforward cannot create a target");
    assert_eq!(ok(&router,"GET",&endpoint(&source,""),None).await,source);

    let next = ok(&router,"POST",&endpoint(&source,"/rollforward"),Some(next_period.clone())).await;
    let origin = next["rollforward_origin"].clone();
    assert!(origin.is_object(),"A rolled-forward record must carry structured source provenance");
    assert_eq!(next["rollforward_from"],source["id"]);
    assert_eq!(origin["source_engagement_id"],source["id"]);
    assert_eq!(origin["source_snapshot_id"],source_snapshot["id"]);
    assert_eq!(origin["source_input_revision"],source_snapshot["input_revision"]);
    assert_eq!(origin["source_input_hash"],source_snapshot["input_hash"]);
    assert_eq!(origin["source_output_hash"],source_snapshot["output_hash"]);
    assert_eq!(origin["source_period_start"],source["document"]["period_start"]);
    assert_eq!(origin["source_period_end"],source["document"]["period_end"]);
    assert_eq!(origin["target_period_start"],"2026-01-01");
    assert_eq!(origin["target_period_end"],"2026-12-31");
    assert!(chrono::DateTime::parse_from_rfc3339(origin["copied_at"].as_str().unwrap()).is_ok());
    assert_eq!(origin["source_snapshot"],source_snapshot,"Retain the exact snapshot, not a pointer to the mutable source record");
    assert!(origin["source_snapshot"].get("rollforward_origin").is_none(),"The embedded snapshot is not a recursive engagement archive");
    let preimage:nxt_os::engagement::models::EngagementInput=serde_json::from_value(origin["source_snapshot"]["input_document"].clone()).unwrap();
    assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&preimage).unwrap())),origin["source_input_hash"].as_str().unwrap());
    assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&origin["source_snapshot"]["result"]).unwrap())),origin["source_output_hash"].as_str().unwrap());
    let policy=&origin["copy_policy"];
    assert_eq!(policy["version"],"nxt-engagement-rollforward/v1");
    assert_eq!(policy["comparatives"],"source_snapshot_adjusted_trial_balance");
    assert_eq!(policy["current_balances"],"zero");
    assert_eq!(policy["name"],"source_unless_explicitly_overridden");
    assert_eq!(policy["source_evidence"],"embedded_source_snapshot_archive_only");
    assert_eq!(policy["issues"],"open_and_carry_forward_only");
    assert_eq!(policy["source_mutated"],false);
    assert_eq!(policy["external_modules_mutated"],false);
    assert_eq!(policy["preserved_fields"],json!([
        "client_ref","engagement_kind","currency","account_ids_codes_names_kinds_and_groups",
        "group_definitions_and_gifi_metadata","workpaper_ids_references_titles_folders_and_dependencies",
        "workpaper_account_references"
    ]));
    assert_eq!(policy["cleared_fields"],json!([
        "current_account_balances","local_adjustments","workpaper_bodies","workpaper_annotations","workpaper_adjustment_references",
        "notes","active_calculation","calculation_history","signoffs","signoff_history","lock_history","import_history"
    ]));

    assert_eq!(next["status"],"draft");
    assert!(next["snapshot"].is_null());
    assert_eq!(next["snapshots"],json!([]));
    assert_eq!(next["signoffs"],json!([]));
    assert_eq!(next["signoff_history"],json!([]));
    assert_eq!(next["document"]["adjustments"],json!([]));
    assert_eq!(next["document"]["prior_period_start"],source["document"]["period_start"]);
    assert_eq!(next["document"]["prior_period_end"],source["document"]["period_end"]);
    assert_eq!(next["document"]["groups"],source["document"]["groups"]);
    for account in next["document"]["accounts"].as_array().unwrap() {
        let row=source_snapshot["result"]["accounts"].as_array().unwrap().iter().find(|row|row["id"]==account["id"]).unwrap();
        assert_eq!(account["prior_debit_cents"],row["adjusted_debit_cents"]);
        assert_eq!(account["prior_credit_cents"],row["adjusted_credit_cents"]);
        assert_eq!(account["debit_cents"],0); assert_eq!(account["credit_cents"],0);
    }
    assert!(next["document"]["workpapers"].as_array().unwrap().iter().all(|p|p["body"]==""&&p["adjustment_ids"]==json!([])));
    let after_rollforward = ok(&router,"GET","/api/engagements",None).await;
    fails(&router,"POST",&endpoint(&source,"/rollforward"),Some(next_period),409).await;
    assert_eq!(ok(&router,"GET","/api/engagements",None).await,after_rollforward,"A duplicate source-period rollforward cannot create another target");
    assert_eq!(ok(&router,"GET",&endpoint(&next,""),None).await,next);
    assert_eq!(ok(&router,"GET",&endpoint(&source,""),None).await,source,"Rollforward cannot mutate the locked source");

    // A draft's planning dates may change, but they cannot free the original
    // copy slot: replay protection binds the immutable origin destination.
    let mut replanned=editable(&next);
    replanned["period_start"]=json!("2027-01-01");
    replanned["period_end"]=json!("2027-12-31");
    let replanned=ok(&router,"PUT",&endpoint(&next,""),Some(replanned)).await;
    assert_eq!(replanned["rollforward_origin"],origin);
    let before_replay=ok(&router,"GET","/api/engagements",None).await;
    fails(&router,"POST",&endpoint(&source,"/rollforward"),Some(json!({
        "expected_revision":revision(&source),"period_start":"2026-01-01","period_end":"2026-12-31"
    })),409).await;
    assert_eq!(ok(&router,"GET","/api/engagements",None).await,before_replay);
    let mut restored=editable(&replanned);
    restored["period_start"]=json!("2026-01-01");
    restored["period_end"]=json!("2026-12-31");
    let next=ok(&router,"PUT",&endpoint(&replanned,""),Some(restored)).await;
    assert_eq!(next["rollforward_origin"],origin);

    let reopened = transition(&router,&source,"draft","Correct source after its exact snapshot was copied.").await;
    assert!(reopened["snapshot"].is_null());
    let mut edit=editable(&reopened);
    edit["notes"]=json!("Synthetic source changed after rollforward; prior origin must not follow this edit.");
    edit["accounts"][0]["debit_cents"]=json!(1_020_000);
    edit["accounts"][4]["credit_cents"]=json!(1_220_000);
    let changed_source = ok(&router,"PUT",&endpoint(&reopened,""),Some(edit)).await;
    assert!(changed_source["input_revision"].as_u64().unwrap()>origin["source_input_revision"].as_u64().unwrap());
    assert!(changed_source["snapshot"].is_null());
    assert!(changed_source["snapshots"].as_array().unwrap().contains(&source_snapshot));
    let retained = ok(&router,"GET",&endpoint(&next,""),None).await;
    assert_eq!(retained,next,"Changing the source cannot rewrite the target or its comparatives");
    assert_eq!(retained["rollforward_origin"],origin);
    let package = ok(&router,"GET",&endpoint(&next,"/package"),None).await;
    assert_eq!(package["engagement"]["rollforward_origin"],origin,"Native export preserves the self-contained origin after the source changes");

    let mut target_edit=editable(&retained); target_edit["notes"]=json!("Synthetic next-period planning note.");
    let changed_target=ok(&router,"PUT",&endpoint(&retained,""),Some(target_edit.clone())).await;
    assert_eq!(changed_target["rollforward_origin"],origin,"Target edits cannot replace immutable origin metadata");
    fails(&router,"PUT",&endpoint(&retained,""),Some(target_edit),409).await;
    assert_eq!(ok(&router,"GET",&endpoint(&retained,""),None).await,changed_target);
    assert_eq!(ok(&router,"GET","/api/ledger/entries",None).await,ledger_before);
    assert_eq!(ok(&router,"GET","/api/tax/returns",None).await,tax_before);
    assert_eq!(ok(&router,"GET",&format!("/api/tax/returns/{}",id(&tax_sentinel)),None).await,tax_sentinel);
    assert_eq!(ok(&router,"GET","/api/firm/projects",None).await,firm_before);
    assert_eq!(ok(&router,"GET",&format!("/api/firm/projects/{}",id(&firm_sentinel)),None).await,firm_sentinel);
    assert_eq!(ok(&router,"GET","/api/firm/runs",None).await,json!([]));
    assert_eq!(ok(&router,"GET","/api/firm/approvals",None).await,json!([]));
}

async fn engagement_bridge_acceptance() {
    // Explicit service references match app composition while each module owns
    // a separate memory Store; Ledger capture can inspect its exact source.
    let ledger=Arc::new(nxt_os::ledger::engine::Ledger::new(Store::memory()));
    let firm=Arc::new(nxt_os::firm::Firm::new(Store::memory(),ledger.clone()));
    let tax=Arc::new(nxt_os::tax::Tax::new(Store::memory()));
    let engagements=Arc::new(nxt_os::engagement::Engagements::new(Store::memory(),ledger.clone(),tax.clone(),firm.clone()));
    let router=axum::Router::new()
        .merge(nxt_os::ledger::routes::router(ledger.clone()))
        .merge(nxt_os::firm::router(firm))
        .merge(nxt_os::tax::router(tax))
        .merge(nxt_os::engagement::router(engagements));
    let mut empty = fixture();
    for key in ["accounts","groups","adjustments","workpapers","issues"] {empty[key]=json!([]);}
    empty["name"]=json!("Synthetic Ledger capture target");
    let target = ok(&router,"POST","/api/engagements",Some(empty)).await;
    fails(&router,"POST",&endpoint(&target,"/ledger/preview"),Some(json!({"expected_revision":revision(&target)})),409).await;
    assert!(ledger.store.smembers("gl:accounts").await.unwrap().is_empty(),"Capture must not seed the source chart");
    ok(&router,"PUT","/api/ledger/company",Some(json!({"name":"Synthetic bridge source","currency":"CAD","province":"AB","default_tax_code":"GST"}))).await;
    let source_accounts = ok(&router,"GET","/api/ledger/accounts",None).await;
    ok(&router,"POST","/api/ledger/entries",Some(json!({"date":"2025-01-02","memo":"Synthetic bridge capital","lines":[{"account_id":"acc_1000","debit":1_000_000},{"account_id":"acc_3000","credit":1_000_000}]}))).await;
    let source_entries = ok(&router,"GET","/api/ledger/entries",None).await;
    let preview = ok(&router,"POST",&endpoint(&target,"/ledger/preview"),Some(json!({"expected_revision":revision(&target)}))).await;
    assert_eq!(preview["provenance"]["source"],"nxt_ledger");
    assert_eq!(preview["provenance"]["consistency"],"stable_double_read_not_transactional");
    assert_eq!(preview["provenance"]["source_write_freeze_verified"],false);
    assert_eq!(preview["provenance"]["ledger_writes"],false);
    assert_eq!(preview["provenance"]["source_accounts"].as_u64().unwrap() as usize,source_accounts.as_array().unwrap().len());
    assert_eq!(ok(&router,"GET",&endpoint(&target,""),None).await,target);
    let captured = ok(&router,"POST",&endpoint(&target,"/import/commit"),Some(json!({"expected_revision":revision(&target),"preview_id":id(&preview)}))).await;
    let captured_accounts = captured["document"]["accounts"].as_array().unwrap();
    assert_eq!(captured_accounts.len(),source_accounts.as_array().unwrap().len(),"Zero-balance source accounts are retained");
    assert_eq!(captured_accounts.iter().find(|a|a["id"]=="acc_1000").unwrap()["debit_cents"],1_000_000);
    assert_eq!(captured_accounts.iter().find(|a|a["id"]=="acc_3000").unwrap()["credit_cents"],1_000_000);
    assert_eq!(captured["import_history"][0]["provenance"]["source"],"nxt_ledger");
    assert_eq!(captured["import_history"][0]["source_hash"],preview["source_hash"]);
    assert_eq!(ok(&router,"GET","/api/ledger/entries",None).await,source_entries);
    // The bridge does not silently infer retained earnings from historical P&L.
    ok(&router,"POST","/api/ledger/entries",Some(json!({"date":"2024-12-31","memo":"Synthetic unclosed prior expense","lines":[{"account_id":"acc_1000","credit":100},{"account_id":"acc_6600","debit":100}]}))).await;
    fails(&router,"POST",&endpoint(&captured,"/ledger/preview"),Some(json!({"expected_revision":revision(&captured)})),409).await;

    let source = ok(&router,"POST","/api/engagements",Some(fixture())).await;
    fails(&router,"POST",&endpoint(&source,"/tax/preview"),Some(json!({"expected_revision":revision(&source),"jurisdiction":"AB"})),409).await;
    let source = calculate(&router,&source).await;
    let source = signoff(&router,&source,"wp_lead","prepared").await;
    let source = signoff(&router,&source,"wp_review","prepared").await;
    let source = transition(&router,&source,"in_review","Review synthetic bridge source.").await;
    let source = signoff(&router,&source,"wp_lead","reviewed").await;
    let source = signoff(&router,&source,"wp_review","reviewed").await;
    let source = transition(&router,&source,"locked","Release synthetic bridge source for draft projection.").await;
    let tax_request = json!({"expected_revision":revision(&source),"jurisdiction":"AB"});
    let tax_preview = ok(&router,"POST",&endpoint(&source,"/tax/preview"),Some(tax_request.clone())).await;
    assert_eq!(tax_preview["filing_enabled"],false);
    assert_eq!(tax_preview["taxable_income_inferred"],false);
    assert_eq!(tax_preview["creates_draft_only_after_commit"],true);
    assert!(tax_preview["target_preview_hash_scope"].as_str().unwrap().contains("canonical JSON proposal"));
    assert_eq!(tax_preview["source"]["input_hash"],source["snapshot"]["input_hash"]);
    assert_eq!(tax_preview["source"]["output_hash"],source["snapshot"]["output_hash"]);
    assert_eq!(ok(&router,"GET","/api/tax/returns",None).await,json!([]));
    let preview_id = id(&tax_preview["preview"]);
    let stored_preview = ok(&router,"GET",&format!("/api/tax/import/previews/{preview_id}"),None).await;
    let proposed = &stored_preview["document"];
    assert_eq!(proposed["program"],"T2"); assert_eq!(proposed["tax_year"],2025); assert_eq!(proposed["jurisdiction"],"AB");
    assert_eq!(proposed["inputs"]["scope_confirmed"],false);
    assert_eq!(proposed["inputs"]["taxable_income_cents"],0);
    assert_eq!(proposed["corporate"]["reconciliation_confirmed"],false);
    assert_eq!(proposed["corporate"]["legal_name"],"");
    let gifi = proposed["corporate"]["gifi"].as_array().unwrap();
    for (code,amount) in [("2599",1_700_000),("3499",400_000),("3620",1_300_000),("8299",900_000),("9368",800_000),("9999",100_000)] {
        assert_eq!(gifi.iter().find(|row|row["code"]==code).unwrap()["current_cents"],amount,"Aggregate {code}");
    }
    let imported = ok(&router,"POST","/api/tax/import/commit",Some(json!({"preview_id":preview_id}))).await;
    assert_eq!(imported["status"],"draft"); assert!(imported["calculation"].is_null()); assert!(imported["review"].is_null());
    assert_eq!(ok(&router,"POST","/api/tax/import/commit",Some(json!({"preview_id":preview_id}))).await,imported);
    let repeat = ok(&router,"POST",&endpoint(&source,"/tax/preview"),Some(tax_request)).await;
    assert_eq!(repeat["preview"]["id"],tax_preview["preview"]["id"]);
    assert_eq!(ok(&router,"GET","/api/tax/returns",None).await.as_array().unwrap().len(),1);
    assert_eq!(ok(&router,"GET",&endpoint(&source,""),None).await,source,"Tax proposal cannot alter the engagement source");
    fails(&router,"POST",&endpoint(&source,"/tax/preview"),Some(json!({"expected_revision":revision(&source),"jurisdiction":"ON"})),409).await;

    let project = ok(&router,"POST","/api/firm/projects",Some(json!({"name":"Synthetic engagement preparation","tasks":[]}))).await;
    let proposal_request = json!({"expected_revision":revision(&source),"project_id":id(&project)});
    let proposal = ok(&router,"POST",&endpoint(&source,"/firm-proposal"),Some(proposal_request.clone())).await;
    assert_eq!(proposal["human_approval_required"],true); assert_eq!(proposal["execution_requested"],false); assert_eq!(proposal["cross_module_atomic"],false);
    assert_eq!(proposal["proposal"]["status"],"proposed"); assert_eq!(proposal["proposal"]["requires_approval"],true);
    let replay = ok(&router,"POST",&endpoint(&source,"/firm-proposal"),Some(proposal_request)).await;
    assert_eq!(replay["proposal"]["id"],proposal["proposal"]["id"]);
    let project_path = format!("/api/firm/projects/{}",id(&project));
    assert_eq!(ok(&router,"GET",&project_path,None).await["tasks"],json!([]));
    let decision_path = format!("/api/firm/approvals/{}/decide",proposal["proposal"]["approval_id"].as_str().unwrap());
    let accepted = ok(&router,"POST",&decision_path,Some(json!({"decision":"approve","note":"Synthetic human accepted preparation task."}))).await;
    assert_eq!(accepted["status"],"executed");
    ok(&router,"POST",&decision_path,Some(json!({"decision":"approve","note":"Synthetic human accepted preparation task."}))).await;
    assert_eq!(ok(&router,"GET",&project_path,None).await["tasks"].as_array().unwrap().len(),1,"Approval replay cannot execute a second task");
}
