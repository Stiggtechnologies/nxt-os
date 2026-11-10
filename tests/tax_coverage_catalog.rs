//! Synthetic coverage/entry metadata checks; never certification or full-return equivalence.
use axum::{body::Body,http::Request};
use nxt_os::{app_with,store::Store,tax::{Tax,coverage,calculations,models::ReturnInput}};
use serde_json::{json,Value};
use tower::ServiceExt;

async fn get(r:&axum::Router,path:&str,auth:bool)->(u16,Value) {
    let mut b=Request::builder().uri(path);if auth {b=b.header("authorization","Bearer coverage-test-key");}
    let response=r.clone().oneshot(b.body(Body::empty()).unwrap()).await.unwrap();let status=response.status().as_u16();let bytes=axum::body::to_bytes(response.into_body(),2*1024*1024).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn coverage_is_authenticated_consistent_read_only_and_keeps_2026_defaults_unexposed() {
    unsafe {std::env::set_var("ADMIN_API_KEY","coverage-test-key");}
    let store=Store::memory();let tax=Tax::new(store.clone());
    let i:ReturnInput=serde_json::from_value(json!({"program":"T1","tax_year":2026,"jurisdiction":"QC","name":"TEST-existing API workpaper","inputs":{"scope_confirmed":false,"qpip_premium_cents":123,"eligible_dependant_net_income_cents":456},"notes":"Synthetic read-only preservation test"})).unwrap();
    tax.save(None,i).await.ok().unwrap();let before=store.get("tax:state:v1").await.unwrap();let router=app_with(||store.clone());
    assert_eq!(get(&router,"/api/tax/catalog",false).await.0,401);
    let (status,catalog)=get(&router,"/api/tax/catalog",true).await;assert_eq!(status,200);let c=&catalog["coverage"];assert_eq!(c, &coverage::catalog());
    let (_,cap)=get(&router,"/api/tax/capabilities",true).await;assert_eq!(&cap["coverage"],c);let (_,rates)=get(&router,"/api/tax/rates",true).await;assert_eq!(rates,cap["rate_tables"]);assert_eq!(rates["calculation_years"],json!([2025,2026]));assert_eq!(rates["browser_entry_years"],json!([2025]));assert_eq!(rates["tables"],nxt_os::tax::rates::catalog()["tables"],"rate tables must remain exact");let (_,summary)=get(&router,"/api/tax/summary",true).await;assert_eq!(summary["calculation_years"],json!([2025,2026]));assert_eq!(summary["browser_entry_years"],json!([2025]));assert_eq!(summary["filing_enabled"],false);
    for p in catalog["programs"].as_array().unwrap() {
        let program=p["id"].as_str().unwrap();assert_eq!(p["years"],json!([2025]));assert_eq!(p["entry_schema_year"],2025);assert_eq!(p["entry_schema_id"],coverage::entry_schema_id(program));
        for j in p["jurisdictions"].as_array().unwrap() {
            let jurisdiction=j.as_str().unwrap();assert!(coverage::entry_schema_ready(program,2025,jurisdiction));let mut inputs=p["defaults"].clone();inputs["scope_confirmed"]=json!(true);
            match program {"T1"=>{inputs["full_year_resident"]=json!(true);inputs["provincial_adjustments_not_applicable"]=json!(true);inputs["employment_income_cents"]=json!(5_000_000);},"T5013"=>inputs["allocation_verified"]=json!(true),_=>()}
            let result=calculations::calculate(program,2025,jurisdiction,&inputs).unwrap();assert!(result.supported,"{program}/{jurisdiction}: {:?}",result.diagnostics);
        }
    }
    let row=|year,j:&str| c["scopes"].as_array().unwrap().iter().find(|r|r["program"]=="T1"&&r["tax_year"]==year&&r["jurisdiction"]==j).unwrap();
    assert_eq!(row(2025,"QC")["engine_scope_available"],false);assert_eq!(row(2025,"QC")["browser_entry_available"],false);assert_eq!(row(2026,"QC")["engine_scope_available"],true);assert_eq!(row(2026,"QC")["browser_entry_available"],false);assert!(row(2026,"QC")["entry_schema_id"].is_null());
    assert_eq!(store.get("tax:state:v1").await.unwrap(),before,"coverage GETs cannot rewrite or reset saved inputs");
}

#[tokio::test]
async fn profile_only_future_drafts_do_not_reuse_template_inputs_or_change_the_original() {
    use nxt_os::tax::models::RollforwardInput;
    use nxt_os::tax::t2_intake::IntakeAnswers;
    let tax=Tax::new(Store::memory());
    let input:ReturnInput=serde_json::from_value(json!({"program":"T2","tax_year":2026,"jurisdiction":"AB","name":"TEST-original profile","client_ref":"TEST-carry","period_start":"2026-01-01","period_end":"2026-12-31","inputs":{"taxable_income_cents":98765,"loss_closing_cents":12345,"scope_confirmed":true}})).unwrap();
    let prior=tax.save(None,input).await.ok().unwrap();let original=serde_json::to_vec(&prior).unwrap();
    let next=tax.rollforward(&prior.id,RollforwardInput{expected_revision:prior.revision,tax_year:2027,name:None}).await.ok().unwrap();assert_eq!(next.inputs,json!({"loss_opening_cents":12345}));assert!(next.calculation.is_none());assert!(next.review.is_none());assert_eq!(next.status,"draft");assert!(!calculations::supports_scope("T2",2027,"AB"));
    let date=chrono::NaiveDate::from_ymd_opt(2028,1,1).unwrap();
    let fresh:IntakeAnswers=serde_json::from_value(json!({"incorporated":true,"location":"AB","period_start":"2027-01-01","period_end":"2027-12-31","had_income":true,"client_ref":"TEST-empty"})).unwrap();
    let empty=tax.t2_intake_start(&fresh,"TEST-empty future",None,date).await.ok().unwrap();assert_eq!(empty["return"]["inputs"],json!({}));assert_eq!(empty["return"]["status"],"draft");assert!(empty["return"]["calculation"].is_null());
    let carry:IntakeAnswers=serde_json::from_value(json!({"incorporated":true,"location":"AB","period_start":"2027-01-01","period_end":"2027-12-31","had_income":true,"client_ref":"TEST-carry"})).unwrap();
    let copied=tax.t2_intake_start(&carry,"TEST-carried future",Some(&prior.id),date).await.ok().unwrap();assert_eq!(copied["return"]["inputs"],json!({"loss_opening_cents":12345}));assert!(copied["return"]["calculation"].is_null());assert_eq!(serde_json::to_vec(&tax.get(&prior.id).await.ok().unwrap()).unwrap(),original);
}
