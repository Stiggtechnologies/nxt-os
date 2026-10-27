//! Telephony helpers: phone numbers, Twilio signatures, ConversationRelay
//! tokens, echo suppression, and TwiML. No network calls.

use super::config::{Config, Transport};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

pub fn digits_only(n: &str) -> String {
    n.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Last 10 digits, dropping a leading country digit 1 when the rest is a
/// 10-digit NANP number.
pub fn digits10(n: &str) -> String {
    let d = digits_only(n);
    if d.len() == 11 && d.starts_with('1') {
        d[1..].to_string()
    } else {
        d
    }
}

pub fn e164(n: &str) -> Option<String> {
    let t = n.trim();
    if let Some(rest) = t.strip_prefix('+') {
        let d: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
        if (8..=15).contains(&d.len()) && rest.chars().all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '-' || c == '(' || c == ')') {
            return Some(format!("+{d}"));
        }
        if (8..=15).contains(&d.len()) && rest.chars().filter(|c| c.is_ascii_digit()).count() == d.len() {
            return Some(format!("+{d}"));
        }
    }
    let d = digits10(t);
    if d.len() == 10 {
        Some(format!("+1{d}"))
    } else {
        None
    }
}

pub fn mask_number(n: &str) -> String {
    let d = digits_only(n);
    if d.len() < 8 {
        return "•••".into();
    }
    format!("+{}***{}", &d[..d.len().saturating_sub(7)].chars().take(4).collect::<String>(), &d[d.len() - 4..])
}

/// 211/311/…/911 and 988. An assistant must not dial these.
pub fn is_blocked_service(n: &str) -> bool {
    let d = digits10(n);
    if d.len() != 10 {
        return false;
    }
    let npa = &d[..3];
    npa == "988" || (npa.as_bytes()[1] == b'1' && npa.as_bytes()[2] == b'1')
}

pub fn is_dialable(n: &str) -> bool {
    match e164(n) {
        Some(e) => e.len() == 12 && e.starts_with("+1") && !is_blocked_service(&e),
        None => false,
    }
}

pub fn xml_esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '&' => "&amp;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

pub fn stream_token(secret: &str, call_sid: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(call_sid.as_bytes());
    hex::encode(mac.finalize().into_bytes())[..32].to_string()
}

