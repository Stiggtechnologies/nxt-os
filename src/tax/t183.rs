//! T183 / T183CORP authorization drafts, produced as a native `src/signatures` Draft.
//! The operator posts the draft to /api/signatures (which is itself preparation-only). Nothing is sent.
//! Wording is an NXT summary, not the CRA form; legal/tax review is required before client use.
use super::models::TaxReturn;
use crate::signatures::models::{ContextLink, Document, Draft};

pub fn form_for(program: &str) -> Option<&'static str> {
    match program.to_ascii_uppercase().as_str() { "T1" => Some("T183"), "T2" => Some("T183CORP"), _ => None }
}

pub fn draft(r: &TaxReturn) -> Result<Draft, String> {
    let form = form_for(&r.program).ok_or("T183/T183CORP applies to T1 and T2 returns only")?;
    if r.status != "approved" { return Err("approve the return before preparing the T183 authorization".into()); }
    let calc = r.calculation.as_ref().ok_or("approved return has no calculation snapshot")?;
    let lines = calc.result["lines"].as_array().cloned().unwrap_or_default();
    let mut summary: Vec<String> = lines.iter().take(40).map(|l| format!("{}  {}  {}", l["code"].as_str().unwrap_or(""), l["label"].as_str().unwrap_or(""), l["amount"].as_i64().map(|c| format!("{}.{:02}", c / 100, (c % 100).abs())).unwrap_or_default())).collect();
    if summary.is_empty() { summary.push("See the attached NXT printable workpaper for line amounts.".into()); }
    let page1 = format!("{form} — Information return for electronic filing (NXT preparation draft)\n\nReturn: {} · tax year {} · {}\nCalculation snapshot: {}\nOutput hash: {}\n\nThe taxpayer authorizes the preparer to file this return electronically and confirms the information is correct and complete. This is an NXT draft of the authorization and must be checked against the current CRA {form} before use.", r.name, r.tax_year, r.jurisdiction, calc.id, calc.output_hash);
    Ok(Draft {
        title: format!("{form} authorization — {} {}", r.name, r.tax_year),
        email_subject: format!("{form} authorization for tax year {}", r.tax_year),
        email_message: "Please review and sign the attached authorization.".into(),
        context: Some(ContextLink { module: "tax".into(), record_id: r.id.clone(), label: format!("{form} {}", r.tax_year) }),
        documents: vec![Document { id: "doc-t183".into(), name: format!("{form}.txt"), pages: vec![page1, format!("Summary of return lines\n\n{}", summary.join("\n"))] }],
        recipients: vec![], fields: vec![],
    })
}
