//! Independent Canadian tax workpapers. No DT Max runtime or filing dependency.
pub mod calculations;
pub mod catalog;
pub mod corporate;
pub mod models;
pub mod parallel;
pub mod rates; pub mod t183;
pub mod slips;
pub mod gifi_map;
mod routes;
pub use routes::router;

use crate::{referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{Duration, NaiveDate, SecondsFormat, Utc};
use models::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const STATE_KEY: &str = "tax:state:v1";
const MAX_STATE_BYTES: usize = 8 * 1024 * 1024;
pub type TResult<T> = Result<T, ApiError>;
pub struct Tax { pub store: Store }
fn bad(message: impl Into<String>) -> ApiError { ApiError(StatusCode::BAD_REQUEST, message.into()) }
fn conflict(message: impl Into<String>) -> ApiError { ApiError(StatusCode::CONFLICT, message.into()) }
fn missing() -> ApiError { ApiError(StatusCode::NOT_FOUND, "tax return not found".into()) }
fn now() -> String { Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true) }
fn id(prefix: &str) -> String { format!("{prefix}_{}", uuid::Uuid::new_v4().simple()) }
fn hash(value: &impl serde::Serialize) -> TResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| bad("cannot serialize workpaper"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
fn text(value: &str, field: &str, max: usize, required: bool) -> TResult<()> {
    if value.len() > max || (required && value.trim().is_empty()) || value.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(bad(format!("invalid {field}; maximum {max} bytes")));
    }
    Ok(())
}
fn date(value: &str) -> TResult<NaiveDate> {
    if value.len() != 10 { return Err(bad("dates must use YYYY-MM-DD")); }
    let d = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| bad("invalid calendar date"))?;
    if d.format("%Y-%m-%d").to_string() != value { return Err(bad("dates must use YYYY-MM-DD")); }
    Ok(d)
}
fn program(value: &str) -> TResult<String> {
    let p = value.to_ascii_uppercase();
    if !["T1", "T2", "T3", "T5013"].contains(&p.as_str()) { return Err(bad("program must be T1, T2, T3 or T5013")); }
    Ok(p)
}
fn bounded_json(v: &Value, depth: usize) -> TResult<()> {
    if depth > 8 { return Err(bad("input nesting limit exceeded")); }
    match v {
        Value::String(s) => text(s, "input text", 32_000, false)?,
        Value::Number(n) if n.as_i64().is_none() => return Err(bad("numeric inputs must be integer cents or whole-number parameters")),
        Value::Array(a) => { if a.len() > 500 { return Err(bad("input array limit exceeded")); } for x in a { bounded_json(x, depth + 1)?; } },
        Value::Object(o) => { if o.len() > 150 { return Err(bad("input field limit exceeded")); } for (k,x) in o { text(k, "input key", 100, true)?; bounded_json(x, depth + 1)?; } },
        _ => (),
    }
    Ok(())
}
fn validate(mut input: ReturnInput) -> TResult<ReturnInput> {
    input.program = program(&input.program)?;
    if !(2000..=2100).contains(&input.tax_year) { return Err(bad("tax year must be between 2000 and 2100")); }
    input.jurisdiction = input.jurisdiction.to_ascii_uppercase();
    if !["AB","BC","MB","NB","NL","NS","NT","NU","ON","PE","QC","SK","YT","CA","CA-FED"].contains(&input.jurisdiction.as_str()) { return Err(bad("invalid jurisdiction")); }
    text(&input.name, "return name", 200, true)?; text(&input.client_ref, "client reference", 100, false)?; text(&input.notes, "notes", 16_000, false)?;
    input.name = input.name.trim().into(); input.client_ref = input.client_ref.trim().into();
    match (&input.period_start, &input.period_end) {
        (Some(start),Some(end)) => {
            let a = date(start)?; let b = date(end)?;
            if b < a || (b-a).num_days() > 370 { return Err(bad("fiscal period must be ordered and no longer than 371 days")); }
            if !end.starts_with(&input.tax_year.to_string()) { return Err(bad("tax year must match the fiscal period end")); }
            if input.program == "T1" && (start != &format!("{}-01-01", input.tax_year) || end != &format!("{}-12-31",input.tax_year)) { return Err(bad("this T1 workpaper supports a full calendar year only")); }
        },
        (None,None) => (),
        _ => return Err(bad("supply both fiscal period dates or neither")),
    }
    if !input.inputs.is_object() { return Err(bad("inputs must be an object")); }
    bounded_json(&input.inputs, 0)?;
    if serde_json::to_vec(&input.inputs).map_err(|_| bad("invalid inputs"))?.len() > 128 * 1024 { return Err(bad("return inputs exceed 128 KiB")); }
    if input.parties.len() > 100 { return Err(bad("at most 100 related parties are allowed")); }
    if let Some(profile)=&input.corporate {
        if input.program!="T2" { return Err(bad("a corporate profile is supported for T2 only")); }
        corporate::validate(profile).map_err(bad)?;
    }
    let mut ids = HashSet::new();
    for party in &mut input.parties {
        if party.id.is_empty() { party.id = id("party"); }
        text(&party.id,"party id",80,true)?; text(&party.role,"party role",80,true)?; text(&party.name,"party name",200,true)?; text(&party.identifier,"party identifier",100,false)?;
        if !ids.insert(party.id.clone()) { return Err(bad("duplicate party id")); }
        if !(0..=10_000).contains(&party.share_basis_points) || party.allocation_cents < 0 { return Err(bad("invalid party share or allocation")); }
    }
    Ok(input)
}
fn input_hash(r: &TaxReturn) -> TResult<String> { hash(&ReturnInput::from(r)) }
fn assert_revision(r: &TaxReturn, expected: u64) -> TResult<()> {
    if r.revision != expected { Err(conflict("return changed; reload before trying again")) } else { Ok(()) }
}
fn audit(s: &mut TaxState, return_id: &str, action: &str, detail: impl Into<String>) {
    // Append-only, newest first. Events are never truncated or edited; capacity is bounded by the state size limit.
    let prev_hash=s.audit.first().map(|e|if e.hash.is_empty(){"pre-chain".to_string()}else{e.hash.clone()}).unwrap_or_else(||"genesis".into());
    let mut e=AuditEvent { id:id("taxaudit"), return_id:return_id.into(), at:now(), action:action.into(), detail:detail.into(), prev_hash, hash:String::new() };
    e.hash=audit_hash(&e); s.audit.insert(0,e);
}
fn audit_hash(e: &AuditEvent) -> String { hex::encode(Sha256::digest(format!("{}|{}|{}|{}|{}|{}",e.id,e.return_id,e.at,e.action,e.detail,e.prev_hash).as_bytes())) }
/// Verifies the audit hash chain from oldest to newest. Pre-chain events (saved before hashing existed) are reported, not verified.
pub fn verify_audit(events: &[AuditEvent]) -> Value {
    let mut prev: Option<String>=None; let mut chained=0; let mut legacy=0;
    for e in events.iter().rev() {
        if e.hash.is_empty() { legacy+=1; prev=None; continue; }
        let expected_prev=prev.clone().unwrap_or_else(||if chained==0 && legacy==0 {"genesis".into()} else {"pre-chain".into()});
        if e.prev_hash!=expected_prev || audit_hash(e)!=e.hash { return json!({"valid":false,"broken_at":e.id,"chained":chained,"pre_chain":legacy}); }
        chained+=1; prev=Some(e.hash.clone());
    }
    json!({"valid":true,"chained":chained,"pre_chain":legacy})
}
fn record(input: &ReturnInput, record_id: &str, at: &str) -> TaxReturn {
    TaxReturn { id:record_id.into(), program:input.program.clone(), tax_year:input.tax_year, jurisdiction:input.jurisdiction.clone(), name:input.name.clone(), client_ref:input.client_ref.clone(), period_start:input.period_start.clone(), period_end:input.period_end.clone(), inputs:input.inputs.clone(), parties:input.parties.clone(), corporate:input.corporate.clone(), notes:input.notes.clone(), revision:1, input_revision:1, status:"draft".into(), calculation:None, calculations:Vec::new(), review:None, reviews:Vec::new(), rollforward_from:None, created_at:at.into(), updated_at:at.into(), parallel_comparisons:Vec::new(), prior_lines:Vec::new(), preparer_signoff:None, filed_record:None }
}