pub fn token_ok(secret: &str, call_sid: &str, token: &str) -> bool {
    if call_sid.is_empty() || token.is_empty() {
        return false;
    }
    let want = stream_token(secret, call_sid);
    crate::referral::routes::constant_time_eq(want.as_bytes(), token.as_bytes())
}

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    if i < data.len() {
        let rem = data.len() - i;
        let mut n = (data[i] as u32) << 16;
        if rem == 2 {
            n |= (data[i + 1] as u32) << 8;
        }
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if rem == 2 {
            out.push(T[((n >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

/// Twilio's webhook signature: Base64(HMAC-SHA1(token, url + sorted name/value pairs)).
pub fn twilio_signature(token: &str, url: &str, params: &[(String, String)]) -> String {
    let mut pairs = params.to_vec();
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut data = url.to_string();
    for (k, v) in &pairs {
        data.push_str(k);
        data.push_str(v);
    }
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, token.as_bytes());
    b64(ring::hmac::sign(&key, data.as_bytes()).as_ref())
}

pub fn signature_matches(token: &str, url: &str, params: &[(String, String)], given: &str) -> bool {
    let want = twilio_signature(token, url, params);
    crate::referral::routes::constant_time_eq(want.as_bytes(), given.as_bytes())
}

pub fn form_pairs(body: &[u8]) -> Vec<(String, String)> {
    let s = String::from_utf8_lossy(body);
    let mut out = Vec::new();
    if s.is_empty() {
        return out;
    }
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k = url_decode(k);
        if k.is_empty() {
            continue;
        }
        out.push((k, url_decode(v)));
    }
    out
}

fn url_decode(s: &str) -> String {
    let b = s.replace('+', " ");
    let bytes = b.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn param<'a>(params: &'a [(String, String)], key: &str) -> &'a str {
    params.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap_or("")
}

fn norm_words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Drop a leading copy of what the caller just said. ConversationRelay does
/// not feed our own TTS back into recognition; this stops the model from
/// speaking the caller's line back at them anyway.
pub fn strip_echo(user: &str, draft: &str) -> String {
    let draft = draft.trim();
    let user_n = norm_words(user);
    let draft_n = norm_words(draft);
    if draft_n.is_empty() || user_n.len() < 8 {
        return draft.to_string();
    }
    if draft_n == user_n {
        return String::new();
    }
    if let Some(rest) = draft_n.strip_prefix(&user_n) {
        let rest = rest.trim().trim_start_matches(|c: char| matches!(c, ',' | '.' | ':' | '-' | ';' | ' '));
        if rest.is_empty() {
            return String::new();
        }
        // Rebuild from the original tail by skipping the echoed word count.
        let skip = user_n.split_whitespace().count();
        let tail: Vec<&str> = draft.split_whitespace().skip(skip).collect();
        let mut s = tail.join(" ");
        s = s.trim_start_matches(|c: char| matches!(c, ',' | '.' | ':' | '-' | ';' )).trim().to_string();
        return s;
    }
    draft.to_string()
}

pub fn finalize_speech(user: &str, draft: &str) -> String {
    let mut s = strip_echo(user, draft);
    if s.len() > 600 {
        s.truncate(600);
        if let Some(i) = s.rfind(' ') {
            s.truncate(i);
        }
        s.push('.');
    }
    let low = s.to_lowercase();
    if s.trim().is_empty()
        || s.trim().starts_with('{')
        || s.trim().starts_with('[')
        || low.contains("phone manners")
        || low.contains("system prompt")
        || s.contains("GUARDVOICE_")
        || s.contains("ANTHROPIC_API_KEY")
    {
        return "Okay. What do you need?".into();
    }
    s.trim().to_string()
}

/// `None` when the caller barged in and this draft belongs to an older turn.
pub fn gate_speech(start_generation: u64, current_generation: u64, user: &str, draft: &str) -> Option<String> {
    if start_generation != current_generation {
        None
    } else {
        Some(finalize_speech(user, draft))
    }
}

pub fn host_of(config: &Config, headers: &axum::http::HeaderMap) -> String {
    if !config.public_host.is_empty() {
        return config.public_host.clone();
    }
    let raw = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(axum::http::header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    raw.split(',').next().unwrap_or("").trim().trim_end_matches('/').to_string()
}

pub fn public_url(host: &str, path_and_query: &str) -> String {
    format!("https://{host}{path_and_query}")
}

fn wrap(inner: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response>{inner}</Response>")
}

pub fn say_hangup(text: &str) -> String {
    wrap(&format!("<Say language=\"en-CA\">{text}</Say><Hangup/>", text = xml_esc(text)))
}

pub fn empty_response() -> String {
    wrap("")
}

pub fn relay_twiml(config: &Config, host: &str, call_sid: &str, greeting: &str) -> String {
    let token = stream_token(&config.stream_secret, call_sid);
    let url = format!("wss://{host}/api/guardvoice/relay?callSid={call_sid}&amp;token={token}");
    wrap(&format!(
        "<Connect action=\"https://{host}/api/guardvoice/twiml/relay-done\" method=\"POST\">\
<ConversationRelay url=\"{url}\" welcomeGreeting=\"{greet}\" language=\"{lang}\" ttsProvider=\"{tts}\" voice=\"{voice}\" transcriptionProvider=\"{stt}\" speechModel=\"nova-2-phonecall\" interruptible=\"true\" welcomeGreetingInterruptible=\"true\"/>\
</Connect>",
        greet = xml_esc(greeting),
        lang = xml_esc(&config.language),
        tts = xml_esc(&config.tts_provider),
        voice = xml_esc(&config.tts_voice),
        stt = xml_esc(&config.stt_provider),
    ))
}

pub fn gather_twiml(host: &str, say: &str, hangup_after: bool) -> String {
    if hangup_after {
        return say_hangup(say);
    }
    wrap(&format!(
        "<Gather input=\"speech\" speechTimeout=\"auto\" language=\"en-CA\" action=\"https://{host}/api/guardvoice/twiml/gather\" method=\"POST\">\
<Say language=\"en-CA\">{text}</Say></Gather><Say language=\"en-CA\">I didn't catch that. Goodbye.</Say><Hangup/>",
        text = xml_esc(say)
    ))
}

pub fn inbound_twiml(config: &Config, host: &str, call_sid: &str, greeting: &str) -> String {
    match config.transport {
        Transport::Relay => relay_twiml(config, host, call_sid, greeting),
        Transport::Gather => gather_twiml(host, greeting, false),
    }
}

pub fn continue_twiml(config: &Config, host: &str, call_sid: &str, say: &str, hangup_after: bool) -> String {
    match config.transport {
        Transport::Gather => gather_twiml(host, say, hangup_after),
        Transport::Relay if hangup_after => say_hangup(say),
        Transport::Relay => relay_twiml(config, host, call_sid, say),
    }
}

pub fn screened_dial_twiml(config: &Config, host: &str, parent_sid: &str, target_e164: &str, reason: &str, caller_name: &str) -> String {
    let caller_id = config.from_number.as_deref().unwrap_or("");
    let q = format!(
        "p={}&amp;r={}&amp;n={}",
        urlencoding_min(parent_sid),
        urlencoding_min(reason),
        urlencoding_min(caller_name)
    );
    let cid = if caller_id.is_empty() {
        String::new()
    } else {
        format!(" callerId=\"{}\"", xml_esc(caller_id))
    };
    wrap(&format!(
        "<Dial timeout=\"20\"{cid} action=\"https://{host}/api/guardvoice/twiml/transfer-result\" method=\"POST\" answerOnBridge=\"true\">\
<Number url=\"https://{host}/api/guardvoice/twiml/transfer-whisper?{q}\" method=\"POST\">{num}</Number></Dial>",
        num = xml_esc(target_e164)
    ))
}

pub fn whisper_twiml(host: &str, parent_sid: &str, label: &str, caller_name: &str, reason: &str) -> String {
    let who = if caller_name.is_empty() { String::new() } else { format!(", {caller_name}") };
    let why = if reason.is_empty() { String::new() } else { format!(". {reason}") };
    let say = format!("{label} caller{who}{why}. Press 1 to accept, or 2 to send them back to the assistant.");
    wrap(&format!(
        "<Gather numDigits=\"1\" timeout=\"6\" action=\"https://{host}/api/guardvoice/twiml/transfer-screen?p={p}\" method=\"POST\">\
<Say language=\"en-CA\">{text}</Say><Pause length=\"1\"/><Say language=\"en-CA\">Press 1 to accept the call.</Say></Gather><Hangup/>",
        p = urlencoding_min(parent_sid),
        text = xml_esc(&say)
    ))
}

pub fn screen_twiml(digit: &str) -> String {
    if digit == "1" {
        wrap("<Say language=\"en-CA\">Connecting.</Say>")
    } else {
        wrap("<Hangup/>")
    }
}

fn urlencoding_min(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn clean_phrase(s: &str, max: usize) -> String {
    let t: String = s.chars().filter(|c| c.is_alphanumeric() || " .,'?!/&()-".contains(*c)).take(max).collect();
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twilio_docs_signature_vector() {
        let params = vec![
            ("CallSid".into(), "CA1234567890ABCDE".into()),
            ("Caller".into(), "+14158675309".into()),
            ("Digits".into(), "1234".into()),
            ("From".into(), "+14158675309".into()),
            ("To".into(), "+18005551212".into()),
        ];
        let sig = twilio_signature("12345", "https://mycompany.com/myapp.php?foo=1&bar=2", &params);
        assert_eq!(sig, "RSOYDt4T1cUTdK1PDd93/VVr8B8=");
    }

    #[test]
    fn echo_is_not_spoken() {
        let user = "How much cash do we have?";
        assert_eq!(strip_echo(user, user), "");
        assert_eq!(
            finalize_speech(user, "How much cash do we have? Cash on hand is $0.00."),
            "Cash on hand is $0.00."
        );
        assert_eq!(gate_speech(1, 2, user, "Cash on hand is $0.00."), None);
        assert!(finalize_speech(user, "{ \"tool\": 1 }").starts_with("Okay"));
    }

    #[test]
    fn service_numbers_are_not_dialable() {
        assert!(is_blocked_service("+19115550100") || is_blocked_service("9115550100"));
        assert!(is_blocked_service("9885550100"));
        assert!(!is_dialable("+19115550100"));
        assert!(is_dialable("5555550100"));
        assert_eq!(e164("555-555-0100").as_deref(), Some("+15555550100"));
    }

    #[test]
    fn relay_twiml_has_no_secrets() {
        let cfg = Config::mock();
        let xml = relay_twiml(&cfg, "voice.test", "CA123", "Hello there");
        assert!(xml.contains("<ConversationRelay"));
        assert!(xml.contains("wss://voice.test/api/guardvoice/relay"));
        assert!(xml.contains("interruptible=\"true\""));
        assert!(xml.contains("welcomeGreeting=\"Hello there\""));
        assert!(!xml.contains(&cfg.stream_secret));
        assert!(!xml.contains("AC"));
        let token = stream_token(&cfg.stream_secret, "CA123");
        assert!(xml.contains(&token));
        assert!(token_ok(&cfg.stream_secret, "CA123", &token));
        assert!(!token_ok(&cfg.stream_secret, "CA123", "nope"));
    }
}
