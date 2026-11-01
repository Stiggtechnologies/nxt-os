//! Nixon voice commands.
//!
//! The shape follows Sync Context voice (SCX-VOICE): hear a turn, classify
//! it, speak a short answer, and leave approval on the screen. Live speech
//! is OpenAI Realtime when configured, otherwise browser recognition and
//! speech synthesis. The legacy Meet Sync booth is not this surface. This
//! module does not post, file, or approve.
//!
//! The scripted gate matches the idea in draft PR #4's `src/nixon` phone
//! engine: a read can be answered immediately, and a consequential act is
//! not performed by the voice. That phone engine is not imported here.

use super::analyze::{self, evidence_resolves};
use super::navigate::{self, Record};
use super::service::{self, NixonError, ReviewRequest};
use crate::ledger::engine::{fmt_money, Ledger};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
pub struct Scope {
    pub ledger: Arc<Ledger>,
    pub firm: Arc<crate::firm::Firm>,
    pub tax: Arc<crate::tax::Tax>,
    pub engagements: Arc<crate::engagement::Engagements>,
    pub signatures: Arc<crate::signatures::Signatures>,
    pub migrations: Arc<crate::migrations::Migrations>,
    pub company_books: Arc<crate::company_books::CompanyBooks>,
    pub referrals: Arc<crate::referral::engine::Engine>,
}

/// Same spoken-budget Sync still uses in `stripForSpeech`.
pub const SPEECH_MAX_CHARS: usize = 420;

#[derive(Debug, Deserialize)]
pub struct VoiceTurn {
    pub transcript: String,
    /// `push_to_talk` or `open_mic`.
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default)]
    pub last_spoken: Option<String>,
    #[serde(default)]
    pub choices: Vec<navigate::VoiceChoice>,
}

fn default_channel() -> String {
    "push_to_talk".into()
}

