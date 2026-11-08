//! Closed, immutable scanned packets. Chunks keep large original bytes intact;
//! committed associations identify an explicitly reviewed section, not a new return.
use super::*;

pub const PROFILE: &str = "a51e018e4cf6d5901baf269d9ea7fd9be651ee34b3213f80eae11b8480250775";
const CLOSED_SHA: &str = "33af6a05d7d01c2eba3b7a0173f139918633da18300d9d3e9962c7a5f97cf469";
pub const CHUNK_BYTES: usize = 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 20 * 1024 * 1024;
const KIND: &str = "retained_office_T1_reviewed_packet";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewedPacketScope {
    pub source_tax_years: Vec<u16>,
    pub year_first_page: usize,
    pub year_last_page: usize,
    pub federal_core_pages_present: Vec<usize>,
    pub federal_core_pages_expected: usize,
    pub federal_core_sequence_complete: bool,
    pub marked_amended: bool,
    pub hand_annotations_present: bool,
    pub full_return_completeness_verified: bool,
}

#[derive(Deserialize)]
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
        let raw = include_bytes!("retained_office_T1_reviewed_packet_profile.json");
        assert_eq!(hex::encode(Sha256::digest(raw)), CLOSED_SHA);
        let p: Profile = serde_json::from_slice(raw).expect("closed reviewed scanned packet profile");
        assert_eq!(p.profile_sha256, PROFILE);
        assert_eq!(hex::encode(Sha256::digest(serde_json::to_vec(&p.contract).unwrap())), PROFILE);
        assert_eq!((p.record_count, p.document_count), (8, 4));
        assert_eq!(p.allowed_record_sha256.len(), 8);
        assert_eq!(p.document_sha256.len(), 4);
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
        p
    })
}
pub(super) fn document_allowed(sha: &str) -> bool { profile().document_sha256.contains(sha) }
pub(super) fn is_packet(r: &Record) -> bool {
    r.retained_office_provenance.as_ref().is_some_and(|p| p.kind == KIND)
}
fn descriptor(r: &Record) -> Value {
    let mut value = office_descriptor(r);
    value["reviewed_packet_scope"] = json!(r.reviewed_packet_scope);
    value
}
pub(super) fn validate_record(r: &Record) -> Result<()> {
    let p = profile();
    let provenance = r.retained_office_provenance.as_ref().ok_or_else(|| bad("Packet provenance missing"))?;
    let scope = r.reviewed_packet_scope.as_ref().ok_or_else(|| bad("Reviewed packet year section missing"))?;
    let d = p.documents.get(&r.sha256).ok_or_else(|| bad("Packet source digest is not admitted"))?;
    if provenance.kind != KIND || provenance.profile_sha256 != PROFILE
        || provenance.source_manifest_sha256 != p.source_packet_sha256
        || r.export_provenance.is_some() || r.bytes != d.bytes || r.page_labels.len() != d.pages
        || !number(&r.client_number) || !(1990..=2100).contains(&r.tax_year)
        || provenance.native_generation_verified || provenance.as_filed_verified
        || provenance.financial_values_approved || provenance.source_bytes_modified
        || [&provenance.source_manifest_sha256, &provenance.source_occurrence_manifest_sha256,
            &provenance.owner_year_evidence_sha256, &provenance.mixed_owner_year_review_sha256,
            &provenance.static_pdf_review_sha256, &provenance.source_metadata_sha256].iter().any(|s| !digest(s))
        || r.page_labels.iter().any(|s| s.is_empty() || s.len() > 120 || s.chars().any(char::is_control))
        || scope.full_return_completeness_verified || scope.year_first_page == 0
        || scope.year_first_page > scope.year_last_page || scope.year_last_page > d.pages
        || scope.source_tax_years.is_empty() || scope.source_tax_years.len() > 8
        || scope.source_tax_years.windows(2).any(|pair| pair[0] >= pair[1])
        || !scope.source_tax_years.contains(&r.tax_year)
        || ![4, 8].contains(&scope.federal_core_pages_expected)
        || scope.federal_core_pages_present.is_empty()
        || scope.federal_core_pages_present.len() > scope.federal_core_pages_expected
        || scope.federal_core_pages_present != (1..=scope.federal_core_pages_present.len()).collect::<Vec<_>>()
        || scope.federal_core_sequence_complete != (scope.federal_core_pages_present.len() == scope.federal_core_pages_expected)
        || scope.year_last_page - scope.year_first_page + 1 < scope.federal_core_pages_present.len()
    { return Err(bad("Reviewed scanned packet scope or provenance differs")); }
    let raw = serde_json::to_vec(&descriptor(r)).map_err(|_| bad("Cannot encode packet descriptor"))?;
    if !p.allowed_record_sha256.contains(&hex::encode(Sha256::digest(raw))) {
        return Err(bad("Packet association is outside reviewed source owner/year evidence"));
    }
    Ok(())
}
pub(super) fn transport(r: &Record) -> Value {
    json!({"kind":"reviewed_scanned_packet_chunks_v1","chunk_bytes":CHUNK_BYTES,"chunk_count":profile().documents[&r.sha256].chunk_sha256.len()})
}
fn chunk_key(company: &str, sha: &str, index: usize) -> String {
    k(company, &format!("reviewed_packet_blob:{sha}:chunk:{index}"))
}
fn checked_chunk(sha: &str, index: usize, content: &str) -> Result<Vec<u8>> {
    let d = profile().documents.get(sha).ok_or_else(|| bad("Packet source digest is not admitted"))?;
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
pub struct PacketChunkInput {
    pub test_copy: bool,
    pub sha256: String,
    pub chunk_index: usize,
    pub chunk_sha256: String,
    pub content_hex: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacketCommitInput {
    pub test_copy: bool,
    pub client_number: String,
    pub tax_year: u16,
    pub sha256: String,
    pub bytes: usize,
    pub page_labels: Vec<String>,
    pub retained_office_provenance: RetainedOfficeProvenance,
    pub reviewed_packet_scope: ReviewedPacketScope,
}

impl SourceImport {
    pub async fn test_copy_reviewed_packet_profile(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let p=profile();
        Ok(json!({"test_copy":true,"source_scope":company,"profile_sha256":p.profile_sha256,"closed_profile_sha256":CLOSED_SHA,"source_packet_sha256":p.source_packet_sha256,"unique_original_documents":p.document_count,"owner_year_associations":p.record_count,"chunk_bytes":CHUNK_BYTES,"maximum_document_bytes":MAX_DOCUMENT_BYTES,"original_source_bytes_changed":false,"tax_cases_imported":0}))
    }
    pub async fn test_copy_preserve_reviewed_packet_chunk(&self, company: &str, v: PacketChunkInput) -> Result<Value> {
        self.test_copy_guard(company, v.test_copy)?;
        if !digest(&v.sha256) || !digest(&v.chunk_sha256) || v.content_hex.len() > CHUNK_BYTES * 2 {
            return Err(bad("Invalid reviewed packet chunk"));
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
    pub async fn test_copy_commit_reviewed_packet(&self, company: &str, v: PacketCommitInput) -> Result<Value> {
        self.test_copy_guard(company, v.test_copy)?;
        let r = Record {client_number:v.client_number, tax_year:v.tax_year, sha256:v.sha256,
            bytes:v.bytes, page_labels:v.page_labels, export_provenance:None,
            retained_office_provenance:Some(v.retained_office_provenance), reviewed_packet_scope:Some(v.reviewed_packet_scope)};
        validate_record(&r)?;
        if self.store.get(&format!("dtmaxtc:{company}:client:{}", r.client_number)).await.map_err(|e| storage(&e))?.is_none() {
            return Err(missing("Packet owner is not in this source scope"));
        }
        let d = &profile().documents[&r.sha256];
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
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"record":describe(&r),"tax_cases_imported":0}))
    }
    pub async fn test_copy_reviewed_packet_chunk(&self, company: &str, client: &str, year: u16, sha: &str, index: usize) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        if !number(client) || !digest(sha) || !(1990..=2100).contains(&year) { return Err(bad("Invalid packet identity")); }
        let raw = self.store.get(&k(company, &format!("record:{client}:{year}:{sha}"))).await.map_err(|e| storage(&e))?
            .ok_or_else(|| missing("No reviewed packet association for this owner and year"))?;
        let r: Record = serde_json::from_str(&raw).map_err(|_| storage("Invalid packet association"))?;
        if r.client_number != client || r.tax_year != year || r.sha256 != sha { return Err(storage("Packet ownership mismatch")); }
        validate_record(&r).map_err(|_| storage("Stored packet source evidence differs"))?;
        let content = self.store.get(&chunk_key(company, sha, index)).await.map_err(|e| storage(&e))?
            .ok_or_else(|| storage("Original packet chunk missing"))?;
        let bytes = checked_chunk(sha, index, &content).map_err(|_| storage("Stored packet chunk failed verification"))?;
        Ok(json!({"test_copy":true,"source_scope":company,"client_number":client,"tax_year":year,"sha256":sha,"chunk_index":index,"chunk_sha256":hex::encode(Sha256::digest(&bytes)),"bytes":bytes.len(),"content_hex":content,"tax_cases_imported":0}))
    }
}
