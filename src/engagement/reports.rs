//! Engagement report/letter templates, PDF export and handoff payloads for the
//! native e-sign (src/signatures) and tax modules. Pure, no I/O; nothing is
//! sent, saved to another module, signed or filed from here.
use super::{models::EngagementInput, statements};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const TEMPLATE_NOTICE: &str = "[CPA REVIEW REQUIRED] Original NXT draft wording. It has not been reviewed by a CPA and is not represented as conforming to CSRS 4200, CSRE 2400, CAS or any CPA Canada standard.";
pub const TEMPLATES: [&str; 2] = ["compilation_report", "engagement_letter"];

fn standard(kind: &str) -> &'static str {
    match kind {
        "compilation" => "compilation engagement (CSRS 4200)",
        "review" => "review engagement (CSRE 2400)",
        "audit" => "audit engagement (Canadian Auditing Standards)",
        _ => "internal engagement",
    }
}

/// Text lines for a named template, filled from the engagement file.
pub fn template(input: &EngagementInput, name: &str) -> Result<Vec<String>, String> {
    let entity = if input.name.trim().is_empty() { "[Entity name]" } else { input.name.trim() };
    let end = &input.period_end;
    let mut out = vec![TEMPLATE_NOTICE.to_string(), String::new()];
    match name {
        "compilation_report" => {
            if input.kind != "compilation" {
                return Err("A compilation engagement report applies to compilation engagements only".into());
            }
            out.extend([
                "COMPILATION ENGAGEMENT REPORT".into(),
                String::new(),
                format!("To management of {entity}"),
                String::new(),
                format!("On the basis of information provided by management, we have compiled the balance sheet of {entity} as at {end}, the statements of income and retained earnings for the year then ended, and Note 2, which describes the basis of accounting applied in their preparation (\"financial information\")."),
                String::new(),
                "Management is responsible for the accuracy and completeness of the underlying information used to compile the financial information and for the selection of the basis of accounting.".into(),
                String::new(),
                "We performed this engagement in accordance with [standard reference - CPA to confirm], which requires us to comply with relevant ethical requirements. Our responsibility is to assist management in the preparation of the financial information.".into(),
                String::new(),
                "We did not perform an audit engagement or a review engagement, nor were we required to perform procedures to verify the accuracy or completeness of the information provided by management. Accordingly, we do not express an audit opinion or a review conclusion, or provide any form of assurance on the financial information.".into(),
                String::new(),
                "Readers are cautioned that the financial information may not be appropriate for their purposes.".into(),
                String::new(),
                "NXT Accounting  [City, Province]  [Report date]".into(),
            ]);
        }
        "engagement_letter" => {
            out.extend([
                "ENGAGEMENT LETTER".into(),
                String::new(),
                format!("To the directors and management of {entity}"),
                String::new(),
                format!("This letter confirms our understanding of the terms of our {} for the year ending {end}.", standard(&input.kind)),
                String::new(),
                "Objective and scope: [CPA to complete for the engagement type].".into(),
                "Management responsibilities: providing complete and accurate records, selecting the basis of accounting, and access to information and personnel.".into(),
                "Our responsibilities: [CPA to complete].".into(),
                "Limitations: [CPA to complete, including the assurance (if any) provided].".into(),
                "Fees and billing: [to be completed].".into(),
                "Deliverables: draft financial statements and the report described above.".into(),
                String::new(),
                "Please sign below to confirm agreement with these terms.".into(),
                String::new(),
                "For NXT Accounting: ______________________   Date: __________".into(),
                format!("For {entity}: ______________________   Date: __________"),
            ]);
        }
        _ => return Err("Unknown template".into()),
    }
    Ok(out)
}

fn money(cents: i64) -> String {
    let neg = cents < 0;
    let a = cents.unsigned_abs();
    let whole = (a / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 { grouped.push(','); }
        grouped.push(c);
    }
    let s = format!("{grouped}.{:02}", a % 100);
    if neg { format!("({s})") } else { s }
}

