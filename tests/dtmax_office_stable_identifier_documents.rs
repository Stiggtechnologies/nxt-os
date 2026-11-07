use nxt_os::{source_import::{return_pdfs::ReturnPdfInput, SourceImport}, store::Store};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

fn digest(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }
fn input(row: &Value, bytes: &[u8]) -> Value {
    json!({"test_copy":true,"client_number":row["client_number"],"tax_year":row["tax_year"],
        "sha256":row["sha256"],"page_labels":row["page_labels"],
        "retained_office_provenance":row["retained_office_provenance"],"content_hex":hex::encode(bytes)})
}
fn metadata(row: &Value) -> Value {
    json!({"client_number":row["client_number"],"tax_year":row["tax_year"],"sha256":row["sha256"],
        "bytes":row["bytes"],"page_labels":row["page_labels"],"page_count":row["page_labels"].as_array().unwrap().len(),
        "retained_office_provenance":row["retained_office_provenance"],"original_print_output":false,"immutable":true})
}
fn record_key(company: &str, row: &Value) -> String {
    format!("dtmaxtc:{company}:return_pdf:record:{}:{}:{}",row["client_number"].as_str().unwrap(),row["tax_year"],row["sha256"].as_str().unwrap())
}
async fn controls(s: &SourceImport, company: &str) -> Vec<Value> {
    vec![s.test_copy_summary(company).await.unwrap(),s.test_copy_source_fact_summary(company).await.unwrap(),
        s.test_copy_native_slip_summary(company).await.unwrap(),s.test_copy_native_supplement_summary(company).await.unwrap(),
        s.test_copy_legacy_owner_summary(company).await.unwrap(),s.test_copy_personal_source_summary(company).await.unwrap(),
        s.test_copy_printed_cached_figure_summary(company).await.unwrap()]
}

