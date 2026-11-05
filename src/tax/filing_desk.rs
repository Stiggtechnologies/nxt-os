//! Filing Desk: the internal, human-only last step before a return is filed outside NXT.
//!
//! Queue -> section-by-section walkthrough -> filer confirmation (with step-up re-auth) ->
//! append-only filing log. NXT is not CRA-certified, NETFILE stays disabled, and nothing here
//! transmits anything: the confirmation records "ready to submit", calls the existing filing
//! stub (which always refuses) and logs that refusal as the result. A human files.
//!
//! Guardrails enforced here (server-side, not only in the UI):
//! - only staff holding a filer grant (set by an admin, audited) can see the queue or file;
//! - two-person rule: the preparer who signed the return off can never file it;
//! - a confirmation is bound to a fingerprint of the canonical return data; any later change
//!   invalidates it and the return goes back to the queue;
//! - the filing log is append-only (no update or delete path in code or in SQL).
use super::{Tax, TResult, bad, conflict, hash, id, input_hash, missing, now, audit, models::*};
use crate::firm::staff::{Identity, Role, label_user_id};
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const BANNER: &str = "Internal workpaper. Not CRA-certified. NETFILE disabled. A human files.";
pub const SECTIONS: [&str; 4] = ["summary", "slips", "deductions", "final"];
pub const LOG_STREAM: &str = "tax-filing";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeskState {
    #[serde(default)] pub filers: Vec<FilerGrant>,
    #[serde(default)] pub fees: Vec<FeeFlag>,
    #[serde(default)] pub seen: Vec<SeenMark>,
    #[serde(default)] pub confirmations: Vec<DeskConfirmation>,
    #[serde(default)] pub launch: LaunchGate,
    /// Stripe Checkout sessions / refunds for T1 filing fees (pay-before-file).
    #[serde(default)] pub stripe_payments: Vec<super::stripe::StripePayment>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilerGrant { pub user_id: String, pub granted_by: String, pub at: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeeFlag {
    pub return_id: String, pub paid: bool, pub by: String, pub at: String,
    /// Checkout session that marked this fee paid (webhook); absent for manual staff flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stripe_session_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeenMark { pub return_id: String, pub section: String, pub fingerprint: String, pub by: String, pub at: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeskConfirmation {
    pub id: String, pub return_id: String, pub revision: u64, pub fingerprint: String,
    pub by: String, pub filer_id: String, pub at: String, pub status: String, pub result: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LaunchGate {
    /// Client launch of guided intake + Filing Desk. Default off; can only be turned on after a
    /// recorded end-to-end internal test run.
    #[serde(default)] pub client_launch_enabled: bool,
    #[serde(default)] pub test_runs: Vec<LaunchTestRun>,
    #[serde(default)] pub changed_by: Option<String>,
    #[serde(default)] pub changed_at: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchTestRun { pub id: String, pub return_id: String, pub confirmation_id: String, pub fingerprint: String, pub by: String, pub at: String }

#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct FeeInput { pub paid: bool }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct FilerInput { pub user_id: String, pub granted: bool }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct SeenInput { pub section: String, pub fingerprint: String }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct StepUpSmsInput { pub password: String }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct ConfirmInput {
    pub fingerprint: String,
    #[serde(default)] pub authorize: bool,
    pub password: String,
    #[serde(default)] pub sms_code: Option<String>,
    #[serde(default)] pub totp: Option<String>,
}
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct TestRunInput { pub return_id: String }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct LaunchInput { pub enabled: bool }

fn sees(who: &Identity, client_ref: &str) -> bool { who.all_clients() || who.can_client(client_ref) }
fn forbidden(m: &str) -> ApiError { ApiError(StatusCode::FORBIDDEN, m.into()) }

/// Fingerprint of the canonical return data a filer confirms: the full return input, the input
/// revision, the record revision, the review and preparer sign-offs, the calculation output
/// hash and the status. Any edit, recalculation, reopen or re-review changes it.
pub fn fingerprint(r: &TaxReturn) -> TResult<String> {
    hash(&json!({"format":"nxt-filing-fingerprint/v1","return_id":r.id,"input":ReturnInput::from(r),"input_revision":r.input_revision,"revision":r.revision,
        "review":r.review,"preparer_signoff":r.preparer_signoff,"output_hash":r.calculation.as_ref().map(|c| c.output_hash.clone()),"status":r.status}))
}

/// Statutory filing deadline used for sorting (T1: April 30 of the next year; T2: six months
/// after the period end).
pub fn deadline(r: &TaxReturn) -> String {
    if r.program == "T2" {
        if let Some(end) = r.period_end.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
            if let Some(d) = end.checked_add_months(chrono::Months::new(6)) { return d.to_string(); }
        }
        return format!("{}-06-30", r.tax_year + 1);
    }
    format!("{}-04-30", r.tax_year + 1)
}

