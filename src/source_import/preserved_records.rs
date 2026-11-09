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

/// Exact, bounded readback of already imported source records. This avoids
/// enumerating a large global set once per verification page.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFactLookup {pub test_copy:bool,pub kind:String,pub record_ids:Vec<String>}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFactPage {
    pub kind: String, #[serde(default)] pub offset: usize, #[serde(default = "page_limit")] pub limit: usize,
    pub source_return_year: Option<u16>,
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
// The original import classification is immutable. This later review describes
// field bindings only, never approval/recalculation of the stored amounts.
fn year_mapping_review(v: &HistoricalLines) -> Option<Value> {
    if v.vendor_module!="T1" || v.mapping_state!="pending_year_verification" { return None }
    let artifact=match v.tax_year {
        2005=>"90e36c34c9ad98691c872b6d0bc5482459ec5db84f9f32f86b926f234234f745",
        2009=>"78451f0ea751567f4c34ce308bbca508c9f8a6dc0f13697249e5e008b7a4229f",
        2010=>"d51d03f137a7b9c7a37e91bd8aeb5655e887fda56dfb800254829bb125092082",
        2011=>"fc934ed6c99be48e65729cc1f4187e49eb15a66f7010a64ddafab52e7007e6b0",
        2012=>"8ce020c728d0cf6b9ea8804c368dd281f7732d2b03103d6f8f69c68efd9013ce",
        2013=>"9d7254fabd95f679025e3fc7e441304709f259fd7fbfc2425787a231fa3ed07e",
        2014=>"412666ff325b72b66a7a60a50701bcde019357d9a6db991ac2bccaea85256b54",
        2026=>"0f20e028ba2dbf059f6d585c16c56f4243c2cdbe332a7e1b1ef8de14985ccbb5",
        _=>return None,
    };
    Some(json!({"state":"original_form_bindings_checked","tax_year":v.tax_year,"source_form":"E11",
        "package_sha256":artifact,"profile":if v.tax_year==2005{"dtmax-T1-E11-2005/v1"}else if [2013,2014].contains(&v.tax_year){"dtmax-T1-E11-2013-2014/v1"}else{"dtmax-T1-E11-additional-years/v1"},
        "field_ids":{"15000":150,"23600":236,"26000":260,"42000":420,"48400":484,"48500":485},
        "amounts_recalculated":false,"tax_figures_approved":false,"source_year_may_be_preliminary":v.tax_year==2026}))
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
// Shared read-only validation for the additive precision view. The original
// historical-lines serialization, storage and response stay unchanged.
pub(super) fn historical_source_identity_and_review(row:&Value)->Result<(String,Option<Value>)> {
    let v:HistoricalLines=serde_json::from_value(row.clone()).map_err(|_|storage("Invalid stored historical record"))?;
    check_lines(&v).map_err(storage)?;
    Ok((format!("{}:{}:{}",v.client_number,v.tax_year,v.archive_sha256),year_mapping_review(&v)))
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
            let page=service.test_copy_client_source_facts(company,"100",SourceFactPage{kind:kind.into(),offset:0,limit:25,source_return_year:None,_routing_path:None}).await.unwrap();assert_eq!(page["total"],1);assert_eq!(page["records"].as_array().unwrap().len(),1);
        }
    }
}

