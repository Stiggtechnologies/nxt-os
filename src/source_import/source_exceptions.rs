//! Exact unreadable archive contexts, retained independently of client profiles.
//! Header IDs are source references only; they never establish a person or figures.
use super::{bad, conflict, missing, storage, ImportError, SourceImport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
type Result<T> = std::result::Result<T, ImportError>;
const MAX_CONTEXT: usize = 64 * 1024;
const MAX_INDEX: usize = 1_000;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceExceptionInput {
    pub test_copy: bool,
    pub source_archive_sha256: String,
    pub source_archive_bytes: u64,
    pub source_context_offset: u64,
    pub source_context_sha256: String,
    pub source_context_hex: String,
    pub source_year_frame_offset: u64,
    pub source_year_frame_sha256: String,
    pub source_year_frame_hex: String,
    pub family_file_id: String,
    pub client_number: String,
    pub tax_year: u16,
    /// Reported by the source reader; not an assertion of decoded semantics.
    pub reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExceptionPage {
    #[serde(default)] pub offset: usize,
    #[serde(default = "page_limit")] pub limit: usize,
    pub client_number: Option<String>,
    pub tax_year: Option<u16>,
    #[serde(default, rename="path")] pub _routing_path: Option<String>,
}
fn page_limit() -> usize { 25 }
fn sha(s: &str) -> bool { s.len()==64 && s.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn number(s: &str) -> bool { s.parse::<u32>().is_ok_and(|n|n<=i32::MAX as u32 && n.to_string()==s) }
fn hash(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }
fn k(company: &str, suffix: &str) -> String { format!("dtmaxtc:{company}:source_exception:{suffix}") }
fn decode(s: &str, digest: &str, max: usize) -> Result<Vec<u8>> {
    if s.is_empty() || s.len()>max*2 || !sha(digest) { return Err(bad("Invalid source exception byte bounds or digest")); }
    let bytes=hex::decode(s).map_err(|_|bad("Invalid source exception hex"))?;
    if hex::encode(&bytes)!=s || hash(&bytes)!=digest { return Err(bad("Source exception bytes do not match digest")); }
    Ok(bytes)
}
fn u32_at(bytes: &[u8], at: usize) -> u32 { u32::from_le_bytes(bytes[at..at+4].try_into().unwrap()) }
fn validate(v: &SourceExceptionInput) -> Result<Vec<String>> {
    if !v.test_copy || !sha(&v.source_archive_sha256) || !number(&v.family_file_id) || !number(&v.client_number) || !(1990..=2100).contains(&v.tax_year) || !matches!(v.reason.as_str(),"missing_identity_information"|"empty_information_payload") { return Err(bad("Invalid source exception references")); }
    let bytes=decode(&v.source_context_hex,&v.source_context_sha256,MAX_CONTEXT)?;
    let year=decode(&v.source_year_frame_hex,&v.source_year_frame_sha256,12)?;
    if year.len()!=12 || u32_at(&year,0)!=12 || &year[4..8]!=b"EXY\0" || u32_at(&year,8)!=u32::from(v.tax_year) || v.source_year_frame_offset<56 || v.source_year_frame_offset.checked_add(12).is_none_or(|end|end>v.source_context_offset) || v.source_context_offset.checked_add(bytes.len() as u64).is_none_or(|end|end>v.source_archive_bytes) { return Err(bad("Source exception year or archive location mismatch")); }
    let mut at=0;let mut tags=Vec::new();
    while at<bytes.len() {
        if bytes.len()-at<8 { return Err(bad("Truncated source exception frame")); }
        let len=u32_at(&bytes,at) as usize;
        if len<8 || len>bytes.len()-at { return Err(bad("Invalid source exception frame length")); }
        let tag=&bytes[at+4..at+8];
        let valid=match tags.len() { 0=>tag==b"EXC\0" && len==58, 1=>tag==b"EXP\0" && len==12, _=>(tag==b"EXR\0" || tag==b"EXA\0") && len>=36 };
        if !valid { return Err(bad("Unexpected source exception frame sequence")); }
        if tags.is_empty() && (u32_at(&bytes,at+8).to_string()!=v.family_file_id || u32_at(&bytes,at+12).to_string()!=v.client_number) { return Err(bad("Source exception header IDs do not match")); }
        tags.push(String::from_utf8(tag[..3].to_vec()).unwrap());at+=len;
    }
    if tags.len()<2 || tags.len()>256 { return Err(bad("Invalid source exception frame count")); }
    Ok(tags)
}
fn describe(v: &SourceExceptionInput, tags: Vec<String>) -> Value {
    json!({"source_archive_sha256":v.source_archive_sha256,"source_archive_bytes":v.source_archive_bytes,
        "source_context_offset":v.source_context_offset,"source_context_sha256":v.source_context_sha256,
        "source_context_bytes":v.source_context_hex.len()/2,"source_year_frame_offset":v.source_year_frame_offset,
        "source_year_frame_sha256":v.source_year_frame_sha256,"family_file_id":v.family_file_id,
        "client_number":v.client_number,"tax_year":v.tax_year,"reason":v.reason,"frame_tags":tags,
        "immutable":true,"identity_metadata_readable":false,"normalized_amounts_created":false,
        "header_ids_are_source_references_only":true})
}
impl SourceImport {
    pub async fn test_copy_preserve_source_exception(&self, company: &str, v: SourceExceptionInput) -> Result<Value> {
        self.test_copy_guard(company,v.test_copy)?;let tags=validate(&v)?;
        let identity=format!("{}:{}",v.source_archive_sha256,v.source_context_offset);
        let key=k(company,&format!("record:{identity}"));
        let raw=serde_json::to_string(&v).map_err(|_|storage("Source exception encoding failed"))?;
        let created=self.store.set_nx(&key,&raw).await.map_err(|e|storage(&e))?;
        if self.store.get(&key).await.map_err(|e|storage(&e))?.as_deref()!=Some(&raw) { return Err(conflict("Original source exception is immutable")); }
        // A matching retry repairs an interrupted index write without replacing bytes.
        self.store.sadd(&k(company,"all"),&identity).await.map_err(|e|storage(&e))?;
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"record":describe(&v,tags),"source_rows_modified":0,"tax_cases_imported":0}))
    }
    async fn read_source_exception(&self, company: &str, archive: &str, offset: u64) -> Result<(SourceExceptionInput,Vec<String>)> {
        if !sha(archive) || offset<68 { return Err(bad("Invalid source exception identity")); }
        let raw=self.store.get(&k(company,&format!("record:{archive}:{offset}"))).await.map_err(|e|storage(&e))?.ok_or_else(||missing("Source exception not found"))?;
        let v:SourceExceptionInput=serde_json::from_str(&raw).map_err(|_|storage("Invalid stored source exception"))?;
        if v.source_archive_sha256!=archive || v.source_context_offset!=offset { return Err(storage("Source exception identity mismatch")); }
        let tags=validate(&v).map_err(|_|storage("Stored source exception failed verification"))?;Ok((v,tags))
    }
    pub async fn test_copy_source_exceptions(&self, company: &str, page: ExceptionPage) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        if page.limit==0 || page.limit>100 || page.client_number.as_ref().is_some_and(|c|!number(c)) || page.tax_year.is_some_and(|y|!(1990..=2100).contains(&y)) { return Err(bad("Invalid source exception filter")); }
        let ids=self.store.smembers(&k(company,"all")).await.map_err(|e|storage(&e))?;
        if ids.len()>MAX_INDEX { return Err(storage("Source exception index exceeds read bound")); }
        let mut rows=Vec::new();
        for id in ids {
            let (archive,offset)=id.split_once(':').ok_or_else(||storage("Invalid source exception index"))?;
            let offset=offset.parse().map_err(|_|storage("Invalid source exception offset"))?;
            let (v,tags)=self.read_source_exception(company,archive,offset).await?;
            if page.client_number.as_ref().is_none_or(|c|*c==v.client_number) && page.tax_year.is_none_or(|y|y==v.tax_year) { rows.push((v.source_archive_sha256.clone(),v.source_context_offset,describe(&v,tags))); }
        }
        rows.sort_by(|a,b|(&a.0,a.1).cmp(&(&b.0,b.1)));
        let total=rows.len();let records:Vec<_>=rows.into_iter().skip(page.offset).take(page.limit).map(|(_,_,metadata)|metadata).collect();
        Ok(json!({"test_copy":true,"source_scope":company,"offset":page.offset,"limit":page.limit,"total":total,"records":records,"source_rows_modified":0,"tax_cases_imported":0}))
    }
    pub async fn test_copy_source_exception_content(&self, company: &str, archive: &str, offset: u64) -> Result<Value> {
        self.test_copy_guard(company,true)?;let (v,tags)=self.read_source_exception(company,archive,offset).await?;
        Ok(json!({"test_copy":true,"source_scope":company,"record":describe(&v,tags),"source_context_hex":v.source_context_hex,"source_year_frame_hex":v.source_year_frame_hex,"source_rows_modified":0,"tax_cases_imported":0}))
    }
}
