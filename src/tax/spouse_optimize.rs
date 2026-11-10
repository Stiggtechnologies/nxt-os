//! Spouse/partner optimiser for a linked pair of T1 workpapers.
//! PROPOSALS ONLY: nothing here writes to either return. Each proposal carries the
//! combined-tax delta the native engine computes for the change, or `null` where the
//! engine does not calculate the item. Accepting a proposal records an audit event; a
//! preparer enters the amounts on both returns by hand.
use super::{Tax, TResult, audit, bad, conflict, missing, calculations};
use serde_json::{Value, json};

/// Federal maximum transfer of current-year tuition to a spouse (Schedule 11 line 32400 / Schedule 2).
pub const SCH2_TUITION_TRANSFER_MAX_CENTS: i64 = 500_000;
pub const PROPOSAL_KINDS: &[&str] = &["pension_split_t1032", "pool_donations", "pool_medical", "schedule2_tuition_transfer"];

pub struct Side<'a> { pub id: &'a str, pub year: i32, pub jurisdiction: &'a str, pub inputs: &'a Value }

fn cents(v: &Value, k: &str) -> i64 { v.get(k).and_then(Value::as_i64).unwrap_or(0) }
fn with(v: &Value, edits: &[(&str, i64)]) -> Value {
    let mut out = v.clone();
    if let Some(o) = out.as_object_mut() { for (k, c) in edits { o.insert((*k).into(), json!(c)); } }
    out
}
/// Total income tax the engine computes for these inputs (None when the engine rejects them).
pub fn income_tax(s: &Side, inputs: &Value) -> Option<i64> {
    calculations::calculate("T1", s.year, s.jurisdiction, inputs).ok().and_then(|c| {
        if !c.supported || c.diagnostics.iter().any(|d|d.blocks_review || d.severity=="error") {return None;}
        c.totals.get("income_tax").copied()
    })
}
fn pair_delta(a: &Side, ai: &Value, b: &Side, bi: &Value) -> Option<i64> {
    let base = income_tax(a, a.inputs)? + income_tax(b, b.inputs)?;
    Some(income_tax(a, ai)? + income_tax(b, bi)? - base)
}

#[cfg(test)]mod supported_proposal_tests {
    use super::*;
    #[test]fn partial_totals_are_not_supported_savings(){
        let a=json!({"scope_confirmed":true,"full_year_resident":true,"pension_income_cents":6_000_000});
        let b=json!({"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":1_000_000});
        let sa=Side{id:"A",year:2025,jurisdiction:"AB",inputs:&a};let sb=Side{id:"B",year:2025,jurisdiction:"AB",inputs:&b};
        let mut unverified=a.clone();unverified["pension_split_out_cents"]=json!(1_000_000);
        let c=calculations::calculate("T1",2025,"AB",&unverified).unwrap();assert!(c.totals.contains_key("income_tax"));assert!(!c.supported);assert!(c.diagnostics.iter().any(|d|d.blocks_review));assert_eq!(income_tax(&sa,&unverified),None);
        assert!(propose(&sa,&sb)["proposals"].as_array().unwrap().iter().all(|p|p["calculated"]!=true));
        let mut unsupported=b.clone();unsupported["scope_confirmed"]=json!(false);assert_eq!(income_tax(&sb,&unsupported),None);
    }
}

