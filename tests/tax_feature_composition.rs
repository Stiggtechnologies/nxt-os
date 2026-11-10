//! Cross-feature contracts on synthetic, in-memory native preparation records only.
//! No router/credentials/environment changes, imported sources, approval or network.
use nxt_os::{store::Store,firm::staff::{Identity,Role},tax::{
    Tax,models::{TaxReturn,ReturnInput,RevisionInput},
    family_workflow::{MembersInput,PairedInputs},
    batch::{Create,Operation,Selection,Step,ItemState},reports::ReportRequest,
}};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};

fn actor()->Identity {Identity{user_id:"synthetic-composition".into(),display_name:"Synthetic composition".into(),role:Role::Preparer,clients:vec!["A".into()],break_glass:false,mfa:true,must_change_password:false}}
fn digest(bytes:&[u8])->String {hex::encode(Sha256::digest(bytes))}
fn paired(w:&Value)->PairedInputs {serde_json::from_value(json!({
    "expected_family_sha256":w["family_sha256"],"expected_primary_revision":w["primary"]["revision"],
    "expected_spouse_revision":w["spouse"]["revision"],"expected_client_ref":"A",
    "primary_inputs":w["primary"]["inputs"],"spouse_inputs":w["spouse"]["inputs"]
})).unwrap()}
fn grouped(name:&str,wages:i64)->ReturnInput {serde_json::from_value(json!({
    "program":"T1","tax_year":2025,"jurisdiction":"AB","name":name,"client_ref":"A",
    "inputs":{"scope_confirmed":true,"full_year_resident":true,"provincial_adjustments_not_applicable":true,"instalments_cents":0},
    "slip_groups":[{"kind":"T4","issuer":"Synthetic payer","boxes":{"14":wages,"22":0}}]
})).unwrap()}
fn report_request()->ReportRequest {serde_json::from_value(json!({"program":"T1","client_ref":"A","years":[2025],"columns":["input:instalments_cents","line:15000","total:income_tax"]})).unwrap()}

#[tokio::test]
async fn legacy_group_omission_preserves_fixed_bytes_and_calculation_input_hash() {
    // Frozen pre-group ReturnInput shape/hash, independently pinned: not a roundtrip-only assertion.
    let expected=r#"{"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic legacy","client_ref":"A","period_start":null,"period_end":null,"inputs":{"employment_income_cents":0,"scope_confirmed":false},"parties":[],"corporate":null,"notes":"","expected_revision":null}"#;
    const EXPECTED_SHA:&str="d5d5b77a9c33e152357fb0f99b729d3319a3993984d783052fa6ff1e9c3a374d";
    let input:ReturnInput=serde_json::from_str(expected).unwrap();assert!(input.slip_groups.is_none());
    assert_eq!(serde_json::to_vec(&input).unwrap(),expected.as_bytes());assert_eq!(digest(expected.as_bytes()),EXPECTED_SHA);
    let store=Store::memory();let t=Tax::new(store);let r=t.save(None,input).await.unwrap_or_else(|e|panic!("legacy save: {}",e.1));
    assert!(r.slip_groups.is_empty());let bytes=serde_json::to_vec(&r).unwrap();let value:Value=serde_json::from_slice(&bytes).unwrap();assert!(value.get("slip_groups").is_none());
    let again:TaxReturn=serde_json::from_slice(&bytes).unwrap();assert_eq!(serde_json::to_vec(&again).unwrap(),bytes);
    assert_eq!(serde_json::to_vec(&ReturnInput::from(&again)).unwrap(),expected.as_bytes());
    let calculated=t.calculate(&r.id,RevisionInput{expected_revision:r.revision,idempotency_key:None}).await.unwrap_or_else(|e|panic!("legacy calculation: {}",e.1));
    let snapshot=calculated.calculation.unwrap();assert_eq!(snapshot.input_hash,EXPECTED_SHA);
    let document=snapshot.input_document.unwrap();assert!(document.slip_groups.is_none());assert_eq!(digest(&serde_json::to_vec(&document).unwrap()),EXPECTED_SHA);
}

