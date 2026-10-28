//! Original synthetic contacts only; no source archive, user, Store RPC or socket.
pub use nxt_os::{migrations,referral,store};
#[path="../src/company_contact_lists/mod.rs"]mod contacts;
use contacts::{ContactLists,models::*};
use migrations::{Migrations,models::{ChunkInput,SnapshotManifest}};
use store::Store;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,sync::{Arc,atomic::Ordering}};
use axum::{body::Body,http::Request};
use tower::ServiceExt;
fn enc<T:serde::Serialize>(v:&T)->String{serde_json::to_string(&serde_json::to_value(v).unwrap()).unwrap()}
fn sha(v:&[u8])->String{hex::encode(Sha256::digest(v))}
fn cell(v:Option<&str>)->Cell{Cell{present:v.is_some(),cell_type:v.map(|_|"s".into()),value:v.map(Into::into),formula:None}}
fn record(source:&Source,kind:Kind,row:u32)->Record{
 let mut raw:BTreeMap<String,Cell>=kind.fields().iter().map(|(col,_)|(col.to_string(),cell(None))).collect();
 raw.insert("B".into(),cell(Some("Synthetic café <script>alert(1)</script>")));raw.insert("C".into(),cell(Some("")));
 raw.insert("G".into(),Cell{formula:Some("HYPERLINK(\"javascript:synthetic\")".into()),..cell(Some("Original cached source text"))});
 let fields=kind.fields().iter().map(|(col,key)|(key.to_string(),raw[*col].clone())).collect();
 Record{cohort_ref:"synthetic-A".into(),record_type:kind.clone(),source_record_alias:format!("{}_{}",kind.label(),sha(enc(&json!(["synthetic-A",source.archive_sha256,source.report_sha256,source.worksheet_file,row])).as_bytes())),source_id:None,source_id_coverage:"archive_row".into(),source_identity_verified:false,operational_object_created:false,linked_to_journal_contacts:false,formula_fields_present:true,source_archive_sha256:source.archive_sha256.clone(),source_report_sha256:source.report_sha256.clone(),source_member:source.member.clone(),source_worksheet_name:source.worksheet_name.clone(),source_worksheet_file:source.worksheet_file.clone(),source_row:row,source_fields:fields,raw_source_cells:raw}
}
async fn fixture()->(Arc<ContactLists>,Source,String){
 let m=Arc::new(Migrations::new(Store::memory()));let bytes=b"PK\x03\x04synthetic-only";let archive=sha(bytes);let content=hex::encode(bytes);let reference=sha(content.as_bytes());
 m.upload_chunk(ChunkInput{sha256:reference.clone(),content}).await.ok().unwrap();
 let text=json!({"format":"nxt-shadow-report-rows/v1","cohort_ref":"synthetic-A","source_archive_sha256":archive,"source_report_sha256":"a".repeat(64),"report_id":"journal","rows":[[5,[["A","s","Synthetic source",null,null]]]]}).to_string();let rowhash=sha(text.as_bytes());
 m.upload_chunk(ChunkInput{sha256:rowhash.clone(),content:text}).await.ok().unwrap();
 let manifest:SnapshotManifest=serde_json::from_value(json!({"format":"nxt-shadow-snapshot/v1","id":"snapshot-A","cohort_ref":"synthetic-A","company_name":"Synthetic A","source_system":"quickbooks","captured_at":"2026-10-27T00:00:00Z","cutoff_date":"2026-10-27","reviewer_label":"Synthetic operator","source_of_truth":"original_system","mode":"parallel_test","source_archive":{"file_name":"synthetic.zip","sha256":archive,"bytes":bytes.len(),"encoding":"hex","chunks":[reference]},"reports":[{"id":"journal","title":"Synthetic journal","sheet_name":"Sheet1","source_sha256":"a".repeat(64),"source_bytes":10,"columns":["A"],"row_count":1,"parts":[{"sha256":rowhash,"row_start":5,"row_end":5,"row_count":1}]}],"exceptions":[],"limitations":["Synthetic; operator carries separate original-XLS comparison proof."]})).unwrap();
 let published=m.publish(manifest).await.ok().unwrap();let manifest_sha=published["manifest_sha256"].as_str().unwrap().into();
 let source=Source{archive_sha256:archive,archive_bytes:bytes.len(),report_sha256:"b".repeat(64),member:"Customers.xlsx".into(),worksheet_name:"Sheet1".into(),worksheet_file:"xl/worksheets/sheet1.xml".into(),company_heading_sha256:"c".repeat(64),captured_at:"2026-10-27T00:00:00Z".into(),capture_basis:"Synthetic source capture".into()};
 (Arc::new(ContactLists::new(Store::memory(),m)),source,manifest_sha)
}
fn part(source:&Source,kind:Kind,rows:&[u32],other:bool)->Part{
 Part{format:"nxt-company-contact-part/v1".into(),company_id:"synthetic-A".into(),kind:kind.clone(),source:source.clone(),records:rows.iter().map(|n|record(source,kind.clone(),*n)).collect(),other_source_rows:if other{vec![OtherRow{source_row:99,classification:"retained_nonrecord_or_unclassified_row".into(),raw_cells:std::iter::once(("A".into(),cell(Some("Synthetic footer")))).chain(["B","C","D","E","F","G"].into_iter().map(|k|(k.into(),cell(None)))).collect()}]}else{vec![]}}
}
async fn stage(s:&ContactLists,p:Part)->PartRef{let v=s.stage("synthetic-A",p).await.ok().unwrap();serde_json::from_value(v.as_object().unwrap().iter().filter(|(k,_)|["sha256","bytes","records","other_rows"].contains(&k.as_str())).map(|(k,v)|(k.clone(),v.clone())).collect::<serde_json::Map<_,_>>().into()).unwrap()}
fn command(source:&Source,manifest:&str,kind:Kind,parts:Vec<PartRef>,request:&str,expected:Option<String>)->Publish{
 let family=Family{format:"nxt-company-contact-family/v2".into(),company_id:"synthetic-A".into(),kind:kind.clone(),source:source.clone(),headers:kind.headers(),records_total:parts.iter().map(|r|r.records).sum(),nonrecord_rows_total:parts.iter().map(|r|r.other_rows).sum(),parts,source_snapshot_id:"snapshot-A".into(),source_manifest_sha256:manifest.into()};
 let proof=OperatorProof{format:"nxt-source-contact-operator-equality/v1".into(),company_id:"synthetic-A".into(),source_archive_sha256:source.archive_sha256.clone(),source_report_sha256:source.report_sha256.clone(),family_sha256:sha(enc(&family).as_bytes()),comparison_manifest_sha256:"d".repeat(64),source_field_equality_verified:true};
 Publish{request_id:request.into(),expected_index_head:expected,family,source_operator_proof:proof}
}
fn capture(v:&Value)->&str{v["capture_sha256"].as_str().unwrap()}
#[tokio::test]
async fn complete_multipart_records_preserve_original_cells_and_snapshot_without_posting(){
 let(s,source,m)=fixture().await;s.store.set("gl:company","synthetic sentinel").await.ok().unwrap();s.store.set("company-books:synthetic-A:head","journal sentinel").await.ok().unwrap();
 let p1=part(&source,Kind::Customer,&[6,7],false);let p2=part(&source,Kind::Customer,&[8],true);
 let refs=vec![stage(&s,p1.clone()).await,stage(&s,p2.clone()).await];let input=command(&source,&m,Kind::Customer,refs,"pub-A",None);
 assert_eq!(s.rows("synthetic-A",Kind::Customer,&input.family.parts[0].sha256,0,100,786432,false).await.unwrap_err().0,404);
 let publication=s.publish("synthetic-A",input.clone()).await.ok().unwrap();let h=capture(&publication);
 let mut all=vec![];for offset in 0..3{let page=s.rows("synthetic-A",Kind::Customer,h,offset,1,786432,false).await.ok().unwrap();all.push(page["records"][0].clone());assert_eq!(page["total"],3);}
 assert_eq!(all,vec![serde_json::to_value(&p1.records[0]).unwrap(),serde_json::to_value(&p1.records[1]).unwrap(),serde_json::to_value(&p2.records[0]).unwrap()]);
 assert_eq!(all[0]["source_fields"]["email"]["present"],false);assert_eq!(all[0]["source_fields"]["phone_numbers"]["value"],"");
 let other=s.rows("synthetic-A",Kind::Customer,h,0,100,786432,true).await.ok().unwrap();assert_eq!(other["records"],serde_json::to_value(p2.other_source_rows).unwrap());
 assert_eq!(s.publish("synthetic-A",input).await.ok().unwrap()["idempotent_replay"],true);
 assert_eq!(s.store.get("gl:company").await.ok().unwrap().unwrap(),"synthetic sentinel");assert_eq!(s.store.get("company-books:synthetic-A:head").await.ok().unwrap().unwrap(),"journal sentinel");
 assert_eq!(s.detail("synthetic-A",h).await.ok().unwrap()["receipt"]["command"]["family"]["source_manifest_sha256"],m);
}
#[tokio::test]
async fn company_kind_archive_and_equality_proof_are_bound(){
 let(s,source,m)=fixture().await;let p=part(&source,Kind::Customer,&[6],false);
 assert_eq!(s.stage("foreign-company",p.clone()).await.unwrap_err().0,400);
 let r=stage(&s,p).await;let input=command(&source,&m,Kind::Customer,vec![r],"req-A",None);
 let mut wrong=input.clone();wrong.family.source_manifest_sha256="f".repeat(64);wrong.source_operator_proof.family_sha256=sha(enc(&wrong.family).as_bytes());assert_eq!(s.publish("synthetic-A",wrong).await.unwrap_err().0,409);
 let mut wrong=input.clone();wrong.source_operator_proof.source_report_sha256="e".repeat(64);assert_eq!(s.publish("synthetic-A",wrong).await.unwrap_err().0,400);
 let v=s.publish("synthetic-A",input).await.ok().unwrap();assert_eq!(s.detail("foreign-company",capture(&v)).await.unwrap_err().0,404);assert_eq!(s.rows("synthetic-A",Kind::Supplier,capture(&v),0,100,786432,false).await.unwrap_err().0,404);
}
#[tokio::test]
async fn duplicate_reordered_or_missing_parts_never_publish_partial_family(){
 let(s,source,m)=fixture().await;let r=stage(&s,part(&source,Kind::Customer,&[6],false)).await;
 let duplicate=command(&source,&m,Kind::Customer,vec![r.clone(),r.clone()],"duplicate",None);assert_eq!(s.publish("synthetic-A",duplicate).await.unwrap_err().0,400);
 let missing=PartRef{sha256:"f".repeat(64),..r.clone()};assert_eq!(s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![missing],"missing",None)).await.unwrap_err().0,500);
 let later=stage(&s,part(&source,Kind::Customer,&[7],false)).await;
 assert_eq!(s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![later,r],"reorder",None)).await.unwrap_err().0,400);
 assert!(s.list("synthetic-A").await.ok().unwrap()["captures"].as_array().unwrap().is_empty());
}
#[tokio::test]
async fn stage_reservation_recovers_without_duplicate_charge_or_visibility(){
 let(s,source,m)=fixture().await;let p=part(&source,Kind::Customer,&[6],false);s.fail_stage_once.store(true,Ordering::SeqCst);
 assert_eq!(s.stage("synthetic-A",p.clone()).await.unwrap_err().0,500);let before=s.list("synthetic-A").await.ok().unwrap();
 let r=stage(&s,p).await;assert_eq!(s.list("synthetic-A").await.ok().unwrap()["charged_bytes"],before["charged_bytes"]);
 assert!(before["captures"].as_array().unwrap().is_empty());s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![r],"recover",None)).await.ok().unwrap();
}
#[tokio::test]
async fn publication_receipt_crash_retry_is_exact_and_body_changes_conflict(){
 let(s,source,m)=fixture().await;let r=stage(&s,part(&source,Kind::Customer,&[6],false)).await;let input=command(&source,&m,Kind::Customer,vec![r],"recover",None);
 s.fail_publish_once.store(true,Ordering::SeqCst);assert_eq!(s.publish("synthetic-A",input.clone()).await.unwrap_err().0,500);
 assert!(s.list("synthetic-A").await.ok().unwrap()["captures"].as_array().unwrap().is_empty());
 let before=s.list("synthetic-A").await.ok().unwrap()["charged_bytes"].clone();let v=s.publish("synthetic-A",input.clone()).await.ok().unwrap();assert_eq!(s.list("synthetic-A").await.ok().unwrap()["charged_bytes"],before);
 let mut changed=input.clone();changed.source_operator_proof.comparison_manifest_sha256="e".repeat(64);assert_eq!(s.publish("synthetic-A",changed).await.unwrap_err().0,409);
 assert_eq!(s.publish("synthetic-A",input).await.ok().unwrap()["capture_sha256"],v["capture_sha256"]);
}
#[tokio::test]
async fn competing_complete_publications_have_one_visible_winner(){
 let(s,source,m)=fixture().await;let r=stage(&s,part(&source,Kind::Customer,&[6],false)).await;
 let a=command(&source,&m,Kind::Customer,vec![r.clone()],"racer-A",None);let b=command(&source,&m,Kind::Customer,vec![r],"racer-B",None);
 let(a,b)=tokio::join!(s.publish("synthetic-A",a),s.publish("synthetic-A",b));assert_ne!(a.is_ok(),b.is_ok());
 let v=s.list("synthetic-A").await.ok().unwrap();assert_eq!(v["captures"].as_array().unwrap().len(),1);assert_eq!(v["pending_publications"],1);
}
#[tokio::test]
async fn staged_and_published_corruption_fail_closed_without_partial_rows(){
 let(s,source,m)=fixture().await;let r=stage(&s,part(&source,Kind::Customer,&[6],false)).await;
 let v=s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![r.clone()],"good",None)).await.ok().unwrap();
 s.store.set(&format!("company-contact-lists-v2:synthetic-A:part:{}",r.sha256),"{}").await.ok().unwrap();
 assert_eq!(s.rows("synthetic-A",Kind::Customer,capture(&v),0,100,786432,false).await.unwrap_err().0,500);
 assert_eq!(s.list("synthetic-A").await.ok().unwrap()["integrity_scope"],"bounded_index_metadata_only");
 s.store.set("company-contact-lists-v2:synthetic-A:state:v1","{}").await.ok().unwrap();assert_eq!(s.list("synthetic-A").await.unwrap_err().0,500);
}
#[tokio::test]
async fn pending_capacity_denies_new_allocations_but_existing_exact_request_recovers(){
 let(s,source,m)=fixture().await;let r=stage(&s,part(&source,Kind::Customer,&[6],false)).await;
 for n in 0..8{s.fail_publish_once.store(true,Ordering::SeqCst);assert_eq!(s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![r.clone()],&format!("pending-{n}"),None)).await.unwrap_err().0,500);}
 assert_eq!(s.list("synthetic-A").await.ok().unwrap()["pending_publications"],8);
 let before=s.store.get("company-contact-lists-v2:synthetic-A:state:v1").await.ok().unwrap();
 assert_eq!(s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![r.clone()],"ninth",None)).await.unwrap_err().0,409);
 assert_eq!(s.store.get("company-contact-lists-v2:synthetic-A:state:v1").await.ok().unwrap(),before);
 s.publish("synthetic-A",command(&source,&m,Kind::Customer,vec![r],"pending-0",None)).await.ok().unwrap();
}
#[tokio::test]
async fn reserved_part_bytes_are_bounded_and_no_partial_new_object_is_written(){
 let(s,source,_)=fixture().await;let reservations=(0..64).map(|n|json!({"sha256":format!("{n:064x}"),"bytes":524288})).collect::<Vec<_>>();
 let state=json!({"parts":reservations,"slots":[],"published":[],"charged_bytes":33554432});s.store.set("company-contact-lists-v2:synthetic-A:state:v1",&enc(&state)).await.ok().unwrap();
 let p=part(&source,Kind::Customer,&[6],false);let h=sha(enc(&p).as_bytes());assert_eq!(s.stage("synthetic-A",p).await.unwrap_err().0,409);
 assert!(s.store.get(&format!("company-contact-lists-v2:synthetic-A:part:{h}")).await.ok().unwrap().is_none());
}
#[tokio::test]
async fn zero_export_is_explicit_and_supplier_account_reference_is_not_coa_identity(){
 let(s,mut source,m)=fixture().await;source.member="Suppliers.xlsx".into();let mut p=part(&source,Kind::Supplier,&[6],false);
 p.records[0].raw_source_cells.insert("G".into(),cell(Some("0000042")));p.records[0].source_fields.insert("supplier_account_number".into(),cell(Some("0000042")));p.records[0].formula_fields_present=false;
 let r=stage(&s,p).await;let v=s.publish("synthetic-A",command(&source,&m,Kind::Supplier,vec![r],"supplier",None)).await.ok().unwrap();
 assert_eq!(s.rows("synthetic-A",Kind::Supplier,capture(&v),0,100,786432,false).await.ok().unwrap()["records"][0]["source_fields"]["supplier_account_number"]["value"],"0000042");
 let expected=s.list("synthetic-A").await.ok().unwrap()["index_head_sha256"].as_str().unwrap().to_string();
 let f=command(&source,&m,Kind::Supplier,vec![],"empty-export",Some(expected));let v=s.publish("synthetic-A",f).await.ok().unwrap();let page=s.rows("synthetic-A",Kind::Supplier,capture(&v),0,100,786432,false).await.ok().unwrap();
 assert_eq!(page["total"],0);assert_eq!(page["source_roster_complete"],false);assert_eq!(page["controls"]["operational_objects_created"],false);
}
#[tokio::test]
async fn pages_are_bounded_and_at_most_four_parts_are_read_per_call(){
 let(s,source,m)=fixture().await;let mut refs=vec![];for n in 6..12{refs.push(stage(&s,part(&source,Kind::Customer,&[n],false)).await)}
 let v=s.publish("synthetic-A",command(&source,&m,Kind::Customer,refs,"paged",None)).await.ok().unwrap();let first=s.rows("synthetic-A",Kind::Customer,capture(&v),0,100,786432,false).await.ok().unwrap();
 assert_eq!(first["records"].as_array().unwrap().len(),4);assert_eq!(first["parts_checked"],4);assert_eq!(first["next_offset"],4);
 assert_eq!(s.rows("synthetic-A",Kind::Customer,capture(&v),4,100,786432,false).await.ok().unwrap()["records"].as_array().unwrap().len(),2);
 assert_eq!(s.rows("synthetic-A",Kind::Customer,capture(&v),0,0,786432,false).await.unwrap_err().0,400);
}
#[test]
fn schemas_are_strict_and_kind_accepts_only_strings(){
 for v in [json!(null),json!({"customer":null}),json!(["customer"]),json!(true),json!(1),json!("employee"),json!("vendor")]{assert!(serde_json::from_value::<Kind>(v).is_err())}
 assert_eq!(serde_json::from_value::<Kind>(json!("supplier")).unwrap(),Kind::Supplier);
 let mut value=serde_json::to_value(Source{archive_sha256:"a".repeat(64),archive_bytes:1,report_sha256:"b".repeat(64),member:"Customers.xlsx".into(),worksheet_name:"S".into(),worksheet_file:"xl/worksheets/s.xml".into(),company_heading_sha256:"c".repeat(64),captured_at:"2026-10-27T00:00:00Z".into(),capture_basis:"synthetic".into()}).unwrap();value["unknown"]=json!(true);assert!(serde_json::from_value::<Source>(value).is_err());
}
#[tokio::test]
async fn raw_presence_types_formula_and_nonoperational_flags_are_enforced_before_store(){
 let(s,source,_)=fixture().await;
 for n in 0..5 {let mut p=part(&source,Kind::Customer,&[6],false);match n{0=>p.records[0].source_id=Some("guessed".into()),1=>p.records[0].source_identity_verified=true,2=>p.records[0].source_fields.get_mut("email").unwrap().value=Some("guessed".into()),3=>p.records[0].source_row=5,_=>p.records[0].source_record_alias="wrong".into()};assert_eq!(s.stage("synthetic-A",p).await.unwrap_err().0,400)}
 assert!(s.store.get("company-contact-lists-v2:synthetic-A:state:v1").await.ok().unwrap().is_none());
}
#[tokio::test]
async fn http_auth_no_store_and_unconfigured_default_denial_precede_bad_body(){
 // Only this dedicated process uses synthetic credentials; no host/account action.
 unsafe{std::env::remove_var("ADMIN_API_KEY")};let(s,source,manifest)=fixture().await;
 let reference=stage(&s,part(&source,Kind::Customer,&[6],false)).await;
 let capture=s.publish("synthetic-A",command(&source,&manifest,Kind::Customer,vec![reference],"path-test",None)).await.ok().unwrap()["capture_sha256"].as_str().unwrap().to_string();
 let app=contacts::routes::router(s.clone());
 let req=Request::builder().uri("/api/company-books/synthetic-A/source-contact-lists").body(Body::empty()).unwrap();assert_eq!(app.clone().oneshot(req).await.ok().unwrap().status(),503);
 unsafe{std::env::set_var("ADMIN_API_KEY","synthetic-contact-key")};
 let req=Request::builder().method("POST").uri("/api/company-books/synthetic-A/source-contact-lists/parts").header("content-type","application/json").body(Body::from("not JSON")).unwrap();
 let denied=app.clone().oneshot(req).await.ok().unwrap();assert_eq!(denied.status(),401);assert_eq!(denied.headers()["cache-control"],"private, no-store");
 let req=Request::builder().uri("/api/company-books/synthetic-A/source-contact-lists").header("authorization","Bearer synthetic-contact-key").body(Body::empty()).unwrap();let allowed=app.clone().oneshot(req).await.ok().unwrap();assert_eq!(allowed.status(),200);assert_eq!(allowed.headers()["cache-control"],"private, no-store");
 let state_before=s.store.get("company-contact-lists-v2:synthetic-A:state:v1").await.ok().unwrap();
 for (prefix,expected) in [("api/company-books/synthetic-A",200),("/api/company-books/synthetic-A",200),("company-books/synthetic-A",200),("api/company-books/foreign",400)] {
  let transport=format!("{prefix}/source-contact-lists/customer/{capture}/records");
  let query=url::form_urlencoded::Serializer::new(String::new()).append_pair("offset","0").append_pair("limit","100").append_pair("max_bytes","786432").append_pair("path",&transport).finish();
  let req=Request::builder().uri(format!("/api/company-books/synthetic-A/source-contact-lists/customer/{capture}/records?{query}")).header("authorization","Bearer synthetic-contact-key").body(Body::empty()).unwrap();
  let result=app.clone().oneshot(req).await.ok().unwrap();assert_eq!(result.status(),expected);assert_eq!(result.headers()["cache-control"],"private, no-store");
 }
 assert_eq!(s.store.get("company-contact-lists-v2:synthetic-A:state:v1").await.ok().unwrap(),state_before);
 unsafe{std::env::remove_var("ADMIN_API_KEY")};
}
#[test]
fn nullable_original_source_fields_require_explicit_keys_without_normalization(){
 let null_cell=serde_json::to_value(cell(None)).unwrap();
 assert!(serde_json::from_value::<Cell>(null_cell.clone()).is_ok());
 for key in ["type","value","formula"] {let mut bad=null_cell.clone();bad.as_object_mut().unwrap().remove(key);assert!(serde_json::from_value::<Cell>(bad).is_err(),"Missing {key} was normalized");}
 let source=Source{archive_sha256:"a".repeat(64),archive_bytes:1,report_sha256:"b".repeat(64),member:"Customers.xlsx".into(),worksheet_name:"Sheet1".into(),worksheet_file:"xl/worksheets/sheet1.xml".into(),company_heading_sha256:"c".repeat(64),captured_at:"2026-10-27T00:00:00Z".into(),capture_basis:"synthetic".into()};
 let mut record_json=serde_json::to_value(record(&source,Kind::Customer,6)).unwrap();record_json.as_object_mut().unwrap().remove("source_id");assert!(serde_json::from_value::<Record>(record_json).is_err());
}
#[tokio::test]
async fn retained_original_source_rows_cannot_drop_header_columns(){
 let(s,source,_)=fixture().await;
 for mode in [0,1,2] {let mut p=part(&source,Kind::Customer,&[],true);match mode{0=>p.other_source_rows[0].raw_cells.clear(),1=>{p.other_source_rows[0].raw_cells.remove("D");},_=>p.other_source_rows[0].raw_cells.retain(|k,_|k=="A")};assert_eq!(s.stage("synthetic-A",p).await.unwrap_err().0,400);}
 assert!(s.store.get("company-contact-lists-v2:synthetic-A:state:v1").await.ok().unwrap().is_none());
}
#[tokio::test]
async fn supplier_family_larger_than_legacy_capture_preserves_every_row_with_byte_pagination(){
 let(s,mut source,m)=fixture().await;source.member="Suppliers.xlsx".into();
 let mut a=part(&source,Kind::Supplier,&(6..16).collect::<Vec<_>>(),false);
 let mut b=part(&source,Kind::Supplier,&(16..26).collect::<Vec<_>>(),true);
 let value="Synthetic source text ".repeat(119);
 for p in [&mut a,&mut b] {for r in &mut p.records {
  for (col,key) in Kind::Supplier.fields(){let v=cell(Some(&value));r.raw_source_cells.insert(col.into(),v.clone());r.source_fields.insert(key.into(),v);}
  r.formula_fields_present=false;
 }}
 assert!(enc(&a).len()<512*1024&&enc(&b).len()<512*1024);
 assert!(enc(&a).len()+enc(&b).len()>512*1024);
 let expected=a.records.iter().chain(b.records.iter()).map(|r|serde_json::to_value(r).unwrap()).collect::<Vec<_>>();
 let refs=vec![stage(&s,a).await,stage(&s,b).await];
 let v=s.publish("synthetic-A",command(&source,&m,Kind::Supplier,refs,"large-supplier",None)).await.ok().unwrap();
 let mut offset=0;let mut actual=vec![];
 loop {let page=s.rows("synthetic-A",Kind::Supplier,capture(&v),offset,200,49152,false).await.ok().unwrap();
  assert!(enc(&page).len()<=49152);assert!(page["parts_checked"].as_u64().unwrap()<=4);
  actual.extend(page["records"].as_array().unwrap().iter().cloned());
  if page["has_more"]==false{break}let next=page["next_offset"].as_u64().unwrap()as usize;assert!(next>offset);offset=next;
 }
 assert_eq!(actual,expected);assert_eq!(actual.len(),20);
}
