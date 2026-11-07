use axum::{body::Body,http::Request};
use nxt_os::{app_with,source_import::{SourceImport,preserved_pdf_figures::{PreservedPdfFigureBatch,PreservedPdfFigureLookup},preserved_records::{SourceClientPage,SourceFactBatch,SourceFactPage}},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},path::Path};
use tower::ServiceExt;

const PROFILE:&str="4a22f166c06c7f22be26b4aba232a86260f5b22ad82fc5c1c59463c80e3a1d51";
const CACHE:&str="502e76e71cc132a5f74154c364ed398e0eae5cbb8c595a2caceff934b2ae0bf6";
fn digest(b:&[u8])->String{hex::encode(Sha256::digest(b))}
fn id(v:&Value)->String{format!("{}:{}:{}",v["client_number"].as_str().unwrap(),v["source_return_year"],v["pdf"]["sha256"].as_str().unwrap())}
fn batch(rows:Vec<Value>)->PreservedPdfFigureBatch{PreservedPdfFigureBatch{test_copy:true,preserved_figure_profile_sha256:PROFILE.into(),records:rows}}
fn lookup(rows:&[Value])->PreservedPdfFigureLookup{PreservedPdfFigureLookup{test_copy:true,record_ids:rows.iter().map(id).collect(),require_present:true}}
fn client_page(offset:usize)->SourceClientPage{SourceClientPage{offset,limit:100,search:String::new(),year:None,sort:None,direction:None,status:None,province:None,_routing_path:None}}
fn fact_page(y:Option<u16>)->SourceFactPage{SourceFactPage{kind:"preserved-pdf-figures".into(),offset:0,limit:25,source_return_year:y,_routing_path:None}}
fn rows(path:&str)->Vec<Value>{std::fs::read(path).unwrap().split(|b|*b==b'\n').filter(|s|!s.is_empty()).map(|s|serde_json::from_slice(s).unwrap()).collect()}
async fn controls(s:&SourceImport,c:&str)->Vec<Value>{vec![s.test_copy_summary(c).await.unwrap(),s.test_copy_source_fact_summary(c).await.unwrap(),s.test_copy_native_slip_summary(c).await.unwrap(),s.test_copy_native_supplement_summary(c).await.unwrap(),s.test_copy_legacy_owner_summary(c).await.unwrap(),s.test_copy_personal_source_summary(c).await.unwrap()]}
async fn roster(s:&SourceImport,c:&str)->Vec<Value>{let mut result=Vec::new();for offset in (0..5856).step_by(100){let p=s.test_copy_source_clients(c,client_page(offset)).await.unwrap();assert_eq!(p["total"],5856);result.extend(p["clients"].as_array().unwrap().iter().cloned());}result}
async fn seed_profile(store:&Store,c:&str,p:&Value){let n=p["client_number"].as_str().unwrap();store.set(&format!("dtmaxtc:{c}:client:{n}"),&serde_json::to_string(p).unwrap()).await.unwrap();store.sadd(&format!("dtmaxtc:{c}:clients"),n).await.unwrap();}
async fn seed_pdf(store:&Store,c:&str,row:&Value,manifest:&Value,folder:&Path)->(String,String,String,String){
    let p=&row["pdf"];let entry=manifest["records"].as_array().unwrap().iter().find(|v|v["PDF_sha256"]==p["sha256"]).unwrap();let file=folder.join(entry["source_pdf_file"].as_str().unwrap());let bytes=std::fs::read(&file).unwrap();assert!(digest(&bytes)==p["sha256"].as_str().unwrap(),"Private original PDF source hash differs");
    let rk=format!("dtmaxtc:{c}:return_pdf:record:{}:{}:{}",p["client_number"].as_str().unwrap(),p["tax_year"],p["sha256"].as_str().unwrap());let bk=format!("dtmaxtc:{c}:return_pdf:blob:{}",p["sha256"].as_str().unwrap());
    let raw=serde_json::to_string(&json!({"client_number":p["client_number"],"tax_year":p["tax_year"],"sha256":p["sha256"],"page_labels":p["page_labels"],"bytes":p["bytes"]})).unwrap();let hex=hex::encode(bytes);
    store.set(&rk,&raw).await.unwrap();store.set(&bk,&hex).await.unwrap();store.sadd(&format!("dtmaxtc:{c}:return_pdf:client:{}",p["client_number"].as_str().unwrap()),&id(row)).await.unwrap();(rk,raw,bk,hex)
}

