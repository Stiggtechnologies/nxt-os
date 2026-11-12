//! Read-only, scoped data mining of native preparation workpapers. No source imports or tax calculations.
use super::{Tax,TResult,bad,hash,calculations,models::{TaxReturn,ReturnInput}};
use crate::firm::staff::Identity;
use axum::http::StatusCode;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::collections::{BTreeMap,HashSet};

pub const SCHEMA:&str="nxt-tax-report/v1";
pub const MAX_BYTES:usize=3*1024*1024;
pub const MAX_ROWS:usize=10_000;
pub const REQUIRED:&[&str]=&["id","program","tax_year","jurisdiction","revision","calculation_state"];
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predicate { pub column:String,pub operator:String,pub value:Value }
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRequest {
 #[serde(default)] pub program:Option<String>,
 #[serde(default)] pub years:Vec<i32>,
 #[serde(default)] pub jurisdiction:Option<String>,
 #[serde(default)] pub statuses:Vec<String>,
 #[serde(default)] pub client_ref:Option<String>,
 #[serde(default)] pub search:String,
 #[serde(default)] pub columns:Vec<String>,
 #[serde(default)] pub predicates:Vec<Predicate>,
 #[serde(default)] pub include_historical_results:bool,
}
#[derive(Clone,Debug,Serialize)]
pub struct Column {pub key:String,pub label:String,pub kind:String,pub lane:String}
fn column(key:&str,label:&str,kind:&str,lane:&str)->Column {Column{key:key.into(),label:label.into(),kind:kind.into(),lane:lane.into()}}
pub fn columns()->Vec<Column> {
 let mut out=vec![column("id","Workpaper ID","text","identity"),column("name","Name","text","identity"),column("program","Program","text","identity"),column("tax_year","Tax year","integer","identity"),column("jurisdiction","Jurisdiction","text","identity"),column("client_ref","Company / client reference","text","identity"),column("status","Status","text","identity"),column("revision","Revision","integer","identity"),column("input_revision","Input revision","integer","identity"),column("calculation_state","Saved calculation state","text","identity"),column("calculation_ruleset","Saved calculation ruleset","text","identity")];
 let cat=super::catalog::catalog();let mut inputs=BTreeMap::new();
 for p in cat["programs"].as_array().into_iter().flatten() {for section in p["sections"].as_array().into_iter().flatten(){for f in section["fields"].as_array().into_iter().flatten(){
  let Some(key)=f["key"].as_str()else{continue};let kind=match f["kind"].as_str(){Some("money")=>"money",Some("boolean")=>"boolean",Some("number"|"integer")=>"integer",Some("text"|"select")=>"text",_=>continue};
  let label=f["label"].as_str().unwrap_or(key);inputs.entry(key.to_string()).or_insert_with(||column(&format!("input:{key}"),label,kind,"inputs"));
 }}}
 out.extend(inputs.into_values());
 for code in ["15000","22200","22215","23600","26000","30800","31000","33500","33800","34900","34990","35000","40425","42000","42100","43500","44800","48400","48500"]{out.push(column(&format!("line:{code}"),&format!("Saved line {code}"),"money","calculation"));}
 for key in ["taxable_income","net_income","federal_tax","provincial_tax","income_tax","cpp_payable","total_payable","payments","balance","federal_balance","beneficiary_allocations","sbd_income"]{out.push(column(&format!("total:{key}"),&format!("Saved total · {}",key.replace('_'," ")),"money","calculation"));}
 out
}
pub fn catalog()->Value {json!({"schema":SCHEMA,"columns":columns(),"required_columns":REQUIRED,"operators":["equals","not_equals","greater_than","less_than","contains","is_blank","is_present"],"max_rows":MAX_ROWS,"max_response_bytes":MAX_BYTES,"money_unit":"integer_cents","blank_is_zero":false,"native_preparation_workpapers_only":true,"immutable_source_records_included":false,"historical_results_default":false,"financial_approval":false,"filing_supported":false})}
pub(super) fn calc_state(r:&TaxReturn)->TResult<&'static str> {
 let Some(c)=&r.calculation else{return Ok("none")};
 if !calculations::supports_scope(&r.program,r.tax_year,&r.jurisdiction){return Ok("historical_unsupported_scope")}
 if !calculations::uses_current_ruleset(&c.result) || c.input_revision!=r.input_revision || c.input_hash!=hash(&ReturnInput::from(r))? || c.input_document.as_ref().map(hash).transpose()?.as_deref()!=Some(c.input_hash.as_str()) || hash(&c.result)?!=c.output_hash{return Ok("stale")}
 let diagnostics_clear=c.result["diagnostics"].as_array().is_some_and(|d|!d.iter().any(|x|x["blocks_review"]==true||x["severity"]=="error"));
 Ok(if c.result["supported"]==true&&diagnostics_clear{"current_supported"}else{"current_blocked"})
}
fn cell(r:&TaxReturn,c:&Column,state:&str,historical:bool)->TResult<Value> {
 let value=match c.key.as_str(){
  "id"=>json!(r.id),"name"=>json!(r.name),"program"=>json!(r.program),"tax_year"=>json!(r.tax_year),"jurisdiction"=>json!(r.jurisdiction),"client_ref"=>json!(r.client_ref),"status"=>json!(r.status),"revision"=>json!(r.revision),"input_revision"=>json!(r.input_revision),"calculation_state"=>json!(state),"calculation_ruleset"=>r.calculation.as_ref().and_then(|x|x.result.get("ruleset")).cloned().unwrap_or(Value::Null),
  key if key.starts_with("input:")=>r.inputs.get(&key[6..]).cloned().unwrap_or(Value::Null),
  key if key.starts_with("total:")=>if state=="current_supported"||historical{r.calculation.as_ref().and_then(|x|x.result["totals"].get(&key[6..])).cloned().unwrap_or(Value::Null)}else{Value::Null},
  key if key.starts_with("line:")=>if state=="current_supported"||historical{r.calculation.as_ref().and_then(|x|x.result["lines"].as_array()).and_then(|lines|lines.iter().find(|x|x["code"]==&key[5..])).and_then(|x|x.get("amount")).cloned().unwrap_or(Value::Null)}else{Value::Null},
  _=>return Err(bad("unknown report column")),
 };
 if value.is_null(){return Ok(value)}
 let valid=match c.kind.as_str(){"money"=>value.as_i64().is_some_and(|n|n.unsigned_abs()<=100_000_000_000_000),"integer"=>value.as_i64().is_some(),"boolean"=>value.is_boolean(),"text"=>value.as_str().is_some_and(|s|s.len()<=32_000),_=>false};
 if !valid{return Err(bad("A selected saved report field has an invalid scalar type; no partial report was produced"))}Ok(value)
}
fn matches(value:&Value,p:&Predicate,kind:&str)->TResult<bool> {
 let blank=value.is_null()||value.as_str().is_some_and(|s|s.trim().is_empty());
 if p.operator=="is_blank"{return Ok(blank)}if p.operator=="is_present"{return Ok(!blank)}
 if value.is_null(){return Ok(false)}
 match p.operator.as_str(){
  "equals"=>Ok(value==&p.value),"not_equals"=>Ok(value!=&p.value),
  "contains" if kind=="text"=>Ok(value.as_str().unwrap_or("").to_lowercase().contains(&p.value.as_str().unwrap_or("").to_lowercase())),
  "greater_than"|"less_than" if kind=="money"||kind=="integer"=>{let a=value.as_i64().ok_or_else(||bad("numeric report filter requires integer cents or integer values"))?;let b=p.value.as_i64().ok_or_else(||bad("numeric report filter requires an integer value"))?;Ok(if p.operator=="greater_than"{a>b}else{a<b})},
  _=>Err(bad("operator is unavailable for this report column")),
 }
}
pub fn project(records:&[TaxReturn],mut request:ReportRequest,actor:Option<&Identity>)->TResult<Value> {
 let available=columns();let bykey:BTreeMap<_,_>=available.iter().map(|c|(c.key.as_str(),c)).collect();
 if request.search.len()>200 || request.years.len()>101 || request.statuses.len()>7 || request.columns.len()>48 || request.predicates.len()>8{return Err(bad("report selection exceeds its limits"))}
 if let Some(p)=&mut request.program{*p=p.to_ascii_uppercase();if !["T1","T2","T3","T5013"].contains(&p.as_str()){return Err(bad("unknown report program"))}}
 if let Some(j)=&mut request.jurisdiction{*j=j.to_ascii_uppercase();if !["AB","BC","MB","NB","NL","NS","NT","NU","ON","PE","QC","SK","YT","CA","CA-FED"].contains(&j.as_str()){return Err(bad("unknown report jurisdiction"))}}
 if request.years.iter().any(|y|!(2000..=2100).contains(y)) || request.statuses.iter().any(|s|!["draft","calculated","in_review","approved","filed","archived"].contains(&s.as_str())){return Err(bad("unknown report year or status"))}
 if let Some(client)=&request.client_ref{if client.len()>100{return Err(bad("invalid report client reference"))}if actor.is_some_and(|i|!i.can_client(client)){return Err(crate::referral::routes::ApiError(StatusCode::FORBIDDEN,"Report client is outside your assigned scope".into()))}}
 request.years.sort();request.years.dedup();request.statuses.sort();request.statuses.dedup();
 if request.columns.is_empty(){request.columns=vec!["name".into(),"client_ref".into(),"status".into(),"line:15000".into(),"line:26000".into(),"line:42000".into()]}
 let mut selected=REQUIRED.iter().map(|s|s.to_string()).collect::<Vec<_>>();let mut seen:HashSet<String>=selected.iter().cloned().collect();
 for key in &request.columns {if !bykey.contains_key(key.as_str()){return Err(bad("unknown report column"))}if seen.insert(key.clone()){selected.push(key.clone())}}
 for p in &request.predicates {
  let c=bykey.get(p.column.as_str()).ok_or_else(||bad("unknown report predicate column"))?;
  if !["equals","not_equals","greater_than","less_than","contains","is_blank","is_present"].contains(&p.operator.as_str()){return Err(bad("unknown report operator"))}
  if ["is_blank","is_present"].contains(&p.operator.as_str())&&!p.value.is_null(){return Err(bad("blank/present predicates require a null filter value"))}
  if !["is_blank","is_present"].contains(&p.operator.as_str()){
   let valid=match c.kind.as_str(){"money"=>p.value.as_i64().is_some_and(|n|n.unsigned_abs()<=100_000_000_000_000),"integer"=>p.value.as_i64().is_some(),"boolean"=>p.value.is_boolean(),"text"=>p.value.as_str().is_some_and(|s|s.len()<=200),_=>false};if !valid{return Err(bad("report filter has the wrong scalar type"))}
  }
  if p.operator=="contains"&&c.kind!="text" || ["greater_than","less_than"].contains(&p.operator.as_str())&&! ["money","integer"].contains(&c.kind.as_str()){return Err(bad("operator is unavailable for this report column"))}
 }
 let needle=request.search.trim().to_lowercase();let mut scoped:Vec<_>=records.iter().filter(|r|actor.is_none_or(|i|i.can_client(&r.client_ref))).filter(|r|request.client_ref.as_ref().is_none_or(|c|c.eq_ignore_ascii_case(&r.client_ref))).filter(|r|request.program.as_ref().is_none_or(|p|p==&r.program)&& (request.years.is_empty()||request.years.contains(&r.tax_year))&&request.jurisdiction.as_ref().is_none_or(|j|j==&r.jurisdiction)&&(request.statuses.is_empty()||request.statuses.contains(&r.status))&&(needle.is_empty()||format!("{} {} {}",r.id,r.name,r.client_ref).to_lowercase().contains(&needle))).collect();
 scoped.sort_by(|a,b|a.id.cmp(&b.id));let mut rows=Vec::new();let mut row_ids=HashSet::new();
 for r in scoped{
  let state=calc_state(r)?;let mut keep=true;for p in &request.predicates{let c=bykey[p.column.as_str()];if !matches(&cell(r,c,state,request.include_historical_results)?,p,&c.kind)?{keep=false;break}}
  if !keep{continue}if !row_ids.insert(r.id.clone()){return Err(bad("duplicate saved workpaper ID; no partial report was produced"))}if rows.len()>=MAX_ROWS{return Err(bad("report exceeds10000rows; narrow the filters; no partial export was produced"))}
  let mut row=BTreeMap::new();for key in &selected{row.insert(key.clone(),cell(r,bykey[key.as_str()],state,request.include_historical_results)?);}rows.push(row);
 }
 let cols:Vec<_>=selected.iter().map(|k|bykey[k.as_str()]).collect();
 let snapshot=hash(&json!({"request":request,"columns":cols,"rows":rows}))?;
 let result=json!({"schema":SCHEMA,"request":request,"columns":cols,"row_count":rows.len(),"rows":rows,"snapshot_sha256":snapshot,"complete":true,"atomic_snapshot":true,"native_preparation_workpapers_only":true,"source_records_included":false,"money_unit":"integer_cents","blank_is_zero":false,"financial_approval":false,"filing_supported":false});
 if serde_json::to_vec(&result).map_err(|_|bad("cannot encode report"))?.len()>MAX_BYTES{return Err(bad("report exceeds3MiB; narrow the columns/filters; no partial export was produced"))}Ok(result)
}
impl Tax {pub async fn report(&self,request:ReportRequest,actor:Option<&Identity>)->TResult<Value>{project(&self.load_state().await?.returns,request,actor)}}
