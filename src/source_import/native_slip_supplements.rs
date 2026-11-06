//! Immutable additional source fields, anchored to the original native view.
//! Source-year converter labels do not approve a CRA box, allocation or tax figure.
use super::{bad,conflict,storage,ImportError,SourceImport};
use super::native_slip_views::{ActiveValue,NativeField,NativeSlipView};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{collections::{BTreeMap,BTreeSet},sync::OnceLock};
type Result<T>=std::result::Result<T,ImportError>;
pub const PROFILE:&str="17b49048f15e6bac3b0feee2aad55b4b9879716eadb34805c4ad00e8ec06667d";
const CORE:&str="4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc";
const AUTHORITY:&str="e20addca37d6105dfa1fb45b12c136a436c40d40d7bd0b27a94101a9e298345d";
#[derive(Clone,Debug,Deserialize,Serialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeSupplement {
    pub supplemental_profile_sha256:String,
    pub source_view_file_sha256:String,pub source_view_number:u32,pub source_view_line_sha256:String,
    pub client_number:String,pub family_file_id:String,pub source_return_year:u16,
    pub vendor_module:String,pub vendor_slip_kind:String,pub vendor_slip_ordinal:u32,pub vendor_form:String,
    pub from_data_segment:bool,pub original_field_count:u32,pub financial_fields:Vec<NativeField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplementBatch {pub test_copy:bool,pub records:Vec<NativeSupplement>}
