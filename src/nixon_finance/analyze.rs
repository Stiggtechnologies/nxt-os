//! Deterministic finance screens over a read-only context.
//!
//! Amounts come from the context or from a packet the caller supplied.
//! A missing statement, budget, firm database, or tax return stays a gap.

use super::models::*;
use chrono::{Datelike, NaiveDate};
use std::collections::{BTreeMap, HashMap};

const EVIDENCE_CAP: usize = 25;

pub fn analyze(
    ctx: &FinanceContext,
    cutover: Option<&CutoverPacket>,
    surfaces: &[SurfacePacket],
) -> Result<Analysis, String> {
    let as_of = parse_day(&ctx.as_of)?;
    if ctx.source_class != SourceClass::WorkspaceBooks {
        return Err("the ledger projection is always workspace_books".into());
    }
    if let Some(packet) = cutover {
        check_packet_header(
            &packet.source_class,
            &packet.as_of,
            &packet.currency,
            &packet.prepared_by,
            &packet.period_lock,
            packet.lines.len(),
            500,
        )?;
        for line in &packet.lines {
            if line.debit < 0 || line.credit < 0 {
                return Err("cutover amounts must be non-negative cents".into());
            }
            if line.account_code.trim().is_empty() {
                return Err("a cutover line needs an account code".into());
            }
        }
    }
    for surface in surfaces {
        check_packet_header(
            &surface.source_class,
            &surface.as_of,
            "CAD",
            &surface.prepared_by,
            &None,
            surface.items.len(),
            200,
        )?;
        if surface.surface != "firm" && surface.surface != "tax" {
            return Err("a surface packet must be firm or tax".into());
        }
        for item in &surface.items {
            if item.id.trim().is_empty() {
                return Err("a checklist item needs an id".into());
            }
        }
    }

    let mut findings = Vec::new();
    let mut proposals = Vec::new();
    let mut verifications = Vec::new();

    let accounts: HashMap<&str, &CtxAccount> =
        ctx.accounts.iter().map(|a| (a.id.as_str(), a)).collect();
    let by_code: HashMap<&str, &CtxAccount> =
        ctx.accounts.iter().map(|a| (a.code.as_str(), a)).collect();
    let entries: HashMap<&str, &CtxEntry> =
        ctx.entries.iter().map(|e| (e.id.as_str(), e)).collect();

    verify_entries(ctx, &mut findings, &mut proposals, &mut verifications);
    verify_trial_balance(
        ctx,
        as_of,
        &mut findings,
        &mut proposals,
        &mut verifications,
    );
    link_documents(
        ctx,
        &entries,
        &mut findings,
        &mut proposals,
        &mut verifications,
    );
    bank_feed(
        ctx,
        as_of,
        &accounts,
        &mut findings,
        &mut proposals,
        &mut verifications,
    );
    reconciliation(
        ctx,
        &accounts,
        &mut findings,
        &mut proposals,
        &mut verifications,
    );
    cash_movement(ctx, as_of, &mut findings, &mut verifications);
    shift_screen(ctx, as_of, &mut findings, &mut proposals);
    duplicates(ctx, &mut findings, &mut proposals);
    if let Some(packet) = cutover {
        cutover_checks(
            ctx,
            packet,
            &by_code,
            &mut findings,
            &mut proposals,
            &mut verifications,
        );
    } else {
        verifications.push(Verification {
            id: "v_cutover".into(),
            name: "cutover".into(),
            state: "not_run".into(),
            detail: "No source trial balance was supplied. Nixon did not invent one.".into(),
            evidence_ids: vec![],
        });
    }
    for surface in surfaces {
        surface_checks(surface, &mut findings, &mut proposals, &mut verifications);
    }
    if surfaces.iter().all(|s| s.surface != "firm") {
        verifications.push(not_installed("firm"));
    }
    if surfaces.iter().all(|s| s.surface != "tax") {
        verifications.push(not_installed("tax"));
    }

    findings.sort_by(|a, b| a.id.cmp(&b.id));
    proposals.sort_by(|a, b| a.id.cmp(&b.id));
    verifications.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Analysis {
        findings,
        proposals,
        verifications,
    })
}

fn not_installed(surface: &str) -> Verification {
    Verification {
        id: format!("v_{surface}_module"),
        name: format!("{surface}_module"),
        state: "not_run".into(),
        detail: format!(
            "The {surface} module is installed. This review did not read its records because no {surface} packet was supplied. Nixon will not file, post, or complete that module's work."
        ),
        evidence_ids: vec![],
    }
}

fn check_packet_header(
    class: &SourceClass,
    as_of: &str,
    currency: &str,
    prepared_by: &str,
    period_lock: &Option<String>,
    n: usize,
    cap: usize,
) -> Result<(), String> {
    if !class.allowed_on_packet() {
        return Err("a packet must name live_external, simulated, or customer_operational".into());
    }
    parse_day(as_of)?;
    if let Some(lock) = period_lock {
        parse_day(lock)?;
    }
    if currency != "CAD" {
        return Err("Nixon compares CAD books only and does not convert currency".into());
    }
    if prepared_by.trim().chars().count() < 2 {
        return Err("a packet needs the person who prepared it".into());
    }
    if n > cap {
        return Err(format!("a packet is limited to {cap} lines"));
    }
    Ok(())
}

fn parse_day(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| format!("invalid date '{s}', expected YYYY-MM-DD"))
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn month_start(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap()
}

fn net_through<'a>(ctx: &'a FinanceContext, to: &str) -> HashMap<&'a str, i64> {
    let mut m: HashMap<&str, i64> = HashMap::new();
    for e in ctx
        .entries
        .iter()
        .filter(|e| !e.voided && e.date.as_str() <= to)
    {
        for line in &e.lines {
            *m.entry(line.account_id.as_str()).or_default() += line.debit - line.credit;
        }
    }
    m
}

fn activity<'a>(ctx: &'a FinanceContext, from: &str, to: &str) -> HashMap<&'a str, i64> {
    let mut m: HashMap<&str, i64> = HashMap::new();
    for e in ctx
        .entries
        .iter()
        .filter(|e| !e.voided && e.date.as_str() >= from && e.date.as_str() <= to)
    {
        for line in &e.lines {
            *m.entry(line.account_id.as_str()).or_default() += line.debit - line.credit;
        }
    }
    m
}

fn measured(id: &str, reference: &str, date: Option<String>, note: &str) -> Evidence {
    Evidence {
        id: id.into(),
        class: "measured".into(),
        level: "verified_measurement".into(),
        role: "supporting".into(),
        source_system: "nxt.ledger".into(),
        reference: reference.into(),
        source_date: date,
        note: note.into(),
    }
}

