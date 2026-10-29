//! T5013 partnership CCA (draft workpaper): the partnership, not the partners, claims CCA on the
//! depreciable property it owns (CRA T4068, "Capital cost allowance (CCA) – Partnership"). The
//! Schedule 8 computation reuses the T2 Schedule 8 engine (same Income Tax Regulations Part XI),
//! and the line 250 total is allocated to partners for T5013 slip box 040 by the shares the
//! preparer enters from the partnership agreement. Nothing is filed.
use super::t2_schedules::{self, CcaClass, T2Input};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SOURCES: &[(&str, &str)] = &[
    ("CRA T4068 Guide for the Partnership Information Return: T5013 SCH 8 (line 250), slip box 040 (partner's share of line 250 CCA, excluding terminal losses), box 220 (rental or leasing CCA)", "https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/t4068/guide-partnership-information-return-t5013-forms.html"),
    ("CRA, Capital cost allowance (CCA) – Partnership: the partnership claims CCA; partners cannot; fiscal periods under 12 months are prorated", "https://www.canada.ca/en/revenue-agency/services/tax/businesses/topics/sole-proprietorships-partnerships/t5013-partnership-information-return-filing-requirements/capital-cost-allowance-partnership.html"),
    ("CRA T5013 SCH 8, Capital Cost Allowance (2025 and later tax years)", "https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/t5013sch8.html"),
    ("Income Tax Regulations (C.R.C., c. 945) Part XI (ss. 1100–1105), incl. 1100(11)–(14) rental property restriction and 1104(3.1) eligible person or partnership", "https://laws-lois.justice.gc.ca/eng/regulations/C.R.C.,_c._945/FullText.html"),
];

pub const SCOPE: &str = "Draft T5013 Schedule 8 workpaper for a partnership's own depreciable property: the T2 Schedule 8 classes, factors, short fiscal periods and separate classes apply; line 250 is allocated to partners for slip box 040 by preparer-entered shares (basis points summing to 100%, cents by largest remainder). Immediate expensing for partnerships and the Reg 1100(11)–(14) rental property restriction are not modelled and block. Not CRA-certified; nothing is filed.";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Partner {
    pub partner_id: String,
    pub name: String,
    /// Share of the partnership's CCA under the partnership agreement, in basis points (100% = 10,000).
    pub share_bp: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct T5013CcaInput {
    pub fiscal_period_start: String,
    pub fiscal_period_end: String,
    pub cca: Vec<CcaClass>,
    pub partners: Vec<Partner>,
    /// Does any class hold rental or leasing property (Reg 1100(11)–(14), (15)–(20))? Required.
    pub rental_or_leasing_property: Option<bool>,
}

fn diag(out: &mut Vec<Value>, code: &str, msg: impl Into<String>, blocking: bool) {
    out.push(json!({"code":code,"severity":if blocking {"error"} else {"warning"},"message":msg.into(),"blocks_review":blocking}));
}

/// Splits `total` by basis-point shares: floor each, then hand the remaining cents to the largest
/// remainders (earlier partner first on ties). Shares must sum to 10,000.
pub fn allocate(total: i64, shares: &[u32]) -> Vec<i64> {
    let t = i128::from(total);
    let mut out: Vec<i64> = shares.iter().map(|s| (t * i128::from(*s)).div_euclid(10_000) as i64).collect();
    let mut left = total - out.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..shares.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse((t * i128::from(shares[i])).rem_euclid(10_000)), i));
    for &i in order.iter().cycle().take(shares.len() * 2) { if left == 0 { break; } out[i] += 1; left -= 1; }
    out
}

pub fn build(input: &T5013CcaInput) -> Result<Value, String> {
    if input.partners.is_empty() || input.partners.len() > 500 { return Err("enter 1 to 500 partners".into()); }
    if input.partners.iter().any(|p| p.partner_id.trim().is_empty() || p.partner_id.len() > 64 || p.name.len() > 200 || p.share_bp > 10_000) { return Err("each partner needs partner_id (≤ 64 chars), name ≤ 200 chars and share_bp 0..10000".into()); }
    let mut ids: Vec<&str> = input.partners.iter().map(|p| p.partner_id.as_str()).collect(); ids.sort(); ids.dedup();
    if ids.len() != input.partners.len() { return Err("partner_id must be unique".into()); }
    let mut diagnostics = vec![];
    let mut cca = input.cca.clone();
    if cca.iter().any(|c| c.immediate_expensing_cents != 0 || c.diep_cents != 0) {
        diag(&mut diagnostics, "t5013_immediate_expensing", "Immediate expensing for a partnership (eligible partnership under Reg 1104(3.1)(c): all members CCPCs or resident individuals throughout the period; 1100(0.1)(c) income limit, T5013 SCH 8 Parts 1–2) is not modelled; removed from the computation.", true);
        for c in &mut cca { c.immediate_expensing_cents = 0; c.diep_cents = 0; }
    }
    let t2 = T2Input { cca, corporation_type: "partnership".into(), tax_year_start: input.fiscal_period_start.clone(), tax_year_end: input.fiscal_period_end.clone(), ..Default::default() };
    t2_schedules::validate(&t2)?;
    match input.rental_or_leasing_property {
        None => diag(&mut diagnostics, "t5013_rental_property", "Set rental_or_leasing_property: CCA on rental or leasing property is restricted (Reg 1100(11)–(20)) and reported in box 220.", true),
        Some(true) => diag(&mut diagnostics, "t5013_rental_property", "The Reg 1100(11)–(20) rental and leasing property restrictions (and box 220) are not modelled.", true),
        Some(false) => {}
    }
    let s8 = t2_schedules::schedule8(&t2, &mut diagnostics);
    let line_250 = s8["schedule1_line_403_cca_cents"].as_i64().unwrap_or(0);
    let total_bp: u32 = input.partners.iter().map(|p| p.share_bp).sum();
    let allocations = if total_bp == 10_000 {
        let parts = allocate(line_250, &input.partners.iter().map(|p| p.share_bp).collect::<Vec<_>>());
        input.partners.iter().zip(parts).map(|(p, a)| json!({"partner_id":p.partner_id,"name":p.name,"share_bp":p.share_bp,"box_040_cca_cents":a})).collect::<Vec<_>>()
    } else {
        diag(&mut diagnostics, "t5013_partner_shares", format!("Partner shares total {total_bp} basis points; they must total 10,000 (100%)."), true);
        vec![]
    };
    let supported = !diagnostics.iter().any(|d| d["blocks_review"] == true);
    Ok(json!({"format":"nxt-t5013-cca/v1","scope":SCOPE,"supported":supported,"filed":false,
        "schedule_8":s8,"line_250_total_cca_cents":line_250,
        "terminal_loss_cents":s8["schedule1_line_404_terminal_loss_cents"],"recapture_cents":s8["schedule1_line_107_recapture_cents"],
        "allocations":allocations,"diagnostics":diagnostics,
        "sources":SOURCES.iter().chain(t2_schedules::SOURCES.iter()).map(|(t,u)| json!({"title":t,"url":u})).collect::<Vec<_>>()}))
}
