//! One-page T2 intake interview: incorporated? -> location -> reporting period -> income ->
//! expenses, ending in a full T2 or a nil T2 workpaper (or an honest out-of-scope result).
//! Also raises the overlapping-period, locked-tax-year, carry-forward and GT2-import dialogs.
//! Nothing here files. EFILE stays disabled; a human files outside NXT.
use super::{Tax, TResult, audit, bad, conflict, carryforward_inputs, models::*};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};

pub const PROVINCES: &[&str] = &["AB","BC","SK","MB","ON","NB","NS","PE","NL","YT","NT","NU"];
pub const EARLIEST_START: &str = "2023-01-01";

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntakeAnswers {
    #[serde(default)] pub incorporated: Option<bool>,
    /// Province/territory code, or "QC", "MULTI" (permanent establishment in more than one), "OUTSIDE".
    #[serde(default)] pub location: Option<String>,
    #[serde(default)] pub period_start: Option<String>,
    #[serde(default)] pub period_end: Option<String>,
    #[serde(default)] pub had_income: Option<bool>,
    #[serde(default)] pub other_expenses: Option<bool>,
    #[serde(default)] pub client_ref: String,
    /// Set when the interview is reopened from an existing T2 (its tax year is locked).
    #[serde(default)] pub editing_return_id: Option<String>,
}

fn date(s: &str) -> Option<NaiveDate> { NaiveDate::parse_from_str(s, "%Y-%m-%d").ok() }

/// Pure interview evaluation: the next question, field errors, and the outcome when complete.
pub fn evaluate(a: &IntakeAnswers, today: NaiveDate) -> Value {
    let mut errors = serde_json::Map::new();
    let out = |step: &str, errors: serde_json::Map<String, Value>, outcome: Value| json!({"next": step, "errors": errors, "outcome": outcome});
    let Some(inc) = a.incorporated else { return out("incorporated", errors, Value::Null) };
    if !inc {
        return out("done", errors, json!({"route":"t1_self_employment","label":"No T2 needed",
            "message":"An unincorporated business does not file a T2. Report business income on the owner's T1 (Form T2125). Start the T1 interview instead."}));
    }
    let Some(loc) = a.location.as_deref() else { return out("location", errors, Value::Null) };
    match loc {
        "QC" => return out("done", errors, json!({"route":"out_of_scope","label":"Quebec corporation","message":"Quebec corporations also file a CO-17 with Revenu Québec. This workpaper does not prepare Quebec returns; prepare it in the firm's certified software."})),
        "MULTI" => return out("done", errors, json!({"route":"out_of_scope","label":"Multiple jurisdictions","message":"Allocating taxable income across provinces (Schedule 5) is not supported. Prepare it in the firm's certified software."})),
        "OUTSIDE" => return out("done", errors, json!({"route":"out_of_scope","label":"Permanent establishment outside Canada","message":"Non-resident and foreign-establishment corporations are out of scope."})),
        l if PROVINCES.contains(&l) => {}
        _ => { errors.insert("location".into(), json!("Choose a province or territory")); return out("location", errors, Value::Null); }
    }
    let (Some(ps), Some(pe)) = (a.period_start.as_deref(), a.period_end.as_deref()) else { return out("period", errors, Value::Null) };
    let earliest = date(EARLIEST_START).unwrap();
    match (date(ps), date(pe)) {
        (None, _) => { errors.insert("period_start".into(), json!("Enter a valid start date (YYYY-MM-DD)")); }
        (_, None) => { errors.insert("period_end".into(), json!("Enter a valid end date (YYYY-MM-DD)")); }
        (Some(s), Some(e)) => {
            if s < earliest { errors.insert("period_start".into(), json!("The start date must be on or after January 1, 2023")); }
            if e > today { errors.insert("period_end".into(), json!("The end date must be today or earlier")); }
            if e < s { errors.insert("period_end".into(), json!("The end date must be after the start date")); }
            else if (e - s).num_days() > 370 { errors.insert("period_end".into(), json!("A tax year cannot be longer than 53 weeks")); }
        }
    }
    if !errors.is_empty() { return out("period", errors, Value::Null); }
    let Some(income) = a.had_income else { return out("income", errors, Value::Null) };
    let nil = if income { false } else {
        let Some(exp) = a.other_expenses else { return out("expenses", errors, Value::Null) };
        !exp
    };
    let engine = if loc == "AB" { "Federal and Alberta tax are calculated by the native T2 engine." } else { "GIFI, schedules and federal workpapers are prepared; the engine calculates provincial tax for AB only, so provincial tax blocks review until entered by the preparer." };
    let outcome = if nil {
        json!({"route":"nil_t2","label":"Nil T2 return","schedules":["T2 jacket","GIFI Schedule 100 (balance sheet)","GIFI Schedule 125 (income statement; professional fees, bank charges and office expenses only)","Schedule 50 (shareholders)"],
            "message":"No income and only basic expenses: prepare a nil T2. Financial statements are still required.","engine":engine})
    } else {
        json!({"route":"full_t2","label":"Full T2 return","schedules":["T2 jacket","GIFI Schedules 100/125/141","Schedule 1 (net income)","Schedule 8 (CCA)","Schedule 50 (shareholders)","Schedule 200 workpaper"],
            "message":"Income or other expenses: prepare a full T2 from the trial balance.","engine":engine})
    };
    out("done", errors, outcome)
}

fn overlaps(a: (&str, &str), b: (&str, &str)) -> bool { a.0 <= b.1 && b.0 <= a.1 }