/// Compute proposals for a couple. `a` and `b` are the two T1s; nothing is mutated.
pub fn propose(a: &Side, b: &Side) -> Value {
    let mut proposals = Vec::new();
    // 1. Pension income splitting (T1032): try 5% steps up to the 50% cap in each direction.
    for (from, to, from_is_a) in [(a, b, true), (b, a, false)] {
        let pen = cents(from.inputs, "pension_income_cents");
        if pen <= 0 || cents(from.inputs, "pension_split_out_cents") > 0 || cents(to.inputs, "pension_split_in_cents") > 0 { continue; }
        let mut best: Option<(i64, i64)> = None;
        for step in 1..=10i64 {
            let amount = pen * step / 20;
            let fi = with(from.inputs, &[("pension_split_out_cents", amount)]);
            let ti = with(to.inputs, &[("pension_split_in_cents", amount)]);
            let d = if from_is_a { pair_delta(a, &fi, b, &ti) } else { pair_delta(a, &ti, b, &fi) };
            if let Some(d) = d { if d < 0 && best.map_or(true, |(_, bd)| d < bd) { best = Some((amount, d)); } }
        }
        if let Some((amount, d)) = best {
            proposals.push(json!({"kind":"pension_split_t1032","from_return_id":from.id,"to_return_id":to.id,"amount_cents":amount,
                "tax_delta_cents":d,"calculated":true,
                "detail":"Elect to split eligible pension income on Form T1032 (line 21000 on the transferor, line 11600 on the receiver). Both spouses sign; the transferor's eligible pension income and the same amount must appear on both returns.",
                "requires":["t1032_verified"]}));
        }
    }
    // 2/3. Pool donations or medical expenses on one return.
    for (key, kind, label) in [("donations_cents", "pool_donations", "donations"), ("medical_expenses_cents", "pool_medical", "medical expenses")] {
        let (da, db) = (cents(a.inputs, key), cents(b.inputs, key));
        if da + db <= 0 { continue; }
        let mut best: Option<(&str, i64)> = None;
        for target_a in [true, false] {
            let (ai, bi) = if target_a { (with(a.inputs, &[(key, da + db)]), with(b.inputs, &[(key, 0)])) } else { (with(a.inputs, &[(key, 0)]), with(b.inputs, &[(key, da + db)])) };
            if let Some(d) = pair_delta(a, &ai, b, &bi) { if d < 0 && best.map_or(true, |(_, bd)| d < bd) { best = Some((if target_a { a.id } else { b.id }, d)); } }
        }
        if let Some((target, d)) = best {
            proposals.push(json!({"kind":kind,"to_return_id":target,"amount_cents":da + db,"tax_delta_cents":d,"calculated":true,
                "detail":format!("Claim the couple's combined {label} on one return. Receipts must be for either spouse; medical uses one 12-month period."),
                "requires":["credits_verified"]}));
        }
    }
    // 4. Tuition transfer to a spouse (Schedule 2): not calculated by the engine.
    for (from, to) in [(a, b), (b, a)] {
        let t = cents(from.inputs, "tuition_cents");
        if t > 0 {
            proposals.push(json!({"kind":"schedule2_tuition_transfer","from_return_id":from.id,"to_return_id":to.id,
                "amount_cents":t.min(SCH2_TUITION_TRANSFER_MAX_CENTS),"tax_delta_cents":Value::Null,"calculated":false,
                "detail":"Possible transfer of unused current-year tuition (Schedule 11 line 32400 / Schedule 2), capped at the federal $5,000 maximum less the amount the student needs. The engine does not calculate Schedule 2; the preparer must compute the unused amount."}));
        }
    }
    json!({"applied":false,"write_back":false,"filing_enabled":false,"proposals":proposals,
        "not_evaluated":["Age, pension, disability and spouse amounts transferred on Schedule 2","Childcare expenses (lower-income spouse)","Provincial transfer schedules","Carryforwards"],
        "note":"Proposals only. Nothing was changed on either return. A preparer reviews and enters each accepted item."})
}

impl Tax {
    async fn proposals_for(&self, family_id: &str) -> TResult<(String, Value)> {
        let s = self.load().await?;
        let link = s.families.iter().find(|f| f.id == family_id).ok_or_else(|| bad("family link not found"))?;
        let a = s.returns.iter().find(|r| r.id == link.primary_return_id).ok_or_else(missing)?;
        let b = s.returns.iter().find(|r| r.id == link.spouse_return_id).ok_or_else(missing)?;
        let sa = Side { id: &a.id, year: a.tax_year, jurisdiction: &a.jurisdiction, inputs: &a.inputs };
        let sb = Side { id: &b.id, year: b.tax_year, jurisdiction: &b.jurisdiction, inputs: &b.inputs };
        let mut out = propose(&sa, &sb);
        out["family_id"] = json!(family_id);
        Ok((a.id.clone(), out))
    }
    /// The family link a T1 belongs to (either side), for the guided review page.
    pub async fn family_for_return(&self, return_id: &str) -> TResult<Value> {
        let s = self.load().await?;
        let link = s.families.iter().find(|f| f.primary_return_id == return_id || f.spouse_return_id == return_id).ok_or_else(|| bad("this return is not linked to a spouse"))?;
        super::family_view(&s, link)
    }
    /// Read-only spouse proposals for a linked family.
    pub async fn optimize_family(&self, family_id: &str) -> TResult<Value> { Ok(self.proposals_for(family_id).await?.1) }
    /// Record that a preparer accepted one proposal. Writes an audit event only; returns are unchanged.
    pub async fn accept_proposal(&self, family_id: &str, kind: &str) -> TResult<Value> {
        if !PROPOSAL_KINDS.contains(&kind) { return Err(bad("unknown proposal kind")); }
        let (primary, out) = self.proposals_for(family_id).await?;
        let p = out["proposals"].as_array().and_then(|ps| ps.iter().find(|p| p["kind"] == kind).cloned())
            .ok_or_else(|| conflict("that proposal no longer applies to these returns"))?;
        let detail = format!("Spouse proposal accepted for manual entry: {kind}, {} cents, delta {}. Neither return was changed; nothing was filed.", p["amount_cents"], p["tax_delta_cents"]);
        self.mutate(|s| { audit(s, &primary, "spouse_proposal_accepted", detail.clone()); Ok(()) }).await?;
        Ok(json!({"accepted":true,"applied":false,"write_back":false,"proposal":p,
            "next":"Enter the amounts on both returns and recalculate. NXT did not change either return."}))
    }
}
