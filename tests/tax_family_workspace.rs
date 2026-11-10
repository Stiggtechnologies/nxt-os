use nxt_os::{tax::{Tax,models::*,family_workflow::*},store::Store};
use serde_json::{Value,json};
async fn add(t:&Tax,name:&str,year:i32,company:&str)->TaxReturn {
    t.save(None,serde_json::from_value(json!({"program":"T1","tax_year":year,"jurisdiction":"AB","name":name,"client_ref":company,"inputs":{"scope_confirmed":false,"employment_income_cents":0}})).unwrap()).await.ok().unwrap()
}
fn link(a:&TaxReturn,b:&TaxReturn)->MembersInput {
    MembersInput{label:"Synthetic family".into(),primary_return_id:a.id.clone(),spouse_return_id:b.id.clone(),expected_primary_revision:a.revision,expected_spouse_revision:b.revision,expected_client_ref:a.client_ref.clone(),expected_family_sha256:None}
}
fn inputs(w:&Value)->PairedInputs {serde_json::from_value(json!({"expected_family_sha256":w["family_sha256"],"expected_primary_revision":w["primary"]["revision"],"expected_spouse_revision":w["spouse"]["revision"],"expected_client_ref":w["client_ref"],"primary_inputs":w["primary"]["inputs"],"spouse_inputs":w["spouse"]["inputs"]})).unwrap()}
#[test]
fn legacy_family_bytes_have_no_new_default_field(){
    let v=json!({"id":"legacy","label":"Legacy","primary_return_id":"A","spouse_return_id":"B","created_at":"old"});
    let f:FamilyLink=serde_json::from_value(v.clone()).unwrap();assert_eq!(f.revision,0);assert_eq!(serde_json::to_value(f).unwrap(),v);
}
#[tokio::test]
async fn family_away_and_back_cannot_restore_a_stale_fingerprint(){
    let t=Tax::new(Store::memory());let a=add(&t,"TEST-A",2025,"").await;let b=add(&t,"TEST-B",2025,"").await;
    let w=t.save_family_members(None,link(&a,&b)).await.ok().unwrap();let fid=w["family"]["id"].as_str().unwrap();
    let mut change=link(&a,&b);change.label="Different".into();change.expected_family_sha256=Some(w["family_sha256"].as_str().unwrap().into());let next=t.save_family_members(Some(fid),change).await.ok().unwrap();
    let mut back=link(&a,&b);back.expected_family_sha256=Some(next["family_sha256"].as_str().unwrap().into());let restored=t.save_family_members(Some(fid),back).await.ok().unwrap();assert_eq!(restored["family"]["label"],w["family"]["label"]);assert_ne!(restored["family_sha256"],w["family_sha256"]);assert!(t.save_family_pair(fid,inputs(&w)).await.is_err());
}
#[tokio::test]
async fn durable_membership_requires_same_year_company_revision_and_unique_members() {
    let t=Tax::new(Store::memory());let a=add(&t,"TEST-primary",2025,"").await;let b=add(&t,"TEST-spouse",2025,"").await;
    let other_year=add(&t,"TEST-year",2026,"").await;assert!(t.save_family_members(None,link(&a,&other_year)).await.is_err());
    let other_company=add(&t,"TEST-company",2025,"other").await;assert!(t.save_family_members(None,link(&a,&other_company)).await.is_err());
    let mut stale=link(&a,&b);stale.expected_primary_revision+=1;assert!(t.save_family_members(None,stale).await.is_err());
    let before=serde_json::to_vec(&t.get(&a.id).await.ok().unwrap()).unwrap();let w=t.save_family_members(None,link(&a,&b)).await.ok().unwrap();assert_eq!(serde_json::to_vec(&t.get(&a.id).await.ok().unwrap()).unwrap(),before);
    assert_eq!(w["automatic_plan_apply_available"],false);assert_eq!(w["primary_current_supported_calculation"],false);
    assert!(t.save_family_members(None,link(&b,&a)).await.is_err());
    let fid=w["family"]["id"].as_str().unwrap();let mut update=link(&a,&b);update.label="Updated family".into();update.expected_family_sha256=Some(w["family_sha256"].as_str().unwrap().into());let changed=t.save_family_members(Some(fid),update.clone()).await.ok().unwrap();assert_ne!(w["family_sha256"],changed["family_sha256"]);assert!(t.save_family_members(Some(fid),update).await.is_err());
}
#[tokio::test]
async fn paired_inputs_are_both_or_none_and_preserve_snapshot_and_review_history() {
    let store=Store::memory();let t=Tax::new(store.clone());let a=add(&t,"TEST-primary",2025,"").await;let b=add(&t,"TEST-spouse",2025,"").await;
    let w=t.save_family_members(None,link(&a,&b)).await.ok().unwrap();let fid=w["family"]["id"].as_str().unwrap();let before=store.get("tax:state:v1").await.ok().unwrap();
    for bad in ["spouse_type","primary_unknown","scope","revision","family"] {
        let mut i=inputs(&w);i.primary_inputs["employment_income_cents"]=json!(10000);
        match bad {"spouse_type"=>i.spouse_inputs["employment_income_cents"]=json!("bad"),"primary_unknown"=>i.primary_inputs["unverified"]=json!(1),"scope"=>i.expected_client_ref="wrong".into(),"revision"=>i.expected_spouse_revision+=1,_=>i.expected_family_sha256="stale".into()}
        assert!(t.save_family_pair(fid,i).await.is_err());assert_eq!(store.get("tax:state:v1").await.ok().unwrap(),before);
    }
    // Legacy single current snapshots/review without array copies must also survive invalidation.
    let mut state:TaxState=store.get_json("tax:state:v1").await.ok().unwrap().unwrap();
    for r in &mut state.returns {let d=ReturnInput::from(&*r);r.calculation=Some(CalculationSnapshot{id:format!("legacy-{}",r.id),input_revision:r.input_revision,request_revision:r.revision,idempotency_key:None,input_document:Some(d),input_hash:"legacy-input".into(),output_hash:"legacy-output".into(),created_at:"legacy-at".into(),result:json!({"supported":false})});r.review=Some(Review{at:"legacy-at".into(),reviewer:"Synthetic".into(),note:"preserve".into(),input_hash:"legacy-input".into(),output_hash:"legacy-output".into()});}
    store.set_json("tax:state:v1",&state).await.ok().unwrap();let w=t.family_workspace(fid).await.ok().unwrap();let mut i=inputs(&w);i.primary_inputs["employment_income_cents"]=json!(12345);i.spouse_inputs["employment_income_cents"]=json!(67890);let result=t.save_family_pair(fid,i.clone()).await.ok().unwrap();
    for side in ["primary","spouse"] {assert_eq!(result[side]["revision"],2);assert_eq!(result[side]["input_revision"],2);assert_eq!(result[side]["status"],"draft");assert!(result[side]["calculation"].is_null());assert!(result[side]["review"].is_null());assert_eq!(result[side]["calculations"][0],w[side]["calculation"]);assert_eq!(result[side]["reviews"][0],w[side]["review"]);}
    assert_eq!(result["primary"]["inputs"]["employment_income_cents"],12345);assert_eq!(result["spouse"]["inputs"]["employment_income_cents"],67890);assert!(t.save_family_pair(fid,i).await.is_err());
}
#[tokio::test]
async fn unavailable_year_locked_return_and_removed_fields_fail_closed() {
    let store=Store::memory();let t=Tax::new(store.clone());let a=add(&t,"TEST-one",2026,"").await;let b=add(&t,"TEST-two",2026,"").await;let w=t.save_family_members(None,link(&a,&b)).await.ok().unwrap();let fid=w["family"]["id"].as_str().unwrap();assert_eq!(w["paired_entry_available"],false);let before=store.get("tax:state:v1").await.ok().unwrap();assert!(t.save_family_pair(fid,inputs(&w)).await.is_err());assert_eq!(store.get("tax:state:v1").await.ok().unwrap(),before);
    let a=add(&t,"TEST-three",2025,"B").await;let b=add(&t,"TEST-four",2025,"B").await;let w=t.save_family_members(None,link(&a,&b)).await.ok().unwrap();let fid=w["family"]["id"].as_str().unwrap();let mut i=inputs(&w);i.primary_inputs.as_object_mut().unwrap().remove("scope_confirmed");assert!(t.save_family_pair(fid,i).await.is_err());
    let mut state:TaxState=store.get_json("tax:state:v1").await.ok().unwrap().unwrap();state.returns.iter_mut().find(|r|r.id==b.id).unwrap().status="approved".into();store.set_json("tax:state:v1",&state).await.ok().unwrap();let before=store.get("tax:state:v1").await.ok().unwrap();assert!(t.save_family_pair(fid,inputs(&w)).await.is_err());assert_eq!(store.get("tax:state:v1").await.ok().unwrap(),before);
}

