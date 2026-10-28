//! Nonposting preserved QBO customer/supplier lists. Not operational identities.
pub mod models;
mod validation;
pub mod routes;
use crate::{migrations::Migrations,referral::routes::ApiError,store::Store};
use axum::http::StatusCode;
use models::*;
use serde::{Serialize,de::DeserializeOwned};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{collections::HashSet,sync::Arc};
pub type Result<T>=std::result::Result<T,ApiError>;
const PART_MAX:usize=512*1024;
const BOOK_MAX:usize=24*1024*1024;
const META_MAX:usize=128*1024;
const COMPANY_MAX:usize=32*1024*1024;
const PART_SLOTS:usize=256;
const FAMILIES:usize=64;
const PENDING:usize=8;
pub(super) fn bad(s:&str)->ApiError {ApiError(StatusCode::BAD_REQUEST,s.into())}
fn conflict(s:&str)->ApiError {ApiError(StatusCode::CONFLICT,s.into())}
fn fault()->ApiError {ApiError(StatusCode::INTERNAL_SERVER_ERROR,"Source contact integrity failed; no partial records returned".into())}
fn missing()->ApiError {ApiError(StatusCode::NOT_FOUND,"Published source contact family not found for this company".into())}
fn storage(_:String)->ApiError {fault()}
fn digest(s:&str)->bool {s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
fn id(s:&str)->bool {!s.is_empty()&&s.len()<=96&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.-".contains(&b))}
fn text(s:&str,n:usize)->bool {!s.trim().is_empty()&&s.len()<=n&&!s.contains('\0')}
fn hash(b:&[u8])->String {hex::encode(Sha256::digest(b))}
fn encoded<T:Serialize>(v:&T)->Result<String> {serde_json::to_value(v).and_then(|v|serde_json::to_string(&v)).map_err(|_|bad("Cannot encode source contact metadata"))}
fn boundary()->Value {json!({"read_only":true,"source_authoritative":true,"live_cutover_allowed":false,"operational_objects_created":false,"payroll_migrated":false,"identity_limit":"Archive-row aliases are not durable source identities; existing shared workspace access is not individual company-role authorization.","equality_basis":"Operator-provided original-XLS equality proof; server verifies submitted fields and published archive binding, not XLS contents."})}
fn head(s:&State)->Result<Option<String>> {Ok(if s.published.is_empty(){None}else{Some(hash(encoded(&s.published)?.as_bytes()))})}
pub struct ContactLists {
    pub store:Store,migrations:Arc<Migrations>,
    #[cfg(test)] pub fail_stage_once:std::sync::atomic::AtomicBool,
    #[cfg(test)] pub fail_publish_once:std::sync::atomic::AtomicBool,
}
impl ContactLists {
    pub fn new(store:Store,migrations:Arc<Migrations>)->Self {Self {store,migrations,
        #[cfg(test)] fail_stage_once:std::sync::atomic::AtomicBool::new(false),
        #[cfg(test)] fail_publish_once:std::sync::atomic::AtomicBool::new(false)}}
    fn key(c:&str,k:&str,h:&str)->Result<String> {if !id(c){return Err(bad("Invalid company reference"))}Ok(format!("company-contact-lists-v2:{c}:{k}:{h}"))}
    async fn state(&self,c:&str)->Result<(Option<String>,State)> {
        let raw=self.store.get(&Self::key(c,"state","v1")?).await.map_err(storage)?;
        let Some(v)=raw.as_ref() else{return Ok((None,State::default()))};
        if v.len()>META_MAX{return Err(fault())}
        let s:State=serde_json::from_str(v).map_err(|_|fault())?;
        if encoded(&s)?!=*v||s.parts.len()>PART_SLOTS||s.slots.len()>FAMILIES+PENDING||s.published.len()>FAMILIES{return Err(fault())}
        let mut parts=HashSet::new();let mut requests=HashSet::new();let mut receipts=HashSet::new();let mut published=HashSet::new();let mut charged=0usize;
        for p in &s.parts {if !digest(&p.sha256)||p.bytes==0||p.bytes>PART_MAX||!parts.insert(&p.sha256){return Err(fault())}charged=charged.checked_add(p.bytes).ok_or_else(fault)?;}
        for p in &s.slots {
            if !id(&p.request_id)||!digest(&p.command_sha256)||!digest(&p.receipt_sha256)||!digest(&p.family_sha256)||p.receipt_bytes==0||p.receipt_bytes>META_MAX
                ||p.records_total>20000||p.nonrecord_rows_total>2048||!requests.insert(&p.request_id)||!receipts.insert(&p.receipt_sha256){return Err(fault())}
            charged=charged.checked_add(p.receipt_bytes).ok_or_else(fault)?;
            if p.published {published.insert(p.receipt_sha256.as_str());}
        }
        let listed=s.published.iter().map(String::as_str).collect::<HashSet<_>>();
        if listed.len()!=s.published.len()||listed!=published||s.slots.iter().filter(|p|!p.published).count()>PENDING||charged!=s.charged_bytes||charged>COMPANY_MAX{return Err(fault())}
        Ok((raw,s))
    }
    async fn cas(&self,c:&str,raw:Option<&str>,s:&State)->Result<bool> {
        let next=encoded(s)?;if next.len()>META_MAX{return Err(conflict("Contact metadata capacity reached"))}
        self.store.compare_exchange(&Self::key(c,"state","v1")?,raw,&next).await.map_err(storage)
    }
    async fn immutable<T:Serialize>(&self,c:&str,k:&str,h:&str,v:&T)->Result<()> {
        let raw=encoded(v)?;let key=Self::key(c,k,h)?;
        if hash(raw.as_bytes())!=h{return Err(fault())}
        self.store.set_nx(&key,&raw).await.map_err(storage)?;
        if self.store.get(&key).await.map_err(storage)?.as_deref()!=Some(&raw){return Err(fault())}Ok(())
    }
    async fn object<T:Serialize+DeserializeOwned>(&self,c:&str,k:&str,h:&str,max:usize)->Result<T> {
        if !digest(h){return Err(bad("Invalid source contact object hash"))}
        let raw=self.store.get(&Self::key(c,k,h)?).await.map_err(storage)?.ok_or_else(fault)?;
        if raw.len()>max||hash(raw.as_bytes())!=h{return Err(fault())}
        let v:T=serde_json::from_str(&raw).map_err(|_|fault())?;if encoded(&v)?!=raw{return Err(fault())}Ok(v)
    }
    pub async fn stage(&self,c:&str,p:Part)->Result<Value> {
        validation::part(c,&p)?;let raw=encoded(&p)?;let h=hash(raw.as_bytes());let bytes=raw.len();let mut reserved=false;
        for _ in 0..4 {
            let (before,mut s)=self.state(c).await?;
            if let Some(prior)=s.parts.iter().find(|r|r.sha256==h) {if prior.bytes!=bytes{return Err(fault())}reserved=true;break}
            if s.parts.len()>=PART_SLOTS||s.charged_bytes.checked_add(bytes).is_none_or(|n|n>COMPANY_MAX){return Err(conflict("Contact staging budget exhausted; no eviction"))}
            s.parts.push(Reservation{sha256:h.clone(),bytes});s.charged_bytes+=bytes;
            if self.cas(c,before.as_deref(),&s).await?{reserved=true;break}
        }
        if !reserved{return Err(conflict("Contact staging reservation raced; retry same part"))}
        #[cfg(test)] if self.fail_stage_once.swap(false,std::sync::atomic::Ordering::SeqCst){return Err(fault())}
        self.immutable(c,"part",&h,&p).await?;
        Ok(json!({"sha256":h,"bytes":bytes,"records":p.records.len(),"other_rows":p.other_source_rows.len(),"controls":boundary()}))
    }
    async fn bind(&self,c:&str,f:&Family)->Result<()> {
        let d=self.migrations.detail(&f.source_snapshot_id).await?;let m=&d["manifest"];
        if d["manifest_sha256"]!=f.source_manifest_sha256||m["cohort_ref"]!=c||m["source_archive"]["sha256"]!=f.source.archive_sha256
            ||m["source_archive"]["bytes"]!=f.source.archive_bytes||m["source_of_truth"]!="original_system"||m["mode"]!="parallel_test" {return Err(conflict("Published source Snapshot company/archive/canonical manifest differs"))}Ok(())
    }
    async fn part_at(&self,c:&str,s:&State,r:&PartRef,f:&Family)->Result<Part> {
        if !s.parts.iter().any(|p|p.sha256==r.sha256&&p.bytes==r.bytes){return Err(fault())}
        let p:Part=self.object(c,"part",&r.sha256,PART_MAX).await?;validation::part(c,&p).map_err(|_|fault())?;
        if p.kind!=f.kind||p.source!=f.source||encoded(&p)?.len()!=r.bytes||p.records.len()!=r.records||p.other_source_rows.len()!=r.other_rows{return Err(fault())}Ok(p)
    }
    async fn full_graph(&self,c:&str,s:&State,f:&Family)->Result<()> {
        validation::family(c,f)?;let mut seen=HashSet::new();let mut last=0;
        for r in &f.parts {
            let p=self.part_at(c,s,r,f).await?;
            for row in p.records {
                if !seen.insert(row.source_row)||row.source_row<=last{return Err(bad("Repeated or reordered original contact rows"))}last=row.source_row;
            }
            for row in p.other_source_rows {if !seen.insert(row.source_row){return Err(bad("Source nonrecord rows overlap"))}}
        }Ok(())
    }
    pub async fn publish(&self,c:&str,p:Publish)->Result<Value> {
        validation::command(c,&p)?;let command_sha=hash(encoded(&p)?.as_bytes());
        let receipt=Receipt{format:"nxt-company-contact-publication/v1".into(),company_id:c.into(),command:p.clone(),command_sha256:command_sha.clone()};
        let receipt_raw=encoded(&receipt)?;let receipt_sha=hash(receipt_raw.as_bytes());let family_sha=hash(encoded(&p.family)?.as_bytes());
        if receipt_raw.len()>META_MAX{return Err(bad("Contact publication metadata exceeds bound"))}
        let mut reserved=false;
        for _ in 0..4 {
            let (before,mut s)=self.state(c).await?;
            if let Some(slot)=s.slots.iter().find(|r|r.request_id==p.request_id) {
                if slot.command_sha256!=command_sha||slot.receipt_sha256!=receipt_sha{return Err(conflict("Request ID already binds another immutable contact command"))}
                if slot.published {self.receipt(c,&s,&receipt_sha).await?;return Ok(json!({"capture_sha256":receipt_sha,"index_head_sha256":head(&s)?,"idempotent_replay":true,"controls":boundary()}))}
                if head(&s)?!=p.expected_index_head{return Err(conflict("Pending contact publication has stale index; reservation retained"))}
                self.bind(c,&p.family).await?;self.full_graph(c,&s,&p.family).await?;reserved=true;break
            }
            if head(&s)?!=p.expected_index_head{return Err(conflict("Contact index changed; refresh before publication"))}
            if s.slots.iter().any(|r|r.published&&r.family_sha256==family_sha){return Err(conflict("This exact contact family is already published; use original request receipt"))}
            if s.slots.len()>=FAMILIES+PENDING||s.published.len()>=FAMILIES||s.slots.iter().filter(|r|!r.published).count()>=PENDING
                ||s.charged_bytes.checked_add(receipt_raw.len()).is_none_or(|n|n>COMPANY_MAX){return Err(conflict("Contact publication budget exhausted; reservations retained"))}
            self.bind(c,&p.family).await?;self.full_graph(c,&s,&p.family).await?;
            s.slots.push(Slot{request_id:p.request_id.clone(),command_sha256:command_sha.clone(),receipt_sha256:receipt_sha.clone(),receipt_bytes:receipt_raw.len(),family_sha256:family_sha.clone(),kind:p.family.kind.clone(),records_total:p.family.records_total,nonrecord_rows_total:p.family.nonrecord_rows_total,published:false});
            s.charged_bytes+=receipt_raw.len();
            if self.cas(c,before.as_deref(),&s).await?{reserved=true;break}
        }
        if !reserved{return Err(conflict("Contact reservation raced; retry exact request"))}
        self.immutable(c,"receipt",&receipt_sha,&receipt).await?;
        #[cfg(test)] if self.fail_publish_once.swap(false,std::sync::atomic::Ordering::SeqCst){return Err(fault())}
        #[cfg(test)] tokio::task::yield_now().await;
        for _ in 0..4 {
            let (before,mut s)=self.state(c).await?;
            let index=s.slots.iter().position(|r|r.request_id==p.request_id).ok_or_else(fault)?;
            if s.slots[index].command_sha256!=command_sha||s.slots[index].receipt_sha256!=receipt_sha{return Err(fault())}
            if s.slots[index].published{return Ok(json!({"capture_sha256":receipt_sha,"index_head_sha256":head(&s)?,"idempotent_replay":true,"controls":boundary()}))}
            if head(&s)?!=p.expected_index_head{return Err(conflict("Contact publication lost CAS; invisible charged reservation retained"))}
            s.slots[index].published=true;s.published.push(receipt_sha.clone());
            if self.cas(c,before.as_deref(),&s).await? {return Ok(json!({"capture_sha256":receipt_sha,"index_head_sha256":head(&s)?,"idempotent_replay":false,"controls":boundary()}))}
        }Err(conflict("Contact publication CAS raced; exact retry recovers"))
    }
    async fn receipt(&self,c:&str,s:&State,h:&str)->Result<Receipt> {
        if !s.published.iter().any(|id|id==h){return Err(missing())}
        let slot=s.slots.iter().find(|r|r.receipt_sha256==h&&r.published).ok_or_else(fault)?;
        let r:Receipt=self.object(c,"receipt",h,META_MAX).await?;
        if r.format!="nxt-company-contact-publication/v1"||r.company_id!=c||hash(encoded(&r.command)?.as_bytes())!=r.command_sha256||r.command_sha256!=slot.command_sha256
            ||r.command.request_id!=slot.request_id||hash(encoded(&r.command.family)?.as_bytes())!=slot.family_sha256
            ||r.command.family.kind!=slot.kind||r.command.family.records_total!=slot.records_total||r.command.family.nonrecord_rows_total!=slot.nonrecord_rows_total
            ||encoded(&r)?.len()!=slot.receipt_bytes {return Err(fault())}
        validation::command(c,&r.command).map_err(|_|fault())?;self.bind(c,&r.command.family).await?;Ok(r)
    }
    pub async fn list(&self,c:&str)->Result<Value> {
        let (_,s)=self.state(c).await?;
        let captures=s.published.iter().map(|h|{let p=s.slots.iter().find(|p|p.receipt_sha256==*h).expect("validated slot");json!({"capture_sha256":h,"family_sha256":p.family_sha256,"record_type":p.kind,"records_total":p.records_total,"nonrecord_rows_total":p.nonrecord_rows_total})}).collect::<Vec<_>>();
        Ok(json!({"company_id":c,"index_head_sha256":head(&s)?,"captures":captures,"integrity_scope":"bounded_index_metadata_only","charged_bytes":s.charged_bytes,"pending_publications":s.slots.iter().filter(|p|!p.published).count(),"source_roster_complete":false,"controls":boundary()}))
    }
    pub async fn detail(&self,c:&str,h:&str)->Result<Value> {
        let (_,s)=self.state(c).await?;let r=self.receipt(c,&s,h).await?;
        Ok(json!({"company_id":c,"capture_sha256":h,"receipt":r,"integrity_scope":"publication_manifest_and_source_snapshot","controls":boundary()}))
    }
    pub async fn rows(&self,c:&str,kind:Kind,h:&str,offset:usize,limit:usize,max_bytes:usize,other:bool)->Result<Value> {
        if !(1..=200).contains(&limit)||!(16384..=786432).contains(&max_bytes){return Err(bad("Contact page limit/byte budget invalid"))}
        let (_,s)=self.state(c).await?;let r=self.receipt(c,&s,h).await?;let f=&r.command.family;
        if f.kind!=kind{return Err(missing())}
        let total=if other{f.nonrecord_rows_total}else{f.records_total};if offset>total{return Err(bad("Contact page offset beyond source records"))}
        let mut output=vec![];let mut position=0;let mut reads=0;
        for reference in &f.parts {
            let count=if other{reference.other_rows}else{reference.records};
            if position+count<=offset {position+=count;continue}
            if reads>=4||output.len()>=limit{break}
            let part=self.part_at(c,&s,reference,f).await?;reads+=1;
            let rows=if other{part.other_source_rows.into_iter().map(|r|serde_json::to_value(r).map_err(|_|fault())).collect::<Result<Vec<_>>>()?}else{part.records.into_iter().map(|r|serde_json::to_value(r).map_err(|_|fault())).collect::<Result<Vec<_>>>()?};
            for (i,row) in rows.into_iter().enumerate() {
                if position+i<offset{continue}if output.len()>=limit{break}output.push(row);
                let response=page(c,h,&kind,offset,total,max_bytes,&output,reads,other);
                if encoded(&response)?.len()>max_bytes {output.pop();if output.is_empty(){return Err(bad("Original contact row cannot fit requested byte budget"))}return Ok(page(c,h,&kind,offset,total,max_bytes,&output,reads,other))}
            }
            position+=count;
        }
        Ok(page(c,h,&kind,offset,total,max_bytes,&output,reads,other))
    }
}
fn page(c:&str,h:&str,k:&Kind,offset:usize,total:usize,budget:usize,rows:&[Value],reads:usize,other:bool)->Value {
    let next=offset+rows.len();json!({"company_id":c,"capture_sha256":h,"record_type":k,"offset":offset,"max_bytes":budget,"total":total,"records":rows,"next_offset":if next<total{Some(next)}else{None},"has_more":next<total,"row_set":if other{"other_source_rows"}else{"records"},"parts_checked":reads,"integrity_scope":"selected_parts_and_publication_manifest","source_roster_complete":false,"controls":boundary()})
}
