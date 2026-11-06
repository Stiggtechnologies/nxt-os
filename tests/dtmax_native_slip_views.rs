use axum::{body::Body,http::Request,Router};
use nxt_os::{app_with,source_import::{SourceImport,native_slip_views::{NativeSlipBatch,NativeSlipView}},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/native-source-test/test-copy";
fn app()->Router {unsafe{std::env::set_var("ADMIN_API_KEY","native-source-local-test");}app_with(Store::memory)}
async fn request(app:&Router,method:&str,path:&str,body:Value)->(u16,Value) {
    let r=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("authorization","Bearer native-source-local-test").header("content-type","application/json").body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap();
    let r=app.clone().oneshot(r).await.unwrap();let status=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),usize::MAX).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
fn context()->Value {json!({"client_number":"100","family_file_id":"999","tax_year":2025,"archive_sha256":"a".repeat(64),"vendor_module":"T1","mapping_state":"vendor_static_verified","values_cents":{"15000":null,"23600":null,"26000":null,"42000":null,"48400":null,"48500":null}})}
fn view(number:u32)->Value {json!({"source_archive_sha256":"a".repeat(64),"source_view_file_sha256":"b".repeat(64),"source_view_line_sha256":"c".repeat(64),"source_view_number":number,"client_number":"100","family_file_id":"999","source_return_year":2025,"vendor_module":"T1","vendor_slip_kind":"T4","vendor_slip_ordinal":number-1,"vendor_form":"T4","from_data_segment":true,"original_field_count":5,"financial_fields":[{"source_field_index":1,"field_id":13908,"source_type_tag":4,"value":{"kind":"text","text":"123.45"}},{"source_field_index":3,"field_id":13099,"source_type_tag":4,"value":{"kind":"text","text":"0"}}]})}
async fn prepare(app:&Router) {assert_eq!(request(app,"POST","historical-lines",json!({"test_copy":true,"records":[context()]})).await.0,200);}

