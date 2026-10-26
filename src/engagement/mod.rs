//! Original, bounded NXT engagement workpapers; no Caseware runtime dependency.
pub mod analysis;
pub mod bridges;
pub mod models;
mod routes;
pub mod storage_models;
use crate::{referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{Duration, SecondsFormat, Utc};
use models::*;
pub use routes::router;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
pub use storage_models::*;

const STATE_KEY: &str = "engagements:state:v1";
pub type EResult<T> = Result<T, ApiError>;
pub struct Engagements {
    pub store: Store,
    pub ledger: Arc<crate::ledger::engine::Ledger>,
    pub tax: Arc<crate::tax::Tax>,
    pub firm: Arc<crate::firm::Firm>,
}
pub(crate) fn bad(s: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, s.into())
}
pub(crate) fn conflict(s: impl Into<String>) -> ApiError {
    ApiError(StatusCode::CONFLICT, s.into())
}
fn missing() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "engagement not found".into())
}
pub(crate) fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
fn id(p: &str) -> String {
    format!("{p}_{}", uuid::Uuid::new_v4().simple())
}
pub(crate) fn hash(v: &impl serde::Serialize) -> EResult<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(v).map_err(|_| bad("cannot encode engagement"))?,
    )))
}
fn check_revision(r: &EngagementRecord, v: u64) -> EResult<()> {
    if r.revision != v {
        Err(conflict("engagement changed; reload before retrying"))
    } else {
        Ok(())
    }
}
fn editable(r: &EngagementRecord) -> EResult<()> {
    if ["locked", "archived"].contains(&r.status.as_str()) {
        Err(conflict("reopen engagement to draft before changing it"))
    } else {
        Ok(())
    }
}
fn label(v: &str, name: &str, max: usize) -> EResult<()> {
    if v.trim().is_empty()
        || v.len() > max
        || v.chars().any(|c| {
            c.is_control() && !(name != "operator label" && matches!(c, '\n' | '\r' | '\t'))
        })
    {
        Err(bad(format!("invalid {name}")))
    } else {
        Ok(())
    }
}
fn checked_input(mut input: EngagementInput) -> EResult<EngagementInput> {
    input.expected_revision = None;
    analysis::validate(&input).map_err(bad)?;
    if serde_json::to_vec(&input)
        .map_err(|_| bad("invalid input"))?
        .len()
        > 512 * 1024
    {
        return Err(bad("engagement input exceeds 512 KiB"));
    }
    Ok(input)
}
fn record(input: EngagementInput, record_id: &str, at: &str) -> EngagementRecord {
    EngagementRecord {
        id: record_id.into(),
        revision: 1,
        input_revision: 1,
        status: "draft".into(),
        document: input,
        snapshot: None,
        snapshots: vec![],
        signoffs: vec![],
        signoff_history: vec![],
        lock_history: vec![],
        import_history: vec![],
        created_at: at.into(),
        updated_at: at.into(),
        rollforward_from: None,
    }
}
fn touch(r: &mut EngagementRecord) {
    r.revision += 1;
    r.updated_at = now();
}
fn invalidate(r: &mut EngagementRecord) {
    r.snapshot = None;
    r.signoffs.clear();
    r.status = "draft".into();
}
fn audit(s: &mut EngagementState, record_id: &str, action: &str, detail: &str) {
    s.audit.insert(
        0,
        AuditEvent {
            id: id("engaudit"),
            engagement_id: record_id.into(),
            action: action.into(),
            detail: detail.into(),
            at: now(),
        },
    );
    s.audit.truncate(1000);
}
pub(crate) fn fresh(r: &EngagementRecord) -> EResult<&Snapshot> {
    let s = r
        .snapshot
        .as_ref()
        .ok_or_else(|| conflict("calculate the current saved engagement first"))?;
    if s.input_revision != r.input_revision
        || s.input_hash != hash(&r.document)?
        || s.input_hash != hash(&s.input_document)?
        || s.output_hash != hash(&s.result)?
    {
        return Err(conflict("saved calculation is stale or corrupt"));
    }
    if s.result["supported"] != true {
        return Err(conflict(
            "resolve all blocking calculation diagnostics first",
        ));
    }
    Ok(s)
}
fn checkpoint(r: &EngagementRecord, paper: &str, stage: &str) -> bool {
    r.snapshot.as_ref().is_some_and(|snapshot| {
        r.signoffs.iter().any(|s| {
            s.workpaper_id == paper
                && s.stage == stage
                && s.input_hash == snapshot.input_hash
                && s.output_hash == snapshot.output_hash
        })
    })
}
fn required_done(r: &EngagementRecord, stage: &str) -> bool {
    let required = r
        .document
        .workpapers
        .iter()
        .filter(|p| p.required)
        .collect::<Vec<_>>();
    !required.is_empty() && required.iter().all(|p| checkpoint(r, &p.id, stage))
}

