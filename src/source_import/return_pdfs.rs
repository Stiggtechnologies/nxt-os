//! Immutable T1 documents with explicit origin evidence. Separate from the
//! synthetic opaque-file lane: no calculation, identity editing or filing.
use super::{bad, conflict, missing, storage, SourceImport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
#[path = "reviewed_packet_pdfs.rs"]
pub mod reviewed_packets;
type Result<T> = std::result::Result<T, super::ImportError>;
pub const MAX_PDF_BYTES: usize = 2 * 1024 * 1024;
const OFFICE_JSON_LIMIT: usize = 4_400_000;
pub const OFFICE_PROFILE: &str = "b51cc8aec7d930f6e5b4d05575538f87856a39c600f5893a0828aff96816812e";
const OFFICE_CLOSED_SHA: &str = "cdf3d627d0a531653e57dff0e966e85a44b76da32a17e5634c2bd12afc6049ef";
pub const STABLE_OFFICE_PROFILE: &str = "8b45d6655423ff1f2e0f8202a1009f25b1e472c73f7b7591b581a73264f42028";
const STABLE_OFFICE_CLOSED_SHA: &str = "4440152771f22dab553674aad5628d90e42f19456805eace7a09d64f4ef3de04";
pub const SCANNED_OFFICE_PROFILE: &str = "606e38ceaa60949c7d631761beb49bd6afc5c95fb56fe1b0a9290a78062c0480";
const SCANNED_OFFICE_CLOSED_SHA: &str = "a8ea94063e20e441652079483b08d5ce6b2f127b2ad27b932db603656ad6621e";
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReturnPdfInput {
    pub test_copy: bool,
    pub client_number: String,
    pub tax_year: u16,
    pub sha256: String,
    pub page_labels: Vec<String>,
    pub content_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_provenance: Option<CachedNativeExport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_office_provenance: Option<RetainedOfficeProvenance>,
}

/// Owner/year evidence does not establish native generation, return completeness or filing.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RetainedOfficeProvenance {
    pub kind: String,
    pub profile_sha256: String,
    pub source_manifest_sha256: String,
    pub source_occurrence_manifest_sha256: String,
    pub owner_year_evidence_sha256: String,
    pub mixed_owner_year_review_sha256: String,
    pub static_pdf_review_sha256: String,
    pub source_metadata_sha256: String,
    pub native_generation_verified: bool,
    pub as_filed_verified: bool,
    pub financial_values_approved: bool,
    pub source_bytes_modified: bool,
}
#[derive(Deserialize)]
struct OfficeClosedProfile {
    contract: Value,
    profile_sha256: String,
    source_packet_sha256: String,
    record_count: usize,
    allowed_record_sha256: BTreeSet<String>,
    document_sha256: BTreeSet<String>,
}
fn office_profile() -> &'static OfficeClosedProfile {
    static P: OnceLock<OfficeClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("retained_office_T1_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)), OFFICE_CLOSED_SHA);
        let p: OfficeClosedProfile = serde_json::from_slice(raw).expect("closed office T1 profile");
        assert_eq!(p.profile_sha256, OFFICE_PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())), OFFICE_PROFILE);
        assert!(digest(&p.source_packet_sha256) && p.record_count > 0 && p.record_count <= 314);
        assert_eq!(p.allowed_record_sha256.len(), p.record_count);
        assert_eq!(p.document_sha256.len(), p.record_count);
        assert!(p.allowed_record_sha256.iter().chain(&p.document_sha256).all(|s|digest(s)));
        p
    })
}
fn stable_office_profile() -> &'static OfficeClosedProfile {
    static P: OnceLock<OfficeClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("retained_office_T1_stable_identifier_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)), STABLE_OFFICE_CLOSED_SHA);
        let p: OfficeClosedProfile = serde_json::from_slice(raw).expect("closed stable-identifier office T1 profile");
        assert_eq!(p.profile_sha256, STABLE_OFFICE_PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())), STABLE_OFFICE_PROFILE);
        assert!(digest(&p.source_packet_sha256));
        assert_eq!(p.record_count, 2);
        assert_eq!(p.allowed_record_sha256.len(), 2);
        assert_eq!(p.document_sha256.len(), 2);
        assert!(p.allowed_record_sha256.iter().chain(&p.document_sha256).all(|s|digest(s)));
        assert!(p.document_sha256.is_disjoint(&office_profile().document_sha256));
        p
    })
}
fn scanned_office_profile() -> &'static OfficeClosedProfile {
    static P: OnceLock<OfficeClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("retained_office_T1_scanned_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)), SCANNED_OFFICE_CLOSED_SHA);
        let p: OfficeClosedProfile = serde_json::from_slice(raw).expect("closed scanned-office T1 profile");
        assert_eq!(p.profile_sha256, SCANNED_OFFICE_PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())), SCANNED_OFFICE_PROFILE);
        assert!(digest(&p.source_packet_sha256));
        assert_eq!(p.record_count, 1);
        assert_eq!(p.allowed_record_sha256.len(), 1);
        assert_eq!(p.document_sha256.len(), 1);
        assert!(p.allowed_record_sha256.iter().chain(&p.document_sha256).all(|s|digest(s)));
        assert!(p.document_sha256.is_disjoint(&office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&stable_office_profile().document_sha256));
        p
    })
}
fn office_descriptor(r: &Record) -> Value {
    json!({"client_number":r.client_number,"tax_year":r.tax_year,"sha256":r.sha256,"bytes":r.bytes,"page_labels":r.page_labels,"retained_office_provenance":r.retained_office_provenance})
}
fn validate_record_provenance(r: &Record) -> Result<()> {
    if reviewed_packets::is_packet(r) { return reviewed_packets::validate_record(r); }
    if r.reviewed_packet_scope.is_some() { return Err(bad("A reviewed packet section requires its explicit source contract")); }
    if let Some(p) = &r.retained_office_provenance {
        if r.export_provenance.is_some() || p.native_generation_verified
            || p.as_filed_verified || p.financial_values_approved || p.source_bytes_modified
            || [&p.source_manifest_sha256,&p.source_occurrence_manifest_sha256,&p.owner_year_evidence_sha256,
                &p.mixed_owner_year_review_sha256,&p.static_pdf_review_sha256,&p.source_metadata_sha256].iter().any(|s|!digest(s))
        { return Err(bad("Retained office T1 provenance differs")); }
        let selected = if p.kind == "retained_office_T1_document" && p.profile_sha256 == OFFICE_PROFILE {
            office_profile()
        } else if p.kind == "retained_office_T1_stable_identifier_document" && p.profile_sha256 == STABLE_OFFICE_PROFILE {
            stable_office_profile()
        } else if p.kind == "retained_office_T1_scanned_document" && p.profile_sha256 == SCANNED_OFFICE_PROFILE {
            scanned_office_profile()
        } else { return Err(bad("Retained office T1 evidence method or contract differs")); };
        let raw = serde_json::to_vec(&office_descriptor(r)).map_err(|_|bad("Cannot encode office T1 descriptor"))?;
        if !selected.document_sha256.contains(&r.sha256)
            || !selected.allowed_record_sha256.contains(&hex::encode(Sha256::digest(raw)))
        { return Err(bad("Office T1 document is outside independently closed owner/year evidence")); }
    } else if office_profile().document_sha256.contains(&r.sha256) || stable_office_profile().document_sha256.contains(&r.sha256) || scanned_office_profile().document_sha256.contains(&r.sha256) || reviewed_packets::document_allowed(&r.sha256) {
        return Err(bad("This retained office T1 document requires its explicit office provenance"));
    }
    if let Some(p)=&r.export_provenance {validate_export(p)?;}
    Ok(())
}

