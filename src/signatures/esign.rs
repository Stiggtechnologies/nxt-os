//! Live NXT e-signature on top of the reviewed preparation workflow.
//!
//! Flow: an `approved` preparation gets one uploaded PDF, then staff send it.
//! Each recipient gets an expiring tokenized link (only the SHA-256 of the
//! token is stored). The signer verifies email with a one-time code,
//! consents to electronic records, adopts a typed or drawn signature, then
//! signs or declines. Routing order is enforced. When the last signer
//! finishes, NXT flattens the field values into the PDF, hashes it, and
//! builds a certificate of completion from the append-only, hash-chained
//! ceremony events (time, IP, user agent).
//!
//! Delivery is off by default: `NXT_ESIGN_DELIVERY` unset or `test` means
//! links and one-time codes are returned in-app and nothing is emailed. No
//! other mode is implemented, so real sends are impossible from this code.
use super::*;
use crate::client_journey::{Actor, ClientJourney, JourneyDef, StepStatus};
use base64::Engine as _;
use lopdf::{Dictionary, Object, Stream, dictionary};
use serde::{Deserialize, Serialize};

pub const PDF_MAX: usize = 3 * 1024 * 1024; // base64 must fit the 4.5 MB Vercel request limit
const OTP_TTL_SECS: i64 = 600;
const OTP_ATTEMPTS: u8 = 5;
pub const CONSENT_VERSION: &str = "nxt-erecords-consent/v1";
pub const CONSENT_TEXT: &str = "I agree to use electronic records and signatures for this document. I can ask for a paper copy, and I can withdraw consent before signing by declining. My electronic signature has the same effect as my handwritten signature.";

fn esign_key(id: &str) -> String { format!("signatures:esign:{id}") }
fn pdf_key(sha: &str) -> String { format!("signatures:pdf:{sha}") }
fn token_key(token_hash: &str) -> String { format!("signatures:esign-token:{token_hash}") }
fn sha(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }
fn gone(m: &str) -> ApiError { ApiError(StatusCode::GONE, m.into()) }
fn forbidden(m: &str) -> ApiError { ApiError(StatusCode::FORBIDDEN, m.into()) }
fn b64() -> base64::engine::GeneralPurpose { base64::engine::general_purpose::STANDARD }
fn secret() -> String { format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()) }

const OTP_COOLDOWN_SECS: i64 = 60;
const OTP_MAX_SENDS: u8 = 5;
pub const BULK_MAX: usize = 25;
fn outbox_key(id: &str) -> String { format!("signatures:esign-outbox:{id}") }