impl Engagements {
    pub fn new(
        store: Store,
        ledger: Arc<crate::ledger::engine::Ledger>,
        tax: Arc<crate::tax::Tax>,
        firm: Arc<crate::firm::Firm>,
    ) -> Self {
        Self {
            store,
            ledger,
            tax,
            firm,
        }
    }
    async fn load(&self) -> EResult<EngagementState> {
        match self.store.get(STATE_KEY).await? {
            Some(raw) => serde_json::from_str(&raw)
                .map_err(|_| conflict("engagement state is corrupt; recovery required")),
            None => Ok(EngagementState::default()),
        }
    }
    async fn mutate<T>(&self, mut f: impl FnMut(&mut EngagementState) -> EResult<T>) -> EResult<T> {
        for _ in 0..16 {
            let raw = self.store.get(STATE_KEY).await?;
            let mut s = match &raw {
                Some(raw) => serde_json::from_str::<EngagementState>(raw)
                    .map_err(|_| conflict("engagement state is corrupt; recovery required"))?,
                None => EngagementState::default(),
            };
            let out = f(&mut s)?;
            for r in &s.engagements {
                if serde_json::to_vec(r)
                    .map_err(|_| bad("cannot encode engagement"))?
                    .len()
                    > 1024 * 1024
                {
                    return Err(conflict(
                        "engagement history exceeds 1 MiB; export and preserve before continuing separately",
                    ));
                }
            }
            let next =
                serde_json::to_string(&s).map_err(|_| bad("cannot encode engagement state"))?;
            if next.len() > 8 * 1024 * 1024 {
                return Err(conflict("engagement workspace exceeds 8 MiB capacity"));
            }
            if self
                .store
                .compare_exchange(STATE_KEY, raw.as_deref(), &next)
                .await?
            {
                return Ok(out);
            }
        }
        Err(conflict("workspace changed repeatedly; reload and retry"))
    }
    pub fn catalog() -> Value {
        json!({"engine":"Independent NXT engagement workpapers","ruleset":"nxt-engagement-workpapers-v1","currency":"CAD","status":"validation_only","kinds":["internal","compilation","review","audit"],"account_kinds":["asset","liability","equity","revenue","expense"],"exports":["nxt-engagement-json","internal-html"],"limitations":["Original bounded workpapers; full Caseware parity is not verified.","Text documents only; no CaseView, Office embedding, proprietary files, SmartSync or consolidation.","Local working adjustments do not post to Ledger.","Shared workspace credential; operator labels are not verified identities.","No assurance report or certified tax filing is generated."]})
    }
    pub async fn get(&self, record_id: &str) -> EResult<EngagementRecord> {
        self.load()
            .await?
            .engagements
            .into_iter()
            .find(|r| r.id == record_id)
            .ok_or_else(missing)
    }
    pub async fn list(&self) -> EResult<Vec<Value>> {
        Ok(self.load().await?.engagements.into_iter().map(|r|json!({"id":r.id,"revision":r.revision,"input_revision":r.input_revision,"name":r.document.name,"client_ref":r.document.client_ref,"kind":r.document.kind,"period_start":r.document.period_start,"period_end":r.document.period_end,"status":r.status,"accounts":r.document.accounts.len(),"workpapers":r.document.workpapers.len(),"open_issues":r.document.issues.iter().filter(|i|i.status=="open").count(),"supported":r.snapshot.as_ref().map(|s|s.result["supported"].clone()),"updated_at":r.updated_at,"created_at":r.created_at,"rollforward_from":r.rollforward_from})).collect())
    }
    pub async fn summary(&self) -> EResult<Value> {
        let s = self.load().await?;
        let n = |status: &str| s.engagements.iter().filter(|r| r.status == status).count();
        Ok(
            json!({"total":s.engagements.len(),"drafts":n("draft"),"in_review":n("in_review"),"locked":n("locked"),"archived":n("archived"),"open_blocking_issues":s.engagements.iter().flat_map(|r|&r.document.issues).filter(|i|i.blocking&&i.status=="open").count(),"storage":self.store.backend_name(),"auth_required":crate::referral::routes::auth_required(),"access_model":"shared workspace credential; labels are metadata","status":"validation_only"}),
        )
    }
    pub async fn audit(&self) -> EResult<Vec<AuditEvent>> {
        Ok(self.load().await?.audit)
    }
    pub async fn save(
        &self,
        existing: Option<&str>,
        input: EngagementInput,
    ) -> EResult<EngagementRecord> {
        let expected = input.expected_revision;
        if existing.is_some() != expected.is_some() {
            return Err(bad("expected_revision is required only when editing"));
        }
        let input = checked_input(input)?;
        let rid = existing.map(str::to_owned).unwrap_or_else(|| id("eng"));
        let at = now();
        self.mutate(|s| {
            let old = s.engagements.iter().find(|r| r.id == rid).cloned();
            let mut r = if existing.is_some() {
                let mut r = old.ok_or_else(missing)?;
                check_revision(&r, expected.unwrap())?;
                editable(&r)?;
                r.document = input.clone();
                r.input_revision += 1;
                invalidate(&mut r);
                touch(&mut r);
                r
            } else {
                if s.engagements.len() >= 200 {
                    return Err(conflict("engagement capacity reached (200)"));
                }
                record(input.clone(), &rid, &at)
            };
            r.document.expected_revision = None;
            s.engagements.retain(|r| r.id != rid);
            s.engagements.push(r.clone());
            audit(
                s,
                &rid,
                if existing.is_some() {
                    "edited"
                } else {
                    "created"
                },
                "Saved input; active calculation and sign-offs invalidated on edit",
            );
            Ok(r)
        })
        .await
    }
    pub async fn calculate(&self, rid: &str, input: RevisionInput) -> EResult<EngagementRecord> {
        self.mutate(|s| {
            let r = s
                .engagements
                .iter_mut()
                .find(|r| r.id == rid)
                .ok_or_else(missing)?;
            check_revision(r, input.expected_revision)?;
            editable(r)?;
            if r.snapshot.is_some() {
                return Ok(r.clone());
            }
            if r.snapshots.len() >= 20 {
                return Err(conflict("snapshot history capacity reached (20)"));
            }
            let result = analysis::analyze(&r.document);
            let snapshot = Snapshot {
                id: id("engcalc"),
                input_revision: r.input_revision,
                input_hash: hash(&r.document)?,
                output_hash: hash(&result)?,
                input_document: r.document.clone(),
                result,
                created_at: now(),
            };
            r.snapshots.push(snapshot.clone());
            r.snapshot = Some(snapshot);
            r.signoffs.clear();
            touch(r);
            let out = r.clone();
            audit(
                s,
                rid,
                "calculated",
                "Saved immutable input and calculation hashes; no Ledger posting",
            );
            Ok(out)
        })
        .await
    }
    pub async fn signoff(&self, rid: &str, input: SignoffInput) -> EResult<EngagementRecord> {
        label(&input.operator, "operator label", 120)?;
        label(&input.note, "sign-off note", 4000)?;
        if !["prepared", "reviewed"].contains(&input.stage.as_str()) {
            return Err(bad("stage must be prepared or reviewed"));
        }
        self.mutate(|s| {
            let r = s
                .engagements
                .iter_mut()
                .find(|r| r.id == rid)
                .ok_or_else(missing)?;
            check_revision(r, input.expected_revision)?;
            editable(r)?;
            let snapshot = fresh(r)?.clone();
            let paper = r
                .document
                .workpapers
                .iter()
                .find(|p| p.id == input.workpaper_id)
                .ok_or_else(|| bad("unknown workpaper"))?;
            if paper.body.trim().is_empty() {
                return Err(conflict("workpaper content is required before sign-off"));
            }
            if input.stage == "reviewed"
                && (r.status != "in_review" || !checkpoint(r, &paper.id, "prepared"))
            {
                return Err(conflict("start review after preparing this workpaper"));
            }
            if !paper
                .depends_on
                .iter()
                .all(|p| checkpoint(r, p, &input.stage))
            {
                return Err(conflict("complete dependent workpaper checkpoints first"));
            }
            if input.stage == "prepared" && r.status != "draft" {
                return Err(conflict("prepare workpapers in draft status"));
            }
            if checkpoint(r, &paper.id, &input.stage) {
                return Err(conflict(
                    "this stage is already signed for the active snapshot",
                ));
            }
            if r.signoff_history.len() >= 2000 {
                return Err(conflict("sign-off history capacity reached"));
            }
            let sign = Signoff {
                id: id("sign"),
                workpaper_id: paper.id.clone(),
                stage: input.stage.clone(),
                operator: input.operator.trim().into(),
                note: input.note.clone(),
                input_hash: snapshot.input_hash,
                output_hash: snapshot.output_hash,
                at: now(),
            };
            r.signoffs.push(sign.clone());
            r.signoff_history.push(sign);
            touch(r);
            let out = r.clone();
            audit(
                s,
                rid,
                &format!("workpaper_{}", input.stage),
                "Internal responsibility label bound to current input/output hashes",
            );
            Ok(out)
        })
        .await
    }
    pub async fn transition(&self, rid: &str, input: TransitionInput) -> EResult<EngagementRecord> {
        label(&input.operator, "operator label", 120)?;
        label(&input.note, "transition reason", 4000)?;
        if !["draft", "in_review", "locked", "archived"].contains(&input.status.as_str()) {
            return Err(bad("invalid status"));
        }
        self.mutate(|s| {
            let r = s
                .engagements
                .iter_mut()
                .find(|r| r.id == rid)
                .ok_or_else(missing)?;
            check_revision(r, input.expected_revision)?;
            if r.status == input.status {
                return Err(conflict("engagement is already in that status"));
            }
            match input.status.as_str() {
                "in_review" => {
                    if r.status != "draft" {
                        return Err(conflict("start review from draft"));
                    }
                    fresh(r)?;
                    if !required_done(r, "prepared") {
                        return Err(conflict("prepare every required workpaper before review"));
                    }
                }
                "locked" => {
                    if r.status != "in_review" {
                        return Err(conflict("complete review before locking"));
                    }
                    fresh(r)?;
                    if !required_done(r, "reviewed") {
                        return Err(conflict("review every required workpaper before locking"));
                    }
                }
                "archived" => {
                    if r.status != "locked" {
                        return Err(conflict("lock the engagement before archiving"));
                    }
                }
                "draft" => {
                    invalidate(r);
                }
                _ => unreachable!(),
            };
            if r.lock_history.len() >= 20 {
                return Err(conflict("transition history capacity reached (20)"));
            }
            r.lock_history.push(LockEvent {
                status: input.status.clone(),
                operator: input.operator.clone(),
                note: input.note.clone(),
                snapshot_id: r.snapshot.as_ref().map(|v| v.id.clone()),
                input_hash: r.snapshot.as_ref().map(|v| v.input_hash.clone()),
                at: now(),
            });
            r.status = input.status.clone();
            touch(r);
            let out = r.clone();
            audit(
                s,
                rid,
                &format!("transition_{}", input.status),
                &format!("{}: {}", input.operator, input.note),
            );
            Ok(out)
        })
        .await
    }
    pub async fn rollforward(
        &self,
        rid: &str,
        input: RollforwardInput,
    ) -> EResult<EngagementRecord> {
        let new_id = id("eng");
        self.mutate(|s|{let old=s.engagements.iter().find(|r|r.id==rid).cloned().ok_or_else(missing)?;check_revision(&old,input.expected_revision)?;if old.status!="locked"{return Err(conflict("lock source engagement before rolling forward"));}let snapshot=fresh(&old)?;if s.engagements.len()>=200{return Err(conflict("engagement capacity reached"));}if input.period_start<=old.document.period_end{return Err(bad("next period must start after source period ends"));}if s.engagements.iter().any(|r|r.rollforward_from.as_deref()==Some(rid)&&r.document.period_start==input.period_start&&r.document.period_end==input.period_end){return Err(conflict("this source period has already been rolled forward"));}let mut d=old.document.clone();d.name=input.name.clone().unwrap_or_else(||d.name.clone());d.prior_period_start=Some(d.period_start.clone());d.prior_period_end=Some(d.period_end.clone());d.period_start=input.period_start.clone();d.period_end=input.period_end.clone();let rows=snapshot.result["accounts"].as_array().ok_or_else(||conflict("snapshot rows unavailable"))?;for a in &mut d.accounts{let row=rows.iter().find(|v|v["id"]==a.id).ok_or_else(||conflict("snapshot account missing"))?;a.prior_debit_cents=row["adjusted_debit_cents"].as_i64().ok_or_else(||conflict("invalid snapshot amount"))?;a.prior_credit_cents=row["adjusted_credit_cents"].as_i64().ok_or_else(||conflict("invalid snapshot amount"))?;a.debit_cents=0;a.credit_cents=0;}d.adjustments.clear();for paper in &mut d.workpapers{paper.body.clear();paper.adjustment_ids.clear();}d.issues.retain(|i|i.carry_forward&&i.status=="open");d.notes.clear();let d=checked_input(d)?;let mut r=record(d,&new_id,&now());r.rollforward_from=Some(rid.into());s.engagements.push(r.clone());audit(s,rid,"rolled_forward","Created separate next-period draft with adjusted comparatives; current balances, paper bodies and approvals cleared");Ok(r)}).await
    }
    pub async fn preview(&self, rid: &str, input: PreviewInput) -> EResult<Value> {
        let raw = match &input.document {
            Value::String(v) => v.clone(),
            v => serde_json::to_string(v).map_err(|_| bad("invalid native document"))?,
        };
        if raw.len() > 512 * 1024 {
            return Err(bad("trial balance import exceeds 512 KiB"));
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Package {
            format: String,
            accounts: Vec<Account>,
        }
        let package: Package = serde_json::from_str(&raw)
            .map_err(|_| bad("invalid native trial balance schema; cents must be integers"))?;
        if package.format != "nxt-engagement-tb/v1" {
            return Err(bad("only nxt-engagement-tb/v1 is supported"));
        }
        let source_hash = hex::encode(Sha256::digest(raw.as_bytes()));
        self.stage_accounts(
            rid,
            input.expected_revision,
            package.accounts,
            source_hash,
            json!({"source":"native_json","hash_scope":"submitted UTF-8 document"}),
        )
        .await
    }
    pub(crate) async fn stage_accounts(
        &self,
        rid: &str,
        expected: u64,
        accounts: Vec<Account>,
        source_hash: String,
        provenance: Value,
    ) -> EResult<Value> {
        let pid = id("engpreview");
        let at = now();
        let expires =
            (Utc::now() + Duration::minutes(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
        self.mutate(|s|{let r=s.engagements.iter().find(|r|r.id==rid).ok_or_else(missing)?;check_revision(r,expected)?;editable(r)?;let old=r.document.accounts.iter().map(|a|(a.id.as_str(),a)).collect::<HashMap<_,_>>();let mut warnings=vec!["Replaces saved accounts only after explicit commit; local adjustments and source Ledger remain unchanged.".into()];let mut accounts=accounts.clone();for a in &mut accounts{a.group_id=old.get(a.id.as_str()).filter(|prior|prior.kind==a.kind).and_then(|a|a.group_id.clone());}let mut d=r.document.clone();d.accounts=accounts.clone();let d=checked_input(d)?;let checks=analysis::analyze(&d);let canonical_hash=hash(&accounts)?;let unmapped=accounts.iter().filter(|a|a.group_id.is_none()).count();if unmapped>0{warnings.push(format!("{unmapped} accounts require mapping before review."));}let p=ImportPreview{id:pid.clone(),engagement_id:rid.into(),expected_revision:expected,source_hash:source_hash.clone(),canonical_hash,created_at:at.clone(),expires_at:expires.clone(),summary:json!({"accounts":accounts.len(),"unmapped_accounts":unmapped,"existing_accounts":r.document.accounts.len(),"target_revision":expected}),warnings,accounts,checks,committed_revision:None,provenance:provenance.clone()};s.previews.retain(|p|p.expires_at>at||p.committed_revision.is_some());if s.previews.len()>=40{return Err(conflict("import preview capacity reached (40)"));}let out=preview_value(&p);s.previews.push(p);audit(s,rid,"import_preview","Validated reviewed-copy trial balance; no target inputs changed");Ok(out)}).await
    }
    pub async fn commit(&self, rid: &str, input: CommitInput) -> EResult<EngagementRecord> {
        self.mutate(|s| {
            let p = s
                .previews
                .iter()
                .find(|p| p.id == input.preview_id && p.engagement_id == rid)
                .cloned()
                .ok_or_else(|| bad("import preview not found"))?;
            let r = s
                .engagements
                .iter_mut()
                .find(|r| r.id == rid)
                .ok_or_else(missing)?;
            if hash(&p.accounts)? != p.canonical_hash {
                return Err(conflict("stored preview accounts do not match their hash"));
            }
            if p.committed_revision.is_some() {
                return Ok(r.clone());
            }
            check_revision(r, input.expected_revision)?;
            editable(r)?;
            if p.expected_revision != r.revision {
                return Err(conflict("preview is stale; preview again"));
            }
            if p.expires_at < now() {
                return Err(conflict("preview expired"));
            }
            if r.import_history.len() >= 100 {
                return Err(conflict("import history capacity reached (100)"));
            }
            let mut d = r.document.clone();
            d.accounts = p.accounts;
            let d = checked_input(d)?;
            r.document = d;
            r.input_revision += 1;
            r.import_history.push(ImportReceipt {
                preview_id: p.id,
                source_hash: p.source_hash,
                canonical_hash: p.canonical_hash,
                provenance: p.provenance,
                input_revision: r.input_revision,
                at: now(),
            });
            invalidate(r);
            touch(r);
            let out = r.clone();
            s.previews
                .iter_mut()
                .find(|p| p.id == input.preview_id)
                .unwrap()
                .committed_revision = Some(out.revision);
            audit(
                s,
                rid,
                "import_committed",
                "Committed validated trial balance copy and invalidated active approvals",
            );
            Ok(out)
        })
        .await
    }
    pub async fn package(&self, rid: &str) -> EResult<Value> {
        Ok(
            json!({"format":"nxt-engagement/v1","scope":"Internal NXT workpaper archive; not a Caseware native file or assurance report","engagement":self.get(rid).await?}),
        )
    }
}
fn preview_value(p: &ImportPreview) -> Value {
    json!(p)
}
