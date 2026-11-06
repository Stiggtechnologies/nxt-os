//! Source facts only: no tax calculation, filing, production cutover, or identity inference.
//! Family file IDs are independent of individual client numbers. Absent amounts remain null.
use super::{ImportError, SourceImport, bad, storage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

include!("afr_tag_profile.rs");
type Result<T> = std::result::Result<T, ImportError>;
const LINES: &[&str] = &["15000", "23600", "26000", "42000", "48400", "48500"];
const SLIPS: &[&str] = &["T4Slip", "T2202Slip", "T4FHSASlip", "T4ASlip", "T5008Slip", "RRSPContributionReceiptSlip", "T3Slip", "T4ESlip", "T5007Slip", "RC210Slip", "T5Slip", "T4RSPSlip", "T4AOASSlip", "T4APSlip", "T4RIFSlip", "RC62Slip", "T5013Slip", "T1204Slip", "ManitobaRentAssistSlip"];
fn k(c: &str, suffix: &str) -> String { format!("dtmaxtc:{c}:{suffix}") }
fn sha(s: &str) -> bool { s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn number(s: &str) -> bool { !s.is_empty() && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit()) }
fn year(y: u16) -> bool { (1990..=2100).contains(&y) }
fn reject(r: &mut BTreeMap<String, u64>, reason: &str) { *r.entry(reason.into()).or_default() += 1; }
fn canonical_cents(s: &str) -> bool { s.parse::<i64>().is_ok_and(|n| n.to_string() == s) }
fn local_tag(s: &str) -> &str { s.rsplit('}').next().unwrap_or(s) }
fn encode<T: Serialize>(v: &T) -> Result<String> { serde_json::to_string(v).map_err(|_| storage("source record encoding failed")) }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFactBatch { pub test_copy: bool, pub records: Vec<Value> }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFactPage {
    pub kind: String, #[serde(default)] pub offset: usize, #[serde(default = "page_limit")] pub limit: usize,
    // Vercel's API rewrite supplies this infrastructure query parameter. The
    // authenticated Axum Path remains authoritative; never use it for scope.
    #[serde(default,rename="path")] pub _routing_path: Option<String>,
}
fn page_limit() -> usize { 25 }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceClientPage {
    #[serde(default)] pub offset: usize,
    #[serde(default = "page_limit")] pub limit: usize,
    #[serde(default)] pub search: String,
    pub year: Option<u16>,
    pub sort: Option<String>,
    pub direction: Option<String>,
    pub status: Option<String>,
    pub province: Option<String>,
    #[serde(default,rename="path")] pub _routing_path: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct FamilyFile {
    family_file_id: String, tax_year: u16, source_file_sha256: String,
    archive_sha256: String, member_client_numbers: Vec<String>,
}
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct HistoricalLines {
    client_number: String, family_file_id: String, tax_year: u16,
    archive_sha256: String, vendor_module: String, mapping_state: String,
    /// Every requested line is present as a key. Null means absent, never inferred zero.
    values_cents: BTreeMap<String, Option<String>>,
}
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct FieldTree { tag: String, attributes: BTreeMap<String, String>, text: Option<String>, children: Vec<FieldTree> }
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct AfrSlip {
    client_number: String, tax_year: Option<u16>, source_folder_year: u16,
    slip_kind: String, archive_sha256: String, source_frame_sha256: String,
    source_frame_offset: u64, source_occurrence: u32, slip_content_sha256: String,
    vendor_field_tree: FieldTree,
}

fn check_family(v: &FamilyFile) -> std::result::Result<(), &'static str> {
    if !number(&v.family_file_id) || !year(v.tax_year) { return Err("invalid_family_or_year") }
    if !sha(&v.source_file_sha256) || !sha(&v.archive_sha256) { return Err("invalid_source_hash") }
    let set: BTreeSet<_> = v.member_client_numbers.iter().collect();
    if set.len() != v.member_client_numbers.len() || set.len() > 128 || !set.iter().all(|n| number(n)) { return Err("invalid_family_members") }
    // Canonical ordering makes a source replay stable without changing membership.
    if !v.member_client_numbers.windows(2).all(|w| w[0] < w[1]) { return Err("family_members_not_sorted") }
    Ok(())
}
fn check_lines(v: &HistoricalLines) -> std::result::Result<(), &'static str> {
    if !number(&v.client_number) || !number(&v.family_file_id) || !year(v.tax_year) { return Err("invalid_client_family_or_year") }
    if !sha(&v.archive_sha256) { return Err("invalid_source_hash") }
    if !matches!(v.vendor_module.as_str(), "T1" | "T3") { return Err("unsupported_vendor_module") }
    let expected = if v.vendor_module != "T1" { "pending_module_verification" }
        else if (2015..=2025).contains(&v.tax_year) { "vendor_static_verified" } else { "pending_year_verification" };
    if v.mapping_state != expected { return Err("invalid_mapping_state") }
    if v.values_cents.len() != LINES.len() || !LINES.iter().all(|c| v.values_cents.contains_key(*c)) { return Err("six_explicit_line_states_required") }
    if v.values_cents.values().flatten().any(|s| !canonical_cents(s)) { return Err("invalid_integer_cents") }
    if ["48400", "48500"].iter().any(|c| v.values_cents[*c].as_deref().is_some_and(|s| s.starts_with('-'))) { return Err("negative_refund_or_balance") }
    Ok(())
}
fn check_tree(t: &FieldTree, depth: usize, nodes: &mut usize) -> std::result::Result<(), &'static str> {
    *nodes += 1;
    if depth > 12 || *nodes > 1024 || t.text.as_ref().is_some_and(|s| s.len() > 8192) { return Err("slip_tree_limit") }
    // Reviewed CRA tag profile excludes recipient SIN, which lives outside the Slip tree.
    if AFR_TAGS.binary_search(&t.tag.as_str()).is_err() { return Err("unreviewed_slip_tag") }
    if t.attributes.iter().any(|(k,v)| k != "boxNumber" || v.len() > 16 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) { return Err("unreviewed_slip_attribute") }
    for child in &t.children { check_tree(child, depth + 1, nodes)?; }
    Ok(())
}
fn check_afr(v: &AfrSlip) -> std::result::Result<(), &'static str> {
    if !number(&v.client_number) || !year(v.source_folder_year) || v.tax_year.is_some_and(|y| !year(y)) { return Err("invalid_client_or_year") }
    if ![&v.archive_sha256, &v.source_frame_sha256, &v.slip_content_sha256].iter().all(|s| sha(s)) { return Err("invalid_source_hash") }
    if v.source_frame_offset < 56 || v.source_occurrence == 0 || v.source_occurrence > 1_000_000 { return Err("invalid_source_location") }
    if !SLIPS.contains(&v.slip_kind.as_str()) || local_tag(&v.vendor_field_tree.tag) != v.slip_kind { return Err("invalid_slip_kind") }
    check_tree(&v.vendor_field_tree, 0, &mut 0)?;
    let explicit: Vec<_> = v.vendor_field_tree.children.iter().filter(|t| local_tag(&t.tag) == "TaxYear").collect();
    match (explicit.as_slice(), v.tax_year) {
        ([], None) => {},
        ([t], Some(y)) if t.text.as_deref().is_some_and(|s| s.trim().parse::<u16>() == Ok(y)) => {},
        _ => return Err("tax_year_must_match_explicit_slip_field"),
    }
    let value = serde_json::to_value(&v.vendor_field_tree).map_err(|_| "invalid_slip_tree")?;
    let bytes = serde_json::to_vec(&value).map_err(|_| "invalid_slip_tree")?;
    if hex::encode(Sha256::digest(bytes)) != v.slip_content_sha256 { return Err("slip_content_hash_mismatch") }
    Ok(())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[tokio::test]
    async fn matching_retry_repairs_indexes_after_body_only_persistence() {
        let store=crate::store::Store::memory();let service=SourceImport::new(store.clone());let company="recovery-test";
        let family=json!({"family_file_id":"999","tax_year":2025,"source_file_sha256":"c".repeat(64),"archive_sha256":"a".repeat(64),"member_client_numbers":["100"]});
        let historical=json!({"client_number":"100","family_file_id":"999","tax_year":2013,"archive_sha256":"a".repeat(64),"vendor_module":"T1","mapping_state":"pending_year_verification","values_cents":{"15000":"123","23600":null,"26000":null,"42000":null,"48400":null,"48500":null}});
        let tree=json!({"tag":"{http://www.cra-arc.gc.ca/xmlns/tdd/2-10-1}T5013Slip","attributes":{},"text":null,"children":[]});
        let hash=hex::encode(Sha256::digest(serde_json::to_vec(&tree).unwrap()));
        let afr=json!({"client_number":"100","tax_year":null,"source_folder_year":2025,"slip_kind":"T5013Slip","archive_sha256":"a".repeat(64),"source_frame_sha256":"b".repeat(64),"source_frame_offset":56,"source_occurrence":1,"slip_content_sha256":hash,"vendor_field_tree":tree});
        store.set(&k(company,&format!("family_file:999:2025:{}","c".repeat(64))),&encode(&serde_json::from_value::<FamilyFile>(family.clone()).unwrap()).unwrap()).await.unwrap();
        store.set(&k(company,&format!("historical_lines:100:2013:{}","a".repeat(64))),&encode(&serde_json::from_value::<HistoricalLines>(historical.clone()).unwrap()).unwrap()).await.unwrap();
        store.set(&k(company,&format!("afr_occurrence:{}:1","a".repeat(64))),&encode(&serde_json::from_value::<AfrSlip>(afr.clone()).unwrap()).unwrap()).await.unwrap();
        for result in [service.test_copy_family_files(company,SourceFactBatch{test_copy:true,records:vec![family]}).await.unwrap(),service.test_copy_historical_lines(company,SourceFactBatch{test_copy:true,records:vec![historical]}).await.unwrap(),service.test_copy_afr_slips(company,SourceFactBatch{test_copy:true,records:vec![afr]}).await.unwrap()] {
            assert_eq!(result["created"],0);assert_eq!(result["unchanged_replays"],1);
        }
        let summary=service.test_copy_source_fact_summary(company).await.unwrap();
        assert_eq!(summary["counts"],json!({"family_files":1,"family_member_links":1,"historical_lines":1,"historical_lines_pending":1,"afr_occurrences":1,"afr_contents":1,"afr_missing_year":1}));
        for kind in ["family-files","historical-lines","afr-slips"] {
            let page=service.test_copy_client_source_facts(company,"100",SourceFactPage{kind:kind.into(),offset:0,limit:25,_routing_path:None}).await.unwrap();assert_eq!(page["total"],1);assert_eq!(page["records"].as_array().unwrap().len(),1);
        }
    }
}

