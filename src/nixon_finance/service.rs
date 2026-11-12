//! Read the ledger, run the screens, and record decisions.
//!
//! Writes stay on the Nixon case file and the existing ledger audit log.
//! Nothing in this file calls a posting or filing method.

use super::analyze::{self, evidence_resolves};
use super::models::*;
use crate::ledger::engine::{Ledger, LedgerError};
use crate::ledger::models::{AccountKind, AccountRole};
use hex::encode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub struct NixonError {
    pub status: u16,
    pub message: String,
}

impl NixonError {
    pub(crate) fn new(status: u16, message: impl Into<String>) -> Self {
        NixonError {
            status,
            message: message.into(),
        }
    }
}

impl From<LedgerError> for NixonError {
    fn from(e: LedgerError) -> Self {
        let (status, message) = match e {
            LedgerError::BadRequest(m) => (400, m),
            LedgerError::NotFound(m) => (404, m),
            LedgerError::Conflict(m) => (409, m),
            LedgerError::Storage(m) => (500, m),
        };
        NixonError { status, message }
    }
}

impl From<String> for NixonError {
    fn from(message: String) -> Self {
        NixonError {
            status: 400,
            message,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub cutover: Option<CutoverPacket>,
    #[serde(default)]
    pub surfaces: Vec<SurfacePacket>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionRequest {
    pub actor_name: String,
    pub decision: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct ActionRequest {
    pub kind: String,
    pub actor_name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub autonomy_level: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Serialize, Deserialize)]
struct Idempotency {
    review_id: String,
    digest: String,
}

pub fn require_person(name: &str) -> Result<String, NixonError> {
    let full = name.trim();
    // Signed-in staff are stamped as "Name [usr_id]"; validate the name and keep the id for audit.
    let (trimmed, tag) = match full.rsplit_once(" [") {
        Some((n, id)) if id.ends_with(']') && id[..id.len() - 1].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => (n.trim(), true),
        _ => (full, false),
    };
    let count = trimmed.chars().count();
    if !(2..=80).contains(&count) {
        return Err(NixonError::new(400, "a named person is required"));
    }
    let lower = trimmed.to_lowercase();
    const DENY: &[&str] = &[
        "nixon",
        "system",
        "ai",
        "assistant",
        "nxt",
        "anonymous",
        "admin",
        "user",
        "operator",
    ];
    if DENY.contains(&lower.as_str()) {
        return Err(NixonError::new(
            400,
            "the decision has to name a person, not Nixon or a role",
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_alphabetic() || matches!(c, ' ' | '-' | '\'' | '.'))
    {
        return Err(NixonError::new(400, "use the person's name"));
    }
    Ok(if tag { full.to_string() } else { trimmed.to_string() })
}

#[cfg(test)]
mod person_tests {
    use super::require_person;
    #[test]
    fn staff_stamped_label_is_a_person() {
        assert_eq!(require_person("Orville Test [usr_70c3cb7ddf0e4fe6]").ok().unwrap(), "Orville Test [usr_70c3cb7ddf0e4fe6]");
        assert!(require_person("Nixon [usr_1]").is_err());
        assert!(require_person("x9 [usr_1]").is_err());
        assert!(require_person("Bob [usr_1; drop]").is_err());
        assert_eq!(require_person(" Orville Test ").ok().unwrap(), "Orville Test");
    }

    #[tokio::test]
    async fn a_stale_write_never_overwrites_a_decision() {
        use super::{DecisionRequest, decide, get_review, get_review_versioned, replace_review, save_review};
        use crate::ledger::engine::Ledger;
        use crate::nixon_finance::models::Review;
        let ledger = Ledger::new(crate::store::Store::memory());
        let review: Review = serde_json::from_value(serde_json::json!({
            "id": "ge_cas", "correlation_id": "cor_cas", "created_at": "2026-11-12T00:00:00Z", "as_of": "2026-11-12",
            "currency": "CAD", "source_class": "workspace_books", "digest": "d", "idempotency_key": null,
            "autonomy_level": "advisory", "execution_mode": "advisory", "posting": "disabled", "filing": "disabled",
            "summary": "Synthetic", "findings": [], "verifications": [], "decisions": [], "cutover": null, "surfaces": [],
            "scope": "gods_eye",
            "proposals": [{"id": "p1", "finding_id": null, "kind": "staff_follow_up", "changes_books": false, "files_tax": false,
                "execution": "disabled", "applied": false, "status": "pending_approval", "summary": "s",
                "prepared_by": "God's Eye watcher", "payload": {}}],
        })).unwrap();
        save_review(&ledger, &review).await.ok().unwrap();
        // A slow writer (an AI draft) reads first; a person decides meanwhile.
        let (mut stale, raw) = get_review_versioned(&ledger, "ge_cas").await.ok().unwrap();
        decide(&ledger, "ge_cas", "p1", DecisionRequest { actor_name: "Casey Quinn".into(), decision: "dismiss".into(), note: String::new() }).await.ok().unwrap();
        stale.proposals[0].payload["ai_draft"] = serde_json::json!({"text": "late"});
        let err = replace_review(&ledger, &stale, &raw).await.unwrap_err();
        assert_eq!(err.status, 409);
        let now = get_review(&ledger, "ge_cas").await.ok().unwrap();
        assert_eq!(now.proposals[0].status, "dismissed");
        assert!(now.proposals[0].payload.get("ai_draft").is_none());
        // The preparer cannot approve under another label format.
        let mut r2 = now.clone();
        r2.id = "ge_cas2".into();
        r2.proposals[0].status = "pending_approval".into();
        save_review(&ledger, &r2).await.ok().unwrap();
        for who in ["God's Eye watcher [usr_1]", "Gods Eye  Watcher"] {
            let e = decide(&ledger, "ge_cas2", "p1", DecisionRequest { actor_name: who.into(), decision: "approve".into(), note: String::new() }).await.unwrap_err();
            assert_eq!(e.status, 403, "{who}");
        }
    }

    #[test]
    fn self_approval_cannot_hide_behind_label_format() {
        use super::same_person;
        assert!(same_person("God's Eye watcher [usr_1]", "God's Eye watcher"));
        assert!(same_person("Gods  Eye   Watcher", "God's Eye watcher"));
        assert!(same_person("Jane Doe", "Jane Doe [usr_abc]"));
        assert!(same_person("J. Doe [usr_abc]", "Jane Doe [usr_abc]"), "same stamped id");
        assert!(same_person("jane-doe", "Jane Doe"));
        assert!(!same_person("Casey Quinn", "God's Eye watcher"));
        assert!(!same_person("Casey Quinn [usr_2]", "Jane Doe [usr_1]"));
    }
}

pub(crate) fn nid(prefix: &str) -> String {
    format!(
        "{prefix}_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

fn kind_name(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Asset => "asset",
        AccountKind::Liability => "liability",
        AccountKind::Equity => "equity",
        AccountKind::Income => "income",
        AccountKind::Expense => "expense",
    }
}

fn role_name(role: AccountRole) -> &'static str {
    match role {
        AccountRole::None => "none",
        AccountRole::Bank => "bank",
        AccountRole::CreditCard => "credit_card",
        AccountRole::Receivable => "receivable",
        AccountRole::Payable => "payable",
        AccountRole::TaxPayable => "tax_payable",
        AccountRole::TaxRecoverable => "tax_recoverable",
        AccountRole::RetainedEarnings => "retained_earnings",
    }
}

pub async fn load_context(ledger: &Ledger) -> Result<FinanceContext, NixonError> {
    let company = ledger.company().await?;
    let accounts = ledger.accounts_read().await?;
    let mut entries = ledger.entries().await?;
    let docs = ledger.docs(None).await?;
    let payments = ledger.payments().await?;
    let reconciliations = ledger.reconciliations().await?;
    let feed_rows = ledger.feed().await?;
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let mut feed = Vec::new();
    for row in feed_rows {
        let item = &row["item"];
        feed.push(CtxFeed {
            id: item["id"].as_str().unwrap_or_default().to_string(),
            account_id: item["account_id"].as_str().unwrap_or_default().to_string(),
            date: item["date"].as_str().unwrap_or_default().to_string(),
            description: item["description"].as_str().unwrap_or_default().to_string(),
            amount: item["amount"].as_i64().unwrap_or(0),
            status: item["status"].as_str().unwrap_or_default().to_string(),
        });
    }
    feed.sort_by(|a, b| a.id.cmp(&b.id));
    let closing = company
        .closing_date
        .as_ref()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    Ok(FinanceContext {
        as_of: crate::ledger::engine::today(),
        currency: company.currency,
        company_name: company.name,
        closing_date: closing,
        source_class: SourceClass::WorkspaceBooks,
        accounts: accounts
            .into_iter()
            .map(|a| CtxAccount {
                id: a.id,
                code: a.code,
                name: a.name,
                debit_normal: a.kind.debit_normal(),
                kind: kind_name(a.kind).into(),
                role: role_name(a.role).into(),
            })
            .collect(),
        entries: entries
            .into_iter()
            .map(|e| CtxEntry {
                id: e.id,
                date: e.date,
                memo: e.memo,
                source: e.source,
                source_id: e.source_id,
                voided: e.voided,
                lines: e
                    .lines
                    .into_iter()
                    .map(|l| CtxLine {
                        account_id: l.account_id,
                        debit: l.debit,
                        credit: l.credit,
                        cleared: l.cleared,
                    })
                    .collect(),
            })
            .collect(),
        feed,
        reconciliations: reconciliations
            .into_iter()
            .map(|r| CtxRecon {
                id: r.id,
                account_id: r.account_id,
                statement_date: r.statement_date,
                statement_balance: r.statement_balance,
            })
            .collect(),
        docs: docs
            .into_iter()
            .map(|d| CtxDoc {
                id: d.id,
                number: d.number,
                kind: match d.kind {
                    crate::ledger::models::DocKind::Invoice => "invoice",
                    crate::ledger::models::DocKind::Bill => "bill",
                }
                .into(),
                entry_id: d.entry_id,
                voided: d.voided,
            })
            .collect(),
        payments: payments
            .into_iter()
            .map(|p| CtxPayment {
                id: p.id,
                entry_id: p.entry_id,
                voided: p.voided,
            })
            .collect(),
    })
}

pub fn context_digest(
    ctx: &FinanceContext,
    cutover: &Option<CutoverPacket>,
    surfaces: &[SurfacePacket],
) -> String {
    let raw = serde_json::to_vec(
        &json!({ "v": 1, "ctx": ctx, "cutover": cutover, "surfaces": surfaces }),
    )
    .unwrap_or_default();
    encode(Sha256::digest(raw))
}

fn summary_of(analysis: &Analysis) -> String {
    let review = analysis
        .findings
        .iter()
        .filter(|f| f.requires_human_review)
        .count();
    let blocked = analysis
        .proposals
        .iter()
        .filter(|p| p.status == "blocked")
        .count();
    format!(
        "{} finding(s), {} waiting for a named person, {} blocked. Nixon did not post or file.",
        analysis.findings.len(),
        review,
        blocked
    )
}

pub async fn run_review(ledger: &Ledger, request: ReviewRequest) -> Result<Review, NixonError> {
    if let Some(key) = &request.idempotency_key {
        let count = key.chars().count();
        if !(8..=80).contains(&count)
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(NixonError::new(
                400,
                "idempotency_key must be 8–80 letters, numbers, dashes, or underscores",
            ));
        }
    }
    let mut surfaces = request.surfaces;
    surfaces.sort_by(|a, b| {
        a.surface
            .cmp(&b.surface)
            .then(a.prepared_by.cmp(&b.prepared_by))
    });
    let ctx = load_context(ledger).await?;
    let digest = context_digest(&ctx, &request.cutover, &surfaces);
    if let Some(key) = &request.idempotency_key {
        if let Some(prior) = ledger
            .store
            .get_json::<Idempotency>(&format!("nixon:idem:{key}"))
            .await
            .map_err(|m| NixonError::new(500, m))?
        {
            if prior.digest != digest {
                return Err(NixonError::new(
                    409,
                    "this idempotency key was already used for a different snapshot",
                ));
            }
            return get_review(ledger, &prior.review_id).await;
        }
    }
    let analysis = analyze::analyze(&ctx, request.cutover.as_ref(), &surfaces)
        .map_err(|m| NixonError::new(400, m))?;
    evidence_resolves(&ctx, request.cutover.as_ref(), &surfaces, &analysis)
        .map_err(|m| NixonError::new(500, m))?;
    let review = Review {
        id: nid("rev"),
        correlation_id: nid("cor"),
        created_at: crate::ledger::engine::now(),
        as_of: ctx.as_of.clone(),
        currency: ctx.currency.clone(),
        source_class: ctx.source_class,
        digest,
        idempotency_key: request.idempotency_key.clone(),
        autonomy_level: "advisory".into(),
        execution_mode: "advisory".into(),
        posting: "disabled".into(),
        filing: "disabled".into(),
        summary: summary_of(&analysis),
        findings: analysis.findings,
        proposals: analysis.proposals,
        verifications: analysis.verifications,
        decisions: vec![],
        cutover: request.cutover,
        surfaces,
        scope: "workspace_ledger".into(),
    };
    save_review(ledger, &review).await?;
    if let Some(key) = &review.idempotency_key {
        let record = Idempotency {
            review_id: review.id.clone(),
            digest: review.digest.clone(),
        };
        let won = ledger
            .store
            .set_nx(
                &format!("nixon:idem:{key}"),
                &serde_json::to_string(&record).unwrap(),
            )
            .await
            .map_err(|m| NixonError::new(500, m))?;
        if !won {
            if let Some(prior) = ledger
                .store
                .get_json::<Idempotency>(&format!("nixon:idem:{key}"))
                .await
                .map_err(|m| NixonError::new(500, m))?
            {
                if prior.digest == review.digest {
                    return get_review(ledger, &prior.review_id).await;
                }
            }
            return Err(NixonError::new(
                409,
                "this idempotency key was already used for a different snapshot",
            ));
        }
    }
    ledger
        .audit(
            "nixon.review",
            format!(
                "correlation={} review={} findings={} posting=disabled filing=disabled",
                review.correlation_id,
                review.id,
                review.findings.len()
            ),
        )
        .await?;
    Ok(review)
}

pub(crate) async fn save_review(ledger: &Ledger, review: &Review) -> Result<(), NixonError> {
    ledger
        .store
        .set_json(&format!("nixon:review:{}", review.id), review)
        .await
        .map_err(|m| NixonError::new(500, m))?;
    ledger
        .store
        .sadd("nixon:reviews", &review.id)
        .await
        .map_err(|m| NixonError::new(500, m))?;
    Ok(())
}

/// A review plus the exact stored text it was read from, for compare-and-swap updates.
pub(crate) async fn get_review_versioned(ledger: &Ledger, id: &str) -> Result<(Review, String), NixonError> {
    let raw = ledger
        .store
        .get(&format!("nixon:review:{id}"))
        .await
        .map_err(|m| NixonError::new(500, m))?
        .ok_or_else(|| NixonError::new(404, format!("review '{id}' not found")))?;
    let review = serde_json::from_str(&raw).map_err(|_| NixonError::new(500, "a stored review is unreadable"))?;
    Ok((review, raw))
}

/// Replaces an existing review only while it still matches `expected` (the text read by
/// [`get_review_versioned`]). A concurrent decision or draft is never overwritten: the
/// later writer gets 409 and must reload.
pub(crate) async fn replace_review(ledger: &Ledger, review: &Review, expected: &str) -> Result<(), NixonError> {
    let raw = serde_json::to_string(review).map_err(|e| NixonError::new(500, e.to_string()))?;
    let swapped = ledger
        .store
        .compare_exchange(&format!("nixon:review:{}", review.id), Some(expected), &raw)
        .await
        .map_err(|m| NixonError::new(500, m))?;
    if !swapped {
        return Err(NixonError::new(409, "this review changed while the request was in progress; reload and try again"));
    }
    Ok(())
}

/// The person behind a decision label: the name of "Name [usr_id]" reduced to lowercase
/// letters and digits (so spacing, case and punctuation cannot disguise it), and the id.
fn person_key(label: &str) -> (String, Option<String>) {
    let full = label.trim();
    let (name, id) = match full.rsplit_once('[') {
        Some((n, rest)) if rest.ends_with(']') => (n, Some(rest[..rest.len() - 1].trim().to_ascii_lowercase())),
        _ => (full, None),
    };
    let name: String = name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    (name, id.filter(|i| !i.is_empty()))
}

/// True when two labels name the same person (same normalized name, or the same stamped id).
pub(crate) fn same_person(a: &str, b: &str) -> bool {
    let (na, ia) = person_key(a);
    let (nb, ib) = person_key(b);
    (!na.is_empty() && na == nb) || matches!((ia, ib), (Some(x), Some(y)) if x == y)
}

pub async fn get_review(ledger: &Ledger, id: &str) -> Result<Review, NixonError> {
    ledger
        .store
        .get_json(&format!("nixon:review:{id}"))
        .await
        .map_err(|m| NixonError::new(500, m))?
        .ok_or_else(|| NixonError::new(404, format!("review '{id}' not found")))
}

pub async fn list_reviews(ledger: &Ledger) -> Result<Vec<Value>, NixonError> {
    let ids = ledger
        .store
        .smembers("nixon:reviews")
        .await
        .map_err(|m| NixonError::new(500, m))?;
    let mut reviews = Vec::new();
    for id in ids {
        if let Some(review) = ledger
            .store
            .get_json::<Review>(&format!("nixon:review:{id}"))
            .await
            .map_err(|m| NixonError::new(500, m))?
        {
            let pending = review
                .proposals
                .iter()
                .filter(|p| p.status == "pending_approval")
                .count();
            reviews.push(json!({
                "id": review.id,
                "correlation_id": review.correlation_id,
                "created_at": review.created_at,
                "as_of": review.as_of,
                "summary": review.summary,
                "finding_count": review.findings.len(),
                "pending_approval": pending,
                "posting": review.posting,
                "filing": review.filing,
                "scope": if review.scope.is_empty() { "workspace_ledger" } else { review.scope.as_str() },
            }));
        }
    }
    reviews.sort_by(|a, b| {
        b["created_at"]
            .as_str()
            .unwrap_or("")
            .cmp(a["created_at"].as_str().unwrap_or(""))
    });
    Ok(reviews)
}

pub async fn attention(ledger: &Ledger) -> Result<Vec<Value>, NixonError> {
    let mut reviews = list_reviews(ledger).await?;
    let Some(latest) = reviews.first_mut() else {
        return Ok(vec![]);
    };
    let id = latest["id"].as_str().unwrap_or("").to_string();
    if id.is_empty() {
        return Ok(vec![]);
    }
    let review = get_review(ledger, &id).await?;
    Ok(review
        .findings
        .iter()
        .filter(|f| f.requires_human_review)
        .filter(|f| {
            review
                .proposals
                .iter()
                .find(|p| p.finding_id.as_deref() == Some(f.id.as_str()))
                .is_none_or(|p| p.status == "pending_approval" || p.status == "blocked")
        })
        .take(3)
        .map(|f| json!({ "title": f.title, "body": f.summary }))
        .collect())
}

pub async fn decide(
    ledger: &Ledger,
    review_id: &str,
    proposal_id: &str,
    request: DecisionRequest,
) -> Result<Review, NixonError> {
    let actor = require_person(&request.actor_name)?;
    let decision = match request.decision.as_str() {
        "approve" => "approve",
        "reject" => "reject",
        // God's Eye findings are dismissed rather than rejected; same gate, same record.
        "dismiss" => "dismiss",
        _ => return Err(NixonError::new(400, "decision must be approve, reject or dismiss")),
    };
    if request.note.chars().count() > 2_000 {
        return Err(NixonError::new(400, "note is too long"));
    }
    let (mut review, stored) = get_review_versioned(ledger, review_id).await?;
    let proposal = review
        .proposals
        .iter_mut()
        .find(|p| p.id == proposal_id)
        .ok_or_else(|| NixonError::new(404, "proposal not found"))?;
    if proposal.status == "blocked" {
        return Err(NixonError::new(
            409,
            "this action stays blocked. Approval cannot post or file.",
        ));
    }
    if proposal.status != "pending_approval" {
        return Err(NixonError::new(409, "this proposal already has a decision"));
    }
    if same_person(&actor, &proposal.prepared_by) {
        return Err(NixonError::new(
            403,
            "the person who prepared this cannot approve it",
        ));
    }
    proposal.status = match decision {
        "approve" => "approved_no_execution",
        "dismiss" => "dismissed",
        _ => "rejected",
    }
    .into();
    proposal.applied = false;
    proposal.execution = "disabled".into();
    let correlation = review.correlation_id.clone();
    review.decisions.push(Decision {
        proposal_id: proposal_id.to_string(),
        actor_name: actor.clone(),
        decision: decision.into(),
        note: request.note,
        at: crate::ledger::engine::now(),
        execution: "disabled".into(),
        verification_state: "not_verification".into(),
    });
    replace_review(ledger, &review, &stored).await?;
    ledger
        .audit(
            "nixon.decision",
            format!("correlation={correlation} review={review_id} proposal={proposal_id} actor={actor} decision={decision} execution=disabled verification=not_verification"),
        )
        .await?;
    Ok(review)
}

pub async fn refuse_execute(
    ledger: &Ledger,
    review_id: &str,
    proposal_id: &str,
    actor_name: &str,
) -> Result<(), NixonError> {
    let review = get_review(ledger, review_id).await?;
    if !review.proposals.iter().any(|p| p.id == proposal_id) {
        return Err(NixonError::new(404, "proposal not found"));
    }
    ledger
        .audit(
            "nixon.execution_refused",
            format!(
                "correlation={} review={review_id} proposal={proposal_id} actor={} execution=disabled",
                review.correlation_id,
                actor_name.trim()
            ),
        )
        .await?;
    Err(NixonError::new(
        403,
        "Nixon cannot post entries, file taxes, or change the books. Approval is not execution.",
    ))
}

pub async fn block_action(
    ledger: &Ledger,
    request: ActionRequest,
) -> Result<GateRecord, NixonError> {
    let actor = require_person(&request.actor_name)?;
    const KINDS: &[&str] = &[
        "post_journal",
        "void_entry",
        "file_return",
        "remit_tax",
        "categorize_feed",
        "lock_period",
    ];
    if !KINDS.contains(&request.kind.as_str()) {
        return Err(NixonError::new(400, "Nixon does not accept that action"));
    }
    let requested = request.autonomy_level.unwrap_or_else(|| "advisory".into());
    if !matches!(
        requested.as_str(),
        "advisory" | "conditional" | "controlled"
    ) {
        return Err(NixonError::new(
            400,
            "autonomy_level must be advisory, conditional, or controlled",
        ));
    }
    let record = GateRecord {
        id: nid("gate"),
        correlation_id: nid("cor"),
        created_at: crate::ledger::engine::now(),
        kind: request.kind.clone(),
        actor_name: actor,
        note: request.note.chars().take(2_000).collect(),
        autonomy_requested: requested.clone(),
        autonomy_applied: "advisory".into(),
        execution: "disabled".into(),
        applied: false,
        status: "blocked".into(),
        changes_books: matches!(
            request.kind.as_str(),
            "post_journal" | "void_entry" | "categorize_feed" | "lock_period" | "remit_tax"
        ),
        files_tax: request.kind == "file_return",
        payload: request.payload,
    };
    ledger
        .store
        .set_json(&format!("nixon:gate:{}", record.id), &record)
        .await
        .map_err(|m| NixonError::new(500, m))?;
    ledger
        .store
        .sadd("nixon:gates", &record.id)
        .await
        .map_err(|m| NixonError::new(500, m))?;
    let downgrade = if requested != "advisory" {
        " autonomy_downgraded=true"
    } else {
        ""
    };
    ledger
        .audit(
            "nixon.action_blocked",
            format!(
                "correlation={} gate={} kind={} actor={} execution=disabled applied=false{downgrade}",
                record.correlation_id, record.id, record.kind, record.actor_name
            ),
        )
        .await?;
    Ok(record)
}

pub fn status_body() -> Value {
    json!({
        "name": "Nixon",
        "layer": "finance",
        "posting": "disabled",
        "filing": "disabled",
        "regulatory_modules": "not_in_scope",
        "autonomy": "advisory",
        "execution_mode": "advisory",
        "autonomy_upgrade": "refused",
        "screens": {
            "shift": SHIFT_SCREEN,
            "shift_min_cents": SHIFT_MIN_CENTS,
            "shift_min_ratio_bp": SHIFT_MIN_RATIO_BP,
        },
        "surfaces": {
            "ledger": "installed",
            "firm": "installed",
            "tax": "installed",
            "migration": "installed",
        },
        "depends_on": "Firm, Tax, Engagements, and migration are on main. Nixon reads the ledger and accepts firm, tax, and cutover packets. Approving a Nixon finding does not complete a Firm task, publish a migration receipt, or file a return.",
        "guardvoice": "Draft PR #4 owns src/nixon as the phone engine and is not modified here. Finance Nixon lives in src/nixon_finance.",
    })
}
