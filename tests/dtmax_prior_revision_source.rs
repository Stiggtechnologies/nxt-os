use axum::{body::Body,http::Request};
use nxt_os::{app_with,source_import::{SourceImport,return_pdfs::source_archives::{ArchiveChunkInput,ArchiveCommitInput}},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::path::Path;
use tower::ServiceExt;
fn sha(b:&[u8])->String{hex::encode(Sha256::digest(b))}
fn input(row:&Value)->ArchiveCommitInput{let mut r=row.clone();r.as_object_mut().unwrap().remove("file");r["test_copy"]=json!(true);serde_json::from_value(r).unwrap()}
fn chunk(b:&[u8],digest:&str)->ArchiveChunkInput{ArchiveChunkInput{test_copy:true,sha256:digest.into(),chunk_index:0,chunk_sha256:sha(b),content_hex:hex::encode(b)}}
async fn controls(s:&SourceImport,c:&str)->Vec<Value>{vec![s.test_copy_summary(c).await.unwrap(),s.test_copy_source_fact_summary(c).await.unwrap(),s.test_copy_native_slip_summary(c).await.unwrap(),s.test_copy_native_supplement_summary(c).await.unwrap(),s.test_copy_legacy_owner_summary(c).await.unwrap(),s.test_copy_personal_source_summary(c).await.unwrap(),s.test_copy_printed_cached_figure_summary(c).await.unwrap(),s.test_copy_preserved_pdf_figure_summary(c).await.unwrap()]}
#[tokio::test]
async fn prior_revision_profile_is_authenticated_finite_and_does_not_expose_source_people(){
 unsafe{std::env::set_var("ADMIN_API_KEY","prior-revision-local-test");}
 let app=app_with(Store::memory);let path="/api/source-imports/dtmax/companies/prior-router/test-copy/identification-source-archives/prior-revision-profile";
 assert_eq!(app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap().status(),401);
 let response=app.oneshot(Request::builder().uri(path).header("authorization","Bearer prior-revision-local-test").body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),200);
 let b=axum::body::to_bytes(response.into_body(),4096).await.unwrap();let p:Value=serde_json::from_slice(&b).unwrap();
 assert_eq!(p["readable_prior_revisions"],1);assert_eq!(p["latest_encrypted_versions_unlocked"],0);assert_eq!(p["physical_printed_copies"],2);assert_eq!(p["normalized_slips_created"],0);assert_eq!(p["financial_values_approved"],false);assert!(p.get("documents").is_none());assert!(p.get("client_number").is_none());
}
#[tokio::test]
async fn optional_actual_prior_revision_preserves_profiles_controls_and_rejects_forged_sources(){
 let Ok(path)=std::env::var("NXT_DTMAX_PRIOR_T4_MANIFEST")else{return};let path=Path::new(&path);let raw=std::fs::read(path).unwrap();assert_eq!(sha(&raw),"8beb9ec1ae1141450b11ec5f9f444a643892e0065065917d4e6caea46157c033");let m:Value=serde_json::from_slice(&raw).unwrap();let rows=m["records"].as_array().unwrap();assert_eq!(rows.len(),1);let row=&rows[0];let cid=row["client_number"].as_str().unwrap();let digest=row["sha256"].as_str().unwrap();let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();assert_eq!(bytes.len(),340070);assert_eq!(sha(&bytes),digest);
 let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);assert_eq!(sha(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
 let company="actual-prior-T4-private";let store=Store::memory();let s=SourceImport::new(store.clone());let mut saved=Vec::new();
 for p in profiles{let key=format!("dtmaxtc:{company}:client:{}",p["client_number"].as_str().unwrap());let raw=serde_json::to_string(p).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),p["client_number"].as_str().unwrap()).await.unwrap();saved.push((key,raw));}
 let before=controls(&s,company).await;assert!(s.test_copy_commit_source_archive(company,input(row)).await.is_err());
 assert!(s.test_copy_preserve_source_archive_chunk(company,chunk(b"not admitted",digest)).await.is_err());
 assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk(&bytes,digest)).await.unwrap()["created"],true);
 assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk(&bytes,digest)).await.unwrap()["created"],false);
 for fault in ["owner","year","profile","latest","blank","revision","approval","scope"]{
  let mut wrong=row.clone();match fault{
   "owner"=>wrong["client_number"]=json!("999999999"),"year"=>wrong["tax_year"]=json!(2024),"profile"=>wrong["retained_office_provenance"]["profile_sha256"]=json!("e14edac6ccc8d3d1067997ce45b858ec943317e03f988bbc47cedae384c21418"),"approval"=>wrong["retained_office_provenance"]["financial_values_approved"]=json!(true),"scope"=>wrong["source_archive_scope"]["remaining_packet_pages_unverified"]=json!(true),_=>{
    let mut o:Value=serde_json::from_str(wrong["source_archive_scope"]["printed_slip_observations_json"].as_str().unwrap()).unwrap();match fault{"latest"=>o["earlier_revision"]["latest_encrypted_revision_unlocked"]=json!(true),"blank"=>o["physical_copies"][0]["fields"][3]["amount_cents"]=json!(0),_=>o["earlier_revision"]["source_byte_end_exclusive"]=json!(340069)};let text=serde_json::to_string(&o).unwrap();wrong["source_archive_scope"]["printed_slip_observations_sha256"]=json!(sha(text.as_bytes()));wrong["source_archive_scope"]["printed_slip_observations_json"]=json!(text);
   }
  }assert!(s.test_copy_commit_source_archive(company,input(&wrong)).await.is_err(),"forged {fault} accepted");
 }
 let committed=s.test_copy_commit_source_archive(company,input(row)).await.unwrap();assert_eq!(committed["created"],true);assert_eq!(s.test_copy_commit_source_archive(company,input(row)).await.unwrap()["created"],false);
 let content=s.test_copy_return_pdf_content(company,cid,2025,digest).await.unwrap();assert!(content.get("content_hex").is_none());assert_eq!(content["record"]["source_archive_scope"],row["source_archive_scope"]);assert_eq!(content["record"]["original_print_output"],false);
 let part=s.test_copy_source_archive_chunk(company,cid,2025,digest,0).await.unwrap();assert_eq!(hex::decode(part["content_hex"].as_str().unwrap()).unwrap(),bytes);
 assert!(s.test_copy_source_archive_chunk(company,"999999999",2025,digest,0).await.is_err());assert!(s.test_copy_source_archive_chunk(company,cid,2024,digest,0).await.is_err());assert!(s.test_copy_source_archive_chunk("other-prior-namespace",cid,2025,digest,0).await.is_err());
 let list=s.test_copy_return_pdfs(company,cid).await.unwrap();assert_eq!(list["records"].as_array().unwrap().len(),1);
 assert_eq!(controls(&s,company).await,before);for(key,raw)in saved{assert_eq!(store.get(&key).await.unwrap().as_deref(),Some(raw.as_str()));}
 println!("PRIOR_T4_ACTUAL: source_bytes=340070; profiles=5856; immutable_replay=true; controls=8; normalized_slips=0; latest_encrypted_unlocked=0");
}
