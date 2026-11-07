use nxt_os::{source_import::{SourceImport,return_pdfs::ReturnPdfInput},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn canonical(v:&Value)->Vec<u8>{serde_json::to_vec(v).unwrap()}
async fn controls(s:&SourceImport,c:&str)->Vec<Value>{vec![s.test_copy_summary(c).await.unwrap(),s.test_copy_source_fact_summary(c).await.unwrap(),s.test_copy_native_slip_summary(c).await.unwrap(),s.test_copy_native_supplement_summary(c).await.unwrap(),s.test_copy_legacy_owner_summary(c).await.unwrap(),s.test_copy_personal_source_summary(c).await.unwrap()]}
#[tokio::test]
async fn optional_actual_closed_office_documents_keep_all_legacy_metadata_profiles_and_controls(){
 let Ok(packet)=std::env::var("NXT_DTMAX_OFFICE_PACKET")else{return};
 let packet=std::path::Path::new(&packet);let manifest:Value=serde_json::from_slice(&std::fs::read(packet).unwrap()).unwrap();let rows=manifest["records"].as_array().unwrap();assert!(!rows.is_empty());
 let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);
 assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(profiles).unwrap())),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
 let legacy:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_LEGACY").unwrap()).unwrap()).unwrap();let legacy=legacy["records"].as_array().unwrap();assert_eq!(legacy.len(),2593);
 let company="office-private-qa";let store=Store::memory();let s=SourceImport::new(store.clone());
 let mut snapshots=Vec::new();
 for p in profiles{let id=p["client_number"].as_str().unwrap();let k=format!("dtmaxtc:{company}:client:{id}");let raw=serde_json::to_string(p).unwrap();store.set(&k,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),id).await.unwrap();snapshots.push((k,raw));}
 for r in legacy{let id=format!("{}:{}:{}",r["client_number"].as_str().unwrap(),r["tax_year"],r["sha256"].as_str().unwrap());let k=format!("dtmaxtc:{company}:return_pdf:record:{id}");let raw=serde_json::to_string(r).unwrap();store.set(&k,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:return_pdf:client:{}",r["client_number"].as_str().unwrap()),&id).await.unwrap();snapshots.push((k,raw));}
 let before=controls(&s,company).await;let mut total_bytes=0;
 for (i,r) in rows.iter().enumerate(){
  let file=packet.parent().unwrap().join(r["file"].as_str().unwrap());let bytes=std::fs::read(&file).unwrap();let digest=hex::encode(Sha256::digest(&bytes));assert!(digest==r["sha256"].as_str().unwrap(),"Private office source SHA differs");total_bytes+=bytes.len();
  let input=json!({"test_copy":true,"client_number":r["client_number"],"tax_year":r["tax_year"],"sha256":r["sha256"],"page_labels":r["page_labels"],"retained_office_provenance":r["retained_office_provenance"],"content_hex":hex::encode(&bytes)});
  // Packet records contain only the API fields plus the three staging fields.
  assert!(canonical(&input).len()<=4_400_000,"Private office request exceeds transport bound");
  let response=s.test_copy_preserve_return_pdf(company,serde_json::from_value::<ReturnPdfInput>(input.clone()).unwrap()).await.unwrap();assert_eq!(response["created"],true);assert_eq!(response["record"]["original_print_output"],false);assert!(response["record"]["retained_office_provenance"]==r["retained_office_provenance"],"Private office provenance differs");assert!(response["record"].get("export_provenance").is_none());
  assert_eq!(s.test_copy_preserve_return_pdf(company,serde_json::from_value(input.clone()).unwrap()).await.unwrap()["created"],false);
  let read=s.test_copy_return_pdf_content(company,r["client_number"].as_str().unwrap(),r["tax_year"].as_u64().unwrap()as u16,&digest).await.unwrap();assert!(read["content_hex"]==hex::encode(&bytes),"Private office bytes differ");assert!(read["record"]==response["record"],"Private office full descriptor differs");assert!(canonical(&read).len()<=4_400_000);
  if i==0{for variant in ["owner","year","proof","flag","omit","both","bytes"]{let mut bad=input.clone();match variant{"owner"=>bad["client_number"]=profiles.iter().find(|p|p["client_number"]!=r["client_number"]).unwrap()["client_number"].clone(),"year"=>bad["tax_year"]=json!(1990),"proof"=>bad["retained_office_provenance"]["owner_year_evidence_sha256"]=json!("0".repeat(64)),"flag"=>bad["retained_office_provenance"]["native_generation_verified"]=json!(true),"omit"=>{bad.as_object_mut().unwrap().remove("retained_office_provenance");},"both"=>bad["export_provenance"]=legacy.iter().find(|r|r.get("export_provenance").is_some()).unwrap()["export_provenance"].clone(),_=>bad["content_hex"]=json!("00")};assert!(s.test_copy_preserve_return_pdf(company,serde_json::from_value(bad).unwrap()).await.is_err(),"Invalid private office input accepted");}}
  assert!(std::fs::read(file).unwrap()==bytes,"Original office source file changed");
 }
 for (k,raw) in snapshots{assert!(store.get(&k).await.unwrap().as_deref()==Some(&raw),"Existing legacy metadata/profile changed");}
 assert!(controls(&s,company).await==before,"Original six controls changed");
 for r in legacy{let response=s.test_copy_return_pdfs(company,r["client_number"].as_str().unwrap()).await.unwrap();let old=response["records"].as_array().unwrap().iter().find(|p|p["sha256"]==r["sha256"]&&p["tax_year"]==r["tax_year"]).unwrap();assert_eq!(old["original_print_output"],true);assert!(old.get("retained_office_provenance").is_none());}
 println!("PRIVATE_OFFICE_DOCUMENTS_EXACT: documents={}; bytes={total_bytes}; unchanged_legacy_metadata=2593; unchanged_profiles=5856; six_controls_unchanged=true",rows.len());
}
