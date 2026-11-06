//! Historical Info1 facts, separate from current client profiles. SIN stays masked.
use super::{bad,conflict,missing,storage,ImportError,SourceImport};
use chrono::NaiveDate;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{collections::BTreeMap,sync::OnceLock};
type Result<T>=std::result::Result<T,ImportError>;
pub const PROFILE:&str="9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e";
const FIELDS:[&str;7]=["Relation","SIN","BirthDate","Status","Sex","Prov-Residence","Language"];
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PersonalField {
 pub raw_code:Option<String>,pub candidate:Value,pub state:String,pub display_eligible:bool,
 #[serde(default,skip_serializing_if="Option::is_none")]pub raw_integer_code:Option<i32>,
 #[serde(default,skip_serializing_if="Option::is_none")]pub residency:Option<String>,
 #[serde(default,skip_serializing_if="Option::is_none")]pub raw_redacted:Option<bool>,
 #[serde(default,skip_serializing_if="Option::is_none")]pub raw_bytes_sha256:Option<String>,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Info1 {
 pub frame_offset:u64,pub frame_sha256:String,pub expanded_sha256:String,pub expanded_bytes:u32,pub crc_verified:bool,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PersonalSourceInfo {
 pub schema:String,pub source_candidate_line:u32,pub archive_sha256:String,pub client_number:String,pub family_file_id:String,
 #[serde(rename="source_EXF_file_id")]pub source_exf_file_id:Option<i32>,
 pub source_year:u16,pub source_year_frame_offset:u64,pub source_context_offset:u64,
 #[serde(rename="source_EXC_sha256")]pub source_exc_sha256:String,
 pub reader_module:String,pub info_record_state:String,pub display_state:String,pub t1_display_eligible:bool,pub info1:Info1,pub fields:BTreeMap<String,PersonalField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalBatch {pub test_copy:bool,pub binding_profile_sha256:String,pub records:Vec<PersonalSourceInfo>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalLookup {pub test_copy:bool,pub record_ids:Vec<String>}
fn profile()-> &'static Value {static P:OnceLock<Value>=OnceLock::new();P.get_or_init(||serde_json::from_str(include_str!("personal_source_profile.json")).expect("reviewed Info1 source bindings"))}
fn sha(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
fn number(s:&str)->bool{s.parse::<u32>().is_ok_and(|n|n>0&&n<=i32::MAX as u32&&n.to_string()==s)}
fn id(r:&PersonalSourceInfo)->String{format!("{}:{}:{}",r.client_number,r.source_year,r.archive_sha256)}
fn key(company:&str,id:&str)->String{format!("dtmaxtc:{company}:personal_source:{PROFILE}:record:{id}")}
fn index(company:&str)->String{format!("dtmaxtc:{company}:personal_source:{PROFILE}:all")}
fn context_key(company:&str,id:&str)->String{format!("dtmaxtc:{company}:historical_lines:{id}")}
fn parsed_field(name:&str,f:&PersonalField,context_eligible:bool)->Result<PersonalField>{
 let mut expected=PersonalField{raw_code:f.raw_code.clone(),candidate:Value::Null,state:"present_blank".into(),display_eligible:false,raw_integer_code:None,residency:None,raw_redacted:None,raw_bytes_sha256:None};
 if name=="SIN" {
    if f.raw_code.is_some()||f.raw_redacted!=Some(true)||!f.raw_bytes_sha256.as_deref().is_some_and(sha){return Err(bad("SIN source must remain redacted and hash-bound"))}
    expected.raw_redacted=Some(true);expected.raw_bytes_sha256=f.raw_bytes_sha256.clone();expected.state=f.state.clone();
    if f.state=="present_nine_digit_candidate_not_identity_verified" {
        let s=f.candidate.as_str().ok_or_else(||bad("Masked SIN candidate missing"))?;
        if s.len()!=12||!s.starts_with("***-***-")||!s[8..].bytes().all(|b|b.is_ascii_digit()){return Err(bad("Unmasked or malformed SIN refused"))}
        expected.candidate=f.candidate.clone();expected.display_eligible=context_eligible;
    }else if !matches!(f.state.as_str(),"present_blank"|"source_placeholder_zero_sentinel"|"present_invalid_or_unsupported_format"){return Err(bad("Unsupported SIN source state"))}
    return Ok(expected)
 }
 let raw=f.raw_code.as_deref().ok_or_else(||bad("Info1 field raw code missing"))?;
 let width=match name{"Relation"|"Prov-Residence"=>2,"BirthDate"=>10,_=>1};
 if raw.chars().count()>width{return Err(bad("Info1 raw code exceeds fixed source width"))}
 if raw.trim().is_empty(){return Ok(expected)}
 let maps=&profile()["code_maps"];
 if name=="BirthDate" {
    let value=raw.trim();let exact=value.len()==10&&value.as_bytes().get(2)==Some(&b'-')&&value.as_bytes().get(5)==Some(&b'-')&&value.bytes().enumerate().all(|(i,b)|i==2||i==5||b.is_ascii_digit());
    if !exact {expected.state="present_invalid_or_unsupported_format".into()}
    else if let Ok(date)=NaiveDate::parse_from_str(value,"%d-%m-%Y") {if date.format("%d-%m-%Y").to_string()==value&&date.format("%Y").to_string()!="0000"{expected.state="present_exact_date_candidate".into();expected.candidate=json!(date.format("%Y-%m-%d").to_string())}else{expected.state="present_invalid_calendar_date".into()}}
    else{expected.state="present_invalid_calendar_date".into()}
 }else if matches!(name,"Relation"|"Prov-Residence") {
    let text=raw.trim();let digits=text.strip_prefix(['+','-']).unwrap_or(text);
    if digits.is_empty()||!digits.bytes().all(|b|b.is_ascii_digit()){expected.state="present_invalid_or_unsupported_format".into()}
    else{
        let code=text.parse::<i32>().map_err(|_|bad("Info1 integer outside source range"))?;expected.raw_integer_code=Some(code);let mapped=&maps[name][code.to_string()];
        if mapped.is_null(){expected.state=if name=="Relation"{"present_unrecognized_relation_code"}else{"present_unrecognized_code"}.into()}
        else{
            expected.state="present_mapped_code".into();
            if name=="Relation" {expected.candidate=if mapped.is_object(){mapped.clone()}else{json!({"taxpayer_type":"Dependant","relationship":mapped})}}
            else if mapped.is_object(){expected.candidate=mapped["province"].clone();expected.residency=Some(mapped["residency"].as_str().unwrap().into())}
            else{expected.candidate=mapped.clone();expected.residency=Some("Resident".into())}
        }
    }
 }else{
    let mapped=&maps[name][raw];if mapped.is_null(){expected.state="present_unrecognized_code".into()}else{expected.state="present_mapped_code".into();expected.candidate=mapped.clone()}
 }
 expected.display_eligible=context_eligible&&!expected.candidate.is_null();Ok(expected)
}
fn validate(r:&PersonalSourceInfo,context:&Value)->Result<()> {
 let eligible=r.reader_module=="T1";
 if r.source_year_frame_offset>u32::MAX as u64-12||r.source_context_offset>u32::MAX as u64||r.info1.frame_offset>u32::MAX as u64{return Err(bad("Personal source offset exceeds native archive bound"))}
 if r.schema!="dtmax-personal-source-display/v1"||!(1..=32752).contains(&r.source_candidate_line)||!sha(&r.archive_sha256)||!sha(&r.source_exc_sha256)||!number(&r.client_number)||!number(&r.family_file_id)||!(1990..=2100).contains(&r.source_year)||!matches!(r.reader_module.as_str(),"T1"|"T3")||r.info_record_state!="present_nonempty_crc_verified"||r.t1_display_eligible!=eligible||r.display_state!=if eligible{"eligible_t1_source_year"}else{"held_module_origin"}||r.source_year_frame_offset<56||r.source_year_frame_offset+12>r.source_context_offset||r.info1.frame_offset<=r.source_context_offset||!r.info1.crc_verified||!sha(&r.info1.frame_sha256)||!sha(&r.info1.expanded_sha256)||!(130..=65536).contains(&r.info1.expanded_bytes)||r.fields.len()!=7||FIELDS.iter().any(|name|!r.fields.contains_key(*name)){return Err(bad("Personal source context, bounds or display gate differs"))}
 if context["client_number"]!=r.client_number||context["family_file_id"]!=r.family_file_id||context["tax_year"]!=r.source_year||context["archive_sha256"]!=r.archive_sha256||context["vendor_module"]!=r.reader_module{return Err(bad("Personal source requires its exact original historical context"))}
 for (name,f) in &r.fields {if parsed_field(name,f,eligible)?!=*f{return Err(bad("Personal source candidate differs from its raw code or held state"))}}
 Ok(())
}
fn describe(r:&PersonalSourceInfo)->Value {let mut value=serde_json::to_value(r).expect("serializable personal source");value["binding_profile_sha256"]=json!(PROFILE);value["current_profile_updated"]=json!(false);value["identity_verified"]=json!(false);value}
impl SourceImport {
 pub async fn test_copy_personal_source_info(&self,company:&str,batch:PersonalBatch)->Result<Value>{
    self.test_copy_guard(company,batch.test_copy)?;
    if batch.binding_profile_sha256!=PROFILE||batch.records.is_empty()||batch.records.len()>250{return Err(bad("Personal source profile or batch bound differs"))}
    let keys:Vec<_>=batch.records.iter().map(|r|context_key(company,&id(r))).collect();let contexts=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
    if contexts.len()!=batch.records.len(){return Err(storage("Historical context cardinality differs"))}
    let mut seen=BTreeMap::new();let mut prepared=Vec::new();
    for(r,context)in batch.records.iter().zip(contexts){let context:Value=serde_json::from_str(&context.ok_or_else(||bad("Original historical context missing"))?).map_err(|_|storage("Stored historical context unreadable"))?;validate(r,&context)?;
        if seen.insert(id(r),r).is_some_and(|prior|prior!=r){return Err(conflict("Conflicting personal sources in one batch"))}
        let raw=serde_json::to_string(r).map_err(|_|storage("Personal source encoding failed"))?;prepared.push((key(company,&id(r)),raw,id(r)));
    }
    let keys:Vec<_>=prepared.iter().map(|r|r.0.clone()).collect();let existing=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
    if existing.len()!=prepared.len(){return Err(storage("Personal source preflight cardinality differs"))}
    for ((_,raw,_),existing) in prepared.iter().zip(existing){if existing.as_ref().is_some_and(|prior|prior!=raw){return Err(conflict("Historical personal source is immutable"))}}
    let mut created=0;let mut unchanged=0;
    // Keep remote writes bounded. Each record remains write-once; retries repair
    // an interrupted index without replacing any existing historical source.
    for chunk in prepared.chunks(4){let mut writes=tokio::task::JoinSet::new();for(k,raw,identity)in chunk{let store=self.store.clone();let(k,raw,identity,all)=(k.clone(),raw.clone(),identity.clone(),index(company));writes.spawn(async move{
        let added=store.set_nx(&k,&raw).await.map_err(|e|storage(&e))?;if store.get(&k).await.map_err(|e|storage(&e))?.as_deref()!=Some(&raw){return Err(conflict("Historical personal source is immutable"))}store.sadd(&all,&identity).await.map_err(|e|storage(&e))?;Ok(added)
    });}while let Some(result)=writes.join_next().await{if result.map_err(|_|storage("Personal source write task interrupted"))??{created+=1}else{unchanged+=1}}}
    Ok(json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,"current_profiles_modified":0,"original_historical_records_modified":0,"unmasked_SINs_stored":0,"tax_cases_created":0}))
 }
 pub(super) async fn attach_personal_source_info(&self,company:&str,records:&mut[Value])->Result<()> {
    let keys:Vec<_>=records.iter().map(|r|key(company,&format!("{}:{}:{}",r["client_number"].as_str().unwrap_or(""),r["tax_year"],r["archive_sha256"].as_str().unwrap_or("")))).collect();let values=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
    if values.len()!=records.len(){return Err(storage("Personal source page cardinality differs"))}
    for(context,raw)in records.iter_mut().zip(values){if let Some(raw)=raw{let r:PersonalSourceInfo=serde_json::from_str(&raw).map_err(|_|storage("Stored personal source unreadable"))?;validate(&r,context).map_err(|_|storage("Stored personal source fails provenance validation"))?;context["source_personal_info"]=describe(&r)}}Ok(())
 }
 pub async fn test_copy_read_personal_source_info(&self,company:&str,lookup:PersonalLookup)->Result<Value>{
    self.test_copy_guard(company,lookup.test_copy)?;
    if lookup.record_ids.is_empty()||lookup.record_ids.len()>250||lookup.record_ids.iter().any(|id|id.len()>100||!id.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)||b==b':')){return Err(bad("Personal source lookup bound or identities differ"))}
    let keys:Vec<_>=lookup.record_ids.iter().map(|id|key(company,id)).collect();let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;let context_keys:Vec<_>=lookup.record_ids.iter().map(|id|context_key(company,id)).collect();let contexts=self.store.mget(&context_keys).await.map_err(|e|storage(&e))?;
    if rows.len()!=lookup.record_ids.len()||contexts.len()!=rows.len(){return Err(storage("Personal source lookup cardinality differs"))}
    let mut records=Vec::new();
    for((identity,raw),context)in lookup.record_ids.iter().zip(rows).zip(contexts){let r:PersonalSourceInfo=serde_json::from_str(&raw.ok_or_else(||missing("Personal source not imported"))?).map_err(|_|storage("Stored personal source unreadable"))?;let context:Value=serde_json::from_str(&context.ok_or_else(||storage("Historical context missing"))?).map_err(|_|storage("Historical context unreadable"))?;validate(&r,&context).map_err(|_|storage("Personal source context differs"))?;if identity!=&id(&r){return Err(storage("Personal source lookup identity differs"))}records.push(json!({"source_personal_info":describe(&r),"original_historical_record":context}));}
    Ok(json!({"test_copy":true,"source_scope":company,"record_ids":lookup.record_ids,"records":records,"current_profiles_modified":0,"original_historical_records_modified":0}))
 }
 pub async fn test_copy_personal_source_summary(&self,company:&str)->Result<Value>{self.test_copy_guard(company,true)?;let ids=self.store.smembers(&index(company)).await.map_err(|e|storage(&e))?;if ids.len()>100_000{return Err(storage("Personal source index exceeds bound"))}Ok(json!({"test_copy":true,"source_scope":company,"binding_profile_sha256":PROFILE,"source_contexts":ids.len(),"source_fields":ids.len()*7,"current_profiles_modified":0,"unmasked_SINs_stored":0,"tax_cases_created":0}))}
}
