//! Original NXT internal agreement preparation. No send, signing or connector credentials.
pub mod models;
pub mod esign;
mod routes;
use crate::{referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use models::*;
pub use routes::router;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const STATE_KEY: &str = "signatures:state:v1";
const STATE_MAX: usize = 4 * 1024 * 1024;
const ENVELOPE_MAX: usize = 128 * 1024;
const IDENTITY_LIMIT: &str = "Shared workspace key. Operator and reviewer labels are metadata, not verified identities or independent approvals.";
type SResult<T> = Result<T, ApiError>;

pub struct Signatures {
    pub store: Store,
}
fn bad(v: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, v.into())
}
fn conflict(v: impl Into<String>) -> ApiError {
    ApiError(StatusCode::CONFLICT, v.into())
}
fn missing() -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        "signature preparation record not found".into(),
    )
}
fn storage_error(_: String) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "signature preparation storage unavailable".into(),
    )
}
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}
fn hash(value: &impl serde::Serialize) -> SResult<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|_| bad("cannot encode preparation"))?,
    )))
}
fn label(value: &str, max: usize, multiline: bool) -> bool {
    value.len() <= max
        && !value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
}
fn nonempty(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value == value.trim() && label(value, max, false)
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.:".contains(&c))
}
fn email(value: &str) -> bool {
    if value.len() > 254 || !value.is_ascii() {
        return false;
    }
    let mut parts = value.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
        && domain.contains('.')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
fn controls() -> Value {
    json!({"preparation_only":true,"external_send_enabled":false,"signing_enabled":false,"external_delivery":false,"legal_effect":false,"completion_certificate":false,"connector":{"provider":"docusign","status":"unconfigured"},"identity_limit":IDENTITY_LIMIT,"source_parity":"unverified","context_links":"user-supplied metadata, not verified source bridges"})
}
fn controlled(mut value: Value) -> Value {
    value
        .as_object_mut()
        .unwrap()
        .extend(controls().as_object().unwrap().clone());
    value
}
fn validate(draft: &Draft) -> SResult<()> {
    if !nonempty(&draft.title, 200)
        || !label(&draft.email_subject, 200, false)
        || !label(&draft.email_message, 8000, true)
    {
        return Err(bad("title, subject or message is invalid"));
    }
    if let Some(context) = &draft.context {
        if !["firm", "tax", "engagements"].contains(&context.module.as_str())
            || !identifier(&context.record_id)
            || !nonempty(&context.label, 240)
        {
            return Err(bad("invalid NXT context reference"));
        }
    }
    if draft.documents.len() > 10
        || draft.recipients.len() > 30
        || draft.fields.len() > 200
        || serde_json::to_vec(draft)
            .map_err(|_| bad("invalid preparation"))?
            .len()
            > ENVELOPE_MAX
    {
        return Err(bad(
            "preparation exceeds document, recipient, field or 128 KiB bound",
        ));
    }
    let mut docs = HashSet::new();
    for document in &draft.documents {
        if !identifier(&document.id)
            || !docs.insert(&document.id)
            || !nonempty(&document.name, 200)
            || document.pages.is_empty()
            || document.pages.len() > 20
            || document
                .pages
                .iter()
                .any(|p| !label(p, 16000, true) || p.trim().is_empty())
        {
            return Err(bad(
                "invalid document, duplicate document id or empty/oversize native page",
            ));
        }
    }
    let mut recipients = HashSet::new();
    for recipient in &draft.recipients {
        if !identifier(&recipient.id)
            || !recipients.insert(&recipient.id)
            || !nonempty(&recipient.role_name, 100)
            || !label(&recipient.name, 200, false)
            || (!recipient.email.is_empty() && !email(&recipient.email))
            || !["signer", "copy", "needs_to_view"].contains(&recipient.kind.as_str())
            || !(1..=100).contains(&recipient.routing_order)
        {
            return Err(bad("invalid recipient role, order, email or duplicate id"));
        }
    }
    let mut fields = HashSet::new();
    for field in &draft.fields {
        let document = draft.documents.iter().find(|d| d.id == field.document_id);
        let recipient = draft.recipients.iter().find(|r| r.id == field.recipient_id);
        if !identifier(&field.id)
            || !fields.insert(&field.id)
            || !nonempty(&field.label, 100)
            || !["signature", "initials", "name", "date", "text", "checkbox"]
                .contains(&field.kind.as_str())
            || field.page == 0
            || document.is_none_or(|d| field.page > d.pages.len())
            || recipient.is_none_or(|r| r.kind != "signer")
            || field.width == 0
            || field.height == 0
            || u32::from(field.x) + u32::from(field.width) > 10000
            || u32::from(field.y) + u32::from(field.height) > 10000
        {
            return Err(bad(
                "invalid field, page, signer assignment or page-relative bounds",
            ));
        }
    }
    Ok(())
}
fn readiness(draft: &Draft) -> Vec<String> {
    let mut findings = Vec::new();
    if draft.documents.is_empty() {
        findings.push("Add an original NXT text document.".into());
    }
    if draft.email_subject.trim().is_empty() {
        findings.push("Add a proposed email subject.".into());
    }
    if !draft.recipients.iter().any(|r| r.kind == "signer") {
        findings.push("Add at least one signer role.".into());
    }
    for r in &draft.recipients {
        if !nonempty(&r.name, 200) || !email(&r.email) {
            findings.push(format!(
                "Complete the name and email for role {}.",
                r.role_name
            ));
        }
        if r.kind == "signer"
            && !draft
                .fields
                .iter()
                .any(|f| f.recipient_id == r.id && f.kind == "signature" && f.required)
        {
            findings.push(format!(
                "Add a required signature placeholder for role {}.",
                r.role_name
            ));
        }
    }
    findings
}
fn check(record: &Envelope, revision: u64, content_hash: Option<&str>) -> SResult<()> {
    if record.revision != revision || content_hash.is_some_and(|h| h != record.content_hash) {
        Err(conflict("preparation changed; reload before retrying"))
    } else {
        Ok(())
    }
}
fn event(
    state: &mut SignatureState,
    record_id: &str,
    action: &str,
    operator: &str,
    revision: u64,
    content_hash: &str,
    note: &str,
) -> SResult<()> {
    if state.events.len() >= 2000 {
        return Err(conflict(
            "audit capacity reached; preserve and archive workspace before adding records",
        ));
    }
    let mut e = AuditEvent {
        id: id("siga"),
        occurred_at: now(),
        record_id: record_id.into(),
        action: action.into(),
        operator_label: operator.into(),
        revision,
        content_hash: content_hash.into(),
        previous_event_hash: state.events.last().map(|e| e.event_hash.clone()),
        note: note.into(),
        event_hash: String::new(),
    };
    e.event_hash = hash(&e)?;
    state.events.push(e);
    Ok(())
}
fn detail(record: &Envelope) -> Value {
    controlled(json!({"envelope":record,"readiness_findings":readiness(&record.draft)}))
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn resource_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
fn timestamp(value: &str) -> bool {
    value.len() <= 40 && chrono::DateTime::parse_from_rfc3339(value).is_ok()
}
fn invariant(condition: bool) -> SResult<()> {
    if condition {
        Ok(())
    } else {
        Err(bad("inconsistent preparation state"))
    }
}

/// The audit projection checks current records against retained transitions;
/// it cannot independently attest history or recover prior document revisions.
struct Projection<'a> {
    revision: u64,
    status: &'static str,
    content_hash: &'a str,
    creator: &'a str,
    reviewed_by: Option<&'a str>,
}
fn validate_state(state: &SignatureState) -> SResult<()> {
    invariant(
        state.envelopes.len() <= 100 && state.templates.len() <= 30 && state.events.len() <= 2000,
    )?;
    let mut record_ids = HashSet::new();
    for record in &state.envelopes {
        validate(&record.draft)?;
        invariant(
            resource_id(&record.id, "sig_")
                && record_ids.insert(record.id.as_str())
                && (1..=2000).contains(&record.revision)
                && ["draft", "in_review", "approved", "parallel_sent", "parallel_signed", "parallel_voided", "parallel_declined", "cancelled"]
                    .contains(&record.status.as_str())
                && nonempty(&record.created_by_label, 120)
                && timestamp(&record.created_at)
                && timestamp(&record.updated_at)
                && record.content_hash == hash(&record.draft)?,
        )?;
        if ["approved", "parallel_sent", "parallel_signed", "parallel_voided", "parallel_declined"].contains(&record.status.as_str()) {
            invariant(
                record.reviewed_content_hash.as_deref() == Some(record.content_hash.as_str())
                    && record
                        .reviewed_by_label
                        .as_deref()
                        .is_some_and(|v| nonempty(v, 120)),
            )?;
        } else {
            invariant(
                record.reviewed_content_hash.is_none() && record.reviewed_by_label.is_none(),
            )?;
        }
        if ["in_review", "approved", "parallel_sent", "parallel_signed", "parallel_voided", "parallel_declined"].contains(&record.status.as_str()) {
            invariant(readiness(&record.draft).is_empty())?;
        }
        if let Some(template_id) = &record.template_id {
            invariant(state.templates.iter().any(|t| t.id == *template_id))?;
        }
        if let Some(token) = &record.lock_token {
            invariant(token.starts_with("sigl_") && nonempty(token, 80))?;
        }
        for value in &record.field_values {
            invariant(record.draft.fields.iter().any(|field| field.id == value.field_id && !matches!(field.kind.as_str(), "signature" | "initials")) && nonempty(&value.value, 200))?;
        }
    }
    if state.folders.len() > 50 { return Err(bad("inconsistent preparation state")); }
    let mut folder_ids = HashSet::new();
    for folder in &state.folders {
        invariant(resource_id(&folder.id, "sigf_") && folder_ids.insert(folder.id.as_str()) && nonempty(&folder.name, 80))?;
    }
    for record in &state.envelopes {
        if let Some(folder_id) = &record.folder_id {
            invariant(folder_ids.contains(folder_id.as_str()))?;
        }
    }
    for template in &state.templates {
        validate(&template.draft)?;
        invariant(
            resource_id(&template.id, "sigt_")
                && record_ids.insert(template.id.as_str())
                && template.version == 1
                && nonempty(&template.name, 200)
                && nonempty(&template.created_by_label, 120)
                && timestamp(&template.created_at)
                && template.draft.context.is_none()
                && template
                    .draft
                    .recipients
                    .iter()
                    .all(|r| r.name.is_empty() && r.email.is_empty())
                && template.content_hash == hash(&template.draft)?,
        )?;
    }
    let mut event_ids = HashSet::new();
    let mut previous_hash: Option<&str> = None;
    let mut projections: HashMap<&str, Projection<'_>> = HashMap::new();
    for event in &state.events {
        invariant(
            resource_id(&event.id, "siga_")
                && event_ids.insert(event.id.as_str())
                && record_ids.contains(event.record_id.as_str())
                && timestamp(&event.occurred_at)
                && nonempty(&event.operator_label, 120)
                && label(&event.note, 2000, false)
                && digest(&event.content_hash)
                && digest(&event.event_hash)
                && event.previous_event_hash.as_deref() == previous_hash,
        )?;
        let mut material = event.clone();
        material.event_hash.clear();
        invariant(event.event_hash == hash(&material)?)?;
        previous_hash = Some(&event.event_hash);
        if let Some(template) = state.templates.iter().find(|t| t.id == event.record_id) {
            invariant(
                event.action == "template_created"
                    && event.revision == 1
                    && event.content_hash == template.content_hash
                    && event.operator_label == template.created_by_label
                    && !projections.contains_key(event.record_id.as_str()),
            )?;
            projections.insert(
                &event.record_id,
                Projection {
                    revision: 1,
                    status: "template",
                    content_hash: &event.content_hash,
                    creator: &event.operator_label,
                    reviewed_by: None,
                },
            );
            continue;
        }
        let record = state
            .envelopes
            .iter()
            .find(|r| r.id == event.record_id)
            .ok_or_else(|| bad("inconsistent preparation state"))?;
        if let Some(projection) = projections.get_mut(event.record_id.as_str()) {
            invariant(event.revision == projection.revision + 1)?;
            if event.action != "draft_updated" {
                invariant(
                    event.content_hash == projection.content_hash && !event.note.trim().is_empty(),
                )?;
            }
            projection.status = match (projection.status, event.action.as_str()) {
                ("draft", "draft_updated") => "draft",
                ("draft", "submit_review") => "in_review",
                ("in_review", "approve") => "approved",
                ("approved", "parallel_send") => "parallel_sent",
                ("parallel_sent", "parallel_sign") => "parallel_signed",
                ("parallel_sent", "parallel_resend") => "parallel_sent",
                ("parallel_sent", "parallel_correct") => "draft",
                ("parallel_sent", "parallel_void") => "parallel_voided",
                ("parallel_sent", "parallel_decline") => "parallel_declined",
                (status @ ("draft" | "in_review" | "approved" | "parallel_sent" | "parallel_signed" | "parallel_voided" | "parallel_declined"), "parallel_lock" | "parallel_unlock" | "assign_folder") => status,
                ("in_review" | "approved" | "parallel_sent" | "parallel_signed" | "parallel_voided" | "parallel_declined", "return_to_draft") => "draft",
                ("draft" | "in_review" | "approved" | "parallel_sent" | "parallel_signed" | "parallel_voided" | "parallel_declined", "cancel") => "cancelled",
                _ => return Err(bad("inconsistent preparation state")),
            };
            projection.revision = event.revision;
            projection.content_hash = &event.content_hash;
            if event.action == "approve" {
                projection.reviewed_by = Some(event.operator_label.as_str());
            } else if !matches!(projection.status, "approved" | "parallel_sent" | "parallel_signed" | "parallel_voided" | "parallel_declined") {
                projection.reviewed_by = None;
            }
        } else {
            let bulk = event.action == "parallel_bulk_created";
            invariant(
                event.revision == 1
                    && (bulk
                        || event.action
                            == if record.template_id.is_some() {
                                "template_instantiated"
                            } else {
                                "draft_created"
                            }),
            )?;
            projections.insert(
                &event.record_id,
                Projection {
                    revision: 1,
                    status: if bulk { "parallel_sent" } else { "draft" },
                    content_hash: &event.content_hash,
                    creator: &event.operator_label,
                    reviewed_by: if bulk { Some(event.operator_label.as_str()) } else { None },
                },
            );
        }
    }
    invariant(projections.len() == record_ids.len())?;
    for record in &state.envelopes {
        let projection = projections
            .get(record.id.as_str())
            .ok_or_else(|| bad("inconsistent preparation state"))?;
        invariant(
            projection.revision == record.revision
                && projection.status == record.status
                && projection.content_hash == record.content_hash
                && projection.creator == record.created_by_label
                && projection.reviewed_by == record.reviewed_by_label.as_deref(),
        )?;
    }
    Ok(())
}

impl Signatures {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
    async fn read(&self) -> SResult<(Option<String>, SignatureState)> {
        let raw = self.store.get(STATE_KEY).await.map_err(storage_error)?;
        if raw.as_ref().is_some_and(|r| r.len() > STATE_MAX) {
            return Err(storage_error(String::new()));
        }
        let state: SignatureState = match &raw {
            Some(r) => serde_json::from_str(r).map_err(|_| storage_error(String::new()))?,
            None => SignatureState::default(),
        };
        validate_state(&state).map_err(|_| storage_error(String::new()))?;
        Ok((raw, state))
    }
    async fn mutate<T>(&self, action: impl Fn(&mut SignatureState) -> SResult<T>) -> SResult<T> {
        for _ in 0..8 {
            let (raw, mut state) = self.read().await?;
            let result = action(&mut state)?;
            validate_state(&state).map_err(|_| storage_error(String::new()))?;
            let encoded = serde_json::to_string(&state)
                .map_err(|_| bad("cannot encode preparation workspace"))?;
            if encoded.len() > STATE_MAX {
                return Err(conflict(
                    "preparation workspace exceeds 4 MiB; archive before adding records",
                ));
            }
            if self
                .store
                .compare_exchange(STATE_KEY, raw.as_deref(), &encoded)
                .await
                .map_err(storage_error)?
            {
                return Ok(result);
            }
        }
        Err(conflict(
            "preparation workspace changed; reload before retrying",
        ))
    }
    pub fn catalog() -> Value {
        controlled(
            json!({"module":"NXT Signatures","native_scope":["original text document pages","recipient roles and routing order","page-relative placeholders","internal review","role-only templates","parallel send rehearsal","parallel sign rehearsal","internal folders","edit lock","bulk rehearsal","audit and preparation export"],"views":["home","drafts","review","approved","templates","audit","connector"],"limits":{"envelopes":100,"templates":30,"folders":50,"audit_events":2000,"document_pages":20,"documents":10,"recipients":30,"fields":200,"draft_bytes":ENVELOPE_MAX,"state_bytes":STATE_MAX},"parallel_rehearsal":"parallel_sent, parallel_signed, parallel_voided, and parallel_declined stay inside NXT. Resend, correct, and bulk rehearsal do not send email. No DocuSign API call, legal effect, or completion certificate.","unsupported":["PDF upload/rendering","external send","recipient authentication","signature execution","signed-document evidence","completion certificates","webhooks","external envelope import","external bulk delivery","mailbox sharing","reminders","payments","conditional routing","independent user identities"]}),
        )
    }
    pub async fn list(&self) -> SResult<Value> {
        let (_, state) = self.read().await?;
        let envelopes:Vec<Value> = state.envelopes.iter().rev().map(|r| json!({"id":r.id,"title":r.draft.title,"status":r.status,"revision":r.revision,"updated_at":r.updated_at,"recipients":r.draft.recipients.len(),"documents":r.draft.documents.len(),"context":r.draft.context,"readiness_findings":readiness(&r.draft).len()})).collect();
        let templates:Vec<Value> = state.templates.iter().map(|t| json!({"id":t.id,"name":t.name,"version":t.version,"content_hash":t.content_hash,"created_at":t.created_at,"roles":t.draft.recipients})).collect();
        Ok(controlled(
            json!({"envelopes":envelopes,"templates":templates,"audit_events":state.events.len(),"storage":self.store.backend_name()}),
        ))
    }
    pub async fn get(&self, record_id: &str) -> SResult<Value> {
        let (_, state) = self.read().await?;
        Ok(detail(
            state
                .envelopes
                .iter()
                .find(|r| r.id == record_id)
                .ok_or_else(missing)?,
        ))
    }
    pub async fn save(&self, record_id: Option<&str>, input: SaveInput) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) {
            return Err(bad("operator label is required"));
        }
        validate(&input.draft)?;
        if record_id.is_none() && input.expected_revision.is_some() {
            return Err(bad("new drafts must omit expected_revision"));
        }
        let new_id = id("sig");
        self.mutate(|state| {
            let record = if let Some(record_id) = record_id {
                let record = state
                    .envelopes
                    .iter_mut()
                    .find(|r| r.id == record_id)
                    .ok_or_else(missing)?;
                check(
                    record,
                    input
                        .expected_revision
                        .ok_or_else(|| bad("expected_revision is required"))?,
                    None,
                )?;
                if record.lock_token.is_some() {
                    return Err(conflict("preparation is locked; unlock it before editing"));
                }
                if record.status != "draft" {
                    return Err(conflict("return the preparation to draft before editing"));
                }
                record.revision += 1;
                record.draft = input.draft.clone();
                record.content_hash = hash(&record.draft)?;
                record.updated_at = now();
                record.reviewed_content_hash = None;
                record.reviewed_by_label = None;
                record.clone()
            } else {
                if state.envelopes.len() >= 100 {
                    return Err(conflict(
                        "envelope capacity reached; archive before adding records",
                    ));
                }
                let record = Envelope {
                    id: new_id.clone(),
                    revision: 1,
                    status: "draft".into(),
                    created_at: now(),
                    updated_at: now(),
                    created_by_label: input.operator_label.clone(),
                    content_hash: hash(&input.draft)?,
                    reviewed_content_hash: None,
                    reviewed_by_label: None,
                    template_id: None,
                    draft: input.draft.clone(),
                    folder_id: None,
                    lock_token: None,
                    resend_count: 0,
                    field_values: Vec::new(),
                };
                state.envelopes.push(record.clone());
                record
            };
            event(
                state,
                &record.id,
                if record_id.is_some() {
                    "draft_updated"
                } else {
                    "draft_created"
                },
                &input.operator_label,
                record.revision,
                &record.content_hash,
                "",
            )?;
            Ok(detail(&record))
        })
        .await
    }
    pub async fn transition(&self, record_id: &str, input: TransitionInput) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) || !nonempty(&input.note, 2000) {
            return Err(bad("operator label and review note are required"));
        }
        self.mutate(|state| {
            let record = state
                .envelopes
                .iter_mut()
                .find(|r| r.id == record_id)
                .ok_or_else(missing)?;
            check(
                record,
                input.expected_revision,
                Some(&input.expected_content_hash),
            )?;
            match (record.status.as_str(), input.action.as_str()) {
                ("draft", "submit_review") => {
                    if !readiness(&record.draft).is_empty() {
                        return Err(conflict(
                            "resolve preparation readiness findings before review",
                        ));
                    }
                    record.status = "in_review".into();
                }
                ("in_review", "approve") => {
                    if !readiness(&record.draft).is_empty() {
                        return Err(conflict(
                            "resolve preparation readiness findings before approval",
                        ));
                    }
                    record.status = "approved".into();
                    record.reviewed_content_hash = Some(record.content_hash.clone());
                    record.reviewed_by_label = Some(input.operator_label.clone());
                }
                ("approved", "parallel_send") => {
                    record.status = "parallel_sent".into();
                }
                ("parallel_sent", "parallel_sign") => {
                    for value in &input.field_values {
                        let field = record.draft.fields.iter().find(|field| field.id == value.field_id).ok_or_else(|| bad("unknown field"))?;
                        if matches!(field.kind.as_str(), "signature" | "initials") {
                            return Err(conflict("signature and initials placeholders stay unfilled; this rehearsal has no legal effect"));
                        }
                        if !nonempty(&value.value, 200) { return Err(bad("field value is invalid")); }
                    }
                    record.field_values = input.field_values.iter().map(|value| FieldValue { field_id: value.field_id.clone(), value: value.value.trim().into() }).collect();
                    record.status = "parallel_signed".into();
                }
                ("parallel_sent", "parallel_resend") => {
                    record.resend_count = record.resend_count.saturating_add(1);
                }
                ("parallel_sent", "parallel_correct") => {
                    record.status = "draft".into();
                    record.reviewed_content_hash = None;
                    record.reviewed_by_label = None;
                }
                ("parallel_sent", "parallel_void") => {
                    record.status = "parallel_voided".into();
                }
                ("parallel_sent", "parallel_decline") => {
                    record.status = "parallel_declined".into();
                }
                ("in_review" | "approved" | "parallel_sent" | "parallel_signed" | "parallel_voided" | "parallel_declined", "return_to_draft") => {
                    record.status = "draft".into();
                    record.reviewed_content_hash = None;
                    record.reviewed_by_label = None;
                }
                ("draft" | "in_review" | "approved" | "parallel_sent" | "parallel_signed" | "parallel_voided" | "parallel_declined", "cancel") => {
                    record.status = "cancelled".into();
                    record.reviewed_content_hash = None;
                    record.reviewed_by_label = None;
                }
                _ => return Err(conflict("unsupported internal preparation transition")),
            }
            record.revision += 1;
            record.updated_at = now();
            let record = record.clone();
            event(
                state,
                &record.id,
                &input.action,
                &input.operator_label,
                record.revision,
                &record.content_hash,
                &input.note,
            )?;
            Ok(detail(&record))
        })
        .await
    }
    pub async fn create_template(&self, record_id: &str, input: TemplateInput) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) || !nonempty(&input.name, 200) {
            return Err(bad("operator label and template name are required"));
        }
        let template_id = id("sigt");
        self.mutate(|state| {
            if state.templates.len()>=30 { return Err(conflict("template capacity reached")); }
            let record=state.envelopes.iter().find(|r|r.id==record_id).ok_or_else(missing)?;
            check(record,input.expected_revision,Some(&input.expected_content_hash))?;
            if record.status=="cancelled" { return Err(conflict("cancelled preparation cannot create a template")); }
            let mut draft=record.draft.clone(); draft.context=None;
            for r in &mut draft.recipients { r.name.clear(); r.email.clear(); }
            let template=Template {id:template_id.clone(),name:input.name.clone(),version:1,created_at:now(),created_by_label:input.operator_label.clone(),content_hash:hash(&draft)?,draft,favorite:false};
            event(state,&template.id,"template_created",&input.operator_label,1,&template.content_hash,"Recipient names, emails and NXT context removed. Native document text and proposed message retained.")?;
            state.templates.push(template.clone());
            Ok(controlled(json!({"template":template})))
        }).await
    }
    pub async fn template(&self, template_id: &str) -> SResult<Value> {
        let (_, state) = self.read().await?;
        Ok(controlled(
            json!({"template":state.templates.iter().find(|t|t.id==template_id).ok_or_else(missing)?}),
        ))
    }
    pub async fn instantiate(&self, template_id: &str, input: InstantiateInput) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120)
            || !nonempty(&input.title, 200)
            || input.recipients.len() > 30
        {
            return Err(bad(
                "operator label, draft title or recipient count is invalid",
            ));
        }
        let record_id = id("sig");
        self.mutate(|state| {
            if state.envelopes.len() >= 100 {
                return Err(conflict("envelope capacity reached"));
            }
            let template = state
                .templates
                .iter()
                .find(|t| t.id == template_id)
                .ok_or_else(missing)?;
            if input.expected_template_hash != template.content_hash {
                return Err(conflict("template changed; reload before using it"));
            }
            let mut draft = template.draft.clone();
            draft.title = input.title.clone();
            let mut seen = HashSet::new();
            for identity in &input.recipients {
                if !seen.insert(&identity.id) {
                    return Err(bad("duplicate recipient identity"));
                }
                let recipient = draft
                    .recipients
                    .iter_mut()
                    .find(|r| r.id == identity.id)
                    .ok_or_else(|| bad("recipient identity must match a template role"))?;
                recipient.name = identity.name.clone();
                recipient.email = identity.email.clone();
            }
            validate(&draft)?;
            let record = Envelope {
                id: record_id.clone(),
                revision: 1,
                status: "draft".into(),
                created_at: now(),
                updated_at: now(),
                created_by_label: input.operator_label.clone(),
                content_hash: hash(&draft)?,
                reviewed_content_hash: None,
                reviewed_by_label: None,
                template_id: Some(template_id.into()),
                draft,
                folder_id: None,
                lock_token: None,
                resend_count: 0,
                field_values: Vec::new(),
            };
            event(
                state,
                &record.id,
                "template_instantiated",
                &input.operator_label,
                1,
                &record.content_hash,
                "",
            )?;
            state.envelopes.push(record.clone());
            Ok(detail(&record))
        })
        .await
    }
    pub async fn audit(&self, offset: usize, limit: usize) -> SResult<Value> {
        if !(1..=200).contains(&limit) {
            return Err(bad("audit limit must be 1 to 200"));
        }
        let (_, state) = self.read().await?;
        Ok(controlled(
            json!({"total":state.events.len(),"offset":offset,"events":state.events.iter().skip(offset).take(limit).collect::<Vec<_>>(),"integrity_scope":"SHA-256 chained workspace events; not an external tamper-proof timestamp or signing certificate"}),
        ))
    }
    pub async fn create_folder(&self, name: &str) -> SResult<Value> {
        if !nonempty(name, 80) { return Err(bad("folder name is required")); }
        let folder_id = id("sigf");
        self.mutate(|state| {
            if state.folders.len() >= 50 { return Err(conflict("folder capacity reached")); }
            state.folders.push(Folder { id: folder_id.clone(), name: name.trim().into() });
            Ok(controlled(json!({"id": folder_id, "mailbox_sharing": false, "external_delivery": false})))
        }).await
    }
    pub async fn assign_folder(&self, record_id: &str, input: TransitionInput) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) || !nonempty(&input.note, 80) { return Err(bad("operator label and folder id are required")); }
        self.mutate(|state| {
            if !state.folders.iter().any(|folder| folder.id == input.note) { return Err(bad("unknown folder")); }
            let record = state.envelopes.iter_mut().find(|record| record.id == record_id).ok_or_else(missing)?;
            check(record, input.expected_revision, Some(&input.expected_content_hash))?;
            if record.status == "cancelled" { return Err(conflict("cancelled preparation cannot be filed")); }
            record.folder_id = Some(input.note.clone());
            record.revision += 1;
            record.updated_at = now();
            let record = record.clone();
            event(state, &record.id, "assign_folder", &input.operator_label, record.revision, &record.content_hash, &input.note)?;
            Ok(detail(&record))
        }).await
    }
    pub async fn set_lock(&self, record_id: &str, input: TransitionInput, lock: bool) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) || !nonempty(&input.note, 2000) { return Err(bad("operator label and note are required")); }
        self.mutate(|state| {
            let record = state.envelopes.iter_mut().find(|record| record.id == record_id).ok_or_else(missing)?;
            check(record, input.expected_revision, Some(&input.expected_content_hash))?;
            if record.status == "cancelled" { return Err(conflict("cancelled preparation cannot be locked")); }
            if lock {
                if record.lock_token.is_some() { return Err(conflict("preparation is already locked")); }
                record.lock_token = Some(id("sigl"));
            } else if record.lock_token.is_none() {
                return Err(conflict("preparation is not locked"));
            } else {
                record.lock_token = None;
            }
            record.revision += 1;
            record.updated_at = now();
            let record = record.clone();
            event(state, &record.id, if lock { "parallel_lock" } else { "parallel_unlock" }, &input.operator_label, record.revision, &record.content_hash, &input.note)?;
            Ok(detail(&record))
        }).await
    }
    pub async fn favorite_template(&self, template_id: &str, favorite: bool) -> SResult<Value> {
        self.mutate(|state| {
            let template = state.templates.iter_mut().find(|template| template.id == template_id).ok_or_else(missing)?;
            template.favorite = favorite;
            Ok(controlled(json!({"template": template.clone(), "external_share": false})))
        }).await
    }
    pub async fn bulk_rehearse(&self, record_id: &str, input: TransitionInput, copies: u8) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) || !nonempty(&input.note, 2000) || !(1..=10).contains(&copies) {
            return Err(bad("operator label, note, or copy count is invalid"));
        }
        self.mutate(|state| {
            let source = state.envelopes.iter().find(|record| record.id == record_id).cloned().ok_or_else(missing)?;
            check(&source, input.expected_revision, Some(&input.expected_content_hash))?;
            if source.status != "approved" || source.reviewed_content_hash.as_deref() != Some(source.content_hash.as_str()) {
                return Err(conflict("approve the preparation before a bulk rehearsal"));
            }
            if state.envelopes.len() + usize::from(copies) > 100 { return Err(conflict("envelope capacity reached")); }
            let mut ids = Vec::new();
            for _ in 0..copies {
                let record = Envelope {
                    id: id("sig"), revision: 1, status: "parallel_sent".into(), created_at: now(), updated_at: now(),
                    created_by_label: input.operator_label.clone(), content_hash: source.content_hash.clone(),
                    reviewed_content_hash: Some(source.content_hash.clone()), reviewed_by_label: Some(input.operator_label.clone()),
                    template_id: None, draft: source.draft.clone(), folder_id: None, lock_token: None, resend_count: 0, field_values: Vec::new(),
                };
                event(state, &record.id, "parallel_bulk_created", &input.operator_label, 1, &record.content_hash, &input.note)?;
                ids.push(record.id.clone());
                state.envelopes.push(record);
            }
            Ok(controlled(json!({"ids": ids, "external_delivery": false, "legal_effect": false, "email_sent": false})))
        }).await
    }
    pub async fn package(&self, record_id: &str) -> SResult<Value> {
        let (_, state) = self.read().await?;
        let record = state
            .envelopes
            .iter()
            .find(|r| r.id == record_id)
            .ok_or_else(missing)?;
        if record.status != "approved"
            || record.reviewed_content_hash.as_deref() != Some(&record.content_hash)
        {
            return Err(conflict(
                "approve the current preparation before exporting a review package",
            ));
        }
        Ok(controlled(
            json!({"format":"nxt-signature-preparation/v1","envelope":record,"events":state.events.iter().filter(|e|e.record_id==record_id).collect::<Vec<_>>(),"contains_signatures":false,"document_format":"original NXT text pages, not a signed PDF","purpose":"Internal preparation review only. No recipient was contacted and no signature was executed."}),
        ))
    }
}
