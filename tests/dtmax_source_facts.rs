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
    for (method,path) in [("POST","family-files"),("POST","historical-lines"),("POST","afr-slips"),("GET","source-fact-summary"),("GET","source-clients"),("GET","clients/100/source-facts?kind=historical-lines")] {
        let req=Request::builder().method(method).uri(format!("{BASE}/{path}"))
            .header("content-type","application/json").body(Body::from("{\"test_copy\":true,\"records\":[]}")).unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(),401);
    }
}
#[tokio::test]
async fn t1_source_worklist_uses_individual_years_and_search_without_creating_cases() {
    let app=app();
    request(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"100","name":"Synthetic Person"},{"client_number":"999","name":"Separate Family Stem"}]})).await;
    request(&app,"POST","historical-lines",json!({"test_copy":true,"records":[lines(2025),lines(2013)]})).await;
    let (status,page)=request(&app,"GET","source-clients?limit=100&year=2013&search=person",Value::Null).await;
    assert_eq!(status,200);assert_eq!(page["source_scope"],"source-fact-test");assert_eq!(page["program"],"T1");assert_eq!(page["total"],1);assert_eq!(page["tax_cases_created"],0);
    assert_eq!(page["clients"][0]["client_number"],"100");assert_eq!(page["clients"][0]["historical_years"],json!([2013,2025]));assert_eq!(page["clients"][0]["pending_years"],json!([]));assert_eq!(page["clients"][0]["pending_years_at_import"],json!([2013]));
    let (status,hosted)=request(&app,"GET","source-clients?limit=100&year=2013&search=person&path=%2Fapi%2Fignored-rewrite",Value::Null).await;assert_eq!(status,200);assert_eq!(hosted,page);
    let (status,hosted)=request(&app,"GET","clients/100/source-facts?kind=historical-lines&offset=0&limit=100&path=%2Fapi%2Fother-company",Value::Null).await;assert_eq!(status,200);assert_eq!(hosted["client_number"],"100");assert_eq!(hosted["records"].as_array().unwrap().len(),2);
    assert_eq!(request(&app,"GET","clients/100/source-facts?kind=historical-lines&unexpected=1",Value::Null).await.0,400);
    assert!(page["clients"][0].get("email").is_none());
    let (_,page)=request(&app,"GET","source-clients?limit=1&offset=1",Value::Null).await;assert_eq!(page["total"],2);assert_eq!(page["clients"][0]["client_number"],"999");assert_eq!(page["clients"][0]["historical_years"],json!([]));
    assert_eq!(request(&app,"GET","source-clients?limit=101",Value::Null).await.0,400);
}
#[tokio::test]
async fn source_client_filters_and_sort_apply_before_pagination() {
    let app=app();
    request(&app,"POST","clients",json!({"test_copy":true,"clients":[
        {"client_number":"10","name":"Zulu Synthetic","province":"AB","processing_status":"Ready to print"},
        {"client_number":"2","name":"Alpha Synthetic","province":"BC","processing_status":"In progress"},
        {"client_number":"30","name":"Beta Synthetic","province":"AB","processing_status":"Ready to print"}]})).await;
    let (status,p)=request(&app,"GET","source-clients?sort=name&limit=1&offset=1",Value::Null).await;
    assert_eq!(status,200);assert_eq!(p["total"],3);assert_eq!(p["clients"][0]["client_number"],"30");
    let (_,p)=request(&app,"GET","source-clients?sort=name&direction=desc&province=AB&status=Ready%20to%20print&limit=1",Value::Null).await;
    assert_eq!(p["total"],2);assert_eq!(p["clients"][0]["client_number"],"10");assert_eq!(p["available_provinces"],json!(["AB","BC"]));
    assert_eq!(p["clients"][0]["source_profile"]["processing_status"],"Ready to print");
    assert_eq!(request(&app,"GET","source-clients?sort=sin",Value::Null).await.0,400);
    assert_eq!(request(&app,"GET","source-clients?direction=random",Value::Null).await.0,400);
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
    let bytes=axum::body::to_bytes(response.into_body(),usize::MAX).await.unwrap();
    (status,serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"error":String::from_utf8_lossy(&bytes)})))
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
async fn original_year_form_review_never_changes_amounts_or_upgrades_other_modules_and_years() {
    let app=app();let mut records=vec![lines(2005),lines(2013),lines(2014),lines(2026)];let mut trust=lines(2014);trust["vendor_module"]=json!("T3");trust["mapping_state"]=json!("pending_module_verification");trust["archive_sha256"]=json!("b".repeat(64));records.push(trust);
    request(&app,"POST","clients",json!({"test_copy":true,"clients":[{"client_number":"100","name":"Synthetic Person"}]})).await;
    request(&app,"POST","historical-lines",json!({"test_copy":true,"records":records})).await;
    let (_,summary)=request(&app,"GET","source-fact-summary",Value::Null).await;assert_eq!(summary["counts"]["historical_lines_pending"],5);assert_eq!(summary["historical_mapping_review"]["original_form_bindings_checked"],3);assert_eq!(summary["historical_mapping_review"]["remaining_pending"],2);
    let (_,page)=request(&app,"GET","source-clients?limit=100",Value::Null).await;assert_eq!(page["clients"][0]["pending_years"],json!([2005,2014]));assert_eq!(page["clients"][0]["pending_years_at_import"],json!([2005,2013,2014,2026]));
    let (_,page)=request(&app,"GET","clients/100/source-facts?kind=historical-lines",Value::Null).await;
    for r in page["records"].as_array().unwrap() {
        let original=records.iter().find(|v|v["tax_year"]==r["tax_year"]&&v["archive_sha256"]==r["archive_sha256"]).unwrap();assert_eq!(r["values_cents"],original["values_cents"]);assert_eq!(r["mapping_state"],original["mapping_state"]);
        if r["vendor_module"]=="T1" && [json!(2013),json!(2014),json!(2026)].contains(&r["tax_year"]) {assert_eq!(r["year_mapping_review"]["state"],"original_form_bindings_checked");assert_eq!(r["year_mapping_review"]["field_ids"]["15000"],150);assert_eq!(r["year_mapping_review"]["tax_figures_approved"],false);} else {assert!(r.get("year_mapping_review").is_none());}
    }
    let (_,replay)=request(&app,"POST","historical-lines",json!({"test_copy":true,"records":records})).await;assert_eq!(replay["unchanged_replays"],5);
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
#[tokio::test]
async fn optional_private_pending_rows_keep_every_stored_byte_during_binding_review() {
    let Ok(path)=std::env::var("NXT_DTMAX_PENDING_MAPPING_INPUTS")else{return};
    let rows:Vec<Value>=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(rows.len(),2663);
    let store=Store::memory();let service=nxt_os::source_import::SourceImport::new(store.clone());let company="private-binding-review";
    for chunk in rows.chunks(1000) {let response=service.test_copy_historical_lines(company,nxt_os::source_import::preserved_records::SourceFactBatch{test_copy:true,records:chunk.to_vec()}).await.unwrap();assert!(response["rejected"].as_object().unwrap().is_empty());}
    let mut originals=Vec::new();
    for r in &rows {let key=format!("dtmaxtc:{company}:historical_lines:{}:{}:{}",r["client_number"].as_str().unwrap(),r["tax_year"].as_u64().unwrap(),r["archive_sha256"].as_str().unwrap());let raw=store.get(&key).await.unwrap().unwrap();originals.push((key,raw));}
    let summary=service.test_copy_source_fact_summary(company).await.unwrap();assert_eq!(summary["counts"]["historical_lines_pending"],2663);assert_eq!(summary["historical_mapping_review"]["original_form_bindings_checked"],2573);assert_eq!(summary["historical_mapping_review"]["remaining_pending"],90);assert_eq!(summary["tax_cases_imported"],0);
    for (key,raw) in originals {assert_eq!(store.get(&key).await.unwrap().as_deref(),Some(raw.as_str()));}
    println!("PRIVATE_PENDING_ROWS_REVIEWED=2663; ORIGINAL_FORM_BINDINGS_CHECKED=2573; OTHER_PENDING=90; STORED_BYTES_CHANGED=0");
}