impl SourceImport {
    /// Personal T1 source clients are a firm-wide import scope, not QBO companies
    /// or independently calculated NXT tax cases. Years come from individual
    /// native historical facts plus separately qualified PDF observation years,
    /// rather than legacy physical-file stems.
    pub async fn test_copy_source_clients(&self, company: &str, page: SourceClientPage) -> Result<Value> {
        self.source_clients_projection(company,page,false).await
    }
    pub async fn test_copy_effective_source_clients(&self, company: &str, page: SourceClientPage) -> Result<Value> {
        self.source_clients_projection(company,page,true).await
    }
    async fn source_clients_projection(&self, company: &str, page: SourceClientPage, effective:bool) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        let sort=page.sort.as_deref().unwrap_or("client_number");let direction=page.direction.as_deref().unwrap_or("asc");
        if page.limit==0 || page.limit>100 || page.search.chars().count()>100 || page.year.is_some_and(|y| !year(y)) || !["client_number","name","processing_status"].contains(&sort) || !["asc","desc"].contains(&direction) || page.status.as_ref().is_some_and(|s|s.len()>80) || page.province.as_ref().is_some_and(|s|s.len()>40) { return Err(bad("Invalid source client filter")) }
        let mut years: BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        let mut pending_at_import: BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        for (index,target) in [("historical_lines",&mut years),("historical_lines_pending",&mut pending_at_import)] {
            for identity in self.store.smembers(&k(company,index)).await.map_err(|e| storage(&e))? {
                let mut parts=identity.split(':');
                let client=parts.next().ok_or_else(|| storage("invalid historical index"))?;
                let tax_year=parts.next().and_then(|x| x.parse::<u16>().ok()).ok_or_else(|| storage("invalid historical index year"))?;
                target.entry(client.into()).or_default().insert(tax_year);
            }
        }
        let (_,unresolved)=self.review_pending_mapping_records(company).await?;
        let mut pending:BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        for v in unresolved {pending.entry(v.client_number).or_default().insert(v.tax_year);}
        let paper_years=self.preserved_pdf_figure_years(company).await?;
        let archive_years=self.identification_source_archive_years(company).await?;
        let (recovered_years,effective_numbers,effective_profiles)=if effective {
            let (years,numbers,profiles)=self.effective_recovered_client_inputs(company).await?;(years,Some(numbers),profiles)
        }else{(self.recovered_native_source_years(company).await?,None,BTreeMap::new())};
        let available: BTreeSet<u16>=years.values().chain(paper_years.values()).chain(archive_years.values()).chain(recovered_years.values()).flat_map(|v|v.iter().copied()).collect();
        let mut numbers=if let Some(numbers)=effective_numbers{numbers}else{self.store.smembers(&k(company,"clients")).await.map_err(|e| storage(&e))?};
        numbers.sort_by(|a,b|(a.len(),a).cmp(&(b.len(),b)));
        let needle=page.search.trim().to_lowercase();let mut clients=Vec::new();let mut statuses=BTreeSet::new();let mut provinces=BTreeSet::new();
        for chunk in numbers.chunks(500) {
            let uncached:Vec<_>=chunk.iter().filter(|n|!effective_profiles.contains_key(*n)).cloned().collect();
            let keys:Vec<_>=uncached.iter().map(|n|k(company,&format!("client:{n}"))).collect();
            let fetched=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
            if fetched.len()!=uncached.len(){return Err(storage("Client profile read cardinality differs"));}
            let fetched:BTreeMap<_,_>=uncached.into_iter().zip(fetched).collect();
            for number in chunk {
                let client:Value=if let Some(v)=effective_profiles.get(number){v.clone()}else{let raw=fetched.get(number).and_then(|v|v.as_ref()).ok_or_else(||storage("client index points to missing record"))?;serde_json::from_str(raw).map_err(|_|storage("stored client unreadable"))?};
                let name=client["name"].as_str().ok_or_else(||storage("stored client name missing"))?;
                let status=client["processing_status"].as_str().unwrap_or("");let province=client["province"].as_str().unwrap_or("");
                if !status.is_empty(){statuses.insert(status.to_string());}if !province.is_empty(){provinces.insert(province.to_string());}
                let native_historical=years.get(number).cloned().unwrap_or_default();
                let paper=paper_years.get(number).cloned().unwrap_or_default();
                let archives=archive_years.get(number).cloned().unwrap_or_default();
                let recovered=recovered_years.get(number).cloned().unwrap_or_default();
                let historical:BTreeSet<u16>=native_historical.iter().chain(paper.iter()).chain(archives.iter()).chain(recovered.iter()).copied().collect();
                if page.year.is_some_and(|y|!historical.contains(&y)) || !format!("{number} {name}").to_lowercase().contains(&needle) || page.status.as_ref().is_some_and(|s|s!=status) || page.province.as_ref().is_some_and(|s|s!=province) { continue }
                let mut profile=serde_json::Map::new();
                for field in ["processing_status","email","home_phone","work_phone","cell_phone","street","city","postal_code","province","country"] {
                    if let Some(value)=client.get(field).and_then(Value::as_str) { profile.insert(field.into(),json!(value)); }
                }
                let mut row=json!({"client_number":number,"name":name,"historical_years":historical,"pending_years":pending.get(number).cloned().unwrap_or_default(),"pending_years_at_import":pending_at_import.get(number).cloned().unwrap_or_default(),"source_profile":profile});
                if !recovered.is_empty() {
                    row["recovered_native_source_years"]=json!(recovered);
                    row["native_historical_years"]=json!(native_historical);
                    row["recovered_native_only_years"]=json!(recovered.iter().copied().filter(|y|!native_historical.contains(y) && !paper.contains(y) && !archives.contains(y)).collect::<BTreeSet<_>>());
                }
                if !paper.is_empty() || !archives.is_empty() {
                    row["native_historical_years"]=json!(native_historical);
                    row["preserved_pdf_figure_years"]=json!(paper);
                    if !archives.is_empty(){row["source_archive_years"]=json!(archives);}
                    row["pdf_only_years"]=json!(paper.union(&archives).copied().filter(|y|!native_historical.contains(y) && !recovered.contains(y)).collect::<BTreeSet<_>>());
                }
                clients.push(row);
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
        let names=["family_files","family_member_links","historical_lines","historical_lines_pending","afr_occurrences","afr_contents","afr_missing_year"];
        let keys:Vec<_>=names.iter().map(|name|k(company,name)).collect();
        let values=self.store.source_set_counts(&keys).await.map_err(|e|storage(&e))?;
        let counts:BTreeMap<_,_>=names.into_iter().zip(values).collect();
        let (reviewed,unresolved)=self.review_pending_mapping_records(company).await?;
        Ok(json!({"test_copy":true,"counts":counts,"historical_mapping_review":{"pending_at_import":reviewed+unresolved.len(),"original_form_bindings_checked":reviewed,"remaining_pending":unresolved.len(),"original_import_classifications_preserved":true,"amounts_changed":0,"tax_figures_approved":false},"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    async fn review_pending_mapping_records(&self, company: &str) -> Result<(usize,Vec<HistoricalLines>)> {
        let identities=self.store.smembers(&k(company,"historical_lines_pending")).await.map_err(|e|storage(&e))?;
        if identities.len()>100_000 {return Err(storage("Historical pending index exceeds review bound"))}
        let mut reviewed=0;let mut unresolved=Vec::new();
        for chunk in identities.chunks(500) {
            let keys:Vec<_>=chunk.iter().map(|id|k(company,&format!("historical_lines:{id}"))).collect();
            for (id,raw) in chunk.iter().zip(self.store.mget(&keys).await.map_err(|e|storage(&e))?) {
                let v:HistoricalLines=serde_json::from_str(&raw.ok_or_else(||storage("Pending historical body missing"))?).map_err(|_|storage("Invalid pending historical body"))?;
                check_lines(&v).map_err(storage)?;
                if *id!=format!("{}:{}:{}",v.client_number,v.tax_year,v.archive_sha256) || v.mapping_state=="vendor_static_verified" {return Err(storage("Pending historical index mismatch"))}
                if year_mapping_review(&v).is_some() {reviewed+=1;} else {unresolved.push(v);}
            }
        }
        Ok((reviewed,unresolved))
    }
    /// Paginated source evidence for a stable individual client number. Pending mappings
    /// remain labelled in the returned record, and no return is recalculated or created.
    pub async fn test_copy_client_source_facts(&self, company: &str, client: &str, page: SourceFactPage) -> Result<Value> {
        if page.kind=="wire-exact-historical-lines" {return self.test_copy_client_wire_exact_historical_lines(company,client,page).await}
        if page.kind=="recovered-native-sources" {return self.test_copy_client_recovered_native_sources(company,client,page).await}
        if page.kind=="archived-extract-sources" {return self.test_copy_client_archived_extract_sources(company,client,page).await}
        if page.kind=="recovered-native-effective-sources" {return self.test_copy_effective_recovered_page(company,Some(client),super::recovered_native_sources::RecoveredNativePage{offset:page.offset,limit:page.limit,source_return_year:page.source_return_year,_routing_path:None}).await}
        if page.kind=="parsed-native-forms" {return self.test_copy_client_parsed_native_forms(company,client,page).await}
        if page.kind=="source-paper-documents" {return self.test_copy_client_source_paper_documents(company,client,page).await}
        if page.kind=="source-summary-figures" {return self.test_copy_client_source_summary_figures(company,client,page).await}
        if page.kind=="preserved-pdf-figures" {return self.test_copy_client_preserved_pdf_figures(company,client,page).await}
        if page.kind=="printed-cached-figures" {return self.test_copy_client_printed_cached_figures(company,client,page).await}
        if page.kind=="linked-t5008" {return self.test_copy_client_legacy_slips(company,client,page).await}
        if page.kind=="native-slip-views" {return self.test_copy_client_native_slip_views(company,client,page).await}
        if page.kind=="historical-family-lineage" {return self.test_copy_client_legacy_family_lineage(company,client,page).await}
        if page.kind=="historical-variants" {return self.test_copy_client_native_historical_variants(company,client,page).await}
        if page.source_return_year.is_some() {return Err(bad("Source return year filter applies only to native slip views"))}
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
            let mut record=serde_json::from_str::<Value>(&raw).map_err(|_| storage("stored source fact unreadable"))?;
            if page.kind=="historical-lines" {
                let v:HistoricalLines=serde_json::from_value(record.clone()).map_err(|_|storage("Invalid stored historical record"))?;check_lines(&v).map_err(storage)?;
                if v.client_number!=client {return Err(storage("Historical client mismatch"))}
                if let Some(review)=year_mapping_review(&v) {record["year_mapping_review"]=review;}
            }
            records.push(record);
        }
        if page.kind=="historical-lines" {
            self.attach_personal_source_info(company,&mut records).await?;
            self.attach_native_module_reviews(company,&mut records).await?;
        }
        Ok(json!({"test_copy":true,"client_number":client,"kind":page.kind,"total":total,"offset":page.offset,"records":records,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    pub async fn test_copy_read_source_facts(&self,company:&str,lookup:SourceFactLookup)->Result<Value> {
        self.test_copy_guard(company,lookup.test_copy)?;
        if lookup.record_ids.is_empty() || lookup.record_ids.len()>250 || lookup.record_ids.iter().any(|id|id.is_empty() || id.len()>180 || !id.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b==b':')) {return Err(bad("Invalid source lookup identities or bound"))}
        let record=match lookup.kind.as_str(){"family-files"=>"family_file","historical-lines"=>"historical_lines","afr-slips"=>"afr_occurrence",_=>return Err(bad("Unsupported source lookup kind"))};
        let keys:Vec<_>=lookup.record_ids.iter().map(|id|k(company,&format!("{record}:{id}"))).collect();let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if rows.len()!=keys.len(){return Err(storage("Source lookup cardinality differs"))}
        let mut records=Vec::new();let mut response_bytes=0usize;
        for (id,raw) in lookup.record_ids.iter().zip(rows) {
            let raw=raw.ok_or_else(||super::missing("Requested preserved source record is absent"))?;
            response_bytes+=raw.len();if response_bytes>3*1024*1024{return Err(bad("Source lookup response exceeds bound; request fewer identities"))}
            let value:Value=serde_json::from_str(&raw).map_err(|_|storage("Stored source record unreadable"))?;
            let expected=match lookup.kind.as_str(){
                "family-files"=>{let v:FamilyFile=serde_json::from_value(value.clone()).map_err(|_|storage("Invalid family source"))?;check_family(&v).map_err(storage)?;format!("{}:{}:{}",v.family_file_id,v.tax_year,v.source_file_sha256)},
                "historical-lines"=>{let v:HistoricalLines=serde_json::from_value(value.clone()).map_err(|_|storage("Invalid historical source"))?;check_lines(&v).map_err(storage)?;format!("{}:{}:{}",v.client_number,v.tax_year,v.archive_sha256)},
                _=>{let v:AfrSlip=serde_json::from_value(value.clone()).map_err(|_|storage("Invalid CRA source"))?;check_afr(&v).map_err(storage)?;format!("{}:{}",v.archive_sha256,v.source_occurrence)},
            };
            if id!=&expected{return Err(storage("Source lookup record identity differs"))}
            records.push(value);
        }
        Ok(json!({"test_copy":true,"source_scope":company,"kind":lookup.kind,"record_ids":lookup.record_ids,"records":records,"source_records_modified":0,"tax_cases_imported":0}))
    }
}
