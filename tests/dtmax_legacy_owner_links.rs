use axum::{Router,body::Body,http::Request};
use nxt_os::{app_with,store::Store,source_import::SourceImport};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/owner-link-test/test-copy";
fn hash(v:&Value)->String{hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn app()->Router{unsafe{std::env::set_var("ADMIN_API_KEY","owner-link-local-test");}app_with(Store::memory)}
async fn call(app:&Router,method:&str,path:&str,body:Value,auth:bool)->(u16,Value){
    let mut r=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json");if auth{r=r.header("authorization","Bearer owner-link-local-test");}
    let r=app.clone().oneshot(r.body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap()).await.unwrap();
    let status=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),usize::MAX).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
fn source(row:u32,weak:bool)->Value{json!({"client_number":"0","recipient_name":"PERSON, Synthetic","tax_year":2024,"source_file_sha256":"c".repeat(64),"row":row,"boxes":if weak{json!({"20":"1,234.50"})}else{json!({"17":"SEC A","18":"Example Unit","20":"1,234.50","21":"0"})}})}
fn cra()->Value{
    let ns="http://www.cra-arc.gc.ca/xmlns/tdd/2-10-1";
    let mut children=vec![json!({"tag":format!("{{{ns}}}TaxYear"),"attributes":{},"text":"2024","children":[]})];
    for (tag,code,value) in [("DisposedSecurityIdentificationNumber","17","SEC A"),("DisposedSecurityDescription","18","Example Unit"),("SecurityBookValueAmount","20","1234.5"),("SecuritySettlementAmount","21","0.00")]{children.push(json!({"tag":format!("{{{ns}}}{tag}"),"attributes":{"boxNumber":code},"text":value,"children":[]}));}
    let tree=json!({"tag":format!("{{{ns}}}T5008Slip"),"attributes":{},"text":null,"children":children});
    json!({"client_number":"100","tax_year":2024,"source_folder_year":2025,"slip_kind":"T5008Slip","archive_sha256":"a".repeat(64),"source_frame_sha256":"b".repeat(64),"source_frame_offset":56,"source_occurrence":1,"slip_content_sha256":hash(&tree),"vendor_field_tree":tree})
}
fn identity(row:u32)->String{format!("0:2024:{}:{row}","c".repeat(64))}
fn link(row:u32,weak:bool)->Value{let c=cra();json!({"legacy_record_id":identity(row),"legacy_record_sha256":hash(&source(row,weak)),"owner_client_number":"100","owner_name_sha256":hex::encode(Sha256::digest(b"Synthetic Person")),"evidence_mode":if weak{"same_recipient_file_and_all_boxes"}else{"recipient_name_and_all_boxes"},"matching_cra_records":[{"archive_sha256":c["archive_sha256"],"source_occurrence":1,"slip_content_sha256":c["slip_content_sha256"]}],"supporting_legacy_record_ids":if weak{vec![identity(1)]}else{vec![]}})}
async fn prepare(app:&Router){
    assert_eq!(call(app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"100","name":"Synthetic Person"},{"client_number":"200","name":"Another Person"}]}),true).await.1["created"],2);
    assert_eq!(call(app,"POST","t5008",json!({"test_copy":true,"slips":[source(1,false),source(2,true)]}),true).await.1["unlinked"],2);
    assert_eq!(call(app,"POST","afr-slips",json!({"test_copy":true,"records":[cra()]}),true).await.1["created"],1);
}
#[tokio::test]
async fn preserves_original_rows_and_controls_while_exposing_verified_owner_and_year(){
    let app=app();prepare(&app).await;let before=call(&app,"GET","summary",Value::Null,true).await.1;
    assert_eq!(call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[link(2,true)]}),true).await.0,409);
    for replay in [false,true]{let (status,p)=call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[link(1,false)]}),true).await;assert_eq!(status,200,"{p}");assert_eq!(p[if replay{"unchanged_replays"}else{"created"}],1);}
    let (status,p)=call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[link(2,true)]}),true).await;assert_eq!(status,200,"{p}");
    let (_,summary)=call(&app,"GET","legacy-owner-links/summary",Value::Null,true).await;assert_eq!(summary["verified_owner_links"],2);assert_eq!(summary["remaining_owner_exceptions"],0);assert_eq!(summary["raw_legacy_CSV_owner_ids_unregistered"],2);
    let (_,p)=call(&app,"GET","clients/100/source-facts?kind=linked-t5008&source_return_year=2024&offset=1&limit=1",Value::Null,true).await;assert_eq!(p["total"],2);assert_eq!(p["records"][0]["original_source_record"],source(2,true));assert_eq!(p["records"][0]["client_number"],"100");assert_eq!(p["records"][0]["source_owner_id_changed"],false);
    assert_eq!(call(&app,"GET","clients/100/source-facts?kind=linked-t5008&source_return_year=2023",Value::Null,true).await.1["total"],0);
    assert_eq!(call(&app,"GET","clients/200/source-facts?kind=linked-t5008",Value::Null,true).await.1["total"],0);
    assert_eq!(call(&app,"GET","summary",Value::Null,true).await.1,before);
    let (_,raw)=call(&app,"GET","legacy-records?kind=t5008",Value::Null,true).await;assert_eq!(raw["total"],2);assert_eq!(raw["records"][0]["record"],source(1,false));
}
#[tokio::test]
async fn rejects_wrong_owner_name_year_hash_boxes_and_unverified_support_before_any_write(){
    let app=app();prepare(&app).await;
    for (field,value) in [("owner_client_number",json!("200")),("owner_name_sha256",json!("0".repeat(64))),("legacy_record_sha256",json!("0".repeat(64))),("legacy_record_id",json!(identity(3)))]{
        let mut changed=link(1,false);changed[field]=value;let(status,_)=call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[link(1,false),changed]}),true).await;assert_ne!(status,200);
        assert_eq!(call(&app,"GET","legacy-owner-links/summary",Value::Null,true).await.1["verified_owner_links"],0);
    }
    let mut mismatch=source(3,false);mismatch["boxes"]["20"]=json!("1,23");call(&app,"POST","t5008",json!({"test_copy":true,"slips":[mismatch.clone()]}),true).await;
    let mut l=link(1,false);l["legacy_record_id"]=json!(identity(3));l["legacy_record_sha256"]=json!(hash(&mismatch));assert_eq!(call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[l]}),true).await.0,400);
    let mut l=link(1,false);l["matching_cra_records"][0]["slip_content_sha256"]=json!("f".repeat(64));assert_eq!(call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[l]}),true).await.0,409);
}
#[tokio::test]
async fn duplicate_registered_names_do_not_establish_an_owner(){
    let app=app();prepare(&app).await;call(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"300","name":"Person Synthetic"}]}),true).await;
    assert_eq!(call(&app,"POST","legacy-owner-links",json!({"test_copy":true,"records":[link(1,false)]}),true).await.0,409);
}
#[tokio::test]
async fn owner_routes_require_auth_and_test_copy_and_bounded_pages(){
    let app=app();prepare(&app).await;
    for(method,path)in[("POST","legacy-owner-links"),("GET","legacy-owner-links/summary"),("GET","clients/100/source-facts?kind=linked-t5008")]{assert_eq!(call(&app,method,path,json!({"test_copy":true,"records":[link(1,false)]}),false).await.0,401);}
    assert_eq!(call(&app,"POST","legacy-owner-links",json!({"test_copy":false,"records":[link(1,false)]}),true).await.0,409);
    for query in ["limit=0","limit=101","source_return_year=3000"]{assert_eq!(call(&app,"GET",&format!("clients/100/source-facts?kind=linked-t5008&{query}"),Value::Null,true).await.0,400);}
}
#[tokio::test]
async fn optional_private_owner_package_preserves_all_31_sources_and_repairs_replays(){
    let Ok(path)=std::env::var("NXT_DTMAX_OWNER_LINK_INPUTS")else{return};let bytes=std::fs::read(path).unwrap();let data:Value=serde_json::from_slice(&bytes).unwrap();
    let store=Store::memory();let service=SourceImport::new(store.clone());let scope="private-owner-links";
    for chunk in data["clients"].as_array().unwrap().chunks(1000){let result=service.test_copy_clients(scope,serde_json::from_value(json!({"test_copy":true,"clients":chunk})).unwrap()).await.unwrap();assert!(result["rejected"].as_object().unwrap().is_empty(),"{result}");}
    service.test_copy_slips(scope,serde_json::from_value(json!({"test_copy":true,"slips":data["original_slips"]})).unwrap()).await.unwrap();
    let result=service.test_copy_afr_slips(scope,serde_json::from_value(json!({"test_copy":true,"records":data["cra_records"]})).unwrap()).await.unwrap();assert!(result["rejected"].as_object().unwrap().is_empty(),"{result}");
    let before=service.test_copy_summary(scope).await.unwrap();
    let first:nxt_os::source_import::legacy_owner_links::LegacyOwnerLink=serde_json::from_value(data["batches"][0]["records"][0].clone()).unwrap();
    store.set(&format!("dtmaxtc:{scope}:legacy_owner_link:{}",first.legacy_record_id),&serde_json::to_string(&first).unwrap()).await.unwrap();
    // A previous interrupted body claim has no all/client indexes. A matching
    // replay must repair them without replacing either the link or its source.
    for (index,batch) in data["batches"].as_array().unwrap().iter().enumerate(){let result=service.test_copy_legacy_owner_links(scope,serde_json::from_value(batch.clone()).unwrap()).await.unwrap();assert_eq!(result["created"],batch["records"].as_array().unwrap().len()-usize::from(index==0));let result=service.test_copy_legacy_owner_links(scope,serde_json::from_value(batch.clone()).unwrap()).await.unwrap();assert_eq!(result["unchanged_replays"],batch["records"].as_array().unwrap().len());}
    let summary=service.test_copy_legacy_owner_summary(scope).await.unwrap();assert_eq!(summary["verified_owner_links"],31);assert_eq!(summary["remaining_owner_exceptions"],0);assert_eq!(service.test_copy_summary(scope).await.unwrap(),before);
    for original in data["original_slips"].as_array().unwrap(){let id=format!("{}:{}:{}:{}",original["client_number"].as_str().unwrap(),original["tax_year"],original["source_file_sha256"].as_str().unwrap(),original["row"]);let body:Value=serde_json::from_str(&store.get(&format!("dtmaxtc:{scope}:t5008:{id}")).await.unwrap().unwrap()).unwrap();assert_eq!(&body,original);}
    println!("PRIVATE_OWNER_LINKS=31; OWNER_EXCEPTIONS_REMAINING=0; ORIGINAL_ROWS_AND_CONTROLS_UNCHANGED=true; NORMALIZED_SLIPS_CREATED=0");
}
