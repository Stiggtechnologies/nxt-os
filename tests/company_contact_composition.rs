//! Actual isolated app factory composition; no network/listener/source records.
use axum::{body::Body,http::Request};
use nxt_os::{app_with,store::Store};
use tower::ServiceExt;
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
static AUTH_ENV:std::sync::Mutex<()>=std::sync::Mutex::new(());
fn sha(v:&[u8])->String{hex::encode(Sha256::digest(v))}
fn enc<T:serde::Serialize>(v:&T)->Vec<u8>{serde_json::to_vec(&serde_json::to_value(v).unwrap()).unwrap()}
async fn call(app:&axum::Router,method:&str,path:&str,body:Option<Value>)->(u16,Value){
 let mut r=Request::builder().method(method).uri(path).header("authorization","Bearer synthetic-only-contact-key");
 let b=if let Some(value)=body{r=r.header("content-type","application/json");Body::from(serde_json::to_vec(&value).unwrap())}else{Body::empty()};
 let result=app.clone().oneshot(r.body(b).unwrap()).await.unwrap();let status=result.status().as_u16();if path.contains("/source-contact-lists"){assert_eq!(result.headers()["cache-control"],"private, no-store");}
 (status,serde_json::from_slice(&axum::body::to_bytes(result.into_body(),1024*1024).await.unwrap()).unwrap())
}
#[tokio::test]
async fn contact_routes_are_registered_in_actual_factory_with_default_denial(){
 let _guard=AUTH_ENV.lock().unwrap();
 unsafe{std::env::remove_var("ADMIN_API_KEY")};let app=app_with(Store::memory);
 let r=app.oneshot(Request::builder().uri("/api/company-books/synthetic-A/source-contact-lists").body(Body::empty()).unwrap()).await.unwrap();assert_eq!(r.status(),503);assert_eq!(r.headers()["cache-control"],"private, no-store");
}
#[tokio::test]
async fn contact_company_route_and_existing_employee_route_coexist_behind_admin(){
 let _guard=AUTH_ENV.lock().unwrap();
 unsafe{std::env::set_var("ADMIN_API_KEY","synthetic-only-contact-key")};let app=app_with(Store::memory);
 for path in ["/api/company-books/synthetic-A/source-contact-lists","/api/company-books/synthetic-A/source-lists"]{
  let r=app.clone().oneshot(Request::builder().uri(path).header("authorization","Bearer synthetic-only-contact-key").body(Body::empty()).unwrap()).await.unwrap();assert_eq!(r.status(),200);assert_eq!(r.headers()["cache-control"],"private, no-store");
 }
 unsafe{std::env::remove_var("ADMIN_API_KEY")};
}
#[tokio::test]
async fn real_factory_http_source_publication_and_readback_do_not_create_ledger_objects(){
 use nxt_os::company_contact_lists::models::*;
 let _guard=AUTH_ENV.lock().unwrap();unsafe{std::env::set_var("ADMIN_API_KEY","synthetic-only-contact-key")};let app=app_with(Store::memory);
 let(_,before)=call(&app,"GET","/api/ledger/contacts?kind=customer",None).await;
 let bytes=b"PK\x03\x04synthetic-http";let archive=sha(bytes);let content=hex::encode(bytes);let archive_ref=sha(content.as_bytes());
 assert_eq!(call(&app,"POST","/api/migrations/chunks",Some(json!({"sha256":archive_ref,"content":content}))).await.0,200);
 let rows=json!({"format":"nxt-shadow-report-rows/v1","cohort_ref":"synthetic-A","source_archive_sha256":archive,"source_report_sha256":"a".repeat(64),"report_id":"journal","rows":[[5,[["A","s","Synthetic",null,null]]]]}).to_string();let rowsha=sha(rows.as_bytes());
 assert_eq!(call(&app,"POST","/api/migrations/chunks",Some(json!({"sha256":rowsha,"content":rows}))).await.0,200);
 let manifest=json!({"format":"nxt-shadow-snapshot/v1","id":"http-source","cohort_ref":"synthetic-A","company_name":"Synthetic source A","source_system":"quickbooks","captured_at":"2026-10-27T00:00:00Z","cutoff_date":"2026-10-27","reviewer_label":"Synthetic source operator","source_of_truth":"original_system","mode":"parallel_test","source_archive":{"file_name":"synthetic.zip","sha256":archive,"bytes":bytes.len(),"encoding":"hex","chunks":[archive_ref]},"reports":[{"id":"journal","title":"Synthetic journal","sheet_name":"Sheet1","source_sha256":"a".repeat(64),"source_bytes":10,"columns":["A"],"row_count":1,"parts":[{"sha256":rowsha,"row_start":5,"row_end":5,"row_count":1}]}],"exceptions":[],"limitations":["Synthetic only; no original XLS read or independent identity proof."]});
 let(status,published)=call(&app,"POST","/api/migrations/snapshots",Some(manifest)).await;assert_eq!(status,200);let server_sha=published["manifest_sha256"].as_str().unwrap().to_string();
 let source=Source{archive_sha256:archive,archive_bytes:bytes.len(),report_sha256:"b".repeat(64),member:"Customers.xlsx".into(),worksheet_name:"Sheet1".into(),worksheet_file:"xl/worksheets/sheet1.xml".into(),company_heading_sha256:"c".repeat(64),captured_at:"2026-10-27T00:00:00Z".into(),capture_basis:"Synthetic HTTP source capture".into()};
 let raw=[("B",Some("Synthetic <customer>")),("C",Some("")),("D",None),("E",Some("Synthetic full name")),("F",Some("Synthetic address")),("G",Some("Synthetic shipping address"))].into_iter().map(|(k,v)|(k.into(),Cell{present:v.is_some(),cell_type:v.map(|_|"str".into()),value:v.map(Into::into),formula:None})).collect::<std::collections::BTreeMap<String,Cell>>();
 let records=(6..8).map(|row|Record{cohort_ref:"synthetic-A".into(),record_type:Kind::Customer,source_record_alias:format!("customer_{}",sha(&enc(&json!(["synthetic-A",source.archive_sha256,source.report_sha256,source.worksheet_file,row])))),source_id:None,source_id_coverage:"archive_row".into(),source_identity_verified:false,operational_object_created:false,linked_to_journal_contacts:false,formula_fields_present:false,source_archive_sha256:source.archive_sha256.clone(),source_report_sha256:source.report_sha256.clone(),source_member:source.member.clone(),source_worksheet_name:source.worksheet_name.clone(),source_worksheet_file:source.worksheet_file.clone(),source_row:row,source_fields:Kind::Customer.fields().into_iter().map(|(col,key)|(key.into(),raw[col].clone())).collect(),raw_source_cells:raw.clone()}).collect::<Vec<_>>();
 let part=Part{format:"nxt-company-contact-part/v1".into(),company_id:"synthetic-A".into(),kind:Kind::Customer,source:source.clone(),records:records.clone(),other_source_rows:vec![]};
 assert_eq!(call(&app,"POST","/api/company-books/foreign/source-contact-lists/parts",Some(json!(part))).await.0,400);
 let(status,uploaded)=call(&app,"POST","/api/company-books/synthetic-A/source-contact-lists/parts",Some(json!(part))).await;assert_eq!(status,200);
 let family=Family{format:"nxt-company-contact-family/v2".into(),company_id:"synthetic-A".into(),kind:Kind::Customer,source:source.clone(),headers:Kind::Customer.headers(),parts:vec![PartRef{sha256:uploaded["sha256"].as_str().unwrap().into(),bytes:uploaded["bytes"].as_u64().unwrap()as usize,records:2,other_rows:0}],records_total:2,nonrecord_rows_total:0,source_snapshot_id:"http-source".into(),source_manifest_sha256:server_sha};
 let proof=OperatorProof{format:"nxt-source-contact-operator-equality/v1".into(),company_id:"synthetic-A".into(),source_archive_sha256:source.archive_sha256,source_report_sha256:source.report_sha256,family_sha256:sha(&enc(&family)),comparison_manifest_sha256:"d".repeat(64),source_field_equality_verified:true};
 let command=Publish{request_id:"synthetic-http-publish".into(),expected_index_head:None,family,source_operator_proof:proof};let(status,publication)=call(&app,"POST","/api/company-books/synthetic-A/source-contact-lists",Some(json!(command))).await;assert_eq!(status,200);let capture=publication["capture_sha256"].as_str().unwrap();
 let(status,page)=call(&app,"GET",&format!("/api/company-books/synthetic-A/source-contact-lists/customer/{capture}/records?offset=0&limit=1&max_bytes=786432"),None).await;assert_eq!(status,200);assert_eq!(page["records"][0],serde_json::to_value(&records[0]).unwrap());assert_eq!(page["next_offset"],1);
 let(_,after)=call(&app,"GET","/api/ledger/contacts?kind=customer",None).await;assert_eq!(before,after);
 let(_,books)=call(&app,"GET","/api/company-books/synthetic-A",None).await;assert!(books["head"].is_null());
 unsafe{std::env::remove_var("ADMIN_API_KEY")};
}