fn documented(id: &str, reference: &str, system: &str, note: &str) -> Evidence {
    Evidence {
        id: id.into(),
        class: "documented".into(),
        level: "confirmed_history".into(),
        role: "supporting".into(),
        source_system: system.into(),
        reference: reference.into(),
        source_date: None,
        note: note.into(),
    }
}

fn hypothesis(
    finding_id: &str,
    statement: &str,
    evidence_ids: Vec<String>,
    gap_ids: Vec<String>,
    refusal: &str,
) -> Hypothesis {
    Hypothesis {
        id: format!("h_{finding_id}"),
        label: "hypothesis".into(),
        statement: statement.into(),
        confidence: None,
        confidence_refusal: refusal.into(),
        evidence_ids,
        gap_ids,
    }
}

fn push_ack(
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    mut finding: Finding,
    prepared_by: &str,
) {
    if finding.requires_human_review {
        let proposal = acknowledge_proposal(&finding.id, &finding.title, prepared_by);
        finding.proposal_id = Some(proposal.id.clone());
        proposals.push(proposal);
    }
    findings.push(finding);
}

fn verify_entries(
    ctx: &FinanceContext,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    let mut bad = Vec::new();
    for e in ctx.entries.iter().filter(|e| !e.voided) {
        let (d, c) = e
            .lines
            .iter()
            .fold((0i64, 0i64), |(d, c), l| (d + l.debit, c + l.credit));
        let both = e.lines.iter().any(|l| l.debit != 0 && l.credit != 0);
        if d != c || both {
            bad.push(e);
        }
    }
    if bad.is_empty() {
        verifications.push(Verification {
            id: "v_entries_balance".into(),
            name: "entries_balance".into(),
            state: "passed".into(),
            detail:
                "Every non-void journal balances, and no line carries both a debit and a credit."
                    .into(),
            evidence_ids: vec![],
        });
        return;
    }
    let id = "f_unbalanced_entries";
    let evidence: Vec<Evidence> = bad
        .iter()
        .take(EVIDENCE_CAP)
        .enumerate()
        .map(|(i, e)| {
            measured(
                &format!("e_{id}_{i}"),
                &format!("entry:{}", e.id),
                Some(e.date.clone()),
                &format!("{} is not a balanced journal", e.id),
            )
        })
        .collect();
    let evidence_ids: Vec<String> = evidence.iter().map(|e| e.id.clone()).collect();
    verifications.push(Verification {
        id: "v_entries_balance".into(),
        name: "entries_balance".into(),
        state: "failed".into(),
        detail: format!("{} non-void journal(s) do not balance", bad.len()),
        evidence_ids: evidence_ids.clone(),
    });
    push_ack(
        findings,
        proposals,
        Finding {
            id: id.into(),
            capability: "anomaly".into(),
            risk_level: "critical".into(),
            title: "A journal does not balance".into(),
            summary: format!(
                "{} non-void journal(s) have debits that do not equal credits, or a line with both a debit and a credit. Nixon did not post a correcting entry.",
                bad.len()
            ),
            requires_human_review: true,
            autonomy_level: "conditional".into(),
            evidence,
            hypotheses: vec![],
            gaps: vec![],
            proposal_id: None,
        },
        "nixon",
    );
}

fn verify_trial_balance(
    ctx: &FinanceContext,
    as_of: NaiveDate,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    let to = ymd(as_of);
    let net = net_through(ctx, &to);
    let (mut debit, mut credit) = (0i64, 0i64);
    for value in net.values() {
        if *value > 0 {
            debit += value;
        } else {
            credit += -value;
        }
    }
    let reference = format!("report:trial-balance:{to}");
    if debit == credit {
        verifications.push(Verification {
            id: "v_trial_balance".into(),
            name: "trial_balance".into(),
            state: "passed".into(),
            detail: format!("Trial balance at {to} debits and credits are both {debit} cents."),
            evidence_ids: vec![],
        });
        return;
    }
    let evidence = vec![measured(
        "e_tb",
        &reference,
        Some(to.clone()),
        &format!("trial balance debits {debit} credits {credit}"),
    )];
    verifications.push(Verification {
        id: "v_trial_balance".into(),
        name: "trial_balance".into(),
        state: "failed".into(),
        detail: format!(
            "Trial balance at {to} is out by {} cents. Nixon did not force it to balance.",
            (debit - credit).abs()
        ),
        evidence_ids: vec!["e_tb".into()],
    });
    push_ack(
        findings,
        proposals,
        Finding {
            id: "f_trial_balance".into(),
            capability: "anomaly".into(),
            risk_level: "critical".into(),
            title: "The trial balance does not balance".into(),
            summary: format!("At {to}, debits are {debit} cents and credits are {credit} cents."),
            requires_human_review: true,
            autonomy_level: "conditional".into(),
            evidence,
            hypotheses: vec![],
            gaps: vec![],
            proposal_id: None,
        },
        "nixon",
    );
}

fn link_documents(
    ctx: &FinanceContext,
    entries: &HashMap<&str, &CtxEntry>,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    let mut broken = Vec::new();
    for doc in &ctx.docs {
        let entry = entries.get(doc.entry_id.as_str()).copied();
        if entry.is_none_or(|e| e.voided && !doc.voided) {
            broken.push(format!("doc:{}", doc.id));
        }
    }
    for pay in &ctx.payments {
        let entry = entries.get(pay.entry_id.as_str()).copied();
        if entry.is_none_or(|e| e.voided && !pay.voided) {
            broken.push(format!("payment:{}", pay.id));
        }
    }
    if broken.is_empty() {
        verifications.push(Verification {
            id: "v_document_links".into(),
            name: "document_links".into(),
            state: "passed".into(),
            detail: "Invoices, bills, and payments point at a journal that is still on the books."
                .into(),
            evidence_ids: vec![],
        });
        return;
    }
    let id = "f_document_links";
    let evidence: Vec<Evidence> = broken
        .iter()
        .take(EVIDENCE_CAP)
        .enumerate()
        .map(|(i, reference)| {
            documented(
                &format!("e_{id}_{i}"),
                reference,
                "nxt.ledger",
                "the document and its journal disagree",
            )
        })
        .collect();
    let evidence_ids: Vec<String> = evidence.iter().map(|e| e.id.clone()).collect();
    verifications.push(Verification {
        id: "v_document_links".into(),
        name: "document_links".into(),
        state: "failed".into(),
        detail: format!(
            "{} document link(s) do not match a live journal",
            broken.len()
        ),
        evidence_ids: evidence_ids.clone(),
    });
    push_ack(
        findings,
        proposals,
        Finding {
            id: id.into(),
            capability: "anomaly".into(),
            risk_level: "high".into(),
            title: "A document is detached from its journal".into(),
            summary: "An invoice, bill, or payment points at a missing or void journal while the document itself is still open. Nixon did not void or repost it.".into(),
            requires_human_review: true,
            autonomy_level: "conditional".into(),
            evidence,
            hypotheses: vec![],
            gaps: vec![],
            proposal_id: None,
        },
        "nixon",
    );
}

