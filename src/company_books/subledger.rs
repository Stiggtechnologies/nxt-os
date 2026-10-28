//! Parallel AR/AP, classes, attachments, and native-journal sync.
//! Imported QuickBooks rows stay authoritative. Nothing here writes back.
use super::{bad, corrupt, date, err, hash, storage, text, valid_id, CompanyBooks, Result};
use super::models::{Entry, JournalInput, NativeLine, ReverseInput};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ParallelState {
    #[serde(default)]
    classes: Vec<Class>,
    #[serde(default)]
    documents: Vec<OpenDoc>,
    #[serde(default)]
    payments: Vec<DocPayment>,
    #[serde(default)]
    attachments: Vec<Attachment>,
    #[serde(default)]
    annotations: Vec<Annotation>,
    #[serde(default)]
    deletions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Class { id: String, name: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct DocLine { account_id: String, amount_cents: i64, class_id: Option<String> }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenDoc {
    id: String,
    kind: String,
    contact: String,
    date: String,
    memo: String,
    control_account: String,
    lines: Vec<DocLine>,
    total_cents: i64,
    journal_entry_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct DocPayment { id: String, document_id: String, amount_cents: i64, journal_entry_id: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Attachment { id: String, owner_id: String, filename: String, sha256: String, byte_len: usize }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Annotation { entry_id: String, sync: u64, memo: Option<String>, class_id: Option<String> }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassInput { pub request_id: String, pub name: String }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocLineInput { pub account_id: String, pub amount_cents: i64, pub class_id: Option<String> }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentInput {
    pub request_id: String,
    pub expected_head: String,
    pub operator_label: String,
    pub kind: String,
    pub contact: String,
    pub date: String,
    pub memo: String,
    pub control_account: String,
    pub lines: Vec<DocLineInput>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentInput {
    pub request_id: String,
    pub expected_head: String,
    pub operator_label: String,
    pub document_id: String,
    pub cash_account: String,
    pub date: String,
    pub amount_cents: i64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SparseInput {
    pub expected_head: String,
    pub expected_sync: u64,
    pub memo: Option<String>,
    pub class_id: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteInput {
    pub request_id: String,
    pub expected_head: String,
    pub operator_label: String,
    pub date: String,
    pub memo: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentInput { pub owner_id: String, pub filename: String, pub content_hex: String }

fn parallel_key(company: &str) -> String { format!("company-books:{company}:parallel-subledger:v1") }
fn cents(n: i64) -> String { n.to_string() }

impl CompanyBooks {
    async fn parallel_load(&self, company: &str) -> Result<(Option<String>, ParallelState)> {
        let raw = self.store.get(&parallel_key(company)).await.map_err(|_| storage(String::new()))?;
        let state = match &raw {
            None => ParallelState::default(),
            Some(text) => serde_json::from_str(text).map_err(|_| corrupt())?,
        };
        Ok((raw, state))
    }
    async fn parallel_save(&self, company: &str, expected: Option<&str>, state: &ParallelState) -> Result<()> {
        let next = serde_json::to_string(state).map_err(|_| bad("Parallel subledger cannot be encoded"))?;
        if next.len() > 1024 * 1024 { return Err(bad("Parallel subledger exceeds 1 MiB")); }
        if self.store.compare_exchange(&parallel_key(company), expected, &next).await.map_err(|_| storage(String::new()))? {
            return Ok(());
        }
        Err(err(StatusCode::CONFLICT, "Parallel subledger changed; retry"))
    }
    fn guard_native(entry: &Entry) -> Result<()> {
        let native = entry.source_metadata.as_ref().and_then(|v| v.get("origin")).and_then(|v| v.as_str()) == Some("native_adjustment")
            || entry.source_id.starts_with("nxt-native:");
        if native { Ok(()) } else {
            Err(err(StatusCode::CONFLICT, "Imported QuickBooks rows stay authoritative. Sparse update and delete apply only to parallel NXT journals."))
        }
    }
    pub async fn add_class(&self, company: &str, input: ClassInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !text(&input.name, 80) || input.name.trim().is_empty() { return Err(bad("Class name and request id are required")); }
        let (raw, mut state) = self.parallel_load(company).await?;
        if state.classes.iter().any(|c| c.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "write_back": false}));
        }
        if state.classes.len() >= 200 { return Err(bad("Class list is full")); }
        state.classes.push(Class { id: input.request_id.clone(), name: input.name.trim().into() });
        self.parallel_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn open_document(&self, company: &str, input: DocumentInput) -> Result<Value> {
        if !matches!(input.kind.as_str(), "invoice" | "bill") { return Err(bad("Document kind must be invoice or bill")); }
        if !date(&input.date) || !text(&input.memo, 500) || !text(&input.contact, 120) || input.contact.trim().is_empty() { return Err(bad("Document contact, date, or memo is invalid")); }
        if input.lines.is_empty() || input.lines.len() > 40 { return Err(bad("A document needs 1–40 lines")); }
        let (raw, mut state) = self.parallel_load(company).await?;
        if state.documents.iter().any(|d| d.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "write_back": false}));
        }
        let mut total = 0_i64;
        for line in &input.lines {
            if line.amount_cents <= 0 { return Err(bad("Document line amounts must be positive cents")); }
            total = total.checked_add(line.amount_cents).ok_or_else(|| bad("Amount overflow"))?;
            if let Some(class) = &line.class_id {
                if !state.classes.iter().any(|c| &c.id == class) { return Err(bad("Unknown class")); }
            }
        }
        let mut lines = Vec::new();
        if input.kind == "invoice" {
            lines.push(NativeLine { account_id: input.control_account.clone(), debit_cents: cents(total), credit_cents: "0".into(), memo: input.memo.clone() });
            for line in &input.lines {
                lines.push(NativeLine { account_id: line.account_id.clone(), debit_cents: "0".into(), credit_cents: cents(line.amount_cents), memo: input.memo.clone() });
            }
        } else {
            for line in &input.lines {
                lines.push(NativeLine { account_id: line.account_id.clone(), debit_cents: cents(line.amount_cents), credit_cents: "0".into(), memo: input.memo.clone() });
            }
            lines.push(NativeLine { account_id: input.control_account.clone(), debit_cents: "0".into(), credit_cents: cents(total), memo: input.memo.clone() });
        }
        let posted = self.journal(company, JournalInput {
            request_id: input.request_id.clone(),
            expected_head: input.expected_head.clone(),
            operator_label: input.operator_label.clone(),
            date: input.date.clone(),
            memo: input.memo.clone(),
            lines,
        }).await?;
        let head = posted["head_sha256"].as_str().unwrap_or("").to_string();
        let page = self.entries(company, 0, 200, None).await?;
        let entry_id = page["rows"].as_array().and_then(|rows| rows.iter().rev().find(|row| row["memo"] == input.memo)).and_then(|row| row["id"].as_str()).unwrap_or("").to_string();
        if entry_id.is_empty() { return Err(corrupt()); }
        let doc = OpenDoc {
            id: input.request_id.clone(),
            kind: input.kind.clone(),
            contact: input.contact.trim().into(),
            date: input.date.clone(),
            memo: input.memo.clone(),
            control_account: input.control_account.clone(),
            lines: input.lines.iter().map(|line| DocLine { account_id: line.account_id.clone(), amount_cents: line.amount_cents, class_id: line.class_id.clone() }).collect(),
            total_cents: total,
            journal_entry_id: entry_id,
        };
        state.documents.push(doc.clone());
        self.parallel_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": doc.id, "journal_entry_id": doc.journal_entry_id, "total_cents": total, "open_cents": total, "head_sha256": head, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn apply_payment(&self, company: &str, input: PaymentInput) -> Result<Value> {
        if input.amount_cents <= 0 { return Err(bad("Payment amount must be positive cents")); }
        let (raw, mut state) = self.parallel_load(company).await?;
        if state.payments.iter().any(|p| p.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "write_back": false}));
        }
        let doc = state.documents.iter().find(|d| d.id == input.document_id).cloned().ok_or_else(|| err(StatusCode::NOT_FOUND, "Open document not found"))?;
        let paid: i64 = state.payments.iter().filter(|p| p.document_id == doc.id).map(|p| p.amount_cents).sum();
        let open = doc.total_cents - paid;
        if input.amount_cents > open { return Err(bad("Payment exceeds the open AR/AP balance")); }
        let (debit, credit) = if doc.kind == "invoice" {
            (input.cash_account.clone(), doc.control_account.clone())
        } else {
            (doc.control_account.clone(), input.cash_account.clone())
        };
        let posted = self.journal(company, JournalInput {
            request_id: input.request_id.clone(),
            expected_head: input.expected_head,
            operator_label: input.operator_label,
            date: input.date,
            memo: format!("Parallel payment {}", doc.id),
            lines: vec![
                NativeLine { account_id: debit, debit_cents: cents(input.amount_cents), credit_cents: "0".into(), memo: "Payment".into() },
                NativeLine { account_id: credit, debit_cents: "0".into(), credit_cents: cents(input.amount_cents), memo: "Payment".into() },
            ],
        }).await?;
        let head = posted["head_sha256"].as_str().unwrap_or("").to_string();
        let page = self.entries(company, 0, 200, None).await?;
        let memo = format!("Parallel payment {}", doc.id);
        let entry_id = page["rows"].as_array().and_then(|rows| rows.iter().rev().find(|row| row["memo"] == memo)).and_then(|row| row["id"].as_str()).unwrap_or("").to_string();
        state.payments.push(DocPayment { id: input.request_id.clone(), document_id: doc.id.clone(), amount_cents: input.amount_cents, journal_entry_id: entry_id });
        self.parallel_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "open_cents": open - input.amount_cents, "head_sha256": head, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn subledger(&self, company: &str) -> Result<Value> {
        let (_, state) = self.parallel_load(company).await?;
        let rows: Vec<Value> = state.documents.iter().map(|doc| {
            let paid: i64 = state.payments.iter().filter(|p| p.document_id == doc.id).map(|p| p.amount_cents).sum();
            json!({"id": doc.id, "kind": doc.kind, "contact": doc.contact, "total_cents": doc.total_cents, "open_cents": doc.total_cents - paid, "journal_entry_id": doc.journal_entry_id, "classes": doc.lines.iter().filter_map(|l| l.class_id.clone()).collect::<Vec<_>>()})
        }).collect();
        Ok(json!({"documents": rows, "classes": state.classes.len(), "attachments": state.attachments.len(), "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn sparse_native(&self, company: &str, entry_id: &str, input: SparseInput) -> Result<Value> {
        let (head, _) = self.current(company).await?.ok_or_else(|| err(StatusCode::NOT_FOUND, "Company has no committed book"))?;
        if head != input.expected_head { return Err(err(StatusCode::CONFLICT, "Stale SyncToken")); }
        let (_, _, _, entries) = self.book(company).await?;
        let entry = entries.iter().find(|e| e.id == entry_id).ok_or_else(|| err(StatusCode::NOT_FOUND, "Journal not found"))?;
        Self::guard_native(entry)?;
        if let Some(memo) = &input.memo { if !text(memo, 500) { return Err(bad("Memo is invalid")); } }
        let (raw, mut state) = self.parallel_load(company).await?;
        if let Some(class) = &input.class_id {
            if !state.classes.iter().any(|c| &c.id == class) { return Err(bad("Unknown class")); }
        }
        let current = state.annotations.iter().find(|a| a.entry_id == entry_id).map(|a| a.sync).unwrap_or(0);
        if current != input.expected_sync { return Err(err(StatusCode::CONFLICT, "Stale sparse sync")); }
        state.annotations.retain(|a| a.entry_id != entry_id);
        let next = current + 1;
        state.annotations.push(Annotation { entry_id: entry_id.into(), sync: next, memo: input.memo.clone(), class_id: input.class_id.clone() });
        self.parallel_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"entry_id": entry_id, "sync": next, "amounts_unchanged": true, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn delete_native(&self, company: &str, entry_id: &str, input: DeleteInput) -> Result<Value> {
        let (_, _, _, entries) = self.book(company).await?;
        let entry = entries.iter().find(|e| e.id == entry_id).ok_or_else(|| err(StatusCode::NOT_FOUND, "Journal not found"))?;
        Self::guard_native(entry)?;
        let (raw, mut state) = self.parallel_load(company).await?;
        if state.deletions.iter().any(|id| id == entry_id) {
            return Err(err(StatusCode::CONFLICT, "Journal was already deleted in the parallel book"));
        }
        let posted = self.reverse(company, entry_id, ReverseInput {
            request_id: input.request_id,
            expected_head: input.expected_head,
            operator_label: input.operator_label,
            date: input.date,
            memo: input.memo,
        }).await?;
        state.deletions.push(entry_id.into());
        self.parallel_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"entry_id": entry_id, "status": "deleted_in_parallel", "head_sha256": posted["head_sha256"], "source_bytes_unchanged": true, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn attach(&self, company: &str, input: AttachmentInput) -> Result<Value> {
        if !text(&input.filename, 120) || input.filename.trim().is_empty() { return Err(bad("Attachment filename is invalid")); }
        let bytes = hex::decode(input.content_hex.trim()).map_err(|_| bad("Attachment content_hex is not hex"))?;
        if bytes.is_empty() || bytes.len() > 65_536 { return Err(bad("Attachment must be 1–65536 bytes")); }
        let digest = hash(&bytes);
        let (raw, mut state) = self.parallel_load(company).await?;
        let known = state.documents.iter().any(|d| d.id == input.owner_id) || self.book(company).await?.3.iter().any(|e| e.id == input.owner_id);
        if !known { return Err(err(StatusCode::NOT_FOUND, "Attachment owner not found")); }
        if state.attachments.len() >= 50 { return Err(bad("Attachment list is full")); }
        let id = format!("att_{digest}");
        if !state.attachments.iter().any(|a| a.id == id) {
            state.attachments.push(Attachment { id: id.clone(), owner_id: input.owner_id, filename: input.filename.trim().into(), sha256: digest.clone(), byte_len: bytes.len() });
            self.parallel_save(company, raw.as_deref(), &state).await?;
        }
        Ok(json!({"id": id, "sha256": digest, "byte_len": bytes.len(), "content_executed": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }
}