impl Tax {
    /// Evaluate the interview and the dialogs that depend on existing workpapers.
    pub async fn t2_intake(&self, a: &IntakeAnswers, today: NaiveDate) -> TResult<Value> {
        super::text(&a.client_ref, "client reference", 120, false)?;
        let mut v = evaluate(a, today);
        let mut dialogs = Vec::new();
        if let (Some(ps), Some(pe)) = (a.period_start.as_deref(), a.period_end.as_deref()) {
            if let (Some(_), Some(e)) = (date(ps), date(pe)) {
                let s = self.load().await?;
                let year = e.format("%Y").to_string().parse::<i32>().unwrap_or(0);
                if let Some(id) = &a.editing_return_id {
                    if let Some(r) = s.returns.iter().find(|r| &r.id == id) {
                        if r.tax_year != year {
                            dialogs.push(json!({"kind":"locked_tax_year","blocking":true,"return_id":r.id,
                                "message":format!("This return is for tax year {}. The tax year of a started return can't change; start a new return for {year}.", r.tax_year)}));
                        }
                    }
                }
                if !a.client_ref.trim().is_empty() {
                    let mine: Vec<&TaxReturn> = s.returns.iter().filter(|r| r.program == "T2" && r.client_ref == a.client_ref && a.editing_return_id.as_deref() != Some(r.id.as_str())).collect();
                    for r in &mine {
                        if let (Some(rs), Some(re)) = (r.period_start.as_deref(), r.period_end.as_deref()) {
                            if overlaps((ps, pe), (rs, re)) {
                                dialogs.push(json!({"kind":"overlapping_period","blocking":true,"return_id":r.id,
                                    "message":format!("These dates overlap an existing T2 ({rs} to {re}). Adjust the dates or open that return.")}));
                            }
                        }
                    }
                    if let Some(prior) = mine.iter().filter(|r| r.tax_year == year - 1).max_by_key(|r| r.period_end.clone()) {
                        dialogs.push(json!({"kind":"carry_forward","blocking":false,"return_id":prior.id,"revision":prior.revision,
                            "prior_in_progress": !matches!(prior.status.as_str(), "approved" | "filed"),
                            "message":"A prior-year T2 exists for this corporation. Carry forward its profile, shareholders and closing balances?"}));
                    }
                }
            }
        }
        v["dialogs"] = json!(dialogs);
        v["gt2_import"] = json!({"supported":false,"message":"ProFile .GT2 files are a proprietary binary format and are not read. Export from the source software and use the DT Max / NXT import on the Import page."});
        v["filing"] = json!({"enabled":false,"banner":"Internal workpaper. Not CRA-certified. NETFILE disabled. A human files."});
        Ok(v)
    }

    /// Start the T2 workpaper the interview chose. Blocks on out-of-scope routes and blocking dialogs.
    pub async fn t2_intake_start(&self, a: &IntakeAnswers, name: &str, carry_forward_from: Option<&str>, today: NaiveDate) -> TResult<Value> {
        let v = self.t2_intake(a, today).await?;
        let route = v["outcome"]["route"].as_str().unwrap_or("");
        if route != "full_t2" && route != "nil_t2" { return Err(bad("the interview did not reach a T2 return")); }
        if v["dialogs"].as_array().is_some_and(|d| d.iter().any(|d| d["blocking"] == true)) { return Err(conflict("resolve the blocking intake dialog first")); }
        let pe = a.period_end.clone().unwrap_or_default();
        let year: i32 = pe.get(0..4).and_then(|y| y.parse().ok()).ok_or_else(|| bad("invalid period end"))?;
        let note = format!("Created by the T2 intake interview: {} ({}). Not CRA-certified; a human files.", v["outcome"]["label"].as_str().unwrap_or(""), a.location.as_deref().unwrap_or(""));
        let mut input: ReturnInput = serde_json::from_value(json!({"program":"T2","tax_year":year,"jurisdiction":a.location,"name":name,
            "client_ref":a.client_ref,"period_start":a.period_start,"period_end":a.period_end,"inputs":{},"notes":note,"expected_revision":null})).map_err(|e| bad(e.to_string()))?;
        if let Some(prior_id) = carry_forward_from {
            let ok = v["dialogs"].as_array().is_some_and(|d| d.iter().any(|d| d["kind"] == "carry_forward" && d["return_id"] == prior_id));
            if !ok { return Err(bad("carry forward is only offered from the prior-year T2 for this corporation")); }
            let prior = self.get(prior_id).await?;
            input.inputs = carryforward_inputs(&prior.inputs);
            input.parties = prior.parties.clone();
            for p in &mut input.parties { p.id = super::id("party"); p.allocation_cents = 0; }
            if let Some(mut profile) = prior.corporate.clone() { profile.gifi.clear(); profile.adjustments.clear(); profile.reconciliation_confirmed = false; profile.notes.clear(); input.corporate = Some(profile); }
        }
        let created = self.save(None, input).await?;
        let (cid, route_s, cf) = (created.id.clone(), route.to_string(), carry_forward_from.map(str::to_owned));
        self.mutate(|s| {
            if let (Some(p), Some(r)) = (cf.clone(), s.returns.iter_mut().find(|r| r.id == cid)) { r.rollforward_from = Some(p); }
            audit(s, &cid, "t2_intake", format!("T2 intake route {route_s}{}. Nothing was filed.", if cf.is_some() { "; prior-year profile carried forward" } else { "" }));
            Ok(())
        }).await?;
        Ok(json!({"return": self.get(&cid).await?, "outcome": v["outcome"], "filing_enabled": false}))
    }
}
