//! Synthetic preparation records only; no imported SOURCE state is involved.
use nxt_os::{store::Store,tax::{Tax,models::*,slips}};
use serde_json::{json,Value};
fn doc(groups:Option<Value>)->ReturnInput{let mut v=json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"TEST durable groups","inputs":{"scope_confirmed":true,"full_year_resident":true,"payroll_verified":false,"donations_cents":123,"credits_verified":false}});if let Some(g)=groups{v["slip_groups"]=g;}serde_json::from_value(v).unwrap()}
fn group(kind:&str,boxes:Value)->Value{json!({"kind":kind,"issuer":"Synthetic payer","boxes":boxes})}
fn rev(r:&TaxReturn)->RevisionInput{RevisionInput{expected_revision:r.revision,idempotency_key:None}}
#[tokio::test]
async fn save_reload_edit_remove_snapshot_package_and_old_put_preserve(){
 let store=Store::memory();let tax=Tax::new(store.clone());let groups=json!([group("T4",json!({"14":12345,"22":0})),group("T4",json!({"14":100})),group("T5",json!({"13":55}))]);
 let r=tax.save(None,doc(Some(groups.clone()))).await.ok().unwrap();assert_eq!(r.inputs["employment_income_cents"],12445);assert_eq!(r.slip_groups.len(),3);assert_eq!(r.inputs["payroll_verified"],false);assert!(r.slip_groups[0].boxes.contains_key("22"));
 let reloaded=Tax::new(store).get(&r.id).await.ok().unwrap();assert_eq!(reloaded.slip_groups,r.slip_groups);
 let calc=tax.calculate(&r.id,rev(&r)).await.ok().unwrap();assert_eq!(serde_json::to_value(calc.calculation.as_ref().unwrap().input_document.as_ref().unwrap().slip_groups.clone()).unwrap(),groups);
 let mut edit=ReturnInput::from(&calc);edit.expected_revision=Some(calc.revision);edit.slip_groups=Some(serde_json::from_value(json!([group("T4",json!({"14":42}))])).unwrap());
 let edited=tax.save(Some(&r.id),edit).await.ok().unwrap();assert_eq!(edited.inputs["employment_income_cents"],42);assert_eq!(edited.inputs["tax_withheld_cents"],0);assert_eq!(edited.inputs["interest_income_cents"],0);assert!(edited.calculation.is_none());assert!(edited.review.is_none());assert_eq!(edited.calculations.len(),1);
 let mut legacy=ReturnInput::from(&edited);legacy.slip_groups=None;legacy.inputs["employment_income_cents"]=json!(999);legacy.expected_revision=Some(edited.revision);
 let preserved=tax.save(Some(&r.id),legacy).await.ok().unwrap();assert_eq!(preserved.slip_groups.len(),1);assert_eq!(preserved.inputs["employment_income_cents"],42);
 let mut clear=ReturnInput::from(&preserved);clear.expected_revision=Some(preserved.revision);clear.slip_groups=Some(vec![]);let deleted=tax.save(Some(&r.id),clear).await.ok().unwrap();assert!(deleted.slip_groups.is_empty());assert_eq!(deleted.inputs["employment_income_cents"],0);assert_eq!(deleted.inputs["donations_cents"],123);assert_eq!(deleted.inputs["credits_verified"],false);
 let pack=tax.package(&r.id).await.ok().unwrap();assert!(pack["return"].get("slip_groups").is_none());assert_eq!(pack["calculation_history"][0]["input_document"]["slip_groups"],groups);
}
#[tokio::test]
async fn conflict_bounds_availability_and_unsupported_boxes_fail_closed(){
 let tax=Tax::new(Store::memory());let mut input=doc(Some(json!([group("T5",json!({"25":101}))])));input.inputs["donations_cents"]=json!(0);let r=tax.save(None,input).await.ok().unwrap();let c=tax.calculate(&r.id,rev(&r)).await.unwrap_or_else(|e|panic!("calculation rejected: {}",e.1));let result=&c.calculation.as_ref().unwrap().result;assert_eq!(result["supported"],false);assert!(result["diagnostics"].as_array().unwrap().iter().any(|d|d["code"]=="slip_box_not_calculated"&&d["blocks_review"]==true));
 let mut stale=ReturnInput::from(&r);stale.expected_revision=Some(r.revision);assert!(tax.save(Some(&r.id),stale).await.is_err());
 let rolled=tax.rollforward(&c.id,RollforwardInput{expected_revision:c.revision,tax_year:2026,name:None}).await.ok().unwrap();assert!(rolled.slip_groups.is_empty());
 for (year,j)in [(2026,"AB"),(2025,"QC")]{let mut d=doc(Some(json!([group("T4",json!({"14":1}))])));d.tax_year=year;d.jurisdiction=j.into();assert!(tax.save(None,d).await.is_err());}
 let mut moved=ReturnInput::from(&c);moved.expected_revision=Some(c.revision);moved.jurisdiction="QC".into();moved.slip_groups=None;assert!(tax.save(Some(&c.id),moved).await.is_err());
 for groups in [json!([group("T4",json!({"99":1}))]),json!([group("T4",json!({"14":-1}))]),Value::Array(vec![group("T4",json!({}));201]),json!([group("T4",json!({"14":100000000000000_i64})),group("T4",json!({"14":100000000000000_i64}))])]{assert!(tax.save(None,doc(Some(groups))).await.is_err());}
 assert!(serde_json::from_value::<ReturnInput>(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"TEST malformed","slip_groups":[{"kind":"T4","boxes":{"14":1.5}}]})).is_err());
}
#[test]
fn empty_fields_preserve_legacy_input_bytes_and_exact_groups(){
 let input=doc(None);let before=serde_json::to_vec(&input).unwrap();assert_eq!(String::from_utf8(before.clone()).unwrap(),r#"{"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"TEST durable groups","client_ref":"","period_start":null,"period_end":null,"inputs":{"credits_verified":false,"donations_cents":123,"full_year_resident":true,"payroll_verified":false,"scope_confirmed":true},"parties":[],"corporate":null,"notes":"","expected_revision":null}"#);assert!(!String::from_utf8(before.clone()).unwrap().contains("slip_groups"));
 let mut v:Value=serde_json::from_slice(&before).unwrap();v["slip_groups"]=Value::Null;let again:ReturnInput=serde_json::from_value(v).unwrap();assert_eq!(before,serde_json::to_vec(&again).unwrap());
 let g=vec![serde_json::from_value(group("T4",json!({"14":1}))).unwrap();200];let total=slips::aggregate(&g).unwrap();assert_eq!(total["inputs"]["employment_income_cents"],200);
}

#[tokio::test]
async fn actual_routes_auth_revision_and_reload(){
 use axum::{body::Body,http::Request};use tower::ServiceExt;
 unsafe{std::env::set_var("ADMIN_API_KEY","synthetic-slip-groups-key");}
 let store=Store::memory();let app=nxt_os::app_with(||store.clone());
 async fn call(app:&axum::Router,method:&str,path:&str,body:Value,auth:bool)->(u16,Value){let mut req=Request::builder().method(method).uri(path).header("content-type","application/json");if auth{req=req.header("authorization","Bearer synthetic-slip-groups-key");}let resp=app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();let status=resp.status().as_u16();let b=axum::body::to_bytes(resp.into_body(),2*1024*1024).await.unwrap();(status,serde_json::from_slice(&b).unwrap_or(Value::Null))}
 let d=serde_json::to_value(doc(Some(json!([group("T4",json!({"14":123}))])))).unwrap();assert_eq!(call(&app,"POST","/api/tax/returns",d.clone(),false).await.0,401);
 let(status,r)=call(&app,"POST","/api/tax/returns",d,true).await;assert_eq!(status,200);let path=format!("/api/tax/returns/{}",r["id"].as_str().unwrap());let(status,got)=call(&app,"GET",&path,Value::Null,true).await;assert_eq!(status,200);assert_eq!(got["slip_groups"][0]["boxes"]["14"],123);
 let mut edit=serde_json::to_value(ReturnInput::from(&serde_json::from_value::<TaxReturn>(r).unwrap())).unwrap();edit["expected_revision"]=json!(0);assert_eq!(call(&app,"PUT",&path,edit.clone(),true).await.0,409);edit["expected_revision"]=got["revision"].clone();edit["slip_groups"]=json!([]);let(status,cleared)=call(&app,"PUT",&path,edit,true).await;assert_eq!(status,200);assert!(cleared.get("slip_groups").is_none());assert_eq!(cleared["inputs"]["employment_income_cents"],0);
}

#[tokio::test]
async fn calculation_rejects_any_bypassed_derived_input_contradiction(){
 let store=Store::memory();let tax=Tax::new(store.clone());let r=tax.save(None,doc(Some(json!([group("T4",json!({"14":1}))])))).await.ok().unwrap();
 let mut state:TaxState=store.get_json("tax:state:v1").await.unwrap().unwrap();state.returns[0].inputs["employment_income_cents"]=json!(999);store.set_json("tax:state:v1",&state).await.unwrap();assert!(tax.calculate(&r.id,rev(&r)).await.is_err());
}
