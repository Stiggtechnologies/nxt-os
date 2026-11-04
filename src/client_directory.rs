//! One read-only list of the clients NXT holds records for, joined across modules by client
//! reference. It reports what each module stores and states what no module holds yet.
//! Nothing here creates a client record. Scoped accounts see their assigned clients only.

use crate::{company_books::CompanyBooks, engagement::Engagements, firm::Firm, migrations::Migrations, referral::routes::ApiError, signatures::Signatures, tax::Tax};
use axum::{Json, Router, extract::{Path, State}, http::{HeaderValue, StatusCode, header}, middleware, response::Response, routing::get};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

const BOOK_PAGE: usize = 50;
const BOOK_PAGES: usize = 11;

pub struct Directory {
    pub company_books: Arc<CompanyBooks>,
    pub migrations: Arc<Migrations>,
    pub tax: Arc<Tax>,
    pub engagements: Arc<Engagements>,
    pub signatures: Arc<Signatures>,
    pub firm: Arc<Firm>,
}

#[derive(Default)]
struct Row {
    client_ref: String,
    name: Option<String>,
    in_source_roster: bool,
    source_snapshots: u64,
    book_stage: Option<String>,
    tax_returns: u64,
    engagements: u64,
    signature_envelopes: u64,
    projects: u64,
    closes: u64,
}

#[derive(Default)]
struct Rows(BTreeMap<String, Row>);

impl Rows {
    /// Client references match without regard to case, as `Identity::can_client` does.
    fn get(&mut self, client: &str) -> Option<&mut Row> {
        let client = client.trim();
        if client.is_empty() { return None; }
        let row = self.0.entry(client.to_ascii_lowercase()).or_default();
        if row.client_ref.is_empty() { row.client_ref = client.to_string(); }
        Some(row)
    }
}

/// Records the firm has that are not client-linked in this deployment, and why.
fn not_loaded() -> Value {
    let dtmax = if crate::source_import::test_copy::hosted_switch_on() {
        "Kept in its own list under DT Max clients. It is not joined to these rows, because DT Max client numbers are not company ids."
    } else {
        "Loads only into a local test copy. Hosted storage refuses it until the owner sets NXT_DTMAX_HOSTED=on."
    };
    json!([
        {"record": "DT Max client list and contacts", "reason": dtmax},
        {"record": "DT Max return content", "reason": "The return files are vendor-encoded. NXT holds a per-year file index at most, and needs a DT Max export for content."},
        {"record": "QuickBooks customers, suppliers, invoices and bills", "reason": "The journal import does not include them. They need typed source exports."},
        {"record": "Caseware, WebTax, DocuSign and payroll client records", "reason": "No client-level importer yet. These lanes are preservation only."}
    ])
}

impl Directory {
    async fn book_rows(&self, rows: &mut Rows) -> Option<()> {
        let mut offset = 0;
        for _ in 0..BOOK_PAGES {
            let page = self.company_books.list_page(offset, BOOK_PAGE).await.ok()?;
            let companies = page["companies"].as_array()?;
            for book in companies {
                let Some(row) = rows.get(book["company_id"].as_str().unwrap_or("")) else { continue };
                row.book_stage = book["stage"].as_str().map(str::to_string);
                if row.name.is_none() { row.name = book["previews"][0]["company"]["name"].as_str().map(str::to_string); }
            }
            if page["has_more"] != json!(true) { return Some(()); }
            offset += companies.len();
        }
        None
    }

    async fn collect(&self) -> (Rows, Vec<&'static str>) {
        let mut rows = Rows::default();
        let mut unavailable = vec![];
        match self.company_books.roster_view().await {
            Ok(v) => for c in v["companies"].as_array().into_iter().flatten() {
                if let Some(row) = rows.get(c["company_id"].as_str().unwrap_or("")) {
                    row.in_source_roster = true;
                    row.name = c["company_name"].as_str().map(str::to_string);
                }
            },
            Err(_) => unavailable.push("source roster"),
        }
        if self.book_rows(&mut rows).await.is_none() { unavailable.push("company books"); }
        match self.migrations.list().await {
            Ok(v) => for s in v["snapshots"].as_array().into_iter().flatten() {
                if let Some(row) = rows.get(s["cohort_ref"].as_str().unwrap_or("")) {
                    row.source_snapshots += 1;
                    if row.name.is_none() { row.name = s["company_name"].as_str().map(str::to_string); }
                }
            },
            Err(_) => unavailable.push("source reports"),
        }
        match self.tax.list(None).await {
            Ok(v) => for r in &v { if let Some(row) = rows.get(r["client_ref"].as_str().unwrap_or("")) { row.tax_returns += 1; } },
            Err(_) => unavailable.push("tax returns"),
        }
        match self.engagements.list().await {
            Ok(v) => for e in &v { if let Some(row) = rows.get(e["client_ref"].as_str().unwrap_or("")) { row.engagements += 1; } },
            Err(_) => unavailable.push("engagements"),
        }
        match self.signatures.list().await {
            Ok(v) => for e in v["envelopes"].as_array().into_iter().flatten() {
                if let Some(row) = rows.get(e["client_ref"].as_str().unwrap_or("")) { row.signature_envelopes += 1; }
            },
            Err(_) => unavailable.push("e-sign envelopes"),
        }
        match self.firm.snapshot().await {
            Ok(s) => {
                for p in &s.projects { if let Some(row) = rows.get(&p.client_ref) { row.projects += 1; } }
                for c in &s.closes { if let Some(row) = rows.get(&c.client_ref) { row.closes += 1; } }
            }
            Err(_) => unavailable.push("firm work"),
        }
        (rows, unavailable)
    }
}