impl Tax {
    pub fn new(store: Store) -> Self { Self { store } }
    async fn load(&self) -> TResult<TaxState> { Ok(self.store.get_json(STATE_KEY).await?.unwrap_or_default()) }
    async fn mutate<T>(&self, mut f: impl FnMut(&mut TaxState) -> TResult<T>) -> TResult<T> {
        for _ in 0..16 {
            let raw = self.store.get(STATE_KEY).await?;
            let mut state: TaxState = match &raw { Some(v) => serde_json::from_str(v).map_err(|_| conflict("tax state is corrupt; recovery is required"))?, None => TaxState::default() };
            let out = f(&mut state)?;
            for r in &state.returns {
                if serde_json::to_vec(r).map_err(|_|bad("return could not be encoded"))?.len()>1024*1024 { return Err(conflict("return history exceeds the 1 MiB record limit; preserve/export the workpaper before starting a separate record")); }
            }
            let next = serde_json::to_string(&state).map_err(|_| bad("tax state could not be encoded"))?;
            if next.len() > MAX_STATE_BYTES { return Err(conflict("tax workspace capacity exceeded (8 MiB)")); }
            if self.store.compare_exchange(STATE_KEY,raw.as_deref(),&next).await? { return Ok(out); }
        }
        Err(conflict("tax workspace changed repeatedly; retry after reloading"))
    }
    pub async fn list(&self, filter: Option<&str>) -> TResult<Vec<Value>> {
        let filter = filter.map(program).transpose()?;
        Ok(self.load().await?.returns.into_iter().filter(|r| filter.as_ref().is_none_or(|p| &r.program == p)).map(|r| {
            let calculation=r.calculation.as_ref().map(|c|json!({"id":c.id,"input_revision":c.input_revision,"created_at":c.created_at,"result":{"supported":c.result["supported"]}}));
            json!({"id":r.id,"program":r.program,"tax_year":r.tax_year,"jurisdiction":r.jurisdiction,"name":r.name,"client_ref":r.client_ref,"period_start":r.period_start,"period_end":r.period_end,"revision":r.revision,"input_revision":r.input_revision,"status":r.status,"created_at":r.created_at,"updated_at":r.updated_at,"calculation":calculation})
        }).collect())
    }
    pub async fn get(&self, record_id: &str) -> TResult<TaxReturn> { self.load().await?.returns.into_iter().find(|r| r.id == record_id).ok_or_else(missing) }
    pub async fn summary(&self) -> TResult<Value> {
        let s = self.load().await?; let n = |status: &str| s.returns.iter().filter(|r|r.status == status).count();
        let by_program = ["T1","T2","T3","T5013"].into_iter().map(|p|(p.to_string(),json!(s.returns.iter().filter(|r|r.program == p).count()))).collect::<serde_json::Map<_,_>>();
        Ok(json!({"returns":s.returns.len(),"total":s.returns.len(),"drafts":n("draft"),"calculated":n("calculated"),"in_review":n("in_review"),"approved":n("approved"),"archived":n("archived"),"by_program":by_program,"storage":self.store.backend_name(),"auth_required":crate::referral::routes::auth_required(),"filing_enabled":false,"calculation_years":[2025],"engine":"Independent NXT engine","access_model":"shared workspace credential; labels are not verified individual identities"}))
    }
    pub fn capabilities() -> Value {
        json!({"engine":"independent","filing_enabled":false,"efile":efile_status(),"rate_tables":rates::catalog(),"filed_status":"human record of filing done outside NXT; requires approved return, signed T183 envelope id and reference","certification":"not_certified","parallel_compare":"supplied dtmax or webtax lines only; vendor file bytes are not fetched","sdk":{"mode":"reference_only","connected":false},"exports":["nxt-tax-json","printable-workpaper-html"],"programs":catalog::catalog()["programs"],"limitations":["Native calculations cover only the explicit ruleset cases in the catalog, not every DT Max form, province, year or election.","Output is an internal workpaper, not a CRA/Revenu Québec filing or certified form.","A parallel comparison does not replace the old system and does not file.","Related parties are metadata; use calculation input fields for supported allocations.","Client/company isolation and individual approval authority are not implemented by the shared workspace credential.","No DT Max client database, SDK runtime, license secret or filing credential is accessed."]})
    }
    pub async fn save(&self, existing: Option<&str>, input: ReturnInput) -> TResult<TaxReturn> {
        let input = validate(input)?; let record_id = existing.map(str::to_owned).unwrap_or_else(||id("tax")); let at=now();
        if existing.is_some() && input.expected_revision.is_none() { return Err(bad("expected_revision is required when editing")); }
        if existing.is_none() && input.expected_revision.is_some() { return Err(bad("expected_revision cannot be supplied on creation")); }
        self.mutate(|s| {
            let prior=s.returns.iter().find(|r|r.id==record_id).cloned();
            if existing.is_some() && prior.is_none() { return Err(missing()); }
            if prior.is_none() && s.returns.len() >= 500 { return Err(conflict("tax return capacity reached (500)")); }
            let mut r=record(&input,&record_id,&at);
            if let Some(previous)=prior {
                assert_revision(&previous,input.expected_revision.unwrap())?;
                if ["approved","filed","archived"].contains(&previous.status.as_str()) { return Err(conflict("reopen the return to draft before editing")); }
                if previous.program != input.program || previous.tax_year != input.tax_year { return Err(bad("program and year are immutable; create or roll forward a return")); }
                r.revision=previous.revision+1; r.input_revision=previous.input_revision+1; r.created_at=previous.created_at; r.rollforward_from=previous.rollforward_from; r.calculations=previous.calculations; r.reviews=previous.reviews; r.parallel_comparisons=previous.parallel_comparisons; r.prior_lines=previous.prior_lines;
            }
            s.returns.retain(|x|x.id!=record_id); s.returns.push(r.clone()); audit(s,&record_id,if existing.is_some(){"edited"}else{"created"},"Input revision saved; prior calculation/review invalidated"); Ok(r)
        }).await
    }
    pub async fn set_prior_lines(&self, record_id: &str, expected: u64, lines: Vec<PriorLine>) -> TResult<Value> {
        if lines.len() > 40 { return Err(bad("at most 40 prior-year lines are allowed")); }
        let mut seen = HashSet::new();
        for line in &lines {
            if line.code.is_empty() || line.code.len() > 16 || !line.code.chars().all(|c| c.is_ascii_alphanumeric()) || !seen.insert(line.code.clone()) {
                return Err(bad("prior-year line codes must be unique short alphanumeric labels"));
            }
        }
        let stored = lines.clone();
        let record = self.mutate(|s| {
            let r = s.returns.iter_mut().find(|r| r.id == record_id).ok_or_else(missing)?;
            assert_revision(r, expected)?;
            if r.program != "T1" { return Err(bad("prior-year columns are stored on T1 returns")); }
            if ["approved", "filed", "archived"].contains(&r.status.as_str()) { return Err(conflict("reopen the return to draft before editing prior-year columns")); }
            r.prior_lines = stored.clone();
            r.revision += 1;
            r.updated_at = now();
            let out = r.clone();
            audit(s, record_id, "prior_lines", "Stored operator prior-year amounts. Credits were not transferred and filing stays off.");
            Ok(out)
        }).await?;
        Ok(json!({"return": record, "transfers_credits": false, "filing_enabled": false, "keywords_loaded": false, "write_back": false, "authoritative_system": "dtmax"}))
    }
    pub async fn calculate(&self, record_id: &str, request: RevisionInput) -> TResult<TaxReturn> {
        if let Some(key)=&request.idempotency_key { text(key,"idempotency key",160,true)?; }
        let original=self.get(record_id).await?;
        let fingerprint=input_hash(&original)?;
        if let Some(key)=&request.idempotency_key {
            if let Some(prior)=original.calculations.iter().find(|c|c.idempotency_key.as_ref()==Some(key)) {
                if prior.input_hash!=fingerprint || prior.input_revision!=original.input_revision { return Err(conflict("idempotency key was already used for another input revision")); }
                if original.calculation.as_ref().is_some_and(|c|c.id==prior.id) { return Ok(original); }
                return Err(conflict("that calculation has been superseded; use a new idempotency key"));
            }
        }
        if original.calculation.as_ref().is_some_and(|c|c.input_document.is_some() && c.request_revision==request.expected_revision && c.input_hash==fingerprint && c.input_revision==original.input_revision) { return Ok(original); }
        assert_revision(&original,request.expected_revision)?;
        if ["approved","filed","archived"].contains(&original.status.as_str()) { return Err(conflict("reopen the return before recalculating")); }
        let result=calculations::calculate(&original.program,original.tax_year,&original.jurisdiction,&original.inputs).map_err(bad)?;
        let mut result=serde_json::to_value(result).map_err(|_|bad("calculation could not be encoded"))?;
        if original.program=="T2" {
            let issue=match (&original.period_start,&original.period_end) {
                (Some(start),Some(end)) => {
                    let days=(date(end)?-date(start)?).num_days()+1;
                    (original.inputs["fiscal_days"].as_i64().unwrap_or(365)!=days).then_some("Fiscal period dates and the calculation's fiscal_days must agree.")
                },
                _ => Some("Record fiscal period start/end dates before reviewing a corporate tax workpaper."),
            };
            if let Some(message)=issue {
                result["supported"]=json!(false);
                if let Some(d)=result["diagnostics"].as_array_mut(){d.push(json!({"code":"fiscal_period","severity":"error","message":message,"field":"fiscal_days","blocks_review":true}));}
            }
            if let Some(profile)=&original.corporate {
                let analysis=corporate::analyze(profile,original.inputs["taxable_income_cents"].as_i64().unwrap_or(0),&original.jurisdiction,original.inputs["small_business_eligible"].as_bool().unwrap_or(false));
                if let Some(diags)=analysis["diagnostics"].as_array(){
                    if diags.iter().any(|d|d["blocks_review"]==true){result["supported"]=json!(false);}
                    if let Some(target)=result["diagnostics"].as_array_mut(){target.extend(diags.clone());}
                }
                result["corporate_analysis"]=analysis;
            }
        }
        let output_hash=hash(&result)?; let snapshot_id=id("calculation"); let at=now();
        self.mutate(|s| {
            let r=s.returns.iter_mut().find(|r|r.id==record_id).ok_or_else(missing)?;
            // A retried or racing calculation of exactly the same rules and inputs is one snapshot.
            if r.calculation.as_ref().is_some_and(|c|c.input_document.is_some() && c.input_hash==fingerprint && c.output_hash==output_hash && c.input_revision==r.input_revision) { return Ok(r.clone()); }
            assert_revision(r,request.expected_revision)?;
            if input_hash(r)? != fingerprint { return Err(conflict("inputs changed while calculating")); }
            if r.calculations.len()>=20 { return Err(conflict("calculation history capacity reached (20); preserve/export the return before creating a new workpaper")); }
            let snapshot=CalculationSnapshot{id:snapshot_id.clone(),input_revision:r.input_revision,request_revision:request.expected_revision,idempotency_key:request.idempotency_key.clone(),input_document:Some(ReturnInput::from(&*r)),input_hash:fingerprint.clone(),output_hash:output_hash.clone(),created_at:at.clone(),result:result.clone()};
            r.calculations.push(snapshot.clone()); r.calculation=Some(snapshot);
            r.review=None; r.status="calculated".into(); r.revision+=1; r.updated_at=at.clone(); let out=r.clone();
            audit(s,record_id,"calculated",format!("Snapshot {}",snapshot_id)); Ok(out)
        }).await
    }
    pub async fn transition(&self, record_id: &str, input: TransitionInput) -> TResult<TaxReturn> {
        text(&input.note,"review note",4000,false)?; text(&input.reviewer,"reviewer label",120,false)?;
        if !["draft","in_review","approved","filed","archived"].contains(&input.status.as_str()) { return Err(bad("invalid transition status")); }
        text(&input.filing_reference,"filing reference",120,false)?; text(&input.t183_envelope_id,"T183 envelope id",120,false)?;
        let at=now();
        self.mutate(|s| {
            let r=s.returns.iter_mut().find(|r|r.id==record_id).ok_or_else(missing)?; assert_revision(r,input.expected_revision)?;
            if input.status==r.status { return Ok(r.clone()); }
            if r.status=="filed" && input.status!="archived" { return Err(conflict("a return recorded as filed can only be archived; amendments start from a new return")); }
            if input.status=="filed" {
                if r.status!="approved" { return Err(conflict("only an approved return can be recorded as filed")); }
                if input.reviewer.trim().is_empty() || input.note.trim().is_empty() { return Err(bad("operator label and note are required to record filing")); }
                if input.t183_envelope_id.trim().is_empty() { return Err(conflict("a signed T183/T183CORP envelope id is required before recording filing")); }
                if input.filing_reference.trim().is_empty() { return Err(bad("the CRA confirmation/reference received outside NXT is required")); }
                r.filed_record=Some(FiledRecord{by:input.reviewer.trim().into(),at:at.clone(),reference:input.filing_reference.trim().into(),t183_envelope_id:input.t183_envelope_id.trim().into()});
            } else if input.status=="draft" {
                if input.note.trim().is_empty(){return Err(bad("a reason is required to reopen a return"));} r.review=None; r.calculation=None; r.preparer_signoff=None;
            } else if input.status=="archived" {
                if input.note.trim().is_empty(){return Err(bad("an archive reason is required"));}
            } else {
                let c=r.calculation.as_ref().ok_or_else(||conflict("calculate the current inputs first"))?;
                if c.input_revision!=r.input_revision || c.input_hash!=input_hash(r)? || c.input_document.as_ref().map(hash).transpose()?.as_deref()!=Some(c.input_hash.as_str()) || c.output_hash!=hash(&c.result)? { return Err(conflict("calculation is stale or invalid")); }
                if c.result["supported"]!=true || c.result["diagnostics"].as_array().is_none_or(|a|a.iter().any(|d|d["blocks_review"]==true || d["severity"]=="error")) { return Err(conflict("resolve blocking calculation diagnostics before review")); }
                if input.status=="in_review" && r.status!="calculated" { return Err(conflict("only a calculated return can enter review")); }
                if input.status=="in_review" {
                    if input.reviewer.trim().is_empty() { return Err(bad("preparer sign-off label is required to send a return to review")); }
                    r.preparer_signoff=Some(SignOff{by:input.reviewer.trim().into(),at:at.clone()});
                }
                if input.status=="approved" {
                    if r.status!="in_review" { return Err(conflict("return must enter review before approval")); }
                    if input.note.trim().is_empty() || input.reviewer.trim().is_empty(){return Err(bad("reviewer label and review note are required"));}
                    if r.preparer_signoff.as_ref().is_some_and(|p|p.by.trim().eq_ignore_ascii_case(input.reviewer.trim())) { return Err(conflict("reviewer must be a different person from the preparer")); }
                    if r.reviews.len()>=20 { return Err(conflict("review history capacity reached (20)")); }
                    let review=Review{at:at.clone(),reviewer:input.reviewer.trim().into(),note:input.note.trim().into(),input_hash:c.input_hash.clone(),output_hash:c.output_hash.clone()};
                    r.reviews.push(review.clone()); r.review=Some(review);
                }
            }
            r.status=input.status.clone(); r.revision+=1; r.updated_at=at.clone(); let out=r.clone(); audit(s,record_id,"transition",format!("Status {} (internal workpaper only). Operator: {}. Reason: {}",input.status,input.reviewer.trim(),input.note.trim())); Ok(out)
        }).await
    }
    pub async fn rollforward(&self, record_id: &str, input: RollforwardInput) -> TResult<TaxReturn> {
        let prior=self.get(record_id).await?; assert_revision(&prior,input.expected_revision)?;
        if input.tax_year!=prior.tax_year+1 { return Err(bad("roll forward to the next tax year only")); }
        let mut next=ReturnInput::from(&prior); next.tax_year=input.tax_year; next.name=input.name.unwrap_or_else(||format!("{} — {}",prior.name,input.tax_year)); next.period_start=None; next.period_end=None;
        next.inputs=carryforward_inputs(&prior.inputs); next.notes="Rolled forward profile only. Income, allocations, calculations and approvals were cleared. Closing carryforward balances (*_closing_cents) became *_opening_cents and must be verified against the notice of assessment.".into();
        if let Some(profile)=&mut next.corporate { profile.gifi.clear(); profile.adjustments.clear(); profile.reconciliation_confirmed=false; profile.notes.clear(); }
        for party in &mut next.parties { party.id=id("party"); party.allocation_cents=0; }
        let next=validate(next)?; let new_id=id("tax"); let at=now();
        self.mutate(|s| {
            let current=s.returns.iter().find(|r|r.id==record_id).ok_or_else(missing)?; assert_revision(current,input.expected_revision)?;
            if s.returns.len()>=500 { return Err(conflict("tax return capacity reached")); }
            if s.returns.iter().any(|r|r.rollforward_from.as_deref()==Some(record_id)&&r.tax_year==input.tax_year){return Err(conflict("this return has already been rolled forward"));}
            let mut r=record(&next,&new_id,&at); r.rollforward_from=Some(record_id.into()); s.returns.push(r.clone()); audit(s,&new_id,"rolled_forward","Profile copied; all year-specific financial inputs cleared"); Ok(r)
        }).await
    }
    pub async fn audit(&self) -> TResult<Vec<AuditEvent>> { Ok(self.load().await?.audit) }
    pub async fn package(&self, record_id: &str) -> TResult<Value> {
        let r=self.get(record_id).await?;
        Ok(json!({"format":"nxt-tax-return/v1","classification":"internal_workpaper_not_for_filing","exported_at":now(),"return":ReturnInput::from(&r),"calculation_snapshot":r.calculation,"calculation_history":r.calculations,"review":r.review,"review_history":r.reviews,"input_hash":input_hash(&r)?,"filing_enabled":false}))
    }
    pub async fn preview(&self, document: Value) -> TResult<Value> {
        self.preview_internal(document,false).await
    }
    pub(crate) async fn preview_from_engagement(&self, document: Value) -> TResult<Value> {
        self.preview_internal(document,true).await
    }
    async fn preview_internal(&self, mut document: Value, dedupe: bool) -> TResult<Value> {
        if let Some(s)=document.as_str() { document=serde_json::from_str(s).map_err(|_|bad("document must be valid JSON"))?; }
        if document["format"]!="nxt-tax-return/v1" { return Err(bad("only nxt-tax-return/v1 workpaper packages are supported; DT Max database/SDK files are not accepted")); }
        let mut input:ReturnInput=serde_json::from_value(document["return"].clone()).map_err(|_|bad("invalid return document"))?;
        input.expected_revision=None; let input=validate(input)?;
        // This private flag is selected by the native bridge, never by fields in
        // an uploaded package. Its hash namespace separates trusted bridge
        // retries from independently requested public import previews.
        let source_hash=if dedupe {hash(&json!({"kind":"nxt-engagement-tax-proposal/v1","document":document}))?}else{hash(&document)?};
        let preview_id=id("taximport"); let at=now(); let expires=(Utc::now()+Duration::minutes(30)).to_rfc3339_opts(SecondsFormat::Secs,true);
        let summary=json!({"program":input.program,"tax_year":input.tax_year,"jurisdiction":input.jurisdiction,"name":input.name,"client_ref":input.client_ref,"input_fields":input.inputs.as_object().map_or(0,|o|o.len()),"parties":input.parties.len()});
        let preview=ImportPreview{id:preview_id.clone(),source_hash,created_at:at,expires_at:expires,summary,warnings:vec!["Creates a new draft. Imported calculations, review decisions and filing claims are never trusted.".into()],committed_return_id:None,document:input};
        self.mutate(|s| {
            s.imports.retain(|p|p.expires_at>now() || p.committed_return_id.is_some());
            if let Some(existing)=s.imports.iter().find(|p|dedupe&&p.source_hash==preview.source_hash) {
                let mut out=serde_json::to_value(existing).map_err(|_|bad("invalid stored preview"))?;
                out.as_object_mut().unwrap().remove("document");
                return Ok(out);
            }
            if s.imports.len()>=20 { return Err(conflict("import preview capacity reached (20)")); }
            s.imports.push(preview.clone()); let mut out=serde_json::to_value(&preview).map_err(|_|bad("invalid preview"))?;out.as_object_mut().unwrap().remove("document");Ok(out)
        }).await
    }
    /// Read the exact saved input proposal under the same workspace authorization
    /// as Tax returns. Imported calculations/reviews are never part of this input.
    pub async fn import_preview(&self, preview_id: &str) -> TResult<Value> {
        let state=self.load().await?;
        let preview=state.imports.into_iter().find(|p|p.id==preview_id).ok_or_else(||bad("import preview not found"))?;
        Ok(json!(preview))
    }
    pub async fn commit_import(&self, preview_id: &str) -> TResult<TaxReturn> {
        let new_id=id("tax"); let at=now();
        self.mutate(|s| {
            let p=s.imports.iter().find(|p|p.id==preview_id).cloned().ok_or_else(||bad("import preview not found"))?;
            if let Some(existing)=&p.committed_return_id { return s.returns.iter().find(|r|&r.id==existing).cloned().ok_or_else(missing); }
            if p.expires_at<=now() { return Err(conflict("import preview expired; preview the document again")); }
            if s.returns.len()>=500 { return Err(conflict("tax return capacity reached")); }
            let r=record(&p.document,&new_id,&at); s.returns.push(r.clone()); s.imports.iter_mut().find(|p|p.id==preview_id).unwrap().committed_return_id=Some(new_id.clone());audit(s,&new_id,"imported",format!("Reviewed import preview {}",preview_id));Ok(r)
        }).await
    }
    /// Diff the saved workpaper against operator-supplied DT Max or WebTax lines.
    /// A missing artifact hash means the lines are not tied to a vendor file.
    pub async fn compare_parallel(&self, record_id: &str, mut source: parallel::ParallelSource) -> TResult<Value> {
        source.program = program(&source.program)?;
        source.jurisdiction = source.jurisdiction.to_ascii_uppercase();
        let current = self.get(record_id).await?;
        if current.program != source.program || current.tax_year != source.tax_year || current.jurisdiction != source.jurisdiction {
            return Err(bad("source program, year and jurisdiction must match this workpaper"));
        }
        let calculation = current.calculation.as_ref().ok_or_else(|| conflict("calculate the workpaper before comparing it"))?;
        let lines = calculation.result["lines"].as_array().ok_or_else(|| conflict("calculation has no lines"))?;
        let mut nxt = std::collections::BTreeMap::new();
        for line in lines {
            let code = line["code"].as_str().ok_or_else(|| bad("calculation line code is missing"))?;
            let amount = line["amount"].as_i64().ok_or_else(|| bad("calculation line is not integer cents"))?;
            nxt.insert(code.to_string(), amount);
        }
        let mut report = parallel::compare(&nxt, &source).map_err(bad)?;
        report["return_id"] = json!(record_id);
        report["calculation_id"] = json!(calculation.id);
        report["output_hash"] = json!(calculation.output_hash);
        let stored = report.clone();
        self.mutate(|s| {
            let r = s.returns.iter_mut().find(|r| r.id == record_id).ok_or_else(missing)?;
            let calc = r.calculation.as_ref().ok_or_else(|| conflict("calculate the workpaper before comparing it"))?;
            if calc.id != stored["calculation_id"].as_str().unwrap_or("") { return Err(conflict("calculation changed; compare again")); }
            if r.parallel_comparisons.len() >= 20 { return Err(conflict("parallel comparison history is full")); }
            r.parallel_comparisons.push(stored.clone());
            r.revision += 1;
            r.updated_at = now();
            audit(s, record_id, "parallel_compare", "Compared supplied source lines. The source stays authoritative. Nothing was filed or written back.");
            Ok(stored.clone())
        }).await
    }
    pub async fn link_family(&self, label: &str, primary: &str, spouse: &str) -> TResult<Value> {
        text(label, "family label", 160, false)?;
        if label.trim().is_empty() { return Err(bad("family label is required")); }
        if primary == spouse { return Err(bad("a family needs two different T1 workpapers")); }
        let new_id = id("family");
        let at = now();
        self.mutate(|s| {
            let a = s.returns.iter().find(|r| r.id == primary).ok_or_else(missing)?;
            let b = s.returns.iter().find(|r| r.id == spouse).ok_or_else(missing)?;
            if a.program != "T1" || b.program != "T1" { return Err(bad("family comparison is limited to two T1 workpapers")); }
            if let Some(existing) = s.families.iter().find(|f| f.primary_return_id == primary && f.spouse_return_id == spouse).cloned() {
                return family_view(s, &existing);
            }
            if s.families.len() >= 100 { return Err(conflict("family link capacity reached")); }
            let link = FamilyLink { id: new_id.clone(), label: label.trim().into(), primary_return_id: primary.into(), spouse_return_id: spouse.into(), created_at: at.clone() };
            s.families.push(link.clone());
            audit(s, primary, "family_link", "Side-by-side T1 totals. No spouse credit was transferred and nothing was filed.");
            family_view(s, &link)
        }).await
    }
    pub async fn family(&self, family_id: &str) -> TResult<Value> {
        let state = self.load().await?;
        let link = state.families.iter().find(|f| f.id == family_id).ok_or_else(|| bad("family link not found"))?;
        family_view(&state, link)
    }
}

