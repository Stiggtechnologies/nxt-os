use axum::{Router,body::Body,http::Request};
use nxt_os::{app_with,store::Store};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/legacy-read-test/test-copy";
fn app()->Router {
    unsafe {std::env::set_var("ADMIN_API_KEY","legacy-read-local-test");}
    app_with(Store::memory)
}
async fn call(app:&Router,method:&str,path:&str,body:Value,auth:bool)->(u16,Value) {
    let mut req=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json");
    if auth {req=req.header("authorization","Bearer legacy-read-local-test");}
    let response=app.clone().oneshot(req.body(if method=="GET" {Body::empty()}else{Body::from(body.to_string())}).unwrap()).await.unwrap();
    assert_eq!(response.headers().get("cache-control").unwrap(),"private, no-store");
    let status=response.status().as_u16();let bytes=axum::body::to_bytes(response.into_body(),usize::MAX).await.unwrap();
    (status,serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"error":String::from_utf8_lossy(&bytes)})))
}
fn indexed(client:&str,year:u16,sha:char)->Value {
    json!({"client_number":client,"tax_year":year,"extension":format!("P{:02}",year%100),"size_bytes":100,"sha256":sha.to_string().repeat(64),"header_year":year,"sections":2,"last_saved_epoch":1,"content_state":"vendor_encoded_not_decoded"})
}
#[tokio::test]
async fn actual_unmatched_ledger_preserves_stems_and_requires_exact_family_hash() {
    let app=app();
    call(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"100","name":"Synthetic principal"},{"client_number":"200","name":"Synthetic partner"}]}),true).await;
    call(&app,"POST","returns",json!({"test_copy":true,"returns":[indexed("99",2024,'a'),indexed("100",2024,'b'),indexed("9",2023,'c')]}),true).await;
    call(&app,"POST","family-files",json!({"test_copy":true,"records":[
        {"family_file_id":"99","tax_year":2024,"source_file_sha256":"a".repeat(64),"archive_sha256":"e".repeat(64),"member_client_numbers":["100","200"]},
        {"family_file_id":"9","tax_year":2023,"source_file_sha256":"d".repeat(64),"archive_sha256":"e".repeat(64),"member_client_numbers":["100"]}]}),true).await;
    let (_,before)=call(&app,"GET","summary",Value::Null,true).await;
    let (status,page)=call(&app,"GET","legacy-records?kind=returns&limit=1",Value::Null,true).await;
    assert_eq!(status,200);assert_eq!(page["total"],2);assert_eq!(page["source_total"],3);assert_eq!(page["raw_unlinked_total"],2);
    assert_eq!(page["records"][0]["record_id"],"9:2023");assert!(page["records"][0]["exact_family_file_context"].is_null());
    let (_,page)=call(&app,"GET","legacy-records?kind=returns&limit=1&offset=1&path=%2Fapi%2Fother-scope",Value::Null,true).await;
    assert_eq!(page["source_scope"],"legacy-read-test");let row=&page["records"][0];
    assert_eq!(row["record_id"],"99:2024");assert_eq!(row["record"]["client_number"],"99");assert_eq!(row["raw_client_registered"],false);
    assert_eq!(row["exact_family_file_context"]["member_client_numbers"],json!(["100","200"]));
    assert_eq!(row["record_sha256"],hex::encode(Sha256::digest(serde_json::to_vec(&row["record"]).unwrap())));
    let (_,linked)=call(&app,"GET","legacy-records?kind=returns&linkage=linked",Value::Null,true).await;
    assert_eq!(linked["total"],1);assert_eq!(linked["records"][0]["record"]["client_number"],"100");
    let (_,after)=call(&app,"GET","summary",Value::Null,true).await;assert_eq!(before,after);
    assert_eq!(page["source_rows_modified"],0);assert_eq!(page["tax_cases_imported"],0);
}
#[tokio::test]
async fn unlinked_slips_retain_exact_source_identity_and_verbatim_boxes() {
    let app=app();let slip=json!({"client_number":"0","recipient_name":"Synthetic recipient","tax_year":2024,"source_file_sha256":"a".repeat(64),"row":7,"boxes":{"20":"1,234.50","21":"0"}});
    call(&app,"POST","t5008",json!({"test_copy":true,"slips":[slip.clone()]}),true).await;
    let (status,page)=call(&app,"GET","legacy-records?kind=t5008",Value::Null,true).await;
    assert_eq!(status,200);assert_eq!(page["total"],1);assert_eq!(page["records"][0]["record"],slip);
    assert_eq!(page["records"][0]["record_id"],format!("0:2024:{}:7","a".repeat(64)));
    assert_eq!(page["records"][0]["raw_client_registered"],false);
    assert!(page["records"][0]["exact_family_file_context"].is_null());
    let (_,empty)=call(&app,"GET","legacy-records?kind=t5008&offset=1",Value::Null,true).await;
    assert_eq!(empty["total"],1);assert_eq!(empty["records"],json!([]));
}
#[tokio::test]
async fn ledger_reads_require_auth_and_bounded_declared_filters() {
    let app=app();
    assert_eq!(call(&app,"GET","legacy-records?kind=returns",Value::Null,false).await.0,401);
    for query in ["kind=clients","kind=returns&limit=0","kind=returns&limit=101","kind=returns&linkage=guess","kind=returns&unexpected=1","kind=returns&offset=-1"] {
        assert_eq!(call(&app,"GET",&format!("legacy-records?{query}"),Value::Null,true).await.0,400);
    }
}
