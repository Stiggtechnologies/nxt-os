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
    let trimmed = name.trim();
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
    Ok(trimmed.to_string())
}

fn nid(prefix: &str) -> String {
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
    let accounts = ledger.accounts().await?;
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

async fn save_review(ledger: &Ledger, review: &Review) -> Result<(), NixonError> {
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
        _ => return Err(NixonError::new(400, "decision must be approve or reject")),
    };
    if request.note.chars().count() > 2_000 {
        return Err(NixonError::new(400, "note is too long"));
    }
    let mut review = get_review(ledger, review_id).await?;
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
    if actor.eq_ignore_ascii_case(proposal.prepared_by.trim()) {
        return Err(NixonError::new(
            403,
            "the person who prepared this cannot approve it",
        ));
    }
    proposal.status = if decision == "approve" {
        "approved_no_execution"
    } else {
        "rejected"
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
    save_review(ledger, &review).await?;
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
