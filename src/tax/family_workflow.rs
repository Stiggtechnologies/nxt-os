//! Durable family comparison and atomic, explicitly entered paired inputs.
//! No automatic optimisation/election, calculation, approval or source mapping.
use super::{Tax,TResult,models::*,bad,conflict,missing,hash,audit,now,id};
use serde::{Deserialize,Serialize};
use crate::firm::staff::{Identity,check_policy};
use axum::http::{Method,StatusCode};
use serde_json::{Value,json};

#[derive(Clone,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct MembersInput {
    pub label:String, pub primary_return_id:String, pub spouse_return_id:String,
    pub expected_primary_revision:u64, pub expected_spouse_revision:u64,
    pub expected_client_ref:String,
    #[serde(default)] pub expected_family_sha256:Option<String>,
}
#[derive(Clone,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedInputs {
    pub expected_family_sha256:String, pub expected_primary_revision:u64,
    pub expected_spouse_revision:u64, pub expected_client_ref:String,
    pub primary_inputs:Value, pub spouse_inputs:Value,
}
fn denied()->super::ApiError {super::ApiError(StatusCode::FORBIDDEN,"Family access requires assignment to both members".into())}
fn policy(actor:&Identity,write:bool)->TResult<()> {
    if actor.must_change_password{return Err(denied());}
    check_policy(actor,if write{&Method::PUT}else{&Method::GET},"/api/tax/family-workspaces",&json!({})).map_err(|_|denied())
}
fn allowed(actor:&Identity,r:&TaxReturn)->bool {actor.can_client(&r.client_ref)}
fn authorized_return<'a>(s:&'a TaxState,id:&str,actor:&Identity)->TResult<&'a TaxReturn> {
    s.returns.iter().find(|r|r.id==id).filter(|r|allowed(actor,r)).ok_or_else(denied)
}
fn authorized_family<'a>(s:&'a TaxState,id:&str,actor:&Identity)->TResult<&'a FamilyLink> {
    let f=s.families.iter().find(|f|f.id==id).ok_or_else(denied)?;
    authorized_return(s,&f.primary_return_id,actor)?;authorized_return(s,&f.spouse_return_id,actor)?;Ok(f)
}
fn actor_view(s:&TaxState,f:&FamilyLink,actor:&Identity)->TResult<Value> {
    authorized_family(s,&f.id,actor)?;
    let mut out=view(s,f)?;let write=policy(actor,true).is_ok();out["write_permitted"]=json!(write);
    out["paired_entry_available"]=json!(write&&out["paired_entry_available"]==true);Ok(out)
}
fn pair(a:&TaxReturn,b:&TaxReturn,company:&str)->TResult<()> {
    if a.id==b.id || a.program!="T1" || b.program!="T1" {return Err(bad("Choose two different T1 workpapers"));}
    if a.tax_year!=b.tax_year {return Err(conflict("Family members must have the same tax year"));}
    if a.client_ref!=b.client_ref || a.client_ref!=company {return Err(conflict("Family workspace/company differs; reload in the correct workspace"));}
    Ok(())
}
fn members<'a>(s:&'a TaxState,f:&FamilyLink)->TResult<(&'a TaxReturn,&'a TaxReturn)> {
    let a=s.returns.iter().find(|r|r.id==f.primary_return_id).ok_or_else(missing)?;
    let b=s.returns.iter().find(|r|r.id==f.spouse_return_id).ok_or_else(missing)?;
    pair(a,b,&a.client_ref)?;Ok((a,b))
}
fn exclusive(s:&TaxState,f:&FamilyLink)->bool {
    !s.families.iter().any(|other|other.id!=f.id && [&other.primary_return_id,&other.spouse_return_id].iter().any(|id|**id==f.primary_return_id||**id==f.spouse_return_id))
}
fn fresh(r:&TaxReturn)->bool {
    super::calculations::supports_scope(&r.program,r.tax_year,&r.jurisdiction) && r.calculation.as_ref().is_some_and(|c|
      c.input_revision==r.input_revision && super::input_hash(r).ok().as_ref()==Some(&c.input_hash)
      && c.input_document.as_ref().and_then(|d|hash(d).ok()).as_ref()==Some(&c.input_hash)
      && hash(&c.result).ok().as_ref()==Some(&c.output_hash) && c.result["supported"]==true
      && c.result["diagnostics"].as_array().is_some_and(|ds|!ds.iter().any(|d|d["blocks_review"]==true||d["severity"]=="error")))
}
fn assert_group_inputs(groups:&[super::slips::Slip],proposed:&Value)->TResult<()> {
    if groups.is_empty(){return Ok(());}
    let aggregation=super::slips::aggregate(groups).map_err(bad)?;
    for (key,value) in aggregation["inputs"].as_object().ok_or_else(||bad("Grouped input contract differs"))? {
        if proposed.get(key)!=Some(value){return Err(bad("Saved slip groups manage this key; edit its occurrences instead"));}
    }
    Ok(())
}
fn editable_inputs(r:&TaxReturn,v:&Value)->TResult<()> {
    if !["draft","calculated","in_review"].contains(&r.status.as_str()) || r.filed_record.is_some() {return Err(conflict("Reopen both returns before paired entry; filed records remain immutable"));}
    if !super::coverage::entry_schema_ready("T1",r.tax_year,&r.jurisdiction) {return Err(conflict("Verified year-specific paired entry is unavailable"));}
    // Additive grouped-input model can land independently; cloning preserves its field.
    let saved=serde_json::to_value(r).map_err(|_|bad("Saved return encoding differs"))?;
    if let Some(value)=saved.get("slip_groups") {
        let groups:Vec<super::slips::Slip>=serde_json::from_value(value.clone()).map_err(|_|bad("Saved slip groups differ"))?;
        assert_group_inputs(&groups,v)?;
    }
    let catalog=super::catalog::catalog();let p=catalog["programs"].as_array().unwrap().iter().find(|p|p["id"]=="T1").unwrap();
    let o=v.as_object().ok_or_else(||bad("Paired inputs must be objects"))?;
    for (k,value) in o {
        let Some(field)=p["sections"].as_array().unwrap().iter().flat_map(|s|s["fields"].as_array().unwrap()).find(|f|f["key"]==k.as_str()) else {
            // Simpler paired entry preserves existing fields it cannot edit, including
            // values derived from supported saved slip groups. It cannot add/change them.
            if r.inputs.get(k)==Some(value){continue;}
            return Err(bad("Unknown or unverified paired input field"));
        };
        let valid=match field["kind"].as_str().unwrap_or("") {
            "boolean"=>value.is_boolean(),"money"=>value.as_i64().is_some_and(|n|(0..=100_000_000_000_000).contains(&n)),
            "number"=>value.as_i64().is_some(),"text"|"choice"=>value.is_string(),_=>false};
        if !valid {return Err(bad("Paired field type or monetary bound differs"));}
    }
    // Existing unrepresented fields may not be silently erased through a simpler editor.
    for k in r.inputs.as_object().ok_or_else(||bad("Saved input shape differs"))?.keys() {
        if !o.contains_key(k) {return Err(bad("Paired entry cannot remove a saved field"));}
    }
    let mut input=ReturnInput::from(r);input.inputs=v.clone();super::validate(input)?;Ok(())
}