#[derive(Debug, Serialize)]
pub struct VoiceAnswer {
    pub say: String,
    pub intent: String,
    pub action: String,
    pub effect: String,
    pub posting: String,
    pub filing: String,
    pub approval: String,
    pub review_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<navigate::OpenTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<navigate::VoiceChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceIntent {
    Hold,
    Echo,
    Presence,
    Greeting,
    Social,
    RunReview,
    Explain,
    Reconciliation,
    Cash,
    RefuseApprove,
    RefusePost,
    RefuseFile,
    Navigate,
    Help,
}

pub fn normalize_heard(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drop a transcript that is the speaker hearing its own last line.
pub fn is_own_voice(heard: &str, spoken: &str) -> bool {
    let heard = normalize_heard(heard);
    let spoken = normalize_heard(spoken);
    if heard.len() < 10 || spoken.len() < 10 {
        return false;
    }
    if spoken.contains(&heard) || heard.contains(&spoken) {
        return true;
    }
    let heard_words: Vec<&str> = heard.split(' ').collect();
    let spoken_words: std::collections::HashSet<&str> = spoken.split(' ').collect();
    if heard_words.is_empty() {
        return false;
    }
    let overlap = heard_words.iter().filter(|w| spoken_words.contains(*w)).count();
    overlap * 100 / heard_words.len() >= 70
}

pub fn strip_for_speech(text: &str) -> String {
    let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.chars().count() <= SPEECH_MAX_CHARS {
        return trimmed;
    }
    let cut: String = trimmed.chars().take(SPEECH_MAX_CHARS).collect();
    let last_stop = cut.rfind(". ").or_else(|| cut.rfind("? ")).unwrap_or(0);
    if last_stop > 80 {
        cut.chars().take(last_stop + 1).collect()
    } else {
        format!("{}…", cut.trim_end())
    }
}

fn dollars_in_speech(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(idx) = rest.find(" cents") {
        let head = &rest[..idx];
        let digits_at = head.rfind(|c: char| !c.is_ascii_digit() && c != '-').map(|i| i + 1).unwrap_or(0);
        let number = &head[digits_at..];
        if let Ok(cents) = number.parse::<i64>() {
            if !number.is_empty() {
                out.push_str(&head[..digits_at]);
                out.push_str(&fmt_money(cents));
                rest = &rest[idx + " cents".len()..];
                continue;
            }
        }
        out.push_str(&rest[..idx + " cents".len()]);
        rest = &rest[idx + " cents".len()..];
    }
    out.push_str(rest);
    out
}

pub fn classify(transcript: &str, channel: &str) -> Result<VoiceIntent, String> {
    if channel != "push_to_talk" && channel != "open_mic" {
        return Err("channel must be push_to_talk or open_mic".into());
    }
    let heard = normalize_heard(transcript);
    if heard.is_empty() {
        return Ok(VoiceIntent::Hold);
    }
    if is_refusal_file(&heard) {
        return Ok(VoiceIntent::RefuseFile);
    }
    if is_refusal_post(&heard) {
        return Ok(VoiceIntent::RefusePost);
    }
    if is_refusal_approve(&heard) {
        return Ok(VoiceIntent::RefuseApprove);
    }
    if is_presence(&heard) {
        return Ok(VoiceIntent::Presence);
    }
    if is_greeting(&heard) {
        return Ok(VoiceIntent::Greeting);
    }
    if is_social(&heard) {
        return Ok(VoiceIntent::Social);
    }
    if is_run_review(&heard) {
        return Ok(VoiceIntent::RunReview);
    }
    if is_explain(&heard) {
        return Ok(VoiceIntent::Explain);
    }
    if is_reconciliation(&heard) {
        return Ok(VoiceIntent::Reconciliation);
    }
    if is_cash(&heard) {
        return Ok(VoiceIntent::Cash);
    }
    if is_navigation(&heard) {
        return Ok(VoiceIntent::Navigate);
    }
    let addressed = heard.split(' ').any(|w| w == "nixon");
    if channel == "open_mic" && !addressed {
        return Ok(VoiceIntent::Hold);
    }
    Ok(VoiceIntent::Help)
}

fn is_navigation(heard: &str) -> bool {
    let verb = heard.contains("open") || heard.contains("show") || heard.contains("pull up") || heard.contains("go to");
    let noun = heard.contains("ledger")
        || heard.contains("firm")
        || heard.contains("tax")
        || heard.contains("engagement")
        || heard.contains("signature")
        || heard.contains("referral")
        || heard.contains("migration")
        || heard.contains("parallel")
        || heard.contains("company book")
        || heard.contains("nixon")
        || heard.contains("account")
        || heard.contains("invoice")
        || heard.contains("bill")
        || heard.contains("client")
        || heard.contains("journal")
        || heard.contains("advocate");
    verb && noun
}

fn is_refusal_file(heard: &str) -> bool {
    (heard.contains("file") || heard.contains("submit"))
        && (heard.contains("tax") || heard.contains("return") || heard.contains("cra") || heard.contains("t2"))
}

fn is_refusal_post(heard: &str) -> bool {
    heard.contains("post a journal")
        || heard.contains("post the journal")
        || heard.contains("book an entry")
        || heard.contains("record a journal")
        || (heard.contains("categorize") && heard.contains("bank"))
}

fn is_refusal_approve(heard: &str) -> bool {
    heard.contains("approve") || heard.contains("reject the") || heard.contains("sign off")
}

fn is_presence(heard: &str) -> bool {
    heard.contains("can you hear")
        || heard.contains("could you hear")
        || heard.contains("are you there")
        || heard.contains("are you listening")
        || heard.contains("cant hear")
        || heard.contains("cannot hear")
}

fn is_greeting(heard: &str) -> bool {
    matches!(
        heard,
        "nixon" | "hey nixon" | "hi nixon" | "hello nixon" | "hello" | "hi" | "hey" | "good morning" | "good afternoon"
    )
}

fn is_social(heard: &str) -> bool {
    matches!(heard, "thanks" | "thank you" | "ok" | "okay" | "got it" | "yes" | "yeah")
}

fn is_run_review(heard: &str) -> bool {
    (heard.contains("review") && (heard.contains("run") || heard.contains("start") || heard.contains("do a")))
        || heard.contains("finance review")
}

fn is_explain(heard: &str) -> bool {
    heard.contains("explain")
}

fn is_reconciliation(heard: &str) -> bool {
    heard.contains("reconcil")
}

fn is_cash(heard: &str) -> bool {
    heard.contains("cash")
}

fn presence_line(heard: &str) -> &'static str {
    if heard.contains("cant hear") || heard.contains("cannot hear") {
        "I can hear you. If my voice is quiet, check your speaker volume."
    } else if heard.contains("can you hear") || heard.contains("could you hear") {
        "Yes. I can hear you."
    } else {
        "I'm here and listening."
    }
}

fn answer_shell(intent: &str, say: String, review_id: Option<String>, effect: &str) -> VoiceAnswer {
    VoiceAnswer {
        say: strip_for_speech(&dollars_in_speech(&say)),
        intent: intent.into(),
        action: "speak".into(),
        effect: effect.into(),
        posting: "disabled".into(),
        filing: "disabled".into(),
        approval: "not_by_voice".into(),
        review_id,
        open: None,
        choices: vec![],
    }
}

pub async fn handle(scope: &Scope, turn: VoiceTurn) -> Result<VoiceAnswer, NixonError> {
    let ledger = scope.ledger.as_ref();
    if turn.transcript.chars().count() > 2_000 {
        return Err(NixonError::new(400, "transcript is too long"));
    }
    if let Some(spoken) = turn.last_spoken.as_deref() {
        if is_own_voice(&turn.transcript, spoken) {
            let answer = VoiceAnswer {
                say: String::new(),
                intent: "echo".into(),
                action: "hold".into(),
                effect: "none".into(),
                posting: "disabled".into(),
                filing: "disabled".into(),
                approval: "not_by_voice".into(),
                review_id: None,
                open: None,
                choices: vec![],
            };
            audit(ledger, &turn, &answer).await?;
            return Ok(answer);
        }
    }
    let intent = classify(&turn.transcript, &turn.channel).map_err(|m| NixonError::new(400, m))?;
    if let Some(picked) = navigate::pick_choice(&normalize_heard(&turn.transcript), &turn.choices) {
        let answer = answer_shell("navigate", format!("Opening {}. {}", picked.label, picked.summary), None, "none");
        let mut answer = answer;
        answer.open = Some(navigate::OpenTarget { app: picked.app, view: picked.view });
        audit(ledger, &turn, &answer).await?;
        return Ok(answer);
    }
    let heard = normalize_heard(&turn.transcript);
    let answer = match intent {
        VoiceIntent::Hold => VoiceAnswer {
            say: if heard.is_empty() {
                String::new()
            } else {
                "Listening. Say Nixon when you want an answer.".into()
            },
            intent: "hold".into(),
            action: "hold".into(),
            effect: "none".into(),
            posting: "disabled".into(),
            filing: "disabled".into(),
            approval: "not_by_voice".into(),
            review_id: None,
            open: None,
            choices: vec![],
        },
        VoiceIntent::Echo => unreachable!("echo is handled before classify"),
        VoiceIntent::Presence => answer_shell("presence", presence_line(&heard).into(), None, "none"),
        VoiceIntent::Greeting => answer_shell("greeting", "Hello. I'm listening.".into(), None, "none"),
        VoiceIntent::Social => answer_shell(
            "social",
            if heard.contains("thank") { "You're welcome.".into() } else { "Understood. I'm listening.".into() },
            None,
            "none",
        ),
        VoiceIntent::RefuseApprove => answer_shell(
            "refuse_approve",
            "I can't approve or reject that. A named person has to decide on the screen. Voice does not change the books.".into(),
            None,
            "none",
        ),
        VoiceIntent::RefusePost => answer_shell(
            "refuse_post",
            "I can't post a journal or categorize the bank feed. That stays on the Ledger screen.".into(),
            None,
            "none",
        ),
        VoiceIntent::RefuseFile => answer_shell(
            "refuse_file",
            "I can't file a tax return. Filing stays off.".into(),
            None,
            "none",
        ),
        VoiceIntent::Navigate => open_records(scope, &heard).await?,
        VoiceIntent::Help => answer_shell(
            "help",
            "I can run a review, explain a finding, show reconciliation gaps, give the cash position, or open a module, account, invoice, client, or engagement. I can't post, file, or approve.".into(),
            None,
            "none",
        ),
        VoiceIntent::RunReview => {
            let review = service::run_review(
                ledger,
                ReviewRequest { idempotency_key: None, cutover: None, surfaces: vec![] },
            )
            .await?;
            answer_shell(
                "run_review",
                format!("I recorded a review. {}", review.summary),
                Some(review.id),
                "review_recorded",
            )
        }
        VoiceIntent::Explain => explain(ledger, &heard).await?,
        VoiceIntent::Reconciliation => speak_live(ledger, "reconciliation").await?,
        VoiceIntent::Cash => speak_live(ledger, "cash").await?,
    };
    audit(ledger, &turn, &answer).await?;
    Ok(answer)
}

async fn explain(ledger: &Ledger, heard: &str) -> Result<VoiceAnswer, NixonError> {
    let reviews = service::list_reviews(ledger).await?;
    let Some(latest) = reviews.first() else {
        return Ok(answer_shell(
            "explain",
            "There is no review yet. Ask me to run one. I won't invent a finding.".into(),
            None,
            "none",
        ));
    };
    let id = latest["id"].as_str().unwrap_or("");
    let review = service::get_review(ledger, id).await?;
    let topic = heard
        .split(' ')
        .filter(|w| !matches!(*w, "explain" | "the" | "a" | "finding" | "about" | "nixon" | "please" | "me"))
        .collect::<Vec<_>>()
        .join(" ");
    let finding = review
        .findings
        .iter()
        .find(|f| !topic.is_empty() && normalize_heard(&format!("{} {}", f.title, f.summary)).contains(&topic))
        .or_else(|| review.findings.iter().find(|f| f.requires_human_review))
        .or_else(|| review.findings.first());
    let Some(finding) = finding else {
        return Ok(answer_shell("explain", "The latest review has no findings. I won't invent one.".into(), Some(review.id), "none"));
    };
    let hypothesis = finding
        .hypotheses
        .first()
        .map(|h| format!(" Hypothesis, not a cause: {}", h.statement))
        .unwrap_or_default();
    Ok(answer_shell(
        "explain",
        format!("{} {}", finding.summary, hypothesis),
        Some(review.id),
        "none",
    ))
}

async fn speak_live(ledger: &Ledger, kind: &str) -> Result<VoiceAnswer, NixonError> {
    let ctx = service::load_context(ledger).await?;
    let analysis = analyze::analyze(&ctx, None, &[]).map_err(|m| NixonError::new(500, m))?;
    evidence_resolves(&ctx, None, &[], &analysis).map_err(|m| NixonError::new(500, m))?;
    let say = if kind == "cash" {
        analysis
            .findings
            .iter()
            .find(|f| f.id == "f_cash_movement")
            .map(|f| f.summary.clone())
            .unwrap_or_else(|| "I could not read a cash position, so I did not invent one.".into())
    } else {
        let gaps: Vec<&str> = analysis
            .findings
            .iter()
            .filter(|f| f.capability == "reconciliation" || f.id.starts_with("f_recon"))
            .map(|f| f.summary.as_str())
            .collect();
        if gaps.is_empty() {
            "I don't see a reconciliation gap in the books I can read. I did not invent a statement balance.".into()
        } else {
            format!("{} reconciliation note(s). {}", gaps.len(), gaps[0])
        }
    };
    Ok(answer_shell(kind, say, None, "none"))
}

async fn open_records(scope: &Scope, heard: &str) -> Result<VoiceAnswer, NixonError> {
    if let Some(module) = navigate::module_target(heard) {
        let mut answer = answer_shell("navigate", format!("Opening {}. {}", module.label, module.summary), None, "none");
        answer.open = Some(navigate::OpenTarget { app: module.app, view: module.view });
        return Ok(answer);
    }
    if heard.contains("journal") && heard.contains("month") {
        return journal_month(scope.ledger.as_ref()).await;
    }
    let query = heard
        .split(' ')
        .filter(|w| !matches!(*w, "open" | "the" | "show" | "me" | "pull" | "up" | "go" | "to" | "please" | "nixon" | "a" | "an" | "of"))
        .collect::<Vec<_>>()
        .join(" ");
    let catalog = catalog(scope).await;
    let hits = navigate::rank(&query, &catalog);
    if hits.is_empty() {
        return Ok(answer_shell("navigate", format!("I couldn't find {query}. I didn't open a guessed record."), None, "none"));
    }
    if hits.len() == 1 {
        let hit = hits[0];
        let mut answer = answer_shell("navigate", format!("Opening {}. {}", hit.label, hit.summary), None, "none");
        answer.open = Some(navigate::OpenTarget { app: hit.app.clone(), view: hit.view.clone() });
        return Ok(answer);
    }
    let choices: Vec<_> = hits.iter().map(|h| navigate::choice(h)).collect();
    let spoken = choices.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c.label)).collect::<Vec<_>>().join(" ");
    let mut answer = answer_shell("navigate", format!("I found {} matches. {spoken} Say first or second.", choices.len()), None, "none");
    answer.choices = choices;
    Ok(answer)
}

