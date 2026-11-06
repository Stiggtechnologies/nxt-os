//! Immutable, authenticated original DT Max print output. Separate from the
//! synthetic opaque-file lane: no calculation, identity editing or filing.
use super::{bad, conflict, missing, storage, SourceImport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
type Result<T> = std::result::Result<T, super::ImportError>;
pub const MAX_PDF_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReturnPdfInput {
    pub test_copy: bool,
    pub client_number: String,
    pub tax_year: u16,
    pub sha256: String,
    pub page_labels: Vec<String>,
    pub content_hex: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_active_content_is_rejected() {
        let action=lopdf::dictionary!{"S"=>"JavaScript","JS"=>lopdf::Object::string_literal("example")};
        let annotation=lopdf::dictionary!{"Type"=>"Annot","A"=>action};
        let page=lopdf::Object::Dictionary(lopdf::dictionary!{"Annots"=>vec![lopdf::Object::Dictionary(annotation)]});
        assert!(!static_object(&page,0,&mut 0));
        assert!(static_object(&lopdf::Object::Dictionary(lopdf::dictionary!{"Type"=>"Page"}),0,&mut 0));
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Record {
    client_number: String, tax_year: u16, sha256: String,
    page_labels: Vec<String>, bytes: usize,
}
fn number(s: &str) -> bool { !s.is_empty() && s.len() <= 12 && s.bytes().all(|b|b.is_ascii_digit()) }
fn digest(s: &str) -> bool { s.len()==64 && s.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn k(company: &str, rest: &str) -> String { format!("dtmaxtc:{company}:return_pdf:{rest}") }
fn describe(r: &Record) -> Value { json!({"client_number":r.client_number,"tax_year":r.tax_year,"sha256":r.sha256,"page_labels":r.page_labels,"page_count":r.page_labels.len(),"bytes":r.bytes,"original_print_output":true,"immutable":true}) }
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
impl SourceImport {
    pub async fn test_copy_preserve_return_pdf(&self, company: &str, v: ReturnPdfInput) -> Result<Value> {
        self.test_copy_guard(company,v.test_copy)?;
        if !number(&v.client_number) || !(1990..=2100).contains(&v.tax_year) || !digest(&v.sha256) { return Err(bad("Invalid PDF client, year or digest")); }
        if v.content_hex.len()>MAX_PDF_BYTES*2 { return Err(bad("Return PDF exceeds 2 MiB bound")); }
        if v.page_labels.is_empty() || v.page_labels.len()>500 || v.page_labels.iter().any(|s|s.is_empty() || s.len()>120 || s.chars().any(char::is_control)) { return Err(bad("Invalid PDF page index")); }
        let client_key=format!("dtmaxtc:{company}:client:{}",v.client_number);
        if self.store.get(&client_key).await.map_err(|e|storage(&e))?.is_none() { return Err(missing("PDF client is not in this source scope")); }
        let bytes=hex::decode(&v.content_hex).map_err(|_|bad("Invalid PDF hex"))?;
        if hex::encode(&bytes)!=v.content_hex || hex::encode(Sha256::digest(&bytes))!=v.sha256 { return Err(bad("PDF bytes do not match declared digest")); }
        let doc=lopdf::Document::load_mem(&bytes).map_err(|_|bad("Return PDF could not be parsed"))?;
        if doc.is_encrypted() { return Err(bad("Encrypted PDF requires an authorized readable export; original remains preserved separately")); }
        if doc.get_pages().len()!=v.page_labels.len() { return Err(bad("Page index does not match parsed PDF page count")); }
        // Original print output is static; active content and attachments are not
        // accepted into the preview lane. The bytes are never rewritten.
        let mut nodes=0;
        if !doc.objects.values().all(|o|static_object(o,0,&mut nodes)) {return Err(bad("Active or overly nested PDF content is not supported by the source preview"))}
        let record=Record{client_number:v.client_number.clone(),tax_year:v.tax_year,sha256:v.sha256.clone(),page_labels:v.page_labels,bytes:bytes.len()};
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
            if r.client_number!=client { return Err(storage("PDF client mismatch")); } records.push(describe(&r));
        }
        Ok(json!({"test_copy":true,"source_scope":company,"client_number":client,"records":records,"tax_cases_imported":0}))
    }
    pub async fn test_copy_return_pdf_content(&self, company: &str, client: &str, year: u16, sha: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?; if !number(client) || !digest(sha) || !(1990..=2100).contains(&year) { return Err(bad("Invalid PDF identity")); }
        let raw=self.store.get(&k(company,&format!("record:{client}:{year}:{sha}"))).await.map_err(|e|storage(&e))?.ok_or_else(||missing("No original return PDF for this client and year"))?;
        let r:Record=serde_json::from_str(&raw).map_err(|_|storage("Invalid PDF record"))?;
        if r.client_number!=client || r.tax_year!=year || r.sha256!=sha { return Err(storage("PDF ownership mismatch")); }
        let content=self.store.get(&k(company,&format!("blob:{sha}"))).await.map_err(|e|storage(&e))?.ok_or_else(||storage("PDF bytes missing"))?;
        if content.len()>MAX_PDF_BYTES*2 { return Err(storage("PDF blob exceeds bound")); }
        let bytes=hex::decode(&content).map_err(|_|storage("Invalid stored PDF bytes"))?;
        if bytes.len()!=r.bytes || hex::encode(Sha256::digest(&bytes))!=sha { return Err(storage("Stored PDF failed hash verification")); }
        Ok(json!({"test_copy":true,"source_scope":company,"record":describe(&r),"content_hex":content,"tax_cases_imported":0}))
    }
}