fn profile()-> &'static Value {static P:OnceLock<Value>=OnceLock::new();P.get_or_init(||serde_json::from_str(include_str!("native_slip_supplement_profiles.json")).expect("reviewed extra-field profile"))}
fn labels()-> &'static Value {static P:OnceLock<Value>=OnceLock::new();P.get_or_init(||serde_json::from_str(include_str!("native_slip_historical_labels.json")).expect("reviewed year labels"))}
fn id(s:&NativeSupplement)->String {format!("{}:{:06}",s.source_view_file_sha256,s.source_view_number)}
fn key(company:&str,id:&str)->String {format!("dtmaxtc:{company}:native_supplement:{PROFILE}:record:{id}")}
fn index(company:&str,suffix:&str)->String {format!("dtmaxtc:{company}:native_supplement:{PROFILE}:{suffix}")}
fn core_key(company:&str,s:&NativeSupplement)->String {format!("dtmaxtc:{company}:native_slip:record:{}:{:06}:{CORE}",s.source_view_file_sha256,s.source_view_number)}
fn validate(s:&NativeSupplement,v:&NativeSlipView)->Result<()> {
    if s.supplemental_profile_sha256!=PROFILE || v.financial_profile_sha256!=CORE || s.vendor_module!="T1" || s.source_view_file_sha256!=v.source_view_file_sha256 || s.source_view_number!=v.source_view_number || s.source_view_line_sha256!=v.source_view_line_sha256 || s.client_number!=v.client_number || s.family_file_id!=v.family_file_id || s.source_return_year!=v.source_return_year || s.vendor_module!=v.vendor_module || s.vendor_slip_kind!=v.vendor_slip_kind || s.vendor_slip_ordinal!=v.vendor_slip_ordinal || s.vendor_form!=v.vendor_form || s.from_data_segment!=v.from_data_segment || s.original_field_count!=v.original_field_count || s.financial_fields.is_empty() || s.financial_fields.len()>512 {return Err(bad("Additional financial source context or profile differs"))}
    let allowed=profile()["profiles"][s.source_return_year.to_string()][&s.vendor_slip_kind].as_array().ok_or_else(||bad("Additional fields have no reviewed source-year profile"))?;
    let mut previous=None;
    for f in &s.financial_fields {
        if !allowed.iter().any(|n|n.as_u64()==Some(f.field_id as u64)) || f.source_field_index>=s.original_field_count || previous.is_some_and(|i|i>=f.source_field_index) || v.financial_fields.iter().any(|core|core.source_field_index==f.source_field_index || core.field_id==f.field_id) || f.source_type_tag!=4 || !matches!(&f.value,ActiveValue::Text{text:Some(t)} if !t.trim().is_empty() && t.len()<=4096) {return Err(bad("Additional source field is unreviewed, overlapping, unordered or inactive"))}
        previous=Some(f.source_field_index);
    }
    Ok(())
}
pub(super) fn historical_review(v:&NativeSlipView)->Value {
    let mut fields=BTreeMap::new();
    if v.vendor_module=="T1" {if let Some(bindings)=labels()[v.source_return_year.to_string()][&v.vendor_slip_kind].as_object() {for f in &v.financial_fields {if let Some(review)=bindings.get(&f.field_id.to_string()){fields.insert(f.field_id.to_string(),review.clone());}}}}
    json!({"authority_matrix_sha256":AUTHORITY,"source_return_year":v.source_return_year,"field_bindings":fields,"box_assignment_approved":false,"normalization_approved":false})
}
impl SourceImport {
    pub async fn test_copy_native_slip_supplements(&self,company:&str,batch:SupplementBatch)->Result<Value> {
        self.test_copy_guard(company,batch.test_copy)?;
        if batch.records.is_empty() || batch.records.len()>250 {return Err(bad("Additional source batch must contain 1-250 records"))}
        // Resolve and validate the entire batch before any writes.
        let keys:Vec<_>=batch.records.iter().map(|s|core_key(company,s)).collect();
        let originals=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if originals.len()!=batch.records.len(){return Err(storage("Original view cardinality differs"))}
        let mut seen=BTreeMap::new();
        for (s,raw) in batch.records.iter().zip(originals) {
            let v:NativeSlipView=serde_json::from_str(&raw.ok_or_else(||bad("Additional fields require the original preserved source view"))?).map_err(|_|storage("Original native source is corrupt"))?;
            validate(s,&v)?;
            if seen.insert(id(s),s).is_some_and(|old|old!=s){return Err(conflict("Conflicting additional views in one batch"))}
            let raw=serde_json::to_string(s).map_err(|_|storage("Additional source encoding failed"))?;
            if self.store.get(&key(company,&id(s))).await.map_err(|e|storage(&e))?.is_some_and(|old|old!=raw){return Err(conflict("Additional financial source is immutable"))}
        }
        let mut created=0;let mut unchanged=0;
        for s in &batch.records {
            let identity=id(s);let raw=serde_json::to_string(s).map_err(|_|storage("Additional source encoding failed"))?;let k=key(company,&identity);
            let added=self.store.set_nx(&k,&raw).await.map_err(|e|storage(&e))?;
            if self.store.get(&k).await.map_err(|e|storage(&e))?.as_deref()!=Some(&raw){return Err(conflict("Additional financial source is immutable"))}
            for suffix in ["all".to_owned(),format!("client:{}",s.client_number),format!("client_year:{}:{}",s.client_number,s.source_return_year)] {self.store.sadd(&index(company,&suffix),&identity).await.map_err(|e|storage(&e))?;}
            if added {created+=1}else{unchanged+=1}
        }
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,"original_native_views_modified":0,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
    pub(super) async fn attach_native_supplements(&self,company:&str,views:&[NativeSlipView],records:&mut [Value])->Result<()> {
        let keys:Vec<_>=views.iter().map(|v|key(company,&format!("{}:{:06}",v.source_view_file_sha256,v.source_view_number))).collect();
        let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if rows.len()!=views.len() || records.len()!=views.len(){return Err(storage("Additional source read cardinality differs"))}
        for ((v,record),raw) in views.iter().zip(records).zip(rows) {
            record["historical_field_review"]=historical_review(v);
            if let Some(raw)=raw {
                let s:NativeSupplement=serde_json::from_str(&raw).map_err(|_|storage("Additional source record is corrupt"))?;validate(&s,v).map_err(|_|storage("Stored additional source context differs"))?;
                let mut expanded=v.clone();expanded.financial_fields=s.financial_fields.clone();
                record["additional_financial_fields"]=json!(s.financial_fields);record["supplemental_profile_sha256"]=json!(PROFILE);
                record["additional_field_review"]=historical_review(&expanded);
            }
        }
        Ok(())
    }
    pub async fn test_copy_native_supplement_summary(&self,company:&str)->Result<Value> {
        self.test_copy_guard(company,true)?;
        let ids=self.store.smembers(&index(company,"all")).await.map_err(|e|storage(&e))?;
        if ids.len()>100_000 {return Err(storage("Additional source index exceeds read bound"))}
        let mut fields=0;let mut clients=BTreeSet::new();
        for chunk in ids.chunks(250) {
            let keys:Vec<_>=chunk.iter().map(|id|key(company,id)).collect();
            for raw in self.store.mget(&keys).await.map_err(|e|storage(&e))? {let s:NativeSupplement=serde_json::from_str(&raw.ok_or_else(||storage("Additional source index record missing"))?).map_err(|_|storage("Additional source record is corrupt"))?;fields+=s.financial_fields.len();clients.insert(s.client_number);}
        }
        Ok(json!({"test_copy":true,"source_scope":company,"supplemental_profile_sha256":PROFILE,"additional_source_views":ids.len(),"additional_financial_fields":fields,"affected_clients":clients.len(),"original_native_views_modified":0,"normalized_slips_created":0,"tax_cases_imported":0,"financial_values_approved":false}))
    }
}
