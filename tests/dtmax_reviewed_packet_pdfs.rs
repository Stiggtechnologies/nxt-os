use axum::{body::Body,http::Request};
use nxt_os::{app_with,source_import::{SourceImport,return_pdfs::reviewed_packets::{PacketChunkInput,PacketCommitInput,CHUNK_BYTES}},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,path::Path};
use tower::ServiceExt;
fn sha(b:&[u8])->String{hex::encode(Sha256::digest(b))}
fn commit(row:&Value)->PacketCommitInput{
    let mut v=row.clone();v.as_object_mut().unwrap().remove("file");v["test_copy"]=json!(true);serde_json::from_value(v).unwrap()
}
fn metadata(row:&Value)->Value{
    let mut r=row.clone();r.as_object_mut().unwrap().remove("file");r["page_count"]=json!(r["page_labels"].as_array().unwrap().len());r["original_print_output"]=json!(false);r["immutable"]=json!(true);
    r["content_transport"]=json!({"kind":"reviewed_scanned_packet_chunks_v1","chunk_bytes":CHUNK_BYTES,"chunk_count":(r["bytes"].as_u64().unwrap()as usize).div_ceil(CHUNK_BYTES)});r
}
fn identity(row:&Value)->String{format!("{}:{}:{}",row["client_number"].as_str().unwrap(),row["tax_year"],row["sha256"].as_str().unwrap())}
fn record_key(company:&str,row:&Value)->String{format!("dtmaxtc:{company}:return_pdf:record:{}",identity(row))}
fn chunk_input(row:&Value,part:&[u8],i:usize)->PacketChunkInput{PacketChunkInput{test_copy:true,sha256:row["sha256"].as_str().unwrap().into(),chunk_index:i,chunk_sha256:sha(part),content_hex:hex::encode(part)}}
async fn controls(s:&SourceImport,c:&str)->Vec<Value>{vec![s.test_copy_summary(c).await.unwrap(),s.test_copy_source_fact_summary(c).await.unwrap(),s.test_copy_native_slip_summary(c).await.unwrap(),s.test_copy_native_supplement_summary(c).await.unwrap(),s.test_copy_legacy_owner_summary(c).await.unwrap(),s.test_copy_personal_source_summary(c).await.unwrap(),s.test_copy_printed_cached_figure_summary(c).await.unwrap(),s.test_copy_preserved_pdf_figure_summary(c).await.unwrap()]}

#[tokio::test]
async fn packet_routes_require_admin_and_reject_unclosed_chunks_without_exposing_owner_data(){
    unsafe{std::env::set_var("ADMIN_API_KEY","packet-local-test");}
    let app=app_with(Store::memory);let base="/api/source-imports/dtmax/companies/packet-router-test/test-copy/";
    for (method,path)in [("POST","reviewed-return-packets/chunks"),("POST","reviewed-return-packets/commit"),("GET","reviewed-return-packets/profile"),("GET","clients/100/return-pdfs/2022/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/chunks/0")]{
        let response=app.clone().oneshot(Request::builder().method(method).uri(format!("{base}{path}")).header("content-type","application/json").body(Body::from("{}")).unwrap()).await.unwrap();assert_eq!(response.status(),401);
    }
    let response=app.clone().oneshot(Request::builder().uri(format!("{base}reviewed-return-packets/profile")).header("authorization","Bearer packet-local-test").body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),200);let b=axum::body::to_bytes(response.into_body(),4096).await.unwrap();let p:Value=serde_json::from_slice(&b).unwrap();assert_eq!(p["unique_original_documents"],4);assert_eq!(p["owner_year_associations"],8);assert_eq!(p["closed_profile_sha256"],"33af6a05d7d01c2eba3b7a0173f139918633da18300d9d3e9962c7a5f97cf469");assert!(p.get("documents").is_none());
    let value=json!({"test_copy":true,"sha256":"a".repeat(64),"chunk_index":0,"chunk_sha256":sha(b"unclosed"),"content_hex":hex::encode(b"unclosed")});
    let response=app.oneshot(Request::builder().method("POST").uri(format!("{base}reviewed-return-packets/chunks")).header("authorization","Bearer packet-local-test").header("content-type","application/json").body(Body::from(value.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),400);
}