/// Printable lines for the draft statements.
pub fn statement_lines(input: &EngagementInput) -> Vec<String> {
    let s = statements::generate(input);
    let mut out = vec![statements::REVIEW_NOTICE.to_string(), String::new(), format!("{} - draft financial statements", input.name), format!("Year ended {}", input.period_end), String::new()];
    let has_prior = !s["prior"].is_null();
    let row = |label: &str, sect: &str, key: &str| {
        let cur = s["current"][sect][key].as_i64().unwrap_or(0);
        let mut l = format!("{label:<44}{:>18}", money(cur));
        if has_prior { l.push_str(&format!("{:>18}", money(s["prior"][sect][key].as_i64().unwrap_or(0)))); }
        l
    };
    let head = |t: &str| { let mut l = format!("{t:<44}{:>18}", input.period_end); if has_prior { l.push_str(&format!("{:>18}", input.prior_period_end.clone().unwrap_or_default())); } l };
    out.push(head("BALANCE SHEET"));
    for (l, k) in [("Cash","cash_cents"),("Other current assets","other_current_assets_cents"),("Total current assets","current_assets_cents"),("Long-term assets","long_term_assets_cents"),("TOTAL ASSETS","total_assets_cents"),("Current liabilities","current_liabilities_cents"),("Long-term liabilities","long_term_liabilities_cents"),("Total liabilities","total_liabilities_cents"),("Share capital","share_capital_cents"),("Other equity","other_equity_cents"),("Retained earnings","retained_earnings_cents"),("Total equity","total_equity_cents")] { out.push(row(l, "balance_sheet", k)); }
    out.push(String::new());
    out.push(head("STATEMENT OF INCOME"));
    for (l, k) in [("Revenue","revenue_cents"),("Cost of sales","cost_of_sales_cents"),("Gross profit","gross_profit_cents"),("Operating expenses","operating_expenses_cents"),("Income before income taxes","income_before_taxes_cents"),("Income taxes","income_taxes_cents"),("NET INCOME","net_income_cents")] { out.push(row(l, "income_statement", k)); }
    out.push(String::new());
    out.push(head("STATEMENT OF RETAINED EARNINGS"));
    for (l, k) in [("Opening","opening_cents"),("Net income","net_income_cents"),("Dividends","dividends_cents"),("Closing","closing_cents")] { out.push(row(l, "retained_earnings", k)); }
    if !s["cash_flow"].is_null() {
        out.push(String::new());
        out.push("STATEMENT OF CASH FLOWS (indirect)".into());
        for (l, k) in [("Net income","net_income_cents"),("Change in non-cash working capital","change_in_non_cash_working_capital_cents"),("Operating activities","operating_cents"),("Investing activities","investing_cents"),("Financing activities","financing_cents"),("Net change in cash","net_change_in_cash_cents")] {
            out.push(format!("{l:<44}{:>18}", money(s["cash_flow"][k].as_i64().unwrap_or(0))));
        }
    }
    out.push(String::new());
    out.push("NOTES".into());
    for n in s["notes"].as_array().into_iter().flatten() {
        out.push(format!("{}. {}", n["number"], n["title"].as_str().unwrap_or("")));
        out.extend(wrap(n["text"].as_str().unwrap_or(""), 95));
    }
    if s["supported"] != true {
        out.push(String::new());
        out.push("DIAGNOSTICS (resolve before issue):".into());
        for d in s["diagnostics"].as_array().into_iter().flatten() { out.extend(wrap(&format!("- {}", d["message"].as_str().unwrap_or("")), 95)); }
    }
    out
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for w in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + w.len() > width { out.push(std::mem::take(&mut line)); }
        if !line.is_empty() { line.push(' '); }
        line.push_str(w);
    }
    if !line.is_empty() || out.is_empty() { out.push(line); }
    out
}

/// Minimal dependency-free PDF (Letter, Courier 9pt). Non-ASCII becomes '?'.
pub fn pdf(lines: &[String]) -> Vec<u8> {
    let wrapped: Vec<String> = lines.iter().flat_map(|l| if l.len() > 100 { wrap(l, 100) } else { vec![l.clone()] }).collect();
    let pages: Vec<&[String]> = if wrapped.is_empty() { vec![&[][..]] } else { wrapped.chunks(64).collect() };
    let mut objs: Vec<String> = Vec::new();
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 4 + i * 2).collect();
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", page_ids.iter().map(|i| format!("{i} 0 R")).collect::<Vec<_>>().join(" "), pages.len()));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into());
    for (i, page) in pages.iter().enumerate() {
        let mut stream = String::from("BT /F1 9 Tf 11 TL 40 752 Td\n");
        for l in page.iter() {
            let esc: String = l.chars().map(|c| match c { '(' => "\\(".into(), ')' => "\\)".into(), '\\' => "\\\\".into(), c if c.is_ascii() && !c.is_ascii_control() => c.to_string(), _ => "?".into() }).collect();
            stream.push_str(&format!("({esc}) Tj T*\n"));
        }
        stream.push_str(&format!("ET\nBT /F1 8 Tf 40 30 Td (Page {} of {} - DRAFT for CPA review) Tj ET\n", i + 1, pages.len()));
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", page_ids[i] + 1));
        objs.push(format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()));
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets { out.extend(format!("{o:010} 00000 n \n").as_bytes()); }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// Engagement-letter payload shaped as a `signatures::models::Draft`, for the
/// native e-sign module to load. It is returned only; nothing is created or sent.
pub fn esign_handoff(input: &EngagementInput, record_id: &str) -> Result<Value, String> {
    let lines = template(input, "engagement_letter")?;
    let pages: Vec<String> = lines.chunks(40).map(|c| c.join("\n")).collect();
    Ok(json!({
        "format":"nxt-engagement-esign-handoff/v1","submitted":false,
        "note":"Recipients, emails and fields are left for the e-sign module and the CPA to complete. Letter wording requires CPA review before sending.",
        "draft":{"title":format!("Engagement letter - {} - {}", input.name, input.period_end),"email_subject":"","email_message":"",
            "context":{"module":"engagements","record_id":record_id,"label":input.name},
            "documents":[{"id":"engagement-letter","name":"Engagement letter","pages":pages}],
            "recipients":[],"fields":[]}
    }))
}

/// GIFI CSV (code,current,prior,description; dollars, negatives in brackets)
/// in the input format of scripts/import-gifi-csv.py (tax module, PR #42).
/// Detail lines only; GIFI total lines are left to the tax module.
pub fn gifi_csv(input: &EngagementInput) -> String {
    let s = statements::generate(input);
    let mut by: BTreeMap<String, (i64, i64, String)> = BTreeMap::new();
    for l in s["leadsheets"].as_array().into_iter().flatten() {
        let Some(code) = l["gifi_code"].as_str().filter(|c| !c.is_empty()) else { continue };
        let e = by.entry(code.to_string()).or_insert((0, 0, l["name"].as_str().unwrap_or("").to_string()));
        e.0 += l["final_cents"].as_i64().unwrap_or(0);
        e.1 += l["prior_cents"].as_i64().unwrap_or(0);
    }
    let mut out = String::from("code,current,prior,description\n");
    for (code, (c, p, d)) in by {
        let d = d.replace(['"', '\n', '\r'], " ");
        out.push_str(&format!("{code},\"{}\",\"{}\",\"{d}\"\n", money(c), money(p)));
    }
    out
}