fn bank_feed(
    ctx: &FinanceContext,
    as_of: NaiveDate,
    accounts: &HashMap<&str, &CtxAccount>,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    let pending: Vec<&CtxFeed> = ctx.feed.iter().filter(|f| f.status == "pending").collect();
    let future: Vec<&CtxFeed> = pending
        .iter()
        .copied()
        .filter(|f| f.date.as_str() > ctx.as_of.as_str())
        .collect();
    let unknown: Vec<&CtxFeed> = ctx
        .feed
        .iter()
        .filter(|f| {
            accounts
                .get(f.account_id.as_str())
                .is_none_or(|a| a.role != "bank" && a.role != "credit_card")
        })
        .collect();
    if pending.is_empty() && unknown.is_empty() {
        verifications.push(Verification {
            id: "v_bank_feed".into(),
            name: "bank_feed".into(),
            state: "passed".into(),
            detail: "No bank-feed row is waiting to be categorized.".into(),
            evidence_ids: vec![],
        });
    } else if !pending.is_empty() {
        let id = "f_pending_feed";
        let mut evidence = Vec::new();
        for (i, item) in pending.iter().take(EVIDENCE_CAP).enumerate() {
            evidence.push(measured(
                &format!("e_{id}_{i}"),
                &format!("feed:{}", item.id),
                Some(item.date.clone()),
                &format!(
                    "{} {} cents on {}",
                    item.description, item.amount, item.date
                ),
            ));
        }
        let mut gaps = Vec::new();
        if pending.len() > EVIDENCE_CAP {
            gaps.push(Gap {
                id: format!("g_{id}_more"),
                note: format!(
                    "{} further pending rows are not listed individually",
                    pending.len() - EVIDENCE_CAP
                ),
            });
        }
        let evidence_ids: Vec<String> = evidence.iter().map(|e| e.id.clone()).collect();
        verifications.push(Verification {
            id: "v_bank_feed".into(),
            name: "bank_feed".into(),
            state: "failed".into(),
            detail: format!("{} imported bank row(s) are still pending", pending.len()),
            evidence_ids: evidence_ids.clone(),
        });
        push_ack(
            findings,
            proposals,
            Finding {
                id: id.into(),
                capability: "reconciliation".into(),
                risk_level: "medium".into(),
                title: format!("{} bank row(s) are not in the books", pending.len()),
                summary: "Imported bank activity is still pending. Nixon did not categorize it or post a journal.".into(),
                requires_human_review: true,
                autonomy_level: "conditional".into(),
                evidence,
                hypotheses: vec![],
                gaps,
                proposal_id: None,
            },
            "nixon",
        );
    }
    if !future.is_empty() {
        let id = "f_future_feed";
        let evidence: Vec<Evidence> = future
            .iter()
            .take(EVIDENCE_CAP)
            .enumerate()
            .map(|(i, item)| {
                measured(
                    &format!("e_{id}_{i}"),
                    &format!("feed:{}", item.id),
                    Some(item.date.clone()),
                    "dated after the review date",
                )
            })
            .collect();
        push_ack(
            findings,
            proposals,
            Finding {
                id: id.into(),
                capability: "anomaly".into(),
                risk_level: "medium".into(),
                title: "A bank row is dated after the review date".into(),
                summary: format!(
                    "{} pending row(s) are dated after {}.",
                    future.len(),
                    ymd(as_of)
                ),
                requires_human_review: true,
                autonomy_level: "conditional".into(),
                evidence,
                hypotheses: vec![],
                gaps: vec![],
                proposal_id: None,
            },
            "nixon",
        );
    }
    if !unknown.is_empty() {
        let id = "f_feed_account";
        let evidence: Vec<Evidence> = unknown
            .iter()
            .take(EVIDENCE_CAP)
            .enumerate()
            .map(|(i, item)| {
                measured(
                    &format!("e_{id}_{i}"),
                    &format!("feed:{}", item.id),
                    Some(item.date.clone()),
                    "feed account is not a bank or card",
                )
            })
            .collect();
        push_ack(
            findings,
            proposals,
            Finding {
                id: id.into(),
                capability: "anomaly".into(),
                risk_level: "high".into(),
                title: "A bank row points at an account Nixon cannot treat as cash".into(),
                summary: "The feed names an account that is missing or is not a bank or credit card. Nixon did not move the row.".into(),
                requires_human_review: true,
                autonomy_level: "conditional".into(),
                evidence,
                hypotheses: vec![],
                gaps: vec![],
                proposal_id: None,
            },
            "nixon",
        );
        if pending.is_empty() {
            verifications.push(Verification {
                id: "v_bank_feed".into(),
                name: "bank_feed".into(),
                state: "failed".into(),
                detail: "A feed row does not point at a bank or card account.".into(),
                evidence_ids: vec![],
            });
        }
    }
}