fn fix_link(r: &TaxReturn, field: Option<&str>) -> String {
    let enc = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c.to_string() } else { format!("%{:02X}", c as u32) }).collect::<String>();
    match (r.program.as_str(), field) {
        ("T1", Some(f)) => format!("#tt-t1/{}?screen=review&field={}", enc(&r.id), enc(f)),
        ("T1", None) => format!("#tt-t1/{}?screen=review", enc(&r.id)),
        _ => format!("#return/{}", enc(&r.id)),
    }
}

/// Blocking items that stop the walkthrough, each with a Fix-it link. Mirrors the review gate
/// in `Tax::transition` and the guided review page.
pub fn blockers(r: &TaxReturn) -> Vec<Value> {
    let mut out = vec![];
    let mut push = |code: &str, message: String, field: Option<&str>| out.push(json!({"code":code,"message":message,"field":field,"fix":fix_link(r, field)}));
    if !["T1", "T2"].contains(&r.program.as_str()) { push("program", "the Filing Desk handles T1 and T2 returns only".into(), None); }
    if r.status != "approved" { push("not_approved", format!("the return is {}; it must pass review (approved) first", r.status), None); }
    if r.review.is_none() { push("no_review", "no reviewer approval is on file".into(), None); }
    match r.calculation.as_ref() {
        None => push("not_calculated", "calculate the current inputs first".into(), None),
        Some(c) => {
            let fresh = c.input_revision == r.input_revision && input_hash(r).map(|h| h == c.input_hash).unwrap_or(false) && hash(&c.result).map(|h| h == c.output_hash).unwrap_or(false);
            if !fresh { push("stale_calculation", "the calculation is stale; recalculate and send back to review".into(), None); }
            if c.result["supported"] != true { push("unsupported", "the engine does not support this return's situation".into(), None); }
            for d in c.result["diagnostics"].as_array().into_iter().flatten() {
                if d["blocks_review"] == true || d["severity"] == "error" {
                    push(d["code"].as_str().unwrap_or("diagnostic"), d["message"].as_str().unwrap_or("blocking review item").to_string(), d["field"].as_str());
                }
            }
        }
    }
    out
}

fn fee_paid(d: &DeskState, rid: &str) -> bool { d.fees.iter().find(|f| f.return_id == rid).is_some_and(|f| f.paid) }
fn is_filer(d: &DeskState, who: &Identity) -> bool { !who.break_glass && d.filers.iter().any(|g| g.user_id == who.user_id) }
fn valid_confirmation<'a>(d: &'a DeskState, r: &TaxReturn, fp: &str) -> Option<&'a DeskConfirmation> {
    d.confirmations.iter().rev().find(|c| c.return_id == r.id && c.fingerprint == fp && c.revision == r.revision)
}
fn preparer(r: &TaxReturn) -> String { r.preparer_signoff.as_ref().map(|s| s.by.clone()).unwrap_or_default() }

/// Two-person rule: refuses when the filer is the preparer, or the preparer label is not a
/// verified staff label (then nobody can prove the filer is someone else).
pub fn two_person_rule(r: &TaxReturn, who: &Identity) -> Result<(), String> {
    let p = preparer(r);
    match label_user_id(&p) {
        None => Err("the preparer sign-off is not a verified staff label, so the two-person rule cannot be checked".into()),
        Some(pid) if pid == who.user_id => Err("two-person rule: the preparer cannot file their own return".into()),
        Some(_) => Ok(()),
    }
}

fn totals(r: &TaxReturn) -> Value { r.calculation.as_ref().map(|c| c.result["totals"].clone()).unwrap_or(Value::Null) }
fn money(r: &TaxReturn) -> Value {
    match totals(r)["balance"].as_i64() {
        Some(b) if b < 0 => json!({"kind":"refund","cents":-b}),
        Some(b) if b > 0 => json!({"kind":"owing","cents":b}),
        Some(_) => json!({"kind":"even","cents":0}),
        None => json!({"kind":"pending","cents":0}),
    }
}
fn client_label(r: &TaxReturn) -> String { if r.client_ref.trim().is_empty() { r.name.clone() } else { format!("{} · {}", r.name, r.client_ref) } }

