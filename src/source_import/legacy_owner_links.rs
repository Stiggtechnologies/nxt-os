//! Source-preserving owner links for retired-ID T5008 CSV rows.
//! A link requires a unique registered recipient name and matching stored CRA
//! evidence. It never overwrites a source owner ID or creates another tax slip.
use super::{ImportError, SourceImport, bad, conflict, storage};
use super::preserved_records::SourceFactPage;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, ImportError>;
fn key(company:&str, rest:&str)->String {format!("dtmaxtc:{company}:{rest}")}
fn digest(v:&Value)->Result<String> {Ok(hex::encode(Sha256::digest(serde_json::to_vec(v).map_err(|_|storage("Cannot hash source row"))?)))}
fn sha(s:&str)->bool {s.len()==64 && s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
fn number(s:&str)->bool {!s.is_empty()&&s.len()<=12&&s.bytes().all(|b|b.is_ascii_digit())}
fn name(s:&str)->Vec<String> {let mut parts:Vec<_>=s.split(|c:char|!c.is_alphanumeric()).filter(|s|!s.is_empty()).map(str::to_lowercase).collect();parts.sort();parts}
fn legacy_id(s:&str)->bool {
    let p:Vec<_>=s.split(':').collect();
    matches!(p.as_slice(),[n,y,h,r] if number(n)&&y.parse::<u16>().is_ok_and(|v|(1990..=2035).contains(&v))&&sha(h)&&r.parse::<u32>().is_ok_and(|v|v>0&&v<=100_000))
}
// Exact decimal comparison without binary floats, rounding, or permissive comma
// removal. Malformed grouping is held rather than interpreted as another value.
fn decimal(s:&str)->Option<String> {
    let mut s=s.trim();if s.len()>128{return None}
    let negative=s.starts_with('-');if negative||s.starts_with('+'){s=&s[1..];}
    if let Some(rest)=s.strip_prefix('$'){s=rest;}
    let (whole,fraction)=s.split_once('.').unwrap_or((s,""));
    if fraction.len()>24||!fraction.bytes().all(|b|b.is_ascii_digit()){return None}
    let groups:Vec<_>=whole.split(',').collect();
    if groups.iter().any(|g|g.is_empty()||!g.bytes().all(|b|b.is_ascii_digit())) || groups.len()>1&&(groups[0].len()>3||groups[1..].iter().any(|g|g.len()!=3)){return None}
    let joined=groups.concat();let w=joined.trim_start_matches('0');let f=fraction.trim_end_matches('0');
    let w=if w.is_empty(){"0"}else{w};let sign=if negative&&(w!="0"||!f.is_empty()){"-"}else{""};
    Some(format!("{sign}{w}.{f}"))
}
fn normalized_box(box_number:&str,text:&str)->Result<String> {
    if ["16","19","20","21"].contains(&box_number) {decimal(text).map(|s|format!("n:{s}")).ok_or_else(||bad("Unreviewed source numeric representation"))}
    else {Ok(format!("s:{}",text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()))}
}
fn collect_boxes(tree:&Value, out:&mut BTreeMap<String,BTreeSet<String>>, depth:usize)->Result<()> {
    if depth>12{return Err(storage("CRA source tree exceeds depth bound"))}
    if let (Some(code),Some(text))=(tree["attributes"]["boxNumber"].as_str(),tree["text"].as_str()) {
        if !text.trim().is_empty(){let code=code.parse::<u16>().map(|n|n.to_string()).unwrap_or_else(|_|code.into());out.entry(code.clone()).or_default().insert(normalized_box(&code,text)?);}
    }
    for child in tree["children"].as_array().ok_or_else(||storage("Malformed CRA source children"))? {collect_boxes(child,out,depth+1)?;}
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CraOwnerEvidence {pub archive_sha256:String,pub source_occurrence:u32,pub slip_content_sha256:String}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LegacyOwnerLink {
    pub legacy_record_id:String,pub legacy_record_sha256:String,pub owner_client_number:String,
    pub owner_name_sha256:String,pub evidence_mode:String,
    pub matching_cra_records:Vec<CraOwnerEvidence>,pub supporting_legacy_record_ids:Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyOwnerBatch {pub test_copy:bool,pub records:Vec<LegacyOwnerLink>}

impl SourceImport {
    async fn legacy_owner_source(&self,company:&str,id:&str)->Result<Value> {
        if !legacy_id(id){return Err(bad("Invalid legacy T5008 identity"))}
        let raw=self.store.get(&key(company,&format!("t5008:{id}"))).await.map_err(|e|storage(&e))?.ok_or_else(||conflict("Original legacy source row is missing"))?;
        let row:Value=serde_json::from_str(&raw).map_err(|_|storage("Invalid legacy source body"))?;
        if format!("{}:{}:{}:{}",row["client_number"].as_str().unwrap_or(""),row["tax_year"],row["source_file_sha256"].as_str().unwrap_or(""),row["row"])!=id{return Err(storage("Legacy source body identity mismatch"))}
        Ok(row)
    }
    pub async fn test_copy_legacy_owner_links(&self,company:&str,batch:LegacyOwnerBatch)->Result<Value> {
        self.test_copy_guard(company,batch.test_copy)?;
        if batch.records.is_empty()||batch.records.len()>50{return Err(bad("At most 50 owner links per batch"))}
        let ids=self.store.smembers(&key(company,"clients")).await.map_err(|e|storage(&e))?;
        if ids.len()>100_000{return Err(storage("Client roster exceeds bound"))}
        let registered:BTreeSet<_>=ids.iter().cloned().collect();let mut names:BTreeMap<Vec<String>,Vec<String>>=BTreeMap::new();let mut profiles=BTreeMap::new();
        for chunk in ids.chunks(500) {
            let keys:Vec<_>=chunk.iter().map(|id|key(company,&format!("client:{id}"))).collect();
            for (id,raw) in chunk.iter().zip(self.store.mget(&keys).await.map_err(|e|storage(&e))?) {
                let row:Value=serde_json::from_str(&raw.ok_or_else(||storage("Registered profile body is missing"))?).map_err(|_|storage("Unreadable registered profile"))?;
                let text=row["name"].as_str().ok_or_else(||storage("Registered name is missing"))?;
                names.entry(name(text)).or_default().push(id.clone());profiles.insert(id.clone(),text.to_string());
            }
        }
        let mut seen=BTreeSet::new();let mut prepared=Vec::new();
        // Validate the complete batch before any new link body is claimed.
        for link in &batch.records {
            if !seen.insert(&link.legacy_record_id)||!number(&link.owner_client_number)||!sha(&link.legacy_record_sha256)||!sha(&link.owner_name_sha256)||link.matching_cra_records.is_empty()||link.matching_cra_records.len()>100||link.supporting_legacy_record_ids.len()>100{return Err(bad("Invalid owner evidence bounds"))}
            let source=self.legacy_owner_source(company,&link.legacy_record_id).await?;
            if digest(&source)?!=link.legacy_record_sha256{return Err(conflict("Original legacy row hash changed"))}
            if registered.contains(source["client_number"].as_str().unwrap_or("")){return Err(conflict("Source owner is already registered"))}
            let owner_name=profiles.get(&link.owner_client_number).ok_or_else(||conflict("Owner is not registered"))?;
            if hex::encode(Sha256::digest(owner_name.as_bytes()))!=link.owner_name_sha256{return Err(conflict("Registered owner name changed"))}
            let recipient=source["recipient_name"].as_str().ok_or_else(||storage("Legacy recipient name missing"))?;let tokens=name(recipient);
            if tokens.len()<2||recipient.to_lowercase().starts_with("dt max client")||names.get(&tokens)!=Some(&vec![link.owner_client_number.clone()]){return Err(conflict("Recipient name is not uniquely registered to this owner"))}
            let source_boxes=source["boxes"].as_object().ok_or_else(||storage("Legacy source boxes missing"))?;
            let mut required=BTreeMap::new();for (code,value) in source_boxes {let text=value.as_str().ok_or_else(||storage("Source box must be text"))?;if !text.trim().is_empty(){required.insert(code.clone(),normalized_box(code,text)?);}}
            if required.is_empty(){return Err(conflict("No populated source boxes to corroborate"))}
            if link.evidence_mode=="recipient_name_and_all_boxes" {
                if !link.supporting_legacy_record_ids.is_empty()||required.len()<3||!["20","21"].iter().any(|c|required.contains_key(*c))||!["17","18"].iter().any(|c|required.contains_key(*c)){return Err(conflict("Insufficient independent source fingerprint"))}
            } else if link.evidence_mode=="same_recipient_file_and_all_boxes" {
                if link.supporting_legacy_record_ids.is_empty(){return Err(conflict("Same-file support is required"))}
                let mut supports=BTreeSet::new();
                for id in &link.supporting_legacy_record_ids {
                    if id==&link.legacy_record_id||!supports.insert(id){return Err(bad("Invalid supporting source identities"))}
                    let raw=self.store.get(&key(company,&format!("legacy_owner_link:{id}"))).await.map_err(|e|storage(&e))?.ok_or_else(||conflict("Supporting owner link is not verified yet"))?;
                    let support:LegacyOwnerLink=serde_json::from_str(&raw).map_err(|_|storage("Unreadable supporting owner link"))?;
                    let other=self.legacy_owner_source(company,id).await?;
                    if support.legacy_record_id!=*id||support.owner_client_number!=link.owner_client_number||support.evidence_mode!="recipient_name_and_all_boxes"||digest(&other)?!=support.legacy_record_sha256||other["source_file_sha256"]!=source["source_file_sha256"]||other["tax_year"]!=source["tax_year"]||name(other["recipient_name"].as_str().unwrap_or(""))!=tokens{return Err(conflict("Supporting row is not the same recipient file and year"))}
                }
            } else {return Err(bad("Unsupported owner evidence mode"))}
            let mut refs=BTreeSet::new();
            for reference in &link.matching_cra_records {
                if !sha(&reference.archive_sha256)||!sha(&reference.slip_content_sha256)||reference.source_occurrence==0||!refs.insert((&reference.archive_sha256,reference.source_occurrence)){return Err(bad("Invalid CRA evidence identity"))}
                let raw=self.store.get(&key(company,&format!("afr_occurrence:{}:{}",reference.archive_sha256,reference.source_occurrence))).await.map_err(|e|storage(&e))?.ok_or_else(||conflict("Stored CRA corroboration is missing"))?;
                let cra:Value=serde_json::from_str(&raw).map_err(|_|storage("Unreadable CRA evidence"))?;
                if cra["client_number"]!=link.owner_client_number||cra["tax_year"]!=source["tax_year"]||cra["slip_kind"]!="T5008Slip"||cra["archive_sha256"]!=reference.archive_sha256||cra["source_occurrence"]!=reference.source_occurrence||cra["slip_content_sha256"]!=reference.slip_content_sha256||digest(&cra["vendor_field_tree"])?!=reference.slip_content_sha256{return Err(conflict("CRA evidence belongs to another owner, year or content"))}
                let mut boxes=BTreeMap::new();collect_boxes(&cra["vendor_field_tree"],&mut boxes,0)?;
                if required.iter().any(|(code,value)|boxes.get(code)!=Some(&BTreeSet::from([value.clone()]))){return Err(conflict("A populated legacy source box does not match CRA evidence"))}
            }
            let body=serde_json::to_string(link).map_err(|_|storage("Cannot encode owner evidence"))?;
            let k=key(company,&format!("legacy_owner_link:{}",link.legacy_record_id));
            if let Some(old)=self.store.get(&k).await.map_err(|e|storage(&e))? {if old!=body{return Err(conflict("Immutable owner evidence conflicts"))}}
            prepared.push((link,k,body));
        }
        let (mut created,mut replays)=(0,0);
        for (link,k,body) in prepared {
            if self.store.set_nx(&k,&body).await.map_err(|e|storage(&e))? {created+=1;}
            else if self.store.get(&k).await.map_err(|e|storage(&e))?.as_deref()==Some(body.as_str()){replays+=1;}
            else{return Err(conflict("Immutable owner evidence conflicts"))}
            self.store.sadd(&key(company,"legacy_owner_links"),&link.legacy_record_id).await.map_err(|e|storage(&e))?;
            self.store.sadd(&key(company,&format!("legacy_owner_links:client:{}",link.owner_client_number)),&link.legacy_record_id).await.map_err(|e|storage(&e))?;
        }
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":replays,"source_rows_modified":0,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
    pub async fn test_copy_legacy_owner_summary(&self,company:&str)->Result<Value> {
        self.test_copy_guard(company,true)?;
        let clients:BTreeSet<_>=self.store.smembers(&key(company,"clients")).await.map_err(|e|storage(&e))?.into_iter().collect();
        let sources=self.store.smembers(&key(company,"t5008")).await.map_err(|e|storage(&e))?;
        let unlinked:BTreeSet<_>=sources.into_iter().filter(|id|!clients.contains(id.split(':').next().unwrap_or(""))).collect();
        let links=self.store.smembers(&key(company,"legacy_owner_links")).await.map_err(|e|storage(&e))?;
        if links.len()>100_000{return Err(storage("Owner link index exceeds bound"))}
        let mut resolved=BTreeSet::new();
        for id in links {
            let raw=self.store.get(&key(company,&format!("legacy_owner_link:{id}"))).await.map_err(|e|storage(&e))?.ok_or_else(||storage("Owner link index body missing"))?;
            let link:LegacyOwnerLink=serde_json::from_str(&raw).map_err(|_|storage("Unreadable owner link"))?;
            if link.legacy_record_id!=id||!clients.contains(&link.owner_client_number)||!unlinked.contains(&id)||digest(&self.legacy_owner_source(company,&id).await?)?!=link.legacy_record_sha256{return Err(storage("Owner link no longer matches source identity"))}
            resolved.insert(id);
        }
        Ok(json!({"source_scope":company,"test_copy":true,"raw_legacy_CSV_owner_ids_unregistered":unlinked.len(),"verified_owner_links":resolved.len(),"remaining_owner_exceptions":unlinked.difference(&resolved).count(),"source_rows_modified":0,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
    pub async fn test_copy_client_legacy_slips(&self,company:&str,client:&str,page:SourceFactPage)->Result<Value> {
        self.test_copy_guard(company,true)?;
        if !number(client)||page.limit==0||page.limit>100||page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)){return Err(bad("Invalid linked CSV page"))}
        if self.store.get(&key(company,&format!("client:{client}"))).await.map_err(|e|storage(&e))?.is_none(){return Err(conflict("Client not registered"))}
        let mut ids=self.store.smembers(&key(company,&format!("legacy_owner_links:client:{client}"))).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>10_000{return Err(storage("Linked CSV page exceeds bound"))}
        let mut rows=Vec::new();
        for id in ids {
            let raw=self.store.get(&key(company,&format!("legacy_owner_link:{id}"))).await.map_err(|e|storage(&e))?.ok_or_else(||storage("Linked CSV evidence missing"))?;
            let link:LegacyOwnerLink=serde_json::from_str(&raw).map_err(|_|storage("Unreadable linked CSV evidence"))?;
            let source=self.legacy_owner_source(company,&id).await?;
            if link.legacy_record_id!=id||link.owner_client_number!=client||digest(&source)?!=link.legacy_record_sha256{return Err(storage("Linked CSV belongs to another source or client"))}
            if page.source_return_year.is_some_and(|y|source["tax_year"]!=y){continue}
            rows.push(json!({"client_number":client,"tax_year":source["tax_year"],"original_source_record":source,"owner_link":link,"source_owner_id_changed":false,"normalized_tax_slip_created":false,"economic_slip_uniqueness_verified":false,"financial_values_approved":false}));
        }
        let total=rows.len();let selected:Vec<_>=rows.into_iter().skip(page.offset).take(page.limit).collect();
        Ok(json!({"source_scope":company,"test_copy":true,"client_number":client,"kind":"linked-t5008","source_return_year":page.source_return_year,"offset":page.offset,"limit":page.limit,"total":total,"records":selected,"source_rows_modified":0,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
}
