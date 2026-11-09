//! Optional source-only identity addendum. Original contexts and indexes are immutable.
use super::{ImportError, SourceImport, bad, conflict, storage};
use super::recovered_native_sources::{RecoveredNativeLookup, RecoveredNativePage};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, ImportError>;
fn digest(v: &Value)->String { hex(&serde_json::to_vec(v).unwrap()) }
fn hex(v: &[u8])->String {Sha256::digest(v).iter().map(|b|format!("{b:02x}")).collect()}
fn profile()->&'static Value {static P:std::sync::OnceLock<Value>=std::sync::OnceLock::new();P.get_or_init(||serde_json::from_str(include_str!("recovered_native_identity_link_profile.json")).unwrap())}
fn text<'a>(v: &'a Value,k: &str)->Result<&'a str>{v[k].as_str().ok_or_else(||bad("Identity proof string missing"))}
fn key(c: &str,s: &str)->String {format!("dtmaxtc:{c}:recovered_native_identity_links:{}:{s}",profile()["profile_sha256"].as_str().unwrap())}
fn base(c: &str,s: &str)->String {format!("dtmaxtc:{c}:{s}")}
fn envelope(c: &str)->Value {json!({"test_copy":true,"source_scope":c,"kind":"recovered-native-effective-sources","identity_link_profile_sha256":profile()["profile_sha256"],"recovered_native_source_profile_sha256":profile()["contract"]["original_recovered_source_profile_sha256"],"original_packets_modified":0,"original_queue_modified":0,"current_profiles_modified":0,"tax_cases_imported":0,"native_contexts_created":0,"earlier_figures_or_slips_modified":0,"vendor_activation_required":false,"financial_values_approved":false})}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityLinkBatch {pub test_copy:bool,pub profile_sha256:String,pub records:Vec<Value>}
fn validate_identity_profile(p:&Value)->Result<()>{
    if p["schema"]!="recovered-native-identity-link-closed-profile/v1"||digest(&p["contract"])!=p["profile_sha256"]{return Err(storage("Identity closed profile contract differs"));}
    for k in ["link_record_sha256","original_null_context_sha256"]{
        let rows=p[k].as_array().ok_or_else(||storage("Identity profile hash list missing"))?;
        if p[k]!=p["contract"][k]||rows.len()!=16||rows.iter().any(|v|v.as_str().is_none_or(|s|s.len()!=64||!s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))))||rows.iter().map(Value::to_string).collect::<BTreeSet<_>>().len()!=16{return Err(storage("Identity profile hash inventory differs"));}
    }
    if p["expected_counts"]!=p["contract"]["expected_counts"]{return Err(storage("Identity profile counts differ"));}Ok(())
}
fn validate_closed_link(l: &Value)->Result<()> {
    let p=profile();validate_identity_profile(p)?;
    if !p["contract"]["link_record_sha256"].as_array().unwrap().iter().any(|h|h==&digest(l))||!p["contract"]["original_null_context_sha256"].as_array().unwrap().contains(&l["original_context_sha256"]) {
        return Err(bad("Identity link is outside the closed independent addendum"));
    }
    if l["source_only"]!=true || l["original_context_client_number"]!=Value::Null || l["original_packet_modified"]!=false || l["original_queue_modified"]!=false || l["current_profiles_modified"]!=0 || l["tax_cases_imported"]!=0 || l["financial_values_approved"]!=false || l["legal_name_change_inferred"]!=false {
        return Err(bad("Identity link exceeds source-only scope"));
    } Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use super::super::recovered_native_sources::RecoveredNativeBatch;
    fn page()->RecoveredNativePage{RecoveredNativePage{offset:0,limit:1,source_return_year:None,_routing_path:None}}
    #[test]
    fn public_profile_is_hash_only_and_closed(){
        let p=profile();assert_eq!(digest(&p["contract"]),p["profile_sha256"]);
        assert_eq!(p["link_record_sha256"].as_array().unwrap().len(),16);
        assert_eq!(p["original_null_context_sha256"].as_array().unwrap().len(),16);
        assert_eq!(p["expected_counts"]["effective_assigned_contexts"],54);
        assert_eq!(p["expected_counts"]["effective_unassigned_contexts"],26);
        assert!(validate_closed_link(&json!({"context_id":"synthetic"})).is_err());
        for k in ["link_record_sha256","original_null_context_sha256","expected_counts"]{let mut changed=p.clone();changed[k]=Value::Null;assert!(validate_identity_profile(&changed).is_err());}
    }
    #[tokio::test]
    async fn empty_overlay_does_not_claim_full_import_and_rejects_other_scope(){
        let s=SourceImport::new(Store::memory());let summary=s.test_copy_effective_recovered_summary("dtmax-test-copy").await.unwrap();
        assert_eq!(summary["identity_links_received"],0);assert_eq!(summary["assigned_contexts"],0);assert_eq!(summary["identity_links_complete"],false);
        assert!(s.test_copy_effective_recovered_summary("other-test-copy").await.is_err());
        assert!(s.test_copy_effective_recovered_page("dtmax-test-copy",None,RecoveredNativePage{limit:2,..page()}).await.is_err());
    }
    #[tokio::test]
    async fn private_all_16_links_preserve_80_originals_and_raw_queue(){
        let Ok(directory)=std::env::var("NXT_IDENTITY_ORIGINAL_DIR") else{return};
        let root=std::path::Path::new(&directory);let company="dtmax-test-copy";let store=Store::memory();let s=SourceImport::new(store.clone());
        let f3:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();
        assert_eq!(digest(&f3["clients"]),profile()["contract"]["required_F3_canonical_sha256"]);
        let mut before=vec![];for row in f3["clients"].as_array().unwrap(){let n=text(row,"client_number").unwrap();let k=base(company,&format!("client:{n}"));let raw=row.to_string();store.set(&k,&raw).await.unwrap();store.sadd(&base(company,"clients"),n).await.unwrap();before.push((k,raw));}
        let read=|p:std::path::PathBuf|->RecoveredNativeBatch{serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()};
        s.test_copy_recovered_native_sources(company,read(root.join("requests/supporting-evidence.private.json"))).await.unwrap();
        let mut originals=vec![];for file in std::fs::read_dir(root.join("contexts")).unwrap(){let c:Value=serde_json::from_slice(&std::fs::read(file.unwrap().path()).unwrap()).unwrap();s.test_copy_recovered_native_sources(company,RecoveredNativeBatch{test_copy:true,contexts:vec![c.clone()],supporting_records:vec![]}).await.unwrap();originals.push(c);}
        let links:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_IDENTITY_LINK_BATCH").unwrap()).unwrap()).unwrap();
        let records=links["records"].as_array().unwrap();assert_eq!(records.len(),16);
        let make=|records:Vec<Value>|IdentityLinkBatch{test_copy:true,profile_sha256:profile()["profile_sha256"].as_str().unwrap().into(),records};
        let raw_before=s.test_copy_recovered_native_source_summary(company).await.unwrap();assert_eq!(raw_before["unassigned_contexts"],42);
        // No early write on a later invalid row.
        let mut changed=records[1].clone();changed["target_client_number"]=json!("0");
        assert!(s.test_copy_recovered_identity_links(company,make(vec![records[0].clone(),changed])).await.is_err());
        assert_eq!(s.test_copy_effective_recovered_summary(company).await.unwrap()["identity_links_received"],0);
        let one=s.test_copy_recovered_identity_links(company,make(vec![records[0].clone()])).await.unwrap();assert_eq!(one["assigned_contexts"],39);assert_eq!(one["unassigned_contexts"],41);assert_eq!(one["identity_links_complete"],false);
        let all=s.test_copy_recovered_identity_links(company,make(records.clone())).await.unwrap();assert_eq!(all["created"],15);assert_eq!(all["unchanged_replays"],1);assert_eq!(all["assigned_contexts"],54);assert_eq!(all["unassigned_contexts"],26);assert_eq!(all["identity_links_complete"],true);
        assert_eq!(s.test_copy_recovered_identity_links(company,make(records.clone())).await.unwrap()["unchanged_replays"],16);
        assert_eq!(raw_before,s.test_copy_recovered_native_source_summary(company).await.unwrap());
        assert_eq!(s.test_copy_unassigned_recovered_native_sources(company,page()).await.unwrap()["total"],42);
        assert_eq!(s.test_copy_effective_recovered_page(company,None,page()).await.unwrap()["total"],26);
        // Production stores raw client profiles without derived return_years.
        // Exercise the existing normalization using one request-local returns read.
        let mut stripped=vec![];for row in f3["clients"].as_array().unwrap(){let n=text(row,"client_number").unwrap();for y in row["return_years"].as_array().unwrap(){store.sadd(&base(company,"returns"),&format!("{n}:{}",y.as_u64().unwrap())).await.unwrap();}let mut raw=row.clone();raw.as_object_mut().unwrap().remove("return_years");let k=base(company,&format!("client:{n}"));store.set(&k,&raw.to_string()).await.unwrap();stripped.push((k,row.to_string()));}
        store.take_test_reads();let stripped_summary=s.test_copy_effective_recovered_summary(company).await.unwrap();let reads=store.take_test_reads();assert_eq!(stripped_summary["assigned_contexts"],54);
        assert_eq!(reads.iter().filter(|r|r==&&vec![base(company,"returns")]).count(),1);
        assert_eq!(reads.iter().filter(|r|r==&&vec![base(company,"clients")]).count(),1);
        assert_eq!(reads.iter().filter(|r|r.iter().any(|k|k.starts_with(&base(company,"client:")))).count(),1);
        assert_eq!(reads.iter().filter(|r|r.iter().any(|k|k.contains(":context:"))).count(),1);
        store.take_test_reads();assert_eq!(s.test_copy_effective_recovered_page(company,None,page()).await.unwrap()["total"],26);let reads=store.take_test_reads();assert_eq!(reads.iter().filter(|r|r==&&vec![base(company,"returns")]).count(),1);assert_eq!(reads.iter().filter(|r|r.iter().any(|k|k.contains(":context:"))).count(),1);
        store.take_test_reads();let p=super::super::preserved_records::SourceClientPage{offset:0,limit:100,search:String::new(),year:None,sort:None,direction:None,status:None,province:None,_routing_path:None};s.test_copy_effective_source_clients(company,p).await.unwrap();let reads=store.take_test_reads();assert_eq!(reads.iter().filter(|r|r==&&vec![base(company,"clients")]).count(),1);assert_eq!(reads.iter().filter(|r|r==&&vec![base(company,"returns")]).count(),1);for l in records{let target=base(company,&format!("client:{}",text(l,"target_client_number").unwrap()));assert_eq!(reads.iter().flatten().filter(|k|*k==&target).count(),1);}
        for(k,raw)in stripped{store.set(&k,&raw).await.unwrap();}
        for original in &originals{let id=text(original,"context_id").unwrap();let wrapper=s.test_copy_effective_recovered_context(company,id).await.unwrap();assert!(wrapper["original_context"]==*original,"Original context changed");if original["client_number"].is_null() && !wrapper["source_identity_link"].is_null(){assert!(wrapper["original_context"]["client_number"].is_null());assert_eq!(wrapper["effective_source_client_number"],wrapper["source_identity_link"]["target_client_number"]);}}
        for (k,v) in before {assert!(store.get(&k).await.unwrap().as_deref()==Some(&v),"Current SOURCE profile changed");}
        let l=&records[0];let client=text(l,"target_client_number").unwrap();
        let make_page=||super::super::preserved_records::SourceClientPage{offset:0,limit:100,search:client.into(),year:Some(l["source_year"].as_u64().unwrap() as u16),sort:None,direction:None,status:None,province:None,_routing_path:None};
        let clients=s.test_copy_effective_source_clients(company,make_page()).await.unwrap();let row=clients["clients"].as_array().unwrap().iter().find(|r|r["client_number"]==client).unwrap();assert!(row["recovered_native_source_years"].as_array().unwrap().contains(&l["source_year"]));
        let old_clients=s.test_copy_source_clients(company,make_page()).await.unwrap();assert!(old_clients["clients"].as_array().unwrap().iter().all(|r|r["client_number"]!=client||r["recovered_native_source_years"].as_array().is_none_or(|ys|!ys.contains(&l["source_year"]))));
        if let Some(directory)=std::env::var("NXT_IDENTITY_RUST_PROJECTION_DIR").ok().filter(|s|!s.is_empty()) {
            let out=std::path::Path::new(&directory);std::fs::create_dir(out).unwrap();
            let mut contexts=vec![];for original in &originals{contexts.push(s.test_copy_effective_recovered_context(company,text(original,"context_id").unwrap()).await.unwrap());}
            let client_page=super::super::preserved_records::SourceClientPage{offset:0,limit:100,search:String::new(),year:None,sort:None,direction:None,status:None,province:None,_routing_path:None};
            // Exact Rust projections for the closed source contexts; no credentials or remote store.
            let mut clients=vec![];let ids:BTreeSet<_>=contexts.iter().filter_map(|w|w["effective_source_client_number"].as_str()).collect();
            for n in ids{let p=super::super::preserved_records::SourceClientPage{search:n.into(),..super::super::preserved_records::SourceClientPage{offset:0,limit:100,search:String::new(),year:None,sort:None,direction:None,status:None,province:None,_routing_path:None}};let v=s.test_copy_effective_source_clients(company,p).await.unwrap();clients.push(v["clients"].as_array().unwrap().iter().find(|r|r["client_number"]==n).unwrap().clone());}
            let all=s.test_copy_effective_source_clients(company,client_page).await.unwrap();
            let projection=json!({"schema":"local-memory-Rust-identity-source-projection/v1","actual_Rust_methods":true,"remote_API_called":false,"credentials_accessed":false,"original_summary":raw_before,"effective_summary":s.test_copy_effective_recovered_summary(company).await.unwrap(),"effective_contexts":contexts,"effective_source_clients":clients,"source_client_envelope":all});
            std::fs::write(out.join("API-PROJECTION.private.json"),serde_json::to_vec(&projection).unwrap()).unwrap();
        }
        let l=&records[0];let k=base(company,&format!("client:{}",text(l,"target_client_number").unwrap()));let prior=store.get(&k).await.unwrap().unwrap();let mut target:Value=serde_json::from_str(&prior).unwrap();target["name"]=json!("SYNTHETIC CHANGED TARGET");store.set(&k,&target.to_string()).await.unwrap();assert!(s.test_copy_effective_recovered_summary(company).await.is_err());store.set(&k,&prior).await.unwrap();
        // A stored-link corruption is not an absent link or an unassigned context.
        let k=key(company,&format!("link:{}",text(l,"context_id").unwrap()));store.set(&k,"{}").await.unwrap();assert!(s.test_copy_effective_recovered_summary(company).await.is_err());
    }
}

