//! Closed original source archives. Only the listed identification or summary pages
//! establish owner/year links. Unreviewed body pages are never promoted to returns.
use super::*;

pub const PROFILE: &str = "07747dec1e7e0e03365da5252bff44b12299291dad250cc49cb7ea9cb1389d97";
const CLOSED_SHA: &str = "4e0e607dedb8d6c8cb7bd322b4dbecff2e92f962b74684e705faec1d0a9cb233";
const ADDITIONAL_PROFILE: &str = "3985c4e4c02638f4abdceb6723fc8b37fea46db25c54306b4da799a5c53a7de4";
const ADDITIONAL_CLOSED_SHA: &str = "b488297b50d116e199883838ff5dadbb7feb73e92b82754aa86ed610e3a68b04";
const MIXED_PROFILE: &str = "e14edac6ccc8d3d1067997ce45b858ec943317e03f988bbc47cedae384c21418";
const MIXED_CLOSED_SHA: &str = "a76279e1b6ef820b7bd47b624e362847e8b063f9839545990b45501f367b4bca";
pub const CHUNK_BYTES: usize = 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
const KIND: &str = "retained_office_identification_source_archive";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceArchiveScope {
    pub source_role: String,
    pub verified_source_pages: Vec<VerifiedSourcePage>,
    pub same_year_native_context_present_at_source_audit: bool,
    pub remaining_packet_pages_unverified: bool,
    pub full_return_completeness_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_mixed_client_packet: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name_match: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name_qualification: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printed_slip_observations_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printed_slip_observations_sha256: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerifiedSourcePage {
    pub page: usize,
    pub layout: String,
    pub marked_amended: bool,
    pub annotations_visible: bool,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Document {
    bytes: usize,
    pages: usize,
    chunk_sha256: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    contract: Value,
    profile_sha256: String,
    source_packet_sha256: String,
    record_count: usize,
    document_count: usize,
    allowed_record_sha256: BTreeSet<String>,
    document_sha256: BTreeSet<String>,
    documents: BTreeMap<String, Document>,
}
fn profile() -> &'static Profile {
    static P: OnceLock<Profile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("identification_source_archive_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)), CLOSED_SHA);
        let p: Profile = serde_json::from_slice(raw).expect("closed identification source archive profile");
        assert_eq!(p.profile_sha256, PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())), PROFILE);
        assert_eq!((p.record_count, p.document_count), (131, 69));
        assert_eq!(p.allowed_record_sha256.len(), 131);
        assert_eq!(p.document_sha256.len(), 69);
        assert_eq!(p.documents.keys().cloned().collect::<BTreeSet<_>>(), p.document_sha256);
        assert!(digest(&p.source_packet_sha256));
        assert!(p.allowed_record_sha256.iter().chain(&p.document_sha256).all(|s| digest(s)));
        for d in p.documents.values() {
            assert!(d.bytes > 0 && d.bytes <= MAX_DOCUMENT_BYTES && d.pages > 0 && d.pages <= 500);
            assert_eq!(d.chunk_sha256.len(), d.bytes.div_ceil(CHUNK_BYTES));
            assert!(d.chunk_sha256.iter().all(|s| digest(s)));
        }
        assert!(p.document_sha256.is_disjoint(&office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&stable_office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&scanned_office_profile().document_sha256));
        assert!(p.document_sha256.iter().all(|s| !reviewed_packets::document_allowed(s)));
        p
    })
}
fn additional_profile() -> &'static Profile {
    static P: OnceLock<Profile> = OnceLock::new();
    P.get_or_init(|| {
        let raw=include_bytes!("additional_identification_source_archive_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)),ADDITIONAL_CLOSED_SHA);
        let p:Profile=serde_json::from_slice(raw).expect("closed additional source archive profile");
        assert_eq!(p.profile_sha256,ADDITIONAL_PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())),ADDITIONAL_PROFILE);
        assert_eq!((p.record_count,p.document_count),(22,19));
        assert_eq!((p.allowed_record_sha256.len(),p.document_sha256.len()),(22,19));
        assert_eq!(p.documents.keys().cloned().collect::<BTreeSet<_>>(),p.document_sha256);
        assert!(digest(&p.source_packet_sha256));
        assert!(p.allowed_record_sha256.iter().chain(&p.document_sha256).all(|s|digest(s)));
        let prior=profile();assert!(p.allowed_record_sha256.is_disjoint(&prior.allowed_record_sha256));
        let mut reused=0;
        for (sha,d) in &p.documents {
            assert!(d.bytes>0&&d.bytes<=MAX_DOCUMENT_BYTES&&d.pages>0&&d.pages<=500);
            assert_eq!(d.chunk_sha256.len(),d.bytes.div_ceil(CHUNK_BYTES));
            assert!(d.chunk_sha256.iter().all(|s|digest(s)));
            if let Some(old)=prior.documents.get(sha){assert!(d==old);reused+=1;}
        }
        assert_eq!(reused,3);
        assert!(p.document_sha256.is_disjoint(&office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&stable_office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&scanned_office_profile().document_sha256));
        assert!(p.document_sha256.iter().all(|s|!reviewed_packets::document_allowed(s)));
        p
    })
}
fn mixed_profile() -> &'static Profile {
    static P: OnceLock<Profile> = OnceLock::new();
    P.get_or_init(|| {
        let raw=include_bytes!("mixed_identification_source_archive_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)),MIXED_CLOSED_SHA);
        let p:Profile=serde_json::from_slice(raw).expect("closed mixed source archive profile");
        assert_eq!(p.profile_sha256,MIXED_PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())),MIXED_PROFILE);
        assert_eq!((p.record_count,p.document_count),(22,6));
        assert_eq!((p.allowed_record_sha256.len(),p.document_sha256.len()),(22,6));
        assert_eq!(p.documents.keys().cloned().collect::<BTreeSet<_>>(),p.document_sha256);
        assert!(digest(&p.source_packet_sha256));
        assert!(p.allowed_record_sha256.iter().chain(&p.document_sha256).all(|s|digest(s)));
        for prior in [profile(),additional_profile()] {
            assert!(p.allowed_record_sha256.is_disjoint(&prior.allowed_record_sha256));
            assert!(p.document_sha256.is_disjoint(&prior.document_sha256));
        }
        for d in p.documents.values() {
            assert!(d.bytes>0&&d.bytes<=MAX_DOCUMENT_BYTES&&d.pages>0&&d.pages<=500);
            assert_eq!(d.chunk_sha256.len(),d.bytes.div_ceil(CHUNK_BYTES));
            assert!(d.chunk_sha256.iter().all(|s|digest(s)));
        }
        assert!(p.document_sha256.is_disjoint(&office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&stable_office_profile().document_sha256));
        assert!(p.document_sha256.is_disjoint(&scanned_office_profile().document_sha256));
        assert!(p.document_sha256.iter().all(|s|!reviewed_packets::document_allowed(s)));
        p
    })
}
fn document(sha: &str) -> Option<&'static Document> {
    profile().documents.get(sha).or_else(||additional_profile().documents.get(sha)).or_else(||mixed_profile().documents.get(sha))
}
pub(super) fn document_allowed(sha: &str) -> bool { document(sha).is_some() }
pub(super) fn is_archive(r: &Record) -> bool {
    r.retained_office_provenance.as_ref().is_some_and(|p| p.kind == KIND)
}
fn descriptor(r: &Record) -> Value {
    let mut value = office_descriptor(r);
    value["source_archive_scope"] = json!(r.source_archive_scope);
    value
}
pub(super) fn validate_record(r: &Record) -> Result<()> {
    let provenance = r.retained_office_provenance.as_ref().ok_or_else(|| bad("Packet provenance missing"))?;
    let p=match provenance.profile_sha256.as_str(){PROFILE=>profile(),ADDITIONAL_PROFILE=>additional_profile(),MIXED_PROFILE=>mixed_profile(),_=>return Err(bad("Packet source profile is not admitted"))};
    let scope = r.source_archive_scope.as_ref().ok_or_else(|| bad("Source archive year section missing"))?;
    let d = p.documents.get(&r.sha256).ok_or_else(|| bad("Packet source digest is not admitted"))?;
    if provenance.kind != KIND || provenance.profile_sha256 != p.profile_sha256
        || provenance.source_manifest_sha256 != p.source_packet_sha256
        || r.export_provenance.is_some() || r.bytes != d.bytes || r.page_labels.len() != d.pages
        || !number(&r.client_number) || !(1990..=2100).contains(&r.tax_year)
        || provenance.native_generation_verified || provenance.as_filed_verified
        || provenance.financial_values_approved || provenance.source_bytes_modified
        || [&provenance.source_manifest_sha256, &provenance.source_occurrence_manifest_sha256,
            &provenance.owner_year_evidence_sha256, &provenance.mixed_owner_year_review_sha256,
            &provenance.static_pdf_review_sha256, &provenance.source_metadata_sha256].iter().any(|s| !digest(s))
        || r.page_labels.iter().any(|s| s.is_empty() || s.len() > 120 || s.chars().any(char::is_control))
        || scope.full_return_completeness_verified
        || !(["T1_identification_archive", "tax_summary_and_supporting_slips"].contains(&scope.source_role.as_str()) || (p.profile_sha256==MIXED_PROFILE && scope.source_role=="printed_slip_identification_archive"))
        || scope.verified_source_pages.is_empty() || scope.verified_source_pages.len() > 20
        || scope.verified_source_pages.iter().any(|a| a.page == 0 || a.page > d.pages
            || !(["regular", "condensed", "summary"].contains(&a.layout.as_str())
                || ([ADDITIONAL_PROFILE,MIXED_PROFILE].contains(&p.profile_sha256.as_str()) && a.layout=="unclassified")))
        || scope.verified_source_pages.windows(2).any(|a| a[0].page >= a[1].page)
        || (scope.source_role == "T1_identification_archive" && (!scope.remaining_packet_pages_unverified || scope.verified_source_pages.iter().any(|a| a.layout == "summary")))
        || (scope.source_role == "tax_summary_and_supporting_slips" && (scope.remaining_packet_pages_unverified || scope.verified_source_pages.len() != 1 || scope.verified_source_pages[0].layout != "summary"))
        || r.reviewed_packet_scope.is_some()
    { return Err(bad("Source archive scope or provenance differs")); }
    let extended=scope.known_mixed_client_packet.is_some()||scope.source_name_match.is_some()||scope.source_name_qualification.is_some()||scope.printed_slip_observations_json.is_some()||scope.printed_slip_observations_sha256.is_some();
    if p.profile_sha256!=MIXED_PROFILE && extended {return Err(bad("New source qualifications require their closed profile"));}
    if p.profile_sha256==MIXED_PROFILE {
        if scope.known_mixed_client_packet==Some(false)||!scope.remaining_packet_pages_unverified {return Err(bad("Mixed archive ownership qualification differs"));}
        if scope.source_role=="printed_slip_identification_archive" {
            if scope.same_year_native_context_present_at_source_audit||scope.source_name_match.is_some()||scope.source_name_qualification.is_some()||scope.known_mixed_client_packet.is_some()||scope.verified_source_pages.len()!=1 {return Err(bad("Printed slip identification scope differs"));}
            let text=scope.printed_slip_observations_json.as_ref().ok_or_else(||bad("Printed source observations missing"))?;
            if text.len()>8192||scope.printed_slip_observations_sha256.as_deref()!=Some(hex::encode(Sha256::digest(text.as_bytes())).as_str()) {return Err(bad("Printed observation source hash differs"));}
            let _:Value=serde_json::from_str(text).map_err(|_|bad("Printed observations are not JSON"))?;
        } else if scope.source_role!="T1_identification_archive"||scope.printed_slip_observations_json.is_some()||scope.printed_slip_observations_sha256.is_some()||scope.source_name_match.is_none()||!matches!(scope.source_name_qualification.as_deref(),Some("whole_name_cells_match_original"|"courtesy_title_in_first_name_cell"|"first_name_spelling_variation")) {return Err(bad("General identification source qualification differs"));}
    }
    let raw = serde_json::to_vec(&descriptor(r)).map_err(|_| bad("Cannot encode packet descriptor"))?;
    if !p.allowed_record_sha256.contains(&hex::encode(Sha256::digest(raw))) {
        return Err(bad("Packet association is outside reviewed source owner/year evidence"));
    }
    Ok(())
}
pub(super) fn transport(r: &Record) -> Value {
    json!({"kind":"identification_source_archive_chunks_v1","chunk_bytes":CHUNK_BYTES,"chunk_count":document(&r.sha256).expect("validated source archive").chunk_sha256.len()})
}
fn chunk_key(company: &str, sha: &str, index: usize) -> String {
    k(company, &format!("identification_archive_blob:{sha}:chunk:{index}"))
}
fn checked_chunk(sha: &str, index: usize, content: &str) -> Result<Vec<u8>> {
    let d = document(sha).ok_or_else(|| bad("Packet source digest is not admitted"))?;
    let expected_hash = d.chunk_sha256.get(index).ok_or_else(|| bad("Packet chunk index is outside source"))?;
    let expected_bytes = (d.bytes - index * CHUNK_BYTES).min(CHUNK_BYTES);
    if content.len() != expected_bytes * 2 { return Err(bad("Packet chunk byte length differs")); }
    let bytes = hex::decode(content).map_err(|_| bad("Invalid packet chunk encoding"))?;
    if hex::encode(&bytes) != content || hex::encode(Sha256::digest(&bytes)) != *expected_hash {
        return Err(bad("Packet chunk failed source hash verification"));
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveChunkInput {
    pub test_copy: bool,
    pub sha256: String,
    pub chunk_index: usize,
    pub chunk_sha256: String,
    pub content_hex: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveCommitInput {
    pub test_copy: bool,
    pub client_number: String,
    pub tax_year: u16,
    pub sha256: String,
    pub bytes: usize,
    pub page_labels: Vec<String>,
    pub retained_office_provenance: RetainedOfficeProvenance,
    pub source_archive_scope: SourceArchiveScope,
}

impl SourceImport {
    pub(in crate::source_import) async fn identification_source_archive_years(&self, company: &str) -> Result<BTreeMap<String, BTreeSet<u16>>> {
        let mut ids=self.store.smembers(&k(company, "identification_archive_associations")).await.map_err(|e| storage(&e))?;
        ids.sort();
        if ids.len()>profile().record_count+additional_profile().record_count+mixed_profile().record_count { return Err(storage("Source archive year index exceeds closed bound")); }
        let keys:Vec<_>=ids.iter().map(|id|k(company,&format!("record:{id}"))).collect();
        let mut years:BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        for (id,raw) in ids.iter().zip(self.store.mget(&keys).await.map_err(|e|storage(&e))?) {
            let r:Record=serde_json::from_str(&raw.ok_or_else(||storage("Source archive year index is incomplete"))?).map_err(|_|storage("Invalid source archive association"))?;
            if *id!=format!("{}:{}:{}",r.client_number,r.tax_year,r.sha256) {return Err(storage("Source archive year identity differs"));}
            validate_record(&r).map_err(|_|storage("Source archive year evidence differs"))?;
            years.entry(r.client_number).or_default().insert(r.tax_year);
        }
        Ok(years)
    }
    pub async fn test_copy_source_archive_profile(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let p=profile();
        Ok(json!({"test_copy":true,"source_scope":company,"profile_sha256":p.profile_sha256,"closed_profile_sha256":CLOSED_SHA,"source_packet_sha256":p.source_packet_sha256,"unique_original_documents":p.document_count,"owner_year_associations":p.record_count,"chunk_bytes":CHUNK_BYTES,"maximum_document_bytes":MAX_DOCUMENT_BYTES,"original_source_bytes_changed":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_additional_source_archive_profile(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        let p=additional_profile();
        Ok(json!({"test_copy":true,"source_scope":company,"profile_sha256":p.profile_sha256,"closed_profile_sha256":ADDITIONAL_CLOSED_SHA,"source_packet_sha256":p.source_packet_sha256,"unique_original_documents":p.document_count,"new_unique_original_documents":16,"reused_prior_original_documents":3,"owner_year_associations":p.record_count,"chunk_bytes":CHUNK_BYTES,"maximum_document_bytes":MAX_DOCUMENT_BYTES,"original_source_bytes_changed":false,"prior_immutable_associations_changed":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_mixed_source_archive_profile(&self,company:&str)->Result<Value>{
        self.test_copy_guard(company,true)?;let p=mixed_profile();
        Ok(json!({"test_copy":true,"source_scope":company,"profile_sha256":p.profile_sha256,"closed_profile_sha256":MIXED_CLOSED_SHA,"source_packet_sha256":p.source_packet_sha256,"unique_original_documents":p.document_count,"owner_year_associations":p.record_count,"General_identification_associations":20,"printed_slip_identification_associations":2,"physical_printed_slip_copies":4,"chunk_bytes":CHUNK_BYTES,"maximum_document_bytes":MAX_DOCUMENT_BYTES,"original_source_bytes_changed":false,"prior_immutable_associations_changed":false,"native_contexts_created":0,"normalized_slips_created":0,"financial_values_approved":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_preserve_source_archive_chunk(&self, company: &str, v: ArchiveChunkInput) -> Result<Value> {
        self.test_copy_guard(company, v.test_copy)?;
        if !digest(&v.sha256) || !digest(&v.chunk_sha256) || v.content_hex.len() > CHUNK_BYTES * 2 {
            return Err(bad("Invalid source archive chunk"));
        }
        let bytes = checked_chunk(&v.sha256, v.chunk_index, &v.content_hex)?;
        if hex::encode(Sha256::digest(&bytes)) != v.chunk_sha256 { return Err(bad("Declared packet chunk digest differs")); }
        let key = chunk_key(company, &v.sha256, v.chunk_index);
        let created = self.store.set_nx(&key, &v.content_hex).await.map_err(|e| storage(&e))?;
        if self.store.get(&key).await.map_err(|e| storage(&e))?.as_deref() != Some(&v.content_hex) {
            return Err(conflict("Original packet chunk is immutable"));
        }
        Ok(json!({"test_copy":true,"source_scope":company,"sha256":v.sha256,"chunk_index":v.chunk_index,"chunk_sha256":v.chunk_sha256,"bytes":bytes.len(),"created":created,"owner_association_committed":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_commit_source_archive(&self, company: &str, v: ArchiveCommitInput) -> Result<Value> {
        self.test_copy_guard(company, v.test_copy)?;
        let r = Record {client_number:v.client_number, tax_year:v.tax_year, sha256:v.sha256,
            bytes:v.bytes, page_labels:v.page_labels, export_provenance:None,
            retained_office_provenance:Some(v.retained_office_provenance), source_archive_scope:Some(v.source_archive_scope), reviewed_packet_scope:None};
        validate_record(&r)?;
        if self.store.get(&format!("dtmaxtc:{company}:client:{}", r.client_number)).await.map_err(|e| storage(&e))?.is_none() {
            return Err(missing("Packet owner is not in this source scope"));
        }
        let d = document(&r.sha256).expect("validated source archive");
        let mut bytes = Vec::with_capacity(d.bytes);
        for index in 0..d.chunk_sha256.len() {
            let content = self.store.get(&chunk_key(company, &r.sha256, index)).await.map_err(|e| storage(&e))?
                .ok_or_else(|| missing("Original packet is incomplete; a source chunk is missing"))?;
            bytes.extend(checked_chunk(&r.sha256, index, &content).map_err(|_| storage("Stored packet chunk failed verification"))?);
        }
        if bytes.len() != r.bytes || hex::encode(Sha256::digest(&bytes)) != r.sha256 {
            return Err(storage("Complete original packet failed hash verification"));
        }
        validate_static_pdf(&bytes, r.page_labels.len())?;
        let identity = format!("{}:{}:{}", r.client_number, r.tax_year, r.sha256);
        let raw = serde_json::to_string(&r).map_err(|_| storage("encode packet association"))?;
        let key = k(company, &format!("record:{identity}"));
        let created = self.store.set_nx(&key, &raw).await.map_err(|e| storage(&e))?;
        if self.store.get(&key).await.map_err(|e| storage(&e))?.as_deref() != Some(&raw) {
            return Err(conflict("Original packet association is immutable"));
        }
        self.store.sadd(&k(company, &format!("client:{}", r.client_number)), &identity).await.map_err(|e| storage(&e))?;
        self.store.sadd(&k(company, "identification_archive_associations"), &identity).await.map_err(|e| storage(&e))?;
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"record":describe(&r),"tax_cases_imported":0}))
    }
    pub async fn test_copy_source_archive_chunk(&self, company: &str, client: &str, year: u16, sha: &str, index: usize) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        if !number(client) || !digest(sha) || !(1990..=2100).contains(&year) { return Err(bad("Invalid packet identity")); }
        let raw = self.store.get(&k(company, &format!("record:{client}:{year}:{sha}"))).await.map_err(|e| storage(&e))?
            .ok_or_else(|| missing("No source archive association for this owner and year"))?;
        let r: Record = serde_json::from_str(&raw).map_err(|_| storage("Invalid packet association"))?;
        if r.client_number != client || r.tax_year != year || r.sha256 != sha { return Err(storage("Packet ownership mismatch")); }
        validate_record(&r).map_err(|_| storage("Stored packet source evidence differs"))?;
        let content = self.store.get(&chunk_key(company, sha, index)).await.map_err(|e| storage(&e))?
            .ok_or_else(|| storage("Original packet chunk missing"))?;
        let bytes = checked_chunk(sha, index, &content).map_err(|_| storage("Stored packet chunk failed verification"))?;
        Ok(json!({"test_copy":true,"source_scope":company,"client_number":client,"tax_year":year,"sha256":sha,"chunk_index":index,"chunk_sha256":hex::encode(Sha256::digest(&bytes)),"bytes":bytes.len(),"content_hex":content,"tax_cases_imported":0}))
    }
}

#[cfg(test)]
mod source_archive_tests {
    use super::*;
    #[test]
    fn mixed_registry_is_additive_and_new_scope_is_omitted_from_old_descriptors() {
        let old=profile();let add=additional_profile();let p=mixed_profile();
        assert_eq!((old.record_count,add.record_count,p.record_count),(131,22,22));
        assert_eq!(p.documents.values().map(|d|d.bytes).sum::<usize>(),94560121);
        assert_eq!(p.documents.values().map(|d|d.pages).sum::<usize>(),207);
        assert_eq!(p.documents.values().map(|d|d.chunk_sha256.len()).sum::<usize>(),93);
        let raw=json!({"source_role":"T1_identification_archive","verified_source_pages":[{"page":1,"layout":"regular","marked_amended":false,"annotations_visible":false}],"same_year_native_context_present_at_source_audit":false,"remaining_packet_pages_unverified":true,"full_return_completeness_verified":false});
        let scope:SourceArchiveScope=serde_json::from_value(raw.clone()).unwrap();assert_eq!(serde_json::to_value(scope).unwrap(),raw);
    }
    #[test]
    fn optional_all_mixed_original_descriptors_and_qualification_negatives() {
        let Ok(path)=std::env::var("NXT_DTMAX_MIXED_SOURCE_ARCHIVE_MANIFEST")else{return};let path=std::path::Path::new(&path);
        let raw=std::fs::read(path).unwrap();assert_eq!(hex::encode(Sha256::digest(&raw)),"3fe88043a7380da4e0dda4f824c130a013f9a5329bfdab06ac1951b9f8996e4e");let m:Value=serde_json::from_slice(&raw).unwrap();let rows=m["records"].as_array().unwrap();assert_eq!(rows.len(),22);let mut seen=BTreeSet::new();let mut names=0;let mut mixed=0;let mut slips=0;
        for row in rows {
            let mut v=row.clone();v.as_object_mut().unwrap().remove("file");let r:Record=serde_json::from_value(v).unwrap();validate_record(&r).unwrap();let scope=r.source_archive_scope.as_ref().unwrap();names+=usize::from(scope.source_name_match==Some(false));mixed+=usize::from(scope.known_mixed_client_packet==Some(true));slips+=usize::from(scope.source_role=="printed_slip_identification_archive");
            for mode in ["owner","year","oldprofile","page","role","omitscope","name","native","approved","observation"] {
                let mut b=r.clone();match mode {"owner"=>b.client_number="999999999".into(),"year"=>b.tax_year=1990,"oldprofile"=>b.retained_office_provenance.as_mut().unwrap().profile_sha256=PROFILE.into(),"page"=>b.source_archive_scope.as_mut().unwrap().verified_source_pages[0].page=1,"role"=>b.source_archive_scope.as_mut().unwrap().source_role="complete_return".into(),"omitscope"=>b.source_archive_scope=None,"name"=>b.source_archive_scope.as_mut().unwrap().source_name_match=Some(true),"native"=>b.source_archive_scope.as_mut().unwrap().same_year_native_context_present_at_source_audit=!scope.same_year_native_context_present_at_source_audit,"approved"=>b.retained_office_provenance.as_mut().unwrap().financial_values_approved=true,_=>b.source_archive_scope.as_mut().unwrap().printed_slip_observations_json=Some("{}".into())};if descriptor(&b)!=descriptor(&r){assert!(validate_record(&b).is_err());}
            }
            if seen.insert(r.sha256.clone()){let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();assert_eq!(hex::encode(Sha256::digest(&bytes)),r.sha256);assert_eq!(bytes.len(),r.bytes);validate_static_pdf(&bytes,r.page_labels.len()).unwrap();for(i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){assert_eq!(checked_chunk(&r.sha256,i,&hex::encode(part)).unwrap(),part);}}
        }
        assert_eq!((seen.len(),names,mixed,slips),(6,6,6,2));println!("MIXED_SOURCE_CLOSED: originals=6; links=22; pages=207; bytes=94560121; chunks=93; name_match_false=6; mixed_client_links=6; printed_slip_records=2; no_source_values_logged=true");
    }
    #[test]
    fn additional_registry_keeps_prior_profile_and_reused_originals_exact() {
        let old=profile();let p=additional_profile();
        assert_eq!((old.document_count,old.record_count),(69,131));
        assert_eq!((p.document_count,p.record_count),(19,22));
        assert_eq!(p.documents.values().map(|d|d.bytes).sum::<usize>(),221792347);
        assert_eq!(p.documents.values().map(|d|d.pages).sum::<usize>(),507);
        let added:Vec<_>=p.documents.iter().filter(|(s,_)|!old.documents.contains_key(*s)).map(|(_,d)|d).collect();
        assert_eq!(added.len(),16);assert_eq!(added.iter().map(|d|d.bytes).sum::<usize>(),161645165);
        assert_eq!(added.iter().map(|d|d.pages).sum::<usize>(),382);
        assert_eq!(added.iter().map(|d|d.chunk_sha256.len()).sum::<usize>(),163);
        assert_eq!(old.record_count+p.record_count,153);
    }
    #[test]
    fn optional_all_additional_originals_and_twenty_two_links_match_closed_evidence() {
        let Ok(path)=std::env::var("NXT_DTMAX_ADDITIONAL_SOURCE_ARCHIVE_MANIFEST")else{return};let path=std::path::Path::new(&path);
        let raw=std::fs::read(path).unwrap();assert_eq!(hex::encode(Sha256::digest(&raw)),"730c9208edcc545fdefa410d73c81aa2b0460175526b96944a45af9728c360c5");
        let m:Value=serde_json::from_slice(&raw).unwrap();let rows=m["records"].as_array().unwrap();assert_eq!(rows.len(),22);let mut seen=BTreeSet::new();let mut anchors=0;let mut unclassified=0;
        for row in rows {
            let mut v=row.clone();v.as_object_mut().unwrap().remove("file");let r:Record=serde_json::from_value(v).unwrap();validate_record(&r).unwrap();
            let scope=r.source_archive_scope.as_ref().unwrap();anchors+=scope.verified_source_pages.len();unclassified+=scope.verified_source_pages.iter().filter(|a|a.layout=="unclassified").count();
            for field in ["owner","year","proof","full_return","source_scope","native_context","role","page","label","old_profile","unknown_profile","layout"] {
                let mut b=r.clone();match field {
                    "owner"=>b.client_number="999999999".into(),"year"=>b.tax_year=1990,
                    "proof"=>b.retained_office_provenance.as_mut().unwrap().owner_year_evidence_sha256="0".repeat(64),
                    "full_return"=>b.source_archive_scope.as_mut().unwrap().full_return_completeness_verified=true,
                    "source_scope"=>b.source_archive_scope.as_mut().unwrap().remaining_packet_pages_unverified=false,
                    "native_context"=>b.source_archive_scope.as_mut().unwrap().same_year_native_context_present_at_source_audit=!scope.same_year_native_context_present_at_source_audit,
                    "role"=>b.source_archive_scope.as_mut().unwrap().source_role="complete_T1".into(),
                    "page"=>b.source_archive_scope.as_mut().unwrap().verified_source_pages[0].page=0,
                    "label"=>b.page_labels[0]="Changed".into(),
                    "old_profile"=>b.retained_office_provenance.as_mut().unwrap().profile_sha256=PROFILE.into(),
                    "unknown_profile"=>b.retained_office_provenance.as_mut().unwrap().profile_sha256="0".repeat(64),
                    _=>b.source_archive_scope.as_mut().unwrap().verified_source_pages[0].layout="guessed".into(),
                };assert!(validate_record(&b).is_err(),"Unreviewed additional association accepted");
            }
            if seen.insert(r.sha256.clone()) {
                let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();assert_eq!(bytes.len(),r.bytes);assert_eq!(hex::encode(Sha256::digest(&bytes)),r.sha256);validate_static_pdf(&bytes,r.page_labels.len()).unwrap();
                for(i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){assert_eq!(checked_chunk(&r.sha256,i,&hex::encode(part)).unwrap(),part);}
                assert!(checked_chunk(&r.sha256,0,"00").is_err());
            }
        }
        assert_eq!((seen.len(),anchors,unclassified),(19,23,3));
        println!("ADDITIONAL_ARCHIVES_EXACT: documents=19; new_originals=16; new_pages=382; new_bytes=161645165; links=22; reviewed_panels=23; unclassified_layouts=3; catalog_chunks=222; descriptor_negative_checks=264; old131_unchanged=true");
    }
    #[test]
    fn closed_archive_profile_is_distinct_and_bounds_are_explicit() {
        let p=profile();assert_eq!((p.document_count,p.record_count),(69,131));
        assert_eq!(MAX_DOCUMENT_BYTES,64*1024*1024);assert_eq!(CHUNK_BYTES,1048576);
        assert_ne!(PROFILE,reviewed_packets::PROFILE);
        assert_eq!(p.documents.values().map(|d|d.bytes).sum::<usize>(),887704026);
        assert_eq!(p.documents.values().map(|d|d.pages).sum::<usize>(),2076);
        assert_eq!(p.documents.values().map(|d|d.chunk_sha256.len()).sum::<usize>(),882);
    }
    #[test]
    fn optional_all_actual_original_archives_and_page_links_match_closed_evidence() {
        let Ok(path)=std::env::var("NXT_DTMAX_SOURCE_ARCHIVE_MANIFEST")else{return};let path=std::path::Path::new(&path);
        let raw=std::fs::read(path).unwrap();assert_eq!(hex::encode(Sha256::digest(&raw)),"5e8ecd20055f0fcbdcbb0a549ae094e21640e141ecbb8232962a952cc5814640");
        let m:Value=serde_json::from_slice(&raw).unwrap();let rows=m["records"].as_array().unwrap();assert_eq!(rows.len(),131);let mut seen=BTreeSet::new();let mut anchors=0;
        for row in rows {
            let mut v=row.clone();v.as_object_mut().unwrap().remove("file");let r:Record=serde_json::from_value(v).unwrap();validate_record(&r).unwrap();
            let scope=r.source_archive_scope.as_ref().unwrap();anchors+=scope.verified_source_pages.len();
            for field in ["owner","year","evidence","full_return","scope","native_context","role","page","label"] {
                let mut bad=r.clone();match field {
                    "owner"=>bad.client_number="999999999".into(),"year"=>bad.tax_year=1990,
                    "evidence"=>bad.retained_office_provenance.as_mut().unwrap().owner_year_evidence_sha256="0".repeat(64),
                    "full_return"=>bad.source_archive_scope.as_mut().unwrap().full_return_completeness_verified=true,
                    "scope"=>bad.source_archive_scope.as_mut().unwrap().remaining_packet_pages_unverified=!scope.remaining_packet_pages_unverified,
                    "native_context"=>bad.source_archive_scope.as_mut().unwrap().same_year_native_context_present_at_source_audit=!scope.same_year_native_context_present_at_source_audit,
                    "role"=>bad.source_archive_scope.as_mut().unwrap().source_role="complete_T1".into(),
                    "page"=>bad.source_archive_scope.as_mut().unwrap().verified_source_pages[0].page=0,
                    _=>bad.page_labels[0]="Changed".into(),
                };assert!(validate_record(&bad).is_err(),"Unreviewed source archive change accepted");
            }
            if seen.insert(r.sha256.clone()) {
                let bytes=std::fs::read(path.parent().unwrap().join(row["file"].as_str().unwrap())).unwrap();assert_eq!(bytes.len(),r.bytes);assert_eq!(hex::encode(Sha256::digest(&bytes)),r.sha256);validate_static_pdf(&bytes,r.page_labels.len()).unwrap();
                for (i,part)in bytes.chunks(CHUNK_BYTES).enumerate(){assert_eq!(checked_chunk(&r.sha256,i,&hex::encode(part)).unwrap(),part);}
                assert!(checked_chunk(&r.sha256,0,"00").is_err());
            }
        }
        assert_eq!((seen.len(),anchors),(69,140));
        println!("SOURCE_ARCHIVES_ALL_EXACT: documents=69; physical_pages=2076; bytes=887704026; owner_year_links=131; verified_identification_panels=139; summary_anchor=1; chunks=882; descriptor_negative_checks=1179");
    }
}