/// Delivery mode. `test` (default) shows links and codes in-app. `outbox`
/// renders each email into a staff-visible outbox for a provider or a person
/// to send; NXT itself never transmits. Anything else is refused.
pub fn delivery_mode() -> SResult<&'static str> {
    match std::env::var("NXT_ESIGN_DELIVERY").ok().as_deref().map(str::trim) {
        None | Some("") | Some("test") => Ok("test"),
        Some("outbox") => Ok("outbox"),
        Some(_) => Err(conflict("NXT does not send e-sign email itself; NXT_ESIGN_DELIVERY must be unset, 'test' or 'outbox'")),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientMeta { pub ip: String, pub user_agent: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CeremonyEvent {
    pub seq: u64,
    pub at: String,
    pub action: String,
    pub actor: String,
    pub ip: String,
    pub user_agent: String,
    pub detail: String,
    pub previous_hash: Option<String>,
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Adoption { pub kind: String, pub text: Option<String>, pub strokes: Option<Vec<Vec<[u16; 2]>>>, pub initials: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signer {
    pub recipient_id: String,
    pub name: String,
    pub email: String,
    pub kind: String,
    pub routing_order: u16,
    pub token_hash: String,
    pub expires_at: String,
    pub otp_hash: Option<String>,
    pub otp_expires_at: Option<String>,
    pub otp_attempts: u8,
    pub verified: bool,
    pub consented: bool,
    pub adoption: Option<Adoption>,
    pub status: String, // waiting | active | signed | declined
    pub values: Vec<FieldValue>,
    #[serde(default)]
    pub otp_sends: u8,
    #[serde(default)]
    pub otp_last_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ceremony {
    pub envelope_id: String,
    pub title: String,
    pub status: String, // prepared | sent | completed | declined | voided
    pub template_key: Option<String>,
    pub document_id: Option<String>,
    pub pdf_sha256: Option<String>,
    pub pdf_pages: usize,
    pub content_hash: String,
    pub delivery_mode: String,
    pub signers: Vec<Signer>,
    pub fields: Vec<Field>,
    pub events: Vec<CeremonyEvent>,
    pub signed_pdf_sha256: Option<String>,
    pub certificate_pdf_sha256: Option<String>,
    pub completed_at: Option<String>,
}

impl Ceremony {
    fn push(&mut self, action: &str, actor: &str, meta: &ClientMeta, detail: &str) -> SResult<()> {
        let previous_hash = self.events.last().map(|e| e.hash.clone());
        let mut e = CeremonyEvent {
            seq: self.events.len() as u64 + 1, at: now(), action: action.into(), actor: actor.into(),
            ip: clip(&meta.ip, 64), user_agent: clip(&meta.user_agent, 300), detail: detail.into(),
            previous_hash, hash: String::new(),
        };
        e.hash = hash(&(&e.seq, &e.at, &e.action, &e.actor, &e.ip, &e.user_agent, &e.detail, &e.previous_hash, &self.envelope_id))?;
        self.events.push(e);
        Ok(())
    }
    /// Recomputes every event hash and link.
    pub fn verify_chain(&self) -> bool {
        let mut prev: Option<String> = None;
        for (i, e) in self.events.iter().enumerate() {
            let h = hash(&(&e.seq, &e.at, &e.action, &e.actor, &e.ip, &e.user_agent, &e.detail, &e.previous_hash, &self.envelope_id));
            if e.seq != i as u64 + 1 || e.previous_hash != prev || h.ok().as_deref() != Some(e.hash.as_str()) { return false; }
            prev = Some(e.hash.clone());
        }
        true
    }
    fn public(&self) -> Value {
        json!({"envelope_id":self.envelope_id,"title":self.title,"status":self.status,"template_key":self.template_key,
            "pdf_sha256":self.pdf_sha256,"pdf_pages":self.pdf_pages,"delivery_mode":self.delivery_mode,
            "signers":self.signers.iter().map(|s| json!({"recipient_id":s.recipient_id,"name":s.name,"email":s.email,"routing_order":s.routing_order,"status":s.status,"verified":s.verified,"consented":s.consented,"expires_at":s.expires_at})).collect::<Vec<_>>(),
            "events":self.events,"audit_chain_valid":self.verify_chain(),
            "signed_pdf_sha256":self.signed_pdf_sha256,"certificate_pdf_sha256":self.certificate_pdf_sha256,"completed_at":self.completed_at,
            "external_email_sent":false})
    }
    fn active_order(&self) -> Option<u16> {
        self.signers.iter().filter(|s| s.kind == "signer" && s.status != "signed").map(|s| s.routing_order).min()
    }
}

fn clip(v: &str, n: usize) -> String { v.chars().filter(|c| !c.is_control()).take(n).collect() }

// ---------- built-in templates ----------

fn field(id: &str, rid: &str, page: usize, kind: &str, label: &str, x: u16, y: u16, w: u16, h: u16) -> Field {
    Field { id: id.into(), document_id: "doc".into(), recipient_id: rid.into(), page, kind: kind.into(), label: label.into(), required: true, x, y, width: w, height: h }
}
fn recipient(id: &str, role: &str, order: u16) -> Recipient {
    Recipient { id: id.into(), role_name: role.into(), name: String::new(), email: String::new(), kind: "signer".into(), routing_order: order }
}

/// Recurring documents. Field boxes are basis points of the page and can be
/// moved after instantiation; the PDF (the firm letter or the official CRA
/// form) is uploaded per envelope.
pub fn builtin(key: &str) -> Option<(Draft, &'static str)> {
    let doc = |name: &str, pages: usize| Document { id: "doc".into(), name: name.into(), pages: (1..=pages).map(|p| format!("Page {p}: upload the PDF for this envelope before sending.")).collect() };
    let base = |title: &str, subject: &str, docs, recipients, fields| Draft { title: title.into(), email_subject: subject.into(), email_message: String::new(), context: None, documents: vec![docs], recipients, fields };
    Some(match key {
        "engagement_letter" => (base("Engagement letter", "Please review and sign your engagement letter", doc("Engagement letter", 1),
            vec![recipient("client", "Client", 1), recipient("professional", "Responsible professional", 2)],
            vec![field("client_sig", "client", 1, "signature", "Client signature", 800, 7600, 3500, 700),
                 field("client_name", "client", 1, "name", "Client name", 800, 8400, 3500, 400),
                 field("client_date", "client", 1, "date", "Date", 4800, 7800, 1800, 400),
                 field("pro_sig", "professional", 1, "signature", "Professional signature", 800, 9000, 3500, 600),
                 field("pro_date", "professional", 1, "date", "Date", 4800, 9100, 1800, 400)]), "engagement_letter"),
        "t183" => (base("T183 Information return for electronic filing of an individual's return", "Please sign your T183 authorization", doc("T183", 2),
            vec![recipient("taxpayer", "Taxpayer", 1)],
            vec![field("tp_sig", "taxpayer", 2, "signature", "Taxpayer signature", 900, 6500, 3500, 600),
                 field("tp_date", "taxpayer", 2, "date", "Date", 5200, 6600, 1800, 400)]), "filing_authorization"),
        "t183corp" => (base("T183CORP Information return for corporations filing electronically", "Please sign the T183CORP authorization", doc("T183CORP", 2),
            vec![recipient("officer", "Authorized signing officer", 1)],
            vec![field("off_sig", "officer", 2, "signature", "Signing officer signature", 900, 6800, 3500, 600),
                 field("off_name", "officer", 2, "name", "Officer name", 900, 6200, 3500, 400),
                 field("off_title", "officer", 2, "text", "Position or title", 5200, 6200, 2800, 400),
                 field("off_date", "officer", 2, "date", "Date", 5200, 6900, 1800, 400)]), "filing_authorization"),
        _ => return None,
    })
}
pub const BUILTIN_KEYS: [&str; 3] = ["engagement_letter", "t183", "t183corp"];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinInput { pub operator_label: String, pub title: Option<String>, pub recipients: Vec<RecipientIdentity>, #[serde(default)] pub context: Option<ContextLink> }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PdfInput { pub operator_label: String, pub expected_revision: u64, pub expected_content_hash: String, pub document_id: String, pub pdf_base64: String }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendInput { pub operator_label: String, pub expected_revision: u64, pub expected_content_hash: String, #[serde(default)] pub expires_days: Option<u16> }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepInput {
    pub operator_label: String,
    /// Reissue a fresh link to active signers whose last link is older than this (default 72h).
    #[serde(default)] pub remind_after_hours: Option<u32>,
    /// Also reissue fresh links where the link has expired (default: report only).
    #[serde(default)] pub reissue_expired: bool,
    /// Report what would happen without changing anything.
    #[serde(default)] pub dry_run: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffInput { pub operator_label: String, pub reason: String }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkInput { pub operator_label: String, pub recipient_id: String }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeInput { pub code: String }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentInput { pub accept: bool, pub consent_version: String }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptInput { pub kind: String, #[serde(default)] pub text: Option<String>, #[serde(default)] pub strokes: Option<Vec<Vec<[u16; 2]>>>, pub initials: String }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignInput { #[serde(default)] pub values: Vec<FieldValue> }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclineInput { pub reason: String }

impl Signatures {
    async fn ceremony(&self, id: &str) -> SResult<Option<(String, Ceremony)>> {
        match self.store.get(&esign_key(id)).await.map_err(storage_error)? {
            Some(raw) => { let c = serde_json::from_str(&raw).map_err(|_| storage_error(String::new()))?; Ok(Some((raw, c))) }
            None => Ok(None),
        }
    }
    /// CAS write; refuses any change that rewrites earlier events.
    async fn put_ceremony(&self, old_raw: Option<&str>, old: Option<&Ceremony>, new: &Ceremony) -> SResult<()> {
        if let Some(old) = old {
            if new.events.len() < old.events.len() || new.events[..old.events.len()].iter().zip(&old.events).any(|(a, b)| a.hash != b.hash) {
                return Err(storage_error(String::new()));
            }
        }
        let enc = serde_json::to_string(new).map_err(|_| bad("cannot encode ceremony"))?;
        if !self.store.compare_exchange(&esign_key(&new.envelope_id), old_raw, &enc).await.map_err(storage_error)? {
            return Err(conflict("e-sign envelope changed; reload before retrying"));
        }
        Ok(())
    }
    async fn envelope(&self, id: &str) -> SResult<Envelope> {
        let (_, state) = self.read().await?;
        state.envelopes.into_iter().find(|r| r.id == id).ok_or_else(missing)
    }

    pub fn esign_catalog() -> Value {
        let templates: Vec<Value> = BUILTIN_KEYS.iter().filter_map(|k| builtin(k).map(|(d, step)| json!({"key":k,"title":d.title,"journey_step":step,
            "recipients":d.recipients.iter().map(|r| json!({"id":r.id,"role_name":r.role_name,"routing_order":r.routing_order})).collect::<Vec<_>>(),
            "pages":d.documents[0].pages.len()}))).collect();
        json!({"builtin_templates":BUILTIN_KEYS,"templates":templates,"delivery_mode":delivery_mode().unwrap_or("misconfigured"),"external_email":false,
            "identity_check":"email one-time code (shown in-app in test mode)","consent_version":CONSENT_VERSION,"consent_text":CONSENT_TEXT,
            "adoption":["typed","drawn"],"field_kinds":["signature","initials","name","date","text","checkbox"],
            "legal_signoff":"needed, not claimed; see docs/ESIGN_LEGAL_BASIS.md"})
    }

    pub async fn instantiate_builtin(&self, key: &str, input: BuiltinInput) -> SResult<Value> {
        let (mut draft, _) = builtin(key).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "unknown built-in template".into()))?;
        if let Some(t) = input.title { draft.title = t; }
        draft.context = input.context;
        for r in &mut draft.recipients {
            let given = input.recipients.iter().find(|g| g.id == r.id).ok_or_else(|| bad(format!("recipient {} is required", r.id)))?;
            r.name = given.name.trim().into(); r.email = given.email.trim().into();
        }
        let created = self.save(None, SaveInput { expected_revision: None, operator_label: input.operator_label.clone(), draft }).await?;
        let id = created["envelope"]["id"].as_str().or_else(|| created["id"].as_str()).ok_or_else(|| storage_error(String::new()))?.to_string();
        let env = self.envelope(&id).await?;
        let mut c = Ceremony { envelope_id: id.clone(), title: env.draft.title.clone(), status: "prepared".into(), template_key: Some(key.into()),
            document_id: None, pdf_sha256: None, pdf_pages: 0, content_hash: env.content_hash.clone(), delivery_mode: "test".into(),
            signers: vec![], fields: vec![], events: vec![], signed_pdf_sha256: None, certificate_pdf_sha256: None, completed_at: None };
        c.push("template_instantiated", &input.operator_label, &ClientMeta { ip: "staff".into(), user_agent: "staff".into() }, key)?;
        self.put_ceremony(None, None, &c).await?;
        Ok(json!({"envelope":created,"esign":c.public()}))
    }

    pub async fn attach_pdf(&self, id: &str, input: PdfInput, meta: ClientMeta) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) { return Err(bad("operator label is required")); }
        let bytes = b64().decode(input.pdf_base64.trim()).map_err(|_| bad("pdf_base64 is not base64"))?;
        if bytes.len() > PDF_MAX || !bytes.starts_with(b"%PDF-") { return Err(bad("upload a PDF up to 3 MiB")); }
        let doc = lopdf::Document::load_mem(&bytes).map_err(|_| bad("PDF could not be parsed"))?;
        if doc.is_encrypted() { return Err(bad("encrypted PDFs are not supported")); }
        let pages = doc.get_pages().len();
        let env = self.envelope(id).await?;
        check(&env, input.expected_revision, Some(&input.expected_content_hash))?;
        if !env.draft.documents.iter().any(|d| d.id == input.document_id) { return Err(bad("unknown document_id")); }
        if env.draft.fields.iter().any(|f| f.document_id == input.document_id && f.page > pages) { return Err(bad("a field is on a page the PDF does not have")); }
        let digest = sha(&bytes);
        self.store.set(&pdf_key(&digest), &b64().encode(&bytes)).await.map_err(storage_error)?;
        let old = self.ceremony(id).await?;
        let mut c = match &old { Some((_, c)) if c.status != "prepared" => return Err(conflict("PDF is locked once the envelope is sent")), Some((_, c)) => c.clone(),
            None => Ceremony { envelope_id: id.into(), title: env.draft.title.clone(), status: "prepared".into(), template_key: None, document_id: None, pdf_sha256: None, pdf_pages: 0, content_hash: env.content_hash.clone(), delivery_mode: "test".into(), signers: vec![], fields: vec![], events: vec![], signed_pdf_sha256: None, certificate_pdf_sha256: None, completed_at: None } };
        c.document_id = Some(input.document_id.clone()); c.pdf_sha256 = Some(digest.clone()); c.pdf_pages = pages; c.content_hash = env.content_hash.clone();
        c.push("pdf_uploaded", &input.operator_label, &meta, &format!("sha256 {digest}, {pages} page(s)"))?;
        self.put_ceremony(old.as_ref().map(|o| o.0.as_str()), old.as_ref().map(|o| &o.1), &c).await?;
        Ok(c.public())
    }

    pub async fn send(&self, id: &str, input: SendInput, meta: ClientMeta) -> SResult<Value> {
        let mode = delivery_mode()?;
        if !nonempty(&input.operator_label, 120) { return Err(bad("operator label is required")); }
        let env = self.envelope(id).await?;
        check(&env, input.expected_revision, Some(&input.expected_content_hash))?;
        if env.status != "approved" || env.reviewed_content_hash.as_deref() != Some(env.content_hash.as_str()) {
            return Err(conflict("only an approved, reviewed preparation can be sent"));
        }
        if !readiness(&env.draft).is_empty() { return Err(conflict("resolve readiness findings before sending")); }
        let (raw, old) = self.ceremony(id).await?.ok_or_else(|| conflict("upload the PDF before sending"))?;
        if old.status != "prepared" || old.pdf_sha256.is_none() { return Err(conflict("envelope already sent or has no PDF")); }
        if old.content_hash != env.content_hash { return Err(conflict("preparation changed after the PDF was attached; re-upload")); }
        let doc_id = old.document_id.clone().unwrap_or_default();
        let days = i64::from(input.expires_days.unwrap_or(7).clamp(1, 30));
        let expires = (Utc::now() + chrono::Duration::days(days)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let mut c = old.clone();
        c.fields = env.draft.fields.iter().filter(|f| f.document_id == doc_id).cloned().collect();
        let mut links = vec![];
        c.signers = vec![];
        for r in &env.draft.recipients {
            let token = secret();
            let th = sha(token.as_bytes());
            self.store.set(&token_key(&th), id).await.map_err(storage_error)?;
            c.signers.push(Signer { recipient_id: r.id.clone(), name: r.name.clone(), email: r.email.clone(), kind: r.kind.clone(), routing_order: r.routing_order,
                token_hash: th, expires_at: expires.clone(), otp_hash: None, otp_expires_at: None, otp_attempts: 0, verified: false, consented: false, adoption: None,
                status: "waiting".into(), values: vec![], otp_sends: 0, otp_last_at: None });
            links.push(json!({"recipient_id":r.id,"name":r.name,"url":format!("/sign/#{token}")}));
        }
        c.status = "sent".into(); c.delivery_mode = mode.into();
        activate(&mut c);
        c.push("sent", &input.operator_label, &meta, &format!("delivery {mode}; links expire {expires}; no email sent"))?;
        self.put_ceremony(Some(&raw), Some(&old), &c).await?;
        self.queue_links(&c, &links, "").await?;
        let shown = if mode == "test" { json!(links) } else { json!([]) };
        Ok(json!({"esign":c.public(),"delivery":{"mode":mode,"email_sent":false,"links":shown,"outbox_messages":if mode == "outbox" { links.len() } else { 0 }}}))
    }

    pub async fn reissue_link(&self, id: &str, input: LinkInput, meta: ClientMeta) -> SResult<Value> {
        let mode = delivery_mode()?;
        let (raw, old) = self.ceremony(id).await?.ok_or_else(missing)?;
        if old.status != "sent" { return Err(conflict("envelope is not out for signature")); }
        let mut c = old.clone();
        let s = c.signers.iter_mut().find(|s| s.recipient_id == input.recipient_id && s.status != "signed").ok_or_else(|| bad("unknown or finished recipient"))?;
        let token = secret(); let th = sha(token.as_bytes());
        self.store.set(&token_key(&th), id).await.map_err(storage_error)?;
        s.token_hash = th; s.verified = false; s.otp_hash = None; s.otp_attempts = 0; s.otp_sends = 0; s.otp_last_at = None;
        s.expires_at = (Utc::now() + chrono::Duration::days(7)).to_rfc3339_opts(SecondsFormat::Secs, true);
        c.push("link_reissued", &input.operator_label, &meta, &input.recipient_id)?;
        self.put_ceremony(Some(&raw), Some(&old), &c).await?;
        let links = vec![json!({"recipient_id":input.recipient_id,"url":format!("/sign/#{token}")})];
        self.queue_links(&c, &links, "Here is a new link. ").await?;
        Ok(json!({"delivery":{"mode":mode,"email_sent":false,"links":if mode == "test" { json!(links) } else { json!([]) }}}))
    }

    /// Expiry sweep + reminders without new infra: call from a cron or by hand. Reminders
    /// reissue a fresh link (old one stops working) and queue it in the outbox; nothing is emailed.
    pub async fn sweep(&self, input: SweepInput, meta: ClientMeta) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) { return Err(bad("operator label is required")); }
        let remind_after = chrono::Duration::hours(i64::from(input.remind_after_hours.unwrap_or(72).clamp(1, 24 * 30)));
        let (_, state) = self.read().await?;
        let now_t = Utc::now();
        let mut out = vec![];
        for env in state.envelopes.iter().take(500) {
            let Some((_, c)) = self.ceremony(&env.id).await? else { continue };
            if c.status != "sent" { continue; }
            for sg in c.signers.iter().filter(|s| s.status == "active") {
                let expired = chrono::DateTime::parse_from_rfc3339(&sg.expires_at).map(|t| t < now_t).unwrap_or(false);
                let last = c.events.iter().rev().find(|e| e.action == "sent" || (e.action == "link_reissued" && e.detail == sg.recipient_id))
                    .and_then(|e| chrono::DateTime::parse_from_rfc3339(&e.at).ok());
                let due = last.is_some_and(|t| now_t - t.with_timezone(&Utc) >= remind_after);
                let action = if expired { if input.reissue_expired { "reissue_expired" } else { "expired" } } else if due { "remind" } else { continue };
                let mut item = json!({"envelope_id":env.id,"recipient_id":sg.recipient_id,"action":action,"expires_at":sg.expires_at});
                if !input.dry_run && action != "expired" {
                    match self.reissue_link(&env.id, LinkInput { operator_label: input.operator_label.clone(), recipient_id: sg.recipient_id.clone() }, meta.clone()).await {
                        Ok(v) => { item["delivery"] = v["delivery"].clone(); }
                        Err(e) => { item["error"] = json!(e.1); }
                    }
                }
                out.push(item);
            }
        }
        Ok(json!({"dry_run":input.dry_run,"email_sent":false,"results":out}))
    }

    pub async fn void(&self, id: &str, input: StaffInput, meta: ClientMeta) -> SResult<Value> {
        if !nonempty(&input.operator_label, 120) || !nonempty(&input.reason, 2000) { return Err(bad("operator label and reason are required")); }
        let (raw, old) = self.ceremony(id).await?.ok_or_else(missing)?;
        if !matches!(old.status.as_str(), "prepared" | "sent") { return Err(conflict("only an open envelope can be voided")); }
        let mut c = old.clone(); c.status = "voided".into();
        c.push("voided", &input.operator_label, &meta, &input.reason)?;
        self.put_ceremony(Some(&raw), Some(&old), &c).await?;
        Ok(c.public())
    }

    pub async fn esign_status(&self, id: &str) -> SResult<Value> {
        Ok(self.ceremony(id).await?.ok_or_else(missing)?.1.public())
    }
    pub async fn ceremony_record(&self, id: &str) -> SResult<Ceremony> {
        Ok(self.ceremony(id).await?.ok_or_else(missing)?.1)
    }
    pub async fn pdf_bytes(&self, digest: &str) -> SResult<Vec<u8>> {
        let raw = self.store.get(&pdf_key(digest)).await.map_err(storage_error)?.ok_or_else(missing)?;
        let bytes = b64().decode(raw).map_err(|_| storage_error(String::new()))?;
        if sha(&bytes) != digest { return Err(storage_error(String::new())); }
        Ok(bytes)
    }
    pub async fn completed_pdf(&self, id: &str, which: &str) -> SResult<Vec<u8>> {
        let c = self.ceremony_record(id).await?;
        let d = match which { "signed" => c.signed_pdf_sha256, "certificate" => c.certificate_pdf_sha256, _ => c.pdf_sha256 };
        self.pdf_bytes(&d.ok_or_else(|| conflict("document not available yet"))?).await
    }

    // ---------- signer ceremony (token-authenticated, no staff key) ----------

    async fn by_token(&self, token: &str) -> SResult<(String, Ceremony, usize)> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(gone("this signing link is invalid or expired")); }
        let th = sha(token.as_bytes());
        let id = self.store.get(&token_key(&th)).await.map_err(storage_error)?.ok_or_else(|| gone("this signing link is invalid or expired"))?;
        let (raw, c) = self.ceremony(&id).await?.ok_or_else(|| gone("this signing link is invalid or expired"))?;
        let i = c.signers.iter().position(|s| s.token_hash == th).ok_or_else(|| gone("this signing link was replaced"))?;
        if c.signers[i].expires_at.as_str() < now().as_str() { return Err(gone("this signing link has expired; ask the firm for a new one")); }
        Ok((raw, c, i))
    }
    fn signer_view(c: &Ceremony, i: usize) -> Value {
        let s = &c.signers[i];
        let step = match () {
            _ if c.status != "sent" => c.status.as_str(),
            _ if s.status == "signed" => "signed",
            _ if s.status == "declined" => "declined",
            _ if s.status != "active" => "waiting_for_others",
            _ if !s.verified => "verify_email",
            _ if !s.consented => "consent",
            _ if s.adoption.is_none() => "adopt",
            _ => "sign",
        };
        let masked = s.email.split_once('@').map(|(l, d)| format!("{}***@{d}", l.chars().next().unwrap_or('*'))).unwrap_or_default();
        json!({"title":c.title,"envelope_id":c.envelope_id,"signer":{"name":s.name,"email":masked,"recipient_id":s.recipient_id},"step":step,
            "consent":{"version":CONSENT_VERSION,"text":CONSENT_TEXT},"pdf_pages":c.pdf_pages,
            "fields": if s.verified { json!(c.fields.iter().filter(|f| f.recipient_id == s.recipient_id).collect::<Vec<_>>()) } else { json!([]) },
            "delivery_mode":c.delivery_mode})
    }
    pub async fn signer_get(&self, token: &str) -> SResult<Value> {
        let (_, c, i) = self.by_token(token).await?;
        Ok(Self::signer_view(&c, i))
    }
    async fn signer_mutate(&self, token: &str, f: impl FnOnce(&mut Ceremony, usize) -> SResult<Value>) -> SResult<Value> {
        let (raw, old, i) = self.by_token(token).await?;
        let mut c = old.clone();
        let extra = f(&mut c, i)?;
        self.put_ceremony(Some(&raw), Some(&old), &c).await?;
        let mut v = Self::signer_view(&c, i);
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) { o.extend(e.clone()); }
        Ok(v)
    }
    fn require_active(c: &Ceremony, i: usize) -> SResult<()> {
        if c.status != "sent" { return Err(conflict("this envelope is no longer open for signing")); }
        if c.signers[i].status != "active" { return Err(conflict("it is not this recipient's turn to sign")); }
        Ok(())
    }
    pub async fn otp_request(&self, token: &str, meta: ClientMeta) -> SResult<Value> {
        let code = { let u = uuid::Uuid::new_v4(); let b = u.as_bytes(); format!("{:06}", u32::from_le_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000) };
        let r = self.signer_mutate(token, |c, i| {
            Self::require_active(c, i)?;
            let s = &mut c.signers[i];
            s.otp_hash = Some(sha(format!("{}:{code}", s.token_hash).as_bytes()));
            s.otp_expires_at = Some((Utc::now() + chrono::Duration::seconds(OTP_TTL_SECS)).to_rfc3339_opts(SecondsFormat::Secs, true));
            if s.otp_sends >= OTP_MAX_SENDS { return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "too many codes for this link; ask the sender for a new link".into())); }
            if let Some(last) = s.otp_last_at.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
                if (Utc::now() - last.with_timezone(&Utc)).num_seconds() < OTP_COOLDOWN_SECS { return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "wait a minute before asking for another code".into())); }
            }
            s.otp_attempts = 0; s.otp_sends += 1; s.otp_last_at = Some(now());
            let (rid, name, email) = (s.recipient_id.clone(), s.name.clone(), s.email.clone());
            c.push("otp_requested", &rid, &meta, &format!("one-time code issued by {} delivery; no email sent by NXT", c.delivery_mode))?;
            if c.delivery_mode == "outbox" {
                Ok(json!({"code_delivery":"queued in the sender's outbox","_outbox":{"to_name":name,"to_email":email,"subject":format!("Your code for {}", c.title),"body":format!("Your one-time code is {code}. It expires in 10 minutes. If you didn't ask for it, ignore this message.")}}))
            } else {
                Ok(json!({"test_code":code,"code_delivery":"shown in-app (test mode); no email sent"}))
            }
        }).await?;
        let mut v = r;
        if let Some(m) = v.as_object_mut().and_then(|o| o.remove("_outbox")) {
            let id = v["envelope_id"].as_str().unwrap_or("").to_string();
            self.outbox_push(&id, vec![m]).await?;
        }
        Ok(v)
    }
    async fn outbox_push(&self, id: &str, msgs: Vec<Value>) -> SResult<()> {
        for _ in 0..8 {
            let old = self.store.get(&outbox_key(id)).await.map_err(storage_error)?;
            let mut list: Vec<Value> = old.as_deref().and_then(|o| serde_json::from_str(o).ok()).unwrap_or_default();
            for mut m in msgs.clone() { m["queued_at"] = json!(now()); m["sent_by_nxt"] = json!(false); list.push(m); }
            let new = serde_json::to_string(&list).map_err(|e| storage_error(e.to_string()))?;
            if self.store.compare_exchange(&outbox_key(id), old.as_deref(), &new).await.map_err(storage_error)? { return Ok(()); }
        }
        Err(conflict("outbox busy; retry"))
    }
    /// Staff view of messages waiting for a provider or a person to send.
    pub async fn outbox(&self, id: &str) -> SResult<Value> {
        let raw = self.store.get(&outbox_key(id)).await.map_err(storage_error)?;
        let list: Value = raw.as_deref().and_then(|o| serde_json::from_str(o).ok()).unwrap_or(json!([]));
        Ok(json!({"envelope_id":id,"messages":list,"sent_by_nxt":false}))
    }
    async fn queue_links(&self, c: &Ceremony, links: &[Value], origin_note: &str) -> SResult<()> {
        if c.delivery_mode != "outbox" { return Ok(()); }
        let msgs = links.iter().filter_map(|l| c.signers.iter().find(|s| l["recipient_id"] == json!(s.recipient_id)).map(|s| json!({"to_name":s.name,"to_email":s.email,
            "subject":format!("Please sign: {}", c.title),"body":format!("{origin_note}Open this private link to review and sign: {{ORIGIN}}{} . It expires {}.", l["url"].as_str().unwrap_or(""), s.expires_at)}))).collect();
        self.outbox_push(&c.envelope_id, msgs).await
    }
    pub async fn otp_verify(&self, token: &str, input: CodeInput, meta: ClientMeta) -> SResult<Value> {
        let r = self.signer_mutate(token, |c, i| {
            Self::require_active(c, i)?;
            let s = &mut c.signers[i];
            let (Some(h), Some(exp)) = (s.otp_hash.clone(), s.otp_expires_at.clone()) else { return Err(conflict("request a code first")) };
            if exp.as_str() < now().as_str() || s.otp_attempts >= OTP_ATTEMPTS { s.otp_hash = None; return Err(forbidden("code expired or too many attempts; request a new code")); }
            let ok = sha(format!("{}:{}", s.token_hash, input.code.trim()).as_bytes()) == h;
            let rid = s.recipient_id.clone();
            if ok { s.verified = true; s.otp_hash = None; } else { s.otp_attempts += 1; }
            c.push(if ok { "identity_verified" } else { "otp_failed" }, &rid, &meta, "email one-time code")?;
            Ok(json!({"verified":ok}))
        }).await?;
        if r["verified"] == false { return Err(forbidden("incorrect code")); }
        Ok(r)
    }
    pub async fn consent(&self, token: &str, input: ConsentInput, meta: ClientMeta) -> SResult<Value> {
        if !input.accept || input.consent_version != CONSENT_VERSION { return Err(bad("consent to the current electronic records disclosure is required")); }
        self.signer_mutate(token, |c, i| {
            Self::require_active(c, i)?;
            if !c.signers[i].verified { return Err(forbidden("verify your email first")); }
            c.signers[i].consented = true;
            let rid = c.signers[i].recipient_id.clone();
            c.push("consented", &rid, &meta, CONSENT_VERSION)?;
            Ok(json!({}))
        }).await
    }
    pub async fn adopt(&self, token: &str, input: AdoptInput, meta: ClientMeta) -> SResult<Value> {
        let a = match input.kind.as_str() {
            "typed" => { let t = input.text.as_deref().unwrap_or("").trim(); if !nonempty(t, 120) || !latin(t) { return Err(bad("type your full name")); } Adoption { kind: "typed".into(), text: Some(t.into()), strokes: None, initials: String::new() } }
            "drawn" => { let s = input.strokes.unwrap_or_default(); let pts: usize = s.iter().map(Vec::len).sum();
                if s.is_empty() || s.len() > 200 || pts < 2 || pts > 5000 || s.iter().flatten().any(|p| p[0] > 1000 || p[1] > 1000) { return Err(bad("draw your signature")); }
                Adoption { kind: "drawn".into(), text: None, strokes: Some(s), initials: String::new() } }
            _ => return Err(bad("kind must be typed or drawn")),
        };
        let initials = input.initials.trim().to_string();
        if !nonempty(&initials, 10) || !latin(&initials) { return Err(bad("initials are required")); }
        self.signer_mutate(token, |c, i| {
            Self::require_active(c, i)?;
            if !c.signers[i].consented { return Err(forbidden("consent first")); }
            let kind = a.kind.clone();
            c.signers[i].adoption = Some(Adoption { initials, ..a });
            let rid = c.signers[i].recipient_id.clone();
            c.push("signature_adopted", &rid, &meta, &kind)?;
            Ok(json!({}))
        }).await
    }
    pub async fn decline(&self, token: &str, input: DeclineInput, meta: ClientMeta) -> SResult<Value> {
        if !nonempty(&input.reason, 2000) { return Err(bad("a reason is required")); }
        self.signer_mutate(token, |c, i| {
            Self::require_active(c, i)?;
            if !c.signers[i].verified { return Err(forbidden("verify your email first")); }
            c.signers[i].status = "declined".into(); c.status = "declined".into();
            let rid = c.signers[i].recipient_id.clone();
            c.push("declined", &rid, &meta, &clip(&input.reason, 2000))?;
            Ok(json!({}))
        }).await
    }
    pub async fn signer_pdf(&self, token: &str) -> SResult<Vec<u8>> {
        let (_, c, i) = self.by_token(token).await?;
        if !c.signers[i].verified { return Err(forbidden("verify your email first")); }
        let d = if c.status == "completed" { c.signed_pdf_sha256.clone() } else { c.pdf_sha256.clone() };
        self.pdf_bytes(&d.ok_or_else(missing)?).await
    }
    pub async fn sign(&self, token: &str, input: SignInput, meta: ClientMeta) -> SResult<Value> {
        let (raw, old, i) = self.by_token(token).await?;
        Self::require_active(&old, i)?;
        let mut c = old.clone();
        let s = c.signers[i].clone();
        if !s.verified || !s.consented || s.adoption.is_none() { return Err(forbidden("verify, consent and adopt a signature first")); }
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut values = vec![];
        for f in c.fields.iter().filter(|f| f.recipient_id == s.recipient_id) {
            let given = input.values.iter().find(|v| v.field_id == f.id).map(|v| v.value.trim().to_string());
            let v = match f.kind.as_str() {
                "signature" | "initials" => "adopted".to_string(),
                "date" => today.clone(),
                "name" => given.filter(|g| !g.is_empty()).unwrap_or_else(|| s.name.clone()),
                "checkbox" => match given.as_deref() { Some("true") => "true".into(), _ if f.required => return Err(bad(format!("{} must be checked", f.label))), _ => "false".into() },
                _ => given.unwrap_or_default(),
            };
            if f.required && v.is_empty() { return Err(bad(format!("{} is required", f.label))); }
            if !label(&v, 200, false) || !latin(&v) { return Err(bad(format!("{} has unsupported characters", f.label))); }
            values.push(FieldValue { field_id: f.id.clone(), value: v });
        }
        if input.values.iter().any(|v| !c.fields.iter().any(|f| f.id == v.field_id && f.recipient_id == s.recipient_id)) { return Err(bad("unknown field")); }
        c.signers[i].values = values; c.signers[i].status = "signed".into();
        c.push("signed", &s.recipient_id, &meta, &format!("{} field(s)", c.signers[i].values.len()))?;
        activate(&mut c);
        if c.active_order().is_none() {
            let original = self.pdf_bytes(c.pdf_sha256.as_deref().unwrap_or("")).await?;
            let signed = flatten(&original, &c)?;
            let ssha = sha(&signed);
            self.store.set(&pdf_key(&ssha), &b64().encode(&signed)).await.map_err(storage_error)?;
            c.signed_pdf_sha256 = Some(ssha.clone());
            c.status = "completed".into(); c.completed_at = Some(now());
            c.push("completed", "system", &ClientMeta { ip: "server".into(), user_agent: "nxt-os".into() }, &format!("signed PDF sha256 {ssha}"))?;
            let cert = certificate_pdf(&c)?;
            let csha = sha(&cert);
            self.store.set(&pdf_key(&csha), &b64().encode(&cert)).await.map_err(storage_error)?;
            c.certificate_pdf_sha256 = Some(csha);
        }
        self.put_ceremony(Some(&raw), Some(&old), &c).await?;
        Ok(Self::signer_view(&c, i))
    }
}