#[tokio::test]
async fn optional_actual_two_identifier_and_one_scan_documents_keep_prior_office_and_legacy_evidence() {
    let Ok(packet) = std::env::var("NXT_DTMAX_STABLE_OFFICE_PACKET") else { return };
    let packet = Path::new(&packet);
    let manifest: Value=serde_json::from_slice(&std::fs::read(packet).unwrap()).unwrap();
    let rows=manifest["records"].as_array().unwrap();assert_eq!(rows.len(),2);
    let original=std::env::var("NXT_DTMAX_OFFICE_PACKET").unwrap();let original=Path::new(&original);
    let original_manifest: Value=serde_json::from_slice(&std::fs::read(original).unwrap()).unwrap();
    let original_rows=original_manifest["records"].as_array().unwrap();assert_eq!(original_rows.len(),177);
    let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();
    let profiles=profiles["clients"].as_array().unwrap();assert_eq!(profiles.len(),5856);
    assert_eq!(digest(&serde_json::to_vec(profiles).unwrap()),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
    let legacy:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_LEGACY").unwrap()).unwrap()).unwrap();
    let legacy=legacy["records"].as_array().unwrap();assert_eq!(legacy.len(),2593);
    let company="stable-office-private-qa";let store=Store::memory();let s=SourceImport::new(store.clone());let mut snapshots=Vec::new();
    for p in profiles {
        let id=p["client_number"].as_str().unwrap();let key=format!("dtmaxtc:{company}:client:{id}");let raw=serde_json::to_string(p).unwrap();
        store.set(&key,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),id).await.unwrap();snapshots.push((key,raw));
    }
    for r in legacy {
        let key=record_key(company,r);let raw=serde_json::to_string(r).unwrap();store.set(&key,&raw).await.unwrap();
        let id=format!("{}:{}:{}",r["client_number"].as_str().unwrap(),r["tax_year"],r["sha256"].as_str().unwrap());
        store.sadd(&format!("dtmaxtc:{company}:return_pdf:client:{}",r["client_number"].as_str().unwrap()),&id).await.unwrap();snapshots.push((key,raw));
    }
    let mut original_bytes=0;
    for row in original_rows {
        let bytes=std::fs::read(original.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();original_bytes+=bytes.len();
        assert!(digest(&bytes)==row["sha256"].as_str().unwrap(),"Original office PDF source hash differs");
        let result=s.test_copy_preserve_return_pdf(company,serde_json::from_value(input(row,&bytes)).unwrap()).await.unwrap();
        assert!(result["record"]==metadata(row),"Original office descriptor differs");
        let key=record_key(company,row);snapshots.push((key.clone(),store.get(&key).await.unwrap().unwrap()));
    }
    assert_eq!(original_bytes,91497591);
    let before=controls(&s,company).await;let mut added_bytes=0;
    for (i,row) in rows.iter().enumerate() {
        let file=packet.parent().unwrap().join(row["file"].as_str().unwrap());let bytes=std::fs::read(&file).unwrap();added_bytes+=bytes.len();
        assert!(digest(&bytes)==row["sha256"].as_str().unwrap(),"New stable-identifier source PDF hash differs");
        let value=input(row,&bytes);assert!(serde_json::to_vec(&value).unwrap().len()<=4_400_000);
        let result=s.test_copy_preserve_return_pdf(company,serde_json::from_value::<ReturnPdfInput>(value.clone()).unwrap()).await.unwrap();
        assert_eq!(result["created"],true);assert!(result["record"]==metadata(row),"New stable-identifier descriptor differs");
        assert_eq!(s.test_copy_preserve_return_pdf(company,serde_json::from_value(value.clone()).unwrap()).await.unwrap()["created"],false);
        let read=s.test_copy_return_pdf_content(company,row["client_number"].as_str().unwrap(),row["tax_year"].as_u64().unwrap()as u16,row["sha256"].as_str().unwrap()).await.unwrap();
        assert!(read["content_hex"]==hex::encode(&bytes) && read["record"]==metadata(row),"New document full-byte or metadata readback differs");
        if i==0 {
            for variant in ["old_kind","old_profile","both_b51","proof","omit","native","approval","owner","year"] {
                let mut bad=value.clone();match variant {
                    "old_kind"=>bad["retained_office_provenance"]["kind"]=json!("retained_office_T1_document"),
                    "old_profile"=>bad["retained_office_provenance"]["profile_sha256"]=json!("b51cc8aec7d930f6e5b4d05575538f87856a39c600f5893a0828aff96816812e"),
                    "both_b51"=>{bad["retained_office_provenance"]["kind"]=json!("retained_office_T1_document");bad["retained_office_provenance"]["profile_sha256"]=json!("b51cc8aec7d930f6e5b4d05575538f87856a39c600f5893a0828aff96816812e");},
                    "proof"=>bad["retained_office_provenance"]["owner_year_evidence_sha256"]=json!("0".repeat(64)),
                    "omit"=>{bad.as_object_mut().unwrap().remove("retained_office_provenance");},
                    "native"=>bad["retained_office_provenance"]["native_generation_verified"]=json!(true),
                    "approval"=>bad["retained_office_provenance"]["financial_values_approved"]=json!(true),
                    "owner"=>bad["client_number"]=profiles.iter().find(|p|p["client_number"]!=row["client_number"]).unwrap()["client_number"].clone(),
                    _=>bad["tax_year"]=json!(1990)
                }
                assert!(s.test_copy_preserve_return_pdf(company,serde_json::from_value(bad).unwrap()).await.is_err(),"Unapproved alternate stable-identifier descriptor accepted");
            }
        }
        assert!(std::fs::read(file).unwrap()==bytes,"New source PDF changed");
    }
    // Preserve the exact two-record method before adding the separate scan.
    for row in rows {let key=record_key(company,row);snapshots.push((key.clone(),store.get(&key).await.unwrap().unwrap()));}
    let scan_packet=std::env::var("NXT_DTMAX_SCANNED_OFFICE_PACKET").unwrap();let scan_packet=Path::new(&scan_packet);
    let scan_manifest:Value=serde_json::from_slice(&std::fs::read(scan_packet).unwrap()).unwrap();let scan_rows=scan_manifest["records"].as_array().unwrap();assert_eq!(scan_rows.len(),1);
    for row in scan_rows {
        let file=scan_packet.parent().unwrap().join(row["file"].as_str().unwrap());let bytes=std::fs::read(&file).unwrap();
        assert_eq!(bytes.len(),1034307);assert_eq!(row["page_labels"].as_array().unwrap().len(),4);
        assert!(digest(&bytes)==row["sha256"].as_str().unwrap(),"Scanned source PDF hash differs");
        let value=input(row,&bytes);let result=s.test_copy_preserve_return_pdf(company,serde_json::from_value(value.clone()).unwrap()).await.unwrap();assert_eq!(result["created"],true);assert!(result["record"]==metadata(row),"Scanned document metadata differs");
        assert_eq!(s.test_copy_preserve_return_pdf(company,serde_json::from_value(value.clone()).unwrap()).await.unwrap()["created"],false);
        let read=s.test_copy_return_pdf_content(company,row["client_number"].as_str().unwrap(),row["tax_year"].as_u64().unwrap()as u16,row["sha256"].as_str().unwrap()).await.unwrap();assert!(read["content_hex"]==hex::encode(&bytes) && read["record"]==metadata(row),"Scanned full-byte or metadata readback differs");
        for variant in ["b51_method","stable_method","omit","proof","native","approval","year","pages"] {
            let mut bad=value.clone();match variant {
                "b51_method"=>{bad["retained_office_provenance"]["kind"]=json!("retained_office_T1_document");bad["retained_office_provenance"]["profile_sha256"]=json!("b51cc8aec7d930f6e5b4d05575538f87856a39c600f5893a0828aff96816812e");},
                "stable_method"=>{bad["retained_office_provenance"]["kind"]=json!("retained_office_T1_stable_identifier_document");bad["retained_office_provenance"]["profile_sha256"]=json!("8b45d6655423ff1f2e0f8202a1009f25b1e472c73f7b7591b581a73264f42028");},
                "omit"=>{bad.as_object_mut().unwrap().remove("retained_office_provenance");},
                "proof"=>bad["retained_office_provenance"]["owner_year_evidence_sha256"]=json!("0".repeat(64)),
                "native"=>bad["retained_office_provenance"]["native_generation_verified"]=json!(true),
                "approval"=>bad["retained_office_provenance"]["financial_values_approved"]=json!(true),
                "year"=>bad["tax_year"]=json!(1990),
                _=>bad["page_labels"]=json!(["Changed scan index"])
            }
            assert!(s.test_copy_preserve_return_pdf(company,serde_json::from_value(bad).unwrap()).await.is_err(),"Scanned document accepted under an unclosed descriptor or method");
        }
        assert!(std::fs::read(file).unwrap()==bytes,"Scanned source file changed");
    }
    for row in rows {
        let bytes=std::fs::read(packet.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();let read=s.test_copy_return_pdf_content(company,row["client_number"].as_str().unwrap(),row["tax_year"].as_u64().unwrap()as u16,row["sha256"].as_str().unwrap()).await.unwrap();assert!(read["content_hex"]==hex::encode(&bytes) && read["record"]==metadata(row),"Original two-record method bytes or metadata changed");
    }
    for row in original_rows {
        let bytes=std::fs::read(original.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();
        let read=s.test_copy_return_pdf_content(company,row["client_number"].as_str().unwrap(),row["tax_year"].as_u64().unwrap()as u16,row["sha256"].as_str().unwrap()).await.unwrap();
        assert!(read["content_hex"]==hex::encode(&bytes) && read["record"]==metadata(row),"Original177 PDF bytes or public metadata changed");
    }
    for (key,raw) in snapshots {assert!(store.get(&key).await.unwrap().as_deref()==Some(&raw),"Existing profile or PDF metadata body changed");}
    assert!(controls(&s,company).await==before,"Source controls or printed-figure summary changed");
    println!("COMBINED_OFFICE_EXACT: stable_documents=2; stable_bytes={added_bytes}; scan_documents=1; scan_bytes=1034307; original177_bytes=91497591; legacy_metadata=2593; profiles=5856; seven_controls_unchanged=true");
}