#[tokio::test]
async fn paired_grouped_edit_is_atomic_preserves_groups_invalidates_report_and_holds_old_batch() {
    let store=Store::memory();let t=Tax::new(store.clone());let a=actor();
    let primary=t.save(None,grouped("Synthetic primary",5_000_000)).await.unwrap_or_else(|e|panic!("primary save: {}",e.1));
    let spouse=t.save(None,grouped("Synthetic spouse",4_000_000)).await.unwrap_or_else(|e|panic!("spouse save: {}",e.1));
    let primary=t.calculate(&primary.id,RevisionInput{expected_revision:primary.revision,idempotency_key:None}).await.unwrap_or_else(|e|panic!("primary calculation: {}",e.1));
    let spouse=t.calculate(&spouse.id,RevisionInput{expected_revision:spouse.revision,idempotency_key:None}).await.unwrap_or_else(|e|panic!("spouse calculation: {}",e.1));
    for r in [&primary,&spouse] {assert_eq!(r.calculation.as_ref().unwrap().result["supported"],true);assert_eq!(r.slip_groups[0].boxes.get("22"),Some(&0));assert_eq!(r.inputs["payroll_verified"],false);}
    let before_report=t.report(report_request(),Some(&a)).await.unwrap_or_else(|e|panic!("before report: {}",e.1));assert_eq!(before_report["row_count"],2);
    for row in before_report["rows"].as_array().unwrap(){assert_eq!(row["calculation_state"],"current_supported");assert!(row["line:15000"].is_i64());}
    let family=t.save_family_members_for(None,MembersInput{label:"Synthetic paired family".into(),primary_return_id:primary.id.clone(),spouse_return_id:spouse.id.clone(),expected_primary_revision:primary.revision,expected_spouse_revision:spouse.revision,expected_client_ref:"A".into(),expected_family_sha256:None},&a).await.unwrap_or_else(|e|panic!("family: {}",e.1));
    let fid=family["family"]["id"].as_str().unwrap();assert_eq!(family["paired_entry_available"],true);
    let job=t.batch_create(&a,Create{request_key:"synthetic-before-paired-edit".into(),operation:Operation::Calculate,company_ref:Some("A".into()),selections:[&primary,&spouse].into_iter().map(|r|Selection{return_id:r.id.clone(),expected_revision:r.revision}).collect()}).await.unwrap_or_else(|e|panic!("batch create: {}",e.1));
    assert!(job.items.iter().all(|i|i.state==ItemState::Pending));
    let state_before_rejection=store.get("tax:state:v1").await.unwrap();let mut wrong=paired(&family);
    wrong.primary_inputs["employment_income_cents"]=json!(5_000_001);wrong.spouse_inputs["instalments_cents"]=json!(99);
    let rejection=t.save_family_pair_for(fid,wrong,&a).await.err().expect("contradictory grouped key must be rejected");assert_eq!(rejection.0.as_u16(),400);
    assert_eq!(store.get("tax:state:v1").await.unwrap(),state_before_rejection,"neither member, history, audit nor pending job may change on rejection");
    let mut edit=paired(&family);edit.primary_inputs["instalments_cents"]=json!(13);edit.spouse_inputs["instalments_cents"]=json!(27);
    let saved=t.save_family_pair_for(fid,edit,&a).await.unwrap_or_else(|e|panic!("paired non-group edit: {}",e.1));
    for (side,old,instalments) in [("primary",&primary,13),("spouse",&spouse,27)] {
        let r:TaxReturn=serde_json::from_value(saved[side].clone()).unwrap();assert_eq!(r.slip_groups,old.slip_groups);assert_eq!(r.inputs["employment_income_cents"],old.inputs["employment_income_cents"]);assert_eq!(r.inputs["tax_withheld_cents"],0);assert_eq!(r.inputs["payroll_verified"],false);assert_eq!(r.inputs["instalments_cents"],instalments);
        assert_eq!(r.revision,old.revision+1);assert_eq!(r.input_revision,old.input_revision+1);assert_eq!(r.status,"draft");assert!(r.calculation.is_none()&&r.review.is_none()&&r.preparer_signoff.is_none());
        assert_eq!(serde_json::to_value(&r.calculations).unwrap(),serde_json::to_value(&old.calculations).unwrap());assert_eq!(r.calculations.len(),1);
        assert_eq!(r.calculations[0].input_document.as_ref().unwrap().slip_groups.as_ref().unwrap(),&old.slip_groups);
    }
    let after_report=t.report(report_request(),Some(&a)).await.unwrap_or_else(|e|panic!("after report: {}",e.1));assert_eq!(after_report["financial_approval"],false);assert_ne!(before_report["snapshot_sha256"],after_report["snapshot_sha256"]);
    for row in after_report["rows"].as_array().unwrap(){assert_eq!(row["calculation_state"],"none");assert!(row["line:15000"].is_null()&&row["total:income_tax"].is_null());}
    let returns_before_steps=[t.get(&primary.id).await.unwrap_or_else(|e|panic!("primary reload: {}",e.1)),t.get(&spouse.id).await.unwrap_or_else(|e|panic!("spouse reload: {}",e.1))];
    let mut done=job.clone();for n in 0..2 {done=t.batch_step(&a,&job.id,Step{expected_revision:done.revision,request_key:format!("synthetic-stale-step-{n}"),company_ref:Some("A".into())}).await.unwrap_or_else(|e|panic!("batch step: {}",e.1));}
    assert_eq!(done.status,"completed_with_holds");assert_eq!(done.original_manifest,job.original_manifest);assert_eq!(done.original_manifest_sha256,job.original_manifest_sha256);
    for item in &done.items{assert_eq!(item.state,ItemState::Held);assert_eq!(item.outcome.as_ref().unwrap()["code"],"stale_selection");assert!(item.artifact.is_none());}
    let returns_after_steps=[t.get(&primary.id).await.unwrap_or_else(|e|panic!("primary final: {}",e.1)),t.get(&spouse.id).await.unwrap_or_else(|e|panic!("spouse final: {}",e.1))];
    assert_eq!(serde_json::to_vec(&returns_after_steps).unwrap(),serde_json::to_vec(&returns_before_steps).unwrap(),"stale batch must not recalculate or modify either saved return");
    assert_eq!(t.report(report_request(),Some(&a)).await.unwrap_or_else(|e|panic!("final report: {}",e.1)),after_report,"batch bookkeeping must not restore invalidated totals or change the native report");
}