fn activate(c: &mut Ceremony) {
    if let Some(o) = c.active_order() {
        for s in &mut c.signers { if s.kind == "signer" && s.routing_order == o && s.status == "waiting" { s.status = "active".into(); } }
    }
}
fn latin(v: &str) -> bool { v.chars().all(|c| (' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c)) }
fn pdf_text(v: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for ch in v.chars() { let b = if (ch as u32) < 256 { ch as u32 as u8 } else { b'?' }; if matches!(b, b'(' | b')' | b'\\') { out.push(b'\\'); } out.push(b); }
    out.push(b')'); out
}
fn num(v: f32) -> String { format!("{v:.2}") }

fn media_box(doc: &lopdf::Document, mut id: lopdf::ObjectId) -> [f32; 4] {
    for _ in 0..32 {
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(Object::Array(a)) = d.get(b"MediaBox").and_then(|o| match o { Object::Reference(r) => doc.get_object(*r), o => Ok(o) }) {
            let v: Vec<f32> = a.iter().filter_map(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|i| i as f32))).collect();
            if v.len() == 4 { return [v[0], v[1], v[2], v[3]]; }
        }
        match d.get(b"Parent").and_then(Object::as_reference) { Ok(p) => id = p, Err(_) => break }
    }
    [0.0, 0.0, 612.0, 792.0]
}
fn inherited_resources(doc: &lopdf::Document, page: lopdf::ObjectId) -> Dictionary {
    let mut id = page;
    for _ in 0..32 {
        let Ok(d) = doc.get_dictionary(id) else { break };
        if let Ok(r) = d.get(b"Resources") {
            return match r { Object::Reference(x) => doc.get_dictionary(*x).cloned().unwrap_or_default(), Object::Dictionary(x) => x.clone(), _ => Dictionary::new() };
        }
        match d.get(b"Parent").and_then(Object::as_reference) { Ok(p) => id = p, Err(_) => break }
    }
    Dictionary::new()
}

/// Draws every field value into page content, removes form widgets and the
/// AcroForm, and stamps an envelope footer. Output has no fillable fields.
pub fn flatten(pdf: &[u8], c: &Ceremony) -> SResult<Vec<u8>> {
    let err = |_| bad("PDF could not be flattened");
    let mut doc = lopdf::Document::load_mem(pdf).map_err(err)?;
    let font = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
    let fontb = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Oblique", "Encoding" => "WinAnsiEncoding"});
    let pages = doc.get_pages();
    for (&n, &pid) in &pages {
        let [llx, lly, urx, ury] = media_box(&doc, pid);
        let (w, h) = (urx - llx, ury - lly);
        let mut ops = String::from("Q q\n");
        for s in &c.signers {
            for v in &s.values {
                let Some(f) = c.fields.iter().find(|f| f.id == v.field_id && f.page == n as usize) else { continue };
                let x = llx + w * f32::from(f.x) / 10000.0;
                let fh = h * f32::from(f.height) / 10000.0;
                let fw = w * f32::from(f.width) / 10000.0;
                let y = ury - h * f32::from(f.y) / 10000.0 - fh;
                let a = s.adoption.as_ref();
                match (f.kind.as_str(), a) {
                    ("signature", Some(Adoption { kind, strokes: Some(st), .. })) if kind == "drawn" => {
                        ops.push_str("0 0 0.55 RG 1.2 w 1 J 1 j\n");
                        for line in st { for (k, p) in line.iter().enumerate() {
                            let px = x + fw * f32::from(p[0]) / 1000.0; let py = y + fh - fh * f32::from(p[1]) / 1000.0;
                            ops.push_str(&format!("{} {} {}\n", num(px), num(py), if k == 0 { "m" } else { "l" }));
                        } ops.push_str("S\n"); }
                    }
                    (kind, _) => {
                        let text = match kind { "signature" => a.and_then(|a| a.text.clone()).unwrap_or_default(), "initials" => a.map(|a| a.initials.clone()).unwrap_or_default(), "checkbox" => if v.value == "true" { "X".into() } else { String::new() }, _ => v.value.clone() };
                        let fname = if matches!(kind, "signature" | "initials") { "NXTS" } else { "NXTF" };
                        let size = (fh * 0.7).clamp(6.0, 22.0);
                        ops.push_str(&format!("BT 0 0 0.55 rg /{fname} {} Tf {} {} Td {} Tj ET\n", num(size), num(x + 1.0), num(y + fh * 0.2), String::from_utf8_lossy(&pdf_text(&text))));
                    }
                }
            }
        }
        ops.push_str(&format!("BT 0.3 0.3 0.3 rg /NXTF 6 Tf {} {} Td {} Tj ET\n", num(llx + 18.0), num(lly + 10.0),
            String::from_utf8_lossy(&pdf_text(&format!("NXT e-sign envelope {} - page {n} of {}", c.envelope_id, pages.len())))));
        let mut res = inherited_resources(&doc, pid);
        let mut fonts = match res.get(b"Font") { Ok(Object::Dictionary(d)) => d.clone(), Ok(Object::Reference(r)) => doc.get_dictionary(*r).cloned().unwrap_or_default(), _ => Dictionary::new() };
        fonts.set("NXTF", Object::Reference(font)); fonts.set("NXTS", Object::Reference(fontb));
        res.set("Font", Object::Dictionary(fonts));
        let pre = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let post = doc.add_object(Stream::new(Dictionary::new(), ops.into_bytes()));
        let existing: Vec<Object> = doc.get_page_contents(pid).into_iter().map(Object::Reference).collect();
        let page = doc.get_object_mut(pid).and_then(Object::as_dict_mut).map_err(err)?;
        let mut contents = vec![Object::Reference(pre)]; contents.extend(existing); contents.push(Object::Reference(post));
        page.set("Contents", contents);
        page.set("Resources", Object::Dictionary(res));
        page.remove(b"Annots");
    }
    if let Ok(cat) = doc.catalog_mut() { cat.remove(b"AcroForm"); }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|_| bad("PDF could not be written"))?;
    Ok(out)
}

