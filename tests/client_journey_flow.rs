use nxt_os::client_journey::*;

fn bot() -> Actor { Actor::Automation { executor: "test".into() } }
fn cpa() -> Actor { Actor::Human { name: "Reviewer".into(), role: "CPA".into() } }

#[test]
fn config_loads_with_20_steps_and_only_legal_human_gates() {
    let def = JourneyDef::load().unwrap();
    assert_eq!(def.steps.len(), 20);
    let human: Vec<_> = def.steps.iter().filter(|s| s.gate == Gate::Human).map(|s| s.key.as_str()).collect();
    assert_eq!(human, vec!["engagement_letter", "filing_authorization", "filing_submission"]);
    assert!(def.step("data_migration").unwrap().executor.contains("scripts/autonomous-client-path.py"));
}

#[test]
fn cannot_start_out_of_order() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c1");
    let err = j.start(&def, "bookkeeping", bot(), "t").unwrap_err();
    assert!(matches!(err, JourneyError::PriorStepIncomplete { .. }));
    assert!(j.ledger.is_empty());
}

#[test]
fn human_gate_blocks_automation_approval() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c1");
    for k in ["website_visit", "lead_capture", "needs_discovery", "service_scoping", "proposal_pricing"] {
        j.start(&def, k, bot(), "t").unwrap();
        assert_eq!(j.finish_work(&def, k, bot(), "t").unwrap(), StepStatus::Completed);
    }
    j.start(&def, "engagement_letter", bot(), "t").unwrap();
    assert_eq!(j.finish_work(&def, "engagement_letter", bot(), "t").unwrap(), StepStatus::AwaitingApproval);
    assert_eq!(j.approve(&def, "engagement_letter", bot(), "t", "x").unwrap_err(), JourneyError::HumanApprovalRequired("engagement_letter".into()));
    assert!(j.start(&def, "client_onboarding", bot(), "t").is_err());
    j.approve(&def, "engagement_letter", cpa(), "t", "signed").unwrap();
    j.start(&def, "client_onboarding", bot(), "t").unwrap();
}

#[test]
fn full_journey_with_skips_and_ledger() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c2");
    assert_eq!(j.skip(&def, "website_visit", bot(), "t", "x").unwrap_err(), JourneyError::NotSkippable("website_visit".into()));
    for s in def.steps.clone() {
        if matches!(s.key.as_str(), "filing_authorization" | "filing_submission") {
            j.skip(&def, &s.key, bot(), "t", "bookkeeping-only engagement").unwrap();
            continue;
        }
        j.start(&def, &s.key, bot(), "t").unwrap();
        if j.finish_work(&def, &s.key, bot(), "t").unwrap() == StepStatus::AwaitingApproval {
            j.approve(&def, &s.key, cpa(), "t", "approved").unwrap();
        }
    }
    assert!(j.is_complete(&def));
    assert_eq!(j.ledger.iter().map(|e| e.seq).collect::<Vec<_>>(), (1..=j.ledger.len() as u64).collect::<Vec<_>>());
}

#[test]
fn failed_step_can_retry() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c3");
    j.start(&def, "website_visit", bot(), "t").unwrap();
    j.fail(&def, "website_visit", bot(), "t", "timeout").unwrap();
    j.start(&def, "website_visit", bot(), "t").unwrap();
    assert_eq!(j.status_of("website_visit"), Some(StepStatus::InProgress));
}

#[test]
fn hooks_are_stubs_only() {
    let h = StubHooks;
    assert!(matches!(h.request_payment("c", 100, "CAD"), HookOutcome::StubOnly { .. }));
    assert!(matches!(h.request_signature("c", "EL"), HookOutcome::StubOnly { .. }));
    assert!(matches!(h.prepare_filing("c", "T2"), HookOutcome::StubOnly { .. }));
    assert_eq!(h.command_for("c")[1], "scripts/autonomous-client-path.py");
}