fn reconciliation(
    ctx: &FinanceContext,
    accounts: &HashMap<&str, &CtxAccount>,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    if ctx.reconciliations.is_empty() {
        verifications.push(Verification {
            id: "v_reconciliation".into(),
            name: "reconciliation".into(),
            state: "not_run".into(),
            detail: "No reconciliation is stored. Nixon did not invent a statement balance.".into(),
            evidence_ids: vec![],
        });
        return;
    }
    let mut latest: HashMap<&str, &CtxRecon> = HashMap::new();
    for rec in &ctx.reconciliations {
        latest
            .entry(rec.account_id.as_str())
            .and_modify(|cur| {
                if rec.statement_date > cur.statement_date {
                    *cur = rec;
                }
            })
            .or_insert(rec);
    }
    let mut failed = false;
    for rec in latest.values() {
        let Some(account) = accounts.get(rec.account_id.as_str()) else {
            continue;
        };
        let sign = |line: &CtxLine| {
            if account.debit_normal {
                line.debit - line.credit
            } else {
                line.credit - line.debit
            }
        };
        let cleared: i64 = ctx
            .entries
            .iter()
            .filter(|e| !e.voided)
            .flat_map(|e| {
                e.lines
                    .iter()
                    .filter(|l| {
                        l.account_id == rec.account_id
                            && l.cleared
                            && e.date.as_str() <= rec.statement_date.as_str()
                    })
                    .map(sign)
            })
            .sum();
        let uncleared: Vec<(String, String)> = ctx
            .entries
            .iter()
            .filter(|e| !e.voided && e.date.as_str() <= rec.statement_date.as_str())
            .flat_map(|e| {
                e.lines.iter().enumerate().filter_map(move |(i, l)| {
                    (l.account_id == rec.account_id && !l.cleared)
                        .then(|| (format!("entry:{}:{i}", e.id), e.date.clone()))
                })
            })
            .collect();
        if cleared != rec.statement_balance || !uncleared.is_empty() {
            failed = true;
            let id = format!("f_recon_{}", account.code);
            let mut evidence = vec![
                measured(
                    &format!("e_{id}_rec"),
                    &format!("recon:{}", rec.id),
                    Some(rec.statement_date.clone()),
                    &format!("statement {}", rec.statement_balance),
                ),
                measured(
                    &format!("e_{id}_cleared"),
                    &format!("account:{}", account.id),
                    Some(rec.statement_date.clone()),
                    &format!("cleared book balance {cleared}"),
                ),
            ];
            for (i, (reference, date)) in uncleared.iter().take(EVIDENCE_CAP).enumerate() {
                evidence.push(measured(
                    &format!("e_{id}_{i}"),
                    reference,
                    Some(date.clone()),
                    "uncleared on or before the statement date",
                ));
            }
            let mut gaps = Vec::new();
            if uncleared.len() > EVIDENCE_CAP {
                gaps.push(Gap {
                    id: format!("g_{id}_more"),
                    note: format!(
                        "{} further uncleared lines are not listed",
                        uncleared.len() - EVIDENCE_CAP
                    ),
                });
            }
            if cleared != rec.statement_balance {
                gaps.push(Gap {
                    id: format!("g_{id}_diff"),
                    note: format!("Cleared lines sum to {cleared} cents and the stored statement is {} cents. Nixon did not draft a plug journal.", rec.statement_balance),
                });
            }
            push_ack(
                findings,
                proposals,
                Finding {
                    id: id,
                    capability: "reconciliation".into(),
                    risk_level: "high".into(),
                    title: format!("{} reconciliation does not tie", account.name),
                    summary: format!(
                        "Through {}, {} uncleared book line(s) remain and the cleared balance is {cleared} cents against a statement of {} cents.",
                        rec.statement_date,
                        uncleared.len(),
                        rec.statement_balance
                    ),
                    requires_human_review: true,
                    autonomy_level: "conditional".into(),
                    evidence,
                    hypotheses: vec![],
                    gaps,
                    proposal_id: None,
                },
                "nixon",
            );
        }
    }
    verifications.push(Verification {
        id: "v_reconciliation".into(),
        name: "reconciliation".into(),
        state: if failed { "failed" } else { "passed" }.into(),
        detail: if failed {
            "At least one stored reconciliation does not match the cleared book lines.".into()
        } else {
            "Stored reconciliations match the cleared lines on or before each statement date."
                .into()
        },
        evidence_ids: vec![],
    });
}

fn cash_movement(
    ctx: &FinanceContext,
    as_of: NaiveDate,
    findings: &mut Vec<Finding>,
    verifications: &mut Vec<Verification>,
) {
    let banks: Vec<&CtxAccount> = ctx.accounts.iter().filter(|a| a.role == "bank").collect();
    let bank_lines = ctx
        .entries
        .iter()
        .filter(|e| !e.voided)
        .flat_map(|e| e.lines.iter())
        .any(|l| banks.iter().any(|a| a.id == l.account_id));
    if banks.is_empty() || !bank_lines {
        verifications.push(Verification {
            id: "v_cash".into(),
            name: "cash_movement".into(),
            state: "not_run".into(),
            detail: "No bank activity is on the books, so Nixon did not calculate a cash movement or invent a prior balance.".into(),
            evidence_ids: vec![],
        });
        findings.push(Finding {
            id: "f_cash_movement".into(),
            capability: "cash_flow".into(),
            risk_level: "low".into(),
            title: "Cash movement was not calculated".into(),
            summary: "There is no bank activity to compare. This is not a cash-flow statement.".into(),
            requires_human_review: false,
            autonomy_level: "advisory".into(),
            evidence: vec![],
            hypotheses: vec![],
            gaps: vec![Gap { id: "g_cash_movement".into(), note: "No bank lines are posted. A full cash-flow statement is not produced by this screen.".into() }],
            proposal_id: None,
        });
        return;
    }
    let prior_end = month_start(as_of) - chrono::Duration::days(1);
    let now_net = net_through(ctx, &ymd(as_of));
    let prior_net = net_through(ctx, &ymd(prior_end));
    let sum = |net: &HashMap<&str, i64>| {
        banks
            .iter()
            .map(|a| {
                if a.debit_normal {
                    *net.get(a.id.as_str()).unwrap_or(&0)
                } else {
                    -net.get(a.id.as_str()).unwrap_or(&0)
                }
            })
            .sum::<i64>()
    };
    let now = sum(&now_net);
    let prior = sum(&prior_net);
    let mut evidence = Vec::new();
    for (i, account) in banks.iter().enumerate() {
        let current = if account.debit_normal {
            *now_net.get(account.id.as_str()).unwrap_or(&0)
        } else {
            -now_net.get(account.id.as_str()).unwrap_or(&0)
        };
        evidence.push(measured(
            &format!("e_cash_{i}"),
            &format!("account:{}", account.id),
            Some(ymd(as_of)),
            &format!("{} ending cash {current} cents; prior month-end is included in the {prior} cent total", account.name),
        ));
    }
    verifications.push(Verification {
        id: "v_cash".into(),
        name: "cash_movement".into(),
        state: "passed".into(),
        detail: format!(
            "Bank cash moved from {prior} cents at {} to {now} cents at {}.",
            ymd(prior_end),
            ymd(as_of)
        ),
        evidence_ids: evidence.iter().map(|e| e.id.clone()).collect(),
    });
    findings.push(Finding {
        id: "f_cash_movement".into(),
        capability: "cash_flow".into(),
        risk_level: "low".into(),
        title: "Bank cash versus the prior month-end".into(),
        summary: format!(
            "Ending cash on bank accounts is {now} cents at {}, against {prior} cents at {}. The change is {} cents. This is bank cash, not a complete cash-flow statement.",
            ymd(as_of),
            ymd(prior_end),
            now - prior
        ),
        requires_human_review: false,
        autonomy_level: "advisory".into(),
        evidence,
        hypotheses: vec![],
        gaps: vec![Gap { id: "g_cash_statement".into(), note: "Operating, investing, and financing sections are not computed. There is no cash-flow budget on the books.".into() }],
        proposal_id: None,
    });
}