/// Certificate of completion: envelope, document hashes, signers, and every
/// ceremony event with time (UTC), IP and user agent.
pub fn certificate_pdf(c: &Ceremony) -> SResult<Vec<u8>> {
    let mut lines: Vec<(f32, String)> = vec![(14.0, "Certificate of completion - NXT e-sign".into()), (9.0, String::new()),
        (9.0, format!("Envelope: {}", c.envelope_id)), (9.0, format!("Title: {}", clip(&c.title, 90))),
        (9.0, format!("Status: {}  Completed (UTC): {}", c.status, c.completed_at.clone().unwrap_or_default())),
        (9.0, format!("Original PDF SHA-256: {}", c.pdf_sha256.clone().unwrap_or_default())),
        (9.0, format!("Signed PDF SHA-256:   {}", c.signed_pdf_sha256.clone().unwrap_or_default())),
        (9.0, format!("Delivery mode: {} (no external email)  Audit chain valid: {}", c.delivery_mode, c.verify_chain())),
        (9.0, String::new()), (11.0, "Signers".into())];
    for s in &c.signers {
        lines.push((8.0, format!("{} <{}>  order {}  {}  identity: email one-time code{}  adoption: {}", clip(&s.name, 40), clip(&s.email, 60), s.routing_order, s.status,
            if s.verified { " (verified)" } else { "" }, s.adoption.as_ref().map(|a| a.kind.as_str()).unwrap_or("-"))));
    }
    lines.push((9.0, String::new())); lines.push((11.0, "Events".into()));
    for e in &c.events {
        lines.push((7.0, format!("#{} {}  {}  by {}  IP {}", e.seq, e.at, e.action, clip(&e.actor, 40), e.ip)));
        lines.push((6.0, format!("    UA: {}", clip(&e.user_agent, 120))));
        if !e.detail.is_empty() { lines.push((6.0, format!("    {}", clip(&e.detail, 130)))); }
        lines.push((6.0, format!("    hash {}", e.hash)));
    }
    let mut doc = lopdf::Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier", "Encoding" => "WinAnsiEncoding"});
    let resources = doc.add_object(dictionary! {"Font" => dictionary! {"F1" => font}});
    let mut kids = vec![];
    for chunk in lines.chunks(70) {
        let mut ops = String::new(); let mut y = 760.0;
        for (size, text) in chunk {
            ops.push_str(&format!("BT /F1 {} Tf 40 {} Td {} Tj ET\n", num(*size), num(y), String::from_utf8_lossy(&pdf_text(text))));
            y -= size + 3.0;
        }
        let content = doc.add_object(Stream::new(Dictionary::new(), ops.into_bytes()));
        kids.push(Object::Reference(doc.add_object(dictionary! {"Type" => "Page", "Parent" => pages_id, "Contents" => content, "Resources" => resources, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()]})));
    }
    let count = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => count}));
    let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|_| bad("certificate could not be written"))?;
    Ok(out)
}

