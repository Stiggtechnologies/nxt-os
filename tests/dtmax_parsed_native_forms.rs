use axum::{body::Body, http::Request};
use nxt_os::{app_with, source_import::parsed_native_forms::PROFILE, store::Store};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn parsed_source_routes_require_admin_and_refuse_unclosed_input() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "parsed-local-test"); }
    let app=app_with(Store::memory);
    let base="/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/";
    for(method,path)in[("POST","parsed-native-forms"),("POST","parsed-native-forms/read"),("GET","parsed-native-forms/summary"),("GET","clients/100/source-facts?kind=parsed-native-forms&source_return_year=2024")]{
        let response=app.clone().oneshot(Request::builder().method(method).uri(format!("{base}{path}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),401);
    }
    let response=app.clone().oneshot(Request::builder().method("POST").uri(format!("{base}parsed-native-forms")).header("authorization","Bearer parsed-local-test").header("content-type","application/json").body(Body::from(json!({"test_copy":true,"profile_sha256":PROFILE,"records":[]}).to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),400);
    let response=app.oneshot(Request::builder().method("POST").uri(format!("{base}parsed-native-forms/read")).header("authorization","Bearer parsed-local-test").header("content-type","application/json").body(Body::from(json!({"test_copy":false,"profile_sha256":PROFILE,"record_ids":["a".repeat(64)]}).to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),409);
}
