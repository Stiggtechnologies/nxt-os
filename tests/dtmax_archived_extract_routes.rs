use axum::{Router,body::Body,http::Request};
use nxt_os::{app_with,store::Store};
use serde_json::{Value,json};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/";
async fn request(app:&Router,method:&str,path:&str,body:Value,auth:bool)->(u16,Value){
 let mut b=Request::builder().method(method).uri(format!("{BASE}{path}")).header("content-type","application/json");if auth{b=b.header("authorization","Bearer archived-route-offline-test");}
 let r=app.clone().oneshot(b.body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap()).await.unwrap();let status=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),2*1024*1024).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
#[tokio::test]
async fn closed_archived_routes_are_admin_gated_additive_and_explicitly_empty(){
 unsafe{std::env::set_var("ADMIN_API_KEY","archived-route-offline-test");}let app=app_with(Store::memory);
 for(method,path)in[("POST","archived-extract-sources"),("POST","archived-extract-sources/read"),("GET","archived-extract-sources/summary"),("GET","archived-extract-sources/unassigned?offset=0&limit=1"),("GET","clients/100/source-facts?kind=archived-extract-sources&offset=0&limit=1&source_return_year=2021")]{assert_eq!(request(&app,method,path,json!({}),false).await.0,401);}
 let(status,p)=request(&app,"GET","archived-extract-sources/summary",Value::Null,true).await;assert_eq!(status,200);assert_eq!(p["received_contexts"],0);assert_eq!(p["assigned_contexts"],0);assert_eq!(p["closed_inventory_complete"],false);assert_eq!(p["current_profiles_modified"],0);
 for path in["archived-extract-sources/unassigned?offset=0&limit=1","clients/100/source-facts?kind=archived-extract-sources&offset=0&limit=1&source_return_year=2021"]{let(status,p)=request(&app,"GET",path,Value::Null,true).await;assert_eq!(status,200,"{p}");assert_eq!(p["records"],json!([]));assert_eq!(p["total"],0);assert_eq!(p["financial_values_approved"],false);}
 assert!(request(&app,"POST","archived-extract-sources",json!({"test_copy":true,"profile_sha256":"unreviewed","records":[]}),true).await.0>=400);
 assert!(request(&app,"GET","archived-extract-sources/unassigned?offset=0&limit=25",Value::Null,true).await.0>=400);
 assert!(request(&app,"GET","archived-extract-sources/unassigned?offset=0&limit=1&source_return_year=2020",Value::Null,true).await.0>=400);
}