// ---------- client journey hook ----------

/// Journey step a built-in template satisfies.
pub fn journey_step_for(template_key: &str) -> Option<&'static str> {
    builtin(template_key).map(|(_, step)| step)
}

/// Completes a human gate (engagement_letter or filing_authorization) from
/// a completed ceremony. The approver must still be a human; the envelope is
/// the evidence recorded in the journey ledger.
pub fn approve_journey_gate(journey: &mut ClientJourney, def: &JourneyDef, c: &Ceremony, approver: Actor, at: &str) -> SResult<()> {
    let step = c.template_key.as_deref().and_then(journey_step_for).ok_or_else(|| conflict("envelope is not an engagement letter or T183 template"))?;
    if c.status != "completed" || !c.verify_chain() { return Err(conflict("envelope is not completed with a valid audit chain")); }
    if journey.status_of(step) != Some(StepStatus::AwaitingApproval) { return Err(conflict("journey step is not awaiting approval")); }
    let note = format!("e-sign envelope {} completed; signed PDF sha256 {}", c.envelope_id, c.signed_pdf_sha256.clone().unwrap_or_default());
    journey.approve(def, step, approver, at, &note).map_err(|e| conflict(format!("journey refused approval: {e:?}")))
}

// ---------- volume: bulk prepare, review and send ----------

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkItem { pub recipients: Vec<RecipientIdentity>, pub pdf_base64: String, #[serde(default)] pub title: Option<String>, #[serde(default)] pub context: Option<ContextLink> }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkPrepareInput { pub operator_label: String, pub items: Vec<BulkItem> }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkIdsInput { pub operator_label: String, pub ids: Vec<String>, #[serde(default)] pub note: Option<String>, #[serde(default)] pub expires_days: Option<u16> }

impl Signatures {
    /// Creates one envelope per item from a built-in template and attaches its PDF.
    /// Items fail independently; nothing is sent.
    pub async fn bulk_prepare(&self, key: &str, input: BulkPrepareInput, meta: ClientMeta) -> SResult<Value> {
        if input.items.is_empty() || input.items.len() > BULK_MAX { return Err(bad(format!("send 1 to {BULK_MAX} items per request"))); }
        builtin(key).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "unknown built-in template".into()))?;
        let mut out = vec![];
        for (n, it) in input.items.into_iter().enumerate() {
            let r: SResult<Value> = async {
                let v = self.instantiate_builtin(key, BuiltinInput { operator_label: input.operator_label.clone(), title: it.title, recipients: it.recipients, context: it.context }).await?;
                let env = &v["envelope"]["envelope"];
                let id = env["id"].as_str().unwrap_or("").to_string();
                self.attach_pdf(&id, PdfInput { operator_label: input.operator_label.clone(), expected_revision: env["revision"].as_u64().unwrap_or(0),
                    expected_content_hash: env["content_hash"].as_str().unwrap_or("").into(), document_id: env["draft"]["documents"][0]["id"].as_str().unwrap_or("").into(), pdf_base64: it.pdf_base64 }, meta.clone()).await?;
                Ok(json!({"index":n,"envelope_id":id,"ok":true}))
            }.await;
            out.push(r.unwrap_or_else(|e| json!({"index":n,"ok":false,"error":e.1})));
        }
        Ok(json!({"results":out}))
    }
    /// One reviewer records review and approval of each listed preparation.
    /// This is the same internal review the single flow uses, at volume.
    pub async fn bulk_approve(&self, input: BulkIdsInput) -> SResult<Value> {
        if input.ids.is_empty() || input.ids.len() > BULK_MAX { return Err(bad(format!("send 1 to {BULK_MAX} ids per request"))); }
        let note = input.note.clone().unwrap_or_else(|| "Bulk review: PDF and recipients checked".into());
        let mut out = vec![];
        for id in &input.ids {
            let r: SResult<()> = async {
                for action in ["submit_review", "approve"] {
                    let e = self.envelope(id).await?;
                    if action == "submit_review" && e.status != "draft" { continue; }
                    self.transition(id, TransitionInput { expected_revision: e.revision, expected_content_hash: e.content_hash.clone(), operator_label: input.operator_label.clone(),
                        action: action.into(), note: note.clone(), field_values: vec![] }).await?;
                }
                Ok(())
            }.await;
            out.push(match r { Ok(()) => json!({"envelope_id":id,"ok":true}), Err(e) => json!({"envelope_id":id,"ok":false,"error":e.1}) });
        }
        Ok(json!({"results":out}))
    }
    pub async fn bulk_send(&self, input: BulkIdsInput, meta: ClientMeta) -> SResult<Value> {
        delivery_mode()?;
        if input.ids.is_empty() || input.ids.len() > BULK_MAX { return Err(bad(format!("send 1 to {BULK_MAX} ids per request"))); }
        let mut out = vec![];
        for id in &input.ids {
            let r: SResult<Value> = async {
                let e = self.envelope(id).await?;
                self.send(id, SendInput { operator_label: input.operator_label.clone(), expected_revision: e.revision, expected_content_hash: e.content_hash.clone(), expires_days: input.expires_days }, meta.clone()).await
            }.await;
            out.push(match r { Ok(v) => json!({"envelope_id":id,"ok":true,"delivery":v["delivery"]}), Err(e) => json!({"envelope_id":id,"ok":false,"error":e.1}) });
        }
        Ok(json!({"results":out}))
    }
}