struct IdentityInputs {
    originals:BTreeMap<String,Value>, targets:BTreeMap<String,Value>,
    registered:BTreeSet<String>, years:BTreeMap<String,BTreeSet<u16>>,
}
impl IdentityInputs {
    fn target(&self,client:&str,expected:&str)->Result<Value>{
        if self.registered.len()!=5856||!self.registered.contains(client){return Err(conflict("Identity target is absent from pinned SOURCE roster"));}
        let mut target=self.targets.get(client).ok_or_else(||conflict("SOURCE target missing"))?.clone();
        if digest(&target)!=expected{target["return_years"]=json!(self.years.get(client).cloned().unwrap_or_default());}
        if target["client_number"]!=client||digest(&target)!=expected{return Err(conflict("Identity target no longer matches pinned F3 profile"));}Ok(target)
    }
}
fn effective_wrapper(original:&Value,link:Option<&Value>)->Value{
    let target=link.map(|l|l["target_client_number"].clone()).unwrap_or_else(||original["client_number"].clone());
    json!({"original_context":original,"effective_source_client_number":target,"source_identity_link":link,"original_packet_modified":false,"current_profiles_modified":0,"tax_cases_imported":0,"financial_values_approved":false})
}
fn effective_summary(c:&str,raw:&Value,links:&BTreeMap<String,Value>)->Value{
    let mut index=raw["context_index"].as_array().unwrap().clone();let mut years=BTreeMap::<String,BTreeSet<u16>>::new();let mut assigned=0;
    for row in &mut index{
        let link=links.get(row["context_id"].as_str().unwrap());let target=link.map(|l|l["target_client_number"].clone()).unwrap_or_else(||row["client_number"].clone());
        row["effective_source_client_number"]=target.clone();row["source_identity_link_id"]=link.map(|l|l["link_id"].clone()).unwrap_or(Value::Null);
        if let Some(n)=target.as_str(){assigned+=1;years.entry(n.into()).or_default().insert(row["source_year"].as_u64().unwrap() as u16);}
    }
    let mut out=envelope(c);out["source_contexts"]=raw["source_contexts"].clone();out["assigned_contexts"]=json!(assigned);out["unassigned_contexts"]=json!(index.len()-assigned);out["identity_links_received"]=json!(links.len());out["identity_links_complete"]=json!(links.len()==16);out["expected_counts"]=profile()["expected_counts"].clone();out["original_assigned_contexts"]=raw["assigned_contexts"].clone();out["original_unassigned_contexts"]=raw["unassigned_contexts"].clone();out["context_index"]=json!(index);out["identity_links"]=json!(links.values().collect::<Vec<_>>());out["client_source_years"]=json!(years);out
}
impl SourceImport {
    fn identity_guard(&self,c:&str)->Result<()>{
        self.test_copy_guard(c,true)?;validate_identity_profile(&profile())?;
        if profile()["contract"]["source_scope"]!=c{return Err(conflict("Identity overlay is closed to its exact SOURCE company"));}Ok(())
    }
    async fn stored_identity_links(&self,c:&str)->Result<BTreeMap<String,Value>>{
        self.identity_guard(c)?;
        let mut ids=self.store.smembers(&key(c,"contexts")).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>16{return Err(storage("Identity overlay index exceeds closed profile"));}
        let keys:Vec<_>=ids.iter().map(|id|key(c,&format!("link:{id}"))).collect();let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if rows.len()!=ids.len(){return Err(storage("Identity link cardinality differs"));}
        let mut links=BTreeMap::new();for(id,raw)in ids.into_iter().zip(rows){let l:Value=serde_json::from_str(&raw.ok_or_else(||storage("Identity link missing"))?).map_err(|_|storage("Identity link corrupt"))?;validate_closed_link(&l)?;if l["context_id"]!=id{return Err(storage("Identity link index differs"));}links.insert(id,l);}Ok(links)
    }
    async fn identity_inputs(&self,c:&str,links:&BTreeMap<String,Value>,extra:&BTreeSet<String>)->Result<IdentityInputs>{
        let mut ids=extra.clone();ids.extend(links.keys().cloned());
        let originals=self.recovered_identity_originals(c,&ids).await?;
        let mut numbers:BTreeSet<String>=links.values().map(|l|l["target_client_number"].as_str().unwrap().to_owned()).collect();
        numbers.extend(originals.values().filter_map(|o|o["client_number"].as_str().map(str::to_owned)));
        let mut inputs=IdentityInputs{originals,targets:BTreeMap::new(),registered:BTreeSet::new(),years:BTreeMap::new()};
        if !numbers.is_empty(){
            inputs.registered=self.store.smembers(&base(c,"clients")).await.map_err(|e|storage(&e))?.into_iter().collect();
            // Exactly one read of the original return index per request. Never cache across requests.
            for id in self.store.smembers(&base(c,"returns")).await.map_err(|e|storage(&e))?{if let Some((n,y))=id.split_once(':'){if let Ok(y)=y.parse(){inputs.years.entry(n.into()).or_default().insert(y);}}}
            let keys:Vec<_>=numbers.iter().map(|n|base(c,&format!("client:{n}"))).collect();let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
            if rows.len()!=numbers.len(){return Err(storage("SOURCE target read cardinality differs"));}
            for(n,raw)in numbers.into_iter().zip(rows){let v:Value=serde_json::from_str(&raw.ok_or_else(||conflict("SOURCE target missing"))?).map_err(|_|storage("SOURCE target corrupt"))?;inputs.targets.insert(n,v);}
        }
        for original in inputs.originals.values(){if let Some(n)=original["client_number"].as_str(){inputs.target(n,text(original,"target_source_profile_sha256")?)?;}}
        for(id,l)in links{let original=inputs.originals.get(id).ok_or_else(||conflict("Identity original missing"))?;let target=inputs.target(text(l,"target_client_number")?,text(l,"target_source_profile_sha256")?)?;Self::identity_anchor(l,original,&target)?;}
        Ok(inputs)
    }
    fn identity_anchor(l:&Value,original:&Value,target:&Value)->Result<()> {
        validate_closed_link(l)?;
        if !original["client_number"].is_null() || digest(&original)!=text(l,"original_context_sha256")? || original["source_year"]!=l["source_year"] || original["source_identity"]!=l["source_identity"] {return Err(conflict("Identity link original null context differs"));}
        let own=&l["own_Info1"];
        let r=original["records"].as_array().unwrap().iter().find(|r|r["record_id"]==own["record_id"] && r["source"]["record_type"]==1).ok_or_else(||bad("Own Info1 witness missing"))?;
        let bytes=STANDARD.decode(text(r,"body_base64")?).map_err(|_|bad("Info1 encoding differs"))?;
        if bytes.len()<112 || hex(&bytes)!=own["BODY_sha256"] || r["source"]["source_year"]!=own["source_year"] || r["source"]["source_file_sha256"]!=own["source_file_sha256"] || r["source"]["source_member_ordinal"]!=own["native_member_ordinal"] {return Err(bad("Own Info1 boundary differs"));}
        let field=|a:usize,b:usize|bytes[a..b].iter().map(|b|char::from(*b)).collect::<String>().trim_end_matches(' ').to_owned();
        for (name,a,b) in [("last_name",8,38),("first_name",38,68),("SIN",82,93),("DOB",94,104)] {
            if hex(&bytes[a..b])!=own["fixed_identity_field_sha256"][name]{return Err(bad("Own Info1 fixed identity field differs"));}
        }
        let sin=field(82,93);if !sin.chars().all(|x|x.is_ascii_digit()||x==' '||x=='-'){return Err(bad("Unsupported SIN witness"));}
        let sin:String=sin.chars().filter(char::is_ascii_digit).collect();
        let dob=chrono::NaiveDate::parse_from_str(&field(94,104),"%d-%m-%Y").map_err(|_|bad("DOB witness invalid"))?.format("%Y-%m-%d").to_string();
        let name=format!("{}, {}",field(8,38),field(38,68));
        if sin.len()!=9 || ["000000000","999999999"].contains(&sin.as_str()) || hex(sin.as_bytes())!=own["SIN_sha256"] || hex(dob.as_bytes())!=own["DOB_ISO_sha256"] || hex(name.as_bytes())!=own["exact_historical_full_name_sha256"] {return Err(bad("Own Info1 normalized identity differs"));}
        let client=text(l,"target_client_number")?;
        let witnesses=l["crossyear_target_Info1_witnesses"].as_array().ok_or_else(||bad("Cross-year witness missing"))?;
        if witnesses.is_empty() || witnesses.iter().any(|w|w["source_year"]==l["source_year"] || w["source_client_reference"]!=client || w["DCL_length_hash_CRC_verified"]!=true || w["identity_hashes"]["SIN_sha256"]!=own["SIN_sha256"] || w["identity_hashes"]["DOB_ISO_sha256"]!=own["DOB_ISO_sha256"] || w["identity_hashes"]["exact_source_full_name_sha256"]!=hex(target["name"].as_str().unwrap_or("").as_bytes())) {return Err(bad("Cross-year target witness differs"));}
        Ok(())
    }