#[cfg(test)]mod group_tests{
    use super::*;
    #[test]fn grouped_authoritative_values_cannot_be_overridden(){
        let groups=vec![serde_json::from_value(json!({"kind":"T4","issuer":"Synthetic","boxes":{"14":12345}})).unwrap()];
        let expected=super::super::slips::aggregate(&groups).unwrap()["inputs"].clone();assert!(assert_group_inputs(&groups,&expected).is_ok());
        let mut wrong=expected.clone();wrong["employment_income_cents"]=json!(0);assert!(assert_group_inputs(&groups,&wrong).is_err());
        assert!(assert_group_inputs(&[],&json!({})).is_ok());
    }
}
fn view(s:&TaxState,f:&FamilyLink)->TResult<Value> {
    let (a,b)=members(s,f)?;
    let ready=exclusive(s,f)&&editable_inputs(a,&a.inputs).is_ok()&&editable_inputs(b,&b.inputs).is_ok();
    Ok(json!({"schema":"nxt-tax-family-workspace/v1","family":f,"family_sha256":hash(f)?,"client_ref":a.client_ref,"tax_year":a.tax_year,
      "primary":a,"spouse":b,"primary_current_supported_calculation":fresh(a),"spouse_current_supported_calculation":fresh(b),
      "paired_entry_available":ready,"automatic_plan_apply_available":false,"filing_enabled":false,
      "plan_blockers":["Existing proposals do not supply a validated, year-specific paired mutation contract.","Pension elections, eligible expenses, unused amounts and provincial effects require evidence not captured by this workflow.","Manual paired input saving does not calculate, approve or apply a proposal."]}))
}
impl Tax {
    pub async fn family_workspaces(&self)->TResult<Value> {self.family_workspaces_for(&Identity::break_glass()).await}
    pub async fn family_workspace(&self,family:&str)->TResult<Value> {self.family_workspace_for(family,&Identity::break_glass()).await}
    pub async fn family_workspaces_for(&self,actor:&Identity)->TResult<Value> {
        policy(actor,false)?;let s=self.load().await?;
        let families=s.families.iter().filter(|f|authorized_family(&s,&f.id,actor).is_ok()).map(|f|json!({"id":f.id,"label":f.label,"primary_return_id":f.primary_return_id,"spouse_return_id":f.spouse_return_id,"client_ref":s.returns.iter().find(|r|r.id==f.primary_return_id).map(|r|r.client_ref.clone())})).collect::<Vec<_>>();
        let candidates=s.returns.iter().filter(|r|r.program=="T1"&&allowed(actor,r)).map(|r|json!({"id":r.id,"name":r.name,"tax_year":r.tax_year,"client_ref":r.client_ref,"revision":r.revision})).collect::<Vec<_>>();
        Ok(json!({"families":families,"candidates":candidates,"write_permitted":policy(actor,true).is_ok()}))
    }
    pub async fn family_workspace_for(&self,family:&str,actor:&Identity)->TResult<Value> {
        policy(actor,false)?;let s=self.load().await?;let f=authorized_family(&s,family,actor)?;actor_view(&s,f,actor)
    }
    pub async fn save_family_members(&self,family:Option<&str>,i:MembersInput)->TResult<Value> {self.save_family_members_for(family,i,&Identity::break_glass()).await}
    pub async fn save_family_members_for(&self,family:Option<&str>,i:MembersInput,actor:&Identity)->TResult<Value> {
        policy(actor,true)?;
        super::text(&i.label,"family label",160,true)?;
        if family.is_some()!=i.expected_family_sha256.is_some(){return Err(bad("Family fingerprint required only for updates"));}
        let new_id=id("family");let at=now();
        self.mutate(|s| {
            policy(actor,true)?;if !actor.can_client(&i.expected_client_ref){return Err(denied());}
            if let Some(fid)=family{authorized_family(s,fid,actor)?;}
            let a=authorized_return(s,&i.primary_return_id,actor)?;
            let b=authorized_return(s,&i.spouse_return_id,actor)?;
            pair(a,b,&i.expected_client_ref)?;super::assert_revision(a,i.expected_primary_revision)?;super::assert_revision(b,i.expected_spouse_revision)?;
            let related=s.families.iter().filter(|f|Some(f.id.as_str())!=family && [&f.primary_return_id,&f.spouse_return_id].iter().any(|id|**id==a.id||**id==b.id)).collect::<Vec<_>>();
            for other in &related{authorized_family(s,&other.id,actor)?;}
            if !related.is_empty(){return Err(conflict("A selected member already belongs to another family"));}
            let out=if let Some(fid)=family {
                let old=s.families.iter().find(|f|f.id==fid).ok_or_else(missing)?;let (old_a,_)=members(s,old)?;
                if old_a.client_ref!=i.expected_client_ref || Some(hash(old)?)!=i.expected_family_sha256 {return Err(conflict("Family membership changed; reload"));}
                let mut f=old.clone();f.revision=f.revision.checked_add(1).ok_or_else(||conflict("Family revision exhausted"))?;f.label=i.label.trim().into();f.primary_return_id=i.primary_return_id.clone();f.spouse_return_id=i.spouse_return_id.clone();f
            }else{if s.families.len()>=100{return Err(conflict("Family capacity reached"));}FamilyLink{id:new_id.clone(),label:i.label.trim().into(),primary_return_id:i.primary_return_id.clone(),spouse_return_id:i.spouse_return_id.clone(),created_at:at.clone(),revision:1}};
            s.families.retain(|f|f.id!=out.id);s.families.push(out.clone());audit(s,&out.primary_return_id,"family_members_updated",format!("Same-year family comparison membership by {}; no identity assignment or financial changes",actor.label()));actor_view(s,&out,actor)
        }).await
    }
    pub async fn save_family_pair(&self,family:&str,i:PairedInputs)->TResult<Value> {self.save_family_pair_for(family,i,&Identity::break_glass()).await}
    pub async fn save_family_pair_for(&self,family:&str,i:PairedInputs,actor:&Identity)->TResult<Value> {
        policy(actor,true)?;let at=now();self.mutate(|s| {
            policy(actor,true)?;if !actor.can_client(&i.expected_client_ref){return Err(denied());}
            let f=authorized_family(s,family,actor)?.clone();
            for other in &s.families{if other.id!=f.id && [&other.primary_return_id,&other.spouse_return_id].iter().any(|id|**id==f.primary_return_id||**id==f.spouse_return_id){authorized_family(s,&other.id,actor)?;}}
            if !exclusive(s,&f){return Err(conflict("A member belongs to multiple family links; resolve membership before paired editing"));}
            if hash(&f)?!=i.expected_family_sha256{return Err(conflict("Family membership changed; reload"));}
            let (a,b)=members(s,&f)?;pair(a,b,&i.expected_client_ref)?;
            super::assert_revision(a,i.expected_primary_revision)?;super::assert_revision(b,i.expected_spouse_revision)?;
            editable_inputs(a,&i.primary_inputs)?;editable_inputs(b,&i.spouse_inputs)?;
            let mut next_a=a.clone();let mut next_b=b.clone();
            for (r,inputs) in [(&mut next_a,&i.primary_inputs),(&mut next_b,&i.spouse_inputs)] {
                if let Some(c)=r.calculation.clone(){if !r.calculations.iter().any(|v|v.id==c.id){if r.calculations.len()>=20{return Err(conflict("Preserve calculation history before paired editing"));}r.calculations.push(c);}}
                if let Some(v)=r.review.clone(){if !r.reviews.iter().any(|x|x.at==v.at&&x.input_hash==v.input_hash&&x.output_hash==v.output_hash){if r.reviews.len()>=20{return Err(conflict("Preserve review history before paired editing"));}r.reviews.push(v);}}
                r.inputs=inputs.clone();r.revision+=1;r.input_revision+=1;r.updated_at=at.clone();r.calculation=None;r.review=None;r.preparer_signoff=None;r.status="draft".into();
            }
            for next in [next_a,next_b]{let id=next.id.clone();*s.returns.iter_mut().find(|r|r.id==id).unwrap()=next;audit(s,&id,"family_pair_inputs_saved",format!("Paired manual inputs saved atomically by {}; recalculate and review each return",actor.label()));}
            actor_view(s,&f,actor)
        }).await
    }
}
