//! Universal Nixon entry points: one start URL (`/apps/nixon/start.html?source=..&utm_*=..`)
//! that lands on the tax-request start and records where the person came from on the intake.
//!
//! Shipped DISABLED. Only `NXT_NIXON_ENTRY_ENABLED=true` turns it on, and even then every
//! route sits behind staff auth: no public links, no pixels, no ad or social integrations.

use serde::{Deserialize, Serialize};

/// Channels the start URL understands. Anything else is recorded as `link`.
pub const CHANNELS: [&str; 7] = ["google", "facebook", "instagram", "linkedin", "x", "ads", "link"];

pub fn enabled() -> bool {
    std::env::var("NXT_NIXON_ENTRY_ENABLED").is_ok_and(|v| v == "true")
}

/// Raw query/body fields as they arrive from the start URL.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryQuery {
    #[serde(default)] pub source: Option<String>,
    #[serde(default)] pub utm_source: Option<String>,
    #[serde(default)] pub utm_medium: Option<String>,
    #[serde(default)] pub utm_campaign: Option<String>,
    #[serde(default)] pub utm_content: Option<String>,
    #[serde(default)] pub utm_term: Option<String>,
    #[serde(default)] pub gclid: Option<String>,
    #[serde(default)] pub fbclid: Option<String>,
    #[serde(default)] pub li_fat_id: Option<String>,
    #[serde(default)] pub twclid: Option<String>,
}

/// What is stored on the intake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntrySource {
    pub channel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub utm_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub utm_medium: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub utm_campaign: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub utm_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub utm_term: Option<String>,
    /// Which ad click id was present (the id itself is not kept).
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub click_ids: Vec<String>,
    pub landing: String,
}

fn clean(v: &Option<String>, field: &str) -> Result<Option<String>, String> {
    let Some(s) = v.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) else { return Ok(None) };
    if s.chars().count() > 100 || !s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ' | '+')) {
        return Err(format!("{field} must be up to 100 letters, digits, - _ . + or spaces"));
    }
    Ok(Some(s.to_lowercase()))
}

fn channel_of(s: &str) -> Option<&'static str> {
    Some(match s {
        "google" | "google.com" | "googleads" | "google_ads" | "adwords" | "youtube" => "google",
        "facebook" | "fb" | "facebook.com" | "meta" | "messenger" => "facebook",
        "instagram" | "ig" | "instagram.com" => "instagram",
        "linkedin" | "li" | "linkedin.com" => "linkedin",
        "x" | "twitter" | "x.com" | "twitter.com" | "t.co" => "x",
        "ads" | "ad" | "paid" | "cpc" | "ppc" | "display" => "ads",
        "link" | "generic" | "direct" | "email_link" | "qr" => "link",
        _ => return None,
    })
}

/// Normalises a start-URL query into the source recorded on the intake.
pub fn classify(q: &EntryQuery) -> Result<EntrySource, String> {
    let source = clean(&q.source, "source")?;
    let utm_source = clean(&q.utm_source, "utm_source")?;
    let utm_medium = clean(&q.utm_medium, "utm_medium")?;
    let mut click_ids = Vec::new();
    for (name, v) in [("gclid", &q.gclid), ("fbclid", &q.fbclid), ("li_fat_id", &q.li_fat_id), ("twclid", &q.twclid)] {
        if clean(v, name)?.is_some() { click_ids.push(name.to_string()); }
    }
    let by_click = click_ids.first().map(|c| match c.as_str() { "gclid" => "google", "fbclid" => "facebook", "li_fat_id" => "linkedin", _ => "x" });
    let channel = source.as_deref().and_then(channel_of)
        .or_else(|| utm_source.as_deref().and_then(channel_of))
        .or(by_click)
        .or_else(|| utm_medium.as_deref().and_then(channel_of))
        .unwrap_or("link");
    Ok(EntrySource {
        channel: channel.into(),
        utm_source,
        utm_medium,
        utm_campaign: clean(&q.utm_campaign, "utm_campaign")?,
        utm_content: clean(&q.utm_content, "utm_content")?,
        utm_term: clean(&q.utm_term, "utm_term")?,
        click_ids,
        landing: "tax_request_start".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn q(src: &str, utm: &str) -> EntryQuery {
        EntryQuery { source: (!src.is_empty()).then(|| src.into()), utm_source: (!utm.is_empty()).then(|| utm.into()), ..Default::default() }
    }
    #[test]
    fn every_channel_classifies() {
        for (s, c) in [("google", "google"), ("Facebook", "facebook"), ("ig", "instagram"), ("linkedin", "linkedin"), ("twitter", "x"), ("x", "x"), ("ads", "ads"), ("", "link"), ("newsletter", "link")] {
            assert_eq!(classify(&q(s, "")).unwrap().channel, c, "{s}");
            assert_eq!(classify(&q("", s)).unwrap().channel, c, "utm {s}");
        }
        let g = classify(&EntryQuery { gclid: Some("abc123".into()), utm_campaign: Some("T1 2025".into()), ..Default::default() }).unwrap();
        assert_eq!((g.channel.as_str(), g.click_ids.clone(), g.utm_campaign.as_deref()), ("google", vec!["gclid".to_string()], Some("t1 2025")));
        assert!(classify(&q("<script>", "")).is_err());
        assert!(CHANNELS.contains(&classify(&q("", "")).unwrap().channel.as_str()));
    }
}