fn summary(r: &Row) -> Value {
    json!({"client_ref": r.client_ref, "name": r.name, "in_source_roster": r.in_source_roster, "source_snapshots": r.source_snapshots,
        "book_stage": r.book_stage, "tax_returns": r.tax_returns, "engagements": r.engagements,
        "signature_envelopes": r.signature_envelopes, "projects": r.projects, "closes": r.closes})
}

type S = State<Arc<Directory>>;

async fn list(State(d): S, identity: Option<axum::Extension<crate::firm::staff::Identity>>) -> Json<Value> {
    let (rows, unavailable) = d.collect().await;
    let mut clients: Vec<&Row> = rows.0.values().collect();
    clients.sort_by_key(|r| r.name.as_deref().unwrap_or(&r.client_ref).to_ascii_lowercase());
    let scoped = identity.is_some_and(|i| !i.all_clients());
    Json(json!({"clients": clients.into_iter().map(summary).collect::<Vec<_>>(), "unavailable": unavailable, "not_loaded": not_loaded(),
        "scoped": scoped, "source_authoritative": true, "live_cutover_allowed": true}))
}

async fn detail(State(d): S, Path(client): Path<String>) -> Result<Json<Value>, ApiError> {
    let (mut rows, unavailable) = d.collect().await;
    let row = rows.0.remove(&client.trim().to_ascii_lowercase())
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no records for this client".into()))?;
    let same = |v: &Value, key: &str| v[key].as_str().is_some_and(|c| c.trim().eq_ignore_ascii_case(client.trim()));
    let pick = |v: &Value, keys: &[&str]| Value::Object(keys.iter().map(|k| (k.to_string(), v[*k].clone())).collect());
    let tax: Vec<Value> = d.tax.list(None).await.unwrap_or_default().iter().filter(|r| same(r, "client_ref"))
        .map(|r| pick(r, &["id", "program", "tax_year", "name", "status"])).collect();
    let engagements: Vec<Value> = d.engagements.list().await.unwrap_or_default().iter().filter(|e| same(e, "client_ref"))
        .map(|e| pick(e, &["id", "name", "status"])).collect();
    let envelopes: Vec<Value> = d.signatures.list().await.ok().and_then(|v| v["envelopes"].as_array().cloned()).unwrap_or_default().iter()
        .filter(|e| same(e, "client_ref")).map(|e| pick(e, &["id", "title", "status"])).collect();
    let snapshots: Vec<Value> = d.migrations.list().await.ok().and_then(|v| v["snapshots"].as_array().cloned()).unwrap_or_default().iter()
        .filter(|s| same(s, "cohort_ref")).map(|s| pick(s, &["id", "company_name"])).collect();
    Ok(Json(json!({"client": summary(&row), "tax_returns": tax, "engagements": engagements, "signature_envelopes": envelopes,
        "source_snapshots": snapshots, "firm_work": d.firm.client_work(&row.client_ref, None).await.ok(),
        "unavailable": unavailable, "not_loaded": not_loaded(), "source_authoritative": true, "live_cutover_allowed": true})))
}

/// The client reference is the path segment, and list rows carry `client_ref`, so the shared
/// guard filters the list and refuses other clients' detail.
async fn client_scope(req: axum::extract::Request, next: middleware::Next) -> Response {
    crate::firm::staff::scope_guard(req, next, "/api/clients", &[], |c| async move { Some(c) }).await
}

pub fn router(state: Arc<Directory>) -> Router {
    Router::new()
        .route("/api/clients", get(list))
        .route("/api/clients/{client}", get(detail))
        .layer(middleware::from_fn(client_scope))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            r.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            r
        }))
        .with_state(state)
}
