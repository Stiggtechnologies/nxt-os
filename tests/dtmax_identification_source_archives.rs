use axum::{body::Body,http::Request};
use nxt_os::{app_with,source_import::{SourceImport,return_pdfs::source_archives::{ArchiveChunkInput,ArchiveCommitInput,CHUNK_BYTES}},store::Store};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::BTreeSet,path::Path};
use tower::ServiceExt;
fn sha(b:&[u8])->String{hex::encode(Sha256::digest(b))}
fn commit(row:&Value)->ArchiveCommitInput{
    let mut v=row.clone();v.as_object_mut().unwrap().remove("file");v["test_copy"]=json!(true);serde_json::from_value(v).unwrap()
}
fn metadata(row:&Value)->Value{
    let mut r=row.clone();r.as_object_mut().unwrap().remove("file");r["page_count"]=json!(r["page_labels"].as_array().unwrap().len());r["original_print_output"]=json!(false);r["immutable"]=json!(true);
    r["content_transport"]=json!({"kind":"identification_source_archive_chunks_v1","chunk_bytes":CHUNK_BYTES,"chunk_count":(r["bytes"].as_u64().unwrap()as usize).div_ceil(CHUNK_BYTES)});r
}
fn identity(row:&Value)->String{format!("{}:{}:{}",row["client_number"].as_str().unwrap(),row["tax_year"],row["sha256"].as_str().unwrap())}
fn record_key(company:&str,row:&Value)->String{format!("dtmaxtc:{company}:return_pdf:record:{}",identity(row))}
fn chunk_input(row:&Value,part:&[u8],i:usize)->ArchiveChunkInput{ArchiveChunkInput{test_copy:true,sha256:row["sha256"].as_str().unwrap().into(),chunk_index:i,chunk_sha256:sha(part),content_hex:hex::encode(part)}}
async fn controls(s:&SourceImport,c:&str)->Vec<Value>{vec![s.test_copy_summary(c).await.unwrap(),s.test_copy_source_fact_summary(c).await.unwrap(),s.test_copy_native_slip_summary(c).await.unwrap(),s.test_copy_native_supplement_summary(c).await.unwrap(),s.test_copy_legacy_owner_summary(c).await.unwrap(),s.test_copy_personal_source_summary(c).await.unwrap(),s.test_copy_printed_cached_figure_summary(c).await.unwrap(),s.test_copy_preserved_pdf_figure_summary(c).await.unwrap()]}

#[tokio::test]
async fn packet_routes_require_admin_and_reject_unclosed_chunks_without_exposing_owner_data(){
    unsafe{std::env::set_var("ADMIN_API_KEY","packet-local-test");}
    let app=app_with(Store::memory);let base="/api/source-imports/dtmax/companies/packet-router-test/test-copy/";
    for (method,path)in [("POST","identification-source-archives/chunks"),("POST","identification-source-archives/commit"),("GET","identification-source-archives/profile"),("GET","identification-source-archives/additional-profile"),("GET","identification-source-archives/mixed-profile"),("GET","clients/100/return-pdfs/2022/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/archive-chunks/0")]{
        let response=app.clone().oneshot(Request::builder().method(method).uri(format!("{base}{path}")).header("content-type","application/json").body(Body::from("{}")).unwrap()).await.unwrap();assert_eq!(response.status(),401);
    }
    let response=app.clone().oneshot(Request::builder().uri(format!("{base}identification-source-archives/profile")).header("authorization","Bearer packet-local-test").body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),200);let b=axum::body::to_bytes(response.into_body(),4096).await.unwrap();let p:Value=serde_json::from_slice(&b).unwrap();assert_eq!(p["unique_original_documents"],69);assert_eq!(p["owner_year_associations"],131);assert_eq!(p["closed_profile_sha256"],"4e0e607dedb8d6c8cb7bd322b4dbecff2e92f962b74684e705faec1d0a9cb233");assert!(p.get("documents").is_none());
    let value=json!({"test_copy":true,"sha256":"a".repeat(64),"chunk_index":0,"chunk_sha256":sha(b"unclosed"),"content_hex":hex::encode(b"unclosed")});
    let response=app.oneshot(Request::builder().method("POST").uri(format!("{base}identification-source-archives/chunks")).header("authorization","Bearer packet-local-test").header("content-type","application/json").body(Body::from(value.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),400);
}