fn shift_screen(
    ctx: &FinanceContext,
    as_of: NaiveDate,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
) {
    let current_start = month_start(as_of);
    let prior_end = current_start - chrono::Duration::days(1);
    let prior_start = month_start(prior_end);
    let current = activity(ctx, &ymd(current_start), &ymd(as_of));
    let prior = activity(ctx, &ymd(prior_start), &ymd(prior_end));
    let mut hits: Vec<(&CtxAccount, i64, i64)> = Vec::new();
    for account in ctx
        .accounts
        .iter()
        .filter(|a| a.kind == "expense" || a.kind == "income")
    {
        let cur_net = *current.get(account.id.as_str()).unwrap_or(&0);
        let prev_net = *prior.get(account.id.as_str()).unwrap_or(&0);
        let cur = if account.debit_normal {
            cur_net
        } else {
            -cur_net
        };
        let prev = if account.debit_normal {
            prev_net
        } else {
            -prev_net
        };
        let delta = cur - prev;
        let ratio = prev != 0
            && (i128::from(delta.abs()) * 10_000 / i128::from(prev.abs()))
                >= i128::from(SHIFT_MIN_RATIO_BP);
        if delta.abs() >= SHIFT_MIN_CENTS && (prev == 0 || ratio) {
            hits.push((account, prev, cur));
        }
    }
    hits.sort_by(|a, b| (b.2 - b.1).abs().cmp(&(a.2 - a.1).abs()));
    if hits.is_empty() {
        return;
    }
    let id = "f_shift_screen";
    let mut evidence = Vec::new();
    let mut gaps = vec![Gap {
        id: format!("g_{id}_budget"),
        note: format!(
            "{SHIFT_SCREEN} compares each account with the prior month. The {} cent and {} basis-point gates are screening constants, not a budget or a management limit. No forecast is on the books.",
            SHIFT_MIN_CENTS, SHIFT_MIN_RATIO_BP
        ),
    }];
    for (i, (account, prev, cur)) in hits.iter().take(15).enumerate() {
        evidence.push(measured(
            &format!("e_{id}_{i}"),
            &format!("account:{}", account.id),
            Some(ymd(as_of)),
            &format!(
                "{} {} cents in {} and {} cents in {}",
                account.name,
                prev,
                ymd(prior_start),
                cur,
                ymd(current_start)
            ),
        ));
    }
    if hits.len() > 15 {
        gaps.push(Gap {
            id: format!("g_{id}_more"),
            note: format!(
                "{} further accounts tripped the screen and are not listed",
                hits.len() - 15
            ),
        });
    }
    let evidence_ids: Vec<String> = evidence.iter().map(|e| e.id.clone()).collect();
    let gap_ids: Vec<String> = gaps.iter().map(|g| g.id.clone()).collect();
    let named = hits
        .iter()
        .take(3)
        .map(|(a, prev, cur)| format!("{} ({prev} → {cur} cents)", a.name))
        .collect::<Vec<_>>()
        .join(", ");
    push_ack(
        findings,
        proposals,
        Finding {
            id: id.into(),
            capability: "variance".into(),
            risk_level: "medium".into(),
            title: "A month-to-month shift tripped the screen".into(),
            summary: format!(
                "{named}. This is a hypothesis about a shift, not a cause, and not a variance to a plan."
            ),
            requires_human_review: true,
            autonomy_level: "conditional".into(),
            evidence: evidence.clone(),
            hypotheses: vec![hypothesis(
                id,
                &format!(
                    "Activity changed between {} and {}. Nixon has no evidence for why.",
                    ymd(prior_start),
                    ymd(as_of)
                ),
                evidence_ids,
                gap_ids,
                "A month-to-month ratio is not evidence of a cause, and this screen is not a budget.",
            )],
            gaps,
            proposal_id: None,
        },
        "nixon",
    );
}

fn duplicates(ctx: &FinanceContext, findings: &mut Vec<Finding>, proposals: &mut Vec<Proposal>) {
    let mut groups: BTreeMap<String, Vec<&CtxEntry>> = BTreeMap::new();
    for entry in ctx.entries.iter().filter(|e| !e.voided) {
        let mut parts: Vec<String> = entry
            .lines
            .iter()
            .map(|l| format!("{}:{}:{}", l.account_id, l.debit, l.credit))
            .collect();
        parts.sort();
        let key = format!(
            "{}|{}|{}|{}",
            entry.date,
            entry.source,
            entry.memo,
            parts.join(",")
        );
        groups.entry(key).or_default().push(entry);
    }
    let mut pairs = Vec::new();
    for group in groups.values() {
        if group.len() > 1 {
            pairs.push((group[0], group[1]));
        }
    }
    if pairs.is_empty() {
        return;
    }
    let id = "f_possible_duplicate";
    let mut evidence = Vec::new();
    for (i, (a, b)) in pairs.iter().take(10).enumerate() {
        evidence.push(measured(
            &format!("e_{id}_{i}a"),
            &format!("entry:{}", a.id),
            Some(a.date.clone()),
            "first journal in a matching pair",
        ));
        evidence.push(measured(
            &format!("e_{id}_{i}b"),
            &format!("entry:{}", b.id),
            Some(b.date.clone()),
            "second journal in a matching pair",
        ));
    }
    let evidence_ids: Vec<String> = evidence.iter().map(|e| e.id.clone()).collect();
    let gap = Gap { id: format!("g_{id}"), note: "The bank statement was not compared with this pair, so a repeated memo is not proof of a double post.".into() };
    push_ack(
        findings,
        proposals,
        Finding {
            id: id.into(),
            capability: "anomaly".into(),
            risk_level: "medium".into(),
            title: "Two journals look the same".into(),
            summary: format!(
                "{} pair(s) share a date, memo, source, and lines. Nixon did not void either journal.",
                pairs.len()
            ),
            requires_human_review: true,
            autonomy_level: "conditional".into(),
            evidence,
            hypotheses: vec![hypothesis(
                id,
                "These journals might be the same event posted twice.",
                evidence_ids,
                vec![gap.id.clone()],
                "A matching memo is a hypothesis, not a finding that the books are wrong.",
            )],
            gaps: vec![gap],
            proposal_id: None,
        },
        "nixon",
    );
}

