//! Closed recovered native SOURCE observations, separate from original history.
use super::preserved_records::SourceFactPage;
use super::{bad, conflict, storage, ImportError, SourceImport};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, ImportError>;
const MAX_PACKET: usize = 3 * 1024 * 1024;
const MAX_RESPONSE: usize = 3 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveredNativeBatch {
    pub test_copy: bool,
    #[serde(default)] pub contexts: Vec<Value>,
    #[serde(default)] pub supporting_records: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveredNativeLookup { pub test_copy: bool, pub context_ids: Vec<String> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveredNativePage {
    #[serde(default)] pub offset: usize,
    #[serde(default = "one")] pub limit: usize,
    pub source_return_year: Option<u16>,
    #[serde(default, rename = "path")] pub _routing_path: Option<String>,
}
fn one() -> usize { 1 }

#[derive(Clone, Deserialize)]
struct ClosedProfile {
    schema: String, profile_sha256: String, contract: Value,
    context_sha256: BTreeSet<String>, supporting_record_sha256: BTreeSet<String>,
    member_record_sha256: BTreeSet<String>,
    #[serde(default)] context_index_sha256: BTreeSet<String>, expected_counts: Value,
}
fn digest(v: &Value) -> String { hex::encode(Sha256::digest(serde_json::to_vec(v).expect("JSON value"))) }
fn bytes_sha(v: &[u8]) -> String { hex::encode(Sha256::digest(v)) }
fn profile() -> &'static ClosedProfile {
    static P: OnceLock<ClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let p: ClosedProfile = serde_json::from_str(include_str!("recovered_native_source_profile.json")).expect("closed recovered-source profile");
        assert_eq!(p.schema,"recovered-native-source-closed-profile/v1");
        assert_eq!(digest(&p.contract),p.profile_sha256);
        assert_eq!(p.context_sha256.len(),p.expected_counts["contexts"].as_u64().unwrap() as usize);
        assert_eq!(p.supporting_record_sha256.len(),p.expected_counts["supporting_records"].as_u64().unwrap() as usize);
        assert_eq!(p.member_record_sha256.len(),p.expected_counts["member_records"].as_u64().unwrap() as usize);
        assert_eq!(p.contract["expected_counts"],p.expected_counts);
        p
    })
}
fn text<'a>(v: &'a Value,k: &str) -> Result<&'a str> { v[k].as_str().ok_or_else(||bad("Recovered source text field missing")) }
fn uint(v: &Value,k: &str) -> Result<u64> { v[k].as_u64().ok_or_else(||bad("Recovered source integer field missing")) }
fn sha(s: &str) -> bool { s.len()==64 && s.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn number(s: &str) -> bool { s.parse::<u32>().is_ok_and(|n|n>0 && n.to_string()==s) }
fn json_has_float(v: &Value) -> bool {
    match v { Value::Number(n)=>n.is_f64(), Value::Array(a)=>a.iter().any(json_has_float), Value::Object(m)=>m.values().any(json_has_float), _=>false }
}
fn crc32(v: &[u8]) -> u32 {
    let mut crc=0xffff_ffffu32;
    for b in v { crc^=*b as u32;for _ in 0..8 {crc=(crc>>1)^if crc&1==1{0xedb8_8320}else{0};} }
    !crc
}
fn hundredths(n: i64) -> String {
    let a=(n as i128).abs();let sign=if n<0{"-"}else{""};let rem=a%100;
    if rem==0 {format!("{sign}{}",a/100)} else if rem%10==0 {format!("{sign}{}.{}",a/100,rem/10)} else {format!("{sign}{}.{rem:02}",a/100)}
}
fn counts(records: &[Value]) -> Result<Value> {
    let (mut forms,mut q,mut e11,mut fields)=(0usize,0usize,0usize,0usize);
    for r in records {
        if let Some(fs)=r["typed"]["forms"].as_array() {
            forms+=fs.len();
            for f in fs {q+=f["fields"].as_array().ok_or_else(||bad("Recovered form fields missing"))?.len();}
        }
        let es=r["e11_source_fields"].as_array().ok_or_else(||bad("Recovered E11 source list missing"))?;
        e11+=es.len();for e in es {fields+=e["fields"].as_array().ok_or_else(||bad("Recovered E11 fields missing"))?.len();}
    }
    Ok(json!({"records":records.len(),"forms":forms,"Q":q,"E11_forms":e11,"E11_field_instances":fields}))
}
fn validate_record(r: &Value,role: &str,p: &ClosedProfile) -> Result<()> {
    let approved=if role=="member"{&p.member_record_sha256}else{&p.supporting_record_sha256};
    if r["schema"]!="recovered-native-source-record/v1" || r["source_record_role"]!=role || !approved.contains(&digest(r))
        || json_has_float(r) {return Err(bad("Recovered source record is outside the closed byte/typed profile"));}
    let id=text(r,"record_id")?;let source=&r["source"];
    if !sha(id) || !sha(text(source,"source_file_sha256")?) || !sha(text(source,"directory_entry_sha256")?)
        || !(1990..=2100).contains(&uint(source,"source_year")?) {return Err(bad("Recovered source identity differs"));}
    let identity=json!({"source_file_sha256":source["source_file_sha256"],"source_year":source["source_year"],
        "section_id":source["section_id"],"source_stored_offset":source["source_stored_offset"],
        "source_stored_bytes":source["source_stored_bytes"],"record_type":source["record_type"]});
    if digest(&identity)!=id {return Err(bad("Recovered native record identity is not its exact source coordinates"));}
    let header=STANDARD.decode(text(r,"native_header_base64")?).map_err(|_|bad("Invalid native header base64"))?;
    let body=STANDARD.decode(text(r,"body_base64")?).map_err(|_|bad("Invalid native body base64"))?;
    if header.len()!=18 || body.len()>0x7d000 || body.len() as u64!=uint(r,"body_bytes")?
        || bytes_sha(&body)!=text(r,"body_sha256")? || bytes_sha(&header)!=text(source,"header_sha256")? {
        return Err(bad("Recovered raw body or source header hash differs"));
    }
    let expanded=u16::from_le_bytes([header[2],header[3]]) as usize | (header[7] as usize)<<16;
    let stored=u16::from_le_bytes([header[4],header[5]]) as u64;
    let crc=u32::from_le_bytes(header[14..18].try_into().unwrap());
    if expanded!=body.len() || stored.checked_add(18)!=Some(uint(source,"source_stored_bytes")?)
        || u16::from_le_bytes([header[0],header[1]]) as u64!=uint(source,"record_type")?
        || header[6]!=1 || header[8]!=17 || crc32(&body)!=crc || text(r,"expanded_body_crc32")?!=format!("{crc:08x}") {
        return Err(bad("Recovered source declared length, selector or CRC differs"));
    }
    if let Some(fs)=r["typed"]["forms"].as_array() {
        let mut reconstructed=hex::decode(text(&r["typed"],"prefix_hex")?).map_err(|_|bad("Invalid native typed prefix"))?;
        for t in r["typed"]["tokens"].as_array().ok_or_else(||bad("Native typed token stream missing"))? {
            reconstructed.extend(hex::decode(text(t,"raw_hex")?).map_err(|_|bad("Invalid native typed token"))?);
        }
        if reconstructed!=body || r["typed"]["byte_reconstruction_exact"]!=true {return Err(bad("Typed native source does not reconstruct its entire raw body"));}
        for f in fs {for q in f["fields"].as_array().ok_or_else(||bad("Typed native fields missing"))? {
            if q["Y"]==2 {
                let cents=q["wire_integer"].as_i64().ok_or_else(||bad("Native hundredths integer missing"))?;
                if text(q,"exact_cents")?!=cents.to_string() || text(q,"wire_exact_decimal")?!=hundredths(cents) {
                    return Err(bad("Exact native hundredths were rounded or changed"));
                }
            }
        }}
    }
    Ok(())
}
fn validate_context(c: &Value,p: &ClosedProfile) -> Result<()> {
    if serde_json::to_vec(c).map_err(|_|bad("Recovered context encoding failed"))?.len()>MAX_PACKET
        || c["schema"]!="recovered-native-source-context/v1" || !p.context_sha256.contains(&digest(c)) || json_has_float(c)
        || c["reader_module"]!="T1" || c["source_only"]!=true || c["financial_values_approved"]!=false || c["recalculated"]!=false
        || !sha(text(c,"context_id")?) || !sha(text(c,"mapping_evidence_sha256")?) || !sha(text(c,"source_identity_evidence_sha256")?) {
        return Err(bad("Recovered context differs from the complete closed packet"));
    }
    let source=&c["source_identity"];let year=uint(c,"source_year")?;
    if !(1990..=2100).contains(&year) || !sha(text(source,"source_file_sha256")?)
        || !source["source_native_client_id"].is_null() && !source["source_native_client_id"].as_str().is_some_and(number)
        || !source["source_family_file_id"].is_null() && !source["source_family_file_id"].as_str().is_some_and(number) {
        return Err(bad("Recovered original source identity fields differ"));
    }
    match c["client_number"].as_str() {
        Some(client) if number(client) && c["target_source_profile_sha256"].as_str().is_some_and(sha)=>{},
        None if c["client_number"].is_null() && c["target_source_profile_sha256"].is_null()=>{},
        _=>return Err(bad("Recovered optional SOURCE target differs")),
    }
    let records=c["records"].as_array().ok_or_else(||bad("Recovered member records missing"))?;
    if records.is_empty() || records.len()>p.member_record_sha256.len() {return Err(bad("Recovered member record count differs"));}
    let mut ids=BTreeSet::new();
    for r in records {
        validate_record(r,"member",p)?;
        if !ids.insert(text(r,"record_id")?) || r["source"]["source_file_sha256"]!=source["source_file_sha256"]
            || r["source"]["source_member_ordinal"]!=source["native_member_ordinal"] || r["source"]["source_year"]!=year {
            return Err(bad("Recovered member context does not match its original source coordinates"));
        }
    }
    if c["counts"]!=counts(records)? {return Err(bad("Recovered context typed counts differ"));}
    let refs=c["supporting_record_ids"].as_array().ok_or_else(||bad("Recovered support references missing"))?;
    let mut seen=BTreeSet::new();for r in refs {
        let id=r.as_str().ok_or_else(||bad("Invalid support record reference"))?;
        if !sha(id) || !seen.insert(id) {return Err(bad("Duplicate or invalid support record reference"));}
    }
    Ok(())
}
fn context_meta(c: &Value) -> Value {
    json!({"context_id":c["context_id"],"client_number":c["client_number"],"source_year":c["source_year"],
        "source_identity":c["source_identity"],"counts":c["counts"],"context_sha256":digest(c),
        "target_source_profile_sha256":c["target_source_profile_sha256"]})
}
fn validate_meta(m: &Value,p: &ClosedProfile) -> Result<()> {
    if !p.context_index_sha256.contains(&digest(m)) || !sha(text(m,"context_id")?)
        || !p.context_sha256.contains(text(m,"context_sha256")?) {return Err(storage("Recovered source metadata index differs from the closed profile"));}Ok(())
}
fn key(company: &str,p: &ClosedProfile,rest: &str) -> String {format!("dtmaxtc:{company}:recovered_native_sources:{}:{rest}",p.profile_sha256)}
fn base_key(company: &str,rest: &str) -> String {format!("dtmaxtc:{company}:{rest}")}
fn envelope(company: &str,p: &ClosedProfile) -> Value {
    json!({"test_copy":true,"source_scope":company,"kind":"recovered-native-sources","recovered_native_source_profile_sha256":p.profile_sha256,
        "vendor_activation_required":false,"source_only":true,"current_profiles_modified":0,"native_contexts_created":0,
        "earlier_figures_or_slips_modified":0,"tax_cases_imported":0,"financial_values_approved":false,
        "full_all_plan_native_decode_claimed":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    fn page(offset:usize)->RecoveredNativePage {RecoveredNativePage{offset,limit:1,source_return_year:None,_routing_path:None}}
    fn fixture()->(ClosedProfile,Value) {
        let mut p=profile().clone();
        let body=b"synthetic";let mut h=[0u8;18];h[0]=7;h[2]=body.len() as u8;h[4]=9;h[6]=1;h[8]=17;h[14..18].copy_from_slice(&crc32(body).to_le_bytes());
        let source=json!({"source_file_sha256":"a".repeat(64),"source_year":2009,"section_id":7,"source_stored_offset":192,"source_stored_bytes":27,"record_type":7,"source_member_ordinal":1,"source_plan":0,"directory_slot":0,"directory_entry_offset":144,"directory_entry_sha256":"b".repeat(64),"header_sha256":bytes_sha(&h)});
        let identity=json!({"source_file_sha256":source["source_file_sha256"],"source_year":source["source_year"],"section_id":source["section_id"],"source_stored_offset":source["source_stored_offset"],"source_stored_bytes":source["source_stored_bytes"],"record_type":source["record_type"]});
        let r=json!({"schema":"recovered-native-source-record/v1","record_id":digest(&identity),"source_record_role":"member","source":source,"native_header_base64":STANDARD.encode(h),"body_base64":STANDARD.encode(body),"body_sha256":bytes_sha(body),"body_bytes":body.len(),"expanded_body_crc32":format!("{:08x}",crc32(body)),"typed":null,"e11_source_fields":[]});
        let c=json!({"schema":"recovered-native-source-context/v1","context_id":"c".repeat(64),"client_number":null,"source_year":2009,"reader_module":"T1","source_identity":{"source_native_client_id":null,"source_family_file_id":null,"native_member_ordinal":1,"source_file_sha256":"a".repeat(64)},"target_source_profile_sha256":null,"records":[r],"supporting_record_ids":[],"mapping_evidence_sha256":"d".repeat(64),"source_identity_evidence_sha256":"e".repeat(64),"counts":{"records":1,"forms":0,"Q":0,"E11_forms":0,"E11_field_instances":0},"source_only":true,"financial_values_approved":false,"recalculated":false});
        p.context_sha256=BTreeSet::from([digest(&c)]);p.member_record_sha256=BTreeSet::from([digest(&c["records"][0])]);p.context_index_sha256=BTreeSet::from([digest(&context_meta(&c))]);
        (p,c)
    }
    #[test]
    fn closed_profile_crc_and_signed_hundredths_are_exact() {
        let p=profile();assert_eq!(p.context_index_sha256.len(),p.context_sha256.len());
        assert_eq!(p.contract["context_sha256"],json!(p.context_sha256));
        assert_eq!(crc32(b"123456789"),0xcbf43926);assert_eq!(crc32(b""),0);
        for (n,s) in [(0,"0"),(-1,"-0.01"),(10,"0.1"),(110,"1.1"),(-12345,"-123.45"),(i64::MIN,"-92233720368547758.08")] {assert_eq!(hundredths(n),s);}
        let(p,c)=fixture();validate_context(&c,&p).unwrap();
        let mut changed=c.clone();changed["source_year"]=json!(2010);assert!(validate_context(&changed,&p).is_err());
        let mut r=c["records"][0].clone();r["body_base64"]=json!(STANDARD.encode(b"Synthetic"));
        let mut approved=p.clone();approved.member_record_sha256.insert(digest(&r));assert!(validate_record(&r,"member",&approved).is_err());
    }
    #[tokio::test]
    async fn synthetic_batches_prevalidate_and_preserve_immutable_packets() {
        let(p,c)=fixture();let store=Store::memory();let s=SourceImport::new(store.clone());let company="dtmax-test-copy";
        let mut changed=c.clone();changed["client_number"]=json!("123");
        assert!(s.preserve_recovered_native(company,RecoveredNativeBatch{test_copy:true,contexts:vec![c.clone(),changed],supporting_records:vec![]},&p).await.is_err());
        assert!(store.smembers(&key(company,&p,"contexts")).await.unwrap().is_empty());
        assert!(s.preserve_recovered_native("another-test-copy",RecoveredNativeBatch{test_copy:true,contexts:vec![c.clone()],supporting_records:vec![]},&p).await.is_err());
        let b=RecoveredNativeBatch{test_copy:true,contexts:vec![c.clone()],supporting_records:vec![]};
        assert_eq!(s.preserve_recovered_native(company,b.clone(),&p).await.unwrap()["created"],1);
        assert_eq!(s.preserve_recovered_native(company,b.clone(),&p).await.unwrap()["unchanged_replays"],1);
        assert_eq!(s.recovered_page(company,"unassigned",None,page(0),&p).await.unwrap()["records"][0],c);
        let k=key(company,&p,&format!("context:{}",c["context_id"].as_str().unwrap()));
        store.set(&k,"{}").await.unwrap();assert!(s.preserve_recovered_native(company,b,&p).await.is_err());assert_eq!(store.get(&k).await.unwrap().as_deref(),Some("{}"));
    }
    // This test is executed locally with the frozen private corpus. No client
    // data or credentials are compiled into the repository or emitted to logs.
    #[tokio::test]
    async fn private_all_contexts_are_lossless_and_original_source_rows_unchanged() {
        let Ok(directory)=std::env::var("NXT_RECOVERED_NATIVE_PACKET_DIR") else{return};
        let root=std::path::Path::new(&directory);let p=profile();let company="dtmax-test-copy";
        let store=Store::memory();let s=SourceImport::new(store.clone());let mut before=Vec::new();
        let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();
        assert_eq!(digest(&profiles["clients"]),p.contract["required_clients_canonical_sha256"]);
        for row in profiles["clients"].as_array().unwrap(){let n=row["client_number"].as_str().unwrap();let k=base_key(company,&format!("client:{n}"));let raw=row.to_string();store.set(&k,&raw).await.unwrap();store.sadd(&base_key(company,"clients"),n).await.unwrap();before.push((k,raw));}
        // Preserve raw original native-history, printed/cached and PDF-source
        // observations under their existing independent source namespaces.
        for (env,kind) in [("NXT_PRESERVED_NATIVE_FIXTURE","historical_lines"),("NXT_PRESERVED_CACHED_FIXTURE","printed_source_figures"),("NXT_PRESERVED_PDF_OBSERVATIONS","preserved_pdf_figures")] {
            let raw=std::fs::read(std::env::var(env).unwrap()).unwrap();let mut count=0;
            for (i,line) in raw.split(|b|*b==b'\n').filter(|l|!l.is_empty()).enumerate(){let row:Value=serde_json::from_slice(line).unwrap();let id=if kind=="historical_lines"{format!("{}:{}:{}",row["client_number"].as_str().unwrap(),row["tax_year"].as_u64().unwrap(),row["archive_sha256"].as_str().unwrap())}else{format!("fixture-{i}")};let k=if kind=="historical_lines"{base_key(company,&format!("{kind}:{id}"))}else{let profile:Value=serde_json::from_str(if kind=="printed_source_figures"{include_str!("printed_cached_figure_profile.json")}else{include_str!("preserved_pdf_figure_profile.json")}).unwrap();let record_id=format!("{}:{}:{}",row["client_number"].as_str().unwrap(),row["source_return_year"].as_u64().unwrap(),row["pdf"]["sha256"].as_str().unwrap());base_key(company,&format!("{}:{}:record:{record_id}",if kind=="printed_source_figures"{"printed_cached_figures"}else{"preserved_pdf_figures"},profile["profile_sha256"].as_str().unwrap()))};let text=String::from_utf8(line.to_vec()).unwrap();store.set(&k,&text).await.unwrap();before.push((k,text));count+=1;}
            if kind=="historical_lines"{assert_eq!(count,32744);}
        }
        let readbatch=|file: &std::path::Path|->RecoveredNativeBatch{serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()};
        let support=readbatch(&root.join("requests/supporting-evidence.private.json"));
        assert_eq!(support.supporting_records.len(),p.supporting_record_sha256.len());
        s.test_copy_recovered_native_sources(company,support.clone()).await.unwrap();
        let mut files:Vec<_>=std::fs::read_dir(root.join("contexts")).unwrap().map(|f|f.unwrap().path()).collect();files.sort();
        let mut originals=Vec::new();let mut members=0usize;
        for file in files {let c:Value=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();validate_context(&c,p).unwrap();let id=c["context_id"].as_str().unwrap();let request=root.join("requests").join(format!("context-{id}.private.json"));assert!(std::fs::metadata(&request).unwrap().len()<=MAX_PACKET as u64);let b=readbatch(&request);assert!(b.contexts[0]==c,"Request packet differs from frozen context");let result=s.test_copy_recovered_native_sources(company,b.clone()).await.unwrap();assert_eq!(result["created"],1);assert_eq!(s.test_copy_recovered_native_sources(company,b).await.unwrap()["unchanged_replays"],1);let found=s.test_copy_read_recovered_native_sources(company,RecoveredNativeLookup{test_copy:true,context_ids:vec![id.into()]}).await.unwrap();assert!(found["records"][0]==c,"Exact recovered readback differs");members+=c["records"].as_array().unwrap().len();originals.push(c);}
        assert_eq!(originals.len(),p.context_sha256.len());assert_eq!(members,p.member_record_sha256.len());
        let sum=s.test_copy_recovered_native_source_summary(company).await.unwrap();
        for (got,expected) in [("source_contexts","contexts"),("member_records","member_records"),("supporting_records","supporting_records"),("assigned_contexts","assigned_contexts"),("unassigned_contexts","unassigned_contexts"),("forms","forms"),("Q","Q"),("E11_field_instances","E11_field_instances")]{assert_eq!(sum[got],p.expected_counts[expected]);}
        let mut queued=BTreeSet::new();for i in 0..p.expected_counts["unassigned_contexts"].as_u64().unwrap() as usize {let row=s.test_copy_unassigned_recovered_native_sources(company,page(i)).await.unwrap();assert_eq!(row["records"].as_array().unwrap().len(),1);assert!(row["records"][0]["client_number"].is_null());queued.insert(row["records"][0]["context_id"].as_str().unwrap().to_owned());}
        assert_eq!(queued.len(),p.expected_counts["unassigned_contexts"].as_u64().unwrap() as usize);
        let evidence=s.test_copy_recovered_native_evidence(company,RecoveredNativePage{offset:0,limit:100,source_return_year:None,_routing_path:None}).await.unwrap();
        let actual:BTreeSet<_>=evidence["records"].as_array().unwrap().iter().map(digest).collect();assert_eq!(actual,p.supporting_record_sha256);
        let mut changed=originals[0].clone();changed["source_year"]=json!(1999);assert!(s.test_copy_recovered_native_sources(company,RecoveredNativeBatch{test_copy:true,contexts:vec![originals[1].clone(),changed],supporting_records:vec![]}).await.is_err());
        let target=originals.iter().find(|c|!c["client_number"].is_null()).unwrap();let n=target["client_number"].as_str().unwrap();let k=base_key(company,&format!("client:{n}"));let prior=store.get(&k).await.unwrap().unwrap();let mut changed:Value=serde_json::from_str(&prior).unwrap();changed["name"]=json!("SYNTHETIC CHANGED TARGET");store.set(&k,&changed.to_string()).await.unwrap();assert!(s.test_copy_recovered_native_sources(company,RecoveredNativeBatch{test_copy:true,contexts:vec![target.clone()],supporting_records:vec![]}).await.is_err());store.set(&k,&prior).await.unwrap();
        let years=s.recovered_native_source_years(company).await.unwrap();for c in &originals {if let Some(n)=c["client_number"].as_str(){assert!(years[n].contains(&(c["source_year"].as_u64().unwrap() as u16)));}}
        for group in before.chunks(250){let keys:Vec<_>=group.iter().map(|(k,_)|k.clone()).collect();let rows=store.mget(&keys).await.unwrap();for (actual,(_,expected)) in rows.iter().zip(group){assert!(actual.as_deref()==Some(expected.as_str()),"An original source row changed");}}
        let mut after=Vec::new();for row in profiles["clients"].as_array().unwrap(){let n=row["client_number"].as_str().unwrap();after.push(serde_json::from_str::<Value>(&store.get(&base_key(company,&format!("client:{n}"))).await.unwrap().unwrap()).unwrap());}
        assert_eq!(digest(&json!(after)),p.contract["required_clients_canonical_sha256"]);
        eprintln!("PRIVATE_RECOVERED_NATIVE_REPLAY_EXECUTED contexts={} member={} supporting={} baseline_rows={} assigned={} queue={}",originals.len(),members,support.supporting_records.len(),before.len(),sum["assigned_contexts"],sum["unassigned_contexts"]);
    }
}
impl SourceImport {
    fn recovered_guard(&self,company: &str,flag: bool,p: &ClosedProfile) -> Result<()> {
        self.test_copy_guard(company,flag)?;
        if p.contract["source_scope"]!=company {return Err(conflict("Recovered source profile is closed to its exact SOURCE company"));}
        Ok(())
    }
    async fn recovered_target_anchor(&self,company: &str,c: &Value,p: &ClosedProfile) -> Result<()> {
        let Some(client)=c["client_number"].as_str() else{return Ok(())};
        let registered=self.store.smembers(&base_key(company,"clients")).await.map_err(|e|storage(&e))?;
        if registered.len() as u64!=p.contract["required_client_count"].as_u64().unwrap_or(0) || !registered.iter().any(|n|n==client) {
            return Err(conflict("Recovered target is absent from the pinned SOURCE client roster"));
        }
        let raw=self.store.get(&base_key(company,&format!("client:{client}"))).await.map_err(|e|storage(&e))?.ok_or_else(||conflict("Recovered SOURCE target profile missing"))?;
        let mut body:Value=serde_json::from_str(&raw).map_err(|_|storage("SOURCE client profile unreadable"))?;
        if body["client_number"]==client && digest(&body)==text(c,"target_source_profile_sha256")? {return Ok(())}
        let mut years=BTreeSet::<u16>::new();
        for id in self.store.smembers(&base_key(company,"returns")).await.map_err(|e|storage(&e))? {
            if let Some((n,y))=id.split_once(':') {if n==client {if let Ok(y)=y.parse(){years.insert(y);}}}
        }
        body["return_years"]=json!(years);
        if body["client_number"]!=client || digest(&body)!=text(c,"target_source_profile_sha256")? {
            return Err(conflict("Recovered SOURCE target no longer matches its exact pinned F3 profile"));
        }
        Ok(())
    }
    async fn recovered_value(&self,k: &str) -> Result<Option<Value>> {
        self.store.get(k).await.map_err(|e|storage(&e))?.map(|raw|serde_json::from_str(&raw).map_err(|_|storage("Recovered source JSON corrupt"))).transpose()
    }
    pub async fn test_copy_recovered_native_sources(&self,company: &str,batch: RecoveredNativeBatch) -> Result<Value> {
        self.preserve_recovered_native(company,batch,profile()).await
    }
    async fn preserve_recovered_native(&self,company: &str,batch: RecoveredNativeBatch,p: &ClosedProfile) -> Result<Value> {
        self.recovered_guard(company,batch.test_copy,p)?;
        if batch.contexts.len()>p.context_sha256.len() || batch.supporting_records.len()>p.supporting_record_sha256.len()
            || batch.contexts.is_empty() && batch.supporting_records.is_empty() {return Err(bad("Recovered source batch count is outside the closed profile"));}
        let mut support=BTreeMap::new();let mut context_ids=BTreeSet::new();
        for r in &batch.supporting_records {
            validate_record(r,"supporting",p)?;
            if support.insert(text(r,"record_id")?.to_owned(),r).is_some() {return Err(bad("Duplicate supporting source record"));}
        }
        // All validation and immutability checks precede the first write.
        for c in &batch.contexts {
            validate_context(c,p)?;validate_meta(&context_meta(c),p)?;self.recovered_target_anchor(company,c,p).await?;
            if !context_ids.insert(text(c,"context_id")?) {return Err(bad("Duplicate recovered source context"));}
            let meta=context_meta(c);
            if self.store.get(&key(company,p,&format!("meta:{}",text(c,"context_id")?))).await.map_err(|e|storage(&e))?
                .is_some_and(|old|old!=meta.to_string()) {return Err(conflict("Recovered source metadata is immutable"));}
            for id in c["supporting_record_ids"].as_array().unwrap() {
                let id=id.as_str().unwrap();if !support.contains_key(id) {
                    let r=self.recovered_value(&key(company,p,&format!("support:{id}"))).await?.ok_or_else(||conflict("Import original-year supporting evidence before its context"))?;
                    validate_record(&r,"supporting",p)?;
                    if text(&r,"record_id")?!=id {return Err(storage("Supporting record identity differs"));}
                }
            }
        }
        for (kind,values) in [("support",&batch.supporting_records),("context",&batch.contexts)] {
            for v in values {
                let id=text(v,if kind=="context"{"context_id"}else{"record_id"})?;
                if self.store.get(&key(company,p,&format!("{kind}:{id}"))).await.map_err(|e|storage(&e))?
                    .is_some_and(|old|old!=v.to_string()) {return Err(conflict("Recovered source packets are immutable"));}
            }
        }
        let(mut created,mut unchanged)=(0usize,0usize);
        for(kind,values)in[("support",&batch.supporting_records),("context",&batch.contexts)] {
            for v in values {
                let id=text(v,if kind=="context"{"context_id"}else{"record_id"})?;let k=key(company,p,&format!("{kind}:{id}"));let body=v.to_string();
                let fresh=self.store.set_nx(&k,&body).await.map_err(|e|storage(&e))?;
                if self.store.get(&k).await.map_err(|e|storage(&e))?.as_deref()!=Some(&body) {return Err(conflict("Recovered source packet changed concurrently"));}
                self.store.sadd(&key(company,p,if kind=="context"{"contexts"}else{"supporting"}),id).await.map_err(|e|storage(&e))?;
                if kind=="context" {
                    let meta=context_meta(v);let mk=key(company,p,&format!("meta:{id}"));let mb=meta.to_string();
                    self.store.set_nx(&mk,&mb).await.map_err(|e|storage(&e))?;
                    if self.store.get(&mk).await.map_err(|e|storage(&e))?.as_deref()!=Some(&mb){return Err(conflict("Recovered source metadata is immutable"));}
                    let target=v["client_number"].as_str().map(|n|format!("client:{n}")).unwrap_or_else(||"unassigned".into());
                    self.store.sadd(&key(company,p,&target),id).await.map_err(|e|storage(&e))?;
                }
                if fresh {created+=1}else{unchanged+=1}
            }
        }
        let mut out=envelope(company,p);out["created"]=json!(created);out["unchanged_replays"]=json!(unchanged);
        out["submitted_contexts"]=json!(batch.contexts.len());out["submitted_supporting_records"]=json!(batch.supporting_records.len());Ok(out)
    }
    async fn recovered_context_read(&self,company: &str,id: &str,p: &ClosedProfile) -> Result<Option<Value>> {
        if !sha(id){return Err(bad("Recovered context ID must be a lowercase SHA256"));}
        let value=self.recovered_value(&key(company,p,&format!("context:{id}"))).await?;
        if let Some(c)=&value {validate_context(c,p)?;if text(c,"context_id")?!=id{return Err(storage("Stored recovered context ID differs"));}self.recovered_target_anchor(company,c,p).await?;}
        Ok(value)
    }
    pub async fn test_copy_read_recovered_native_sources(&self,company: &str,lookup: RecoveredNativeLookup) -> Result<Value> {
        let p=profile();self.recovered_guard(company,lookup.test_copy,p)?;
        if lookup.context_ids.len()!=1 {return Err(bad("Read one recovered context per request to retain the bounded raw/typed body"));}
        let mut out=envelope(company,p);let c=self.recovered_context_read(company,&lookup.context_ids[0],p).await?;
        out["context_ids"]=json!(lookup.context_ids);out["records"]=json!([c]);Ok(out)
    }
    /// Internal bounded batch for the optional identity overlay. Every original
    /// packet is fully checked here; its live target anchor is checked against
    /// the overlay request's single shared roster/profile/year snapshot.
    pub(super) async fn recovered_identity_originals(&self,company:&str,ids:&BTreeSet<String>)->Result<BTreeMap<String,Value>>{
        let p=profile();self.recovered_guard(company,true,p)?;
        if ids.len()>p.context_sha256.len()||ids.iter().any(|id|!sha(id)){return Err(bad("Invalid identity original-context batch"));}
        let keys:Vec<_>=ids.iter().map(|id|key(company,p,&format!("context:{id}"))).collect();
        let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if rows.len()!=ids.len(){return Err(storage("Identity original read cardinality differs"));}
        let mut out=BTreeMap::new();for(id,raw)in ids.iter().zip(rows){
            let c:Value=serde_json::from_str(&raw.ok_or_else(||conflict("Original recovered context must be received first"))?).map_err(|_|storage("Original recovered context corrupt"))?;
            validate_context(&c,p)?;if c["context_id"]!=id.as_str(){return Err(storage("Identity original context ID differs"));}out.insert(id.clone(),c);
        }Ok(out)
    }
    async fn recovered_page(&self,company: &str,index: &str,client: Option<&str>,page: RecoveredNativePage,p: &ClosedProfile) -> Result<Value> {
        self.recovered_guard(company,true,p)?;
        if page.limit!=1 || page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)) {return Err(bad("Recovered source pages require limit=1 and an explicit valid source year when filtered"));}
        let mut ids=self.store.smembers(&key(company,p,index)).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>p.context_sha256.len(){return Err(storage("Recovered source index exceeds the closed profile"));}
        let mut selected=Vec::new();
        let keys:Vec<_>=ids.iter().map(|id|key(company,p,&format!("meta:{id}"))).collect();
        let values=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if values.len()!=ids.len(){return Err(storage("Recovered metadata read cardinality differs"));}
        for (id,raw) in ids.into_iter().zip(values) {
            let m:Value=serde_json::from_str(&raw.ok_or_else(||storage("Recovered source metadata missing"))?).map_err(|_|storage("Recovered metadata corrupt"))?;
            validate_meta(&m,p)?;
            if m["context_id"]!=id || client.is_some_and(|n|m["client_number"]!=n) || index=="unassigned" && !m["client_number"].is_null(){return Err(storage("Recovered source target index differs"));}
            if page.source_return_year.is_none_or(|y|m["source_year"]==y) {selected.push(id);}
        }
        let total=selected.len();let mut rows=Vec::new();
        for id in selected.into_iter().skip(page.offset).take(1) {
            rows.push(self.recovered_context_read(company,&id,p).await?.ok_or_else(||storage("Recovered source body missing"))?);
        }
        if serde_json::to_vec(&rows).map_err(|_|storage("Cannot encode recovered source page"))?.len()>MAX_RESPONSE {return Err(bad("Recovered raw context response exceeds bound"));}
        let mut out=envelope(company,p);out["records"]=json!(rows);out["total"]=json!(total);out["offset"]=json!(page.offset);out["limit"]=json!(1);out["source_return_year"]=json!(page.source_return_year);
        if let Some(client)=client {out["client_number"]=json!(client);}Ok(out)
    }
    pub async fn test_copy_unassigned_recovered_native_sources(&self,company: &str,page: RecoveredNativePage) -> Result<Value> {
        self.recovered_page(company,"unassigned",None,page,profile()).await
    }
    pub async fn test_copy_client_recovered_native_sources(&self,company: &str,client: &str,page: SourceFactPage) -> Result<Value> {
        if !number(client){return Err(bad("Invalid recovered SOURCE client number"));}
        self.recovered_page(company,&format!("client:{client}"),Some(client),RecoveredNativePage{offset:page.offset,limit:page.limit,source_return_year:page.source_return_year,_routing_path:None},profile()).await
    }
    pub async fn test_copy_recovered_native_evidence(&self,company: &str,page: RecoveredNativePage) -> Result<Value> {
        let p=profile();self.recovered_guard(company,true,p)?;
        if page.limit==0 || page.limit>100 || page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)){return Err(bad("Invalid recovered supporting-evidence page"));}
        let mut ids=self.store.smembers(&key(company,p,"supporting")).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>p.supporting_record_sha256.len(){return Err(storage("Supporting evidence index exceeds closed scope"));}
        let mut rows=Vec::new();for id in ids {
            let r=self.recovered_value(&key(company,p,&format!("support:{id}"))).await?.ok_or_else(||storage("Supporting evidence body missing"))?;
            validate_record(&r,"supporting",p)?;if text(&r,"record_id")?!=id{return Err(storage("Supporting evidence identity differs"));}
            if page.source_return_year.is_none_or(|y|r["source"]["source_year"]==y){rows.push(r);}
        }
        let total=rows.len();let rows:Vec<_>=rows.into_iter().skip(page.offset).take(page.limit).collect();
        if serde_json::to_vec(&rows).map_err(|_|storage("Cannot encode supporting evidence"))?.len()>MAX_RESPONSE{return Err(bad("Request fewer original-year supporting records"));}
        let mut out=envelope(company,p);out["kind"]=json!("recovered-native-supporting-evidence");out["records"]=json!(rows);out["total"]=json!(total);out["offset"]=json!(page.offset);out["limit"]=json!(page.limit);out["source_return_year"]=json!(page.source_return_year);Ok(out)
    }
    async fn recovered_metadata(&self,company: &str,p: &ClosedProfile) -> Result<Vec<Value>> {
        let mut ids=self.store.smembers(&key(company,p,"contexts")).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>p.context_sha256.len(){return Err(storage("Recovered context index exceeds closed scope"));}
        let keys:Vec<_>=ids.iter().map(|id|key(company,p,&format!("meta:{id}"))).collect();
        let raws=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if raws.len()!=ids.len(){return Err(storage("Recovered metadata cardinality differs"));}
        let mut rows=Vec::new();for (id,raw) in ids.into_iter().zip(raws) {
            let v:Value=serde_json::from_str(&raw.ok_or_else(||storage("Recovered source metadata missing"))?).map_err(|_|storage("Recovered metadata corrupt"))?;
            validate_meta(&v,p)?;if v["context_id"]!=id{return Err(storage("Recovered metadata ID differs"));}rows.push(v);
        }Ok(rows)
    }
    pub(super) async fn recovered_native_source_years(&self,company: &str) -> Result<BTreeMap<String,BTreeSet<u16>>> {
        let mut years=BTreeMap::<String,BTreeSet<u16>>::new();
        for c in self.recovered_metadata(company,profile()).await? {
            if let Some(client)=c["client_number"].as_str(){years.entry(client.into()).or_default().insert(uint(&c,"source_year")? as u16);}
        }Ok(years)
    }
    pub async fn test_copy_recovered_native_source_summary(&self,company: &str) -> Result<Value> {
        let p=profile();self.recovered_guard(company,true,p)?;
        let index=self.recovered_metadata(company,p).await?;
        let(mut member,mut forms,mut q,mut e11,mut assigned)=(0u64,0u64,0u64,0u64,0u64);
        for c in &index {
            member+=uint(&c["counts"],"records")?;forms+=uint(&c["counts"],"forms")?;q+=uint(&c["counts"],"Q")?;e11+=uint(&c["counts"],"E11_field_instances")?;assigned+=u64::from(!c["client_number"].is_null());
        }
        let support=self.store.smembers(&key(company,p,"supporting")).await.map_err(|e|storage(&e))?.len();
        let mut out=envelope(company,p);out["expected_counts"]=p.expected_counts.clone();out["source_contexts"]=json!(index.len());out["assigned_contexts"]=json!(assigned);out["unassigned_contexts"]=json!(index.len() as u64-assigned);
        out["member_records"]=json!(member);out["supporting_records"]=json!(support);out["forms"]=json!(forms);out["Q"]=json!(q);out["E11_field_instances"]=json!(e11);out["context_index"]=json!(index);Ok(out)
    }
}