#[tokio::test]
async fn optional_additional_archives_reuse_old_originals_and_preserve_all_prior_records(){
    let Ok(path)=std::env::var("NXT_DTMAX_ADDITIONAL_SOURCE_ARCHIVE_MANIFEST")else{return};let path=Path::new(&path);
    let raw=std::fs::read(path).unwrap();assert_eq!(sha(&raw),"730c9208edcc545fdefa410d73c81aa2b0460175526b96944a45af9728c360c5");let m:Value=serde_json::from_slice(&raw).unwrap();let rows=m["records"].as_array().unwrap();assert_eq!(rows.len(),22);
    let old:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_SOURCE_ARCHIVE_MANIFEST").unwrap()).unwrap()).unwrap();let oldrows=old["records"].as_array().unwrap();assert_eq!(oldrows.len(),131);let oldsha:BTreeSet<_>=oldrows.iter().map(|r|r["sha256"].as_str().unwrap().to_string()).collect();
    let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);assert_eq!(sha(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
    let prior:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_PREPACKET_METADATA").unwrap()).unwrap()).unwrap();let four:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_REVIEWED_PACKET_MANIFEST").unwrap()).unwrap()).unwrap();let company="additional-archive-private-qa";let store=Store::memory();let s=SourceImport::new(store.clone());let mut snapshots=Vec::new();
    for p in profiles {let cid=p["client_number"].as_str().unwrap();let key=format!("dtmaxtc:{company}:client:{cid}");let raw=serde_json::to_string(p).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),cid).await.unwrap();snapshots.push((key,raw));}
    let mut prior_count=0;
    for (collection,is_archive)in [(prior["records"].as_array().unwrap(),false),(four["records"].as_array().unwrap(),false),(oldrows,true)]{
        for row in collection {let mut r=row.clone();r.as_object_mut().unwrap().remove("file");let key=record_key(company,&r);let raw=serde_json::to_string(&r).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:return_pdf:client:{}",r["client_number"].as_str().unwrap()),&identity(&r)).await.unwrap();if is_archive{store.sadd(&format!("dtmaxtc:{company}:return_pdf:identification_archive_associations"),&identity(&r)).await.unwrap();}snapshots.push((key,raw));prior_count+=1;}
    }
    assert_eq!(prior_count,14212);let before=controls(&s,company).await;let mut seen=BTreeSet::new();let mut shared=0;let mut chunks=0;
    for row in rows {
        let cid=row["client_number"].as_str().unwrap();let year=row["tax_year"].as_u64().unwrap()as u16;let digest=row["sha256"].as_str().unwrap();let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();assert_eq!(sha(&bytes),digest);
        if seen.insert(digest.to_string()) {
            if oldsha.contains(digest){
                // Seed already preserved immutable chunks under the same source digest.
                for(i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,i)).await.unwrap()["created"],true);}shared+=1;
            } else {assert!(s.test_copy_commit_source_archive(company,commit(row)).await.is_err());}
            for(i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){let result=s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,i)).await.unwrap();assert_eq!(result["created"],!oldsha.contains(digest));assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,i)).await.unwrap()["created"],false);chunks+=1;}
        }
        for wrong in ["owner","year","profile","approval","scope"]{
            let mut b=row.clone();b.as_object_mut().unwrap().remove("file");b["test_copy"]=json!(true);match wrong{"owner"=>b["client_number"]=json!("999999999"),"year"=>b["tax_year"]=json!(1990),"profile"=>b["retained_office_provenance"]["profile_sha256"]=oldrows[0]["retained_office_provenance"]["profile_sha256"].clone(),"approval"=>b["retained_office_provenance"]["financial_values_approved"]=json!(true),_=>b["source_archive_scope"]["remaining_packet_pages_unverified"]=json!(false)};assert!(s.test_copy_commit_source_archive(company,serde_json::from_value(b).unwrap()).await.is_err());
        }
        assert_eq!(s.test_copy_commit_source_archive(company,commit(row)).await.unwrap()["record"],metadata(row));assert_eq!(s.test_copy_commit_source_archive(company,commit(row)).await.unwrap()["created"],false);
        let content=s.test_copy_return_pdf_content(company,cid,year,digest).await.unwrap();assert!(content.get("content_hex").is_none());assert_eq!(content["record"],metadata(row));let mut fresh=Vec::new();for i in 0..bytes.len().div_ceil(CHUNK_BYTES){let c=s.test_copy_source_archive_chunk(company,cid,year,digest,i).await.unwrap();fresh.extend(hex::decode(c["content_hex"].as_str().unwrap()).unwrap());}assert_eq!(fresh,bytes);
        let list=s.test_copy_return_pdfs(company,cid).await.unwrap();assert!(list["records"].as_array().unwrap().contains(&metadata(row)));assert!(s.test_copy_source_archive_chunk(company,"999999999",year,digest,0).await.is_err());assert!(s.test_copy_source_archive_chunk("other-namespace",cid,year,digest,0).await.is_err());
        let overlay=s.test_copy_source_clients(company,serde_json::from_value(json!({"offset":0,"limit":100,"search":cid,"year":year})).unwrap()).await.unwrap();assert!(overlay["clients"].as_array().unwrap().iter().any(|r|r["client_number"]==cid&&r["source_archive_years"].as_array().is_some_and(|a|a.contains(&json!(year)))));
    }
    assert_eq!((seen.len(),shared,chunks),(19,3,222));assert_eq!(store.smembers(&format!("dtmaxtc:{company}:return_pdf:identification_archive_associations")).await.unwrap().len(),153);
    for(key,raw)in snapshots{assert_eq!(store.get(&key).await.unwrap().as_deref(),Some(raw.as_str()),"Earlier source profile or immutable PDF descriptor changed");}assert_eq!(controls(&s,company).await,before);
    let old_profile=s.test_copy_source_archive_profile(company).await.unwrap();assert_eq!(old_profile["unique_original_documents"],69);assert_eq!(old_profile["owner_year_associations"],131);let new=s.test_copy_additional_source_archive_profile(company).await.unwrap();assert_eq!(new["new_unique_original_documents"],16);assert_eq!(new["owner_year_associations"],22);
    println!("ADDITIONAL_ARCHIVES_ROUNDTRIP: originals=19; reused=3; chunks=222; links=22; prior_metadata=14212; profiles=5856; source_controls=8; exact_immutable_replay=true");
}

