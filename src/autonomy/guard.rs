//! Sensitivity guard for autonomy. It builds on God's Eye's
//! [`ai_gateway`](crate::nixon_finance::ai_gateway) (its `Sensitivity`, routing guard and
//! digit redaction) and adds the following:
//! - a content classifier, so text that *looks* sensitive is treated as client data even
//!   when a caller declares it non-sensitive (the NVIDIA free tier is then never planned);
//! - key masking on top of God's Eye's digit redaction;
//! - JSON redaction for the audit log.

pub use crate::nixon_finance::ai_gateway::Sensitivity;
use serde::{Deserialize, Serialize};

/// T0 public, T1 internal, T2 personal or client financial data, T3 tax identifiers,
/// returns or secrets (same tiers as the gateway plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    T0,
    T1,
    T2,
    T3,
}

impl Tier {
    pub fn sensitivity(self) -> Sensitivity {
        if self >= Tier::T2 { Sensitivity::ClientData } else { Sensitivity::NonSensitive }
    }
}

const KEY_MARKERS: &[&str] = &["sk_live_", "sk_test_", "rk_live_", "whsec_", "gho_", "ghp_", "xoxb-", "AKIA", "-----BEGIN"];

/// `YYYY-MM-DD` dates are not identifiers.
fn is_date(segment: &str) -> bool {
    let b = segment.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && segment.chars().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Digit counts of each number in the text, allowing single spaces or dashes inside
/// (046 454 286). Dates are skipped.
fn numbers(text: &str) -> Vec<usize> {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut digits, mut raw) = (Vec::new(), 0usize, String::new());
    let mut flush = |digits: &mut usize, raw: &mut String| {
        if *digits > 0 && !is_date(raw.trim()) {
            out.push(*digits);
        }
        *digits = 0;
        raw.clear();
    };
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_digit() {
            digits += 1;
            raw.push(c);
        } else if (c == '-' || c == ' ') && digits > 0 && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
            raw.push(c);
        } else {
            flush(&mut digits, &mut raw);
        }
    }
    flush(&mut digits, &mut raw);
    out
}

/// Highest tier the text appears to contain.
pub fn classify(text: &str) -> Tier {
    let lower = text.to_lowercase();
    let nums = numbers(text);
    if nums.contains(&9) || lower.contains("sin ") || lower.contains("social insurance") || KEY_MARKERS.iter().any(|m| text.contains(m)) {
        return Tier::T3;
    }
    if ["t1 return", "t2 return", "notice of assessment", " rt0", " rc0", " rp0"].iter().any(|m| lower.contains(m)) {
        return Tier::T3;
    }
    if nums.iter().any(|&n| n >= 7) || (text.contains('@') && text.contains('.')) {
        return Tier::T2;
    }
    if text.contains('$') || ["invoice", "client", "balance"].iter().any(|m| lower.contains(m)) {
        return Tier::T2;
    }
    Tier::T1
}

/// The caller's declared tier or the content's, whichever is higher.
pub fn effective_tier(declared: Tier, text: &str) -> Tier {
    declared.max(classify(text))
}

/// Masks secrets, then applies God's Eye's redaction (7+ digit runs keep their last 3).
pub fn redact(text: &str) -> String {
    let masked: Vec<String> = text
        .split_inclusive(char::is_whitespace)
        .map(|w| if KEY_MARKERS.iter().any(|m| w.contains(m)) { format!("[redacted-key]{}", if w.ends_with(char::is_whitespace) { " " } else { "" }) } else { w.to_string() })
        .collect();
    crate::nixon_finance::ai_gateway::redact(&masked.concat())
}

/// Recursively redacts every string in a JSON value.
pub fn redact_json(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::String(s) => Value::String(redact(s)),
        Value::Array(a) => Value::Array(a.iter().map(redact_json).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), redact_json(v))).collect()),
        other => other.clone(),
    }
}