fn cutover_checks(
    ctx: &FinanceContext,
    packet: &CutoverPacket,
    by_code: &HashMap<&str, &CtxAccount>,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    let preparer = packet.prepared_by.trim();
    let mut source: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let (mut src_debit, mut src_credit) = (0i64, 0i64);
    for line in &packet.lines {
        let code = line.account_code.trim().to_string();
        let slot = source.entry(code).or_insert((0, 0));
        slot.0 += line.debit;
        slot.1 += line.credit;
        src_debit += line.debit;
        src_credit += line.credit;
    }
    let books = net_through(ctx, &packet.as_of);
    let mut book_by_code: BTreeMap<String, (i64, i64, String)> = BTreeMap::new();
    for account in &ctx.accounts {
        let net = *books.get(account.id.as_str()).unwrap_or(&0);
        if net == 0 {
            continue;
        }
        let (d, c) = if net > 0 { (net, 0) } else { (0, -net) };
        book_by_code.insert(account.code.clone(), (d, c, account.id.clone()));
    }
    let mut diffs = Vec::new();
    if src_debit != src_credit {
        diffs.push((
            "balance".into(),
            format!("source debits {src_debit} credits {src_credit}"),
        ));
    }
    for (code, (d, c)) in &source {
        if !by_code.contains_key(code.as_str()) && (d != c) {
            diffs.push((
                format!("missing:{code}"),
                format!("source account {code} is not on the NXT chart"),
            ));
        }
        let (bd, bc) = book_by_code
            .get(code)
            .map(|(d, c, _)| (*d, *c))
            .unwrap_or((0, 0));
        let source_net = d - c;
        let book_net = bd - bc;
        if source_net != book_net {
            diffs.push((
                format!("amount:{code}"),
                format!("{code} source net {source_net} cents, books net {book_net} cents"),
            ));
        }
    }
    for (code, (d, c, _)) in &book_by_code {
        if !source.contains_key(code) {
            diffs.push((format!("absent:{code}"), format!("books show {code} at debit {d} credit {c}, and the source trial balance omits it")));
        }
    }
    if let Some(lock) = &packet.period_lock {
        let closed = ctx.closing_date.as_deref().filter(|d| !d.is_empty());
        if closed.is_none_or(|d| d < lock.as_str()) {
            diffs.push((
                "period_lock".into(),
                format!(
                    "source period lock is {lock} and the books closing date is {}",
                    closed.unwrap_or("unset")
                ),
            ));
        }
    }
    if packet.currency != ctx.currency {
        diffs.push((
            "currency".into(),
            format!("packet {} vs books {}", packet.currency, ctx.currency),
        ));
    }
    let id = "f_cutover";
    if diffs.is_empty() {
        verifications.push(Verification {
            id: "v_cutover".into(),
            name: "cutover".into(),
            state: "passed".into(),
            detail: format!("Source trial balance '{}' matches the books at {} for every non-zero account. Nixon did not post it.", packet.label, packet.as_of),
            evidence_ids: vec![],
        });
        return;
    }
    let mut evidence = vec![documented(
        "e_cutover_packet",
        "cutover:packet",
        "nxt.nixon.packet",
        &format!(
            "{} rows, class {}",
            packet.lines.len(),
            packet.source_class.as_str()
        ),
    )];
    for (i, (key, note)) in diffs.iter().take(EVIDENCE_CAP).enumerate() {
        let reference = if let Some(code) = key
            .strip_prefix("amount:")
            .or_else(|| key.strip_prefix("missing:"))
            .or_else(|| key.strip_prefix("absent:"))
        {
            by_code
                .get(code)
                .map(|a| format!("account:{}", a.id))
                .unwrap_or_else(|| format!("cutover:{code}"))
        } else if key == "period_lock" {
            "company:closing_date".into()
        } else {
            format!("cutover:{key}")
        };
        evidence.push(documented(
            &format!("e_cutover_{i}"),
            &reference,
            "nxt.nixon.packet",
            note,
        ));
    }
    let evidence_ids: Vec<String> = evidence.iter().map(|e| e.id.clone()).collect();
    verifications.push(Verification {
        id: "v_cutover".into(),
        name: "cutover".into(),
        state: "failed".into(),
        detail: format!("{} cutover difference(s). Nothing was posted.", diffs.len()),
        evidence_ids,
    });
    push_ack(
        findings,
        proposals,
        Finding {
            id: id.into(),
            capability: "cutover".into(),
            risk_level: "high".into(),
            title: "The source trial balance does not match the books".into(),
            summary: format!("{} difference(s) against '{}'. Nixon did not create accounts, lock the period, or post a migration journal.", diffs.len(), packet.label),
            requires_human_review: true,
            autonomy_level: "conditional".into(),
            evidence,
            hypotheses: vec![],
            gaps: vec![Gap { id: "g_cutover_live".into(), note: "A packet is not a live connection to the source system. Nixon did not contact that system.".into() }],
            proposal_id: None,
        },
        preparer,
    );
}

fn surface_checks(
    packet: &SurfacePacket,
    findings: &mut Vec<Finding>,
    proposals: &mut Vec<Proposal>,
    verifications: &mut Vec<Verification>,
) {
    let preparer = packet.prepared_by.trim();
    if packet.items.is_empty() {
        verifications.push(Verification {
            id: format!("v_{}_items", packet.surface),
            name: format!("{}_items", packet.surface),
            state: "not_run".into(),
            detail: format!("The {} packet listed no items. Nixon did not scan the installed {} module for this packet.", packet.surface, packet.surface),
            evidence_ids: vec![],
        });
    }
    let open: Vec<&SurfaceItem> = packet
        .items
        .iter()
        .filter(|i| i.state != "complete")
        .collect();
    if !open.is_empty() {
        let id = format!("f_{}_open", packet.surface);
        let evidence: Vec<Evidence> = open
            .iter()
            .take(EVIDENCE_CAP)
            .enumerate()
            .map(|(i, item)| {
                documented(
                    &format!("e_{id}_{i}"),
                    &format!("{}:{}", packet.surface, item.id),
                    "nxt.nixon.packet",
                    &format!("{} is {}", item.label, item.state),
                )
            })
            .collect();
        push_ack(
            findings,
            proposals,
            Finding {
                id: id,
                capability: packet.surface.clone(),
                risk_level: "medium".into(),
                title: format!("A {} item is not complete", packet.surface),
                summary: format!(
                    "{} {} item(s) are not complete. Acknowledging them here does not finish that module's work.",
                    open.len(),
                    packet.surface
                ),
                requires_human_review: true,
                autonomy_level: "conditional".into(),
                evidence,
                hypotheses: vec![],
                gaps: vec![Gap {
                    id: format!("g_{}_module", packet.surface),
                    note: format!(
                        "This packet is the {0} context for this review. Acknowledging it does not change the installed {0} module.",
                        packet.surface
                    ),
                }],
                proposal_id: None,
            },
            preparer,
        );
    }
    if packet.filing_requested {
        let id = format!("f_{}_filing", packet.surface);
        let proposal = blocked_proposal(
            &format!("p_{id}"),
            "file_return",
            "Filing was requested and is blocked. Nixon does not file taxes.",
            preparer,
            serde_json::json!({ "surface": packet.surface, "effect": "none" }),
        );
        let proposal_id = proposal.id.clone();
        proposals.push(proposal);
        findings.push(Finding {
            id: id,
            capability: "tax".into(),
            risk_level: "high".into(),
            title: "Tax filing was requested and refused".into(),
            summary: "The packet asked Nixon to file. Filing stays blocked. This is not a return and not a CRA submission.".into(),
            requires_human_review: true,
            autonomy_level: "advisory".into(),
            evidence: vec![documented("e_filing", &format!("{}:filing", packet.surface), "nxt.nixon.packet", "filing_requested is true")],
            hypotheses: vec![],
            gaps: vec![],
            proposal_id: Some(proposal_id),
        });
        verifications.push(Verification {
            id: format!("v_{}_filing", packet.surface),
            name: "filing".into(),
            state: "refused".into(),
            detail: "Filing is outside Nixon. The request was recorded and not sent.".into(),
            evidence_ids: vec!["e_filing".into()],
        });
    }
}