#[tokio::test]
async fn new_preserved_figure_routes_keep_existing_firm_wide_authentication(){
    unsafe{std::env::set_var("ADMIN_API_KEY","synthetic-preserved-figure-test");}
    let app=app_with(Store::memory);
    for(method,path)in[("POST","preserved-pdf-figures"),("POST","preserved-pdf-figures/read"),("GET","preserved-pdf-figures/summary"),("GET","clients/100/source-facts?kind=preserved-pdf-figures&source_return_year=2010")]{
        let req=Request::builder().method(method).uri(format!("/api/source-imports/dtmax/companies/preserved-test/test-copy/{path}")).header("content-type","application/json").body(Body::from("{}" )).unwrap();assert_eq!(app.clone().oneshot(req).await.unwrap().status(),401);
    }
}

#[tokio::test]
async fn optional_actual_five_keep_native_cached_profiles_and_PDFs_and_add_only_four_qualified_years(){
    let Ok(input)=std::env::var("NXT_PRESERVED_PDF_OBSERVATIONS")else{return};let source=std::fs::read(&input).unwrap();assert_eq!(digest(&source),"3ae88f38ac5c79ecd24ab47b304161385ff355894ba963273cabcdbaf2546a4f");let observations=rows(&input);assert_eq!(observations.len(),5);
    let manifest_path=std::env::var("NXT_PRESERVED_PDF_MANIFEST").unwrap();let manifest_path=Path::new(&manifest_path);let manifest_raw=std::fs::read(manifest_path).unwrap();assert_eq!(digest(&manifest_raw),"89a9ded43780aaec4785181f081b8b7bd0b690f2714b386558dcdc0498699fa0");let manifest:Value=serde_json::from_slice(&manifest_raw).unwrap();
    let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_PRESERVED_PROFILE_FIXTURE").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);assert_eq!(digest(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
    let store=Store::memory();let service=SourceImport::new(store.clone());let company="preserved-five-private";let mut protected=Vec::new();
    for p in profiles{seed_profile(&store,company,p).await;let k=format!("dtmaxtc:{company}:client:{}",p["client_number"].as_str().unwrap());protected.push((k.clone(),store.get(&k).await.unwrap().unwrap()));}
    let native_path=std::env::var("NXT_PRESERVED_NATIVE_FIXTURE").unwrap();let native_raw=std::fs::read(&native_path).unwrap();assert_eq!(digest(&native_raw),"3e58031ae71b03a1378540e1d18be08e7fbe1286bd21ccdf173a16f14142374c");let native=rows(&native_path);assert_eq!(native.len(),32744);
    let target_years:BTreeSet<_>=observations.iter().map(|r|(r["client_number"].as_str().unwrap().to_string(),r["source_return_year"].as_u64().unwrap())).collect();assert_eq!(target_years.len(),4);
    for chunk in native.chunks(1000){let result=service.test_copy_historical_lines(company,SourceFactBatch{test_copy:true,records:chunk.to_vec()}).await.unwrap();assert!(result["rejected"].as_object().unwrap().is_empty(),"Pinned native baseline import rejected");}
    for r in &native{assert!(!target_years.contains(&(r["client_number"].as_str().unwrap().to_string(),r["tax_year"].as_u64().unwrap())));let k=format!("dtmaxtc:{company}:historical_lines:{}:{}:{}",r["client_number"].as_str().unwrap(),r["tax_year"],r["archive_sha256"].as_str().unwrap());protected.push((k.clone(),store.get(&k).await.unwrap().unwrap()));}
    let cache_path=std::env::var("NXT_PRESERVED_CACHED_FIXTURE").unwrap();let cache_raw=std::fs::read(&cache_path).unwrap();assert_eq!(digest(&cache_raw),"d7977b8d81a783a6a23b05dafddfe62daf50571d0200399d4daa4a520fc2702c");let cached=rows(&cache_path);assert_eq!(cached.len(),2527);
    for r in &cached{let key=format!("dtmaxtc:{company}:printed_cached_figures:{CACHE}:record:{}:{}:{}",r["client_number"].as_str().unwrap(),r["source_return_year"],r["pdf"]["sha256"].as_str().unwrap());let raw=serde_json::to_string(r).unwrap();let rid=id(r);store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:printed_cached_figures:{CACHE}:all"),&rid).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:printed_cached_figures:{CACHE}:digests"),&digest(raw.as_bytes())).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:printed_cached_figures:{CACHE}:client:{}",r["client_number"].as_str().unwrap()),&rid).await.unwrap();protected.push((key,raw));}
    let mut PDFs=Vec::new();for r in &observations{PDFs.push(seed_pdf(&store,company,r,&manifest,manifest_path.parent().unwrap()).await);}
    let before=controls(&service,company).await;let old_cache=service.test_copy_printed_cached_figure_summary(company).await.unwrap();assert_eq!(old_cache["PDF_observation_records"],2527);assert_eq!(old_cache["printed_amount_observations"],11734);
    let old_roster=roster(&service,company).await;assert!(old_roster.iter().all(|r|r.get("pdf_only_years").is_none()));
    let first=&observations[0];let all_ids:Vec<_>=observations.iter().map(id).collect();
    let mut bad=observations[1].clone();bad["observations"][0]["printed_cents"]=json!("999999999");assert!(service.test_copy_preserved_pdf_figures(company,batch(vec![first.clone(),bad])).await.is_err());assert_eq!(service.test_copy_preserved_pdf_figure_summary(company).await.unwrap()["PDF_observation_records"],0);
    // Actual PDF owner and bytes are checked before any observation write.
    for kind in ["metadata","blob"]{let(rk,raw,bk,hex)=&PDFs[0];if kind=="metadata"{let mut v:Value=serde_json::from_str(raw).unwrap();v["client_number"]=json!("999999999");store.set(rk,&v.to_string()).await.unwrap();}else{store.set(bk,"00").await.unwrap();}assert!(service.test_copy_preserved_pdf_figures(company,batch(vec![first.clone()])).await.is_err());store.set(rk,raw).await.unwrap();store.set(bk,hex).await.unwrap();}
    let imported=service.test_copy_preserved_pdf_figures(company,batch(observations.clone())).await.unwrap();assert_eq!(imported["created"],5);assert_eq!(imported["submitted"],5);assert_eq!(imported["unchanged_replays"],0);assert_eq!(imported["source_scope"],company);
    let replay=service.test_copy_preserved_pdf_figures(company,batch(observations.clone())).await.unwrap();assert_eq!(replay["created"],0);assert_eq!(replay["unchanged_replays"],5);
    let read=service.test_copy_read_preserved_pdf_figures(company,lookup(&observations)).await.unwrap();assert!(read["records"]==json!(observations),"Actual preserved observations changed during exact readback");assert!(read["record_ids"]==json!(all_ids));
    for kind in ["owner","year","family","cached","amount","blank_zero","bbox","approved","profile"]{
        let mut v=first.clone();match kind{"owner"=>v["client_number"]=json!("999999999"),"year"=>v["source_return_year"]=json!(2024),"family"=>v["family_file_id"]=json!("999"),"cached"=>v["pdf"]["export_provenance"]=json!({"kind":"cached_native_export"}),"amount"=>v["observations"][0]["printed_cents"]=json!("999999999"),"blank_zero"=>{let o=v["observations"].as_array_mut().unwrap().iter_mut().find(|o|o["state"]=="identified_printed_blank").unwrap();o["printed_cents"]=json!("0");},"bbox"=>{let o=&mut v["observations"][0];let mut c:Value=serde_json::from_str(o["printed_candidates_json"].as_str().unwrap()).unwrap();c[0]["line_code_bbox"]=json!([10,10,10,20]);let raw=c.to_string();o["printed_candidates_sha256"]=json!(digest(raw.as_bytes()));o["printed_candidates_json"]=json!(raw);},"approved"=>v["financial_values_approved"]=json!(true),_=>v["preserved_figure_profile_sha256"]=json!(CACHE)}assert!(service.test_copy_preserved_pdf_figures(company,batch(vec![v])).await.is_err(),"Unclosed preserved observation accepted");}
    let after_roster=roster(&service,company).await;let expected:BTreeMap<String,BTreeSet<u16>>=target_years.iter().fold(BTreeMap::new(),|mut m,(c,y)|{m.entry(c.clone()).or_default().insert(*y as u16);m});let mut added=0;
    for(old,new)in old_roster.iter().zip(&after_roster){let client=old["client_number"].as_str().unwrap();assert!(old["client_number"]==new["client_number"]);if let Some(years)=expected.get(client){assert!(new["native_historical_years"]==old["historical_years"]);assert!(new["preserved_pdf_figure_years"]==json!(years)&&new["pdf_only_years"]==json!(years));let mut same=new.clone();for k in ["native_historical_years","preserved_pdf_figure_years","pdf_only_years"]{same.as_object_mut().unwrap().remove(k);}same["historical_years"]=old["historical_years"].clone();assert!(same==*old,"Unrelated roster fields changed");let n:BTreeSet<_>=new["historical_years"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();let o:BTreeSet<_>=old["historical_years"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();assert!(n.difference(&o).copied().collect::<BTreeSet<_>>()==years.iter().map(|y|*y as u64).collect());added+=n.len()-o.len();}else{assert!(old==new,"An unrelated roster row changed");}}
    assert_eq!(added,4);
    for(client,y)in &target_years{let p=service.test_copy_client_source_facts(company,client,fact_page(Some(*y as u16))).await.unwrap();assert!(p["records"].as_array().unwrap().iter().all(|r|r["source_return_year"]==*y));let count=observations.iter().filter(|r|r["client_number"]==*client&&r["source_return_year"]==*y).count();assert_eq!(p["total"],count);}
    let empty=service.test_copy_client_source_facts(company,first["client_number"].as_str().unwrap(),fact_page(Some(2026))).await.unwrap();assert_eq!(empty["total"],0);assert_eq!(empty["source_return_year"],2026);
    assert!(controls(&service,company).await==before,"Original six control responses changed");assert!(service.test_copy_printed_cached_figure_summary(company).await.unwrap()==old_cache,"Original2527 cached summary changed");
    for(key,raw)in &protected{assert!(store.get(key).await.unwrap().as_deref()==Some(raw.as_str()),"Original profile, native history or cached observation body changed");}
    for(rk,raw,bk,hex)in &PDFs{assert!(store.get(rk).await.unwrap().as_deref()==Some(raw)&&store.get(bk).await.unwrap().as_deref()==Some(hex),"Existing PDF metadata or bytes changed");}
    let summary=service.test_copy_preserved_pdf_figure_summary(company).await.unwrap();assert_eq!(summary["PDF_observation_records"],5);assert_eq!(summary["owner_years"],4);assert_eq!(summary["printed_amount_observations"],25);assert_eq!(summary["identified_printed_blanks"],5);
    // Later legitimate native recovery must not invalidate import-time paper evidence.
    let future=json!({"client_number":first["client_number"],"family_file_id":"999999","tax_year":first["source_return_year"],"archive_sha256":"f".repeat(64),"vendor_module":"T1","mapping_state":"pending_year_verification","values_cents":{"15000":null,"23600":null,"26000":null,"42000":null,"48400":null,"48500":null}});let result=service.test_copy_historical_lines(company,SourceFactBatch{test_copy:true,records:vec![future]}).await.unwrap();assert!(result["rejected"].as_object().unwrap().is_empty());
    assert!(service.test_copy_read_preserved_pdf_figures(company,lookup(&observations)).await.unwrap()["records"]==json!(observations));let target=service.test_copy_source_clients(company,SourceClientPage{search:first["client_number"].as_str().unwrap().into(),..client_page(0)}).await.unwrap();let row=target["clients"].as_array().unwrap().iter().find(|r|r["client_number"]==first["client_number"]).unwrap();assert!(row["native_historical_years"].as_array().unwrap().contains(&first["source_return_year"]));assert!(!row["pdf_only_years"].as_array().unwrap().contains(&first["source_return_year"]));
    println!("PRIVATE_PRESERVED_PDF_EXACT: records=5; slots=30; native_bodies=32744; cached_bodies=2527; profiles=5856; derived_owner_year_additions=4; PDF_metadata_and_blobs_unchanged=true; six_controls_unchanged=true; future_native_recovery_preserves_observations=true");
}

#[tokio::test]
async fn optional_interrupted_exact_record_writes_replay_repair_all_three_indexes(){
    let Ok(input)=std::env::var("NXT_PRESERVED_PDF_OBSERVATIONS")else{return};let rows=rows(&input);let row=rows[0].clone();let path=std::env::var("NXT_PRESERVED_PDF_MANIFEST").unwrap();let path=Path::new(&path);let manifest:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for phase in 0..3 {
        let c=format!("paper-repair-{phase}");let store=Store::memory();let s=SourceImport::new(store.clone());seed_profile(&store,&c,&json!({"client_number":row["client_number"],"name":"Synthetic repair owner"})).await;let pdf=seed_pdf(&store,&c,&row,&manifest,path.parent().unwrap()).await;let before=controls(&s,&c).await;
        let raw=serde_json::to_string(&row).unwrap();let rid=id(&row);let p=format!("dtmaxtc:{c}:preserved_pdf_figures:{PROFILE}");store.set_nx(&format!("{p}:record:{rid}"),&raw).await.unwrap();if phase>=1{store.sadd(&format!("{p}:all"),&rid).await.unwrap();}if phase>=2{store.sadd(&format!("{p}:digests"),&digest(raw.as_bytes())).await.unwrap();}
        assert!(s.test_copy_read_preserved_pdf_figures(&c,lookup(std::slice::from_ref(&row))).await.unwrap()["records"]==json!([row]));
        let r=s.test_copy_preserved_pdf_figures(&c,batch(vec![row.clone()])).await.unwrap();assert_eq!(r["created"],0);assert_eq!(r["unchanged_replays"],1);assert_eq!(s.test_copy_preserved_pdf_figure_summary(&c).await.unwrap()["PDF_observation_records"],1);assert_eq!(s.test_copy_client_source_facts(&c,row["client_number"].as_str().unwrap(),fact_page(None)).await.unwrap()["total"],1);
        assert!(controls(&s,&c).await==before);assert!(store.get(&pdf.0).await.unwrap().as_deref()==Some(&pdf.1)&&store.get(&pdf.2).await.unwrap().as_deref()==Some(&pdf.3));
    }
    println!("PRIVATE_PRESERVED_PDF_REPLAY_REPAIR: interrupted_after_record_before_global_digest_client_indexes=3; direct_read_existence_does_not_replace_matching_POST=true");
}
