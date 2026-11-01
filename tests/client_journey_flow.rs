use nxt_os::client_journey::*;

fn bot() -> Actor { Actor::Automation { executor: "test".into() } }
fn cpa() -> Actor { Actor::Human { name: "Reviewer".into(), role: "CPA".into() } }

fn advance_to(def: &JourneyDef, j: &mut ClientJourney, key: &str) {
    for s in def.steps.clone() {
        if s.key == key { break; }
        j.start(def, &s.key, bot(), "t").unwrap();
        if j.finish_work(def, &s.key, bot(), "t").unwrap() == StepStatus::AwaitingApproval {
            j.approve(def, &s.key, cpa(), "t", "approved").unwrap();
        }
    }
}

#[test]
fn walk_loads_every_step_has_action_outcome_gate_and_only_legal_human_gates() {
    let def = JourneyDef::load().unwrap();
    assert!(!def.principle.to_lowercase().contains("cvi"));
    assert!(!def.steps.is_empty());
    for s in &def.steps {
        assert!(!s.action.is_empty() && !s.outcome.is_empty() && !s.gate_check.is_empty(), "{}", s.key);
    }
    let human: Vec<_> = def.steps.iter().filter(|s| s.gate == Gate::Human).map(|s| s.key.as_str()).collect();
    assert_eq!(human, vec!["engagement_letter", "filing_authorization", "filing_submission"]);
    assert!(def.step("data_migration").unwrap().executor.contains("scripts/autonomous-client-path.py"));
    let phases: Vec<_> = def.steps.iter().map(|s| s.phase.as_str()).collect();
    let mut seen = phases.clone();
    seen.dedup();
    assert_eq!(seen, def.phases.iter().map(String::as_str).collect::<Vec<_>>(), "phases appear in walk order");
    assert!(def.step("books_health_check").unwrap().n < def.step("proposal").unwrap().n);
}

#[test]
fn cannot_start_out_of_order() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c1");
    let err = j.start(&def, "monthly_close", bot(), "t").unwrap_err();
    assert!(matches!(err, JourneyError::PriorStepIncomplete { .. }));
    assert!(j.ledger.is_empty());
}

#[test]
fn human_gate_blocks_automation_approval() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c1");
    advance_to(&def, &mut j, "engagement_letter");
    j.start(&def, "engagement_letter", bot(), "t").unwrap();
    assert_eq!(j.finish_work(&def, "engagement_letter", bot(), "t").unwrap(), StepStatus::AwaitingApproval);
    assert_eq!(j.approve(&def, "engagement_letter", bot(), "t", "x").unwrap_err(), JourneyError::HumanApprovalRequired("engagement_letter".into()));
    assert!(j.start(&def, "client_onboarding", bot(), "t").is_err());
    j.approve(&def, "engagement_letter", cpa(), "t", "signed").unwrap();
    j.start(&def, "client_onboarding", bot(), "t").unwrap();
}

#[test]
fn progress_shows_where_the_client_is_and_what_is_next() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c1");
    let p = j.progress(&def);
    assert_eq!((p.done, p.total, p.percent), (0, def.steps.len(), 0));
    assert_eq!(p.current_step.as_deref(), Some("enquiry_received"));
    assert_eq!(p.next_step.as_deref(), Some("discovery_call"));
    advance_to(&def, &mut j, "books_health_check");
    let p = j.progress(&def);
    assert_eq!(p.phase.as_deref(), Some("diagnosis"));
    assert_eq!(p.current_step.as_deref(), Some("books_health_check"));
    assert!(p.current_action.unwrap().contains("diagnostic"));
    assert!(p.gate_check.is_some());
    assert_eq!(p.next_step.as_deref(), Some("health_check_review"));
}

#[test]
fn full_walk_with_skips_and_ledger() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c2");
    assert_eq!(j.skip(&def, "enquiry_received", bot(), "t", "x").unwrap_err(), JourneyError::NotSkippable("enquiry_received".into()));
    for s in def.steps.clone() {
        if matches!(s.key.as_str(), "payroll_setup" | "cfo_advisory" | "strategic_readiness") {
            j.skip(&def, &s.key, bot(), "t", "not on this rung").unwrap();
            continue;
        }
        j.start(&def, &s.key, bot(), "t").unwrap();
        if j.finish_work(&def, &s.key, bot(), "t").unwrap() == StepStatus::AwaitingApproval {
            j.approve(&def, &s.key, cpa(), "t", "approved").unwrap();
        }
    }
    assert!(j.is_complete(&def));
    assert_eq!(j.progress(&def).percent, 100);
    assert_eq!(j.ledger.iter().map(|e| e.seq).collect::<Vec<_>>(), (1..=j.ledger.len() as u64).collect::<Vec<_>>());
}