fn family_view(state: &TaxState, link: &FamilyLink) -> TResult<Value> {
    let primary = state.returns.iter().find(|r| r.id == link.primary_return_id).ok_or_else(missing)?;
    let spouse = state.returns.iter().find(|r| r.id == link.spouse_return_id).ok_or_else(missing)?;
    Ok(json!({
        "id": link.id,
        "label": link.label,
        "optimization": "not_performed",
        "credit_transfer": false,
        "filing_enabled": false,
        "write_back": false,
        "note": "Side-by-side totals only. Spouse amount on an Ontario return is not filled from the other workpaper.",
        "primary": family_side(primary),
        "spouse": family_side(spouse),
    }))
}
fn family_side(r: &TaxReturn) -> Value {
    json!({
        "id": r.id,
        "jurisdiction": r.jurisdiction,
        "tax_year": r.tax_year,
        "status": r.status,
        "supported": r.calculation.as_ref().map(|c| c.result["supported"].clone()),
        "totals": r.calculation.as_ref().and_then(|c| c.result.get("totals").cloned()),
    })
}

/// Carry `<name>_closing_cents` (e.g. non_capital_loss_closing_cents, rrsp_unused_closing_cents)
/// into the next year as `<name>_opening_cents`. Everything else is cleared.
pub fn carryforward_inputs(prior: &Value) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(m) = prior.as_object() { for (k, v) in m { if let Some(stem) = k.strip_suffix("_closing_cents") { if v.is_i64() { out.insert(format!("{stem}_opening_cents"), v.clone()); } } } }
    Value::Object(out)
}
/// E-file is a stub. Even with NXT_TAX_EFILE_ENABLED=true nothing is transmitted:
/// real EFILE/NETFILE/Corporation Internet Filing needs annual CRA certification and an EFILE number.
pub fn efile_status() -> Value {
    let configured = std::env::var("NXT_TAX_EFILE_ENABLED").is_ok_and(|v| v == "true");
    json!({"stub":true,"config_flag":"NXT_TAX_EFILE_ENABLED","config_enabled":configured,"certified":false,"efile_number":null,"transmits":false,"external_dependencies":["CRA EFILE/NETFILE/Corporation Internet Filing software certification (renewed annually)","CRA EFILE number and firm registration","Legal and tax review"]})
}
#[cfg(test)] mod carry_tests { use super::*;
    #[test] fn closing_becomes_opening() { let v=carryforward_inputs(&json!({"non_capital_loss_closing_cents":120000,"employment_income_cents":5,"x_closing_cents":"bad"})); assert_eq!(v,json!({"non_capital_loss_opening_cents":120000})); }
    #[test] fn efile_never_transmits() { assert_eq!(efile_status()["transmits"],false); assert_eq!(efile_status()["certified"],false); } }