#[tokio::test]
async fn optional_largest_archive_and_summary_preserve_all_prior_profiles_metadata_and_source_controls(){
    let Ok(path)=std::env::var("NXT_DTMAX_SOURCE_ARCHIVE_MANIFEST")else{return};let path=Path::new(&path);let raw=std::fs::read(path).unwrap();assert_eq!(sha(&raw),"5e8ecd20055f0fcbdcbb0a549ae094e21640e141ecbb8232962a952cc5814640");
    let manifest:Value=serde_json::from_slice(&raw).unwrap();let rows=manifest["records"].as_array().unwrap();assert_eq!(rows.len(),131);
    let largest=rows.iter().max_by_key(|r|r["bytes"].as_u64().unwrap()).unwrap()["sha256"].as_str().unwrap();
    let rows:Vec<_>=rows.iter().filter(|r|r["sha256"].as_str()==Some(largest)||r["tax_year"]==2005).collect();assert_eq!(rows.len(),8);
    let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);assert_eq!(sha(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
    let fixture:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_PREPACKET_METADATA").unwrap()).unwrap()).unwrap();let prior=fixture["records"].as_array().unwrap();assert!(prior.len()>=13827);
    let company="reviewed-packets-private-qa";let store=Store::memory();let s=SourceImport::new(store.clone());let mut snapshots=Vec::new();
    for p in profiles{let cid=p["client_number"].as_str().unwrap();let key=format!("dtmaxtc:{company}:client:{cid}");let raw=serde_json::to_string(p).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),cid).await.unwrap();snapshots.push((key,raw));}
    for r in prior{let key=record_key(company,r);let raw=serde_json::to_string(r).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:return_pdf:client:{}",r["client_number"].as_str().unwrap()),&identity(r)).await.unwrap();snapshots.push((key,raw));}
    let before=controls(&s,company).await;let mut seen=BTreeSet::new();let mut original_bytes=0;let mut chunk_count=0;
    for row in rows{
        let file=path.parent().unwrap().join(row["file"].as_str().unwrap());let bytes=std::fs::read(&file).unwrap();assert!(sha(&bytes)==row["sha256"].as_str().unwrap(),"Original packet hash differs");
        if seen.insert(row["sha256"].as_str().unwrap().to_string()){
            assert!(s.test_copy_commit_source_archive(company,commit(row)).await.is_err(),"Incomplete original packet was committed");assert!(store.get(&record_key(company,row)).await.unwrap().is_none());
            let parts:Vec<_>=bytes.chunks(CHUNK_BYTES).collect();
            let mut bad=chunk_input(row,parts[0],0);bad.content_hex.replace_range(0..2,"00");assert!(s.test_copy_preserve_source_archive_chunk(company,bad).await.is_err(),"Altered packet bytes accepted");
            assert!(s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,parts[0],parts.len())).await.is_err(),"Out-of-range chunk accepted");
            let mut wrong_hash=chunk_input(row,parts[0],0);wrong_hash.chunk_sha256="0".repeat(64);assert!(s.test_copy_preserve_source_archive_chunk(company,wrong_hash).await.is_err());
            let mut wrong_flag=chunk_input(row,parts[0],0);wrong_flag.test_copy=false;assert!(s.test_copy_preserve_source_archive_chunk(company,wrong_flag).await.is_err());
            for (index,part)in parts.iter().enumerate(){
                if index==parts.len()-1{assert!(s.test_copy_commit_source_archive(company,commit(row)).await.is_err(),"Packet committed with missing final chunk");assert!(store.get(&record_key(company,row)).await.unwrap().is_none());}
                let result=s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,index)).await.unwrap();assert_eq!(result["created"],true);assert_eq!(result["owner_association_committed"],false);
                assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,index)).await.unwrap()["created"],false);chunk_count+=1;
            }
            original_bytes+=bytes.len();
        }
        for variant in ["owner","year","pages","proof","as_filed","approval","partial","scope_year","source_origin"]{
            let mut b=row.clone();b.as_object_mut().unwrap().remove("file");b["test_copy"]=json!(true);
            match variant{
                "owner"=>b["client_number"]=json!("999999999"),"year"=>b["tax_year"]=json!(1990),"pages"=>b["page_labels"][0]=json!("Changed"),"proof"=>b["retained_office_provenance"]["owner_year_evidence_sha256"]=json!("0".repeat(64)),
                "as_filed"=>b["retained_office_provenance"]["as_filed_verified"]=json!(true),"approval"=>b["retained_office_provenance"]["financial_values_approved"]=json!(true),"partial"=>b["source_archive_scope"]["full_return_completeness_verified"]=json!(true),"scope_year"=>b["source_archive_scope"]["verified_source_pages"][0]["page"]=json!(0),_=>b["retained_office_provenance"]["kind"]=json!("retained_office_T1_scanned_document")
            }
            assert!(s.test_copy_commit_source_archive(company,serde_json::from_value(b).unwrap()).await.is_err(),"Unreviewed packet association accepted");
        }
        let result=s.test_copy_commit_source_archive(company,commit(row)).await.unwrap();assert_eq!(result["created"],true);assert!(result["record"]==metadata(row),"Packet descriptor differs");assert_eq!(s.test_copy_commit_source_archive(company,commit(row)).await.unwrap()["created"],false);
        let cid=row["client_number"].as_str().unwrap();let year=row["tax_year"].as_u64().unwrap()as u16;let digest=row["sha256"].as_str().unwrap();
        let content=s.test_copy_return_pdf_content(company,cid,year,digest).await.unwrap();assert!(content.get("content_hex").is_none()&&content["record"]==metadata(row),"Packet metadata transport differs");
        let mut fresh=Vec::new();for index in 0..bytes.len().div_ceil(CHUNK_BYTES){let chunk=s.test_copy_source_archive_chunk(company,cid,year,digest,index).await.unwrap();fresh.extend(hex::decode(chunk["content_hex"].as_str().unwrap()).unwrap());}
        assert!(fresh==bytes,"Complete original packet bytes changed");assert!(std::fs::read(file).unwrap()==bytes,"Local original packet changed");
        let overlay=s.test_copy_source_clients(company, serde_json::from_value(json!({"offset":0,"limit":100,"search":cid,"year":year})).unwrap()).await.unwrap();
        assert!(overlay["clients"].as_array().unwrap().iter().any(|c|c["client_number"]==cid && c["source_archive_years"].as_array().is_some_and(|a|a.contains(&json!(year)))),"Source archive year missing from roster");
        let listed=s.test_copy_return_pdfs(company,cid).await.unwrap();assert!(listed["records"].as_array().unwrap().contains(&metadata(row)),"Packet not indexed exactly for owner");
        assert!(s.test_copy_source_archive_chunk(company,"999999999",year,digest,0).await.is_err());assert!(s.test_copy_source_archive_chunk(company,cid,1990,digest,0).await.is_err());assert!(s.test_copy_source_archive_chunk("another-source-scope",cid,year,digest,0).await.is_err());
        // A matching retry repairs a record committed just before index creation.
        if year==2005{
            let repair="packet-interruption-repair";let original_record=store.get(&record_key(company,row)).await.unwrap().unwrap();store.set(&format!("dtmaxtc:{repair}:client:{cid}"),"{}").await.unwrap();store.set(&record_key(repair,row),&original_record).await.unwrap();
            for (i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){s.test_copy_preserve_source_archive_chunk(repair,chunk_input(row,part,i)).await.unwrap();}
            assert!(s.test_copy_return_pdfs(repair,cid).await.unwrap()["records"].as_array().unwrap().is_empty());assert_eq!(s.test_copy_commit_source_archive(repair,commit(row)).await.unwrap()["created"],false);assert_eq!(s.test_copy_return_pdfs(repair,cid).await.unwrap()["records"].as_array().unwrap().len(),1);
        }
    }
    assert_eq!(seen.len(),2);assert!(original_bytes>54000000);assert!(chunk_count>53);
    for (key,raw)in snapshots{assert!(store.get(&key).await.unwrap().as_deref()==Some(&raw),"Existing profile or immutable PDF metadata changed");}
    assert!(controls(&s,company).await==before,"Source controls or earlier figure summaries changed");
    println!("REVIEWED_PACKETS_EXACT: documents=2; selected_largest_archive_and_2005_summary=true; profiles=5856; prior_metadata={}; source_controls=8; unmatched_admitted=0",prior.len());
}

