//! Durable, bounded workpaper jobs. No approval, authorization, filing or rollforward.
use super::{Tax,TResult,models::*,bad,conflict,hash,input_hash,id,now,calculations};
use crate::firm::staff::{Identity,Role};
use axum::{Router,Json,extract::{State,Path,Query},Extension,http::StatusCode,routing::{get,post}};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use chrono::{Utc,Duration};
use std::{sync::Arc,collections::BTreeSet};
fn is_false(v:&bool)->bool{!*v}
const MAX_JOBS:usize=10;const MAX_ITEMS:usize=25;const ITEM_BYTES:usize=1024*1024;const ARTIFACT_BYTES:usize=2*1024*1024;
#[derive(Clone,Copy,Debug,Serialize,Deserialize,PartialEq)]
#[serde(rename_all="snake_case")]pub enum Operation{Calculate,Package,Print}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Selection{pub return_id:String,pub expected_revision:u64}
#[derive(Clone,Debug,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct Create{pub request_key:String,pub operation:Operation,pub company_ref:Option<String>,pub selections:Vec<Selection>}
#[derive(Clone,Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Step{pub expected_revision:u64,pub request_key:String,pub company_ref:Option<String>}
#[derive(Clone,Debug,Deserialize)]#[serde(deny_unknown_fields)]pub struct Cancel{pub expected_revision:u64,pub company_ref:Option<String>}
#[derive(Clone,Debug,Default,Deserialize)]#[serde(deny_unknown_fields)]pub struct Scope{pub company_ref:Option<String>,#[serde(default,rename="path")]pub _path:Option<String>}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]#[serde(rename_all="snake_case")]pub enum ItemState{Pending,Running,Succeeded,Held,Cancelled}
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct Artifact{pub filename:String,pub content_type:String,pub bytes:usize,pub sha256:String,#[serde(default,skip_serializing_if="Option::is_none")]pub body:Option<String>}
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct Item{
 pub selection:Selection,pub client_ref:String,pub name:String,pub program:String,pub tax_year:i32,pub jurisdiction:String,
 pub input_hash:String,pub input_revision:u64,pub return_sha256:String,pub calculation_key:String,pub state:ItemState,
 pub attempts:u8,pub claim_token:Option<String>,pub captured_at:Option<String>,pub lease_until:Option<String>,pub outcome:Option<Value>,pub artifact:Option<Artifact>,
}
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct BatchJob{
 pub id:String,pub revision:u64,pub operation:Operation,pub company_ref:Option<String>,pub created_by:String,pub created_at:String,pub updated_at:String,
 pub request_key:String,pub request_sha256:String,pub original_manifest:Value,pub original_manifest_sha256:String,pub ruleset:String,
 pub status:String,pub cancel_requested:bool,pub items:Vec<Item>,pub step_keys:Vec<String>,
 #[serde(default,skip_serializing_if="is_false")]pub archived:bool,
}
#[derive(Clone,Debug,Serialize,Deserialize)]pub struct ArchiveRef{pub id:String,pub revision:u64,pub operation:Operation,pub company_ref:Option<String>,pub created_by:String,pub request_key:String,pub request_sha256:String,pub bytes:usize,pub sha256:String,pub archived_at:String,pub bindings:Vec<(String,String)>}
fn archive_key(id:&str)->String{format!("tax:batch-archive:v1:{id}")}
fn archive_scope(a:&Identity,s:&TaxState,r:&ArchiveRef,company:&Option<String>)->TResult<()>{if &r.company_ref!=company{return Err(forbidden())}for(id,client)in &r.bindings{let row=s.returns.iter().find(|v|&v.id==id).ok_or_else(forbidden)?;if &row.client_ref!=client||!allowed(a,client)||company.as_ref().is_some_and(|c|c!=client){return Err(forbidden())}}Ok(())}
fn forbidden()->crate::referral::routes::ApiError{super::ApiError(StatusCode::FORBIDDEN,"batch job is outside the authorized client/company scope".into())}
fn writer(a:&Identity)->TResult<()>{if matches!(a.role,Role::Admin|Role::Preparer|Role::Reviewer){Ok(())}else{Err(forbidden())}}
fn allowed(a:&Identity,c:&str)->bool{a.all_clients()||a.can_client(c)}
fn authorize(a:&Identity,s:&TaxState,j:&BatchJob,company:&Option<String>)->TResult<()>{
 if hash(&j.original_manifest)?!=j.original_manifest_sha256{return Err(conflict("original selection manifest integrity failed"))}
 if &j.company_ref!=company{return Err(forbidden())}
 for i in &j.items{let r=s.returns.iter().find(|r|r.id==i.selection.return_id).ok_or_else(forbidden)?;
  if r.client_ref!=i.client_ref||!allowed(a,&r.client_ref)||company.as_ref().is_some_and(|c|c!=&r.client_ref){return Err(forbidden())}}
 Ok(())
}
fn view(mut j:BatchJob)->BatchJob{for i in &mut j.items{if let Some(a)=&mut i.artifact{a.body=None;}}j}
fn refresh(j:&mut BatchJob){j.status=if j.items.iter().any(|i|i.state==ItemState::Running){if j.cancel_requested{"cancelling"}else{"running"}}else if j.cancel_requested{"cancelled"}else if j.items.iter().any(|i|i.state==ItemState::Pending){"ready"}else if j.items.iter().any(|i|i.state==ItemState::Held){"completed_with_holds"}else{"completed"}.into();j.revision+=1;j.updated_at=now();}
fn outcome(code:&str,message:&str)->Value{json!({"code":code,"message":message})}
fn recovered(r:&TaxReturn,i:&Item,rules:&str)->TResult<Option<Value>>{
 if rules!=calculations::RULESET{return Ok(Some(json!({"code":"ruleset_changed","message":"Captured job uses a historical ruleset; create a new explicit selection. Saved receipts and artifacts remain historical.","supported":false,"recovered_from_history":false})))}
 for c in &r.calculations{if c.idempotency_key.as_deref()==Some(i.calculation_key.as_str())&&c.request_revision==i.selection.expected_revision&&c.input_revision==i.input_revision&&c.input_hash==i.input_hash{
  if !calculations::uses_current_ruleset(&c.result){return Ok(Some(json!({"code":"ruleset_changed","message":"Committed snapshot uses a historical ruleset; retained as evidence, not current supported calculation.","snapshot_id":c.id,"supported":false,"recovered_from_history":true})))}
  if c.output_hash!=hash(&c.result)?||c.input_document.as_ref().map(hash).transpose()?.as_deref()!=Some(c.input_hash.as_str()){return Err(conflict("saved batch calculation integrity failed"))}
  return Ok(Some(json!({"code":"calculation_committed","snapshot_id":c.id,"input_hash":c.input_hash,"output_hash":c.output_hash,"supported":c.result["supported"],"recovered_from_history":true})))
 }}Ok(None)
}
pub(super) fn package_snapshot(r:&TaxReturn,at:&str)->TResult<Value>{Ok(json!({"format":"nxt-tax-return/v1","classification":"internal_workpaper_not_for_filing","exported_at":at,"return":ReturnInput::from(r),"calculation_snapshot":r.calculation,"calculation_history":r.calculations,"review":r.review,"review_history":r.reviews,"input_hash":input_hash(r)?,"filing_enabled":false,"current_scope_supported":calculations::supports_scope(&r.program,r.tax_year,&r.jurisdiction),"current_ruleset":calculations::RULESET,"current_calculation_ruleset_current":r.calculation.as_ref().is_some_and(|c|calculations::uses_current_ruleset(&c.result))}))}
impl Tax{
 async fn load_batch_archive(&self,a:&Identity,key:&str,company:&Option<String>)->TResult<BatchJob>{let s=self.load_state().await?;let r=s.batch_archives.iter().find(|r|r.id==key).ok_or_else(super::missing)?.clone();archive_scope(a,&s,&r,company)?;let raw=self.store.get(&archive_key(key)).await?.ok_or_else(super::missing)?;if raw.len()!=r.bytes||hex::encode(sha2::Sha256::digest(raw.as_bytes()))!=r.sha256{return Err(conflict("archived batch integrity failed"))}let mut j:BatchJob=serde_json::from_str(&raw).map_err(|_|conflict("archived batch is corrupt"))?;if j.id!=r.id||j.revision!=r.revision{return Err(conflict("archive registry differs"))}authorize(a,&self.load_state().await?,&j,company)?;j.archived=true;Ok(j)}
 pub async fn batch_archive_list(&self,a:&Identity,company:Option<String>)->TResult<Vec<Value>>{let s=self.load_state().await?;Ok(s.batch_archives.iter().filter(|r|archive_scope(a,&s,r,&company).is_ok()).map(|r|json!({"id":r.id,"revision":r.revision,"operation":r.operation,"company_ref":r.company_ref,"status":"archived","item_count":r.bindings.len(),"sha256":r.sha256,"bytes":r.bytes})).collect())}
 pub async fn batch_archive(&self,a:&Identity,key:&str,b:Cancel)->TResult<BatchJob>{
  writer(a)?;let s=self.load_state().await?;
  if s.batch_archives.iter().any(|r|r.id==key){let j=self.load_batch_archive(a,key,&b.company_ref).await?;if j.revision!=b.expected_revision{return Err(conflict("archived revision differs"))}return Ok(view(j))}
  let j=s.batches.iter().find(|j|j.id==key).ok_or_else(super::missing)?.clone();authorize(a,&s,&j,&b.company_ref)?;
  if j.revision!=b.expected_revision{return Err(conflict("batch revision changed"))}if j.items.iter().any(|i|matches!(i.state,ItemState::Pending|ItemState::Running)||i.outcome.as_ref().is_some_and(|o|o["code"]=="cancelled_inflight_unconfirmed"||o["code"]=="retry_limit")){return Err(conflict("finish or reconcile all in-flight outcomes before archiving"))}
  let raw=serde_json::to_string(&j).map_err(|_|bad("archive encoding failed"))?;let digest=hex::encode(sha2::Sha256::digest(raw.as_bytes()));
  let fits=|s:&TaxState|s.batch_archives.len()<100&&s.batch_archives.iter().map(|r|r.bytes).sum::<usize>()+raw.len()<=8*1024*1024;
  if !fits(&s){return Err(conflict("immutable batch archive capacity reached (100 jobs /8 MiB); nothing evicted"))}
  // Immutable copy FIRST. A retry accepts only the exact same bytes; never overwrite.
  if !self.store.set_nx(&archive_key(key),&raw).await?&&self.store.get(&archive_key(key)).await?.as_deref()!=Some(raw.as_str()){return Err(conflict("archive copy differs; active job retained"))}
  self.mutate_state(|s|{if let Some(r)=s.batch_archives.iter().find(|r|r.id==key){archive_scope(a,s,r,&b.company_ref)?;if r.sha256!=digest{return Err(conflict("archive registry differs"))}return Ok(())}
   let p=s.batches.iter().position(|j|j.id==key).ok_or_else(super::missing)?;authorize(a,s,&s.batches[p],&b.company_ref)?;
   if hash(&s.batches[p])?!=hash(&j)?{return Err(conflict("batch changed after archive copy; active job retained"))}if !fits(s){return Err(conflict("archive capacity changed; active job retained"))}
   s.batch_archives.push(ArchiveRef{id:j.id.clone(),revision:j.revision,operation:j.operation,company_ref:j.company_ref.clone(),created_by:j.created_by.clone(),request_key:j.request_key.clone(),request_sha256:j.request_sha256.clone(),bytes:raw.len(),sha256:digest.clone(),archived_at:now(),bindings:j.items.iter().map(|i|(i.selection.return_id.clone(),i.client_ref.clone())).collect()});s.batches.remove(p);super::audit(s,"","tax_batch_archived",format!("{} by {}",j.id,a.label()));Ok(())
  }).await?;Ok(view(self.load_batch_archive(a,key,&b.company_ref).await?))
 }
 pub async fn batch_create(&self,a:&Identity,b:Create)->TResult<BatchJob>{
  writer(a)?;super::text(&b.request_key,"batch request key",100,true)?;
  if b.selections.is_empty()||b.selections.len()>MAX_ITEMS||b.company_ref.as_ref().is_some_and(|c|c.trim().is_empty()||c.len()>120){return Err(bad("select1–25 saved returns and an exact client reference or all-authorized scope"))}
  let mut seen=BTreeSet::new();if b.selections.iter().any(|v|v.return_id.is_empty()||v.return_id.len()>100||!seen.insert(v.return_id.clone())){return Err(bad("duplicate or invalid selected return"))}
  let key=id("taxbatch");let request_hash=hash(&b)?;let at=now();
  let prior=self.load_state().await?;if let Some(r)=prior.batch_archives.iter().find(|r|r.created_by==a.user_id&&r.request_key==b.request_key){archive_scope(a,&prior,r,&b.company_ref)?;if r.request_sha256!=request_hash{return Err(conflict("batch key already describes another archived selection"))}return Ok(view(self.load_batch_archive(a,&r.id,&b.company_ref).await?))}
  self.mutate_state(|s|{
   if s.batch_archives.iter().any(|r|r.created_by==a.user_id&&r.request_key==b.request_key){return Err(conflict("request key belongs to an archived job; reload saved history"))}
   if let Some(j)=s.batches.iter().find(|j|j.created_by==a.user_id&&j.request_key==b.request_key){authorize(a,s,j,&b.company_ref)?;if j.request_sha256!=request_hash{return Err(conflict("batch key already describes another selection"))}return Ok(view(j.clone()))}
   if s.batches.len()>=MAX_JOBS{return Err(conflict("batch workspace holds 10 active jobs; archive finished jobs to free capacity"))}
   let mut items=Vec::new();for (n,v) in b.selections.iter().enumerate(){let r=s.returns.iter().find(|r|r.id==v.return_id).ok_or_else(forbidden)?;
    if !allowed(a,&r.client_ref)||b.company_ref.as_ref().is_some_and(|c|c!=&r.client_ref){return Err(forbidden())}super::assert_revision(r,v.expected_revision)?;
    items.push(Item{selection:v.clone(),client_ref:r.client_ref.clone(),name:r.name.clone(),program:r.program.clone(),tax_year:r.tax_year,jurisdiction:r.jurisdiction.clone(),input_hash:input_hash(r)?,input_revision:r.input_revision,return_sha256:hash(r)?,calculation_key:format!("{key}:{n}"),state:ItemState::Pending,attempts:0,claim_token:None,captured_at:None,lease_until:None,outcome:None,artifact:None});
   }
   let original=json!({"format":"nxt-tax-batch-selection/v1","request":b,"captured_returns":items.iter().map(|i|json!({"return_id":i.selection.return_id,"expected_revision":i.selection.expected_revision,"client_ref":i.client_ref,"input_hash":i.input_hash,"return_sha256":i.return_sha256})).collect::<Vec<_>>()});
   let j=BatchJob{id:key.clone(),revision:1,operation:b.operation,company_ref:b.company_ref.clone(),created_by:a.user_id.clone(),created_at:at.clone(),updated_at:at.clone(),request_key:b.request_key.clone(),request_sha256:request_hash.clone(),original_manifest_sha256:hash(&original)?,original_manifest:original,ruleset:calculations::RULESET.into(),status:"ready".into(),cancel_requested:false,items,step_keys:vec![],archived:false};
   s.batches.push(j.clone());super::audit(s,"","tax_batch_created",format!("{} {:?} /{} items by {}",j.id,j.operation,j.items.len(),a.label()));Ok(view(j))
  }).await
 }
 pub async fn batch_list(&self,a:&Identity,company:Option<String>)->TResult<Vec<BatchJob>>{let s=self.load_state().await?;Ok(s.batches.iter().filter(|j|authorize(a,&s,j,&company).is_ok()).cloned().map(view).collect())}
 pub async fn batch_get(&self,a:&Identity,key:&str,company:&Option<String>)->TResult<BatchJob>{let s=self.load_state().await?;if let Some(j)=s.batches.iter().find(|j|j.id==key){authorize(a,&s,j,company)?;return Ok(view(j.clone()))}Ok(view(self.load_batch_archive(a,key,company).await?))}
 pub async fn batch_cancel(&self,a:&Identity,key:&str,b:Cancel)->TResult<BatchJob>{writer(a)?;self.mutate_state(|s|{let p=s.batches.iter().position(|j|j.id==key).ok_or_else(super::missing)?;let mut j=s.batches[p].clone();authorize(a,s,&j,&b.company_ref)?;
   if j.cancel_requested||!j.items.iter().any(|i|matches!(i.state,ItemState::Pending|ItemState::Running)){return Ok(view(j))}if j.revision!=b.expected_revision{return Err(conflict("batch revision changed"))}
   j.cancel_requested=true;for i in &mut j.items{if i.state==ItemState::Pending{i.state=ItemState::Cancelled;i.outcome=Some(outcome("cancelled_before_claim","No operation started"));}}
   refresh(&mut j);s.batches[p]=j.clone();super::audit(s,"","tax_batch_cancelled",format!("{} by {}",j.id,a.label()));Ok(view(j))
  }).await}
 pub async fn batch_step(&self,a:&Identity,key:&str,b:Step)->TResult<BatchJob>{
  writer(a)?;super::text(&b.request_key,"step request key",100,true)?;let claim=id("claim");
  let work=self.mutate_state(|s|{let p=s.batches.iter().position(|j|j.id==key).ok_or_else(super::missing)?;let mut j=s.batches[p].clone();authorize(a,s,&j,&b.company_ref)?;
   let replay=j.step_keys.contains(&b.request_key);if replay&&!(j.step_keys.last()==Some(&b.request_key)&&j.items.iter().any(|i|i.state==ItemState::Running)){return Ok(None)}if !replay&&j.revision!=b.expected_revision{return Err(conflict("batch revision changed; refresh before resuming"))}
   let running=j.items.iter().position(|i|i.state==ItemState::Running);
   if let Some(n)=running{
    let i=&j.items[n];let r=s.returns.iter().find(|r|r.id==i.selection.return_id).ok_or_else(forbidden)?;
    if let Some(mut o)=if j.operation==Operation::Calculate{recovered(r,i,&j.ruleset)?}else{None}{o["recorded_after_cancel"]=json!(j.cancel_requested);if !j.step_keys.contains(&b.request_key)&&j.step_keys.len()<125{j.step_keys.push(b.request_key.clone());}j.items[n].state=if o["supported"]==true{ItemState::Succeeded}else{ItemState::Held};j.items[n].outcome=Some(o);refresh(&mut j);let jid=j.id.clone();s.batches[p]=j;super::audit(s,"","tax_batch_item_reconciled",format!("{} item{} by {}",jid,n,a.label()));return Ok(None)}
    if i.lease_until.as_ref().is_some_and(|v|v>&now()){return Ok(None)}
    if j.cancel_requested{j.items[n].state=ItemState::Cancelled;j.items[n].outcome=Some(outcome("cancelled_inflight_unconfirmed","No committed job-key snapshot found. An already claimed call may still finish; refresh this job and inspect the return before another batch."));refresh(&mut j);let jid=j.id.clone();s.batches[p]=j;super::audit(s,"","tax_batch_item_reconciled",format!("{} item{} by {}",jid,n,a.label()));return Ok(None)}
   }
   let Some(n)=running.or_else(||j.items.iter().position(|i|i.state==ItemState::Pending))else{return Ok(None)};
   if j.cancel_requested{return Ok(None)}
   let i=&mut j.items[n];if i.attempts>=5{i.state=ItemState::Held;i.outcome=Some(outcome("retry_limit","Inspect the saved return before making another job"));refresh(&mut j);let jid=j.id.clone();s.batches[p]=j;super::audit(s,"","tax_batch_item_reconciled",format!("{} item{} by {}",jid,n,a.label()));return Ok(None)}
   if !replay&&j.step_keys.len()>=125{return Err(conflict("job step replay ledger reached its bound"))}i.attempts+=1;i.state=ItemState::Running;i.claim_token=Some(claim.clone());i.captured_at.get_or_insert_with(now);i.lease_until=Some((Utc::now()+Duration::seconds(30)).to_rfc3339_opts(chrono::SecondsFormat::Secs,true));
   let item=i.clone();if !replay{j.step_keys.push(b.request_key.clone());}refresh(&mut j);let op=j.operation;let rules=j.ruleset.clone();let jid=j.id.clone();s.batches[p]=j;super::audit(s,"","tax_batch_claimed",format!("{} item{} by {}",jid,n,a.label()));Ok(Some((n,item,op,rules)))
  }).await?;
  if let Some((n,item,op,rules))=work{
   let state=self.load_state().await?;let job=state.batches.iter().find(|j|j.id==key).ok_or_else(super::missing)?;authorize(a,&state,job,&b.company_ref)?;
   let r=state.returns.iter().find(|r|r.id==item.selection.return_id).ok_or_else(forbidden)?.clone();
   let result: TResult<(Value,Option<Artifact>)>=if let Some(o)=if op==Operation::Calculate{recovered(&r,&item,&rules)?}else{None}{Ok((o,None))}
   else if job.cancel_requested{Ok((outcome("cancelled_before_operation","Claim cancelled before operation"),None))}
   else if r.revision!=item.selection.expected_revision||input_hash(&r)?!=item.input_hash||hash(&r)?!=item.return_sha256{Ok((outcome("stale_selection","Saved return changed after selection; no new operation performed"),None))}
   else if op==Operation::Calculate{
    if rules!=calculations::RULESET{Ok((outcome("ruleset_changed","Create a new explicit selection for the current ruleset"),None))}
    else if !calculations::supports_scope(&r.program,r.tax_year,&r.jurisdiction){Ok((outcome("unsupported_scope","No current calculation dispatcher for this saved return; no calculation performed"),None))}
    else{self.calculate(&r.id,RevisionInput{expected_revision:item.selection.expected_revision,idempotency_key:Some(item.calculation_key.clone())}).await.map(|r|{let c=r.calculation.unwrap();(json!({"code":"calculation_committed","snapshot_id":c.id,"input_hash":c.input_hash,"output_hash":c.output_hash,"supported":c.result["supported"],"recovered_from_history":false}),None)})}
   }else{(||->TResult<(Value,Option<Artifact>)>{
    let body=if op==Operation::Package{serde_json::to_string_pretty(&package_snapshot(&r,item.captured_at.as_deref().unwrap())?).map_err(|_|bad("package encoding failed"))?}else{super::routes::print_workpaper(&r)?};
    if body.len()>ITEM_BYTES{Ok((outcome("artifact_size_hold","Captured export exceeds1 MiB; use the individual workpaper export"),None))}else{let artifact=Artifact{filename:format!("nxt-tax-{}-{}.{}",r.id,if op==Operation::Package{"package"}else{"print"},if op==Operation::Package{"json"}else{"html"}),content_type:if op==Operation::Package{"application/json"}else{"text/html"}.into(),bytes:body.len(),sha256:hex::encode(sha2::Sha256::digest(body.as_bytes())),body:Some(body)};if serde_json::to_vec(&artifact).map_err(|_|bad("export transport encoding failed"))?.len()>3*1024*1024{Ok((outcome("artifact_size_hold","Export transport exceeds3 MiB; use individual export"),None))}else{Ok((outcome("export_ready","Immutable captured workpaper export; not a filing document"),Some(artifact)))}}
   })()};
   let (result,artifact)=match result{Ok(v)=>v,Err(e)if e.0.is_server_error()=>return Err(e),Err(e)=>(json!({"code":"operation_held","message":e.1}),None)};
   self.mutate_state(|s|{let p=s.batches.iter().position(|j|j.id==key).ok_or_else(super::missing)?;let mut j=s.batches[p].clone();authorize(a,s,&j,&b.company_ref)?;
    if j.items[n].claim_token.as_deref()!=Some(claim.as_str()){return Ok(())}
    let total:usize=s.batches.iter().flat_map(|j|&j.items).filter_map(|i|i.artifact.as_ref()).map(|a|a.bytes).sum();let mut art=artifact.clone();let mut out=result.clone();
    if art.as_ref().is_some_and(|v|total+v.bytes>ARTIFACT_BYTES){art=None;out=outcome("artifact_capacity_hold","All durable batch artifacts together are bounded to2 MiB; use individual export");}
    out["recorded_after_cancel"]=json!(j.cancel_requested);let success=out["code"]=="export_ready"||out["code"]=="calculation_committed"&&out["supported"]==true;
    j.items[n].state=if success{ItemState::Succeeded}else if out["code"]=="cancelled_before_operation"{ItemState::Cancelled}else{ItemState::Held};j.items[n].outcome=Some(out);j.items[n].artifact=art;refresh(&mut j);let jid=j.id.clone();s.batches[p]=j;super::audit(s,"","tax_batch_item_recorded",format!("{} item{} by {}",jid,n,a.label()));Ok(())
   }).await?;
  }
  self.batch_get(a,key,&b.company_ref).await
 }
 pub async fn batch_manifest(&self,a:&Identity,key:&str,company:&Option<String>)->TResult<Value>{let j=self.batch_get(a,key,company).await?;if hash(&j.original_manifest)?!=j.original_manifest_sha256{return Err(conflict("batch original manifest integrity failed"))}let payload=json!({"format":"nxt-tax-batch-manifest/v1","original_manifest":j.original_manifest,"original_manifest_sha256":j.original_manifest_sha256,"job":j});Ok(json!({"manifest_sha256":hash(&payload)?,"manifest":payload}))}
 pub async fn batch_artifact(&self,a:&Identity,key:&str,n:usize,company:&Option<String>)->TResult<Artifact>{let s=self.load_state().await?;let j=if let Some(j)=s.batches.iter().find(|j|j.id==key){authorize(a,&s,j,company)?;j.clone()}else{self.load_batch_archive(a,key,company).await?};let artifact=j.items.get(n).and_then(|i|i.artifact.clone()).ok_or_else(super::missing)?;let body=artifact.body.as_ref().ok_or_else(super::missing)?;if body.len()!=artifact.bytes||hex::encode(sha2::Sha256::digest(body.as_bytes()))!=artifact.sha256{return Err(conflict("captured export integrity failed"))}Ok(artifact)}
}
type S=State<Arc<Tax>>;
pub(super)fn routes()->Router<Arc<Tax>>{Router::new().route("/api/tax/batches",get(list).post(create)).route("/api/tax/batches/archives",get(archives)).route("/api/tax/batches/{id}",get(detail)).route("/api/tax/batches/{id}/archive",post(archive)).route("/api/tax/batches/{id}/step",post(step)).route("/api/tax/batches/{id}/cancel",post(cancel)).route("/api/tax/batches/{id}/manifest",get(manifest)).route("/api/tax/batches/{id}/items/{index}/artifact",get(artifact))}
async fn list(State(t):S,Extension(a):Extension<Identity>,Query(q):Query<Scope>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_list(&a,q.company_ref).await?)))}
async fn detail(State(t):S,Extension(a):Extension<Identity>,Path(k):Path<String>,Query(q):Query<Scope>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_get(&a,&k,&q.company_ref).await?)))}
async fn create(State(t):S,Extension(a):Extension<Identity>,Json(b):Json<Create>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_create(&a,b).await?)))}
async fn step(State(t):S,Extension(a):Extension<Identity>,Path(k):Path<String>,Json(b):Json<Step>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_step(&a,&k,b).await?)))}
async fn cancel(State(t):S,Extension(a):Extension<Identity>,Path(k):Path<String>,Json(b):Json<Cancel>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_cancel(&a,&k,b).await?)))}
async fn manifest(State(t):S,Extension(a):Extension<Identity>,Path(k):Path<String>,Query(q):Query<Scope>)->TResult<Json<Value>>{Ok(Json(t.batch_manifest(&a,&k,&q.company_ref).await?))}
async fn artifact(State(t):S,Extension(a):Extension<Identity>,Path((k,n)):Path<(String,usize)>,Query(q):Query<Scope>)->TResult<Json<Artifact>>{Ok(Json(t.batch_artifact(&a,&k,n,&q.company_ref).await?))}
use sha2::Digest;