impl SourceImport {
    /// Personal T1 source clients are a firm-wide import scope, not QBO companies
    /// or independently calculated NXT tax cases. Years come from individual
    /// historical facts rather than legacy physical-file stems.
    pub async fn test_copy_source_clients(&self, company: &str, page: SourceClientPage) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        let sort=page.sort.as_deref().unwrap_or("client_number");let direction=page.direction.as_deref().unwrap_or("asc");
        if page.limit==0 || page.limit>100 || page.search.chars().count()>100 || page.year.is_some_and(|y| !year(y)) || !["client_number","name","processing_status"].contains(&sort) || !["asc","desc"].contains(&direction) || page.status.as_ref().is_some_and(|s|s.len()>80) || page.province.as_ref().is_some_and(|s|s.len()>40) { return Err(bad("Invalid source client filter")) }
        let mut years: BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        let mut pending: BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        for (index,target) in [("historical_lines",&mut years),("historical_lines_pending",&mut pending)] {
            for identity in self.store.smembers(&k(company,index)).await.map_err(|e| storage(&e))? {
                let mut parts=identity.split(':');
                let client=parts.next().ok_or_else(|| storage("invalid historical index"))?;
                let tax_year=parts.next().and_then(|x| x.parse::<u16>().ok()).ok_or_else(|| storage("invalid historical index year"))?;
                target.entry(client.into()).or_default().insert(tax_year);
            }
        }
        let available: BTreeSet<u16>=years.values().flat_map(|v|v.iter().copied()).collect();
        let mut numbers=self.store.smembers(&k(company,"clients")).await.map_err(|e| storage(&e))?;
        numbers.sort_by(|a,b|(a.len(),a).cmp(&(b.len(),b)));
        let needle=page.search.trim().to_lowercase();let mut clients=Vec::new();let mut statuses=BTreeSet::new();let mut provinces=BTreeSet::new();
        for chunk in numbers.chunks(500) {
            let keys:Vec<_>=chunk.iter().map(|n|k(company,&format!("client:{n}"))).collect();
            for (number,raw) in chunk.iter().zip(self.store.mget(&keys).await.map_err(|e| storage(&e))?) {
                let raw=raw.ok_or_else(|| storage("client index points to missing record"))?;
                let client:Value=serde_json::from_str(&raw).map_err(|_|storage("stored client unreadable"))?;
                let name=client["name"].as_str().ok_or_else(||storage("stored client name missing"))?;
                let status=client["processing_status"].as_str().unwrap_or("");let province=client["province"].as_str().unwrap_or("");
                if !status.is_empty(){statuses.insert(status.to_string());}if !province.is_empty(){provinces.insert(province.to_string());}
                let historical=years.get(number).cloned().unwrap_or_default();
                if page.year.is_some_and(|y|!historical.contains(&y)) || !format!("{number} {name}").to_lowercase().contains(&needle) || page.status.as_ref().is_some_and(|s|s!=status) || page.province.as_ref().is_some_and(|s|s!=province) { continue }
                let mut profile=serde_json::Map::new();
                for field in ["processing_status","email","home_phone","work_phone","cell_phone","street","city","postal_code","province","country"] {
                    if let Some(value)=client.get(field).and_then(Value::as_str) { profile.insert(field.into(),json!(value)); }
                }
                clients.push(json!({"client_number":number,"name":name,"historical_years":historical,"pending_years":pending.get(number).cloned().unwrap_or_default(),"source_profile":profile}));
            }
        }
        clients.sort_by(|a,b|{
            let id=|v:&Value|v["client_number"].as_str().unwrap_or("").to_string();let ai=id(a);let bi=id(b);let tie=(ai.len(),&ai).cmp(&(bi.len(),&bi));
            let order=match sort {"name"=>a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase()).then(tie),"processing_status"=>a["source_profile"]["processing_status"].as_str().unwrap_or("").cmp(b["source_profile"]["processing_status"].as_str().unwrap_or("")).then(tie),_=>tie};
            if direction=="desc"{order.reverse()}else{order}
        });
        let total=clients.len();let clients:Vec<_>=clients.into_iter().skip(page.offset).take(page.limit).collect();
        Ok(json!({"source_scope":company,"program":"T1","imported_source_only":true,"total":total,"offset":page.offset,"limit":page.limit,"available_years":available,"available_statuses":statuses,"available_provinces":provinces,"clients":clients,"tax_cases_created":0}))
    }
    // Claim immutable facts atomically. A matching retry must still repair every
    // derived index: an earlier request may have stopped after storing the body.
    async fn claim_source_fact(&self, key: &str, body: &str) -> Result<Option<bool>> {
        if self.store.set_nx(key, body).await.map_err(|e| storage(&e))? { return Ok(Some(true)) }
        match self.store.get(key).await.map_err(|e| storage(&e))? {
            Some(old) if old == body => Ok(Some(false)),
            Some(_) => Ok(None),
            None => Err(storage("claimed source fact is missing")),
        }
    }
    pub async fn test_copy_family_files(&self, company: &str, batch: SourceFactBatch) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.records.len() > 1000 { return Err(bad("At most 1,000 source facts per batch")) }
        let clients: BTreeSet<_> = self.store.smembers(&k(company,"clients")).await.map_err(|e| storage(&e))?.into_iter().collect();
        let (mut created, mut unchanged, mut member_links, mut unknown_members, mut without_context) = (0u64,0u64,0u64,0u64,0u64);
        let mut rejected = BTreeMap::new();
        for row in &batch.records {
            let v: FamilyFile = match serde_json::from_value(row.clone()) { Ok(v) => v, Err(_) => { reject(&mut rejected,"invalid_fields"); continue } };
            if let Err(e) = check_family(&v) { reject(&mut rejected,e); continue }
            let identity = format!("{}:{}:{}",v.family_file_id,v.tax_year,v.source_file_sha256);
            let key = k(company,&format!("family_file:{identity}")); let body = encode(&v)?;
            let Some(is_new) = self.claim_source_fact(&key,&body).await? else { reject(&mut rejected,"conflicting_family_context"); continue };
            self.store.sadd(&k(company,"family_files"),&identity).await.map_err(|e| storage(&e))?;
            if is_new && v.member_client_numbers.is_empty() { without_context += 1; }
            for n in &v.member_client_numbers {
                if is_new { member_links += 1; if !clients.contains(n) { unknown_members += 1; } }
                self.store.sadd(&k(company,"family_member_links"),&format!("{identity}:{n}")).await.map_err(|e| storage(&e))?;
                self.store.sadd(&k(company,&format!("member_family_files:{n}")),&identity).await.map_err(|e| storage(&e))?;
            }
            if is_new { created += 1; } else { unchanged += 1; }
        }
        Ok(json!({"test_copy":true,"submitted":batch.records.len(),"created":created,"unchanged_replays":unchanged,"member_links_created":member_links,"unregistered_member_links":unknown_members,"files_without_member_context":without_context,"rejected":rejected,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    pub async fn test_copy_historical_lines(&self, company: &str, batch: SourceFactBatch) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.records.len() > 1000 { return Err(bad("At most 1,000 source facts per batch")) }
        let (mut created, mut unchanged, mut pending, mut with_values) = (0u64,0u64,0u64,0u64); let mut rejected = BTreeMap::new();
        for row in &batch.records {
            let v: HistoricalLines = match serde_json::from_value(row.clone()) { Ok(v) => v, Err(_) => { reject(&mut rejected,"invalid_fields"); continue } };
            if let Err(e) = check_lines(&v) { reject(&mut rejected,e); continue }
            let identity = format!("{}:{}:{}",v.client_number,v.tax_year,v.archive_sha256);
            let key = k(company,&format!("historical_lines:{identity}")); let body = encode(&v)?;
            let Some(is_new) = self.claim_source_fact(&key,&body).await? else { reject(&mut rejected,"conflicting_historical_lines"); continue };
            self.store.sadd(&k(company,"historical_lines"),&identity).await.map_err(|e| storage(&e))?;
            if v.mapping_state != "vendor_static_verified" { if is_new { pending += 1; } self.store.sadd(&k(company,"historical_lines_pending"),&identity).await.map_err(|e| storage(&e))?; }
            if is_new && v.values_cents.values().any(Option::is_some) { with_values += 1; }
            self.store.sadd(&k(company,&format!("member_historical_lines:{}",v.client_number)),&identity).await.map_err(|e| storage(&e))?;
            if is_new { created += 1; } else { unchanged += 1; }
        }
        Ok(json!({"test_copy":true,"submitted":batch.records.len(),"created":created,"unchanged_replays":unchanged,"pending_mapping":pending,"records_with_values":with_values,"rejected":rejected,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    pub async fn test_copy_afr_slips(&self, company: &str, batch: SourceFactBatch) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.records.len() > 1000 { return Err(bad("At most 1,000 source facts per batch")) }
        let (mut created, mut unchanged, mut distinct, mut missing_year) = (0u64,0u64,0u64,0u64); let mut rejected = BTreeMap::new();
        for row in &batch.records {
            let v: AfrSlip = match serde_json::from_value(row.clone()) { Ok(v) => v, Err(_) => { reject(&mut rejected,"invalid_fields"); continue } };
            if let Err(e) = check_afr(&v) { reject(&mut rejected,e); continue }
            let identity = format!("{}:{}",v.archive_sha256,v.source_occurrence);
            let key = k(company,&format!("afr_occurrence:{identity}")); let body = encode(&v)?;
            let Some(is_new) = self.claim_source_fact(&key,&body).await? else { reject(&mut rejected,"conflicting_source_occurrence"); continue };
            let content = format!("{}:{}",v.client_number,v.slip_content_sha256);
            if self.store.set_nx(&k(company,&format!("afr_content:{content}")),&encode(&v.vendor_field_tree)?).await.map_err(|e| storage(&e))? { distinct += 1; }
            self.store.sadd(&k(company,"afr_occurrences"),&identity).await.map_err(|e| storage(&e))?;
            self.store.sadd(&k(company,"afr_contents"),&content).await.map_err(|e| storage(&e))?;
            self.store.sadd(&k(company,&format!("member_afr_slips:{}",v.client_number)),&identity).await.map_err(|e| storage(&e))?;
            if v.tax_year.is_none() { if is_new { missing_year += 1; } self.store.sadd(&k(company,"afr_missing_year"),&identity).await.map_err(|e| storage(&e))?; }
            if is_new { created += 1; } else { unchanged += 1; }
        }
        Ok(json!({"test_copy":true,"submitted":batch.records.len(),"created":created,"unchanged_replays":unchanged,"new_distinct_client_contents":distinct,"missing_explicit_year":missing_year,"rejected":rejected,"economic_slips_deduplicated":false,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    pub async fn test_copy_source_fact_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        let mut counts=BTreeMap::new();
        for name in ["family_files","family_member_links","historical_lines","historical_lines_pending","afr_occurrences","afr_contents","afr_missing_year"] {
            counts.insert(name,self.store.smembers(&k(company,name)).await.map_err(|e| storage(&e))?.len());
        }
        Ok(json!({"test_copy":true,"counts":counts,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    /// Paginated source evidence for a stable individual client number. Pending mappings
    /// remain labelled in the returned record, and no return is recalculated or created.
    pub async fn test_copy_client_source_facts(&self, company: &str, client: &str, page: SourceFactPage) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        if !number(client) || page.limit == 0 || page.limit > 100 { return Err(bad("Invalid client or page limit")) }
        let (index, record) = match page.kind.as_str() {
            "family-files" => ("member_family_files", "family_file"),
            "historical-lines" => ("member_historical_lines", "historical_lines"),
            "afr-slips" => ("member_afr_slips", "afr_occurrence"),
            _ => return Err(bad("Unknown source fact kind")),
        };
        let mut members=self.store.smembers(&k(company,&format!("{index}:{client}"))).await.map_err(|e| storage(&e))?;
        members.sort(); let total=members.len();
        let keys: Vec<_>=members.into_iter().skip(page.offset).take(page.limit).map(|m| k(company,&format!("{record}:{m}"))).collect();
        let mut records=Vec::new();
        for raw in self.store.mget(&keys).await.map_err(|e| storage(&e))? {
            let raw=raw.ok_or_else(|| storage("source fact index points to missing record"))?;
            records.push(serde_json::from_str::<Value>(&raw).map_err(|_| storage("stored source fact unreadable"))?);
        }
        Ok(json!({"test_copy":true,"client_number":client,"kind":page.kind,"total":total,"offset":page.offset,"records":records,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
}
