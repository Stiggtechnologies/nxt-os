//! Notice-of-assessment lane: metadata plus an optional stored file, attached to a
//! return. NXT never calls CRA or downloads notices; a person files what they received.
use super::{Tax, TResult, bad, conflict};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const KEY: &str = "tax:noa:v1";
const MAX_DOCS: usize = 25;
const MAX_BYTES: usize = 8_000_000;

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|date| date.format("%Y-%m-%d").to_string() == value)
            .unwrap_or(false)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoaDoc {
    pub id: String,
    pub return_id: String,
    pub client_ref: String,
    pub tax_year: i32,
    pub received_date: String,
    #[serde(default)]
    pub balance_cents: Option<i64>,
    #[serde(default)]
    pub note: String,
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub data_base64: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct NoaSummary {
    pub id: String,
    pub return_id: String,
    pub client_ref: String,
    pub tax_year: i32,
    pub received_date: String,
    pub balance_cents: Option<i64>,
    pub note: String,
    pub filename: String,
    pub has_file: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoaInput {
    pub received_date: String,
    #[serde(default)]
    pub balance_cents: Option<i64>,
    #[serde(default)]
    pub note: String,
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub data_base64: Option<String>,
}

async fn load(t: &Tax) -> TResult<Vec<NoaDoc>> {
    Ok(t.store.get_json(KEY).await?.unwrap_or_default())
}

fn summary(doc: &NoaDoc) -> NoaSummary {
    NoaSummary {
        id: doc.id.clone(),
        return_id: doc.return_id.clone(),
        client_ref: doc.client_ref.clone(),
        tax_year: doc.tax_year,
        received_date: doc.received_date.clone(),
        balance_cents: doc.balance_cents,
        note: doc.note.clone(),
        filename: doc.filename.clone(),
        has_file: doc.data_base64.as_deref().is_some_and(|value| !value.is_empty()),
        created_at: doc.created_at.clone(),
    }
}

pub async fn list(t: &Tax, record_id: &str) -> TResult<Value> {
    t.get(record_id).await?;
    let mut docs: Vec<NoaSummary> = load(t).await?.iter().filter(|doc| doc.return_id == record_id).map(summary).collect();
    docs.sort_by(|a, b| b.received_date.cmp(&a.received_date).then(b.created_at.cmp(&a.created_at)));
    Ok(json!({ "notices": docs }))
}

pub async fn add(t: &Tax, record_id: &str, input: NoaInput) -> TResult<Value> {
    let record = t.get(record_id).await?;
    if !valid_date(&input.received_date) {
        return Err(bad("received_date must be a valid YYYY-MM-DD date"));
    }
    if input.filename.trim().is_empty() || input.filename.len() > 200 {
        return Err(bad("the notice needs a file name of at most 200 characters"));
    }
    if input.note.len() > 1000 {
        return Err(bad("the note is limited to 1000 characters"));
    }
    if let Some(file) = &input.data_base64 {
        if file.len() > 1_500_000 {
            return Err(bad("the notice file is larger than 1.5 MB"));
        }
    }
    let mut docs = load(t).await?;
    if docs.len() >= MAX_DOCS {
        return Err(conflict(format!("the notice lane already holds {MAX_DOCS} documents")));
    }
    let used: usize = docs.iter().filter_map(|doc| doc.data_base64.as_ref()).map(|value| value.len()).sum();
    let incoming = input.data_base64.as_ref().map(|value| value.len()).unwrap_or(0);
    if used + incoming > MAX_BYTES {
        return Err(conflict("the notice lane is full; delete a document first"));
    }
    let doc = NoaDoc {
        id: format!("noa_{}", uuid::Uuid::new_v4().simple()),
        return_id: record.id,
        client_ref: record.client_ref,
        tax_year: record.tax_year,
        received_date: input.received_date,
        balance_cents: input.balance_cents,
        note: input.note.trim().to_string(),
        filename: input.filename.trim().to_string(),
        content_type: input.content_type,
        data_base64: input.data_base64.filter(|value| !value.trim().is_empty()),
        created_at: super::now(),
    };
    docs.push(doc.clone());
    t.store.set_json(KEY, &docs).await?;
    Ok(json!({ "notice": summary(&doc) }))
}

pub async fn get(t: &Tax, record_id: &str, noa_id: &str) -> TResult<Value> {
    let doc = load(t).await?.into_iter().find(|doc| doc.return_id == record_id && doc.id == noa_id)
        .ok_or_else(|| super::ApiError(axum::http::StatusCode::NOT_FOUND, "notice not found".into()))?;
    Ok(json!({ "notice": doc }))
}

pub async fn delete(t: &Tax, record_id: &str, noa_id: &str) -> TResult<()> {
    let mut docs = load(t).await?;
    let before = docs.len();
    docs.retain(|doc| !(doc.return_id == record_id && doc.id == noa_id));
    if docs.len() == before {
        return Err(super::ApiError(axum::http::StatusCode::NOT_FOUND, "notice not found".into()));
    }
    t.store.set_json(KEY, &docs).await?;
    Ok(())
}