#[tokio::test]
async fn optional_mixed_archives_roundtrip_preserves_old_profiles_metadata_and_source_controls(){
    let Ok(path)=std::env::var("NXT_DTMAX_MIXED_SOURCE_ARCHIVE_MANIFEST")else{return};let path=Path::new(&path);let raw=std::fs::read(path).unwrap();assert_eq!(sha(&raw),"3fe88043a7380da4e0dda4f824c130a013f9a5329bfdab06ac1951b9f8996e4e");let m:Value=serde_json::from_slice(&raw).unwrap();let rows=m["records"].as_array().unwrap();assert_eq!(rows.len(),22);
    let company="mixed-source-archive-private-qa";let store=Store::memory();let s=SourceImport::new(store.clone());let mut snapshots=Vec::new();
    let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);assert_eq!(sha(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
    for p in profiles {let cid=p["client_number"].as_str().unwrap();let key=format!("dtmaxtc:{company}:client:{cid}");let raw=serde_json::to_string(p).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),cid).await.unwrap();snapshots.push((key,raw));}
    let mut prior_count=0;
    for (env,is_archive)in [("NXT_DTMAX_PREPACKET_METADATA",false),("NXT_DTMAX_REVIEWED_PACKET_MANIFEST",false),("NXT_DTMAX_SOURCE_ARCHIVE_MANIFEST",true),("NXT_DTMAX_ADDITIONAL_SOURCE_ARCHIVE_MANIFEST",true)] {
        let old:Value=serde_json::from_slice(&std::fs::read(std::env::var(env).unwrap()).unwrap()).unwrap();for row in old["records"].as_array().unwrap(){let mut row=row.clone();row.as_object_mut().unwrap().remove("file");let key=record_key(company,&row);let raw=serde_json::to_string(&row).unwrap();store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:return_pdf:client:{}",row["client_number"].as_str().unwrap()),&identity(&row)).await.unwrap();if is_archive{store.sadd(&format!("dtmaxtc:{company}:return_pdf:identification_archive_associations"),&identity(&row)).await.unwrap();}snapshots.push((key,raw));prior_count+=1;}
    }
    assert_eq!(prior_count,14234);
    let cached_path=std::env::var("NXT_PRESERVED_CACHED_FIXTURE").unwrap();let cached_raw=std::fs::read(cached_path).unwrap();assert_eq!(sha(&cached_raw),"d7977b8d81a783a6a23b05dafddfe62daf50571d0200399d4daa4a520fc2702c");let mut cached_count=0;
    for line in cached_raw.split(|b|*b==b'\n').filter(|l|!l.is_empty()){let v:Value=serde_json::from_slice(line).unwrap();let id=format!("{}:{}:{}",v["client_number"].as_str().unwrap(),v["source_return_year"],v["pdf"]["sha256"].as_str().unwrap());let profile=v["printed_figure_profile_sha256"].as_str().unwrap();let prefix=format!("dtmaxtc:{company}:printed_cached_figures:{profile}");let raw=serde_json::to_string(&v).unwrap();let key=format!("{prefix}:record:{id}");store.set(&key,&raw).await.unwrap();store.sadd(&format!("{prefix}:all"),&id).await.unwrap();store.sadd(&format!("{prefix}:digests"),&sha(raw.as_bytes())).await.unwrap();snapshots.push((key,raw));cached_count+=1;}
    assert_eq!(cached_count,2527);assert_eq!(s.test_copy_printed_cached_figure_summary(company).await.unwrap()["PDF_observation_records"],2527);
    let before=controls(&s,company).await;let old_profile=s.test_copy_source_archive_profile(company).await.unwrap();let add_profile=s.test_copy_additional_source_archive_profile(company).await.unwrap();let mut seen=BTreeSet::new();let mut chunks=0;
    for row in rows {
        let cid=row["client_number"].as_str().unwrap();let year=row["tax_year"].as_u64().unwrap()as u16;let digest=row["sha256"].as_str().unwrap();
        if seen.insert(digest.to_string()) {let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();assert_eq!(sha(&bytes),digest);assert!(s.test_copy_commit_source_archive(company,commit(row)).await.is_err());for(i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,i)).await.unwrap()["created"],true);assert_eq!(s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,i)).await.unwrap()["created"],false);chunks+=1;}}
        assert_eq!(s.test_copy_commit_source_archive(company,commit(row)).await.unwrap()["record"],metadata(row));assert_eq!(s.test_copy_commit_source_archive(company,commit(row)).await.unwrap()["created"],false);let content=s.test_copy_return_pdf_content(company,cid,year,digest).await.unwrap();assert_eq!(content["record"],metadata(row));assert!(s.test_copy_return_pdfs(company,cid).await.unwrap()["records"].as_array().unwrap().contains(&metadata(row)));
        assert!(s.test_copy_source_archive_chunk(company,"999999999",year,digest,0).await.is_err());
    }
    // Freshly read every physical original once, independent of commit responses.
    for digest in seen.iter(){let row=rows.iter().find(|r|r["sha256"].as_str()==Some(digest)).unwrap();let mut fresh=Vec::new();for i in 0..(row["bytes"].as_u64().unwrap()as usize).div_ceil(CHUNK_BYTES){let v=s.test_copy_source_archive_chunk(company,row["client_number"].as_str().unwrap(),row["tax_year"].as_u64().unwrap()as u16,digest,i).await.unwrap();fresh.extend(hex::decode(v["content_hex"].as_str().unwrap()).unwrap());}assert_eq!(sha(&fresh),*digest);assert_eq!(fresh,std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap());}
    assert_eq!((seen.len(),chunks),(6,93));assert_eq!(store.smembers(&format!("dtmaxtc:{company}:return_pdf:identification_archive_associations")).await.unwrap().len(),175);
    for(key,raw)in snapshots{assert_eq!(store.get(&key).await.unwrap().as_deref(),Some(raw.as_str()),"Prior source metadata or client profiles changed");}assert_eq!(controls(&s,company).await,before);assert_eq!(s.test_copy_source_archive_profile(company).await.unwrap(),old_profile);assert_eq!(s.test_copy_additional_source_archive_profile(company).await.unwrap(),add_profile);assert_eq!(s.test_copy_mixed_source_archive_profile(company).await.unwrap()["owner_year_associations"],22);
    println!("MIXED_SOURCE_ROUNDTRIP: originals=6; pages=207; bytes=94560121; chunks=93; links=22; old_metadata=14234; client_profiles=5856; cached_records_unchanged=2527; old_profiles_and_controls_exact=true; fresh_all_originals_equal=true");
}