/// A new export of the original application's stored print queue. This does
/// not establish that the PDF itself was retained historically or filed.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CachedNativeExport {
    pub kind: String,
    pub source_backup_sha256: String,
    pub native_family_file_sha256: String,
    pub export_log_sha256: String,
    pub exported_at_utc: String,
    pub recalculated: bool,
    pub as_filed_verified: bool,
    pub figures_csv_sha256: String,
    pub figure_comparisons: BTreeMap<String,String>,
}
fn validate_export(p: &CachedNativeExport) -> Result<()> {
    if p.kind != "cached_native_export"
        || p.source_backup_sha256 != "7b760c8406ac29f6942898818c28be03c823128fdb164b5b03885f9f37b0eed5"
        || !digest(&p.native_family_file_sha256) || !digest(&p.export_log_sha256)
        || p.recalculated || p.as_filed_verified
        || p.figures_csv_sha256 != "c16c0bdc3036932f54e09cdd698513344c945cc4e8ba006cca54cd2fe8f93a94"
        || p.figure_comparisons.len()!=6
        || ["15000","23600","26000","42000","48400","48500"].iter().any(|line|p.figure_comparisons.get(*line).is_none_or(|state|!matches!(state.as_str(),"verified_equal_source"|"printed_source_difference"|"comparison_unavailable_source_absent"|"comparison_unavailable_printed_blank"|"comparison_unavailable_printed_line_not_located"|"held_ambiguous_or_unverified_source_figures"|"held_source_numeric_representation"|"held_ambiguous_printed_amount")))
        || !p.exported_at_utc.ends_with('Z')
        || chrono::DateTime::parse_from_rfc3339(&p.exported_at_utc).is_err()
    { return Err(bad("Cached PDF export provenance did not match")); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Serialize,Deserialize)]
    struct LegacyRecord {
        client_number:String,tax_year:u16,sha256:String,page_labels:Vec<String>,bytes:usize,
        #[serde(default,skip_serializing_if="Option::is_none")]
        export_provenance:Option<CachedNativeExport>,
    }
    #[test]
    fn stable_identifier_method_is_separate_and_cannot_fall_back_to_native_mode() {
        let stable=stable_office_profile();let original=office_profile();
        assert_eq!(stable.record_count,2);assert_eq!(original.record_count,177);
        assert_eq!(stable.source_packet_sha256,"102085c07e719cc72a5c070afd60c0976e1968f37ff0dc3641ac5a2a6a09bcbd");
        assert_ne!(STABLE_OFFICE_PROFILE,OFFICE_PROFILE);
        assert!(stable.document_sha256.is_disjoint(&original.document_sha256));
        for hash in stable.document_sha256.iter().chain(&original.document_sha256) {
            let record:Record=serde_json::from_value(json!({"client_number":"100","tax_year":2024,"sha256":hash,"page_labels":["Synthetic"],"bytes":123})).unwrap();
            assert!(validate_record_provenance(&record).is_err());
        }
    }
    #[test]
    fn scanned_method_is_separately_closed_and_cannot_claim_another_method_or_native_origin() {
        let p=scanned_office_profile();assert_eq!(p.record_count,1);
        assert_ne!(SCANNED_OFFICE_PROFILE,STABLE_OFFICE_PROFILE);assert_ne!(SCANNED_OFFICE_PROFILE,OFFICE_PROFILE);
        for hash in &p.document_sha256 {
            let record:Record=serde_json::from_value(json!({"client_number":"100","tax_year":2018,"sha256":hash,"page_labels":["Synthetic"],"bytes":123})).unwrap();
            assert!(validate_record_provenance(&record).is_err());
        }
    }
    #[test]
    fn office_profile_is_closed_and_legacy_field_omission_is_exact() {
        let p=office_profile();assert!(p.record_count>=3);assert_eq!(p.allowed_record_sha256.len(),p.record_count);
        let v=json!({"client_number":"100","tax_year":2024,"sha256":"a".repeat(64),"page_labels":["Synthetic"],"bytes":123});
        let legacy:LegacyRecord=serde_json::from_value(v).unwrap();let old=serde_json::to_string(&legacy).unwrap();
        let r:Record=serde_json::from_str(&old).unwrap();assert_eq!(serde_json::to_string(&r).unwrap(),old);assert!(describe(&r).get("retained_office_provenance").is_none());assert_eq!(describe(&r)["original_print_output"],true);
    }
    #[test]
    fn optional_all2593_legacy_metadata_serialize_and_describe_identically() {
        let Ok(path)=std::env::var("NXT_DTMAX_OFFICE_LEGACY")else{return};
        let fixture:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let rows=fixture["records"].as_array().unwrap();assert_eq!(rows.len(),2593);
        for v in rows {
            let old:LegacyRecord=serde_json::from_value(v.clone()).unwrap();let raw=serde_json::to_string(&old).unwrap();let r:Record=serde_json::from_str(&raw).unwrap();
            assert!(serde_json::to_string(&r).unwrap()==raw,"Legacy stored PDF body serialization changed");validate_record_provenance(&r).unwrap();
            let mut expected=json!({"client_number":old.client_number,"tax_year":old.tax_year,"sha256":old.sha256,"bytes":old.bytes,"page_labels":old.page_labels,"page_count":old.page_labels.len(),"original_print_output":true,"immutable":true});if let Some(p)=old.export_provenance{expected["export_provenance"]=json!(p);}
            assert!(describe(&r)==expected,"Legacy PDF public metadata changed");
        }
        println!("LEGACY_PDF_METADATA_BYTE_COMPATIBLE: records=2593");
    }
    #[test]
    fn nested_active_content_is_rejected() {
        let action=lopdf::dictionary!{"S"=>"JavaScript","JS"=>lopdf::Object::string_literal("example")};
        let annotation=lopdf::dictionary!{"Type"=>"Annot","A"=>action};
        let page=lopdf::Object::Dictionary(lopdf::dictionary!{"Annots"=>vec![lopdf::Object::Dictionary(annotation)]});
        assert!(!static_object(&page,0,&mut 0));
        assert!(static_object(&lopdf::Object::Dictionary(lopdf::dictionary!{"Type"=>"Page"}),0,&mut 0));
    }
    #[test]
    fn optional_all_prepacket_records_keep_exact_legacy_wire_and_public_metadata() {
        let Ok(path)=std::env::var("NXT_DTMAX_PREPACKET_METADATA")else{return};
        #[derive(Serialize,Deserialize)]
        struct PriorRecord {
            client_number:String,tax_year:u16,sha256:String,page_labels:Vec<String>,bytes:usize,
            #[serde(default,skip_serializing_if="Option::is_none")]
            export_provenance:Option<CachedNativeExport>,
            #[serde(default,skip_serializing_if="Option::is_none")]
            retained_office_provenance:Option<RetainedOfficeProvenance>,
        }
        let fixture:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(fixture["native_cached_package_documents"],13827);
        let rows=fixture["records"].as_array().unwrap();assert!(rows.len()>=13827);
        for value in rows {
            let old:PriorRecord=serde_json::from_value(value.clone()).unwrap();
            let raw=serde_json::to_string(&old).unwrap();let new:Record=serde_json::from_str(&raw).unwrap();
            assert!(serde_json::to_string(&new).unwrap()==raw,"Prepacket stored metadata wire changed");
            validate_record_provenance(&new).unwrap();
            let mut expected=value.clone();expected["original_print_output"]=json!(old.retained_office_provenance.is_none());
            expected["immutable"]=json!(true);expected["page_count"]=json!(old.page_labels.len());
            assert!(describe(&new)==expected,"Prepacket public descriptor changed");
        }
        println!("PREPACKET_METADATA_EXACT: records={}; native_package_documents=13827; earlier_office_documents=180",rows.len());
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Record {
    client_number: String, tax_year: u16, sha256: String,
    page_labels: Vec<String>, bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    export_provenance: Option<CachedNativeExport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retained_office_provenance: Option<RetainedOfficeProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reviewed_packet_scope: Option<reviewed_packets::ReviewedPacketScope>,
}
fn number(s: &str) -> bool { !s.is_empty() && s.len() <= 12 && s.bytes().all(|b|b.is_ascii_digit()) }
fn digest(s: &str) -> bool { s.len()==64 && s.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn k(company: &str, rest: &str) -> String { format!("dtmaxtc:{company}:return_pdf:{rest}") }
fn describe(r: &Record) -> Value {
    let mut value=json!({"client_number":r.client_number,"tax_year":r.tax_year,"sha256":r.sha256,"page_labels":r.page_labels,"page_count":r.page_labels.len(),"bytes":r.bytes,"original_print_output":r.retained_office_provenance.is_none(),"immutable":true});
    if let Some(p)=&r.export_provenance { value["export_provenance"]=json!(p); }
    if let Some(p)=&r.retained_office_provenance { value["retained_office_provenance"]=json!(p); }
    if let Some(scope)=&r.reviewed_packet_scope {
        value["reviewed_packet_scope"]=json!(scope);
        value["content_transport"]=reviewed_packets::transport(r);
    }
    value
}
fn static_object(o: &lopdf::Object, depth: usize, nodes: &mut usize) -> bool {
    *nodes+=1;if depth>32 || *nodes>200_000{return false}
    match o {
        lopdf::Object::Array(values)=>values.iter().all(|v|static_object(v,depth+1,nodes)),
        lopdf::Object::Dictionary(d)=>static_dictionary(d,depth,nodes),
        lopdf::Object::Stream(s)=>static_dictionary(&s.dict,depth,nodes),
        _=>true,
    }
}
fn static_dictionary(d: &lopdf::Dictionary, depth: usize, nodes: &mut usize) -> bool {
    if [b"JS".as_slice(),b"JavaScript",b"EmbeddedFiles",b"XFA",b"AA",b"OpenAction",b"RichMediaContent",b"AcroForm"].iter().any(|key|d.has(key)) || d.get(b"S").ok().and_then(|s|s.as_name().ok()).is_some_and(|s|[b"JavaScript".as_slice(),b"Launch",b"SubmitForm",b"ImportData",b"GoToR"].contains(&s)) || d.get(b"Type").ok().and_then(|s|s.as_name().ok())==Some(b"EmbeddedFile".as_slice()) {return false}
    d.iter().all(|(_,v)|static_object(v,depth+1,nodes))
}
fn validate_static_pdf(bytes: &[u8], pages: usize) -> Result<()> {
    let doc=lopdf::Document::load_mem(bytes).map_err(|_|bad("Return PDF could not be parsed"))?;
    if doc.is_encrypted() { return Err(bad("Encrypted PDF requires an authorized readable export; original remains preserved separately")); }
    if doc.get_pages().len()!=pages { return Err(bad("Page index does not match parsed PDF page count")); }
    let mut nodes=0;
    if !doc.objects.values().all(|o|static_object(o,0,&mut nodes)) { return Err(bad("Active or overly nested PDF content is not supported by the source preview")); }
    Ok(())
}
impl SourceImport {
    pub async fn test_copy_preserve_return_pdf(&self, company: &str, v: ReturnPdfInput) -> Result<Value> {
        self.test_copy_guard(company,v.test_copy)?;
        if let Some(p)=&v.export_provenance { validate_export(p)?; }
        if v.retained_office_provenance.is_some() && serde_json::to_vec(&v).map_err(|_|bad("Cannot encode office PDF transport"))?.len()>OFFICE_JSON_LIMIT {return Err(bad("Office PDF JSON exceeds transport bound"));}
        if !number(&v.client_number) || !(1990..=2100).contains(&v.tax_year) || !digest(&v.sha256) { return Err(bad("Invalid PDF client, year or digest")); }
        if v.content_hex.len()>MAX_PDF_BYTES*2 { return Err(bad("Return PDF exceeds 2 MiB bound")); }
        if v.page_labels.is_empty() || v.page_labels.len()>500 || v.page_labels.iter().any(|s|s.is_empty() || s.len()>120 || s.chars().any(char::is_control)) { return Err(bad("Invalid PDF page index")); }
        let client_key=format!("dtmaxtc:{company}:client:{}",v.client_number);
        if self.store.get(&client_key).await.map_err(|e|storage(&e))?.is_none() { return Err(missing("PDF client is not in this source scope")); }
        let bytes=hex::decode(&v.content_hex).map_err(|_|bad("Invalid PDF hex"))?;
        if hex::encode(&bytes)!=v.content_hex || hex::encode(Sha256::digest(&bytes))!=v.sha256 { return Err(bad("PDF bytes do not match declared digest")); }
        let record=Record{client_number:v.client_number.clone(),tax_year:v.tax_year,sha256:v.sha256.clone(),page_labels:v.page_labels,bytes:bytes.len(),export_provenance:v.export_provenance,retained_office_provenance:v.retained_office_provenance,reviewed_packet_scope:None};
        validate_record_provenance(&record)?;
        validate_static_pdf(&bytes,record.page_labels.len())?;
        let identity=format!("{}:{}:{}",record.client_number,record.tax_year,record.sha256);
        let raw=serde_json::to_string(&record).map_err(|_|storage("encode PDF record"))?;
        self.store.set_nx(&k(company,&format!("blob:{}",record.sha256)),&v.content_hex).await.map_err(|e|storage(&e))?;
        let created=self.store.set_nx(&k(company,&format!("record:{identity}")),&raw).await.map_err(|e|storage(&e))?;
        if self.store.get(&k(company,&format!("record:{identity}"))).await.map_err(|e|storage(&e))?.as_deref()!=Some(&raw) { return Err(conflict("Original PDF record is immutable")); }
        // Repair the client index on matching retries after an interrupted write.
        self.store.sadd(&k(company,&format!("client:{}",record.client_number)),&identity).await.map_err(|e|storage(&e))?;
        Ok(json!({"test_copy":true,"created":created,"record":describe(&record),"tax_cases_imported":0}))
    }
    pub async fn test_copy_return_pdfs(&self, company: &str, client: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?; if !number(client) { return Err(bad("Invalid PDF client")); }
        let mut ids=self.store.smembers(&k(company,&format!("client:{client}"))).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>1000 { return Err(storage("PDF version index exceeds bound")); }
        let keys:Vec<_>=ids.iter().map(|id|k(company,&format!("record:{id}"))).collect();let mut records=Vec::new();
        for raw in self.store.mget(&keys).await.map_err(|e|storage(&e))? {
            let r:Record=serde_json::from_str(&raw.ok_or_else(||storage("PDF index record missing"))?).map_err(|_|storage("Invalid PDF record"))?;
            if r.client_number!=client { return Err(storage("PDF client mismatch")); }
            validate_record_provenance(&r).map_err(|_|storage("Stored PDF provenance differs"))?;
            records.push(describe(&r));
        }
        Ok(json!({"test_copy":true,"source_scope":company,"client_number":client,"records":records,"tax_cases_imported":0}))
    }
    pub async fn test_copy_return_pdf_content(&self, company: &str, client: &str, year: u16, sha: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?; if !number(client) || !digest(sha) || !(1990..=2100).contains(&year) { return Err(bad("Invalid PDF identity")); }
        let raw=self.store.get(&k(company,&format!("record:{client}:{year}:{sha}"))).await.map_err(|e|storage(&e))?.ok_or_else(||missing("No original return PDF for this client and year"))?;
        let r:Record=serde_json::from_str(&raw).map_err(|_|storage("Invalid PDF record"))?;
        if r.client_number!=client || r.tax_year!=year || r.sha256!=sha { return Err(storage("PDF ownership mismatch")); }
        validate_record_provenance(&r).map_err(|_|storage("Stored PDF provenance differs"))?;
        if reviewed_packets::is_packet(&r) {
            return Ok(json!({"test_copy":true,"source_scope":company,"record":describe(&r),"content_transport":reviewed_packets::transport(&r),"tax_cases_imported":0}));
        }
        let content=self.store.get(&k(company,&format!("blob:{sha}"))).await.map_err(|e|storage(&e))?.ok_or_else(||storage("PDF bytes missing"))?;
        if content.len()>MAX_PDF_BYTES*2 { return Err(storage("PDF blob exceeds bound")); }
        let bytes=hex::decode(&content).map_err(|_|storage("Invalid stored PDF bytes"))?;
        if bytes.len()!=r.bytes || hex::encode(Sha256::digest(&bytes))!=sha { return Err(storage("Stored PDF failed hash verification")); }
        let response=json!({"test_copy":true,"source_scope":company,"record":describe(&r),"content_hex":content,"tax_cases_imported":0});
        if r.retained_office_provenance.is_some() && serde_json::to_vec(&response).map_err(|_|storage("Cannot encode office PDF response"))?.len()>OFFICE_JSON_LIMIT {return Err(storage("Office PDF response exceeds transport bound"));}
        Ok(response)
    }
}
