use axum::{body::Body,http::Request,Router};
use nxt_os::{app_with,store::Store};
use serde_json::{json,Value};
use tower::ServiceExt;
const BASE:&str="/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy";
async fn request(app:&Router,method:&str,path:&str,body:Value,auth:bool)->(u16,Value){
    let mut b=Request::builder().method(method).uri(format!("{BASE}/{path}")).header("content-type","application/json");if auth{b=b.header("authorization","Bearer native-recovery-test");}
    let r=app.clone().oneshot(b.body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap()).await.unwrap();let status=r.status().as_u16();let bytes=axum::body::to_bytes(r.into_body(),1024*1024).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
#[tokio::test]
async fn every_recovery_route_is_wired_admin_gated_and_read_only_when_empty(){
    unsafe{std::env::set_var("ADMIN_API_KEY","native-recovery-test");}let app=app_with(Store::memory);
    for path in ["native-merged-slip-fields","native-module-reviews","legacy-family-lineage","historical-variants","printed-cached-figures"] {
        for (method,suffix) in [("POST",""),("POST","/read"),("GET","/summary")] {
            assert_eq!(request(&app,method,&format!("{path}{suffix}"),json!({}),false).await.0,401,"unguarded {path}{suffix}");
        }
        let (status,r)=request(&app,"GET",&format!("{path}/summary"),Value::Null,true).await;assert_eq!(status,200,"{path}: {r}");assert_eq!(r["source_scope"],"dtmax-test-copy");
    }
    let (s,r)=request(&app,"POST","native-merged-slip-fields/read",json!({"test_copy":true,"source_view_numbers":[1]}),true).await;assert_eq!(s,200,"{r}");assert_eq!(r["records"],json!([null]));
    let (s,r)=request(&app,"POST","legacy-family-lineage/read",json!({"test_copy":true,"record_ids":["999:2024"]}),true).await;assert_eq!(s,200,"{r}");assert_eq!(r["records"],json!([null]));
    let (s,r)=request(&app,"POST","historical-variants/read",json!({"test_copy":true,"record_ids":[format!("100:2024:{}","a".repeat(64))]}),true).await;assert_eq!(s,200,"{r}");assert_eq!(r["records"],json!([null]));
    assert_eq!(request(&app,"POST","native-module-reviews",json!({"test_copy":false,"binding_profile_sha256":"d33fd86f7924d50e96f6ce4916ff14df8d4740a46863fb13cb1900842dd59604","records":[]}),true).await.0,409);
}
