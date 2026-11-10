//! Synthetic regression for exact year/jurisdiction dispatch and historical scope gates.
use axum::{body::Body,http::Request};
use nxt_os::{app_with,store::Store,tax::{Tax,calculations,filing_desk,t183,models::*}};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use tower::ServiceExt;

fn inputs() -> Value { json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":5_000_000}) }
fn input(year:i32,jurisdiction:&str) -> ReturnInput { serde_json::from_value(json!({"program":"T1","tax_year":year,"jurisdiction":jurisdiction,"name":"TEST-scope guard","client_ref":"TEST-synthetic-only","inputs":inputs()})).unwrap() }
fn hash<T:serde::Serialize>(v:&T)->String { hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap())) }
async fn call(router:&axum::Router,method:&str,uri:&str,body:Option<Value>)->(u16,Value) {
    let request=Request::builder().method(method).uri(uri).header("authorization","Bearer scope-test-key").header("content-type","application/json").body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let response=router.clone().oneshot(request).await.unwrap();let status=response.status().as_u16();let b=axum::body::to_bytes(response.into_body(),4*1024*1024).await.unwrap();(status,serde_json::from_slice(&b).unwrap_or_else(|_|json!({"html":String::from_utf8_lossy(&b)})))
}
#[test]
fn exact_scope_dispatch_has_no_alberta_fallback() {
    for (year,jurisdiction) in [(2025,"QC"),(2024,"AB"),(2027,"BC"),(2025,"CA"),(2026,"CA-FED")] {
        let c=calculations::calculate("T1",year,jurisdiction,&inputs()).unwrap();
        assert!(!c.supported,"{year}/{jurisdiction}");assert!(c.lines.is_empty());assert!(c.totals.is_empty());assert!(c.diagnostics.iter().any(|d|d.blocks_review));
    }
    for jurisdiction in ["AB","BC","ON"] { let c=calculations::calculate("T1",2025,jurisdiction,&inputs()).unwrap();assert!(c.supported,"{jurisdiction}: {:?}",c.diagnostics);assert!(c.totals.contains_key("income_tax")); }
    let q=calculations::calculate("T1",2026,"QC",&json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"payroll_verified":true,"rrsp_verified":true,"employment_income_cents":10_400_000,"rrsp_deduction_cents":520_000,"rrsp_deduction_limit_cents":520_000,"cpp_enhanced_deduction_cents":100_492,"cpp_base_credit_cents":376_830,"qpip_premium_cents":44_290})).unwrap();
    assert!(q.supported,"{:?}",q.diagnostics);assert!(q.lines.iter().any(|l|l.code=="QC299"));
    assert!(calculations::supports_scope("T2",2026,"AB"));assert!(!calculations::supports_scope("T2",2025,"QC"));
}
#[tokio::test]
async fn invalid_scope_cannot_replay_review_authorize_or_record_filing() {
    unsafe { std::env::set_var("ADMIN_API_KEY","scope-test-key"); }
    let store=Store::memory();let tax=Tax::new(store.clone());let router=app_with(||store.clone());
    let created=tax.save(None,input(2025,"AB")).await.ok().unwrap();let valid=tax.calculate(&created.id,RevisionInput{expected_revision:created.revision,idempotency_key:Some("old-key".into())}).await.ok().unwrap();
    // Simulate a valid-hash persisted record from the former QC->AB dispatcher. No real data.
    let mut legacy=valid.clone();legacy.jurisdiction="QC".into();let preimage=ReturnInput::from(&legacy);let ih=hash(&preimage);
    for c in legacy.calculations.iter_mut().chain(legacy.calculation.iter_mut()) { c.input_document=Some(preimage.clone());c.input_hash=ih.clone();assert_eq!(c.output_hash,hash(&c.result));assert_eq!(c.result["supported"],true); }
    let preserved=serde_json::to_value(&legacy.calculation).unwrap();let mut state:TaxState=store.get_json("tax:state:v1").await.unwrap().unwrap();state.returns[0]=legacy.clone();store.set_json("tax:state:v1",&state).await.unwrap();
    let uri=format!("/api/tax/returns/{}",legacy.id);
    let (status,detail)=call(&router,"GET",&uri,None).await;assert_eq!(status,200);assert_eq!(detail["current_scope_supported"],false);assert_eq!(detail["calculation"],preserved);
    let (_,list)=call(&router,"GET","/api/tax/returns",None).await;assert_eq!(list[0]["current_scope_supported"],false);assert_eq!(list[0]["calculation"]["result"]["supported"],false);
    let (_,html)=call(&router,"GET",&(uri.clone()+"/print"),None).await;assert!(html["html"].as_str().unwrap().contains("Unsupported current calculation scope"));
    let (_,package)=call(&router,"GET",&(uri.clone()+"/package"),None).await;assert_eq!(package["current_scope_supported"],false);assert_eq!(package["calculation_snapshot"],preserved);
    let body=json!({"expected_revision":legacy.revision,"idempotency_key":"old-key"});assert_eq!(call(&router,"POST",&(uri.clone()+"/calculate"),Some(body)).await.0,409);
    for old_status in ["calculated","in_review","approved","filed"] {
        state.returns[0]=legacy.clone();state.returns[0].status=old_status.into();store.set_json("tax:state:v1",&state).await.unwrap();
        for requested in ["in_review","approved","filed"] {
            let body=json!({"expected_revision":legacy.revision,"status":requested,"reviewer":"Synthetic operator","note":"synthetic regression","filing_reference":"TEST-external","t183_envelope_id":"TEST-envelope"});
            assert_eq!(call(&router,"POST",&(uri.clone()+"/transition"),Some(body)).await.0,409,"{old_status}->{requested}");
        }
        assert!(filing_desk::blockers(&state.returns[0]).iter().any(|b|b["code"]=="unsupported_current_scope"));assert!(t183::draft(&state.returns[0]).is_err());
        assert_eq!(call(&router,"POST",&(uri.clone()+"/filing"),None).await.0,409);
    }
    state.returns[0]=legacy.clone();store.set_json("tax:state:v1",&state).await.unwrap();
    let (status,blocked)=call(&router,"POST",&(uri.clone()+"/calculate"),Some(json!({"expected_revision":legacy.revision}))).await;assert_eq!(status,200);assert_eq!(blocked["current_scope_supported"],false);assert_eq!(blocked["calculation"]["result"]["supported"],false);assert!(blocked["calculation"]["result"]["lines"].as_array().unwrap().is_empty());assert_eq!(blocked["calculations"][0],preserved);assert_eq!(blocked["calculations"].as_array().unwrap().len(),2);
    // API accepts unsupported preparation drafts, but calculating them cannot produce a tax result.
    let (status,new_qc)=call(&router,"POST","/api/tax/returns",Some(serde_json::to_value(input(2025,"QC")).unwrap())).await;assert_eq!(status,200);assert_eq!(new_qc["current_scope_supported"],false);
    for (year,jurisdiction) in [(2025,"AB"),(2025,"BC"),(2026,"QC")] {
        let mut doc=serde_json::to_value(input(year,jurisdiction)).unwrap();
        if year==2026 { doc["inputs"]=json!({"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"payroll_verified":true,"rrsp_verified":true,"employment_income_cents":10_400_000,"rrsp_deduction_cents":520_000,"rrsp_deduction_limit_cents":520_000,"cpp_enhanced_deduction_cents":100_492,"cpp_base_credit_cents":376_830,"qpip_premium_cents":44_290}); }
        let (status,r)=call(&router,"POST","/api/tax/returns",Some(doc)).await;assert_eq!(status,200);assert_eq!(r["current_scope_supported"],true);
        let u=format!("/api/tax/returns/{}/calculate",r["id"].as_str().unwrap());let (status,c)=call(&router,"POST",&u,Some(json!({"expected_revision":r["revision"]}))).await;assert_eq!(status,200);assert_eq!(c["calculation"]["result"]["supported"],true,"{year}/{jurisdiction}: {c}");
        if jurisdiction=="QC" { assert!(c["calculation"]["result"]["lines"].as_array().unwrap().iter().any(|l|l["code"]=="QC299")); }
    }
}
