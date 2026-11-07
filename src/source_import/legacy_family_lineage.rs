//! Historical native-family provenance, separate from immutable legacy ReturnIndex rows.
//! Hash-only identity evidence does not decode a return or rewrite a raw file/client ID.
use super::{bad, conflict, storage, ImportError, SourceImport};
use super::preserved_records::SourceFactPage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE: &str = "f05734d0cd42f21a347f3dbbdba428cb9b25e0d62ef3b18b5db9c287b855edb0";
const PERSONAL: &str = "9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeBodyProof {
    pub alternate_source_file_sha256: String,
    pub full_file_byteexact: bool,
    pub shared_body_offset: u32,
    pub shared_body_bytes: u32,
    pub shared_body_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonalProvenance {
    pub historical_record_id: String,
    pub historical_record_sha256: String,
    pub personal_record_sha256: String,
    pub personal_binding_profile_sha256: String,
    pub source_context_offset: u64,
    #[serde(rename="source_EXC_sha256")]
    pub source_exc_sha256: String,
    #[serde(rename="Info1_frame_offset")]
    pub info1_frame_offset: u64,
    #[serde(rename="Info1_frame_sha256")]
    pub info1_frame_sha256: String,
    #[serde(rename="Info1_expanded_sha256")]
    pub info1_expanded_sha256: String,
    #[serde(rename="Info1_expanded_bytes")]
    pub info1_expanded_bytes: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoricalMemberMapping {
    pub old_native_client_number: String,
    pub current_client_number: String,
    pub current_family_file_id: String,
    pub old_native_record_offset: u32,
    pub old_native_record_sha256: String,
    pub current_native_record_offset: u32,
    pub current_native_record_sha256: String,
    pub native_name_equality_evidence_sha256: String,
    #[serde(rename="native_DOB_equality_evidence_sha256")]
    pub native_dob_equality_evidence_sha256: String,
    #[serde(rename="native_nonplaceholder_SIN_equality_evidence_sha256")]
    pub native_nonplaceholder_sin_equality_evidence_sha256: Option<String>,
    pub personal_provenance: PersonalProvenance,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LegacyFamilyLineage {
    pub schema: String,
    pub lineage_profile_sha256: String,
    pub authority_proof_sha256: String,
    pub legacy_record_id: String,
    pub legacy_record_sha256: String,
    pub source_family_file_id: String,
    pub tax_year: u16,
    pub source_file_sha256: String,
    pub source_file_bytes: u32,
    pub old_native_index_sha256: String,
    pub current_native_index_sha256: String,
    pub archive_sha256: String,
    pub native_body_proofs: Vec<NativeBodyProof>,
    pub member_mappings: Vec<HistoricalMemberMapping>,
    pub source_ids_rewritten: bool,
    pub tax_payloads_decoded: bool,
    pub tax_values_approved: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyLineageBatch { pub test_copy: bool, pub records: Vec<LegacyFamilyLineage> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyLineageLookup { pub test_copy: bool, pub record_ids: Vec<String> }
#[derive(Deserialize)]
struct ClosedProfile {
    profile_sha256: String,
    contract: Value,
    allowed_full_record_sha256: BTreeSet<String>,
    held_minimal_return_record_sha256: BTreeSet<String>,
}
fn digest(v: &impl Serialize) -> String {
    let v = serde_json::to_value(v).expect("serializable lineage source");
    hex::encode(Sha256::digest(serde_json::to_vec(&v).expect("canonical lineage source")))
}
fn profile() -> &'static ClosedProfile {
    static P: OnceLock<ClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let p: ClosedProfile = serde_json::from_str(include_str!("legacy_family_lineage_profile.json")).expect("reviewed lineage profile");
        assert_eq!(p.profile_sha256, PROFILE); assert_eq!(digest(&p.contract), PROFILE);
        assert_eq!(p.allowed_full_record_sha256.len(), 3); assert_eq!(p.held_minimal_return_record_sha256.len(), 25);
        p
    })
}
fn sha(s: &str) -> bool { s.len()==64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn number(s: &str) -> bool { s.parse::<u32>().is_ok_and(|n| n.to_string()==s) }
fn record_id(s: &str) -> bool {
    let p:Vec<_>=s.split(':').collect(); matches!(p.as_slice(),[n,y] if number(n) && y.parse::<u16>().is_ok_and(|y| (1990..=2035).contains(&y)))
}
fn key(company: &str, rest: &str) -> String { format!("dtmaxtc:{company}:{rest}") }
fn body_key(company: &str, id: &str) -> String { key(company, &format!("legacy_family_lineage:{PROFILE}:record:{id}")) }
fn index(company: &str, rest: &str) -> String { key(company, &format!("legacy_family_lineage:{PROFILE}:{rest}")) }
fn validate_closed(s: &LegacyFamilyLineage, approved: &BTreeSet<String>) -> Result<()> {
    let p=profile();
    if s.schema!="dtmax-historical-native-family-lineage/v1" || s.lineage_profile_sha256!=PROFILE
        || s.authority_proof_sha256!=p.contract["authority_proof_sha256"] || s.archive_sha256!=p.contract["archive_sha256"]
        || s.old_native_index_sha256!=p.contract["old_native_index_sha256"] || s.current_native_index_sha256!=p.contract["current_native_index_sha256"]
        || !record_id(&s.legacy_record_id) || !matches!(s.tax_year,2023|2024) || s.legacy_record_id!=format!("{}:{}",s.source_family_file_id,s.tax_year)
        || !sha(&s.legacy_record_sha256) || !sha(&s.source_file_sha256) || !(1183..=64*1024*1024).contains(&s.source_file_bytes)
        || s.source_ids_rewritten || s.tax_payloads_decoded || s.tax_values_approved
        || s.native_body_proofs.len()!=1 || s.member_mappings.is_empty() || s.member_mappings.len()>3
        || !s.member_mappings.windows(2).all(|w|w[0].current_client_number < w[1].current_client_number) {
        return Err(bad("Historical family lineage scope, source ID or closed profile differs"));
    }
    let b=&s.native_body_proofs[0];
    if !sha(&b.alternate_source_file_sha256) || !sha(&b.shared_body_sha256) || b.shared_body_offset!=1104
        || b.shared_body_bytes<8000 || b.shared_body_offset.checked_add(b.shared_body_bytes).is_none_or(|n|n>s.source_file_bytes)
        || (b.full_file_byteexact && b.alternate_source_file_sha256!=s.source_file_sha256) {
        return Err(bad("Historical native body provenance differs"));
    }
    let families:BTreeSet<_>=s.member_mappings.iter().map(|m|&m.current_family_file_id).collect();
    if families.len()!=1 { return Err(bad("Historical family members do not map to one corroborated current family")); }
    let mut old_ids=BTreeSet::new();
    for m in &s.member_mappings {
        let r=&m.personal_provenance;
        if !number(&m.old_native_client_number) || !number(&m.current_client_number) || m.current_client_number=="0"
            || !number(&m.current_family_file_id) || !old_ids.insert(&m.old_native_client_number)
            || ![&m.old_native_record_sha256,&m.current_native_record_sha256,&m.native_name_equality_evidence_sha256,
                &m.native_dob_equality_evidence_sha256,&r.historical_record_sha256,&r.personal_record_sha256,
                &r.source_exc_sha256,&r.info1_frame_sha256,&r.info1_expanded_sha256].iter().all(|h|sha(h))
            || m.native_nonplaceholder_sin_equality_evidence_sha256.as_deref().is_some_and(|h|!sha(h))
            || ![m.old_native_record_offset,m.current_native_record_offset].iter().all(|n|*n>=1982 && (*n-1982)%90==0)
            || r.personal_binding_profile_sha256!=PERSONAL
            || r.historical_record_id!=format!("{}:{}:{}",m.current_client_number,s.tax_year,s.archive_sha256)
            || r.source_context_offset>=r.info1_frame_offset || r.info1_frame_offset>u32::MAX as u64
            || !(130..=65536).contains(&r.info1_expanded_bytes) {
            return Err(bad("Historical native member or Info1 provenance differs"));
        }
    }
    if !approved.contains(&digest(s)) { return Err(bad("Complete family lineage is not in the reviewed three-file package")); }
    Ok(())
}
impl SourceImport {
    async fn lineage_registered(&self, company: &str) -> Result<BTreeSet<String>> {
        let clients=self.store.smembers(&key(company,"clients")).await.map_err(|e|storage(&e))?;
        if clients.len()>100_000 { return Err(storage("Lineage client roster exceeds bound")); }
        Ok(clients.into_iter().collect())
    }
    async fn lineage_value(&self, k: &str) -> Result<Value> {
        serde_json::from_str(&self.store.get(k).await.map_err(|e|storage(&e))?.ok_or_else(||conflict("Historical lineage anchor missing"))?)
            .map_err(|_|storage("Historical lineage anchor is corrupt"))
    }
    async fn validate_lineage_anchors(&self, company: &str, s: &LegacyFamilyLineage, clients: &BTreeSet<String>, approved: &BTreeSet<String>) -> Result<()> {
        validate_closed(s,approved)?;
        let ledger_ids=self.store.smembers(&key(company,"returns")).await.map_err(|e|storage(&e))?;
        if ledger_ids.len()>100_000 || !ledger_ids.contains(&s.legacy_record_id) {return Err(conflict("Historical family source is not in the original return ledger"));}
        let original=self.lineage_value(&key(company,&format!("return:{}",s.legacy_record_id))).await?;
        if digest(&original)!=s.legacy_record_sha256 || original["client_number"]!=s.source_family_file_id
            || original["tax_year"]!=s.tax_year || original["header_year"]!=s.tax_year
            || original["sha256"]!=s.source_file_sha256 || original["size_bytes"]!=s.source_file_bytes
            || original["extension"]!=format!("P{:02}",s.tax_year%100) || original["content_state"]!="vendor_encoded_not_decoded" {
            return Err(conflict("Original legacy ReturnIndex row, year or file bytes changed"));
        }
        for m in &s.member_mappings {
            if !clients.contains(&m.current_client_number) { return Err(conflict("Historical family target client is not registered")); }
            let c=self.lineage_value(&key(company,&format!("client:{}",m.current_client_number))).await?;
            if c["client_number"]!=m.current_client_number { return Err(conflict("Registered historical-family client body differs")); }
            let r=&m.personal_provenance;
            let historical=self.lineage_value(&key(company,&format!("historical_lines:{}",r.historical_record_id))).await?;
            let personal=self.lineage_value(&key(company,&format!("personal_source:{PERSONAL}:record:{}",r.historical_record_id))).await?;
            if digest(&historical)!=r.historical_record_sha256 || historical["client_number"]!=m.current_client_number
                || historical["family_file_id"]!=m.current_family_file_id || historical["tax_year"]!=s.tax_year
                || historical["archive_sha256"]!=s.archive_sha256 || historical["vendor_module"]!="T1"
                || digest(&personal)!=r.personal_record_sha256 || personal["schema"]!="dtmax-personal-source-display/v1"
                || personal["client_number"]!=m.current_client_number || personal["family_file_id"]!=m.current_family_file_id
                || personal["source_year"]!=s.tax_year || personal["archive_sha256"]!=s.archive_sha256
                || personal["reader_module"]!="T1" || personal["info_record_state"]!="present_nonempty_crc_verified"
                || personal["source_context_offset"]!=r.source_context_offset || personal["source_EXC_sha256"]!=r.source_exc_sha256
                || personal["info1"]["frame_offset"]!=r.info1_frame_offset || personal["info1"]["frame_sha256"]!=r.info1_frame_sha256
                || personal["info1"]["expanded_sha256"]!=r.info1_expanded_sha256 || personal["info1"]["expanded_bytes"]!=r.info1_expanded_bytes
                || personal["info1"]["crc_verified"]!=true || personal["fields"]["BirthDate"]["state"]!="present_exact_date_candidate"
                || personal["fields"]["SIN"]["raw_redacted"]!=true || !personal["fields"]["SIN"]["raw_code"].is_null() {
                return Err(conflict("Historical family identity support no longer matches preserved Info1 provenance"));
            }
        }
        Ok(())
    }
    pub async fn test_copy_legacy_family_lineage(&self, company: &str, batch: FamilyLineageBatch) -> Result<Value> {
        self.preserve_family_lineage(company,batch,&profile().allowed_full_record_sha256).await
    }
    async fn preserve_family_lineage(&self, company: &str, batch: FamilyLineageBatch, approved: &BTreeSet<String>) -> Result<Value> {
        self.test_copy_guard(company,batch.test_copy)?;
        if batch.records.is_empty() || batch.records.len()>3 { return Err(bad("Historical family batch requires 1-3 reviewed source files")); }
        let clients=self.lineage_registered(company).await?;let mut seen=BTreeSet::new();
        for s in &batch.records {
            if !seen.insert(&s.legacy_record_id) { return Err(bad("Duplicate historical family record identity")); }
            self.validate_lineage_anchors(company,s,&clients,approved).await?;
            let body=serde_json::to_string(s).map_err(|_|storage("Cannot encode family lineage"))?;
            if self.store.get(&body_key(company,&s.legacy_record_id)).await.map_err(|e|storage(&e))?.is_some_and(|old|old!=body) {
                return Err(conflict("Historical family lineage is immutable"));
            }
        }
        let (mut created,mut unchanged)=(0,0);
        for s in &batch.records {
            let body=serde_json::to_string(s).map_err(|_|storage("Cannot encode family lineage"))?;let k=body_key(company,&s.legacy_record_id);
            let added=self.store.set_nx(&k,&body).await.map_err(|e|storage(&e))?;
            if self.store.get(&k).await.map_err(|e|storage(&e))?.as_deref()!=Some(&body) { return Err(conflict("Historical family lineage is immutable")); }
            self.store.sadd(&index(company,"all"),&s.legacy_record_id).await.map_err(|e|storage(&e))?;
            for m in &s.member_mappings { self.store.sadd(&index(company,&format!("client:{}",m.current_client_number)),&s.legacy_record_id).await.map_err(|e|storage(&e))?; }
            if added {created+=1}else{unchanged+=1}
        }
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,
            "source_rows_modified":0,"client_ids_rewritten":0,"tax_payloads_decoded":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_read_legacy_family_lineage(&self, company: &str, lookup: FamilyLineageLookup) -> Result<Value> {
        self.read_family_lineage(company,lookup,&profile().allowed_full_record_sha256).await
    }
    async fn read_family_lineage(&self, company: &str, lookup: FamilyLineageLookup, approved: &BTreeSet<String>) -> Result<Value> {
        self.test_copy_guard(company,lookup.test_copy)?;
        if lookup.record_ids.is_empty() || lookup.record_ids.len()>100 || lookup.record_ids.iter().any(|id|!record_id(id)) {
            return Err(bad("Historical family lookup requires 1-100 original source identities"));
        }
        let keys:Vec<_>=lookup.record_ids.iter().map(|id|body_key(company,id)).collect();
        let raws=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if raws.len()!=keys.len() { return Err(storage("Historical family read cardinality differs")); }
        let clients=self.lineage_registered(company).await?;let mut records=Vec::new();
        for (id,raw) in lookup.record_ids.iter().zip(raws) {
            let s=raw.map(|raw|serde_json::from_str::<LegacyFamilyLineage>(&raw).map_err(|_|storage("Stored family lineage is corrupt"))).transpose()?;
            if let Some(s)=&s {
                if s.legacy_record_id!=*id {return Err(storage("Stored family lineage identity differs"));}
                self.validate_lineage_anchors(company,s,&clients,approved).await?;
            }
            records.push(s);
        }
        Ok(json!({"test_copy":true,"source_scope":company,"lineage_profile_sha256":PROFILE,"record_ids":lookup.record_ids,
            "records":records,"source_ids_rewritten":false,"tax_payloads_decoded":false,"tax_values_approved":false}))
    }
    pub(super) async fn attach_legacy_family_lineage(&self, company: &str, records: &mut [Value]) -> Result<()> {
        if records.is_empty(){return Ok(())}
        let ids=records.iter().map(|r|r["record_id"].as_str().unwrap_or("").to_owned()).collect();
        let response=self.test_copy_read_legacy_family_lineage(company,FamilyLineageLookup{test_copy:true,record_ids:ids}).await?;
        for (record,lineage) in records.iter_mut().zip(response["records"].as_array().ok_or_else(||storage("Lineage attachment cardinality missing"))?) {
            if !lineage.is_null() && (record["record_sha256"]!=lineage["legacy_record_sha256"] || digest(&record["record"])!=lineage["legacy_record_sha256"]) {
                return Err(storage("Lineage ledger attachment source changed"));
            }
            record["historical_family_lineage"]=lineage.clone();
        }
        Ok(())
    }
    pub async fn test_copy_client_legacy_family_lineage(&self, company: &str, client: &str, page: SourceFactPage) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        if !number(client) || page.limit==0 || page.limit>100 || page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)) {
            return Err(bad("Invalid historical family client page"));
        }
        if !self.lineage_registered(company).await?.contains(client) {return Err(conflict("Historical family target client not registered"));}
        let mut ids=self.store.smembers(&index(company,&format!("client:{client}"))).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>3 {return Err(storage("Historical family client index exceeds closed package"));}
        let mut rows=Vec::new();
        if !ids.is_empty() {
            let response=self.test_copy_read_legacy_family_lineage(company,FamilyLineageLookup{test_copy:true,record_ids:ids}).await?;
            for s in response["records"].as_array().ok_or_else(||storage("Historical family client read missing"))? {
                if s.is_null() || !s["member_mappings"].as_array().is_some_and(|members|members.iter().any(|m|m["current_client_number"]==client)) {
                    return Err(storage("Historical family client index member differs"));
                }
                if page.source_return_year.is_none_or(|y|s["tax_year"]==y) {rows.push(s.clone());}
            }
        }
        let total=rows.len();let rows:Vec<_>=rows.into_iter().skip(page.offset).take(page.limit).collect();
        Ok(json!({"test_copy":true,"source_scope":company,"client_number":client,"kind":"historical-family-lineage",
            "total":total,"offset":page.offset,"records":rows,"source_ids_rewritten":false,"tax_payloads_decoded":false,"tax_values_approved":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_legacy_family_lineage_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?;let clients=self.lineage_registered(company).await?;
        let raw=self.store.smembers(&key(company,"returns")).await.map_err(|e|storage(&e))?;
        if raw.len()>100_000 {return Err(storage("Legacy return ledger exceeds bound"));}
        let unlinked:Vec<_>=raw.into_iter().filter(|id|!clients.contains(id.split(':').next().unwrap_or(""))).collect();
        let mut held=0;
        for chunk in unlinked.chunks(250) {
            let keys:Vec<_>=chunk.iter().map(|id|key(company,&format!("return:{id}"))).collect();
            let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
            if rows.len()!=chunk.len(){return Err(storage("Legacy held-container cardinality differs"));}
            for raw in rows {
                let row:Value=serde_json::from_str(&raw.ok_or_else(||storage("Legacy return row missing"))?).map_err(|_|storage("Legacy return row corrupt"))?;
                if profile().held_minimal_return_record_sha256.contains(&digest(&row)){held+=1;}
            }
        }
        let mut ids=self.store.smembers(&index(company,"all")).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>3 {return Err(storage("Historical family lineage exceeds closed package"));}
        let mut targets=BTreeSet::new();
        if !ids.is_empty() {
            let response=self.test_copy_read_legacy_family_lineage(company,FamilyLineageLookup{test_copy:true,record_ids:ids.clone()}).await?;
            for s in response["records"].as_array().ok_or_else(||storage("Family summary read missing"))? {
                for member in s["member_mappings"].as_array().ok_or_else(||storage("Family summary index body missing"))? {
                    targets.insert(member["current_client_number"].as_str().ok_or_else(||storage("Family summary member missing"))?.to_owned());
                }
            }
        }
        Ok(json!({"test_copy":true,"source_scope":company,"lineage_profile_sha256":PROFILE,"raw_unlinked_total":unlinked.len(),
            "historical_family_lineage_source_files":ids.len(),"distinct_corroborated_members":targets.len(),"minimal_native_containers_still_held":held,
            "source_rows_modified":0,"client_ids_rewritten":0,"tax_payloads_decoded":false,"tax_values_approved":false,"tax_cases_imported":0}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (LegacyFamilyLineage,Value,Value,Value) {
        let p=profile();let archive=p.contract["archive_sha256"].as_str().unwrap();let hid=format!("42:2024:{archive}");
        let source=json!({"client_number":"7","tax_year":2024,"header_year":2024,"sha256":"c".repeat(64),
            "size_bytes":10000,"extension":"P24","sections":3,"last_saved_epoch":null,"content_state":"vendor_encoded_not_decoded"});
        let history=json!({"client_number":"42","family_file_id":"8","tax_year":2024,"archive_sha256":archive,"vendor_module":"T1"});
        let personal=json!({"schema":"dtmax-personal-source-display/v1","client_number":"42","family_file_id":"8","source_year":2024,
            "archive_sha256":archive,"reader_module":"T1","info_record_state":"present_nonempty_crc_verified","source_context_offset":512,
            "source_EXC_sha256":"a".repeat(64),"info1":{"frame_offset":1024,"frame_sha256":"b".repeat(64),
            "expanded_sha256":"d".repeat(64),"expanded_bytes":200,"crc_verified":true},
            "fields":{"BirthDate":{"state":"present_exact_date_candidate"},"SIN":{"raw_redacted":true,"raw_code":null}}});
        let s=LegacyFamilyLineage{schema:"dtmax-historical-native-family-lineage/v1".into(),lineage_profile_sha256:PROFILE.into(),
            authority_proof_sha256:p.contract["authority_proof_sha256"].as_str().unwrap().into(),legacy_record_id:"7:2024".into(),
            legacy_record_sha256:digest(&source),source_family_file_id:"7".into(),tax_year:2024,source_file_sha256:"c".repeat(64),source_file_bytes:10000,
            old_native_index_sha256:p.contract["old_native_index_sha256"].as_str().unwrap().into(),
            current_native_index_sha256:p.contract["current_native_index_sha256"].as_str().unwrap().into(),archive_sha256:archive.into(),
            native_body_proofs:vec![NativeBodyProof{alternate_source_file_sha256:"c".repeat(64),full_file_byteexact:true,
                shared_body_offset:1104,shared_body_bytes:8896,shared_body_sha256:"d".repeat(64)}],
            member_mappings:vec![HistoricalMemberMapping{old_native_client_number:"4".into(),current_client_number:"42".into(),
                current_family_file_id:"8".into(),old_native_record_offset:2072,old_native_record_sha256:"b".repeat(64),
                current_native_record_offset:2162,current_native_record_sha256:"d".repeat(64),native_name_equality_evidence_sha256:"e".repeat(64),
                native_dob_equality_evidence_sha256:"f".repeat(64),native_nonplaceholder_sin_equality_evidence_sha256:None,
                personal_provenance:PersonalProvenance{historical_record_id:hid,historical_record_sha256:digest(&history),
                    personal_record_sha256:digest(&personal),personal_binding_profile_sha256:PERSONAL.into(),source_context_offset:512,
                    source_exc_sha256:"a".repeat(64),info1_frame_offset:1024,info1_frame_sha256:"b".repeat(64),
                    info1_expanded_sha256:"d".repeat(64),info1_expanded_bytes:200}}],
            source_ids_rewritten:false,tax_payloads_decoded:false,tax_values_approved:false};
        (s,source,history,personal)
    }
    async fn seed(store:&crate::store::Store,s:&LegacyFamilyLineage,source:&Value,history:&Value,personal:&Value) {
        store.sadd(&key("dtmax-test-copy","returns"),&s.legacy_record_id).await.unwrap();
        store.set(&key("dtmax-test-copy",&format!("return:{}",s.legacy_record_id)),&source.to_string()).await.unwrap();
        for m in &s.member_mappings {
            store.sadd(&key("dtmax-test-copy","clients"),&m.current_client_number).await.unwrap();
            store.set(&key("dtmax-test-copy",&format!("client:{}",m.current_client_number)),&json!({"client_number":m.current_client_number}).to_string()).await.unwrap();
            let id=&m.personal_provenance.historical_record_id;
            store.set(&key("dtmax-test-copy",&format!("historical_lines:{id}")),&history.to_string()).await.unwrap();
            store.set(&key("dtmax-test-copy",&format!("personal_source:{PERSONAL}:record:{id}")),&personal.to_string()).await.unwrap();
        }
    }
    #[test]
    fn closed_lineage_never_approves_tax_values_or_rewrites_source_ids() {
        let (s,_,_,_)=fixture();let approved=BTreeSet::from([digest(&s)]);
        assert!(validate_closed(&s,&approved).is_ok());assert!(validate_closed(&s,&profile().allowed_full_record_sha256).is_err());
        for n in 0..5 {let mut t=s.clone();match n {0=>t.source_ids_rewritten=true,1=>t.tax_payloads_decoded=true,
            2=>t.tax_values_approved=true,3=>t.source_family_file_id="8".into(),_=>t.member_mappings.push(t.member_mappings[0].clone())}
            assert!(validate_closed(&t,&approved).is_err());}
        let mut with_private_value=json!(s);with_private_value["given_name"]=json!("unrequested field");
        assert!(serde_json::from_value::<LegacyFamilyLineage>(with_private_value).is_err());
        let mut altered=s.clone();altered.member_mappings[0].native_name_equality_evidence_sha256="f".repeat(64);
        assert!(validate_closed(&altered,&approved).is_err());
    }
    #[tokio::test]
    async fn exact_return_and_personal_anchors_are_required_before_writes() {
        let store=crate::store::Store::memory();let service=SourceImport::new(store.clone());let(s,source,history,personal)=fixture();
        let approved=BTreeSet::from([digest(&s)]);seed(&store,&s,&source,&history,&personal).await;
        for anchor in ["return","history","personal","client"] {
            let k=match anchor {"return"=>key("dtmax-test-copy","return:7:2024"),"history"=>key("dtmax-test-copy",&format!("historical_lines:{}",s.member_mappings[0].personal_provenance.historical_record_id)),
                "personal"=>key("dtmax-test-copy",&format!("personal_source:{PERSONAL}:record:{}",s.member_mappings[0].personal_provenance.historical_record_id)),
                _=>key("dtmax-test-copy","client:42")};
            let original=store.get(&k).await.unwrap().unwrap();store.set(&k,"{}").await.unwrap();
            assert!(service.preserve_family_lineage("dtmax-test-copy",FamilyLineageBatch{test_copy:true,records:vec![s.clone()]},&approved).await.is_err());
            assert!(store.get(&body_key("dtmax-test-copy",&s.legacy_record_id)).await.unwrap().is_none());store.set(&k,&original).await.unwrap();
        }
        let first=service.preserve_family_lineage("dtmax-test-copy",FamilyLineageBatch{test_copy:true,records:vec![s.clone()]},&approved).await.unwrap();assert_eq!(first["created"],1);
        let replay=service.preserve_family_lineage("dtmax-test-copy",FamilyLineageBatch{test_copy:true,records:vec![s.clone()]},&approved).await.unwrap();assert_eq!(replay["unchanged_replays"],1);
        let raw=store.get(&key("dtmax-test-copy","return:7:2024")).await.unwrap().unwrap();assert_eq!(raw,source.to_string());
        let mut changed=s.clone();changed.member_mappings[0].native_name_equality_evidence_sha256="a".repeat(64);let reviewed_changed=BTreeSet::from([digest(&changed)]);
        assert!(service.preserve_family_lineage("dtmax-test-copy",FamilyLineageBatch{test_copy:true,records:vec![changed]},&reviewed_changed).await.is_err());
    }
    #[tokio::test]
    async fn bulk_read_keeps_nulls_order_and_rechecks_registered_targets() {
        let store=crate::store::Store::memory();let service=SourceImport::new(store.clone());let(s,source,history,personal)=fixture();
        let approved=BTreeSet::from([digest(&s)]);seed(&store,&s,&source,&history,&personal).await;
        service.preserve_family_lineage("dtmax-test-copy",FamilyLineageBatch{test_copy:true,records:vec![s.clone()]},&approved).await.unwrap();
        let response=service.read_family_lineage("dtmax-test-copy",FamilyLineageLookup{test_copy:true,record_ids:vec!["0:2005".into(),s.legacy_record_id.clone(),s.legacy_record_id.clone()]},&approved).await.unwrap();
        assert!(response["records"][0].is_null());assert_eq!(response["records"][1],json!(s));assert_eq!(response["records"][2],json!(s));
        assert!(service.read_family_lineage("dtmax-test-copy",FamilyLineageLookup{test_copy:false,record_ids:vec![s.legacy_record_id.clone()]},&approved).await.is_err());
        let mut wrong=personal.clone();wrong["source_EXC_sha256"]=json!("a".repeat(63));
        store.set(&key("dtmax-test-copy",&format!("personal_source:{PERSONAL}:record:{}",s.member_mappings[0].personal_provenance.historical_record_id)),&wrong.to_string()).await.unwrap();
        assert!(service.read_family_lineage("dtmax-test-copy",FamilyLineageLookup{test_copy:true,record_ids:vec![s.legacy_record_id.clone()]},&approved).await.is_err());
    }
    #[tokio::test]
    async fn optional_real_three_file_package_and_unchanged_40_row_ledger() {
        let Ok(dir)=std::env::var("NXT_DTMAX_FAMILY_LINEAGE_PACKAGE") else{return};let dir=std::path::PathBuf::from(dir);
        let batch:FamilyLineageBatch=serde_json::from_slice(&std::fs::read(dir.join("candidate-batch.private.json")).unwrap()).unwrap();
        assert_eq!(batch.records.len(),3);assert_eq!(batch.records.iter().map(|r|r.member_mappings.len()).sum::<usize>(),4);
        let anchors:Value=serde_json::from_slice(&std::fs::read(dir.join("test-anchors.private.json")).unwrap()).unwrap();
        let store=crate::store::Store::memory();let service=SourceImport::new(store.clone());
        for (id,row) in anchors["returns"].as_object().unwrap() {store.sadd(&key("dtmax-test-copy","returns"),id).await.unwrap();store.set(&key("dtmax-test-copy",&format!("return:{id}")),&row.to_string()).await.unwrap();}
        for (id,row) in anchors["clients"].as_object().unwrap() {store.sadd(&key("dtmax-test-copy","clients"),id).await.unwrap();store.set(&key("dtmax-test-copy",&format!("client:{id}")),&row.to_string()).await.unwrap();}
        for (id,row) in anchors["historical"].as_object().unwrap() {store.set(&key("dtmax-test-copy",&format!("historical_lines:{id}")),&row.to_string()).await.unwrap();}
        for (id,row) in anchors["personal"].as_object().unwrap() {
            let typed:super::super::personal_source_info::PersonalSourceInfo=serde_json::from_value(row.clone()).unwrap();
            assert_eq!(digest(&typed),digest(row),"personal anchor must match actual stored typed representation");
            store.set(&key("dtmax-test-copy",&format!("personal_source:{PERSONAL}:record:{id}")),&serde_json::to_string(&typed).unwrap()).await.unwrap();
        }
        let original=serde_json::to_value(&batch.records).unwrap();
        assert_eq!(batch.records.iter().map(digest).collect::<BTreeSet<_>>(),profile().allowed_full_record_sha256);
        let response=service.test_copy_legacy_family_lineage("dtmax-test-copy",batch).await.unwrap();assert_eq!(response["created"],3);
        let rows:Vec<LegacyFamilyLineage>=serde_json::from_value(original.clone()).unwrap();
        let ids=rows.iter().map(|s|s.legacy_record_id.clone()).collect();
        let read=service.test_copy_read_legacy_family_lineage("dtmax-test-copy",FamilyLineageLookup{test_copy:true,record_ids:ids}).await.unwrap();assert_eq!(read["records"],original);
        let replay=service.test_copy_legacy_family_lineage("dtmax-test-copy",FamilyLineageBatch{test_copy:true,records:rows}).await.unwrap();assert_eq!(replay["unchanged_replays"],3);
        let summary=service.test_copy_legacy_family_lineage_summary("dtmax-test-copy").await.unwrap();
        assert_eq!(summary["raw_unlinked_total"],40);assert_eq!(summary["historical_family_lineage_source_files"],3);
        assert_eq!(summary["minimal_native_containers_still_held"],25);assert_eq!(summary["distinct_corroborated_members"],3);
        let mut ledger=Vec::new();
        for(id,row)in anchors["returns"].as_object().unwrap(){assert_eq!(store.get(&key("dtmax-test-copy",&format!("return:{id}"))).await.unwrap().unwrap(),row.to_string());ledger.push(json!({"record_id":id,"record_sha256":digest(row),"record":row}));}
        service.attach_legacy_family_lineage("dtmax-test-copy",&mut ledger).await.unwrap();
        assert_eq!(ledger.iter().filter(|r|!r["historical_family_lineage"].is_null()).count(),3);
        for id in anchors["clients"].as_object().unwrap().keys() {
            let page=SourceFactPage{kind:"historical-family-lineage".into(),offset:0,limit:100,source_return_year:None,_routing_path:None};
            let linked=service.test_copy_client_legacy_family_lineage("dtmax-test-copy",id,page).await.unwrap();assert!(linked["total"].as_u64().unwrap()>0);
        }
        println!("Validated 3 real lineage files / 3 distinct members / 4 Info1 references; raw unlinked ledger remains 40 and 25 minimal containers remain held");
    }
}