fn subset(v: &Value, pred: impl Fn(&str) -> bool) -> Value {
    Value::Object(v.as_object().map(|o| o.iter().filter(|(k, _)| pred(k)).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default())
}
fn is_slip_key(k: &str) -> bool { let k = k.to_ascii_lowercase(); ["slip", "income", "employment", "t4", "t5", "dividend", "interest", "revenue", "sales", "pension", "gain"].iter().any(|w| k.contains(w)) }
fn is_deduction_key(k: &str) -> bool { let k = k.to_ascii_lowercase(); ["deduct", "credit", "rrsp", "fhsa", "expense", "donation", "medical", "tuition", "childcare", "child_care", "moving", "union", "cca", "loss"].iter().any(|w| k.contains(w)) }

impl Tax {
    async fn desk_who(&self, who: &Identity) -> TResult<DeskState> {
        let s = self.load().await?;
        if !(is_filer(&s.desk, who) || (who.role == Role::Admin && !who.break_glass)) { return Err(forbidden("the Filing Desk is for staff with a filer grant")); }
        Ok(s.desk)
    }

    /// The caller's Filing Desk access. Any staff account may ask.
    pub async fn desk_me(&self, who: &Identity) -> TResult<Value> {
        let s = self.load().await?;
        Ok(json!({"user_id":who.staff_id(),"label":who.label(),"filer":is_filer(&s.desk, who),"admin":who.role==Role::Admin && !who.break_glass,
            "banner":BANNER,"netfile_enabled":false,"client_launch_enabled":s.desk.launch.client_launch_enabled}))
    }

    /// Morning queue: approved T1/T2 with a paid filing fee and no valid confirmation, by deadline.
    pub async fn desk_queue(&self, who: &Identity) -> TResult<Value> {
        self.desk_who(who).await?;
        let s = self.load().await?;
        let mut rows = vec![];
        for r in s.returns.iter().filter(|r| ["T1", "T2"].contains(&r.program.as_str()) && r.status == "approved" && fee_paid(&s.desk, &r.id) && sees(who, &r.client_ref)) {
            let fp = fingerprint(r)?;
            if valid_confirmation(&s.desk, r, &fp).is_some() { continue; }
            let invalidated = s.desk.confirmations.iter().any(|c| c.return_id == r.id);
            let b = blockers(r);
            let prep = preparer(r);
            let own = label_user_id(&prep).is_some() && label_user_id(&prep) == who.staff_id();
            rows.push(json!({"id":r.id,"client":client_label(r),"program":r.program,"tax_year":r.tax_year,"money":money(r),"preparer":preparer(r),
                "deadline":deadline(r),"revision":r.revision,"blockers":b.len(),"invalidated_confirmation":invalidated,
                "own_return":own}));
        }
        rows.sort_by(|a, b| a["deadline"].as_str().cmp(&b["deadline"].as_str()).then(a["client"].as_str().cmp(&b["client"].as_str())));
        Ok(json!({"banner":BANNER,"returns":rows,"netfile_enabled":false}))
    }

    /// Walkthrough data for one return: four sections, seen marks for the current fingerprint,
    /// blockers with Fix-it links, and whether this caller may file.
    pub async fn desk_detail(&self, who: &Identity, rid: &str) -> TResult<Value> {
        let desk = self.desk_who(who).await?;
        let r = self.get(rid).await?;
        let fp = fingerprint(&r)?;
        let seen: Vec<&str> = SECTIONS.iter().copied().filter(|sec| desk.seen.iter().any(|m| m.return_id == r.id && m.section == *sec && m.fingerprint == fp)).collect();
        let b = blockers(&r);
        let tp = two_person_rule(&r, who).err();
        let t = totals(&r);
        let sections = json!({
            "summary": {"client":client_label(&r),"program":r.program,"tax_year":r.tax_year,"jurisdiction":r.jurisdiction,"status":r.status,"revision":r.revision,
                        "preparer":preparer(&r),"reviewer":r.review.as_ref().map(|v| v.reviewer.clone()),"deadline":deadline(&r),"money":money(&r)},
            "slips": {"inputs":subset(&r.inputs, is_slip_key),"totals":subset(&t, is_slip_key)},
            "deductions": {"inputs":subset(&r.inputs, is_deduction_key),"totals":subset(&t, is_deduction_key)},
            "final": {"totals":t},
        });
        let confirmed = valid_confirmation(&desk, &r, &fp).cloned();
        Ok(json!({"banner":BANNER,"return_id":r.id,"fingerprint":fp,"revision":r.revision,"sections":sections,"order":SECTIONS,"seen":seen,
            "blockers":b,"fee_paid":fee_paid(&desk,&r.id),"filer":is_filer(&desk, who),"two_person_rule":tp,
            "can_file": is_filer(&desk, who) && tp.is_none() && b.is_empty() && fee_paid(&desk,&r.id) && seen.len()==SECTIONS.len() && confirmed.is_none(),
            "confirmation":confirmed,"netfile_enabled":false}))
    }

    /// Staff-settable "filing fee paid" flag, audited in the tax audit chain.
    pub async fn desk_fee(&self, who: &Identity, rid: &str, input: FeeInput) -> TResult<Value> {
        if who.break_glass { return Err(forbidden("sign in with your staff account to record a filing fee")); }
        let (by, at) = (who.label(), now());
        self.mutate(|s| {
            s.returns.iter().find(|r| r.id == rid).ok_or_else(missing)?;
            s.desk.fees.retain(|f| f.return_id != rid);
            s.desk.fees.push(FeeFlag { return_id: rid.into(), paid: input.paid, by: by.clone(), at: at.clone(), stripe_session_id: None });
            audit(s, rid, if input.paid { "filing_fee_paid" } else { "filing_fee_cleared" }, format!("by {by}"));
            Ok(json!({"return_id":rid,"fee_paid":input.paid,"by":by,"at":at}))
        }).await
    }

    pub async fn desk_filers(&self, who: &Identity) -> TResult<Value> {
        let s = self.load().await?;
        if who.role != Role::Admin || who.break_glass { return Err(forbidden("filer grants are managed by an admin staff account")); }
        Ok(json!({"filers":s.desk.filers}))
    }

    /// Admin-only filer grant (or revoke), audited.
    pub async fn desk_set_filer(&self, who: &Identity, input: FilerInput) -> TResult<Value> {
        if who.role != Role::Admin || who.break_glass { return Err(forbidden("filer grants are managed by an admin staff account")); }
        let uid = input.user_id.trim().to_string();
        if !uid.starts_with("usr_") || uid.len() > 64 { return Err(bad("user_id must be a staff user id")); }
        let (by, at) = (who.label(), now());
        self.mutate(|s| {
            s.desk.filers.retain(|g| g.user_id != uid);
            if input.granted { s.desk.filers.push(FilerGrant { user_id: uid.clone(), granted_by: by.clone(), at: at.clone() }); }
            audit(s, "", if input.granted { "filer_granted" } else { "filer_revoked" }, format!("{uid} by {by}"));
            Ok(json!({"filers":s.desk.filers}))
        }).await
    }

    /// Marks one walkthrough section seen for the current fingerprint, in order. Blocking
    /// review items stop the flow.
    pub async fn desk_seen(&self, who: &Identity, rid: &str, input: SeenInput) -> TResult<Value> {
        let (by, at) = (who.label(), now());
        let section = input.section.trim().to_string();
        let idx = SECTIONS.iter().position(|s| *s == section).ok_or_else(|| bad("unknown walkthrough section"))?;
        self.mutate(|s| {
            if !is_filer(&s.desk, who) { return Err(forbidden("only staff with a filer grant can work the Filing Desk")); }
            let r = s.returns.iter().find(|r| r.id == rid).ok_or_else(missing)?.clone();
            let fp = fingerprint(&r)?;
            if fp != input.fingerprint { return Err(conflict("the return changed since this page loaded; reload the walkthrough")); }
            let b = blockers(&r);
            if !b.is_empty() { return Err(conflict(format!("blocking review items stop the walkthrough: {}", b.iter().filter_map(|x| x["message"].as_str()).collect::<Vec<_>>().join("; ")))); }
            for prev in &SECTIONS[..idx] {
                if !s.desk.seen.iter().any(|m| m.return_id == rid && m.section == *prev && m.fingerprint == fp) { return Err(conflict(format!("mark \"{prev}\" seen first"))); }
            }
            if !s.desk.seen.iter().any(|m| m.return_id == rid && m.section == section && m.fingerprint == fp) {
                s.desk.seen.push(SeenMark { return_id: rid.into(), section: section.clone(), fingerprint: fp.clone(), by: by.clone(), at: at.clone() });
                audit(s, rid, "filing_desk_section_seen", format!("{section} by {by}"));
            }
            Ok(json!({"seen":SECTIONS.iter().filter(|sec| s.desk.seen.iter().any(|m| m.return_id == rid && m.section == **sec && m.fingerprint == fp)).collect::<Vec<_>>()}))
        }).await
    }

    pub async fn desk_step_up_sms(&self, who: &Identity, input: StepUpSmsInput) -> TResult<Value> {
        self.desk_who(who).await?;
        crate::firm::staff::registry().step_up_sms(who, &input.password).await
    }

    fn desk_ready(s: &TaxState, who: &Identity, rid: &str, fp_in: &str) -> TResult<TaxReturn> {
        if !is_filer(&s.desk, who) { return Err(forbidden("only staff with a filer grant can file")); }
        let r = s.returns.iter().find(|r| r.id == rid).ok_or_else(missing)?.clone();
        two_person_rule(&r, who).map_err(|m| forbidden(&m))?;
        let fp = fingerprint(&r)?;
        if fp != fp_in { return Err(conflict("fingerprint mismatch: the return changed after the walkthrough; review it again")); }
        let b = blockers(&r);
        if !b.is_empty() { return Err(conflict("blocking review items must be fixed before filing")); }
        if !fee_paid(&s.desk, rid) { return Err(conflict("the filing fee is not marked paid")); }
        for sec in SECTIONS { if !s.desk.seen.iter().any(|m| m.return_id == rid && m.section == sec && m.fingerprint == fp) { return Err(conflict(format!("mark every section seen first (missing \"{sec}\")"))); } }
        if valid_confirmation(&s.desk, &r, &fp).is_some() { return Err(conflict("this return version is already confirmed")); }
        Ok(r)
    }

    /// Filer confirmation: checks every guardrail, re-authenticates the filer, records the
    /// return as ready to submit, calls the filing stub (which always refuses; nothing goes to
    /// CRA) and appends the outcome to the append-only filing log.
    pub async fn desk_confirm(&self, who: &Identity, rid: &str, input: ConfirmInput) -> TResult<Value> {
        if who.break_glass { return Err(forbidden("the break-glass key cannot file; sign in with your staff account")); }
        if !input.authorize { return Err(bad("tick \"I reviewed this return and authorize filing\" first")); }
        Self::desk_ready(&self.load().await?, who, rid, &input.fingerprint)?;
        crate::firm::staff::registry().step_up(who, &input.password, input.sms_code.as_deref(), input.totp.as_deref()).await?;
        let refusal = self.filing_refusal(rid).await;
        let result = format!("refused: {}", refusal.1);
        let (by, at, cid) = (who.label(), now(), id("fdc"));
        let conf = self.mutate(|s| {
            let r = Self::desk_ready(s, who, rid, &input.fingerprint)?;
            let c = DeskConfirmation { id: cid.clone(), return_id: rid.into(), revision: r.revision, fingerprint: input.fingerprint.clone(), by: by.clone(),
                filer_id: who.user_id.clone(), at: at.clone(), status: "ready_to_submit".into(), result: result.clone() };
            s.desk.confirmations.push(c.clone());
            audit(s, rid, "filing_confirmed", format!("ready to submit by {by}; revision {}; fingerprint {}; filing stub {}", r.revision, input.fingerprint, result));
            Ok((c, r))
        }).await?;
        let (c, r) = conf;
        let prev = self.store.read_append_only(LOG_STREAM, 1).await.map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e))?;
        let prev_hash = prev.first().and_then(|e| e["hash"].as_str()).unwrap_or("genesis").to_string();
        let mut entry = json!({"format":"nxt-filing-log/v1","id":id("fl"),"confirmation_id":c.id,"return_id":rid,"program":r.program,"tax_year":r.tax_year,
            "client":client_label(&r),"revision":c.revision,"fingerprint":c.fingerprint,"confirmed_by":c.by,"filer_id":c.filer_id,"preparer":preparer(&r),
            "at":c.at,"day":c.at.get(..10).unwrap_or(""),"status":"ready_to_submit",
            "result":{"outcome":"refused","http_status":refusal.0.as_u16(),"message":refusal.1},"transmitted":false,"prev_hash":prev_hash});
        entry["hash"] = json!(hash(&entry)?);
        self.store.append_only(LOG_STREAM, &entry).await.map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e))?;
        Ok(json!({"confirmation":c,"log_entry":entry,"filing_result":"refused","transmitted":false,"banner":BANNER}))
    }

    /// Filing log, newest first, filterable by day (YYYY-MM-DD, UTC) and filer user id.
    pub async fn desk_log(&self, who: &Identity, day: Option<&str>, filer: Option<&str>) -> TResult<Value> {
        self.desk_who(who).await?;
        let s = self.load().await?;
        let all = self.store.read_append_only(LOG_STREAM, 5000).await.map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e))?;
        // Chain check, oldest to newest.
        let mut ok = true; let mut prev = "genesis".to_string();
        for e in all.iter().rev() {
            let mut body = e.clone(); let h = body.as_object_mut().and_then(|o| o.remove("hash")).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            if e["prev_hash"].as_str() != Some(prev.as_str()) || hash(&body).map(|x| x != h).unwrap_or(true) { ok = false; }
            prev = h;
        }
        let visible = |e: &Value| s.returns.iter().find(|r| Some(r.id.as_str()) == e["return_id"].as_str()).is_none_or(|r| sees(who, &r.client_ref));
        let rows: Vec<&Value> = all.iter().filter(|e| day.is_none_or(|d| d.is_empty() || e["day"].as_str() == Some(d)) && filer.is_none_or(|f| f.is_empty() || e["filer_id"].as_str() == Some(f)) && visible(e)).collect();
        Ok(json!({"entries":rows,"chain_verified":ok,"append_only":true,"banner":BANNER}))
    }

    pub async fn desk_launch(&self) -> TResult<Value> { Ok(json!(self.load().await?.desk.launch)) }

    /// Records an end-to-end internal test run: a synthetic (client_ref "TEST-…") return that
    /// went through intake, review and a Filing Desk confirmation that is still valid.
    pub async fn desk_record_test_run(&self, who: &Identity, input: TestRunInput) -> TResult<Value> {
        if who.role != Role::Admin || who.break_glass { return Err(forbidden("the client-launch gate is managed by an admin staff account")); }
        let log = self.store.read_append_only(LOG_STREAM, 5000).await.map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e))?;
        let (by, at) = (who.label(), now());
        self.mutate(|s| {
            let r = s.returns.iter().find(|r| r.id == input.return_id).ok_or_else(missing)?.clone();
            if !r.client_ref.to_ascii_uppercase().starts_with("TEST-") { return Err(conflict("the launch test run must use an internal synthetic return (client reference starting TEST-)")); }
            let fp = fingerprint(&r)?;
            let c = valid_confirmation(&s.desk, &r, &fp).cloned().ok_or_else(|| conflict("the test return has no valid Filing Desk confirmation"))?;
            if !log.iter().any(|e| e["confirmation_id"].as_str() == Some(c.id.as_str()) && e["result"]["outcome"] == "refused" && e["transmitted"] == false) { return Err(conflict("the filing log has no entry for this confirmation")); }
            let run = LaunchTestRun { id: id("flt"), return_id: r.id.clone(), confirmation_id: c.id.clone(), fingerprint: fp, by: by.clone(), at: at.clone() };
            s.desk.launch.test_runs.push(run.clone());
            audit(s, &r.id, "client_launch_test_run", format!("cleared end to end by {by}"));
            Ok(json!(run))
        }).await
    }

    /// Turns the client-launch flag on or off. On requires a recorded, still-valid test run.
    pub async fn desk_set_launch(&self, who: &Identity, input: LaunchInput) -> TResult<Value> {
        if who.role != Role::Admin || who.break_glass { return Err(forbidden("the client-launch gate is managed by an admin staff account")); }
        let (by, at) = (who.label(), now());
        self.mutate(|s| {
            if input.enabled {
                let cleared = s.desk.launch.test_runs.iter().any(|t| s.returns.iter().any(|r| r.id == t.return_id && fingerprint(r).map(|f| f == t.fingerprint).unwrap_or(false)));
                if !cleared { return Err(conflict("client launch stays off until an end-to-end internal test return has cleared intake and the Filing Desk (record a test run first)")); }
            }
            s.desk.launch.client_launch_enabled = input.enabled; s.desk.launch.changed_by = Some(by.clone()); s.desk.launch.changed_at = Some(at.clone());
            audit(s, "", if input.enabled { "client_launch_enabled" } else { "client_launch_disabled" }, format!("by {by}"));
            Ok(json!(s.desk.launch))
        }).await
    }
}