#[tokio::test]
async fn optional_four_actual_packets_preserve_all_prior_profiles_metadata_and_source_controls(){
    let Ok(path)=std::env::var("NXT_DTMAX_REVIEWED_PACKET_MANIFEST")else{return};let path=Path::new(&path);let raw=std::fs::read(path).unwrap();assert_eq!(sha(&raw),"23a3f5b5299feeaf306b95e5ed4e7bb774aeef569cd0a3912719d568a9fd7984");
    let manifest:Value=serde_json::from_slice(&raw).unwrap();let rows=manifest["records"].as_array().unwrap();assert_eq!(rows.len(),8);
    let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);assert_eq!(sha(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
    let fixture:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_PREPACKET_METADATA").unwrap()).unwrap()).unwrap();let prior=fixture["records"].as_array().unwrap();assert!(prior.len()>=13827);
    let company="reviewed-packets-private-qa";let store=Store::memory();let s=SourceImport::new(store.clone());let mut snapshots=Vec::new();
    for p in profiles{let cid=p["client_number"].as_str().unwrap();let key=format!("dtmaxtc:{company}:client:{cid}");let raw=serde_json::to_string(p).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),cid).await.unwrap();snapshots.push((key,raw));}
    for r in prior{let key=record_key(company,r);let raw=serde_json::to_string(r).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:return_pdf:client:{}",r["client_number"].as_str().unwrap()),&identity(r)).await.unwrap();snapshots.push((key,raw));}
    let before=controls(&s,company).await;let mut seen=BTreeSet::new();let mut original_bytes=0;let mut chunk_count=0;
    for row in rows{
        let file=path.parent().unwrap().join(row["file"].as_str().unwrap());let bytes=std::fs::read(&file).unwrap();assert!(sha(&bytes)==row["sha256"].as_str().unwrap(),"Original packet hash differs");
        if seen.insert(row["sha256"].as_str().unwrap().to_string()){
            assert!(s.test_copy_commit_reviewed_packet(company,commit(row)).await.is_err(),"Incomplete original packet was committed");assert!(store.get(&record_key(company,row)).await.unwrap().is_none());
            let parts:Vec<_>=bytes.chunks(CHUNK_BYTES).collect();
            let mut bad=chunk_input(row,parts[0],0);bad.content_hex.replace_range(0..2,"00");assert!(s.test_copy_preserve_reviewed_packet_chunk(company,bad).await.is_err(),"Altered packet bytes accepted");
            assert!(s.test_copy_preserve_reviewed_packet_chunk(company,chunk_input(row,parts[0],parts.len())).await.is_err(),"Out-of-range chunk accepted");
            let mut wrong_hash=chunk_input(row,parts[0],0);wrong_hash.chunk_sha256="0".repeat(64);assert!(s.test_copy_preserve_reviewed_packet_chunk(company,wrong_hash).await.is_err());
            let mut wrong_flag=chunk_input(row,parts[0],0);wrong_flag.test_copy=false;assert!(s.test_copy_preserve_reviewed_packet_chunk(company,wrong_flag).await.is_err());
            for (index,part)in parts.iter().enumerate(){
                if index==parts.len()-1{assert!(s.test_copy_commit_reviewed_packet(company,commit(row)).await.is_err(),"Packet committed with missing final chunk");assert!(store.get(&record_key(company,row)).await.unwrap().is_none());}
                let result=s.test_copy_preserve_reviewed_packet_chunk(company,chunk_input(row,part,index)).await.unwrap();assert_eq!(result["created"],true);assert_eq!(result["owner_association_committed"],false);
                assert_eq!(s.test_copy_preserve_reviewed_packet_chunk(company,chunk_input(row,part,index)).await.unwrap()["created"],false);chunk_count+=1;
            }
            original_bytes+=bytes.len();
        }
        for variant in ["owner","year","pages","proof","as_filed","approval","partial","scope_year","source_origin"]{
            let mut b=row.clone();b.as_object_mut().unwrap().remove("file");b["test_copy"]=json!(true);
            match variant{
                "owner"=>b["client_number"]=json!("999999999"),"year"=>b["tax_year"]=json!(1990),"pages"=>b["page_labels"][0]=json!("Changed"),"proof"=>b["retained_office_provenance"]["owner_year_evidence_sha256"]=json!("0".repeat(64)),
                "as_filed"=>b["retained_office_provenance"]["as_filed_verified"]=json!(true),"approval"=>b["retained_office_provenance"]["financial_values_approved"]=json!(true),"partial"=>b["reviewed_packet_scope"]["full_return_completeness_verified"]=json!(true),"scope_year"=>b["reviewed_packet_scope"]["year_first_page"]=json!(0),_=>b["retained_office_provenance"]["kind"]=json!("retained_office_T1_scanned_document")
            }
            assert!(s.test_copy_commit_reviewed_packet(company,serde_json::from_value(b).unwrap()).await.is_err(),"Unreviewed packet association accepted");
        }
        let result=s.test_copy_commit_reviewed_packet(company,commit(row)).await.unwrap();assert_eq!(result["created"],true);assert!(result["record"]==metadata(row),"Packet descriptor differs");assert_eq!(s.test_copy_commit_reviewed_packet(company,commit(row)).await.unwrap()["created"],false);
        let cid=row["client_number"].as_str().unwrap();let year=row["tax_year"].as_u64().unwrap()as u16;let digest=row["sha256"].as_str().unwrap();
        let content=s.test_copy_return_pdf_content(company,cid,year,digest).await.unwrap();assert!(content.get("content_hex").is_none()&&content["record"]==metadata(row),"Packet metadata transport differs");
        let mut fresh=Vec::new();for index in 0..bytes.len().div_ceil(CHUNK_BYTES){let chunk=s.test_copy_reviewed_packet_chunk(company,cid,year,digest,index).await.unwrap();fresh.extend(hex::decode(chunk["content_hex"].as_str().unwrap()).unwrap());}
        assert!(fresh==bytes,"Complete original packet bytes changed");assert!(std::fs::read(file).unwrap()==bytes,"Local original packet changed");
        let listed=s.test_copy_return_pdfs(company,cid).await.unwrap();assert!(listed["records"].as_array().unwrap().contains(&metadata(row)),"Packet not indexed exactly for owner");
        assert!(s.test_copy_reviewed_packet_chunk(company,"999999999",year,digest,0).await.is_err());assert!(s.test_copy_reviewed_packet_chunk(company,cid,1990,digest,0).await.is_err());assert!(s.test_copy_reviewed_packet_chunk("another-source-scope",cid,year,digest,0).await.is_err());
        // A matching retry repairs a record committed just before index creation.
        if year==2022&&row["page_labels"].as_array().unwrap().len()==3{
            let repair="packet-interruption-repair";let original_record=store.get(&record_key(company,row)).await.unwrap().unwrap();store.set(&format!("dtmaxtc:{repair}:client:{cid}"),"{}").await.unwrap();store.set(&record_key(repair,row),&original_record).await.unwrap();
            for (i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){s.test_copy_preserve_reviewed_packet_chunk(repair,chunk_input(row,part,i)).await.unwrap();}
            assert!(s.test_copy_return_pdfs(repair,cid).await.unwrap()["records"].as_array().unwrap().is_empty());assert_eq!(s.test_copy_commit_reviewed_packet(repair,commit(row)).await.unwrap()["created"],false);assert_eq!(s.test_copy_return_pdfs(repair,cid).await.unwrap()["records"].as_array().unwrap().len(),1);
        }
    }
    assert_eq!((seen.len(),original_bytes,chunk_count),(4,27008906,28));
    for (key,raw)in snapshots{assert!(store.get(&key).await.unwrap().as_deref()==Some(&raw),"Existing profile or immutable PDF metadata changed");}
    assert!(controls(&s,company).await==before,"Source controls or earlier figure summaries changed");
    println!("REVIEWED_PACKETS_EXACT: documents=4; original_pages=49; bytes=27008906; associations=8; chunks=28; profiles=5856; prior_metadata={}; source_controls=8; unmatched_admitted=0",prior.len());
}
