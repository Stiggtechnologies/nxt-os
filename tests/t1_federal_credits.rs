//! Synthetic CRA-form arithmetic and normal save/calculate/report/package contracts.
use nxt_os::{store::Store,tax::{Tax,calculations,models::{ReturnInput,RevisionInput},reports::{self,ReportRequest}}};
use serde_json::{json,Value};
use std::collections::BTreeSet;

fn examples()->Value{serde_json::from_str(include_str!("fixtures/t1_2025_federal_credits.json")).unwrap()}
fn amount(c:&calculations::Calculation,code:&str)->i64{c.lines.iter().find(|l|l.code==code).unwrap_or_else(||panic!("missing {code}")).amount}
#[test]
fn independent_decimal_examples_reconcile_all_2025_federal_paths(){
 for province in ["AB","ON","BC","SK","MB","NB","NS","PE","NL","YT","NT","NU"]{
  for case in examples()["cases"].as_array().unwrap(){
   let result=calculations::calculate("T1",2025,province,&case["inputs"]).unwrap();
   for(code,expected)in case["expected"].as_object().unwrap(){assert_eq!(amount(&result,code),expected.as_i64().unwrap(),"{province}/{} / {code}",case["name"]);}
   let codes: BTreeSet<_>=result.lines.iter().map(|l|l.code.as_str()).collect();assert_eq!(codes.len(),result.lines.len(),"duplicate output codes");
   assert_eq!(amount(&result,"35000"),amount(&result,"33800")+amount(&result,"34900")+amount(&result,"34990"));
   assert_eq!(result.totals["federal_tax"],amount(&result,"42000"));
   assert!(!result.diagnostics.iter().any(|d|d.code=="top_up_tax_credit_not_implemented"));
  }
 }
}
#[test]
fn dividend_credit_stays_outside_line_35000_and_is_subtracted_once(){
 let result=calculations::calculate("T1",2025,"AB",&json!({"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":20_000_000,"eligible_dividends_cents":100_000,"other_dividends_cents":50_000,"dividends_verified":true,"tuition_cents":4_000_000,"credits_verified":true})).unwrap();
 assert!(amount(&result,"40425")>0);
 assert_eq!(amount(&result,"35000"),amount(&result,"33800")+amount(&result,"34900")+amount(&result,"34990"));
 assert_eq!(amount(&result,"42000"),(amount(&result,"federal_before_credits")-amount(&result,"35000")-amount(&result,"40425")).max(0));
}
#[test]
fn first_200_gift_component_only_drives_the_top_up(){
 let case=examples()["cases"].as_array().unwrap().iter().find(|c|c["name"]=="large_donation_not_all_lowest_rate").unwrap().clone();
 let result=calculations::calculate("T1",2025,"AB",&case["inputs"]).unwrap();
 assert!(amount(&result,"34900")>800_000);assert_eq!(amount(&result,"34990"),0);
 assert_eq!(amount(&result,"42000"),case["expected"]["42000"].as_i64().unwrap());
}
#[test]
fn top_up_is_non_refundable_and_does_not_remove_other_scope_guards(){
 let mut inputs=examples()["cases"].as_array().unwrap().iter().find(|c|c["name"]=="combined_claim").unwrap()["inputs"].clone();
 let result=calculations::calculate("T1",2025,"AB",&inputs).unwrap();assert_eq!(amount(&result,"34990"),113);assert_eq!(amount(&result,"42000"),0);
 inputs["self_employment_gross_cents"]=json!(100);inputs["t2125_verified"]=json!(true);inputs["se_cpp_verified"]=json!(true);
 let held=calculations::calculate("T1",2025,"AB",&inputs).unwrap();assert!(!held.supported);assert!(held.diagnostics.iter().any(|d|d.code=="self_employed_cpp_payable_not_implemented"&&d.blocks_review));
 assert!(!calculations::supports_scope("T1",2025,"QC"));
}
#[test]
fn negative_claims_and_excess_ordinary_donations_are_still_rejected(){
 let base=json!({"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000,"credits_verified":true});
 for key in ["tuition_cents","medical_expenses_cents","donations_cents"]{let mut v=base.clone();v[key]=json!(-1);assert!(calculations::calculate("T1",2025,"AB",&v).is_err());}
 let mut v=base;v["donations_cents"]=json!(3_750_001);assert!(calculations::calculate("T1",2025,"AB",&v).unwrap_err().contains("75%"));
}
#[tokio::test]
async fn normal_calculation_reports_and_packages_preserve_exact_credit_lines(){
 let store=Store::memory();let t=Tax::new(store.clone());let case=examples()["cases"].as_array().unwrap().iter().find(|c|c["name"]=="positive_tax_after_top_up").unwrap().clone();
 let input:ReturnInput=serde_json::from_value(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic federal credit workpaper","inputs":case["inputs"]})).unwrap();
 let r=t.save(None,input).await.unwrap_or_else(|e|panic!("save: {}",e.1));let r=t.calculate(&r.id,RevisionInput{expected_revision:r.revision,idempotency_key:None}).await.unwrap_or_else(|e|panic!("calculate: {}",e.1));
 let req:ReportRequest=serde_json::from_value(json!({"program":"T1","years":[2025],"columns":["name","line:33500","line:33800","line:34900","line:34990","line:35000","line:40425","line:42000"]})).unwrap();
 let report=t.report(req,None).await.unwrap_or_else(|e|panic!("report: {}",e.1));assert_eq!(report["rows"][0]["calculation_state"],"current_supported");
 for code in ["33500","33800","34900","34990","35000","42000"]{assert_eq!(report["rows"][0][format!("line:{code}")],case["expected"][code]);}
 let package=t.package(&r.id).await.unwrap_or_else(|e|panic!("package: {}",e.1));assert_eq!(package["calculation_snapshot"],serde_json::to_value(&r.calculation).unwrap());assert_eq!(package["filing_enabled"],false);
 // Exercise actual qualified detail and printable HTTP output on the same synthetic store.
 // This is the only authentication-environment user in this test binary.
 unsafe{std::env::set_var("ADMIN_API_KEY","synthetic-credit-test-key")};
 let router=nxt_os::app_with(||store.clone());
 use tower::ServiceExt;
 let get=|suffix:&str|axum::http::Request::builder().uri(format!("/api/tax/returns/{}{suffix}",r.id)).header("authorization","Bearer synthetic-credit-test-key").body(axum::body::Body::empty()).unwrap();
 let response=router.clone().oneshot(get("")).await.unwrap();assert_eq!(response.status(),200);
 let record:Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),4*1024*1024).await.unwrap()).unwrap();assert_eq!(record["current_ruleset"],calculations::RULESET);assert_eq!(record["current_calculation_ruleset_current"],true);
 let response=router.oneshot(get("/print")).await.unwrap();assert_eq!(response.status(),200);assert!(response.headers()["cache-control"].to_str().unwrap().split(',').any(|v|v.trim()=="no-store"));
 let printed=axum::body::to_bytes(response.into_body(),4*1024*1024).await.unwrap();let text=std::str::from_utf8(&printed).unwrap();assert!(text.contains("Internal workpaper. Not a certified tax return or filing."));assert!(text.contains("34990</td>")&&text.contains("298.94"));assert!(text.contains("35000</td>")&&text.contains("17283.39"));
 if let Some(dir)=std::env::var_os("NXT_FEDERAL_CREDIT_FIXTURE_DIR"){
  let dir=std::path::PathBuf::from(dir);std::fs::create_dir_all(&dir).unwrap();
  std::fs::write(dir.join("print.html"),&printed).unwrap();
  for(name,v)in [("record",record),("catalog",nxt_os::tax::catalog::catalog()),("report",report),("report_catalog",reports::catalog()),("package",package)]{std::fs::write(dir.join(format!("{name}.json")),serde_json::to_vec_pretty(&v).unwrap()).unwrap();}
 }
}