#[tokio::test]
async fn rejects_unauthenticated_native_reads_and_writes() {
    let app=app();
    for (method,path) in [("POST","native-slip-views"),("GET","native-slip-views/summary"),("GET","clients/100/source-facts?kind=native-slip-views")] {
        let r=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json").body(Body::from("{\"test_copy\":true,\"records\":[]}")).unwrap();assert_eq!(app.clone().oneshot(r).await.unwrap().status(),401);
    }
}
#[tokio::test]
async fn preserves_active_text_zero_order_and_cached_origin_without_deduplicating_views() {
    let app=app();prepare(&app).await;let mut second=view(2);second["from_data_segment"]=json!(false);second["financial_fields"][0]["value"]["text"]=Value::Null;
    let mut older_context=context();older_context["tax_year"]=json!(2024);assert_eq!(request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[older_context]})).await.0,200);
    let mut older=view(10);older["source_return_year"]=json!(2024);let records=vec![view(1),second,older,view(11)];
    for replay in [false,true] {let (status,result)=request(&app,"POST","native-slip-views",json!({"test_copy":true,"records":records})).await;assert_eq!(status,200);assert_eq!(result[if replay{"unchanged_replays"}else{"created"}],4);assert_eq!(result["normalized_slips_created"],0);}
    let (status,page)=request(&app,"GET","clients/100/source-facts?kind=native-slip-views&offset=0&limit=2",Value::Null).await;assert_eq!(status,200);assert_eq!(page["total"],4);assert_eq!(page["records"][0]["source_view_number"],1);assert_eq!(page["records"][1]["source_view_number"],2);
    for (stored,original) in page["records"].as_array().unwrap().iter().zip(&records) {
        for (key,value) in original.as_object().unwrap(){assert_eq!(&stored[key],value);}
        assert_eq!(stored["financial_profile_sha256"],"4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc");assert_eq!(stored["financial_values_approved"],false);assert_eq!(stored["normalized_tax_slip_created"],false);
    }
    assert_eq!(request(&app,"GET","clients/200/source-facts?kind=native-slip-views",Value::Null).await.1["total"],0);
    let (_,next)=request(&app,"GET","clients/100/source-facts?kind=native-slip-views&offset=2&limit=2",Value::Null).await;assert_eq!(next["records"][0]["source_view_number"],10);
    let (_,filtered)=request(&app,"GET","clients/100/source-facts?kind=native-slip-views&source_return_year=2025&offset=2&limit=2",Value::Null).await;assert_eq!(filtered["total"],3);assert_eq!(filtered["source_return_year"],2025);assert_eq!(filtered["records"][0]["source_view_number"],11);
    assert_eq!(request(&app,"GET","clients/100/source-facts?kind=native-slip-views&source_return_year=3000",Value::Null).await.0,400);
    assert_eq!(request(&app,"GET","clients/100/source-facts?kind=historical-lines&source_return_year=2025",Value::Null).await.0,400);
    let (_,summary)=request(&app,"GET","source-fact-summary",Value::Null).await;assert_eq!(summary["counts"]["historical_lines"],2);assert_eq!(summary["tax_cases_imported"],0);
}
#[tokio::test]
async fn rejects_inactive_numeric_substitution_identity_fields_and_different_context_before_writes() {
    let app=app();prepare(&app).await;
    let mut numeric=view(2);numeric["financial_fields"][0]["value"]=json!({"kind":"float32","bits_hex":"00000000"});
    let mut unknown=view(3);unknown["financial_fields"][0]["field_id"]=json!(999999);
    let mut other=view(4);other["family_file_id"]=json!("998");
    let mut reordered=view(5);reordered["financial_fields"].as_array_mut().unwrap().reverse();
    for bad in [numeric,unknown,other,reordered] {assert_eq!(request(&app,"POST","native-slip-views",json!({"test_copy":true,"records":[view(1),bad]})).await.0,400);}
    assert_eq!(request(&app,"GET","native-slip-views/summary",Value::Null).await.1["source_views"],0);
    assert_eq!(request(&app,"POST","native-slip-views",json!({"test_copy":false,"records":[view(1)]})).await.0,409);
}
#[tokio::test]
async fn concurrent_conflicting_native_views_are_immutable() {
    let app=app();prepare(&app).await;let original=view(1);let mut changed=original.clone();changed["financial_fields"][0]["value"]["text"]=json!("999.99");
    let (a,b)=tokio::join!(request(&app,"POST","native-slip-views",json!({"test_copy":true,"records":[original]})),request(&app,"POST","native-slip-views",json!({"test_copy":true,"records":[changed]})));
    assert!((a.0==200&&b.0==409)||(a.0==409&&b.0==200));assert_eq!(request(&app,"GET","native-slip-views/summary",Value::Null).await.1["source_views"],1);
}
#[tokio::test]
async fn matching_retry_repairs_a_body_only_write_without_replacing_source() {
    let store=Store::memory();let service=SourceImport::new(store.clone());let company="repair-native";
    service.test_copy_historical_lines(company,serde_json::from_value(json!({"test_copy":true,"records":[context()]})).unwrap()).await.unwrap();
    let row:NativeSlipView=serde_json::from_value(view(1)).unwrap();let body=serde_json::to_string(&row).unwrap();let key=format!("dtmaxtc:{company}:native_slip:record:{}:000001","b".repeat(64));store.set(&key,&body).await.unwrap();
    let result=service.test_copy_native_slip_views(company,NativeSlipBatch{test_copy:true,records:vec![row]}).await.unwrap();assert_eq!(result["unchanged_replays"],1);assert_eq!(service.test_copy_native_slip_summary(company).await.unwrap()["source_views"],1);assert_eq!(store.get(&key).await.unwrap().as_deref(),Some(body.as_str()));
    let page=service.test_copy_client_source_facts(company,"100",serde_json::from_value(json!({"kind":"native-slip-views","source_return_year":2025})).unwrap()).await.unwrap();assert_eq!(page["total"],1);
}
#[tokio::test]
async fn optional_private_native_staging_keeps_every_field_and_original_historical_record() {
    let Ok(path)=std::env::var("NXT_DTMAX_NATIVE_VIEW_STAGING")else{return};
    let root=std::path::Path::new(&path);let summary:Value=serde_json::from_slice(&std::fs::read(root.join("staging-summary.json")).unwrap()).unwrap();
    let facts=std::path::Path::new(&path).parent().unwrap().join("nxt-os-staging-v5-source-facts");let fact_manifest:Value=serde_json::from_slice(&std::fs::read(facts.join("staging-summary.json")).unwrap()).unwrap();
    let store=Store::memory();let service=SourceImport::new(store.clone());let company="private-native-views";let mut histories=Vec::new();
    for item in fact_manifest["batches"].as_array().unwrap() {if item["kind"]!="historical-lines"{continue}let raw=std::fs::read(facts.join(item["file"].as_str().unwrap())).unwrap();assert_eq!(hex::encode(Sha256::digest(&raw)),item["sha256"]);let batch=serde_json::from_slice::<nxt_os::source_import::preserved_records::SourceFactBatch>(&raw).unwrap();histories.extend(batch.records.clone());assert!(service.test_copy_historical_lines(company,batch).await.unwrap()["rejected"].as_object().unwrap().is_empty());}
    let original_summary=service.test_copy_source_fact_summary(company).await.unwrap();let mut accepted=0;let mut fields=0;let mut year_counts=std::collections::BTreeMap::<String,usize>::new();
    for item in summary["batches"].as_array().unwrap(){let raw=std::fs::read(root.join(item["file"].as_str().unwrap())).unwrap();assert_eq!(hex::encode(Sha256::digest(&raw)),item["sha256"]);let rows:Value=serde_json::from_slice(&raw).unwrap();let batch=serde_json::from_slice::<NativeSlipBatch>(&raw).unwrap();let result=service.test_copy_native_slip_views(company,batch).await.unwrap();assert_eq!(result["created"],item["rows"]);accepted+=result["created"].as_u64().unwrap();
        for original in rows["records"].as_array().unwrap(){let key=format!("dtmaxtc:{company}:native_slip:record:{}:{:06}",original["source_view_file_sha256"].as_str().unwrap(),original["source_view_number"].as_u64().unwrap());let stored:Value=serde_json::from_str(&store.get(&key).await.unwrap().unwrap()).unwrap();assert_eq!(&stored,original);fields+=stored["financial_fields"].as_array().unwrap().len();*year_counts.entry(format!("dtmaxtc:{company}:native_slip:client_year:{}:{}",original["client_number"].as_str().unwrap(),original["source_return_year"].as_u64().unwrap())).or_default()+=1;}
        let replay=service.test_copy_native_slip_views(company,serde_json::from_slice(&raw).unwrap()).await.unwrap();assert_eq!(replay["unchanged_replays"],item["rows"]);
    }
    assert_eq!(accepted,47969);assert_eq!(fields,155901);assert_eq!(service.test_copy_native_slip_summary(company).await.unwrap()["source_views"],47969);assert_eq!(service.test_copy_source_fact_summary(company).await.unwrap(),original_summary);
    assert_eq!(year_counts.len(),12026);for (key,count) in year_counts {assert_eq!(store.smembers(&key).await.unwrap().len(),count);}
    for original in histories{let key=format!("dtmaxtc:{company}:historical_lines:{}:{}:{}",original["client_number"].as_str().unwrap(),original["tax_year"].as_u64().unwrap(),original["archive_sha256"].as_str().unwrap());let stored:Value=serde_json::from_str(&store.get(&key).await.unwrap().unwrap()).unwrap();assert_eq!(stored,original);}
    println!("PRIVATE_NATIVE_VIEWS=47969; ACTIVE_FINANCIAL_FIELDS=155901; REPLAYS_EXACT=true; ORIGINAL_HISTORICAL_RECORDS_UNCHANGED=32744; NORMALIZED_SLIPS_CREATED=0");
}
