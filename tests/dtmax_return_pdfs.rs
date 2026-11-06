use axum::{Router,body::Body,http::Request};
use nxt_os::{app_with,store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/pdf-test/test-copy";
fn app()->Router{unsafe{std::env::set_var("ADMIN_API_KEY","pdf-local-test");}app_with(Store::memory)}
fn input()->Value{
    use lopdf::{dictionary,Document,Object,Stream};
    let mut d=Document::with_version("1.5");let pages=d.new_object_id();let content=d.add_object(Stream::new(dictionary!{},vec![]));
    let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()],"Contents"=>content});
    d.objects.insert(pages,Object::Dictionary(dictionary!{"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}));let catalog=d.add_object(dictionary!{"Type"=>"Catalog","Pages"=>pages});d.trailer.set("Root",catalog);let mut bytes=Vec::new();d.save_to(&mut bytes).unwrap();
    json!({"test_copy":true,"client_number":"100","tax_year":2024,"sha256":hex::encode(Sha256::digest(&bytes)),"page_labels":["Synthetic T1 page"],"content_hex":hex::encode(bytes)})
}
async fn req(app:&Router,method:&str,path:&str,value:Value,auth:bool)->(u16,Value){
    let mut b=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json");if auth{b=b.header("authorization","Bearer pdf-local-test");}
    let r=app.clone().oneshot(b.body(if method=="GET"{Body::empty()}else{Body::from(value.to_string())}).unwrap()).await.unwrap();let status=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),6*1024*1024).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
#[tokio::test]
async fn original_pdf_is_immutable_hash_checked_and_cannot_cross_client_or_scope(){
    let app=app();let v=input();let sha=v["sha256"].as_str().unwrap();
    req(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"100","name":"Synthetic Person"},{"client_number":"200","name":"Other Person"}]}),true).await;
    for path in ["return-pdfs".to_string(),"clients/100/return-pdfs".into(),format!("clients/100/return-pdfs/2024/{sha}")]{assert_eq!(req(&app,if path=="return-pdfs"{"POST"}else{"GET"},&path,v.clone(),false).await.0,401);}
    let (status,r)=req(&app,"POST","return-pdfs",v.clone(),true).await;assert_eq!(status,200,"{r}");assert_eq!(r["created"],true);
    let (_,r)=req(&app,"POST","return-pdfs",v.clone(),true).await;assert_eq!(r["created"],false);
    let (_,r)=req(&app,"GET","clients/100/return-pdfs",Value::Null,true).await;assert_eq!(r["records"].as_array().unwrap().len(),1);assert!(r["records"][0].get("content_hex").is_none());
    let (status,r)=req(&app,"GET",&format!("clients/100/return-pdfs/2024/{sha}?path=other-company"),Value::Null,true).await;assert_eq!(status,200);assert_eq!(r["content_hex"],v["content_hex"]);assert_eq!(r["tax_cases_imported"],0);
    for path in [format!("clients/200/return-pdfs/2024/{sha}"),format!("clients/100/return-pdfs/2025/{sha}")]{assert_eq!(req(&app,"GET",&path,Value::Null,true).await.0,404);}
    let mut changed=v.clone();changed["page_labels"]=json!(["Changed page"]);assert_eq!(req(&app,"POST","return-pdfs",changed,true).await.0,409);
    for mutate in ["hash","pages","client","flag","unknown","bytes"]{
        let mut b=v.clone();match mutate{"hash"=>b["sha256"]=json!("0".repeat(64)),"pages"=>b["page_labels"]=json!(["One","Two"]),"client"=>b["client_number"]=json!("999"),"flag"=>b["test_copy"]=json!(false),"unknown"=>b["sin"]=json!("000000000"),_=>b["content_hex"]=json!("00".repeat(2097153))};
        assert_ne!(req(&app,"POST","return-pdfs",b,true).await.0,200,"accepted invalid {mutate}");
    }
}
#[tokio::test]
async fn optional_private_print_output_passes_the_same_parser_and_round_trip(){
    let Ok(path)=std::env::var("NXT_DTMAX_PRINT_OUTPUT")else{return};
    let client=std::env::var("NXT_DTMAX_PRINT_CLIENT").expect("explicit source client required");
    let bytes=std::fs::read(path).unwrap();let doc=lopdf::Document::load_mem(&bytes).unwrap();let pages=doc.get_pages().len();let app=app();
    req(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":client,"name":"Private parser verification"}]}),true).await;
    let sha=hex::encode(Sha256::digest(&bytes));let v=json!({"test_copy":true,"client_number":client,"tax_year":2024,"sha256":sha,"page_labels":(1..=pages).map(|p|format!("Source page {p}")).collect::<Vec<_>>(),"content_hex":hex::encode(&bytes)});
    let (status,result)=req(&app,"POST","return-pdfs",v,true).await;assert_eq!(status,200,"{result}");assert_eq!(result["record"]["page_count"],pages);
    let (status,result)=req(&app,"GET",&format!("clients/{client}/return-pdfs/2024/{sha}"),Value::Null,true).await;assert_eq!(status,200);assert_eq!(result["content_hex"],hex::encode(&bytes));
    println!("PRIVATE_ORIGINAL_PDF_PARSED_AND_ROUND_TRIPPED: pages={pages}; bytes={}",bytes.len());
}
#[tokio::test]
async fn optional_retained_originals_all_pass_static_parser_and_exact_replay(){
    let Ok(path)=std::env::var("NXT_DTMAX_RETAINED_PDF_MANIFEST")else{return};let path=std::path::Path::new(&path);
    let manifest_bytes=std::fs::read(path).unwrap();assert_eq!(hex::encode(Sha256::digest(&manifest_bytes)),"c32b2d8eb1a3c5ded5da8b59e64b8ad2c1311409475551789bcfbf00e82e1991");
    let manifest:Value=serde_json::from_slice(&manifest_bytes).unwrap();let records=manifest["records"].as_array().unwrap();assert_eq!(records.len(),66);
    let app=app();let clients:std::collections::BTreeSet<_>=records.iter().map(|r|r["client_number"].as_str().unwrap()).collect();
    let registered=clients.into_iter().map(|id|json!({"client_number":id,"name":"Private original-PDF parser verification"})).collect::<Vec<_>>();
    assert_eq!(req(&app,"POST","clients",json!({"test_copy":true,"clients":registered}),true).await.0,200);
    let before=req(&app,"GET","summary",Value::Null,true).await.1;let mut pages=0;
    for r in records {
        let file=path.parent().unwrap().join(r["file"].as_str().unwrap());let bytes=std::fs::read(file).unwrap();
        assert!(hex::encode(Sha256::digest(&bytes))==r["sha256"].as_str().unwrap(),"Pinned original PDF hash changed");
        assert_eq!(bytes.len(),r["bytes"].as_u64().unwrap() as usize);
        let v=json!({"test_copy":true,"client_number":r["client_number"],"tax_year":r["tax_year"],"sha256":r["sha256"],"page_labels":r["page_labels"],"content_hex":hex::encode(&bytes)});
        let(status,result)=req(&app,"POST","return-pdfs",v.clone(),true).await;assert_eq!(status,200,"Original PDF static parser rejected a verified staged document: {result}");
        assert_eq!(result["record"]["page_count"],r["page_count"]);assert_eq!(result["created"],true);
        assert_eq!(req(&app,"POST","return-pdfs",v.clone(),true).await.1["created"],false);
        let url=format!("clients/{}/return-pdfs/{}/{}",r["client_number"].as_str().unwrap(),r["tax_year"],r["sha256"].as_str().unwrap());
        let(status,result)=req(&app,"GET",&url,Value::Null,true).await;assert_eq!(status,200);assert!(result["content_hex"]==v["content_hex"],"Original PDF bytes changed during round trip");
        assert_eq!(result["record"]["page_labels"],r["page_labels"]);pages+=r["page_count"].as_u64().unwrap();
    }
    assert_eq!(pages,1276);assert_eq!(req(&app,"GET","summary",Value::Null,true).await.1,before);
    println!("PRIVATE_RETAINED_ORIGINAL_PDFS=66; PAGES=1276; BYTE_EXACT_ROUND_TRIP_AND_REPLAY=true; ORIGINAL_SOURCE_CONTROLS_UNCHANGED=true");
}
