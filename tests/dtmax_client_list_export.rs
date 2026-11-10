//! Synthetic authenticated read-only export. No production identity or network.
use axum::{body::Body,http::Request};
use nxt_os::{app_with,firm::staff,store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::sync::Arc;
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/recovered-native-sources/effective/source-clients";
async fn call(app:&axum::Router,method:&str,url:&str,token:&str,body:Option<Value>)->(u16,Value){
 let mut q=Request::builder().method(method).uri(url);if !token.is_empty(){q=q.header("authorization",format!("Bearer {token}"));}
 let b=if let Some(v)=body{q=q.header("content-type","application/json");Body::from(v.to_string())}else{Body::empty()};
 let r=app.clone().oneshot(q.body(b).unwrap()).await.unwrap();let status=r.status().as_u16();if status==200&&url.starts_with(BASE){assert_eq!(r.headers()["cache-control"],"private, no-store");assert_eq!(r.headers()["x-content-type-options"],"nosniff");}
 let raw=axum::body::to_bytes(r.into_body(),3*1024*1024).await.unwrap();(status,serde_json::from_slice(&raw).unwrap_or(Value::Null))
}
#[tokio::test]
async fn complete_export_is_authenticated_filtered_bounded_and_read_only(){
 unsafe{std::env::set_var("ADMIN_API_KEY","k");std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS","1000");}
 staff::install(Arc::new(staff::Staff::new(Store::memory())));let store=Store::memory();let app=app_with(||store.clone());
 for (id,name,status,province,year) in [("001","Synthetic É, \"quoted\"","Ready","AB",2024),("2","Synthetic second","Ready","AB",2024),("3","Synthetic third","Held","ON",2023)]{
  store.sadd("dtmaxtc:dtmax-test-copy:clients",id).await.unwrap();store.set(&format!("dtmaxtc:dtmax-test-copy:client:{id}"),&json!({"client_number":id,"name":name,"processing_status":status,"province":province}).to_string()).await.unwrap();store.sadd("dtmaxtc:dtmax-test-copy:historical_lines",&format!("{id}:{year}:{}","a".repeat(64))).await.unwrap();
 }
 assert_eq!(call(&app,"GET",&format!("{BASE}/export"),"",None).await.0,401);
 let url=format!("{BASE}/export?search=Synthetic&year=2024&status=Ready&province=AB&sort=name&direction=desc");let (code,p)=call(&app,"GET",&url,"k",None).await;assert_eq!(code,200);assert_eq!(p["schema"],"dtmax-source-client-list-export/v1");assert_eq!(p["projection"]["total"],2);assert_eq!(p["projection"]["clients"][0]["client_number"],"001");assert_eq!(p["projection"]["clients"][1]["client_number"],"2");assert_eq!(p["atomic_snapshot"],false);assert_eq!(p["observed_stable_reads_only"],true);assert_eq!(p["projection_sha256"],hex::encode(Sha256::digest(serde_json::to_vec(&p["projection"]).unwrap())));
 let page=call(&app,"GET",&format!("{BASE}?search=Synthetic&year=2024&status=Ready&province=AB&sort=name&direction=desc&offset=0&limit=100"),"k",None).await;assert_eq!(page.0,200);assert_eq!(page.1["clients"],p["projection"]["clients"]);
 assert_eq!(call(&app,"GET",&format!("{BASE}?limit=101"),"k",None).await.0,400);
 for suffix in ["offset=0","limit=10000","unknown=1","year=1989","year=2101","sort=unsafe","direction=unsafe"]{assert_eq!(call(&app,"GET",&format!("{BASE}/export?{suffix}"),"k",None).await.0,400);}
 assert_eq!(call(&app,"POST",&format!("{BASE}/export"),"k",None).await.0,405);
 assert_eq!(call(&app,"GET",&format!("{BASE}/export?search=no-match"),"k",None).await.1["projection"]["total"],0);
 assert!(call(&app,"GET",&url.replace("dtmax-test-copy","different-copy"),"k",None).await.0>=400);
 // Real synthetic staff authentication; client-limited permission cannot export.
 let pw="synthetic-pass-phrase-1";
 assert_eq!(call(&app,"POST","/api/staff/users","k",Some(json!({"username":"scoped-export","display_name":"Synthetic Scoped","role":"preparer","clients":["001"],"temporary_password":pw}))).await.0,200);
 let token=call(&app,"POST","/api/staff/login","",Some(json!({"username":"scoped-export","password":pw}))).await.1["token"].as_str().unwrap().to_owned();
 let secret=call(&app,"POST","/api/staff/mfa/enroll",&token,Some(json!({}))).await.1["secret"].as_str().unwrap().to_owned();
 assert_eq!(call(&app,"POST","/api/staff/mfa/confirm",&token,Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await.0,200);
 assert_eq!(call(&app,"POST","/api/staff/me/password",&token,Some(json!({"current_password":pw,"new_password":"synthetic-pass-phrase-2"}))).await.0,200);
 assert_eq!(call(&app,"GET",&format!("{BASE}/export"),&token,None).await.0,403);
 assert_eq!(store.smembers("dtmaxtc:dtmax-test-copy:clients").await.unwrap().len(),3);assert_eq!(store.smembers("dtmaxtc:dtmax-test-copy:returns").await.unwrap().len(),0);
 // A missing indexed profile is an error, never a partial roster.
 store.sadd("dtmaxtc:dtmax-test-copy:clients","4").await.unwrap();assert!(call(&app,"GET",&format!("{BASE}/export"),"k",None).await.0>=400);
}
