//! Read-only voice navigation. A spoken name, code, or number opens one
//! screen and returns a short summary. Several close matches are spoken
//! back so the next turn can pick one. Nothing here writes.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceChoice {
    pub label: String,
    pub app: String,
    pub view: String,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct Record {
    pub app: String,
    pub view: String,
    pub label: String,
    pub summary: String,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OpenTarget {
    pub app: String,
    pub view: String,
}

pub fn score(query: &str, keys: &[String]) -> i32 {
    let query = norm(query);
    if query.is_empty() {
        return 0;
    }
    let mut best = 0;
    for key in keys {
        let key = norm(key);
        if key.is_empty() {
            continue;
        }
        let exact = if key == query { 100 } else { 0 };
        let contains = if query.len() >= 2 && (key.contains(&query) || query.contains(&key)) { 82 } else { 0 };
        let q_tokens: Vec<&str> = query.split(' ').filter(|t| t.len() > 1 || t.chars().all(|c| c.is_ascii_digit())).collect();
        let k_tokens: Vec<&str> = key.split(' ').collect();
        let overlap = q_tokens.iter().filter(|t| k_tokens.iter().any(|k| *k == **t || k.contains(*t))).count();
        let fuzzy = if q_tokens.is_empty() { 0 } else { (overlap as i32 * 70) / q_tokens.len() as i32 };
        best = best.max(exact).max(contains).max(fuzzy);
    }
    best
}

pub fn rank<'a>(query: &str, records: &'a [Record]) -> Vec<&'a Record> {
    let mut scored: Vec<_> = records.iter().map(|r| (score(query, &r.keys), r)).filter(|(s, _)| *s >= 55).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.label.cmp(&b.1.label)));
    if scored.is_empty() {
        return vec![];
    }
    let top = scored[0].0;
    if top >= 100 || scored.len() == 1 || top - scored.get(1).map(|s| s.0).unwrap_or(0) >= 15 {
        return vec![scored[0].1];
    }
    scored.into_iter().take(4).map(|(_, r)| r).collect()
}

pub fn choice(record: &Record) -> VoiceChoice {
    VoiceChoice { label: record.label.clone(), app: record.app.clone(), view: record.view.clone(), summary: record.summary.clone() }
}

pub fn pick_choice(heard: &str, choices: &[VoiceChoice]) -> Option<VoiceChoice> {
    let heard = norm(heard);
    let index = if heard.contains("first") || heard == "1" || heard.contains("number 1") {
        Some(0)
    } else if heard.contains("second") || heard == "2" || heard.contains("number 2") {
        Some(1)
    } else if heard.contains("third") || heard == "3" || heard.contains("number 3") {
        Some(2)
    } else if heard.contains("fourth") || heard == "4" {
        Some(3)
    } else {
        None
    };
    if let Some(i) = index {
        return choices.get(i).cloned();
    }
    let mut best: Option<(i32, &VoiceChoice)> = None;
    for choice in choices {
        let s = score(&heard, &[choice.label.clone()]);
        if s >= 70 && best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, choice));
        }
    }
    best.map(|(_, c)| c.clone())
}

pub fn module_target(heard: &str) -> Option<Record> {
    let heard = norm(heard);
    let modules = [
        ("ledger", "ledger", "dashboard", "NXT Ledger", "Invoices, bills, banking, and the journal."),
        ("firm", "firm", "overview", "Firm Operations", "Projects, workflows, and human approvals."),
        ("tax", "tax", "worklist", "NXT Tax", "Internal workpapers. Filing is off."),
        ("engagement", "engagements", "worklist", "NXT Engagements", "Working papers and internal review."),
        ("signature", "signatures", "home", "Prepare signatures", "Internal drafts. Nothing is sent or signed."),
        ("referral", "referrals", "overview", "NXT Referrals", "Advocates, rewards, and loyalty."),
        ("migration", "migrations", "worklist", "Parallel testing", "Read-only source snapshots."),
        ("parallel testing", "migrations", "worklist", "Parallel testing", "Read-only source snapshots."),
        ("company book", "migrations", "companies", "Company books", "Observed companies and source evidence. This does not post journals."),
        ("nixon", "nixon", "review", "Nixon", "The finance review. Voice cannot approve."),
    ];
    for (needle, app, view, label, summary) in modules {
        if heard.contains(needle) && (heard.contains("open") || heard.contains("show") || heard.contains("pull") || heard.contains("go to")) {
            return Some(Record { app: app.into(), view: view.into(), label: label.into(), summary: summary.into(), keys: vec![needle.into()] });
        }
    }
    None
}

fn norm(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(label: &str, keys: &[&str]) -> Record {
        Record { app: "ledger".into(), view: "registers/x".into(), label: label.into(), summary: label.into(), keys: keys.iter().map(|k| (*k).into()).collect() }
    }

    #[test]
    fn exact_code_beats_a_partial_name() {
        let records = vec![rec("Chequing", &["1000", "chequing"]), rec("Inventory", &["1200", "inventory"])];
        let hits = rank("1200", &records);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].label, "Inventory");
    }

    #[test]
    fn close_names_stay_ambiguous() {
        let records = vec![rec("Smith Studio", &["smith studio"]), rec("Smith Dental", &["smith dental"])];
        let hits = rank("smith", &records);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn ordinal_picks_a_spoken_choice() {
        let choices = vec![
            VoiceChoice { label: "Smith Studio".into(), app: "ledger".into(), view: "customers".into(), summary: "a".into() },
            VoiceChoice { label: "Smith Dental".into(), app: "ledger".into(), view: "customers".into(), summary: "b".into() },
        ];
        assert_eq!(pick_choice("the second one", &choices).unwrap().label, "Smith Dental");
    }
}
