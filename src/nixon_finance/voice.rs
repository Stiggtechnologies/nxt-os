//! Nixon voice commands.
//!
//! The shape follows Sync Context voice (SCX-VOICE) and the Meet Sync booth:
//! hear a turn, classify it, speak a short answer, and leave approval on the
//! screen. Speech here is a transcript in and a sentence out. The browser
//! supplies recognition and speech synthesis. This module does not post,
//! file, or approve.
//!
//! The scripted gate matches the idea in draft PR #4's `src/nixon` phone
//! engine: a read can be answered immediately, and a consequential act is
//! not performed by the voice. That phone engine is not imported here.

use super::analyze::{self, evidence_resolves};
use super::service::{self, NixonError, ReviewRequest};
use crate::ledger::engine::{fmt_money, Ledger};
use serde::{Deserialize, Serialize};

/// Same spoken-budget as Meet Sync `stripForSpeech`.
pub const SPEECH_MAX_CHARS: usize = 420;

#[derive(Debug, Deserialize)]
pub struct VoiceTurn {
    pub transcript: String,
    /// `push_to_talk` or `open_mic`.
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default)]
    pub last_spoken: Option<String>,
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
    let addressed = heard.split(' ').any(|w| w == "nixon");
    if channel == "open_mic" && !addressed {
        return Ok(VoiceIntent::Hold);
    }
    Ok(VoiceIntent::Help)
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
    }
}

pub async fn handle(ledger: &Ledger, turn: VoiceTurn) -> Result<VoiceAnswer, NixonError> {
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
            };
            audit(ledger, &turn, &answer).await?;
            return Ok(answer);
        }
    }
    let intent = classify(&turn.transcript, &turn.channel).map_err(|m| NixonError::new(400, m))?;
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
        VoiceIntent::Help => answer_shell(
            "help",
            "I can run a review, explain a finding, show reconciliation gaps, or give the cash position. I can't post, file, or approve.".into(),
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
