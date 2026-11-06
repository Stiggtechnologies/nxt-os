use axum::{Router,body::Body,http::Request};
use nxt_os::{app_with,store::Store};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const BASE: &str="/api/source-imports/dtmax/companies/source-fact-test/test-copy";
fn app() -> Router {
    unsafe { std::env::set_var("ADMIN_API_KEY","source-fact-local-test"); }
    app_with(Store::memory)
}
#[tokio::test]
async fn source_fact_routes_reject_unauthenticated_reads_and_writes() {
    let app=app();
    for (method,path) in [("POST","family-files"),("POST","historical-lines"),("POST","afr-slips"),("GET","source-fact-summary"),("GET","clients/100/source-facts?kind=historical-lines")] {
        let req=Request::builder().method(method).uri(format!("{BASE}/{path}"))
            .header("content-type","application/json").body(Body::from("{\"test_copy\":true,\"records\":[]}")).unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(),401);
    }
}
#[tokio::test]
async fn conflicting_concurrent_source_facts_cannot_overwrite_the_winner() {
    let app=app();let original=lines(2025);let mut changed=original.clone();changed["values_cents"]["15000"]=json!("1");
    let (a,b)=tokio::join!(
        request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[original]})),
        request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[changed]})));
    assert_eq!(a.0,200);assert_eq!(b.0,200);
    assert_eq!(a.1["created"].as_u64().unwrap()+b.1["created"].as_u64().unwrap(),1);
    assert_eq!(a.1["rejected"]["conflicting_historical_lines"].as_u64().unwrap_or(0)+b.1["rejected"]["conflicting_historical_lines"].as_u64().unwrap_or(0),1);
    let (_,page)=request(&app,"GET","clients/100/source-facts?kind=historical-lines",Value::Null).await;
    assert_eq!(page["total"],1);
    assert_eq!(page["records"][0]["values_cents"]["15000"],if a.1["created"]==1 {"123456"} else {"1"});
}
async fn request(app: &Router, method: &str, path: &str, body: Value) -> (u16,Value) {
    let request=Request::builder().method(method).uri(format!("{BASE}/{path}"))
        .header("authorization","Bearer source-fact-local-test").header("content-type","application/json")
        .body(if method=="GET" {Body::empty()} else {Body::from(body.to_string())}).unwrap();
    let response=app.clone().oneshot(request).await.unwrap();let status=response.status().as_u16();
    (status,serde_json::from_slice(&axum::body::to_bytes(response.into_body(),usize::MAX).await.unwrap()).unwrap())
}
fn lines(year: u16) -> Value {
    json!({"client_number":"100","family_file_id":"999","tax_year":year,"archive_sha256":"a".repeat(64),"vendor_module":"T1", "mapping_state":if (2015..=2025).contains(&year){"vendor_static_verified"}else{"pending_year_verification"},
        "values_cents":{"15000":"123456","23600":"0","26000":null,"42000":null,"48400":"2500","48500":null}})
}
fn slip(kind: &str, year: Option<u16>, occurrence: u32) -> Value {
    let children=year.map(|y|vec![json!({"tag":"{http://www.cra-arc.gc.ca/xmlns/tdd/2-10-1}TaxYear","attributes":{},"text":y.to_string(),"children":[]})]).unwrap_or_default();
    let tree=json!({"tag":format!("{{http://www.cra-arc.gc.ca/xmlns/tdd/2-10-1}}{kind}"),"attributes":{},"text":null,"children":children});
    let hash=hex::encode(Sha256::digest(serde_json::to_vec(&tree).unwrap()));
    json!({"client_number":"100","tax_year":year,"source_folder_year":2025,"slip_kind":kind,"archive_sha256":"a".repeat(64),"source_frame_sha256":"b".repeat(64),"source_frame_offset":56,"source_occurrence":occurrence,"slip_content_sha256":hash,"vendor_field_tree":tree})
}
#[tokio::test]
async fn preserves_family_membership_nulls_and_source_occurrences_and_replays() {
    let app=app();
    assert_eq!(request(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"100","name":"Synthetic principal"},{"client_number":"200","name":"Synthetic partner"}]})).await.0,200);
    let family=json!({"family_file_id":"999","tax_year":2025,"source_file_sha256":"c".repeat(64),"archive_sha256":"a".repeat(64),"member_client_numbers":["100","200"]});
    let (status,result)=request(&app,"POST","family-files",json!({"test_copy":true,"records":[family.clone()]})).await;
    assert_eq!(status,200);assert_eq!(result["created"],1);assert_eq!(result["member_links_created"],2);
    let historical=vec![lines(2025),lines(2013)];
    let afr=vec![slip("T4Slip",Some(2024),1),slip("T4Slip",Some(2024),2),slip("T5013Slip",None,3)];
    for (path,records) in [("historical-lines",historical),("afr-slips",afr),("family-files",vec![family])] {
        if path!="family-files" { let (_,r)=request(&app,"POST",path,json!({"test_copy":true,"records":records})).await;assert!(r["rejected"].as_object().unwrap().is_empty()); }
        let (_,replay)=request(&app,"POST",path,json!({"test_copy":true,"records":records})).await;
        assert_eq!(replay["unchanged_replays"],records.len());
    }
    let (_,summary)=request(&app,"GET","source-fact-summary",Value::Null).await;
    assert_eq!(summary["counts"],json!({"family_files":1,"family_member_links":2,"historical_lines":2,"historical_lines_pending":1,"afr_occurrences":3,"afr_contents":2,"afr_missing_year":1}));
    let (_,page)=request(&app,"GET","clients/100/source-facts?kind=historical-lines",Value::Null).await;
    assert_eq!(page["records"][0]["values_cents"]["23600"],"0");assert!(page["records"][0]["values_cents"]["26000"].is_null());
    let (_,page)=request(&app,"GET","clients/200/source-facts?kind=family-files",Value::Null).await;
    assert_eq!(page["records"][0]["family_file_id"],"999");
    let (_,old)=request(&app,"GET","summary",Value::Null).await;assert_eq!(old["clients"],2);assert_eq!(old["returns"],0);
}
#[tokio::test]
async fn refuses_false_mapping_guesses_unknown_fields_bad_hashes_and_conflicting_replays() {
    let app=app();
    assert_eq!(request(&app,"POST","historical-lines",json!({"test_copy":false,"records":[lines(2025)]})).await.0,409);
    let mut bad_mapping=lines(2013);bad_mapping["mapping_state"]=json!("vendor_static_verified");
    let mut missing_state=lines(2025);missing_state["values_cents"].as_object_mut().unwrap().remove("26000");
    let mut decimal_cents=lines(2025);decimal_cents["values_cents"]["15000"]=json!("123.45");
    let mut sin=lines(2025);sin["sin"]=json!("000000000");
    let (_,result)=request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[bad_mapping,missing_state,decimal_cents,sin]})).await;
    assert_eq!(result["created"],0);assert_eq!(result["rejected"].as_object().unwrap().values().map(|v|v.as_u64().unwrap()).sum::<u64>(),4);
    let mut guessed_year=slip("T5013Slip",None,1);guessed_year["tax_year"]=json!(2025);
    let mut bad_hash=slip("T4Slip",Some(2025),2);bad_hash["slip_content_sha256"]=json!("0".repeat(64));
    let mut unknown=slip("T4Slip",Some(2025),3);unknown["vendor_field_tree"]["children"].as_array_mut().unwrap().push(json!({"tag":"SIN","attributes":{},"text":"000000000","children":[]}));
    let (_,result)=request(&app,"POST","afr-slips",json!({"test_copy":true,"records":[guessed_year,bad_hash,unknown]})).await;
    assert_eq!(result["created"],0);assert_eq!(result["rejected"].as_object().unwrap().values().map(|v|v.as_u64().unwrap()).sum::<u64>(),3);
    let original=lines(2025);let (_,r)=request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[original.clone()]})).await;assert_eq!(r["created"],1);
    let mut changed=original;changed["values_cents"]["15000"]=json!("1");
    let (_,r)=request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[changed]})).await;assert_eq!(r["rejected"]["conflicting_historical_lines"],1);
}
#[tokio::test]
async fn optional_private_source_package_loads_and_replays_with_exact_controls() {
    let Ok(root)=std::env::var("NXT_DTMAX_SOURCE_FACT_STAGING") else { return };
    let root=std::path::Path::new(&root);let manifest:Value=serde_json::from_slice(&std::fs::read(root.join("staging-summary.json")).unwrap()).unwrap();
    let app=app();let mut accepted=0;
    for replay in [false,true] {
        for batch in manifest["batches"].as_array().unwrap() {
            let bytes=std::fs::read(root.join(batch["file"].as_str().unwrap())).unwrap();
            assert_eq!(hex::encode(Sha256::digest(&bytes)),batch["sha256"].as_str().unwrap());
            let body:Value=serde_json::from_slice(&bytes).unwrap();
            let (status,result)=request(&app,"POST",batch["kind"].as_str().unwrap(),body).await;
            assert_eq!(status,200,"source batch route failed");assert!(result["rejected"].as_object().unwrap().is_empty(),"source rejection counts: {}",result["rejected"]);
            if replay { assert_eq!(result["unchanged_replays"].as_u64().or(result["unchanged"].as_u64()).unwrap(),batch["rows"].as_u64().unwrap()); }
            else {accepted+=batch["rows"].as_u64().unwrap();}
        }
    }
    let (_,summary)=request(&app,"GET","source-fact-summary",Value::Null).await;
    for (actual,expected) in [("family_files","family-files"),("family_member_links","family_member_links"),("historical_lines","historical-lines"),("historical_lines_pending","historical_records_pending_mapping"),("afr_occurrences","afr-slips"),("afr_contents","afr_distinct_client_contents"),("afr_missing_year","afr_missing_explicit_year")] {
        assert_eq!(summary["counts"][actual],manifest["counts"][expected],"source control failed: {actual}");
    }
    assert_eq!(summary["tax_cases_imported"],0);
    println!("SOURCE_FACT_ROWS_ACCEPTED={accepted}; SOURCE_FACT_CONTROLS={summary}");
}
