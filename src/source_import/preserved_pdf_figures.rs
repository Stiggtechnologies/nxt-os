//! Figures actually observed in preserved PDFs without same-year native context
//! in the pinned import-time inventory. No source replacement or tax calculation.
use super::preserved_records::SourceFactPage;
use super::{bad,conflict,missing,storage,ImportError,SourceImport};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::collections::{BTreeMap,BTreeSet};
use std::sync::OnceLock;

type Result<T> = std::result::Result<T,ImportError>;
const CLOSED_SHA:&str="f296eec4a3f76f9956c1c563fa9ac2742c9702412e39020c0f5c0b04659e1267";
const LINE_AUTHORITY_SHA:&str="cf22e772a77a90221062da1068ff348857c583137fee4a38de377b40b5be956c";
const ABSENCE_AUDIT_SHA:&str="fab25b99c5755b88d6767a4068a636c888cd19c35e070dfb2fb12fac6af4cd2b";
const LINES:[&str;6]=["15000","23600","26000","42000","48400","48500"];
const MAX_RECORDS:usize=5;
const MAX_RESPONSE:usize=3*1024*1024;
const ANCHORS:[&str;9]=["pdf_record_sha256","source_occurrence_manifest_sha256","owner_year_evidence_sha256","native_context_absence_audit_sha256","printed_line_authority_sha256","source_geometry_sha256","geometry_verifier_sha256","source_money_evidence_sha256","independent_review_sha256"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreservedPdfFigureBatch {pub test_copy:bool,pub preserved_figure_profile_sha256:String,pub records:Vec<Value>}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreservedPdfFigureLookup {pub test_copy:bool,pub record_ids:Vec<String>,#[serde(default)]pub require_present:bool}
#[derive(Clone,Default,Deserialize,Serialize,PartialEq,Eq)]
struct Counts {nonnull:usize,zero:usize,blank:usize,held:usize,notlocated:usize}
#[derive(Deserialize)]
struct ClosedProfile {contract:Value,profile_sha256:String,source_package_sha256:String,record_count:usize,owner_year_count:usize,allowed_record_sha256:BTreeMap<String,Counts>}
fn digest(bytes:&[u8])->String{hex::encode(Sha256::digest(bytes))}
fn hash(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
fn number(s:&str)->bool{!s.is_empty()&&s.len()<=12&&s.bytes().all(|b|b.is_ascii_digit())}
fn text<'a>(v:&'a Value,k:&str)->Result<&'a str>{v[k].as_str().ok_or_else(||bad("Preserved PDF observation text missing"))}
fn encode(v:&Value)->Result<String>{serde_json::to_string(v).map_err(|_|bad("Cannot encode preserved PDF observation"))}
fn keys(v:&Value,expected:&[&str])->bool{v.as_object().is_some_and(|m|m.len()==expected.len()&&expected.iter().all(|k|m.contains_key(*k)))}
fn profile()->&'static ClosedProfile {
    static P:OnceLock<ClosedProfile>=OnceLock::new();P.get_or_init(||{
        let raw=include_bytes!("preserved_pdf_figure_profile.json");assert_eq!(digest(raw),CLOSED_SHA);
        let p:ClosedProfile=serde_json::from_slice(raw).expect("closed preserved PDF figure profile");
        assert_eq!(p.record_count,5);assert_eq!(p.owner_year_count,4);assert_eq!(p.allowed_record_sha256.len(),5);
        assert!(hash(&p.profile_sha256)&&hash(&p.source_package_sha256));
        assert_eq!(digest(&serde_json::to_vec(&p.contract).unwrap()),p.profile_sha256);
        assert!(p.allowed_record_sha256.keys().all(|s|hash(s)));p
    })
}
fn authority()->&'static Value {
    static A:OnceLock<Value>=OnceLock::new();A.get_or_init(||{
        let raw=include_bytes!("preserved_pdf_line_authority.json");assert_eq!(digest(raw),LINE_AUTHORITY_SHA);
        let a:Value=serde_json::from_slice(raw).expect("explicit historical printed-line authority");
        assert_eq!(a["schema"],"dtmax-historical-printed-line-semantic-bindings/v1");
        assert_eq!(a["target_requested_codes"],json!(LINES));a
    })
}
fn printed_code(year:u16,line:&str)->Result<&'static str> {
    let a=authority();if !a["source_return_years"].as_array().is_some_and(|v|v.contains(&json!(year))){return Err(bad("Preserved PDF year has no closed printed-line authority"))}
    a["bindings"].as_array().and_then(|v|v.iter().find(|v|v["canonical_line_code"]==line)).and_then(|v|v["printed_line_code"].as_str()).ok_or_else(||bad("No explicit printed-line semantic binding"))
}
fn identity(v:&Value)->Result<String>{Ok(format!("{}:{}:{}",text(v,"client_number")?,v["source_return_year"].as_u64().ok_or_else(||bad("Preserved PDF year missing"))?,text(&v["pdf"],"sha256")?))}
fn valid_id(s:&str)->bool{let p:Vec<_>=s.split(':').collect();p.len()==3&&number(p[0])&&p[1].parse::<u16>().is_ok_and(|y|(1990..=2100).contains(&y))&&hash(p[2])}
fn prefix(c:&str)->String{format!("dtmaxtc:{c}:preserved_pdf_figures:{}",profile().profile_sha256)}
fn key(c:&str,id:&str)->String{format!("{}:record:{id}",prefix(c))}
fn index(c:&str,s:&str)->String{format!("{}:{s}",prefix(c))}
fn member(c:&str,n:&str)->String{index(c,&format!("client:{n}"))}
fn envelope(c:&str)->Value {json!({"test_copy":true,"source_scope":c,"preserved_figure_profile_sha256":profile().profile_sha256,"original_history_modified":0,"original_PDFs_modified":0,"current_profiles_modified":0,"cached_printed_observations_modified":0,"native_contexts_created":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false,"as_filed_verified":false})}
fn decimal(n:i64)->String{let a=n.unsigned_abs();format!("{}{}.{:02}",if n<0{"-"}else{""},a/100,a%100)}
fn amount(s:&str)->Result<i64>{let n=s.parse::<i64>().map_err(|_|bad("Preserved printed amount exceeds integer range"))?;if n.to_string()!=s{return Err(bad("Preserved printed cents are not canonical"))}Ok(n)}
fn bbox(v:&Value)->bool{v.as_array().is_some_and(|a|a.len()==4&&a.iter().all(|v|v.as_f64().is_some_and(|n|n.is_finite()&&(0.0..=20000.0).contains(&n)))&&a[0].as_f64()<a[2].as_f64()&&a[1].as_f64()<a[3].as_f64())}
fn candidates(o:&Value,pages:usize)->Result<Vec<Value>> {
    let raw=text(o,"printed_candidates_json")?;
    if raw.len()>65536||digest(raw.as_bytes())!=text(o,"printed_candidates_sha256")?{return Err(bad("Preserved candidate digest or bound differs"))}
    let rows:Vec<Value>=serde_json::from_str(raw).map_err(|_|bad("Preserved candidate JSON differs"))?;
    if rows.len()>100{return Err(bad("Preserved candidate count exceeds bound"))}
    for c in &rows {
        if c["page_index"].as_u64().is_none_or(|n|n>=pages as u64)||c["printed_code"]!=o["printed_line_code"]||!bbox(c.get("line_code_bbox").or_else(||c.get("code_bbox")).unwrap_or(&Value::Null))||!matches!(c["state"].as_str(),Some("printed_amount_present"|"identified_printed_amount_blank"|"held_ambiguous_money_tokens"|"held_invalid_money_tokens"|"held_money_token_crosses_box_boundary")){return Err(bad("Preserved candidate page, printed code, state or geometry differs"))}
    }Ok(rows)
}
fn shape(v:&Value)->Result<Counts> {
    if !keys(v,&["schema","preserved_figure_profile_sha256","client_number","source_return_year","source_kind","native_context_state","pdf","anchors","geometry_evidence_json","observations","source_fields_only","recalculated","as_filed_verified","preferred_version_selected","financial_values_approved"])||v["schema"]!="dtmax-preserved-PDF-figure-observations/v1"||!number(text(v,"client_number")?)||v["source_kind"]!="preserved_PDF"||v["native_context_state"]!="absent_at_pinned_source_audit"||v["source_fields_only"]!=true||["recalculated","as_filed_verified","preferred_version_selected","financial_values_approved"].iter().any(|k|v[*k]!=false){return Err(bad("Preserved PDF context or immutable assertions differ"))}
    let y=v["source_return_year"].as_u64().filter(|n|*n<=u16::MAX as u64).ok_or_else(||bad("Preserved source year differs"))? as u16;
    let p=&v["pdf"];let pages=p["page_count"].as_u64().unwrap_or(0)as usize;
    if !keys(p,&["client_number","tax_year","sha256","bytes","page_count","page_labels","original_print_output","immutable"])||p["client_number"]!=v["client_number"]||p["tax_year"]!=v["source_return_year"]||!hash(text(p,"sha256")?)||!(1..=500).contains(&pages)||p["bytes"].as_u64().is_none_or(|n|n==0||n>super::return_pdfs::MAX_PDF_BYTES as u64)||p["original_print_output"]!=true||p["immutable"]!=true||p["page_labels"].as_array().is_none_or(|a|a.len()!=pages||a.iter().any(|v|v.as_str().is_none_or(|s|s.is_empty()||s.len()>120||s.chars().any(char::is_control)))){return Err(bad("Preserved existing PDF descriptor differs"))}
    let a=&v["anchors"];if !keys(a,&ANCHORS)||ANCHORS.iter().any(|k|a[*k].as_str().is_none_or(|s|!hash(s)))||a["printed_line_authority_sha256"]!=LINE_AUTHORITY_SHA||a["native_context_absence_audit_sha256"]!=ABSENCE_AUDIT_SHA||a["pdf_record_sha256"]!=digest(encode(p)?.as_bytes()){return Err(bad("Preserved source proof anchors differ"))}
    let g=text(v,"geometry_evidence_json")?;if g.len()>131072||digest(g.as_bytes())!=a["source_geometry_sha256"]||!serde_json::from_str::<Value>(g).is_ok_and(|g|g.is_object()){return Err(bad("Preserved geometry evidence differs"))}
    let rows=v["observations"].as_array().filter(|a|a.len()==6).ok_or_else(||bad("Six ordered preserved observations required"))?;
    let mut n=Counts::default();
    for (o,line) in rows.iter().zip(LINES) {
        if !keys(o,&["line_code","printed_line_code","state","printed_cents","printed_decimal","printed_candidates_json","printed_candidates_sha256"])||o["line_code"]!=line||o["printed_line_code"]!=printed_code(y,line)?{return Err(bad("Preserved printed/canonical line binding or order differs"))}
        let c=candidates(o,pages)?;
        match text(o,"state")? {
            "single_distinct_printed_amount"=>{let value=amount(text(o,"printed_cents")?)?;if o["printed_decimal"]!=decimal(value)||c.is_empty()||c.iter().any(|c|c["state"]!="printed_amount_present"||c["amount"]!=o["printed_decimal"]){return Err(bad("Preserved printed amounts or source occurrences disagree"))}n.nonnull+=1;n.zero+=usize::from(value==0);},
            state=>{if !o["printed_cents"].is_null()||!o["printed_decimal"].is_null(){return Err(bad("Blank or held preserved amount must remain null"))}match state {
                "identified_printed_blank"=>{if c.is_empty()||c.iter().any(|c|c["state"]!="identified_printed_amount_blank"||!c["amount"].is_null()){return Err(bad("Preserved blank evidence differs"))}n.blank+=1;},
                "printed_line_not_located"=>{if !c.is_empty(){return Err(bad("Unlocated preserved line carries candidates"))}n.notlocated+=1;},
                "held_ambiguous_printed_amount"|"held_invalid_printed_money_token"=>{if c.is_empty(){return Err(bad("Held preserved line lacks candidates"))}n.held+=1;},
                _=>return Err(bad("Unknown preserved observation state"))
            }}
        }
    }Ok(n)
}
fn pinned(v:&Value)->Result<(String,String,Counts)> {
    let n=shape(v)?;if v["preserved_figure_profile_sha256"]!=profile().profile_sha256{return Err(bad("Preserved observation profile differs"))}
    let raw=encode(v)?;if raw.len()>600000{return Err(bad("Preserved observation exceeds bounded record size"))}let d=digest(raw.as_bytes());
    if profile().allowed_record_sha256.get(&d)!=Some(&n){return Err(bad("Preserved observation is outside closed exact evidence"))}Ok((raw,d,n))
}