#[tokio::test]
async fn paired_editor_preserves_unrepresented_saved_fields_without_allowing_new_or_changed_values(){
    let store=Store::memory();let t=Tax::new(store.clone());
    let mut input=serde_json::to_value(ReturnInput::from(&add(&t,"Synthetic-template",2025,"A").await)).unwrap();
    input["name"]=json!("Synthetic grouped pension");input["slip_groups"]=json!([{"kind":"T4A","issuer":"Synthetic payer","boxes":{"016":12345}}]);input["inputs"]["legacy_unrepresented_note"]=json!({"original":"Synthetic retained field"});
    let a=t.save(None,serde_json::from_value(input).unwrap()).await.ok().unwrap();let b=add(&t,"Synthetic spouse",2025,"A").await;
    let w=t.save_family_members(None,link(&a,&b)).await.ok().unwrap();assert_eq!(w["paired_entry_available"],true);
    let fid=w["family"]["id"].as_str().unwrap();let before=store.get("tax:state:v1").await.unwrap();
    for mode in ["change","change_unknown","add","remove"] {let mut i=inputs(&w);match mode {
        "change"=>i.primary_inputs["pension_income_cents"]=json!(12346),
        "change_unknown"=>i.primary_inputs["legacy_unrepresented_note"]=json!({"original":"Changed"}),
        "remove"=>{i.primary_inputs.as_object_mut().unwrap().remove("legacy_unrepresented_note");},
        _=>i.primary_inputs["unknown_unverified_new_key"]=json!(1)
    }
        assert!(t.save_family_pair(fid,i).await.is_err());assert_eq!(store.get("tax:state:v1").await.unwrap(),before);
    }
    let mut i=inputs(&w);i.primary_inputs["instalments_cents"]=json!(7);let saved=t.save_family_pair(fid,i).await.ok().unwrap();
    assert_eq!(saved["primary"]["inputs"]["pension_income_cents"],12345);assert_eq!(saved["primary"]["inputs"]["instalments_cents"],7);
    assert_eq!(saved["primary"]["inputs"]["legacy_unrepresented_note"],w["primary"]["inputs"]["legacy_unrepresented_note"]);
    assert_eq!(saved["primary"]["slip_groups"],serde_json::to_value(a.slip_groups).unwrap());
}

