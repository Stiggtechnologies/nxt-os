//! Ordered native financial source fields, never deduplicated or posted as slips.
//! A field's active type controls its value; inactive numeric slots are not money.
use super::{bad, conflict, storage, ImportError, SourceImport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, ImportError>;
const MAX_FIELDS: usize = 512;
const MAX_VIEWS: usize = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActiveValue {
    Text { text: Option<String> },
    Float32 { bits_hex: String },
    Integer { value: i32 },
    Flag { value: u8 },
    Held { source_field_sha256: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeField {
    pub source_field_index: u32,
    pub field_id: u32,
    pub source_type_tag: i32,
    pub value: ActiveValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeSlipView {
    pub financial_profile_sha256: String,
    pub source_archive_sha256: String,
    pub source_view_file_sha256: String,
    pub source_view_line_sha256: String,
    pub source_view_number: u32,
    pub client_number: String,
    pub family_file_id: String,
    pub source_return_year: u16,
    pub vendor_module: String,
    pub vendor_slip_kind: String,
    pub vendor_slip_ordinal: u32,
    pub vendor_form: String,
    pub from_data_segment: bool,
    pub original_field_count: u32,
    pub financial_fields: Vec<NativeField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSlipBatch { pub test_copy: bool, pub records: Vec<NativeSlipView> }
#[derive(Serialize, Deserialize)]
struct FinancialProfile {
    field_ids: BTreeSet<u32>,
    field_labels: BTreeMap<u32, String>,
    selection_review: String,
}
fn profiles() -> &'static BTreeMap<String, FinancialProfile> {
    static PROFILES: OnceLock<BTreeMap<String, FinancialProfile>> = OnceLock::new();
    PROFILES.get_or_init(|| serde_json::from_str(include_str!("native_slip_profiles.json")).expect("reviewed native financial profiles"))
}
fn profile_sha() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(||hex::encode(Sha256::digest(serde_json::to_vec(profiles()).expect("serializable financial profiles"))))
}
fn sha(s: &str) -> bool { s.len()==64 && s.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn number(s: &str) -> bool { s.parse::<u32>().is_ok_and(|n|n<=i32::MAX as u32 && n.to_string()==s) }
fn k(company: &str, suffix: &str) -> String { format!("dtmaxtc:{company}:native_slip:{suffix}") }
fn identity(v: &NativeSlipView) -> String { format!("{}:{:06}:{}",v.source_view_file_sha256,v.source_view_number,v.financial_profile_sha256) }
fn context_key(company: &str, v: &NativeSlipView) -> String {format!("dtmaxtc:{company}:historical_lines:{}:{}:{}",v.client_number,v.source_return_year,v.source_archive_sha256)}
fn validate(v: &NativeSlipView) -> Result<()> {
    if v.financial_profile_sha256!=profile_sha() {return Err(bad("Native financial projection profile differs"))}
    if ![&v.source_archive_sha256,&v.source_view_file_sha256,&v.source_view_line_sha256].iter().all(|s|sha(s)) || !number(&v.client_number) || !number(&v.family_file_id) || v.client_number=="0" || !(1990..=2100).contains(&v.source_return_year) || v.source_view_number==0 || v.source_view_number>MAX_VIEWS as u32 || v.vendor_slip_ordinal>=10_000 || !matches!(v.vendor_module.as_str(),"T1"|"T3") || v.vendor_form.is_empty() || v.vendor_form.len()>64 || !v.vendor_form.bytes().all(|b|b.is_ascii_alphanumeric() || b"_ -".contains(&b)) || v.original_field_count>10_000 || v.financial_fields.len()>MAX_FIELDS || v.financial_fields.len()>v.original_field_count as usize {
        return Err(bad("Invalid native slip source references or bounds"));
    }
    let profile=profiles().get(&v.vendor_slip_kind).ok_or_else(||bad("Native slip financial profile is not reviewed"))?;
    let mut previous=None;
    for f in &v.financial_fields {
        if !profile.field_ids.contains(&f.field_id) || f.source_field_index>=v.original_field_count || previous.is_some_and(|i|i>=f.source_field_index) || !(0..=255).contains(&f.source_type_tag) {return Err(bad("Unreviewed or unordered native financial field"))}
        previous=Some(f.source_field_index);
        let valid=match &f.value {
            ActiveValue::Text{text}=>!matches!(f.source_type_tag,2|6|8|11) && text.as_ref().is_none_or(|s|s.len()<=4096),
            ActiveValue::Float32{bits_hex}=>f.source_type_tag==2 && bits_hex.len()==8 && bits_hex.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            ActiveValue::Integer{value}=>matches!(f.source_type_tag,8|11) && (f.source_type_tag!=8 || i16::try_from(*value).is_ok()),
            ActiveValue::Flag{value}=>f.source_type_tag==6 && *value<=1,
            ActiveValue::Held{source_field_sha256}=>sha(source_field_sha256),
        };
        if !valid {return Err(bad("Native active value does not match its source type"))}
    }
    Ok(())
}
fn describe(v: &NativeSlipView) -> Value {
    let mut value=serde_json::to_value(v).expect("serializable native source view");
    value["source_year_profile_status"]=json!(if v.source_return_year==2025 {"2025_reader_financial_field_profile"}else{"historical_year_binding_pending"});
    value["selection_review"]=json!(profiles()[&v.vendor_slip_kind].selection_review);
    let profile=&profiles()[&v.vendor_slip_kind];
    let labels:BTreeMap<_,_>=v.financial_fields.iter().map(|f|(f.field_id,&profile.field_labels[&f.field_id])).collect();
    value["field_labels"]=json!(labels);
    value["financial_profile_sha256"]=json!(profile_sha());
    value["economic_slip_uniqueness_verified"]=json!(false);
    value["normalized_tax_slip_created"]=json!(false);
    value["taxpayer_allocations_applied"]=json!(false);
    value["financial_values_approved"]=json!(false);
    value["source_year_may_be_preliminary"]=json!(v.source_return_year==2026);
    value
}
impl SourceImport {
    pub async fn test_copy_native_slip_views(&self, company: &str, batch: NativeSlipBatch) -> Result<Value> {
        self.test_copy_guard(company,batch.test_copy)?;
        if batch.records.is_empty() || batch.records.len()>250 {return Err(bad("Native slip batch must contain 1-250 source views"))}
        // Validate every row/context before writing any row from this request.
        for v in &batch.records {validate(v)?;}
        let context_keys:Vec<_>=batch.records.iter().map(|v|context_key(company,v)).collect::<BTreeSet<_>>().into_iter().collect();
        let context_values=self.store.mget(&context_keys).await.map_err(|e|storage(&e))?;
        let contexts:BTreeMap<_,_>=context_keys.into_iter().zip(context_values).collect();
        for v in &batch.records {
            let raw=contexts.get(&context_key(company,v)).and_then(Option::as_ref).ok_or_else(||bad("Native slip has no matching preserved client-year context"))?;
            let context:Value=serde_json::from_str(&raw).map_err(|_|storage("Preserved historical context is corrupt"))?;
            if context["client_number"]!=v.client_number || context["family_file_id"]!=v.family_file_id || context["tax_year"]!=v.source_return_year || context["archive_sha256"]!=v.source_archive_sha256 || context["vendor_module"]!=v.vendor_module {return Err(bad("Native slip ownership or source context differs"))}
        }
        let mut created=0;let mut unchanged=0;
        for v in &batch.records {
            let id=identity(v);let key=k(company,&format!("record:{id}"));let raw=serde_json::to_string(v).map_err(|_|storage("Native source encoding failed"))?;
            let added=self.store.set_nx(&key,&raw).await.map_err(|e|storage(&e))?;
            if self.store.get(&key).await.map_err(|e|storage(&e))?.as_deref()!=Some(&raw) {return Err(conflict("Original native source view is immutable"))}
            self.store.sadd(&k(company,&format!("all:{}",v.financial_profile_sha256)),&id).await.map_err(|e|storage(&e))?;
            self.store.sadd(&k(company,&format!("client:{}:{}",v.client_number,v.financial_profile_sha256)),&id).await.map_err(|e|storage(&e))?;
            self.store.sadd(&k(company,&format!("client_year:{}:{}:{}",v.client_number,v.source_return_year,v.financial_profile_sha256)),&id).await.map_err(|e|storage(&e))?;
            if added {created+=1;}else{unchanged+=1;}
        }
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
    pub async fn test_copy_client_native_slip_views(&self, company: &str, client: &str, page: super::preserved_records::SourceFactPage) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        if !number(client) || page.limit==0 || page.limit>100 || page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)) {return Err(bad("Invalid native source page"))}
        let index=match page.source_return_year {Some(year)=>format!("client_year:{client}:{year}:{}",profile_sha()),None=>format!("client:{client}:{}",profile_sha())};
        let mut ids=self.store.smembers(&k(company,&index)).await.map_err(|e|storage(&e))?;
        if ids.len()>MAX_VIEWS {return Err(storage("Native source index exceeds read bound"))}
        ids.sort();let total=ids.len();let mut records=Vec::new();
        let selected:Vec<_>=ids.into_iter().skip(page.offset).take(page.limit).collect();
        let keys:Vec<_>=selected.iter().map(|id|k(company,&format!("record:{id}"))).collect();
        let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if rows.len()!=selected.len() {return Err(storage("Native source page cardinality mismatch"))}
        let mut views=Vec::new();
        for (id,raw) in selected.into_iter().zip(rows) {
            let raw=raw.ok_or_else(||storage("Native source index points to missing record"))?;
            let v:NativeSlipView=serde_json::from_str(&raw).map_err(|_|storage("Stored native source view is corrupt"))?;validate(&v).map_err(|_|storage("Stored native source view fails validation"))?;
            if v.client_number!=client || identity(&v)!=id || page.source_return_year.is_some_and(|y|y!=v.source_return_year) {return Err(storage("Native source client, year or identity mismatch"))}
            records.push(describe(&v));
            views.push(v);
        }
        self.attach_native_supplements(company,&views,&mut records).await?;
        self.attach_native_merged_slip_fields(company,&views,&mut records).await?;
        Ok(json!({"test_copy":true,"source_scope":company,"client_number":client,"source_return_year":page.source_return_year,"kind":"native-slip-views","offset":page.offset,"total":total,"records":records,"normalized_slips_created":0,"tax_cases_imported":0,"live_cutover_allowed":false}))
    }
    pub async fn test_copy_native_slip_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company,true)?;
        let ids=self.store.smembers(&k(company,&format!("all:{}",profile_sha()))).await.map_err(|e|storage(&e))?;
        if ids.len()>MAX_VIEWS {return Err(storage("Native source index exceeds read bound"))}
        Ok(json!({"test_copy":true,"source_scope":company,"financial_profile_sha256":profile_sha(),"source_view_projections":ids.len(),"source_views":ids.len(),"economic_slip_uniqueness_verified":false,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
}