/// Every evidence reference must point at something in the context or the packet.
pub fn evidence_resolves(
    ctx: &FinanceContext,
    cutover: Option<&CutoverPacket>,
    surfaces: &[SurfacePacket],
    analysis: &Analysis,
) -> Result<(), String> {
    let accounts: HashMap<&str, ()> = ctx.accounts.iter().map(|a| (a.id.as_str(), ())).collect();
    let entries: HashMap<&str, ()> = ctx.entries.iter().map(|e| (e.id.as_str(), ())).collect();
    let feed: HashMap<&str, ()> = ctx.feed.iter().map(|f| (f.id.as_str(), ())).collect();
    let docs: HashMap<&str, ()> = ctx.docs.iter().map(|d| (d.id.as_str(), ())).collect();
    let payments: HashMap<&str, ()> = ctx.payments.iter().map(|p| (p.id.as_str(), ())).collect();
    let recs: HashMap<&str, ()> = ctx
        .reconciliations
        .iter()
        .map(|r| (r.id.as_str(), ()))
        .collect();
    let codes: HashMap<&str, ()> = ctx.accounts.iter().map(|a| (a.code.as_str(), ())).collect();
    let cutover_codes: HashMap<&str, ()> = cutover
        .iter()
        .flat_map(|p| p.lines.iter().map(|l| (l.account_code.as_str(), ())))
        .collect();
    let surface_ids: HashMap<String, ()> = surfaces
        .iter()
        .flat_map(|s| {
            s.items
                .iter()
                .map(|i| (format!("{}:{}", s.surface, i.id), ()))
        })
        .collect();
    for finding in &analysis.findings {
        let blob = format!("{} {}", finding.title, finding.summary);
        if blob.to_lowercase().contains("root cause") {
            return Err(format!(
                "{} labels a hypothesis as a root cause",
                finding.id
            ));
        }
        for h in &finding.hypotheses {
            if h.label != "hypothesis" || h.confidence.is_some() || h.confidence_refusal.is_empty()
            {
                return Err(format!(
                    "{} hypothesis is not labelled as unresolved",
                    finding.id
                ));
            }
            if h.statement.to_lowercase().contains("root cause") {
                return Err("hypothesis text claims a root cause".into());
            }
        }
        for ev in &finding.evidence {
            resolve(
                &ev.reference,
                ctx,
                &accounts,
                &entries,
                &feed,
                &docs,
                &payments,
                &recs,
                &codes,
                &cutover_codes,
                &surface_ids,
                cutover.is_some(),
            )?;
        }
    }
    for proposal in &analysis.proposals {
        if proposal.execution != "disabled" || proposal.applied {
            return Err(format!("{} would execute", proposal.id));
        }
    }
    Ok(())
}

