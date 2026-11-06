use axum::{body::Body,http::Request,Router};
use nxt_os::{app_with,source_import::{SourceImport,native_slip_supplements::{SupplementBatch,NativeSupplement}},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/supplement-test/test-copy";
const CORE:&str="4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc";
const EXTRA:&str="17b49048f15e6bac3b0feee2aad55b4b9879716eadb34805c4ad00e8ec06667d";
fn app()->Router {unsafe{std::env::set_var("ADMIN_API_KEY","native-extra-local-test");}app_with(Store::memory)}
async fn req(app:&Router,method:&str,path:&str,body:Value)->(u16,Value){
    let r=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("authorization","Bearer native-extra-local-test").header("content-type","application/json").body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();let r=app.clone().oneshot(r).await.unwrap();let status=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),usize::MAX).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
fn core()->Value {json!({"financial_profile_sha256":CORE,"source_archive_sha256":"a".repeat(64),"source_view_file_sha256":"b".repeat(64),"source_view_line_sha256":"c".repeat(64),"source_view_number":1,"client_number":"100","family_file_id":"999","source_return_year":2015,"vendor_module":"T1","vendor_slip_kind":"T4","vendor_slip_ordinal":0,"vendor_form":"T4","from_data_segment":true,"original_field_count":5,"financial_fields":[{"source_field_index":1,"field_id":13908,"source_type_tag":4,"value":{"kind":"text","text":"123.45"}}]})}
fn supplement()->Value {let mut c=core();let o=c.as_object_mut().unwrap();o.remove("financial_profile_sha256");o.remove("source_archive_sha256");o.insert("supplemental_profile_sha256".into(),json!(EXTRA));o.insert("financial_fields".into(),json!([{"source_field_index":2,"field_id":12119,"source_type_tag":4,"value":{"kind":"text","text":"0"}},{"source_field_index":4,"field_id":12119,"source_type_tag":4,"value":{"kind":"text","text":"4.25"}}]));c}
async fn prepare(a:&Router){let context=json!({"client_number":"100","family_file_id":"999","tax_year":2015,"archive_sha256":"a".repeat(64),"vendor_module":"T1","mapping_state":"vendor_static_verified","values_cents":{"15000":null,"23600":null,"26000":null,"42000":null,"48400":null,"48500":null}});assert_eq!(req(a,"POST","historical-lines",json!({"test_copy":true,"records":[context]})).await.0,200);assert_eq!(req(a,"POST","native-slip-views",json!({"test_copy":true,"records":[core()]})).await.0,200);}
#[tokio::test]
async fn preserves_core_and_repeated_additional_occurrences_with_year_review(){
 let a=app();prepare(&a).await;let before=req(&a,"GET","native-slip-views/summary",Value::Null).await.1;
 for replay in [false,true] {let (s,r)=req(&a,"POST","native-slip-supplements",json!({"test_copy":true,"records":[supplement()]})).await;assert_eq!(s,200);assert_eq!(r[if replay{"unchanged_replays"}else{"created"}],1);}
 let (s,p)=req(&a,"GET","clients/100/source-facts?kind=native-slip-views&source_return_year=2015",Value::Null).await;assert_eq!(s,200);let r=&p["records"][0];for(k,v)in core().as_object().unwrap(){assert_eq!(&r[k],v)}assert_eq!(r["additional_financial_fields"],supplement()["financial_fields"]);assert_eq!(r["additional_field_review"]["box_assignment_approved"],false);assert_eq!(r["additional_field_review"]["field_bindings"]["12119"]["possible_labels"][0],"Motor vehicle benefits");assert_eq!(req(&a,"GET","native-slip-views/summary",Value::Null).await.1,before);assert_eq!(req(&a,"GET","native-slip-supplements/summary",Value::Null).await.1["additional_financial_fields"],2);
 assert_eq!(req(&a,"GET","clients/100/source-facts?kind=native-slip-views&source_return_year=2016",Value::Null).await.1["total"],0);
}
#[tokio::test]
async fn rejects_wrong_context_overlap_inactive_and_year_profile_before_writes(){
 let a=app();prepare(&a).await;let original=supplement();let mut bads=Vec::new();
 for (key,value) in [("source_return_year",json!(2014)),("vendor_module",json!("T3")),("client_number",json!("101")),("source_view_line_sha256",json!("d".repeat(64))),("supplemental_profile_sha256",json!("0".repeat(64)))] {let mut b=original.clone();b[key]=value;bads.push(b)}
 let mut b=original.clone();b["financial_fields"][0]["source_field_index"]=json!(1);bads.push(b);
 let mut b=original.clone();b["financial_fields"][0]["field_id"]=json!(13908);bads.push(b);
 let mut b=original.clone();b["financial_fields"][0]["value"]=json!({"kind":"float32","bits_hex":"00000000"});bads.push(b);
 let mut b=original.clone();b["financial_fields"][0]["value"]["text"]=Value::Null;bads.push(b);
 let mut b=original.clone();b["financial_fields"].as_array_mut().unwrap().reverse();bads.push(b);
 for b in bads {assert_ne!(req(&a,"POST","native-slip-supplements",json!({"test_copy":true,"records":[original.clone(),b]})).await.0,200);assert_eq!(req(&a,"GET","native-slip-supplements/summary",Value::Null).await.1["additional_source_views"],0);}
 assert_eq!(req(&a,"POST","native-slip-supplements",json!({"test_copy":false,"records":[original]})).await.0,409);
}
#[tokio::test]
async fn immutable_replay_repairs_body_only_indexes_and_refuses_conflicting_amount(){
 let store=Store::memory();let service=SourceImport::new(store.clone());let company="extra-repair";
 let original=core();let core_key=format!("dtmaxtc:{company}:native_slip:record:{}:000001:{CORE}","b".repeat(64));store.set(&core_key,&original.to_string()).await.unwrap();
 let s:NativeSupplement=serde_json::from_value(supplement()).unwrap();let raw=serde_json::to_string(&s).unwrap();let key=format!("dtmaxtc:{company}:native_supplement:{EXTRA}:record:{}:000001","b".repeat(64));store.set(&key,&raw).await.unwrap();
 let r=service.test_copy_native_slip_supplements(company,SupplementBatch{test_copy:true,records:vec![s.clone()]}).await.unwrap();assert_eq!(r["unchanged_replays"],1);assert_eq!(service.test_copy_native_supplement_summary(company).await.unwrap()["additional_financial_fields"],2);
 let mut changed=s;changed.financial_fields.pop();assert!(service.test_copy_native_slip_supplements(company,SupplementBatch{test_copy:true,records:vec![changed]}).await.is_err());assert_eq!(store.get(&key).await.unwrap().unwrap(),raw);assert_eq!(store.get(&core_key).await.unwrap().unwrap(),original.to_string());
}
#[tokio::test]
async fn routes_require_admin_and_profile_is_byte_pinned(){
 let a=app();for(method,path)in[("POST","native-slip-supplements"),("GET","native-slip-supplements/summary")]{let r=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json").body(Body::from("{}" )).unwrap();assert_eq!(a.clone().oneshot(r).await.unwrap().status(),401)}
 let p:Value=serde_json::from_str(include_str!("../src/source_import/native_slip_supplement_profiles.json")).unwrap();assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p).unwrap())),EXTRA);
}
#[tokio::test]
async fn optional_all_692_real_supplements_preserve_every_core_body(){
 let Ok(path)=std::env::var("NXT_DTMAX_NATIVE_SUPPLEMENTS")else{return};let raw=std::fs::read(&path).unwrap();assert_eq!(hex::encode(Sha256::digest(&raw)),"b9c8a265b37a8dd291800be49805dc713954bb710286767871eaaf7b646604cd");
 let rows:Vec<NativeSupplement>=String::from_utf8(raw).unwrap().lines().map(|s|serde_json::from_str(s).unwrap()).collect();assert_eq!(rows.len(),692);
 let core_stage=std::path::PathBuf::from(std::env::var("NXT_DTMAX_NATIVE_VIEW_STAGING").unwrap());let manifest:Value=serde_json::from_slice(&std::fs::read(core_stage.join("staging-summary.json")).unwrap()).unwrap();let wanted:std::collections::BTreeSet<_>=rows.iter().map(|s|s.source_view_number).collect();let store=Store::memory();let service=SourceImport::new(store.clone());let company="private-extra";let mut originals=Vec::new();
 for item in manifest["batches"].as_array().unwrap(){let bytes=std::fs::read(core_stage.join(item["file"].as_str().unwrap())).unwrap();assert_eq!(hex::encode(Sha256::digest(&bytes)),item["sha256"]);let batch:Value=serde_json::from_slice(&bytes).unwrap();for r in batch["records"].as_array().unwrap(){if wanted.contains(&(r["source_view_number"].as_u64().unwrap() as u32)){let key=format!("dtmaxtc:{company}:native_slip:record:{}:{:06}:{CORE}",r["source_view_file_sha256"].as_str().unwrap(),r["source_view_number"].as_u64().unwrap());let body=serde_json::to_string(r).unwrap();store.set(&key,&body).await.unwrap();originals.push((key,body));}}}
 assert_eq!(originals.len(),692);let mut fields=0;for batch in rows.chunks(250){fields+=batch.iter().map(|s|s.financial_fields.len()).sum::<usize>();let r=service.test_copy_native_slip_supplements(company,SupplementBatch{test_copy:true,records:batch.to_vec()}).await.unwrap();assert_eq!(r["created"],batch.len());let replay=service.test_copy_native_slip_supplements(company,SupplementBatch{test_copy:true,records:batch.to_vec()}).await.unwrap();assert_eq!(replay["unchanged_replays"],batch.len());}
 assert_eq!(fields,1118);assert_eq!(service.test_copy_native_supplement_summary(company).await.unwrap()["affected_clients"],464);for(key,body)in originals{assert_eq!(store.get(&key).await.unwrap().unwrap(),body)}println!("REAL_ADDITIONAL_SOURCE_VIEWS=692;FIELDS=1118;CLIENTS=464;ORIGINAL_CORE_BODIES_UNCHANGED=true");
}
