use axum::{Router,body::Body,http::Request};
use nxt_os::{app_with,store::Store};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/exception-test/test-copy";
fn app(store:Store)->Router { unsafe {std::env::set_var("ADMIN_API_KEY","exception-local-test");}app_with(||store.clone()) }
fn hash(bytes:&[u8])->String {hex::encode(Sha256::digest(bytes))}
fn input()->Value {
    let mut bytes=vec![0;70];bytes[0..4].copy_from_slice(&58u32.to_le_bytes());bytes[4..8].copy_from_slice(b"EXC\0");bytes[8..12].copy_from_slice(&99u32.to_le_bytes());bytes[12..16].copy_from_slice(&100u32.to_le_bytes());bytes[58..62].copy_from_slice(&12u32.to_le_bytes());bytes[62..66].copy_from_slice(b"EXP\0");
    let mut year=vec![12,0,0,0,b'E',b'X',b'Y',0];year.extend(2024u32.to_le_bytes());
    json!({"test_copy":true,"source_archive_sha256":"a".repeat(64),"source_archive_bytes":1000,"source_context_offset":100,"source_context_sha256":hash(&bytes),"source_context_hex":hex::encode(bytes),"source_year_frame_offset":56,"source_year_frame_sha256":hash(&year),"source_year_frame_hex":hex::encode(year),"family_file_id":"99","client_number":"100","tax_year":2024,"reason":"missing_identity_information"})
}
async fn call(app:&Router,method:&str,path:&str,body:Value,auth:bool)->(u16,Value) {
    let mut req=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json");if auth{req=req.header("authorization","Bearer exception-local-test");}
    let response=app.clone().oneshot(req.body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap()).await.unwrap();
    assert_eq!(response.headers().get("cache-control").unwrap(),"private, no-store");let status=response.status().as_u16();let bytes=axum::body::to_bytes(response.into_body(),1_000_000).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
#[tokio::test]
async fn exact_unreadable_bytes_are_immutable_without_inventing_profiles_or_figures(){
    let store=Store::memory();let app=app(store.clone());let v=input();
    let (_,before)=call(&app,"GET","summary",Value::Null,true).await;
    let (status,r)=call(&app,"POST","source-exceptions",v.clone(),true).await;assert_eq!(status,200,"{r}");assert_eq!(r["created"],true);assert_eq!(r["record"]["frame_tags"],json!(["EXC","EXP"]));assert_eq!(r["record"]["identity_metadata_readable"],false);assert!(r["record"].get("source_context_hex").is_none());
    let (_,r)=call(&app,"POST","source-exceptions",v.clone(),true).await;assert_eq!(r["created"],false);
    let path=format!("source-exceptions/{}/100","a".repeat(64));let (_,r)=call(&app,"GET",&path,Value::Null,true).await;assert_eq!(r["source_context_hex"],v["source_context_hex"]);assert_eq!(r["source_year_frame_hex"],v["source_year_frame_hex"]);
    let mut changed=v.clone();changed["reason"]=json!("empty_information_payload");assert_eq!(call(&app,"POST","source-exceptions",changed,true).await.0,409);
    let (_,r)=call(&app,"GET","source-exceptions?client_number=100&tax_year=2024&path=other-company",Value::Null,true).await;assert_eq!(r["source_scope"],"exception-test");assert_eq!(r["total"],1);assert!(r["records"][0].get("source_context_hex").is_none());
    assert_eq!(call(&app,"GET","source-exceptions?client_number=200",Value::Null,true).await.1["total"],0);
    let (_,after)=call(&app,"GET","summary",Value::Null,true).await;assert_eq!(before,after);
    // Matching retries repair an index write interrupted after its immutable body.
    let second=Store::memory();second.set(&format!("dtmaxtc:exception-test:source_exception:record:{}:100","a".repeat(64)),&serde_json::to_string(&serde_json::from_value::<nxt_os::source_import::source_exceptions::SourceExceptionInput>(v.clone()).unwrap()).unwrap()).await.unwrap();
    let app2=crate::app(second);assert_eq!(call(&app2,"POST","source-exceptions",v.clone(),true).await.1["created"],false);assert_eq!(call(&app2,"GET","source-exceptions",Value::Null,true).await.1["total"],1);
    store.set(&format!("dtmaxtc:exception-test:source_exception:record:{}:100","a".repeat(64)),"{}").await.unwrap();assert_eq!(call(&app,"GET",&path,Value::Null,true).await.0,503);
}
#[tokio::test]
async fn rejects_untrusted_lengths_ids_years_digests_unknown_fields_and_scope(){
    let app=app(Store::memory());let v=input();
    assert_eq!(call(&app,"POST","source-exceptions",v.clone(),false).await.0,401);assert_eq!(call(&app,"GET","source-exceptions",Value::Null,false).await.0,401);
    for (key,value) in [("client_number",json!("200")),("family_file_id",json!("098")),("tax_year",json!(2025)),("source_context_offset",json!(999)),("source_year_frame_offset",json!(99)),("source_context_sha256",json!("b".repeat(64))),("source_context_hex",json!("00".repeat(65537))),("source_year_frame_hex",json!("00")),("reason",json!("guessed_identity")),("test_copy",json!(false)),("unknown",json!(true))] {
        let mut b=v.clone();b[key]=value;assert_ne!(call(&app,"POST","source-exceptions",b,true).await.0,200,"accepted invalid {key}");
    }
    for query in ["limit=0","limit=101","client_number=abc","tax_year=1","other=1","offset=-1"] { assert_eq!(call(&app,"GET",&format!("source-exceptions?{query}"),Value::Null,true).await.0,400); }
    for (at,len,tag) in [(0,57,b"EXC\0"),(58,12,b"EXC\0"),(58,1,b"EXP\0"),(0,1000,b"EXC\0")] {
        let mut b=v.clone();let mut bytes=hex::decode(b["source_context_hex"].as_str().unwrap()).unwrap();bytes[at..at+4].copy_from_slice(&(len as u32).to_le_bytes());bytes[at+4..at+8].copy_from_slice(tag);b["source_context_hex"]=json!(hex::encode(&bytes));b["source_context_sha256"]=json!(hash(&bytes));assert_eq!(call(&app,"POST","source-exceptions",b,true).await.0,400);
    }
}
#[tokio::test]
async fn optional_original_contexts_pass_parser_and_byte_exact_readback(){
    let Ok(path)=std::env::var("NXT_DTMAX_EXCEPTION_INPUTS")else{return};
    let rows:Vec<Value>=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(rows.len(),8);
    let app=app(Store::memory());let mut bytes=0;
    for row in rows {
        let (status,result)=call(&app,"POST","source-exceptions",row.clone(),true).await;assert_eq!(status,200,"Original context parser rejected source frame structure");
        let path=format!("source-exceptions/{}/{}",row["source_archive_sha256"].as_str().unwrap(),row["source_context_offset"].as_u64().unwrap());let (status,result2)=call(&app,"GET",&path,Value::Null,true).await;assert_eq!(status,200);assert_eq!(result2["record"],result["record"]);assert_eq!(result2["source_context_hex"],row["source_context_hex"]);bytes+=row["source_context_hex"].as_str().unwrap().len()/2;
    }
    assert_eq!(bytes,1269);assert_eq!(call(&app,"GET","source-exceptions",Value::Null,true).await.1["total"],8);
    println!("PRIVATE_ORIGINAL_CONTEXTS_PARSED_AND_ROUND_TRIPPED: contexts=8; bytes=1269");
}