#[cfg(test)]mod tests{
 use super::*;use crate::store::Store;
 fn actor(client:&str)->Identity{Identity{user_id:format!("synthetic-{client}"),display_name:"Synthetic operator".into(),role:Role::Preparer,clients:vec![client.into()],break_glass:false,mfa:true,must_change_password:false}}
 fn input(client:&str,j:&str)->ReturnInput{serde_json::from_value(json!({"program":"T1","tax_year":2025,"jurisdiction":j,"name":"Synthetic <script>workpaper</script>","client_ref":client,"inputs":{"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"employment_income_cents":5000000}})).unwrap()}
 fn create(r:&TaxReturn,op:Operation,key:&str)->Create{Create{request_key:key.into(),operation:op,company_ref:Some(r.client_ref.clone()),selections:vec![Selection{return_id:r.id.clone(),expected_revision:r.revision}]}}
 fn step(j:&BatchJob,key:&str)->Step{Step{expected_revision:j.revision,request_key:key.into(),company_ref:j.company_ref.clone()}}
 async fn running(t:&Tax,j:&BatchJob,key:&str){t.mutate_state(|s|{let j=s.batches.iter_mut().find(|v|v.id==j.id).unwrap();j.items[0].state=ItemState::Running;j.items[0].attempts=1;j.items[0].captured_at=Some(now());j.items[0].lease_until=Some("2000-01-01T00:00:00Z".into());j.items[0].claim_token=Some("lost-claim".into());j.step_keys.push(key.into());j.revision+=1;Ok(())}).await.ok().unwrap();}
 #[tokio::test]async fn captured_exports_keep_returns_hashes_and_original_manifest_unchanged(){
  assert!(serde_json::to_value(TaxState::default()).unwrap().get("batches").is_none());
  let t=Tax::new(Store::memory());let a=actor("A");let r=t.save(None,input("A","AB")).await.ok().unwrap();let before=hash(&r).ok().unwrap();
  for(op,key)in[(Operation::Package,"package"),(Operation::Print,"print")]{let j=t.batch_create(&a,create(&r,op,key)).await.ok().unwrap();let original=j.original_manifest_sha256.clone();let done=t.batch_step(&a,&j.id,step(&j,key)).await.ok().unwrap();assert_eq!(done.status,"completed");assert_eq!(done.original_manifest_sha256,original);assert!(done.items[0].artifact.as_ref().unwrap().body.is_none());let artifact=t.batch_artifact(&a,&j.id,0,&j.company_ref).await.ok().unwrap();
   if op==Operation::Package{let mut normal=t.package(&r.id).await.ok().unwrap();normal["exported_at"]=json!(done.items[0].captured_at);assert_eq!(artifact.body.as_ref().unwrap(),&serde_json::to_string_pretty(&normal).unwrap());}else{assert_eq!(artifact.body.as_ref().unwrap(),&super::super::routes::print_workpaper(&r).ok().unwrap());assert!(!artifact.body.as_ref().unwrap().contains("<script>"));}
   assert_eq!(hash(&t.get(&r.id).await.ok().unwrap()).ok().unwrap(),before);assert!(t.batch_artifact(&actor("B"),&j.id,0,&j.company_ref).await.is_err());let m=t.batch_manifest(&a,&j.id,&j.company_ref).await.ok().unwrap();assert_eq!(m["manifest_sha256"],hash(&m["manifest"]).ok().unwrap());
  }
 }
 #[tokio::test]async fn crash_after_calculation_reconciles_same_request_without_duplicate_snapshot(){
  let store=Store::memory();let t=Tax::new(store.clone());let a=actor("A");let r=t.save(None,input("A","AB")).await.ok().unwrap();let j=t.batch_create(&a,create(&r,Operation::Calculate,"crash")).await.ok().unwrap();running(&t,&j,"lost-step").await;
  t.calculate(&r.id,RevisionInput{expected_revision:r.revision,idempotency_key:Some(j.items[0].calculation_key.clone())}).await.ok().unwrap();
  let restarted=Tax::new(store);let request=Step{expected_revision:j.revision,request_key:"lost-step".into(),company_ref:j.company_ref.clone()};let done=restarted.batch_step(&a,&j.id,request).await.ok().unwrap();assert_eq!(done.status,"completed");assert_eq!(done.items[0].outcome.as_ref().unwrap()["recovered_from_history"],true);
  restarted.batch_step(&a,&j.id,step(&done,"lost-step")).await.ok().unwrap();assert_eq!(restarted.get(&r.id).await.ok().unwrap().calculations.len(),1);
 }
 #[tokio::test]async fn stale_claim_cancel_and_unsupported_scope_never_start_new_calculations(){
  let t=Tax::new(Store::memory());let a=actor("A");let r=t.save(None,input("A","AB")).await.ok().unwrap();let j=t.batch_create(&a,create(&r,Operation::Calculate,"changed")).await.ok().unwrap();running(&t,&j,"lost").await;
  let mut edit=input("A","AB");edit.expected_revision=Some(r.revision);edit.notes="Explicit later edit".into();t.save(Some(&r.id),edit).await.ok().unwrap();let ready=t.batch_get(&a,&j.id,&j.company_ref).await.ok().unwrap();let held=t.batch_step(&a,&j.id,step(&ready,"resume")).await.ok().unwrap();assert_eq!(held.items[0].outcome.as_ref().unwrap()["code"],"stale_selection");assert!(t.get(&r.id).await.ok().unwrap().calculations.is_empty());
  let qc=t.save(None,input("A","QC")).await.ok().unwrap();let q=t.batch_create(&a,create(&qc,Operation::Calculate,"QC")).await.ok().unwrap();let done=t.batch_step(&a,&q.id,step(&q,"step-QC")).await.ok().unwrap();assert_eq!(done.items[0].outcome.as_ref().unwrap()["code"],"unsupported_scope");assert!(t.get(&qc.id).await.ok().unwrap().calculations.is_empty());
  let c=t.batch_create(&a,create(&qc,Operation::Calculate,"cancel")).await.ok().unwrap();let cancelled=t.batch_cancel(&a,&c.id,Cancel{expected_revision:c.revision,company_ref:c.company_ref.clone()}).await.ok().unwrap();t.batch_step(&a,&c.id,step(&cancelled,"no-work")).await.ok().unwrap();assert!(t.get(&qc.id).await.ok().unwrap().calculations.is_empty());assert_eq!(cancelled.items[0].state,ItemState::Cancelled);
 }
 #[tokio::test]async fn cancelled_inflight_recovery_records_committed_effect_but_does_not_replay(){
  let t=Tax::new(Store::memory());let a=actor("A");let r=t.save(None,input("A","AB")).await.ok().unwrap();let j=t.batch_create(&a,create(&r,Operation::Calculate,"late")).await.ok().unwrap();running(&t,&j,"inflight").await;
  t.calculate(&r.id,RevisionInput{expected_revision:r.revision,idempotency_key:Some(j.items[0].calculation_key.clone())}).await.ok().unwrap();let latest=t.batch_get(&a,&j.id,&j.company_ref).await.ok().unwrap();let cancel=t.batch_cancel(&a,&j.id,Cancel{expected_revision:latest.revision,company_ref:j.company_ref.clone()}).await.ok().unwrap();let done=t.batch_step(&a,&j.id,step(&cancel,"reconcile")).await.ok().unwrap();assert_eq!(done.status,"cancelled");assert_eq!(done.items[0].state,ItemState::Succeeded);assert_eq!(done.items[0].outcome.as_ref().unwrap()["recorded_after_cancel"],true);assert_eq!(t.get(&r.id).await.ok().unwrap().calculations.len(),1);
 }
 #[tokio::test]async fn all_item_scope_stale_duplicate_and_creation_replay_are_enforced(){
  let t=Tax::new(Store::memory());let a=actor("A");let r=t.save(None,input("A","AB")).await.ok().unwrap();let j=t.batch_create(&a,create(&r,Operation::Package,"one")).await.ok().unwrap();assert_eq!(t.batch_create(&a,create(&r,Operation::Package,"one")).await.ok().unwrap().id,j.id);assert!(t.batch_create(&a,create(&r,Operation::Print,"one")).await.is_err());
  let mut duplicate=create(&r,Operation::Calculate,"dup");duplicate.selections.push(duplicate.selections[0].clone());assert!(t.batch_create(&a,duplicate).await.is_err());let mut stale=create(&r,Operation::Calculate,"stale");stale.selections[0].expected_revision+=1;assert!(t.batch_create(&a,stale).await.is_err());let mut oversized=create(&r,Operation::Calculate,"oversized");oversized.selections=vec![oversized.selections[0].clone();26];assert!(t.batch_create(&a,oversized).await.is_err());
  assert!(t.batch_get(&actor("B"),&j.id,&j.company_ref).await.is_err());assert!(t.batch_manifest(&actor("B"),&j.id,&j.company_ref).await.is_err());assert!(t.batch_step(&actor("B"),&j.id,step(&j,"foreign")).await.is_err());assert!(t.batch_get(&a,&j.id,&None).await.is_err());assert!(t.batch_list(&actor("B"),Some("A".into())).await.ok().unwrap().is_empty());
  let mut changed=input("B","AB");changed.expected_revision=Some(r.revision);t.save(Some(&r.id),changed).await.ok().unwrap();assert!(t.batch_get(&Identity::break_glass(),&j.id,&j.company_ref).await.is_err());assert!(t.batch_step(&a,&j.id,step(&j,"retargeted")).await.is_err());
 }
 #[tokio::test]async fn archive_copy_retry_frees_active_slot_preserves_bytes_and_all_scope_guards(){
  let t=Tax::new(Store::memory());let a=actor("A");let r=t.save(None,input("A","AB")).await.ok().unwrap();let request=create(&r,Operation::Package,"archive");let j=t.batch_create(&a,request.clone()).await.ok().unwrap();let done=t.batch_step(&a,&j.id,step(&j,"one")).await.ok().unwrap();let body=t.batch_artifact(&a,&j.id,0,&j.company_ref).await.ok().unwrap();
  let s=t.load_state().await.ok().unwrap();let original=s.batches.iter().find(|j|j.id==done.id).unwrap();let raw=serde_json::to_string(original).unwrap();
  // Simulate a crash after immutable copy but before registry/CAS removal.
  assert!(t.store.set_nx(&archive_key(&done.id),&raw).await.unwrap());
  assert!(t.batch_archive(&actor("B"),&done.id,Cancel{expected_revision:done.revision,company_ref:done.company_ref.clone()}).await.is_err());
  let archived=t.batch_archive(&a,&done.id,Cancel{expected_revision:done.revision,company_ref:done.company_ref.clone()}).await.ok().unwrap();assert!(archived.archived);let s=t.load_state().await.ok().unwrap();assert!(s.batches.is_empty());assert_eq!(s.batch_archives.len(),1);assert_eq!(super::super::verify_audit(&s.audit)["valid"],true);
  assert_eq!(t.batch_artifact(&a,&j.id,0,&j.company_ref).await.ok().unwrap().body,body.body);assert_eq!(t.store.get(&archive_key(&done.id)).await.unwrap().unwrap(),raw);
  assert!(t.batch_create(&a,request).await.ok().unwrap().archived);assert!(t.batch_get(&actor("B"),&j.id,&j.company_ref).await.is_err());assert!(t.batch_manifest(&actor("B"),&j.id,&j.company_ref).await.is_err());assert!(t.batch_artifact(&actor("B"),&j.id,0,&j.company_ref).await.is_err());
  t.batch_archive(&a,&done.id,Cancel{expected_revision:done.revision,company_ref:done.company_ref.clone()}).await.ok().unwrap();assert_eq!(t.load_state().await.ok().unwrap().batch_archives.len(),1);
 }
}

async fn archives(State(t):S,Extension(a):Extension<Identity>,Query(q):Query<Scope>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_archive_list(&a,q.company_ref).await?)))}
async fn archive(State(t):S,Extension(a):Extension<Identity>,Path(k):Path<String>,Json(b):Json<Cancel>)->TResult<Json<Value>>{Ok(Json(json!(t.batch_archive(&a,&k,b).await?)))}