fn resolve(
    reference: &str,
    ctx: &FinanceContext,
    accounts: &HashMap<&str, ()>,
    entries: &HashMap<&str, ()>,
    feed: &HashMap<&str, ()>,
    docs: &HashMap<&str, ()>,
    payments: &HashMap<&str, ()>,
    recs: &HashMap<&str, ()>,
    codes: &HashMap<&str, ()>,
    cutover_codes: &HashMap<&str, ()>,
    surface_ids: &HashMap<String, ()>,
    has_cutover: bool,
) -> Result<(), String> {
    let (kind, rest) = reference
        .split_once(':')
        .ok_or_else(|| format!("bad evidence reference {reference}"))?;
    let ok = match kind {
        "entry" => {
            let id = rest.split(':').next().unwrap_or("");
            entries.contains_key(id)
        }
        "feed" => feed.contains_key(rest),
        "account" => accounts.contains_key(rest),
        "doc" => docs.contains_key(rest),
        "payment" => payments.contains_key(rest),
        "recon" => recs.contains_key(rest),
        "report" => rest.starts_with("trial-balance:"),
        "company" => rest == "closing_date",
        "cutover" => {
            has_cutover
                && (rest == "packet"
                    || rest == "balance"
                    || rest == "currency"
                    || codes.contains_key(rest)
                    || cutover_codes.contains_key(rest))
        }
        "firm" | "tax" => surface_ids.contains_key(reference) || rest == "filing",
        _ => false,
    };
    if ok {
        let _ = ctx;
        Ok(())
    } else {
        Err(format!("evidence reference {reference} does not resolve"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, code: &str, kind: &str, role: &str, debit_normal: bool) -> CtxAccount {
        CtxAccount {
            id: id.into(),
            code: code.into(),
            name: code.into(),
            kind: kind.into(),
            role: role.into(),
            debit_normal,
        }
    }

    fn line(account: &str, debit: i64, credit: i64) -> CtxLine {
        CtxLine {
            account_id: account.into(),
            debit,
            credit,
            cleared: false,
        }
    }

    fn entry(id: &str, date: &str, lines: Vec<CtxLine>) -> CtxEntry {
        CtxEntry {
            id: id.into(),
            date: date.into(),
            memo: "memo".into(),
            source: "journal".into(),
            source_id: None,
            voided: false,
            lines,
        }
    }

    fn ctx(entries: Vec<CtxEntry>) -> FinanceContext {
        FinanceContext {
            as_of: "2026-10-28".into(),
            currency: "CAD".into(),
            company_name: "NXT Corporation".into(),
            closing_date: None,
            source_class: SourceClass::WorkspaceBooks,
            accounts: vec![
                account("acc_1000", "1000", "asset", "bank", true),
                account("acc_3000", "3000", "equity", "none", false),
                account("acc_6500", "6500", "expense", "none", true),
            ],
            entries,
            feed: vec![],
            reconciliations: vec![],
            docs: vec![],
            payments: vec![],
        }
    }

    #[test]
    fn balanced_books_do_not_invent_a_cash_figure_without_activity() {
        let analysis = analyze(&ctx(vec![]), None, &[]).unwrap();
        let cash = analysis
            .findings
            .iter()
            .find(|f| f.id == "f_cash_movement")
            .unwrap();
        assert!(cash.evidence.is_empty());
        assert!(cash.gaps.iter().any(|g| g.note.contains("No bank lines")));
        assert!(
            analysis
                .verifications
                .iter()
                .any(|v| v.name == "trial_balance" && v.state == "passed")
        );
        assert!(
            analysis
                .verifications
                .iter()
                .any(|v| v.name == "cutover" && v.state == "not_run")
        );
        evidence_resolves(&ctx(vec![]), None, &[], &analysis).unwrap();
    }

    #[test]
    fn pending_feed_cites_the_row_and_does_not_post() {
        let mut books = ctx(vec![]);
        books.feed.push(CtxFeed {
            id: "feed_1".into(),
            account_id: "acc_1000".into(),
            date: "2026-10-01".into(),
            description: "ADOBE".into(),
            amount: -5649,
            status: "pending".into(),
        });
        let analysis = analyze(&books, None, &[]).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|f| f.id == "f_pending_feed")
            .unwrap();
        assert_eq!(finding.evidence[0].reference, "feed:feed_1");
        assert!(
            analysis
                .proposals
                .iter()
                .all(|p| p.execution == "disabled" && !p.applied && p.kind != "post_journal")
        );
        evidence_resolves(&books, None, &[], &analysis).unwrap();
    }

    #[test]
    fn uncleared_line_is_a_reconciliation_gap() {
        let mut books = ctx(vec![
            entry(
                "e1",
                "2026-10-01",
                vec![line("acc_1000", 1000, 0), line("acc_3000", 0, 1000)],
            ),
            entry(
                "e2",
                "2026-10-02",
                vec![line("acc_1000", 500, 0), line("acc_3000", 0, 500)],
            ),
        ]);
        books.entries[0].lines[0].cleared = true;
        books.reconciliations.push(CtxRecon {
            id: "rec_1".into(),
            account_id: "acc_1000".into(),
            statement_date: "2026-10-31".into(),
            statement_balance: 1500,
        });
        let analysis = analyze(&books, None, &[]).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|f| f.capability == "reconciliation" && f.id.starts_with("f_recon"))
            .unwrap();
        assert!(finding.evidence.iter().any(|e| e.reference == "entry:e2:0"));
        assert!(finding.gaps.iter().any(|g| g.note.contains("plug")));
        evidence_resolves(&books, None, &[], &analysis).unwrap();
    }

    #[test]
    fn shift_is_a_hypothesis_without_a_cause() {
        let books = ctx(vec![
            entry(
                "e1",
                "2026-09-15",
                vec![line("acc_6500", 40_000, 0), line("acc_1000", 0, 40_000)],
            ),
            entry(
                "e2",
                "2026-10-15",
                vec![line("acc_6500", 120_000, 0), line("acc_1000", 0, 120_000)],
            ),
        ]);
        let analysis = analyze(&books, None, &[]).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|f| f.id == "f_shift_screen")
            .unwrap();
        assert_eq!(finding.hypotheses[0].label, "hypothesis");
        assert!(finding.hypotheses[0].confidence.is_none());
        assert!(!finding.summary.to_lowercase().contains("root cause"));
        evidence_resolves(&books, None, &[], &analysis).unwrap();
    }

    #[test]
    fn cutover_difference_does_not_invent_an_account() {
        let books = ctx(vec![entry(
            "e1",
            "2026-10-01",
            vec![line("acc_1000", 100, 0), line("acc_3000", 0, 100)],
        )]);
        let packet = CutoverPacket {
            source_class: SourceClass::CustomerOperational,
            as_of: "2026-10-28".into(),
            currency: "CAD".into(),
            period_lock: Some("2026-09-30".into()),
            prepared_by: "Casey Quinn".into(),
            label: "Source TB".into(),
            lines: vec![
                CutoverLine {
                    account_code: "1000".into(),
                    debit: 100,
                    credit: 0,
                },
                CutoverLine {
                    account_code: "9999".into(),
                    debit: 0,
                    credit: 40,
                },
                CutoverLine {
                    account_code: "3000".into(),
                    debit: 0,
                    credit: 50,
                },
            ],
        };
        let analysis = analyze(&books, Some(&packet), &[]).unwrap();
        let finding = analysis
            .findings
            .iter()
            .find(|f| f.id == "f_cutover")
            .unwrap();
        assert!(
            finding
                .evidence
                .iter()
                .any(|e| e.reference == "cutover:9999")
        );
        assert!(
            finding
                .evidence
                .iter()
                .all(|e| e.reference != "account:acc_9999")
        );
        assert_eq!(
            analysis
                .proposals
                .iter()
                .find(|p| p.finding_id.as_deref() == Some("f_cutover"))
                .unwrap()
                .prepared_by,
            "Casey Quinn"
        );
        evidence_resolves(&books, Some(&packet), &[], &analysis).unwrap();
    }

    #[test]
    fn filing_request_is_blocked() {
        let books = ctx(vec![]);
        let tax = SurfacePacket {
            surface: "tax".into(),
            source_class: SourceClass::Simulated,
            as_of: "2026-10-28".into(),
            prepared_by: "Casey Quinn".into(),
            filing_requested: true,
            items: vec![SurfaceItem {
                id: "t2".into(),
                label: "T2 workpaper".into(),
                state: "unreviewed".into(),
                note: String::new(),
            }],
        };
        let analysis = analyze(&books, None, &[tax.clone()]).unwrap();
        let proposal = analysis
            .proposals
            .iter()
            .find(|p| p.kind == "file_return")
            .unwrap();
        assert_eq!(proposal.status, "blocked");
        assert!(proposal.files_tax && !proposal.applied && proposal.execution == "disabled");
        evidence_resolves(&books, None, &[tax], &analysis).unwrap();
    }

    #[test]
    fn packet_cannot_pretend_to_be_the_live_books() {
        let books = ctx(vec![]);
        let packet = CutoverPacket {
            source_class: SourceClass::WorkspaceBooks,
            as_of: "2026-10-28".into(),
            currency: "CAD".into(),
            period_lock: None,
            prepared_by: "Casey Quinn".into(),
            label: "x".into(),
            lines: vec![],
        };
        assert!(analyze(&books, Some(&packet), &[]).is_err());
    }
}
