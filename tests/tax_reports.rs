use axum::{body::Body,http::Request};
use nxt_os::{app_with,store::Store,tax::{Tax,reports::{self,ReportRequest},models::{ReturnInput,TaxReturn,CalculationSnapshot}},firm::staff::{self,Identity,Role}};
use serde_json::{json,Value};
use sha2::{Sha256,Digest};
use tower::ServiceExt;
fn input(client:&str,name:&str)->ReturnInput {serde_json::from_value(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":name,"client_ref":client,"inputs":{"employment_income_cents":0,"scope_confirmed":false}})).ok().unwrap()}
async fn record(client:&str,name:&str)->TaxReturn {Tax::new(Store::memory()).save(None,input(client,name)).await.ok().unwrap()}
fn actor(client:&str)->Identity{Identity{user_id:"synthetic".into(),display_name:"Synthetic scoped operator".into(),role:Role::Preparer,clients:vec![client.into()],break_glass:false,mfa:true,must_change_password:false}}
fn req()->ReportRequest{serde_json::from_value(json!({"columns":["name","client_ref","input:employment_income_cents","line:42000","total:income_tax"]})).ok().unwrap()}
fn snapshot(r:&mut TaxReturn){let doc=ReturnInput::from(&*r);let hash=hex::encode(Sha256::digest(serde_json::to_vec(&doc).ok().unwrap()));let result=json!({"ruleset":"synthetic fixture","supported":true,"diagnostics":[],"totals":{"income_tax":0},"lines":[{"code":"42000","amount":0}]});let output_hash=hex::encode(Sha256::digest(serde_json::to_vec(&result).ok().unwrap()));r.calculation=Some(CalculationSnapshot{id:"synthetic_calc".into(),input_revision:r.input_revision,request_revision:r.revision,idempotency_key:None,input_document:Some(doc),input_hash:hash,output_hash,created_at:r.created_at.clone(),result});}
#[tokio::test]
async fn report_scopes_before_filter_count_and_fingerprint_and_does_not_mutate(){
 let a=record("A","=Élodie, quoted\nname").await;let mut b=record("B","SECRET").await;let before=serde_json::to_vec(&a).ok().unwrap();let who=actor("A");
 let first=reports::project(&[a.clone(),b.clone()],req(),Some(&who)).ok().unwrap();assert_eq!(first["row_count"],1);assert_eq!(first["rows"][0]["name"],a.name);assert_eq!(first["rows"][0]["input:employment_income_cents"],0);assert!(first["rows"][0]["line:42000"].is_null());
 b.name="other hidden content".into();b.revision+=1;let second=reports::project(&[a.clone(),b],req(),Some(&who)).ok().unwrap();assert_eq!(first,second,"hidden-client changes cannot affect count or snapshot hash");assert_eq!(serde_json::to_vec(&a).ok().unwrap(),before);
 let mut outside=req();outside.client_ref=Some("B".into());assert_eq!(reports::project(&[a],outside,Some(&who)).unwrap_err().0.as_u16(),403);
}
#[tokio::test]
async fn saved_zero_blank_stale_and_unsupported_amounts_remain_distinct(){
 let mut a=record("A","current").await;snapshot(&mut a);let current=reports::project(&[a.clone()],req(),None).ok().unwrap();assert_eq!(current["rows"][0]["calculation_state"],"current_supported");assert_eq!(current["rows"][0]["line:42000"],0);
 if let Ok(dir)=std::env::var("NXT_TAX_REPORT_FIXTURE_DIR") {let root=std::path::Path::new(&dir);std::fs::create_dir(root).ok().unwrap();let fixture=reports::project(&[a.clone()],ReportRequest::default(),None).ok().unwrap();std::fs::write(root.join("catalog.json"),serde_json::to_vec(&reports::catalog()).ok().unwrap()).ok().unwrap();std::fs::write(root.join("report.json"),serde_json::to_vec(&fixture).ok().unwrap()).ok().unwrap();std::fs::write(root.join("synthetic-record.json"),serde_json::to_vec(&a).ok().unwrap()).ok().unwrap();}
 let mut corrupt=a.clone();corrupt.calculation.as_mut().unwrap().result["totals"]["income_tax"]=json!(1);let held=reports::project(&[corrupt],req(),None).ok().unwrap();assert_eq!(held["rows"][0]["calculation_state"],"stale");assert!(held["rows"][0]["total:income_tax"].is_null());
 let mut old=a.clone();old.calculation.as_mut().unwrap().input_document=None;assert_eq!(reports::project(&[old],req(),None).ok().unwrap()["rows"][0]["calculation_state"],"stale");
 let mut blocked=a.clone();let calc=blocked.calculation.as_mut().unwrap();calc.result["diagnostics"]=json!([{"severity":"error","blocks_review":true}]);calc.output_hash=hex::encode(Sha256::digest(serde_json::to_vec(&calc.result).ok().unwrap()));let held=reports::project(&[blocked],req(),None).ok().unwrap();assert_eq!(held["rows"][0]["calculation_state"],"current_blocked");assert!(held["rows"][0]["line:42000"].is_null());
 a.inputs["employment_income_cents"]=json!(1);let stale=reports::project(&[a.clone()],req(),None).ok().unwrap();assert_eq!(stale["rows"][0]["calculation_state"],"stale");assert!(stale["rows"][0]["line:42000"].is_null());
 let mut history=req();history.include_historical_results=true;assert_eq!(reports::project(&[a.clone()],history,None).ok().unwrap()["rows"][0]["line:42000"],0);
 a.jurisdiction="QC".into();let qc=reports::project(&[a],req(),None).ok().unwrap();assert_eq!(qc["rows"][0]["calculation_state"],"historical_unsupported_scope");assert!(qc["rows"][0]["total:income_tax"].is_null());
}
#[tokio::test]
async fn numeric_predicates_do_not_treat_missing_as_zero_or_accept_wrong_types(){
 let a=record("A","zero").await;let mut b=record("A","missing").await;b.inputs=json!({});
 let mut r=req();r.predicates=serde_json::from_value(json!([{"column":"input:employment_income_cents","operator":"equals","value":0}])).ok().unwrap();assert_eq!(reports::project(&[a.clone(),b.clone()],r,None).ok().unwrap()["row_count"],1);
 let mut blank=req();blank.predicates=serde_json::from_value(json!([{"column":"input:employment_income_cents","operator":"is_blank","value":null}])).ok().unwrap();assert_eq!(reports::project(&[a.clone(),b],blank,None).ok().unwrap()["row_count"],1);
 let mut unassigned=a.clone();unassigned.id="unassigned".into();unassigned.client_ref="".into();let mut text_blank=req();text_blank.predicates=serde_json::from_value(json!([{"column":"client_ref","operator":"is_blank","value":null}])).ok().unwrap();assert_eq!(reports::project(&[a.clone(),unassigned],text_blank,None).ok().unwrap()["row_count"],1,"empty text is blank while numeric zero is not");
 for predicate in [json!({"column":"input:employment_income_cents","operator":"equals","value":"0"}),json!({"column":"name","operator":"greater_than","value":"0"}),json!({"column":"private:identifier","operator":"is_present","value":null})]{let mut bad=req();bad.predicates=serde_json::from_value(json!([predicate])).ok().unwrap();assert!(reports::project(&[a.clone()],bad,None).is_err());}
}
#[tokio::test]
async fn filters_complete_empty_and_size_limits_fail_without_partial_output(){
 let a=record("A","zero").await;let mut empty=req();empty.years=vec![2024];let out=reports::project(&[a.clone()],empty,None).ok().unwrap();assert_eq!(out["row_count"],0);assert_eq!(out["complete"],true);
 let mut unknown=req();unknown.columns.push("input:unregistered".into());assert!(reports::project(&[a.clone()],unknown,None).is_err());
 let records:Vec<_>=(0..10001).map(|n|{let mut r=a.clone();r.id=format!("{n:05}");r}).collect();let err=reports::project(&records,req(),None).unwrap_err();assert!(err.1.contains("10000"));
 let records:Vec<_>=(0..110).map(|n|{let mut r=a.clone();r.id=n.to_string();r.name="x".repeat(32000);r}).collect();assert!(reports::project(&records,req(),None).unwrap_err().1.contains("3MiB"));
}
#[tokio::test]
async fn report_routes_require_auth_and_read_native_records_without_state_changes(){
 unsafe{std::env::set_var("ADMIN_API_KEY","report-test-key");std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS","1000");}
 staff::install(std::sync::Arc::new(staff::Staff::new(Store::memory())));
 let store=Store::memory();let tax=Tax::new(store.clone());let a=tax.save(None,input("A","Synthetic report")).await.ok().unwrap();let before=tax.get(&a.id).await.ok().unwrap();let router=app_with(||store.clone());
 let b=tax.save(None,input("B","Hidden from scoped reporter")).await.ok().unwrap();
 for(auth,expected)in[(false,401),(true,200)]{
  let mut b=Request::builder().method("POST").uri("/api/tax/reports/preview").header("content-type","application/json");if auth{b=b.header("authorization","Bearer report-test-key");}
  let response=router.clone().oneshot(b.body(Body::from(serde_json::to_vec(&req()).ok().unwrap())).ok().unwrap()).await.ok().unwrap();assert_eq!(response.status().as_u16(),expected);
  if auth{assert_eq!(response.headers()["cache-control"],"private, no-store");let bytes=axum::body::to_bytes(response.into_body(),reports::MAX_BYTES).await.ok().unwrap();let p:Value=serde_json::from_slice(&bytes).ok().unwrap();assert_eq!(p["row_count"],2);assert_eq!(p["financial_approval"],false);}
 }
 async fn call(router:&axum::Router,path:&str,token:&str,body:Value)->(u16,Value){let mut q=Request::builder().method("POST").uri(path).header("content-type","application/json");if !token.is_empty(){q=q.header("authorization",format!("Bearer {token}"));}let r=router.clone().oneshot(q.body(Body::from(body.to_string())).ok().unwrap()).await.ok().unwrap();let code=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),4*1024*1024).await.ok().unwrap();(code,serde_json::from_slice(&bytes).ok().unwrap())}
 for role in ["preparer","read_only","front_desk"]{
  let user=format!("report-{role}");let pw="synthetic-report-password-1";
  assert_eq!(call(&router,"/api/staff/users","report-test-key",json!({"username":user,"display_name":"Synthetic reporter","role":role,"clients":["A"],"temporary_password":pw})).await.0,200);
  let token=call(&router,"/api/staff/login","",json!({"username":user,"password":pw})).await.1["token"].as_str().unwrap().to_owned();
  assert_eq!(call(&router,"/api/tax/reports/preview",&token,serde_json::to_value(req()).ok().unwrap()).await.0,403,"temporary passwords still cannot open reports");
  let secret=call(&router,"/api/staff/mfa/enroll",&token,json!({})).await.1["secret"].as_str().unwrap().to_owned();
  assert_eq!(call(&router,"/api/staff/mfa/confirm",&token,json!({"code":staff::totp_now(&secret).unwrap()})).await.0,200);
  assert_eq!(call(&router,"/api/staff/me/password",&token,json!({"current_password":pw,"new_password":"synthetic-report-password-2"})).await.0,200);
  let (status,p)=call(&router,"/api/tax/reports/preview",&token,serde_json::to_value(req()).ok().unwrap()).await;assert_eq!(status,200);assert_eq!(p["row_count"],1);assert_eq!(p["rows"][0]["id"],a.id);assert!(!p.to_string().contains("Hidden from scoped"));
  let mut outside=req();outside.client_ref=Some("B".into());assert_eq!(call(&router,"/api/tax/reports/preview",&token,serde_json::to_value(outside).ok().unwrap()).await.0,403);
  if role!="preparer"{assert_eq!(call(&router,&format!("/api/tax/returns/{}",a.id),&token,json!({})).await.0,403,"report permission does not permit writes");}
 }
 assert_eq!(serde_json::to_value(b.clone()).ok().unwrap(),serde_json::to_value(tax.get(&b.id).await.ok().unwrap()).ok().unwrap());
 assert_eq!(serde_json::to_value(before).ok().unwrap(),serde_json::to_value(tax.get(&a.id).await.ok().unwrap()).ok().unwrap());
}