impl SourceImport {
    async fn prepare_preserved_pdf_figures(&self,company:&str,rows:&[Value])->Result<Vec<(String,String,String)>> {
        let mut prepared=Vec::new();let mut ids=BTreeSet::new();
        for v in rows {
            let(raw,d,_)=pinned(v)?;let id=identity(v)?;if !valid_id(&id)||!ids.insert(id.clone()){return Err(bad("Preserved observation identity repeats or differs"))}
            let client=text(v,"client_number")?;if self.store.get(&format!("dtmaxtc:{company}:client:{client}")).await.map_err(|e|storage(&e))?.is_none(){return Err(missing("Preserved PDF owner is not registered"))}
            // This verifies the actual immutable blob SHA/length one PDF at a
            // time. No same-year native return, family or extract is required.
            let pdf=self.test_copy_return_pdf_content(company,client,v["source_return_year"].as_u64().unwrap()as u16,text(&v["pdf"],"sha256")?).await?;
            if pdf["record"]!=v["pdf"]||digest(encode(&pdf["record"])?.as_bytes())!=v["anchors"]["pdf_record_sha256"]{return Err(bad("Preserved observation does not match its uploaded PDF"))}
            prepared.push((id,raw,d));
        }Ok(prepared)
    }
    pub async fn test_copy_preserved_pdf_figures(&self,company:&str,batch:PreservedPdfFigureBatch)->Result<Value> {
        self.test_copy_guard(company,batch.test_copy)?;
        if batch.preserved_figure_profile_sha256!=profile().profile_sha256||batch.records.is_empty()||batch.records.len()>MAX_RECORDS{return Err(bad("Preserved figure batch profile or bound differs"))}
        let rows=self.prepare_preserved_pdf_figures(company,&batch.records).await?;
        for(id,raw,_)in &rows {if self.store.get(&key(company,id)).await.map_err(|e|storage(&e))?.is_some_and(|old|old!=*raw){return Err(conflict("Preserved PDF observation is immutable"))}}
        let mut created=0;
        for(id,raw,d)in rows {
            let new=self.store.set_nx(&key(company,&id),&raw).await.map_err(|e|storage(&e))?;
            if self.store.get(&key(company,&id)).await.map_err(|e|storage(&e))?.as_deref()!=Some(raw.as_str()){return Err(conflict("Preserved PDF observation is immutable"))}
            self.store.sadd(&index(company,"all"),&id).await.map_err(|e|storage(&e))?;
            self.store.sadd(&index(company,"digests"),&d).await.map_err(|e|storage(&e))?;
            self.store.sadd(&member(company,id.split(':').next().unwrap()),&id).await.map_err(|e|storage(&e))?;
            created+=usize::from(new);
        }
        let mut r=envelope(company);r["created"]=json!(created);r["unchanged_replays"]=json!(batch.records.len()-created);r["submitted"]=json!(batch.records.len());Ok(r)
    }
    async fn preserved_pdf_values(&self,company:&str,ids:&[String],required:bool,verify_pdf:bool)->Result<Vec<Option<Value>>> {
        if ids.len()>MAX_RECORDS||ids.iter().any(|s|!valid_id(s)){return Err(bad("Preserved figure lookup exceeds closed bound"))}
        let mut rows=Vec::new();let mut total=0;
        for id in ids {
            let raw=self.store.get(&key(company,id)).await.map_err(|e|storage(&e))?;
            let Some(raw)=raw else {if required{return Err(missing("Preserved PDF observation missing"))}rows.push(None);continue};
            total+=raw.len();if total>MAX_RESPONSE{return Err(storage("Preserved figure response exceeds bound"))}
            let v:Value=serde_json::from_str(&raw).map_err(|_|storage("Preserved observation unreadable"))?;
            let(exact,_,_)=pinned(&v).map_err(|_|storage("Stored preserved observation differs from closed evidence"))?;
            if raw!=exact||identity(&v)?!=*id{return Err(storage("Stored preserved observation identity or bytes differ"))}
            if verify_pdf{self.prepare_preserved_pdf_figures(company,std::slice::from_ref(&v)).await?;}
            rows.push(Some(v));
        }Ok(rows)
    }
    pub async fn test_copy_read_preserved_pdf_figures(&self,company:&str,body:PreservedPdfFigureLookup)->Result<Value> {
        self.test_copy_guard(company,body.test_copy)?;if body.record_ids.is_empty(){return Err(bad("Preserved figure identities required"))}
        let rows=self.preserved_pdf_values(company,&body.record_ids,body.require_present,true).await?;
        let mut r=envelope(company);r["record_ids"]=json!(body.record_ids);r["records"]=json!(rows);Ok(r)
    }
    pub async fn test_copy_client_preserved_pdf_figures(&self,company:&str,client:&str,page:SourceFactPage)->Result<Value> {
        self.test_copy_guard(company,true)?;if !number(client)||page.limit==0||page.limit>100||page.source_return_year.is_some_and(|y|!(1990..=2100).contains(&y)){return Err(bad("Invalid preserved PDF client or year filter"))}
        let mut ids=self.store.smembers(&member(company,client)).await.map_err(|e|storage(&e))?;ids.sort();
        if ids.len()>MAX_RECORDS||ids.iter().any(|id|!valid_id(id)||id.split(':').next()!=Some(client)){return Err(storage("Preserved client index differs"))}
        if let Some(y)=page.source_return_year{ids.retain(|id|id.split(':').nth(1).and_then(|s|s.parse::<u16>().ok())==Some(y));}
        let total=ids.len();let ids:Vec<_>=ids.into_iter().skip(page.offset).take(page.limit).collect();let records=self.preserved_pdf_values(company,&ids,true,true).await?;
        let mut r=envelope(company);r["client_number"]=json!(client);r["kind"]=json!("preserved-pdf-figures");r["source_return_year"]=json!(page.source_return_year);r["offset"]=json!(page.offset);r["total"]=json!(total);r["records"]=json!(records);Ok(r)
    }
    // Read-time overlay only. The original native indexes and client profiles
    // are not written. A later native import may independently supply this year.
    pub(super) async fn preserved_pdf_figure_years(&self,company:&str)->Result<BTreeMap<String,BTreeSet<u16>>> {
        let mut ids=self.store.smembers(&index(company,"all")).await.map_err(|e|storage(&e))?;ids.sort();
        let rows=self.preserved_pdf_values(company,&ids,true,false).await?;let mut out:BTreeMap<String,BTreeSet<u16>>=BTreeMap::new();
        for v in rows.into_iter().flatten(){out.entry(text(&v,"client_number")?.into()).or_default().insert(v["source_return_year"].as_u64().unwrap()as u16);}Ok(out)
    }
    pub async fn test_copy_preserved_pdf_figure_summary(&self,company:&str)->Result<Value> {
        self.test_copy_guard(company,true)?;let mut ids=self.store.smembers(&index(company,"all")).await.map_err(|e|storage(&e))?;ids.sort();
        let rows=self.preserved_pdf_values(company,&ids,true,false).await?;let pins:BTreeSet<_>=self.store.smembers(&index(company,"digests")).await.map_err(|e|storage(&e))?.into_iter().collect();
        let mut exact=BTreeSet::new();let mut years=BTreeSet::new();let mut n=Counts::default();
        for v in rows.into_iter().flatten(){let(_,d,c)=pinned(&v)?;exact.insert(d);years.insert((text(&v,"client_number")?.to_string(),v["source_return_year"].as_u64().unwrap()));n.nonnull+=c.nonnull;n.zero+=c.zero;n.blank+=c.blank;n.held+=c.held;n.notlocated+=c.notlocated;}
        if pins!=exact{return Err(storage("Preserved figure summary digest index differs"))}
        let mut r=envelope(company);r["source_package_sha256"]=json!(profile().source_package_sha256);r["PDF_observation_records"]=json!(ids.len());r["owner_years"]=json!(years.len());r["printed_amount_observations"]=json!(n.nonnull);r["explicit_printed_zeros"]=json!(n.zero);r["identified_printed_blanks"]=json!(n.blank);r["held_printed_amounts"]=json!(n.held);r["unlocated_printed_lines"]=json!(n.notlocated);Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic()->Value {
        let pdf=json!({"client_number":"100","tax_year":2009,"sha256":"a".repeat(64),"bytes":123,"page_count":2,"page_labels":["Synthetic first","Synthetic second"],"original_print_output":true,"immutable":true});
        let mut anchors=serde_json::Map::new();for k in ANCHORS{anchors.insert(k.into(),json!("b".repeat(64)));}
        anchors.insert("pdf_record_sha256".into(),json!(digest(encode(&pdf).unwrap().as_bytes())));anchors.insert("printed_line_authority_sha256".into(),json!(LINE_AUTHORITY_SHA));anchors.insert("native_context_absence_audit_sha256".into(),json!(ABSENCE_AUDIT_SHA));anchors.insert("source_geometry_sha256".into(),json!(digest(b"{}")));
        let obs:Vec<_>=LINES.iter().enumerate().map(|(i,line)|{
            let state=match i{0..=2=>"single_distinct_printed_amount",3=>"identified_printed_blank",4=>"printed_line_not_located",_=>"held_ambiguous_printed_amount"};
            let cents=if i<3{json!(["0","-1","12345"][i])}else{Value::Null};let dec=if i<3{json!(["0.00","-0.01","123.45"][i])}else{Value::Null};
            let c=json!({"page_index":0,"printed_code":printed_code(2009,line).unwrap(),"line_code_bbox":[10,10,20,20],"amount":dec,"state":if i<3{"printed_amount_present"}else if i==3{"identified_printed_amount_blank"}else{"held_ambiguous_money_tokens"}});
            let mut candidates=if i==4{vec![]}else{vec![c.clone()]};if i==0{let mut carry=c;carry["page_index"]=json!(1);candidates.push(carry);}
            let raw=serde_json::to_string(&candidates).unwrap();json!({"line_code":line,"printed_line_code":printed_code(2009,line).unwrap(),"state":state,"printed_cents":cents,"printed_decimal":dec,"printed_candidates_sha256":digest(raw.as_bytes()),"printed_candidates_json":raw})
        }).collect();
        json!({"schema":"dtmax-preserved-PDF-figure-observations/v1","preserved_figure_profile_sha256":profile().profile_sha256,"client_number":"100","source_return_year":2009,"source_kind":"preserved_PDF","native_context_state":"absent_at_pinned_source_audit","pdf":pdf,"anchors":anchors,"geometry_evidence_json":"{}","observations":obs,"source_fields_only":true,"recalculated":false,"as_filed_verified":false,"preferred_version_selected":false,"financial_values_approved":false})
    }
    #[test]
    fn profile_is_separate_and_uses_explicit_year_specific_printed_bindings() {
        assert_eq!(profile().record_count,5);assert_eq!(profile().owner_year_count,4);
        assert_eq!(profile().allowed_record_sha256.values().map(|n|n.nonnull).sum::<usize>(),25);assert_eq!(profile().allowed_record_sha256.values().map(|n|n.blank).sum::<usize>(),5);
        assert_eq!(digest(include_bytes!("printed_cached_figure_profile.json")),"f3593686959075ed6dd0c7224fd88d5b4e20ab6ca732519d166f4ab6fd4cda95");
        for y in [2009,2010,2011,2012]{assert_eq!(printed_code(y,"15000").unwrap(),"150");assert_eq!(printed_code(y,"48400").unwrap(),"484");assert_eq!(printed_code(y,"48500").unwrap(),"485");}
        assert!(printed_code(2008,"15000").is_err());assert!(printed_code(2010,"99900").is_err());
    }
    #[test]
    fn printed_money_keeps_zero_negative_blank_and_held_states_separate() {
        assert_eq!(amount("0").unwrap(),0);assert_eq!(decimal(-1),"-0.01");assert_eq!(decimal(i64::MIN),"-92233720368547758.08");
        for s in ["-0","01","1.0","9223372036854775808"]{assert!(amount(s).is_err());}
        let v=synthetic();let before=v.clone();let n=shape(&v).unwrap();assert_eq!((n.nonnull,n.zero,n.blank,n.held,n.notlocated),(3,1,1,1,1));assert!(v==before);assert!(pinned(&v).is_err());
    }
    #[test]
    fn unqualified_context_cached_origin_cross_code_and_null_as_zero_are_rejected() {
        for variant in ["context","family","cached","wrong_code","null_zero","blank_zero","disagree","bbox"] {
            let mut v=synthetic();match variant {
                "context"=>v["native_context_state"]=json!("absent"),"family"=>v["family_file_id"]=json!("999"),"cached"=>v["pdf"]["export_provenance"]=json!({"kind":"cached_native_export"}),"wrong_code"=>v["observations"][0]["printed_line_code"]=json!("15000"),"null_zero"=>v["observations"][0]["printed_cents"]=Value::Null,"blank_zero"=>v["observations"][3]["printed_cents"]=json!("0"),
                _=>{let o=&mut v["observations"][0];let mut c:Vec<Value>=serde_json::from_str(o["printed_candidates_json"].as_str().unwrap()).unwrap();if variant=="disagree"{c[1]["amount"]=json!("1.00");}else{c[0]["line_code_bbox"]=json!([10,10,10,20]);}let raw=serde_json::to_string(&c).unwrap();o["printed_candidates_sha256"]=json!(digest(raw.as_bytes()));o["printed_candidates_json"]=json!(raw);}
            }assert!(shape(&v).is_err(),"Invalid preserved source semantics accepted");
        }
    }
    #[test]
    fn optional_all_five_private_records_have_exact_closed_shapes_and_counts() {
        let Ok(path)=std::env::var("NXT_PRESERVED_PDF_OBSERVATIONS")else{return};let raw=std::fs::read(path).unwrap();assert_eq!(digest(&raw),profile().source_package_sha256);
        let rows:Vec<Value>=raw.split(|b|*b==b'\n').filter(|s|!s.is_empty()).map(|s|serde_json::from_slice(s).unwrap()).collect();assert_eq!(rows.len(),5);
        let mut count=Counts::default();for row in rows {let(_,_,n)=pinned(&row).unwrap_or_else(|_|panic!("Private preserved source row shape/pin mismatch"));count.nonnull+=n.nonnull;count.blank+=n.blank;}
        assert_eq!((count.nonnull,count.blank),(25,5));println!("PRIVATE_PRESERVED_PDF_SHAPES_EXACT: records=5; numeric=25; blank=5");
    }
}