#[test]
fn failed_step_can_retry() {
    let def = JourneyDef::load().unwrap();
    let mut j = ClientJourney::new(&def, "c3");
    j.start(&def, "enquiry_received", bot(), "t").unwrap();
    j.fail(&def, "enquiry_received", bot(), "t", "timeout").unwrap();
    j.start(&def, "enquiry_received", bot(), "t").unwrap();
    assert_eq!(j.status_of("enquiry_received"), Some(StepStatus::InProgress));
}

#[test]
fn ladder_has_seven_rungs_with_pricing_tbd_and_triggers() {
    let l = Ladder::load().unwrap();
    let ids: Vec<_> = l.tiers.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, vec!["health_check", "catch_up_cleanup", "monthly_bookkeeping", "payroll", "year_end_tax", "cfo_advisory", "strategic"]);
    for t in &l.tiers {
        assert!(t.pricing_tbd && t.pricing_model.contains("pricing TBD by Orville"), "{}", t.id);
        assert!(!t.deliverables.is_empty() && !t.entry_criteria.is_empty());
        if t.rung > 1 { assert!(!t.next_rung_triggers.is_empty(), "{}", t.id); }
    }
    let def = JourneyDef::load().unwrap();
    for s in def.steps.iter().filter_map(|s| s.offer_point_after_tier.as_deref()) {
        assert!(l.tier(s).is_some(), "offer point references unknown tier {s}");
    }
}

#[test]
fn ladder_recommends_next_rung_from_signals() {
    let l = Ladder::load().unwrap();
    let behind = ClientSignals { months_behind: Some(4.0), ..Default::default() };
    assert_eq!(l.recommend_next("health_check", &behind).unwrap().tier, "catch_up_cleanup");
    let current = ClientSignals { monthly_transactions: Some(120.0), ..Default::default() };
    assert_eq!(l.recommend_next("health_check", &current).unwrap().tier, "monthly_bookkeeping");
    let hiring = ClientSignals { employees: Some(3.0), ..Default::default() };
    assert_eq!(l.recommend_next("monthly_bookkeeping", &hiring).unwrap().tier, "payroll");
    let growth = ClientSignals { revenue_growth_pct: Some(35.0), ..Default::default() };
    assert_eq!(l.recommend_next("year_end_tax", &growth).unwrap().tier, "cfo_advisory");
    assert!(l.recommend_next("monthly_bookkeeping", &ClientSignals::default()).is_none());
    assert!(l.recommend_next("strategic", &growth).is_none());
}

#[test]
fn offer_needs_human_approval_before_send_and_is_ledgered() {
    let def = JourneyDef::load().unwrap();
    let l = Ladder::load().unwrap();
    let mut j = ClientJourney::new(&def, "c4");
    let sig = ClientSignals { months_behind: Some(6.0), reconciliation_exceptions: Some(40.0), ..Default::default() };
    assert_eq!(j.draft_offer(&def, &l, "proposal", &sig, bot(), "t").unwrap_err(), OfferError::NotAnOfferPoint("proposal".into()));
    assert_eq!(j.draft_offer(&def, &l, "books_health_check", &sig, bot(), "t").unwrap_err(), OfferError::StepNotReached("books_health_check".into()));
    advance_to(&def, &mut j, "books_health_check");
    j.start(&def, "books_health_check", bot(), "t").unwrap();
    let id = j.draft_offer(&def, &l, "books_health_check", &sig, bot(), "t").unwrap();
    assert_eq!(j.offer_status(id), Some(OfferStatus::Drafted));
    assert_eq!(j.send_offer(id, &l, &StubHooks, bot(), "t").unwrap_err(), OfferError::HumanApprovalRequired(id));
    assert_eq!(j.approve_offer(id, bot(), "t", "x").unwrap_err(), OfferError::HumanApprovalRequired(id));
    j.approve_offer(id, cpa(), "t", "ok to send").unwrap();
    assert!(matches!(j.send_offer(id, &l, &StubHooks, bot(), "t").unwrap(), HookOutcome::StubOnly { .. }));
    assert_eq!(j.offer_status(id), Some(OfferStatus::Sent));
    let statuses: Vec<_> = j.offers.iter().map(|e| e.status).collect();
    assert_eq!(statuses, vec![OfferStatus::Drafted, OfferStatus::Approved, OfferStatus::Sent]);
    assert_eq!(j.offers[0].tier, "catch_up_cleanup");
    // declined offers cannot be sent
    let id2 = j.draft_offer(&def, &l, "books_health_check", &sig, bot(), "t").unwrap();
    j.decline_offer(id2, cpa(), "t", "not now").unwrap();
    assert!(j.send_offer(id2, &l, &StubHooks, bot(), "t").is_err());
}

#[test]
fn hooks_are_stubs_only() {
    let h = StubHooks;
    assert!(matches!(h.request_payment("c", 100, "CAD"), HookOutcome::StubOnly { .. }));
    assert!(matches!(h.request_signature("c", "EL"), HookOutcome::StubOnly { .. }));
    assert!(matches!(h.prepare_filing("c", "T2"), HookOutcome::StubOnly { .. }));
    assert_eq!(h.command_for("c")[1], "scripts/autonomous-client-path.py");
}