#[tokio::test]
async fn optional_mixed_commit_replay_repairs_three_interrupted_index_states(){
    let Ok(path)=std::env::var("NXT_DTMAX_MIXED_SOURCE_ARCHIVE_MANIFEST")else{return};let path=Path::new(&path);let m:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let row=m["records"].as_array().unwrap().iter().filter(|r|r["source_archive_scope"]["source_role"]=="printed_slip_identification_archive").min_by_key(|r|r["bytes"].as_u64()).unwrap();let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();let cid=row["client_number"].as_str().unwrap();let company="mixed-index-replay-qa";let base=Store::memory();let s=SourceImport::new(base.clone());base.set(&format!("dtmaxtc:{company}:client:{cid}"),"{}").await.unwrap();let mut chunk_pairs=Vec::new();
    for(i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){s.test_copy_preserve_source_archive_chunk(company,chunk_input(row,part,i)).await.unwrap();chunk_pairs.push((format!("dtmaxtc:{company}:return_pdf:identification_archive_blob:{}:chunk:{i}",row["sha256"].as_str().unwrap()),hex::encode(part)));}s.test_copy_commit_source_archive(company,commit(row)).await.unwrap();let raw=base.get(&record_key(company,row)).await.unwrap().unwrap();let id=identity(row);let client_index=format!("dtmaxtc:{company}:return_pdf:client:{cid}");let global_index=format!("dtmaxtc:{company}:return_pdf:identification_archive_associations");
    for phase in 0..3 {let store=Store::memory();let s=SourceImport::new(store.clone());store.set(&format!("dtmaxtc:{company}:client:{cid}"),"{}").await.unwrap();for(k,v)in&chunk_pairs{store.set(k,v).await.unwrap();}store.set(&record_key(company,row),&raw).await.unwrap();if phase==1{store.sadd(&client_index,&id).await.unwrap();}if phase==2{store.sadd(&global_index,&id).await.unwrap();}let got=s.test_copy_commit_source_archive(company,commit(row)).await.unwrap();assert_eq!(got["created"],false);assert_eq!(got["record"],metadata(row));assert_eq!(store.smembers(&client_index).await.unwrap(),vec![id.clone()]);assert_eq!(store.smembers(&global_index).await.unwrap(),vec![id.clone()]);assert_eq!(s.test_copy_return_pdfs(company,cid).await.unwrap()["records"],json!([metadata(row)]));assert_eq!(store.get(&record_key(company,row)).await.unwrap().as_deref(),Some(raw.as_str()));}
    println!("MIXED_SOURCE_REPLAY_REPAIRS: missing_both_indexes=true; missing_global_index=true; missing_client_index=true; original_record_exact=true");
}