async fn journal_month(ledger: &Ledger) -> Result<VoiceAnswer, NixonError> {
    let today = crate::ledger::engine::today();
    let month = &today[..7];
    let entries = ledger.entries().await?;
    let matched: Vec<_> = entries.iter().filter(|e| !e.voided && e.date.starts_with(month)).collect();
    let sample = matched.iter().take(3).map(|e| format!("number {} on {}", e.number, e.date)).collect::<Vec<_>>().join(", ");
    let say = if matched.is_empty() {
        format!("There are no journal entries in {month}. I opened the journal and did not invent any.")
    } else {
        format!("{} journal entries in {month}. {sample}.", matched.len())
    };
    let mut answer = answer_shell("navigate", say, None, "none");
    answer.open = Some(navigate::OpenTarget { app: "ledger".into(), view: "journal".into() });
    Ok(answer)
}

async fn catalog(scope: &Scope) -> Vec<Record> {
    let mut out = Vec::new();
    if let Ok(accounts) = scope.ledger.accounts().await {
        for account in accounts {
            let mut keys = vec![account.code.clone(), account.name.clone(), account.id.clone()];
            if matches!(account.role, crate::ledger::models::AccountRole::Bank) {
                keys.extend(["cash".into(), "bank".into(), "cash at bank".into()]);
            }
            out.push(Record {
                app: "ledger".into(),
                view: format!("registers/{}", account.id),
                label: format!("Account {} {}", account.code, account.name),
                summary: format!("{} is account {}.", account.name, account.code),
                keys,
            });
        }
    }
    if let Ok(docs) = scope.ledger.docs(None).await {
        for doc in docs {
            let kind = match doc.kind {
                crate::ledger::models::DocKind::Invoice => "invoices",
                crate::ledger::models::DocKind::Bill => "bills",
            };
            out.push(Record {
                app: "ledger".into(),
                view: format!("{kind}/{}", doc.id),
                label: format!("{} {}", if kind == "invoices" { "Invoice" } else { "Bill" }, doc.number),
                summary: format!("{} {} totals {}.", if kind == "invoices" { "Invoice" } else { "Bill" }, doc.number, fmt_money(doc.total)),
                keys: vec![doc.number.clone(), doc.id.clone(), format!("{kind} {}", doc.number)],
            });
        }
    }
    if let Ok(contacts) = scope.ledger.contacts(None).await {
        for contact in contacts {
            out.push(Record {
                app: "ledger".into(),
                view: match contact.kind {
                    crate::ledger::models::ContactKind::Customer => "customers".into(),
                    crate::ledger::models::ContactKind::Vendor => "vendors".into(),
                },
                label: format!("Client {}", contact.name),
                summary: format!("{} is a {}.", contact.name, if matches!(contact.kind, crate::ledger::models::ContactKind::Customer) { "customer" } else { "vendor" }),
                keys: vec![contact.name.clone(), format!("client {}", contact.name)],
            });
        }
    }
    if let Ok(state) = scope.firm.snapshot().await {
        for project in state.projects {
            let open = project.tasks.iter().filter(|t| !t.completed).count();
            out.push(Record {
                app: "firm".into(),
                view: "projects".into(),
                label: format!("Project {}", project.name),
                summary: format!("{} has {open} open tasks. Opening it does not approve them.", project.name),
                keys: vec![project.name.clone(), format!("project {}", project.name)],
            });
        }
    }
    if let Ok(returns) = scope.tax.list(None).await {
        for record in returns {
            let name = record["name"].as_str().unwrap_or("workpaper");
            let client = record["client_ref"].as_str().unwrap_or("");
            let id = record["id"].as_str().unwrap_or("");
            out.push(Record {
                app: "tax".into(),
                view: format!("return/{id}"),
                label: format!("Tax {name}"),
                summary: format!("{name} is a {} workpaper. Filing is off.", record["program"].as_str().unwrap_or("tax")),
                keys: vec![name.into(), client.into(), format!("client {client}")],
            });
        }
    }
    if let Ok(rows) = scope.engagements.list().await {
        for record in rows {
            let name = record["name"].as_str().unwrap_or("engagement");
            let client = record["client_ref"].as_str().unwrap_or("");
            let id = record["id"].as_str().unwrap_or("");
            out.push(Record {
                app: "engagements".into(),
                view: format!("engagement/{id}"),
                label: format!("Engagement {name}"),
                summary: format!("{name} is {}.", record["status"].as_str().unwrap_or("an engagement")),
                keys: vec![name.into(), client.into(), format!("engagement {name}"), format!("client {client}")],
            });
        }
    }
    if let Ok(page) = scope.signatures.list().await {
        if let Some(rows) = page["envelopes"].as_array() {
            for record in rows {
                let title = record["title"].as_str().unwrap_or("preparation");
                let id = record["id"].as_str().unwrap_or("");
                out.push(Record {
                    app: "signatures".into(),
                    view: format!("envelope/{id}"),
                    label: format!("Signature {title}"),
                    summary: format!("{title} is {}. Nothing has been sent or signed.", record["status"].as_str().unwrap_or("a draft")),
                    keys: vec![title.into()],
                });
            }
        }
    }
    if let Ok(page) = scope.migrations.list().await {
        if let Some(rows) = page["snapshots"].as_array() {
            for record in rows {
                let name = record["company_name"].as_str().unwrap_or("snapshot");
                let id = record["id"].as_str().unwrap_or("");
                out.push(Record {
                    app: "migrations".into(),
                    view: format!("snapshot/{id}"),
                    label: format!("Snapshot {name}"),
                    summary: format!("{name} is a read-only source snapshot."),
                    keys: vec![name.into(), record["cohort_ref"].as_str().unwrap_or("").into()],
                });
            }
        }
    }
    if let Ok(Some(head)) = scope.company_books.store.get("company-books:roster:head:v1").await {
        if let Ok(Some(raw)) = scope.company_books.store.get(&format!("company-books:roster:capture:{head}")).await {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(rows) = value["companies"].as_array() {
                    for row in rows {
                        let name = row["company_name"].as_str().unwrap_or("");
                        let id = row["company_id"].as_str().unwrap_or("");
                        if id.is_empty() {
                            continue;
                        }
                        out.push(Record {
                            app: "migrations".into(),
                            view: format!("company/{id}"),
                            label: format!("Company {name}"),
                            summary: format!("{name} is an observed company. Opening it does not import journals."),
                            keys: vec![name.into(), id.into(), format!("client {name}")],
                        });
                    }
                }
            }
        }
    }
    if let Ok(advocates) = scope.referrals.list_advocates().await {
        for advocate in advocates {
            out.push(Record {
                app: "referrals".into(),
                view: "advocates".into(),
                label: format!("Advocate {}", advocate.name),
                summary: format!("{} has {} referrals.", advocate.name, advocate.referrals),
                keys: vec![advocate.name.clone(), advocate.code.clone(), format!("client {}", advocate.name)],
            });
        }
    }
    out.truncate(400);
    out
}