#[tokio::test]
async fn family_http_routes_authorize_both_members_and_filter_before_counts(){
    use axum::{body::Body,http::Request};use tower::ServiceExt;use std::sync::Arc;use nxt_os::firm::staff;
    unsafe{std::env::set_var("ADMIN_API_KEY","family-test-key");std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS","1000");}
    staff::install(Arc::new(staff::Staff::new(Store::memory())));let store=Store::memory();let app=nxt_os::app_with(||store.clone());let t=Tax::new(store.clone());
    async fn call(app:&axum::Router,method:&str,path:&str,token:&str,body:Value)->(u16,Value){
        let r=app.clone().oneshot(Request::builder().method(method).uri(path).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(if method=="GET"{Body::empty()}else{Body::from(body.to_string())}).unwrap()).await.unwrap();let code=r.status().as_u16();if code==200&&path.starts_with("/api/tax/"){assert_eq!(r.headers()["cache-control"],"private, no-store");}let bytes=axum::body::to_bytes(r.into_body(),1024*1024).await.unwrap();(code,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
    async fn login(app:&axum::Router,user:&str,role:&str)->String{
        let pw="synthetic-password-1";assert_eq!(call(app,"POST","/api/staff/users","family-test-key",json!({"username":user,"display_name":"Synthetic","role":role,"clients":["A"],"temporary_password":pw})).await.0,200);
        let token=call(app,"POST","/api/staff/login","",json!({"username":user,"password":pw})).await.1["token"].as_str().unwrap().to_owned();
        let secret=call(app,"POST","/api/staff/mfa/enroll",&token,json!({})).await.1["secret"].as_str().unwrap().to_owned();assert_eq!(call(app,"POST","/api/staff/mfa/confirm",&token,json!({"code":staff::totp_now(&secret).unwrap()})).await.0,200);
        assert_eq!(call(app,"POST","/api/staff/me/password",&token,json!({"current_password":pw,"new_password":"synthetic-password-2"})).await.0,200);token
    }
    let a1=add(&t,"Allowed-A1",2025,"A").await;let a2=add(&t,"Allowed-A2",2025,"A").await;let a3=add(&t,"Allowed-A3",2025,"A").await;
    let b1=add(&t,"PRIVATE-B1",2025,"B").await;let b2=add(&t,"PRIVATE-B2",2025,"B").await;let b3=add(&t,"PRIVATE-B3",2025,"B").await;
    let u1=add(&t,"PRIVATE-U1",2025,"").await;let u2=add(&t,"PRIVATE-U2",2025,"").await;
    let wa=t.save_family_members(None,link(&a1,&a2)).await.ok().unwrap();let wb=t.save_family_members(None,link(&b1,&b2)).await.ok().unwrap();let wu=t.save_family_members(None,link(&u1,&u2)).await.ok().unwrap();
    let mixed=t.link_family("PRIVATE-mixed",&a3.id,&b3.id).await.ok().unwrap();
    let path="/api/tax/family-workspaces";assert_eq!(call(&app,"GET",path,"",Value::Null).await.0,401);assert_eq!(call(&app,"POST",path,"family-test-key",json!({"automatic_apply":true})).await.0,422);
    let prep=login(&app,"family-preparer","preparer").await;let read=login(&app,"family-reader","read_only").await;
    for token in [&prep,&read]{let (code,v)=call(&app,"GET",path,token,Value::Null).await;assert_eq!(code,200);assert_eq!(v["families"].as_array().unwrap().len(),1);assert_eq!(v["candidates"].as_array().unwrap().len(),3);let text=v.to_string();assert!(!text.contains("PRIVATE"));assert!(!text.contains(wb["family"]["id"].as_str().unwrap()));assert!(!text.contains(wb["family_sha256"].as_str().unwrap()));}
    let aid=wa["family"]["id"].as_str().unwrap();let aurl=format!("{path}/{aid}");assert_eq!(call(&app,"GET",&aurl,&prep,Value::Null).await.0,200);let ro=call(&app,"GET",&aurl,&read,Value::Null).await.1;assert_eq!(ro["write_permitted"],false);assert_eq!(ro["paired_entry_available"],false);
    for fid in [wb["family"]["id"].as_str().unwrap(),wu["family"]["id"].as_str().unwrap(),mixed["id"].as_str().unwrap(),"does-not-exist"]{let (code,v)=call(&app,"GET",&format!("{path}/{fid}"),&prep,Value::Null).await;assert_eq!(code,403);assert!(!v.to_string().contains("PRIVATE"));assert!(!v.to_string().contains("sha256"));}
    let before=store.get("tax:state:v1").await.ok().unwrap();let body=serde_json::to_value(inputs(&wa)).unwrap();
    assert_eq!(call(&app,"PUT",&format!("{aurl}/inputs"),&read,body.clone()).await.0,403);
    let mut cross=serde_json::to_value(link(&a1,&b1)).unwrap();cross["expected_client_ref"]=json!("A");assert_eq!(call(&app,"POST",path,&prep,cross).await.0,403);
    let mut replace=serde_json::to_value(link(&a1,&a2)).unwrap();replace["expected_family_sha256"]=wb["family_sha256"].clone();assert_eq!(call(&app,"PUT",&format!("{path}/{}",wb["family"]["id"].as_str().unwrap()),&prep,replace).await.0,403);
    assert_eq!(store.get("tax:state:v1").await.ok().unwrap(),before);
    let mut update=serde_json::to_value(link(&a1,&a2)).unwrap();update["expected_family_sha256"]=wa["family_sha256"].clone();update["label"]=json!("Authorized-A");let (code,fresh)=call(&app,"PUT",&aurl,&prep,update).await;assert_eq!(code,200);
    let mut pair=serde_json::to_value(inputs(&fresh)).unwrap();pair["primary_inputs"]["employment_income_cents"]=json!(123);pair["spouse_inputs"]["employment_income_cents"]=json!(456);let (code,saved)=call(&app,"PUT",&format!("{aurl}/inputs"),&prep,pair).await;assert_eq!(code,200);assert_eq!(saved["primary"]["inputs"]["employment_income_cents"],123);assert_eq!(saved["spouse"]["inputs"]["employment_income_cents"],456);
    // Exact-prefix dispatch does not open the legacy family module to scoped users.
    assert_eq!(call(&app,"GET",&format!("/api/tax/families/{aid}"),&prep,Value::Null).await.0,403);
}

#[tokio::test]
async fn service_actor_checks_reject_member_scope_changes_inside_the_cas(){
    use nxt_os::firm::staff::{Identity,Role};
    let actor=Identity{user_id:"test".into(),display_name:"Synthetic".into(),role:Role::Preparer,clients:vec!["A".into()],break_glass:false,mfa:true,must_change_password:false};
    let store=Store::memory();let t=Tax::new(store.clone());let a=add(&t,"Allowed-A1",2025,"A").await;let b=add(&t,"Allowed-A2",2025,"A").await;let w=t.save_family_members_for(None,link(&a,&b),&actor).await.ok().unwrap();let fid=w["family"]["id"].as_str().unwrap();
    let initial=store.get("tax:state:v1").await.ok().unwrap();
    for mode in ["readonly","mfa","temporary"] {let mut denied=actor.clone();if mode=="readonly"{denied.role=Role::ReadOnly;}if mode=="mfa"{denied.mfa=false;}if mode=="temporary"{denied.must_change_password=true;}assert_eq!(t.save_family_pair_for(fid,inputs(&w),&denied).await.err().unwrap().0,403);assert_eq!(store.get("tax:state:v1").await.ok().unwrap(),initial);}
    let i=inputs(&w);let mut state:TaxState=store.get_json("tax:state:v1").await.ok().unwrap().unwrap();state.returns.iter_mut().find(|r|r.id==b.id).unwrap().client_ref="B".into();store.set_json("tax:state:v1",&state).await.unwrap();let before=store.get("tax:state:v1").await.ok().unwrap();
    assert_eq!(t.save_family_pair_for(fid,i,&actor).await.err().unwrap().0,403);assert_eq!(store.get("tax:state:v1").await.ok().unwrap(),before);assert_eq!(t.family_workspace_for(fid,&actor).await.err().unwrap().0,403);let list=t.family_workspaces_for(&actor).await.ok().unwrap();assert_eq!(list["families"].as_array().unwrap().len(),0);assert_eq!(list["candidates"].as_array().unwrap().len(),1);
}