    pub async fn test_copy_recovered_identity_links(&self,c:&str,b:IdentityLinkBatch)->Result<Value>{
        self.identity_guard(c)?;let raw=self.test_copy_recovered_native_source_summary(c).await?;
        if !b.test_copy||b.profile_sha256!=profile()["profile_sha256"]||b.records.is_empty()||b.records.len()>16{return Err(bad("Identity link batch scope differs"));}
        if raw["source_contexts"]!=80||raw["member_records"]!=286||raw["supporting_records"]!=47||raw["assigned_contexts"]!=38||raw["unassigned_contexts"]!=42{return Err(conflict("Complete original 80-context source must be received first"));}
        let mut links=self.stored_identity_links(c).await?;let mut ids=BTreeSet::new();
        for l in &b.records{validate_closed_link(l)?;let id=text(l,"context_id")?;if !ids.insert(id){return Err(bad("Duplicate identity-link context"));}if links.get(id).is_some_and(|old|old!=l){return Err(conflict("Identity links are immutable"));}links.insert(id.into(),l.clone());}
        self.identity_inputs(c,&links,&BTreeSet::new()).await?;
        // Check even a body that was saved before its derived index was repaired.
        let keys:Vec<_>=b.records.iter().map(|l|key(c,&format!("link:{}",l["context_id"].as_str().unwrap()))).collect();
        let prior=self.store.mget(&keys).await.map_err(|e|storage(&e))?;if prior.len()!=b.records.len(){return Err(storage("Identity replay cardinality differs"));}
        for(l,old)in b.records.iter().zip(prior){if old.is_some_and(|v|v!=l.to_string()){return Err(conflict("Identity links are immutable"));}}
        let(mut created,mut replay)=(0,0);for l in &b.records{let id=text(l,"context_id")?;let k=key(c,&format!("link:{id}"));let raw=l.to_string();let fresh=self.store.set_nx(&k,&raw).await.map_err(|e|storage(&e))?;if self.store.get(&k).await.map_err(|e|storage(&e))?.as_deref()!=Some(&raw){return Err(conflict("Identity link changed concurrently"));}self.store.sadd(&key(c,"contexts"),id).await.map_err(|e|storage(&e))?;if fresh{created+=1}else{replay+=1}}
        let mut out=effective_summary(c,&raw,&links);out["created"]=json!(created);out["unchanged_replays"]=json!(replay);Ok(out)
    }
    pub async fn test_copy_read_recovered_identity_links(&self,c:&str,l:RecoveredNativeLookup)->Result<Value>{
        if !l.test_copy||l.context_ids.len()!=1{return Err(bad("Read one source identity link"));}
        let links=self.stored_identity_links(c).await?;self.identity_inputs(c,&links,&BTreeSet::new()).await?;let mut out=envelope(c);out["context_ids"]=json!(l.context_ids);out["records"]=json!([links.get(&l.context_ids[0])]);Ok(out)
    }
    pub async fn test_copy_effective_recovered_summary(&self,c:&str)->Result<Value>{
        let raw=self.test_copy_recovered_native_source_summary(c).await?;let links=self.stored_identity_links(c).await?;self.identity_inputs(c,&links,&BTreeSet::new()).await?;Ok(effective_summary(c,&raw,&links))
    }
    /// Share the same request-local roster and raw target profiles with the
    /// filtered client-list reader, avoiding a second read of either input.
    pub(super) async fn effective_recovered_client_inputs(&self,c:&str)->Result<(BTreeMap<String,BTreeSet<u16>>,Vec<String>,BTreeMap<String,Value>)>{
        let raw=self.test_copy_recovered_native_source_summary(c).await?;let links=self.stored_identity_links(c).await?;let inputs=self.identity_inputs(c,&links,&BTreeSet::new()).await?;
        let years=serde_json::from_value(effective_summary(c,&raw,&links)["client_source_years"].clone()).map_err(|_|storage("Effective SOURCE years differ"))?;
        let numbers=if inputs.registered.is_empty(){self.store.smembers(&base(c,"clients")).await.map_err(|e|storage(&e))?}else{inputs.registered.into_iter().collect()};
        Ok((years,numbers,inputs.targets))
    }
    pub async fn test_copy_effective_recovered_context(&self,c:&str,id:&str)->Result<Value>{
        let links=self.stored_identity_links(c).await?;let inputs=self.identity_inputs(c,&links,&BTreeSet::from([id.into()])).await?;
        Ok(effective_wrapper(inputs.originals.get(id).ok_or_else(||conflict("Original recovered context missing"))?,links.get(id)))
    }
    pub async fn test_copy_effective_recovered_page(&self,c:&str,client:Option<&str>,page:RecoveredNativePage)->Result<Value>{
        if page.limit!=1||page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y))||client.is_some_and(|n|n.is_empty()||n.len()>12||!n.bytes().all(|b|b.is_ascii_digit())){return Err(bad("Invalid effective recovered source page"));}
        let raw=self.test_copy_recovered_native_source_summary(c).await?;let links=self.stored_identity_links(c).await?;let summary=effective_summary(c,&raw,&links);
        let mut rows:Vec<_>=summary["context_index"].as_array().unwrap().iter().filter(|r|match client{Some(n)=>r["effective_source_client_number"]==n,None=>r["effective_source_client_number"].is_null()}).filter(|r|page.source_return_year.is_none_or(|y|r["source_year"]==y)).collect();rows.sort_by_key(|r|r["context_id"].as_str().unwrap());
        let total=rows.len();let ids:BTreeSet<String>=rows.into_iter().skip(page.offset).take(1).map(|r|r["context_id"].as_str().unwrap().into()).collect();
        let inputs=self.identity_inputs(c,&links,&ids).await?;let records:Vec<_>=ids.iter().map(|id|effective_wrapper(&inputs.originals[id],links.get(id))).collect();
        let mut out=envelope(c);out["records"]=json!(records);out["total"]=json!(total);out["offset"]=json!(page.offset);out["limit"]=json!(1);out["source_return_year"]=json!(page.source_return_year);if let Some(n)=client{out["client_number"]=json!(n);}Ok(out)
    }
}