async fn audit(ledger: &Ledger, turn: &VoiceTurn, answer: &VoiceAnswer) -> Result<(), NixonError> {
    let heard: String = turn.transcript.chars().take(160).collect();
    ledger
        .audit(
            "nixon.voice",
            format!(
                "intent={} action={} effect={} posting=disabled filing=disabled approval=not_by_voice channel={} transcript={}",
                answer.intent, answer.action, answer.effect, turn.channel, heard
            ),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_refusals_are_distinct() {
        assert_eq!(classify("Nixon, run a review", "push_to_talk").unwrap(), VoiceIntent::RunReview);
        assert_eq!(classify("explain the cash finding", "push_to_talk").unwrap(), VoiceIntent::Explain);
        assert_eq!(classify("show reconciliation gaps", "open_mic").unwrap(), VoiceIntent::Reconciliation);
        assert_eq!(classify("what is our cash position", "push_to_talk").unwrap(), VoiceIntent::Cash);
        assert_eq!(classify("approve the finding", "push_to_talk").unwrap(), VoiceIntent::RefuseApprove);
        assert_eq!(classify("post a journal for the difference", "push_to_talk").unwrap(), VoiceIntent::RefusePost);
        assert_eq!(classify("file the tax return", "push_to_talk").unwrap(), VoiceIntent::RefuseFile);
        assert_eq!(classify("can you hear me", "open_mic").unwrap(), VoiceIntent::Presence);
        assert_eq!(classify("hey nixon", "open_mic").unwrap(), VoiceIntent::Greeting);
        assert!(classify("what did we decide", "meet").is_err());
    }

    #[test]
    fn open_mic_ignores_chatter_that_does_not_address_nixon() {
        assert_eq!(classify("the meeting is still going", "open_mic").unwrap(), VoiceIntent::Hold);
        assert_eq!(classify("the meeting is still going", "push_to_talk").unwrap(), VoiceIntent::Help);
    }

    #[test]
    fn own_voice_and_speech_budget() {
        let spoken = "I recorded a review. Three findings are waiting.";
        assert!(is_own_voice(spoken, spoken));
        assert!(!is_own_voice("run a review", spoken));
        let long = "word ".repeat(200);
        let stripped = strip_for_speech(&long);
        assert!(stripped.chars().count() <= SPEECH_MAX_CHARS + 1);
    }
}
