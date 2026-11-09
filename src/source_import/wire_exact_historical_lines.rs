//! Additive exact original wire cents. Never writes the preserved reader row.
use super::preserved_records::{historical_source_identity_and_review,SourceFactPage};
use super::{bad,conflict,missing,storage,ImportError,SourceImport};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::collections::BTreeSet;
use std::sync::OnceLock;

type Result<T> = std::result::Result<T,ImportError>;
const LINES:[&str;6]=["15000","23600","26000","42000","48400","48500"];
const MAX_BATCH:usize=100;
const MAX_RESPONSE:usize=3*1024*1024;
#[derive(Clone,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct WireExactBatch {pub test_copy:bool,pub profile_sha256:String,pub records:Vec<Value>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireExactLookup {pub test_copy:bool,pub profile_sha256:String,pub record_ids:Vec<String>}
#[derive(Clone,Deserialize)]
struct ClosedProfile {
    schema:String,profile_sha256:String,contract:Value,expected_counts:Value,
    context_sha256:BTreeSet<String>,required_overlay_original_row_sha256:BTreeSet<String>,
}
fn digest(v:&Value)->String {hex::encode(Sha256::digest(serde_json::to_vec(v).expect("JSON value")))}
fn bytes_sha(b:&[u8])->String {hex::encode(Sha256::digest(b))}
fn hash(s:&str)->bool {s.len()==64&&s.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))}
fn text<'a>(v:&'a Value,k:&str)->Result<&'a str>{v[k].as_str().ok_or_else(||bad("Wire precision text missing"))}
fn uint(v:&Value,k:&str)->Result<u64>{v[k].as_u64().ok_or_else(||bad("Wire precision integer missing"))}
fn cents(v:&Value)->Result<i64>{let s=v.as_str().ok_or_else(||bad("Wire cents must be a signed integer string"))?;let n=s.parse::<i64>().map_err(|_|bad("Wire cents outside bound"))?;if n.to_string()!=s{return Err(bad("Wire cents are not canonical"));}Ok(n)}
fn no_float(v:&Value)->bool {match v{Value::Number(n)=>!n.is_f64(),Value::Array(a)=>a.iter().all(no_float),Value::Object(o)=>o.values().all(no_float),_=>true}}
fn profile()->&'static ClosedProfile {
    static P:OnceLock<ClosedProfile>=OnceLock::new();P.get_or_init(||{
        let p:ClosedProfile=serde_json::from_str(include_str!("wire_exact_historical_profile.json")).expect("closed wire profile");
        assert_eq!(p.schema,"wire-exact-historical-lines-closed-profile/v1");assert_eq!(digest(&p.contract),p.profile_sha256);
        assert_eq!(p.contract["expected_counts"],p.expected_counts);
        assert_eq!(digest(&json!(p.context_sha256)),p.contract["context_sha256_inventory_sha256"]);
        assert_eq!(digest(&json!(p.required_overlay_original_row_sha256)),p.contract["required_overlay_row_sha256_inventory_sha256"]);
        assert_eq!(p.context_sha256.len()as u64,p.expected_counts["overlay_contexts"].as_u64().unwrap());
        assert_eq!(p.context_sha256.len(),p.required_overlay_original_row_sha256.len());p
    })
}
fn inventory()->&'static BTreeSet<String> {
    static I:OnceLock<BTreeSet<String>>=OnceLock::new();I.get_or_init(||{
        let raw=include_bytes!("wire_exact_historical_inventory.sha256s");
        assert_eq!(bytes_sha(raw),profile().contract["original_row_inventory_file_sha256"]);
        let text=std::str::from_utf8(raw).expect("hash inventory UTF8");let rows:Vec<_>=text.lines().collect();
        assert!(text.ends_with('\n')&&rows.iter().all(|h|hash(h))&&rows.windows(2).all(|w|w[0]<w[1]));
        let set:BTreeSet<String>=rows.into_iter().map(str::to_owned).collect();
        assert_eq!(set.len()as u64,profile().expected_counts["audited_original_contexts"].as_u64().unwrap());
        assert!(profile().required_overlay_original_row_sha256.is_subset(&set));set
    })
}
fn original_key(company:&str,id:&str)->String {format!("dtmaxtc:{company}:historical_lines:{id}")}
fn key(company:&str,rest:&str)->String {format!("dtmaxtc:{company}:wire_exact_historical_lines:{}:{rest}",profile().profile_sha256)}
fn valid_id(s:&str)->bool {
    let p:Vec<_>=s.split(':').collect();p.len()==3&&!p[0].is_empty()&&p[0].len()<=12&&p[0].bytes().all(|c|c.is_ascii_digit())
        &&p[1].parse::<u16>().is_ok_and(|y|(1990..=2100).contains(&y))&&hash(p[2])
}
fn null_values()->Value {json!({"15000":null,"23600":null,"26000":null,"42000":null,"48400":null,"48500":null})}
fn envelope(company:&str)->Value {
    json!({"test_copy":true,"source_scope":company,"kind":"wire-exact-historical-lines","wire_exact_profile_sha256":profile().profile_sha256,
        "wire_exact_inventory_sha256":profile().contract["original_row_inventory_file_sha256"],"source_only":true,"vendor_activation_required":false,
        "original_historical_records_modified":0,"current_profiles_modified":0,"earlier_figures_or_slips_modified":0,"native_contexts_created":0,
        "tax_cases_imported":0,"financial_values_approved":false,"recalculated":false,"source_versions_reselected":false,"live_cutover_allowed":false})
}
fn validate_packet(p:&Value)->Result<(String,usize)> {
    let profile=profile();
    if p["schema"]!="historical-exact-wire-overlay-context/v1"||p["source_scope"]!="dtmax-test-copy"||!profile.context_sha256.contains(&digest(p))||!no_float(p)
        ||p["source_only"]!=true||p["original_historical_row_modified"]!=false||p["financial_values_approved"]!=false||p["recalculated"]!=false||p["tax_case_owner_assigned"]!=false {
        return Err(bad("Wire overlay is outside the complete closed packet profile"));
    }
    let original=&p["original_historical_row"];let(old_id,_)=historical_source_identity_and_review(original)?;let oldsha=text(p,"original_historical_row_sha256")?;
    if digest(original)!=oldsha||!inventory().contains(oldsha)||!profile.required_overlay_original_row_sha256.contains(oldsha)
        ||original["archive_sha256"]!=profile.contract["source_archive_sha256"]||original["vendor_module"]!="T1" {
        return Err(bad("Wire overlay does not bind the exact original canonical row"));
    }
    let selection=&p["source_selection"];let q=uint(selection,"reader_q_plan")?;
    if ![0,7].contains(&q)||selection["outer_plan"]!=0||selection["E11_form_occurrence"]!=0||selection["marker1_present"]!=true
        ||selection["selected_saved_preferred_context_verified"]!=true||selection["original_reader_module"]!=original["vendor_module"]
        ||selection["original_mapping_state"]!=original["mapping_state"]||!hash(text(selection,"source_context_sha256")?)
        ||p["static_selector_proof_sha256"]!=profile.contract["static_selector_proof_sha256"] {
        return Err(bad("Wire overlay changes the original selected source version or mapping"));
    }
    let derived_id=digest(&json!({"original_historical_row_sha256":oldsha,"source_context_sha256":selection["source_context_sha256"]}));
    if p["context_id"]!=derived_id{return Err(bad("Wire overlay context identity differs"));}
    let fields=p["field_observations"].as_array().ok_or_else(||bad("Wire fields missing"))?;
    if fields.is_empty()||fields.len()>6{return Err(bad("Wire field count outside six source lines"));}
    let mut seen=BTreeSet::new();let mut exact=original["values_cents"].clone();
    for f in fields {
        let line=text(f,"line_code")?;if !LINES.contains(&line)||!seen.insert(line){return Err(bad("Duplicate or unknown exact source line"));}
        let wire=cents(&f["exact_wire_cents"])?;let wire32=i32::try_from(wire).map_err(|_|bad("Wire field outside signed int32"))?;
        if f["original_historical_row_sha256"]!=oldsha||f["original_values_cents"]!=original["values_cents"][line]
            ||cents(&f["original_values_cents"])?==wire||f["source_archive_sha256"]!=original["archive_sha256"]
            ||f["source_context_sha256"]!=selection["source_context_sha256"]||f["source_EXC_offset"]!=selection["source_EXC_offset"]
            ||f["reader_q_plan"]!=q||f["source_outer_plan"]!=0||f["variant_name"]!=selection["variant_name"]
            ||f["source_form_occurrence"]!=0||f["source_field_id_occurrence"]!=0||f["strict_expanded_CRC32_verified"]!=true
            ||f["original_historical_row_modified"]!=false||f["financial_values_approved"]!=false {
            return Err(bad("Wire field differs from original source coordinates or presence"));
        }
        let rt=uint(f,"source_record_type")?;if !(if q==7{[40,41]}else{[30,31]}).contains(&rt){return Err(bad("Wire field uses another snapshot"));}
        let mut raw=vec![2];raw.extend_from_slice(&(line.parse::<u16>().unwrap()/100).to_le_bytes());raw.extend_from_slice(&wire32.to_le_bytes());
        if hex::encode(&raw)!=text(f,"raw_field_hex")?||bytes_sha(&raw)!=text(f,"raw_field_sha256")?
            ||!["source_EXR_frame_sha256","source_expanded_record_sha256","source_BODY_sha256"].iter().all(|k|f[*k].as_str().is_some_and(hash)) {
            return Err(bad("Wire field raw seven-byte proof differs"));
        }
        let start=uint(f,"expanded_record_field_start")?;let end=uint(f,"expanded_record_field_end")?;let expanded=uint(f,"source_expanded_record_bytes")?;
        if start<18||end.checked_sub(start)!=Some(7)||end>expanded||expanded>263140||uint(f,"source_BODY_bytes")?.checked_add(18)!=Some(expanded)
            ||uint(f,"BODY_field_start")?.checked_add(18)!=Some(start)||uint(f,"BODY_field_end")?.checked_add(18)!=Some(end)
            ||!f["source_expanded_record_CRC32"].as_str().is_some_and(|s|s.len()==8&&s.bytes().all(|b|b.is_ascii_hexdigit()))
            ||(f["compression_flag"]!=0&&!(f["compression_flag"]==1&&f["strict_DCL_exact_length_and_no_unconsumed_bytes"]==true)) {
            return Err(bad("Wire field range, strict decoding or CRC proof differs"));
        }
        let mut identity=f.clone();let id=identity.as_object_mut().unwrap().remove("observation_id").ok_or_else(||bad("Wire observation ID missing"))?;
        if id!=digest(&identity){return Err(bad("Wire observation identity differs"));}exact[line]=json!(wire.to_string());
    }
    if exact!=p["exact_wire_values_cents"]{return Err(bad("Wire overlay changed an unobserved or absent line"));}
    Ok((old_id,fields.len()))
}
impl SourceImport {
    fn wire_guard(&self,company:&str,flag:bool,profile_id:&str)->Result<()> {
        self.test_copy_guard(company,flag)?;
        if company!="dtmax-test-copy"||profile_id!=profile().profile_sha256{return Err(conflict("Wire precision is closed to the reviewed SOURCE scope and profile"));}Ok(())
    }
    pub async fn test_copy_wire_exact_historical_lines(&self,company:&str,batch:WireExactBatch)->Result<Value> {
        self.wire_guard(company,batch.test_copy,&batch.profile_sha256)?;
        if batch.records.is_empty()||batch.records.len()>MAX_BATCH{return Err(bad("Import one to100 wire overlays per bounded request"));}
        let mut validated=Vec::new();let mut hashes=BTreeSet::new();
        for p in &batch.records {let(id,n)=validate_packet(p)?;let h=text(p,"original_historical_row_sha256")?.to_owned();if !hashes.insert(h.clone()){return Err(bad("Duplicate original wire overlay row"));}validated.push((id,h,p.to_string(),n));}
        let oldkeys:Vec<_>=validated.iter().map(|(id,_,_,_)|original_key(company,id)).collect();
        let current=self.store.mget(&oldkeys).await.map_err(|e|storage(&e))?;if current.len()!=validated.len(){return Err(storage("Wire original row cardinality differs"));}
        // Every closed packet, exact current original row and existing immutable
        // value is checked before any write. The old namespace is read-only.
        for ((_,hash,_,_),raw) in validated.iter().zip(current) {
            let value:Value=serde_json::from_str(&raw.ok_or_else(||conflict("Original historical row missing"))?).map_err(|_|storage("Original history unreadable"))?;
            if digest(&value)!=*hash{return Err(conflict("Original historical row no longer matches the exact frozen canonical row"));}
        }
        let keys:Vec<_>=validated.iter().map(|(_,hash,_,_)|key(company,&format!("row:{hash}"))).collect();
        let prior=self.store.mget(&keys).await.map_err(|e|storage(&e))?;if prior.len()!=validated.len(){return Err(storage("Wire overlay cardinality differs"));}
        for ((_,_,raw,_),existing) in validated.iter().zip(prior){if existing.as_ref().is_some_and(|s|s!=raw){return Err(conflict("Wire overlay is immutable"));}}
        let(mut created,mut unchanged)=(0usize,0usize);
        for ((_,h,body,_),k) in validated.iter().zip(keys) {
            let fresh=self.store.set_nx(&k,body).await.map_err(|e|storage(&e))?;
            if self.store.get(&k).await.map_err(|e|storage(&e))?.as_deref()!=Some(body.as_str()){return Err(conflict("Wire overlay changed concurrently"));}
            self.store.sadd(&key(company,"all"),h).await.map_err(|e|storage(&e))?;if fresh{created+=1}else{unchanged+=1}
        }
        let mut out=envelope(company);out["created"]=json!(created);out["unchanged_replays"]=json!(unchanged);out["submitted"]=json!(validated.len());Ok(out)
    }
    async fn precision_rows(&self,company:&str,ids:&[String])->Result<Vec<Value>> {
        if ids.is_empty(){return Ok(vec![])}
        let keys:Vec<_>=ids.iter().map(|id|original_key(company,id)).collect();let raw=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if raw.len()!=ids.len(){return Err(storage("Precision historical read cardinality differs"));}
        let mut originals=Vec::new();let mut rows=Vec::new();let mut hashes=Vec::new();
        for(id,raw)in ids.iter().zip(raw){let v:Value=serde_json::from_str(&raw.ok_or_else(||missing("Original historical source row absent"))?).map_err(|_|storage("Original history unreadable"))?;
            let(actual,review)=historical_source_identity_and_review(&v)?;if actual!=*id{return Err(storage("Historical source identity differs"));}
            hashes.push(digest(&v));originals.push(v.clone());let mut row=v;if let Some(review)=review{row["year_mapping_review"]=review;}rows.push(row);
        }
        // Existing qualifications are checked against known ORIGINAL reader
        // rows, before exact values are applied. A valid new/changed row must
        // remain explicitly unverified, without borrowing a stale old review.
        let known:Vec<_>=hashes.iter().enumerate().filter(|(_,h)|inventory().contains(*h)).map(|(i,_)|i).collect();
        let mut qualified:Vec<_>=known.iter().map(|i|rows[*i].clone()).collect();
        if !qualified.is_empty(){self.attach_personal_source_info(company,&mut qualified).await?;self.attach_native_module_reviews(company,&mut qualified).await?;}
        for(i,row)in known.into_iter().zip(qualified){rows[i]=row;}
        let needed:Vec<_>=hashes.iter().enumerate().filter(|(_,h)|profile().required_overlay_original_row_sha256.contains(*h)).collect();
        let keys:Vec<_>=needed.iter().map(|(_,h)|key(company,&format!("row:{h}"))).collect();let values=self.store.mget(&keys).await.map_err(|e|storage(&e))?;
        if values.len()!=needed.len(){return Err(storage("Precision overlay read cardinality differs"));}
        let mut overlays=std::collections::BTreeMap::new();for((i,h),raw)in needed.into_iter().zip(values){if let Some(raw)=raw{
            let p:Value=serde_json::from_str(&raw).map_err(|_|storage("Wire overlay unreadable"))?;let(id,_)=validate_packet(&p)?;
            if id!=ids[i]||p["original_historical_row_sha256"]!=*h||p["original_historical_row"]!=originals[i]{return Err(storage("Stored wire overlay differs from the current original row"));}overlays.insert(i,p);
        }}
        for(i,row)in rows.iter_mut().enumerate(){let original=&originals[i];let h=&hashes[i];let mut exact=null_values();let(mut state,mut basis)=("unverified_original_row",None);
            let overlay=overlays.remove(&i);if inventory().contains(h){if profile().required_overlay_original_row_sha256.contains(h){
                if let Some(p)=&overlay{exact=p["exact_wire_values_cents"].clone();state="verified_exact_wire";basis=Some("closed_original_wire_overlay");}else{state="pending_required_wire_overlay";}
            }else{exact=original["values_cents"].clone();state="verified_exact_wire";basis=Some("inventory_exact_unchanged");}}
            row["reader_values_cents"]=original["values_cents"].clone();row["values_cents"]=exact;row["original_historical_row"]=original.clone();row["original_historical_row_sha256"]=json!(h);
            row["precision_state"]=json!(state);row["precision_basis"]=json!(basis);row["wire_exact_profile_sha256"]=json!(profile().profile_sha256);
            row["wire_exact_inventory_sha256"]=profile().contract["original_row_inventory_file_sha256"].clone();row["wire_exact_overlay_sha256"]=overlay.as_ref().map(digest).map(Value::String).unwrap_or(Value::Null);
            row["wire_exact_overlay"]=overlay.unwrap_or(Value::Null);row["financial_values_approved"]=json!(false);row["recalculated"]=json!(false);row["source_only"]=json!(true);
        }Ok(rows)
    }
    pub async fn test_copy_read_wire_exact_historical_lines(&self,company:&str,lookup:WireExactLookup)->Result<Value> {
        self.wire_guard(company,lookup.test_copy,&lookup.profile_sha256)?;
        if lookup.record_ids.is_empty()||lookup.record_ids.len()>MAX_BATCH||lookup.record_ids.iter().any(|id|!valid_id(id))||lookup.record_ids.iter().collect::<BTreeSet<_>>().len()!=lookup.record_ids.len(){return Err(bad("Read one to100 unique historical source identities"));}
        let rows=self.precision_rows(company,&lookup.record_ids).await?;if serde_json::to_vec(&rows).map_err(|_|storage("Precision response encoding failed"))?.len()>MAX_RESPONSE{return Err(bad("Read fewer precision rows"));}
        let mut out=envelope(company);out["record_ids"]=json!(lookup.record_ids);out["records"]=json!(rows);Ok(out)
    }
    pub async fn test_copy_client_wire_exact_historical_lines(&self,company:&str,client:&str,page:SourceFactPage)->Result<Value> {
        self.wire_guard(company,true,&profile().profile_sha256)?;
        if client.is_empty()||client.len()>12||!client.bytes().all(|c|c.is_ascii_digit())||page.limit==0||page.limit>100||page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)){return Err(bad("Invalid precision source client/year page"));}
        let mut ids=self.store.smembers(&format!("dtmaxtc:{company}:member_historical_lines:{client}")).await.map_err(|e|storage(&e))?;ids.sort();if ids.len()>1000{return Err(storage("Precision client history exceeds bound"));}
        if ids.iter().any(|id|!valid_id(id)||id.split(':').next()!=Some(client)){return Err(storage("Precision historical target index differs"));}
        if let Some(year)=page.source_return_year{ids.retain(|id|id.split(':').nth(1).and_then(|s|s.parse::<u16>().ok())==Some(year));}
        let total=ids.len();let ids:Vec<_>=ids.into_iter().skip(page.offset).take(page.limit).collect();let rows=self.precision_rows(company,&ids).await?;
        let mut out=envelope(company);out["client_number"]=json!(client);out["total"]=json!(total);out["offset"]=json!(page.offset);out["limit"]=json!(page.limit);out["source_return_year"]=json!(page.source_return_year);out["records"]=json!(rows);Ok(out)
    }
    pub async fn test_copy_wire_exact_historical_summary(&self,company:&str)->Result<Value> {
        self.wire_guard(company,true,&profile().profile_sha256)?;let mut hashes=self.store.smembers(&key(company,"all")).await.map_err(|e|storage(&e))?;hashes.sort();
        if hashes.len()>profile().required_overlay_original_row_sha256.len()||hashes.iter().any(|h|!profile().required_overlay_original_row_sha256.contains(h)){return Err(storage("Wire overlay inventory outside closed profile"));}
        let mut fields=0;for group in hashes.chunks(100){let keys:Vec<_>=group.iter().map(|h|key(company,&format!("row:{h}"))).collect();let rows=self.store.mget(&keys).await.map_err(|e|storage(&e))?;if rows.len()!=group.len(){return Err(storage("Wire summary cardinality differs"));}
            for(h,raw)in group.iter().zip(rows){let p:Value=serde_json::from_str(&raw.ok_or_else(||storage("Indexed wire overlay missing"))?).map_err(|_|storage("Wire overlay unreadable"))?;let(_,n)=validate_packet(&p)?;if p["original_historical_row_sha256"]!=*h{return Err(storage("Wire overlay index differs"));}fields+=n;}
        }
        let mut out=envelope(company);out["expected_counts"]=profile().expected_counts.clone();out["overlays_received"]=json!(hashes.len());out["field_observations_received"]=json!(fields);
        out["compiled_original_row_inventory_count"]=json!(inventory().len());out["contexts_exact_without_overlay"]=json!(inventory().len()-profile().required_overlay_original_row_sha256.len());
        out["required_overlay_contexts_missing"]=json!(profile().required_overlay_original_row_sha256.len()-hashes.len());out["closed_overlay_inventory_complete"]=json!(hashes.len()==profile().required_overlay_original_row_sha256.len());
        out["received_original_row_hash_inventory_sha256"]=json!(digest(&json!(hashes)));Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use super::super::preserved_records::{SourceFactLookup,SourceFactPage};
    fn page()->SourceFactPage{serde_json::from_value(json!({"kind":"wire-exact-historical-lines","offset":0,"limit":100})).unwrap()}
    fn original()->Value {json!({"client_number":"100","family_file_id":"999","tax_year":2014,"archive_sha256":"a".repeat(64),"vendor_module":"T1","mapping_state":"pending_year_verification","values_cents":{"15000":"12345","23600":null,"26000":null,"42000":null,"48400":null,"48500":null}})}
    #[test]
    fn profile_and_full_original_row_inventory_are_closed_and_client_free() {
        let p=profile();assert_eq!(inventory().len(),32744);assert_eq!(p.context_sha256.len(),993);
        assert_eq!(p.expected_counts["audited_numeric_slots"],66340);assert_eq!(p.expected_counts["field_observations"],1973);
        assert_eq!(inventory().len()-p.required_overlay_original_row_sha256.len(),31751);
        assert!(!inventory().contains(&digest(&original())));assert!(inventory().iter().all(|s|hash(s)));
        assert!(valid_id(&format!("100:2014:{}","a".repeat(64))));assert!(!valid_id("../another:key"));
    }
    #[tokio::test]
    async fn unknown_originals_remain_unverified_and_scopes_cannot_import() {
        let store=Store::memory();let s=SourceImport::new(store.clone());let company="dtmax-test-copy";let r=original();let(id,_)=historical_source_identity_and_review(&r).unwrap();
        store.set(&original_key(company,&id),&r.to_string()).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:member_historical_lines:100"),&id).await.unwrap();
        let result=s.test_copy_client_source_facts(company,"100",page()).await.unwrap();let row=&result["records"][0];
        assert_eq!(row["precision_state"],"unverified_original_row");assert_eq!(row["values_cents"],null_values());assert!(row["reader_values_cents"]==r["values_cents"]);
        assert!(row["original_historical_row"]==r);assert!(row["wire_exact_overlay"].is_null());
        for(company,flag,profile_id)in[(company,false,profile().profile_sha256.as_str()),("other-test-copy",true,profile().profile_sha256.as_str()),(company,true,"unclosed")]{
            assert!(s.test_copy_wire_exact_historical_lines(company,WireExactBatch{test_copy:flag,profile_sha256:profile_id.into(),records:vec![json!({})]}).await.is_err());
        }
        assert_eq!(s.test_copy_wire_exact_historical_summary(company).await.unwrap()["overlays_received"],0);
        assert!(store.get(&original_key(company,&id)).await.unwrap().as_deref()==Some(r.to_string().as_str()));
    }
    // All private data is provided at test time, never compiled or printed.
    #[tokio::test]
    async fn private_all32744_rows_and993_overlays_are_exact_without_changing_originals() {
        let Ok(directory)=std::env::var("NXT_WIRE_EXACT_CONTEXT_DIR")else{return};let company="dtmax-test-copy";
        let store=Store::memory();let s=SourceImport::new(store.clone());let mut before=Vec::new();let mut original_rows=Vec::new();let mut original_ids=Vec::new();
        let profiles:Value=serde_json::from_slice(&std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap()).unwrap();
        assert_eq!(digest(&profiles["clients"]),"f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2");
        for row in profiles["clients"].as_array().unwrap(){let n=row["client_number"].as_str().unwrap();let k=format!("dtmaxtc:{company}:client:{n}");let raw=row.to_string();store.set(&k,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:clients"),n).await.unwrap();before.push((k,raw));}
        let native=std::fs::read(std::env::var("NXT_PRESERVED_NATIVE_FIXTURE").unwrap()).unwrap();assert_eq!(bytes_sha(&native),profile().contract["original_baseline_sha256"]);
        for line in native.split(|b|*b==b'\n').filter(|s|!s.is_empty()){
            let r:Value=serde_json::from_slice(line).unwrap();let(id,_)=historical_source_identity_and_review(&r).unwrap();assert!(inventory().contains(&digest(&r)));
            let k=original_key(company,&id);let raw=String::from_utf8(line.to_vec()).unwrap();store.set(&k,&raw).await.unwrap();store.sadd(&format!("dtmaxtc:{company}:historical_lines"),&id).await.unwrap();
            store.sadd(&format!("dtmaxtc:{company}:member_historical_lines:{}",r["client_number"].as_str().unwrap()),&id).await.unwrap();before.push((k,raw));original_rows.push(r);original_ids.push(id);
        }
        assert_eq!(original_rows.len(),32744);
        for(env,kind,profile_file)in[("NXT_PRESERVED_CACHED_FIXTURE","printed_cached_figures",include_str!("printed_cached_figure_profile.json")),("NXT_PRESERVED_PDF_OBSERVATIONS","preserved_pdf_figures",include_str!("preserved_pdf_figure_profile.json"))]{
            let p:Value=serde_json::from_str(profile_file).unwrap();let raw=std::fs::read(std::env::var(env).unwrap()).unwrap();
            for line in raw.split(|b|*b==b'\n').filter(|l|!l.is_empty()){let r:Value=serde_json::from_slice(line).unwrap();let id=format!("{}:{}:{}",r["client_number"].as_str().unwrap(),r["source_return_year"].as_u64().unwrap(),r["pdf"]["sha256"].as_str().unwrap());let k=format!("dtmaxtc:{company}:{kind}:{}:record:{id}",p["profile_sha256"].as_str().unwrap());let raw=String::from_utf8(line.to_vec()).unwrap();store.set(&k,&raw).await.unwrap();before.push((k,raw));}
        }
        let mut files:Vec<_>=std::fs::read_dir(directory).unwrap().map(|f|f.unwrap().path()).filter(|p|p.extension().is_some_and(|e|e=="json")).collect();files.sort();
        let packets:Vec<Value>=files.into_iter().map(|f|serde_json::from_slice(&std::fs::read(f).unwrap()).unwrap()).collect();assert_eq!(packets.len(),993);
        let make_batch=|records:Vec<Value>|WireExactBatch{test_copy:true,profile_sha256:profile().profile_sha256.clone(),records};
        let(mut before_ready,mut before_pending)=(0,0);
        for(group,expected)in original_ids.chunks(100).zip(original_rows.chunks(100)){
            let rows=s.precision_rows(company,group).await.unwrap();
            for(row,original)in rows.iter().zip(expected){assert!(row["reader_values_cents"]==original["values_cents"]);
                if profile().required_overlay_original_row_sha256.contains(&digest(original)){before_pending+=1;assert_eq!(row["precision_state"],"pending_required_wire_overlay");assert_eq!(row["values_cents"],null_values());}
                else{before_ready+=1;assert_eq!(row["precision_state"],"verified_exact_wire");assert!(row["values_cents"]==original["values_cents"]);}
            }
        }
        assert_eq!((before_ready,before_pending),(31751,993));
        let first=&packets[0];let(first_id,_)=validate_packet(first).unwrap();let h=first["original_historical_row_sha256"].as_str().unwrap();
        let pending=s.precision_rows(company,std::slice::from_ref(&first_id)).await.unwrap();assert_eq!(pending[0]["precision_state"],"pending_required_wire_overlay");assert_eq!(pending[0]["values_cents"],null_values());assert!(pending[0]["reader_values_cents"]==first["original_historical_row"]["values_cents"]);
        let mut bad_packet=first.clone();bad_packet["exact_wire_values_cents"]["15000"]=json!("123");
        assert!(s.test_copy_wire_exact_historical_lines(company,make_batch(vec![packets[1].clone(),bad_packet])).await.is_err());assert_eq!(s.test_copy_wire_exact_historical_summary(company).await.unwrap()["overlays_received"],0);
        let first_key=original_key(company,&first_id);let prior=store.get(&first_key).await.unwrap().unwrap();let mut changed:Value=serde_json::from_str(&prior).unwrap();changed["values_cents"]["15000"]=json!("123");store.set(&first_key,&changed.to_string()).await.unwrap();
        assert!(s.test_copy_wire_exact_historical_lines(company,make_batch(vec![packets[1].clone(),first.clone()])).await.is_err());assert_eq!(s.test_copy_wire_exact_historical_summary(company).await.unwrap()["overlays_received"],0);
        assert_eq!(s.precision_rows(company,std::slice::from_ref(&first_id)).await.unwrap()[0]["precision_state"],"unverified_original_row");store.set(&first_key,&prior).await.unwrap();
        let old_lookup=SourceFactLookup{test_copy:true,kind:"historical-lines".into(),record_ids:vec![first_id.clone()]};let old_before=s.test_copy_read_source_facts(company,old_lookup).await.unwrap();
        let mut total=0;for group in packets.chunks(100){let b=make_batch(group.to_vec());assert!(serde_json::to_vec(&b).unwrap().len()<1024*1024);total+=s.test_copy_wire_exact_historical_lines(company,b.clone()).await.unwrap()["created"].as_u64().unwrap();assert_eq!(s.test_copy_wire_exact_historical_lines(company,b).await.unwrap()["unchanged_replays"],group.len());}assert_eq!(total,993);
        let summary=s.test_copy_wire_exact_historical_summary(company).await.unwrap();assert_eq!(summary["overlays_received"],993);assert_eq!(summary["field_observations_received"],1973);assert_eq!(summary["required_overlay_contexts_missing"],0);assert_eq!(summary["closed_overlay_inventory_complete"],true);
        let mut corrections=0;let mut numeric=0;let mut absent=0;let mut overlays=0;let mut unchanged=0;
        // Optional exact API projection for the private browser contract replay.
        // This is test-only, create-new, and contains no credential material.
        let mut projection=std::env::var("NXT_WIRE_EXACT_PROJECTION_OUT").ok().map(|path|std::fs::OpenOptions::new().create_new(true).write(true).open(path).unwrap());
        let by_hash:std::collections::BTreeMap<_,_>=packets.iter().map(|p|(p["original_historical_row_sha256"].as_str().unwrap(),p)).collect();
        for(group,expected)in original_ids.chunks(100).zip(original_rows.chunks(100)){
            let r=s.test_copy_read_wire_exact_historical_lines(company,WireExactLookup{test_copy:true,profile_sha256:profile().profile_sha256.clone(),record_ids:group.to_vec()}).await.unwrap();let rows=r["records"].as_array().unwrap();assert_eq!(rows.len(),expected.len());
            for(row,old)in rows.iter().zip(expected){assert_eq!(row["precision_state"],"verified_exact_wire");assert!(row["reader_values_cents"]==old["values_cents"]);assert!(row["original_historical_row"]==*old);assert!(row["vendor_module"]==old["vendor_module"]&&row["mapping_state"]==old["mapping_state"]);
                if let Some(file)=projection.as_mut(){use std::io::Write;file.write_all(&serde_json::to_vec(row).unwrap()).unwrap();file.write_all(b"\n").unwrap();}
                let sha=digest(old);if let Some(packet)=by_hash.get(sha.as_str()){assert!(row["wire_exact_overlay"]==**packet);assert!(row["values_cents"]==packet["exact_wire_values_cents"]);overlays+=1;}else{assert_eq!(row["precision_basis"],"inventory_exact_unchanged");assert!(row["values_cents"]==old["values_cents"]);assert!(row["wire_exact_overlay"].is_null());unchanged+=1;}
                for line in LINES{if old["values_cents"][line].is_null(){absent+=1;assert!(row["values_cents"][line].is_null());}else{numeric+=1;corrections+=usize::from(row["values_cents"][line]!=old["values_cents"][line]);}}
            }
        }
        assert_eq!((overlays,unchanged,numeric,absent,corrections),(993,31751,66340,130124,1973));
        let old_after=s.test_copy_read_source_facts(company,SourceFactLookup{test_copy:true,kind:"historical-lines".into(),record_ids:vec![first_id.clone()]}).await.unwrap();assert!(old_before==old_after);
        let old=first["original_historical_row"].clone();let mut year_page=page();year_page.source_return_year=Some(old["tax_year"].as_u64().unwrap()as u16);let page=s.test_copy_client_source_facts(company,old["client_number"].as_str().unwrap(),year_page).await.unwrap();assert!(page["records"].as_array().unwrap().iter().any(|r|r["original_historical_row_sha256"]==h));
        // An immutable-store conflict cannot write the other valid packet first.
        let immutable_key=key(company,&format!("row:{h}"));let body=store.get(&immutable_key).await.unwrap().unwrap();store.set(&immutable_key,"{}").await.unwrap();assert!(s.test_copy_wire_exact_historical_lines(company,make_batch(vec![packets[1].clone(),first.clone()])).await.is_err());assert_eq!(store.get(&immutable_key).await.unwrap().as_deref(),Some("{}"));store.set(&immutable_key,&body).await.unwrap();
        for group in before.chunks(250){let keys:Vec<_>=group.iter().map(|(k,_)|k.clone()).collect();let current=store.mget(&keys).await.unwrap();for(actual,(_,expected))in current.iter().zip(group){assert!(actual.as_deref()==Some(expected.as_str()),"An original row changed");}}
        let mut after=Vec::new();for row in profiles["clients"].as_array().unwrap(){let k=format!("dtmaxtc:{company}:client:{}",row["client_number"].as_str().unwrap());after.push(serde_json::from_str::<Value>(&store.get(&k).await.unwrap().unwrap()).unwrap());}assert_eq!(digest(&json!(after)),digest(&profiles["clients"]));
        eprintln!("PRIVATE_WIRE_EXACT_REPLAY_EXECUTED original_rows=32744 overlays=993 numeric=66340 absent=130124 differences=1973 original_baseline_rows_unchanged={}",before.len());
    }
}
