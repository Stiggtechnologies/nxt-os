//! One read-only list of the clients NXT holds records for, joined across modules by client
//! reference. It reports what each module stores and states what no module holds yet.
//! Nothing here creates a client record. Scoped accounts see their assigned clients only.

use crate::{company_books::CompanyBooks, engagement::Engagements, firm::Firm, migrations::Migrations, referral::routes::ApiError, signatures::Signatures, tax::Tax};
use axum::{Json, Router, extract::{Path, State}, http::{HeaderValue, StatusCode, header}, middleware, response::Response, routing::get};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet, HashMap}, sync::Arc};

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

/// Rows keyed by canonical client reference, plus aliases that resolve to it. The roster names
/// QuickBooks companies by selector-derived hash ids while books use name-slug ids, so one company
/// can arrive under two references; aliases fold them into one row.
#[derive(Default)]
struct Rows {
    rows: BTreeMap<String, Row>,
    aliases: HashMap<String, String>,
    /// Normalized company name -> canonical keys carrying that name.
    names: HashMap<String, BTreeSet<String>>,
    /// Canonical key -> every reference (lowercased) that resolves to it.
    refs: HashMap<String, BTreeSet<String>>,
}

/// Lowercase alphanumeric words only, so "1589548 ALBERTA LTD." and "1589548 Alberta Ltd" match.
fn normalize_name(name: &str) -> String {
    name.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

impl Rows {
    fn key(&self, client: &str) -> Option<String> {
        let client = client.trim();
        if client.is_empty() { return None; }
        let lower = client.to_ascii_lowercase();
        Some(self.aliases.get(&lower).cloned().unwrap_or(lower))
    }

    fn alias(&mut self, alias: &str, key: &str) {
        let alias = alias.trim().to_ascii_lowercase();
        if alias.is_empty() || alias == key || self.rows.contains_key(&alias) { return; }
        self.aliases.entry(alias.clone()).or_insert_with(|| key.to_string());
        self.refs.entry(key.to_string()).or_default().insert(alias);
    }

    fn remember_name(&mut self, key: &str, name: Option<&str>) {
        if let Some(n) = name.map(normalize_name).filter(|n| !n.is_empty()) { self.names.entry(n).or_default().insert(key.to_string()); }
    }

    /// Client references match without regard to case, as `Identity::can_client` does.
    fn get(&mut self, client: &str) -> Option<&mut Row> {
        let key = self.key(client)?;
        self.refs.entry(key.clone()).or_default().insert(key.clone());
        let row = self.rows.entry(key).or_default();
        if row.client_ref.is_empty() { row.client_ref = client.trim().to_string(); }
        Some(row)
    }

    /// A roster company: its own row (two selector entries are two source companies, even with
    /// one name), with the QuickBooks selector reference as an alias.
    fn roster(&mut self, id: &str, name: Option<&str>, selector: Option<&str>) {
        let Some(key) = self.key(id) else { return };
        let Some(row) = self.get(id) else { return };
        row.in_source_roster = true;
        row.name = name.map(str::to_string);
        self.remember_name(&key, name);
        if let Some(sel) = selector { self.alias(sel, &key); }
    }

    /// A company book: joins its own id when known, otherwise the one existing row with the same
    /// normalized name or QuickBooks file reference. Ambiguous matches stay separate rows.
    fn book(&mut self, id: &str, name: Option<&str>, source_ref: Option<&str>, stage: Option<&str>) {
        let Some(own) = self.key(id) else { return };
        let key = if self.rows.contains_key(&own) { own.clone() } else {
            let by_ref = source_ref.and_then(|r| self.key(r)).filter(|k| self.rows.contains_key(k));
            let by_name = name.map(normalize_name).and_then(|n| self.names.get(&n))
                .filter(|keys| keys.len() == 1).and_then(|keys| keys.iter().next().cloned());
            match by_ref.or(by_name) { Some(k) => { self.alias(&own, &k); k } None => own.clone() }
        };
        if let Some(r) = source_ref { self.alias(r, &key); }
        let Some(row) = self.get(&key) else { return };
        if row.book_stage.is_none() || stage == Some("import_committed") { row.book_stage = stage.map(str::to_string); }
        if row.name.is_none() { row.name = name.map(str::to_string); }
        let keep_name = row.name.clone();
        self.remember_name(&key, keep_name.as_deref());
    }

    /// Every reference (lowercased) that resolves to the canonical key.
    fn refs_of(&self, key: &str) -> BTreeSet<String> {
        self.refs.get(key).cloned().unwrap_or_else(|| BTreeSet::from([key.to_string()]))
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
                let company = &book["previews"][0]["company"];
                rows.book(book["company_id"].as_str().unwrap_or(""), company["name"].as_str(),
                    company["source_company_id"].as_str(), book["stage"].as_str());
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
                rows.roster(c["company_id"].as_str().unwrap_or(""), c["company_name"].as_str(), c["source_navigation_ref"].as_str());
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

/// Alias groups for company ids: every roster and book id (original case) keyed by its lowercased
/// form, mapped to every id of the same company. Built from the roster and the book index only.
#[derive(Default, Clone)]
pub struct AliasGroups {
    groups: HashMap<String, Vec<String>>,
    books: BTreeSet<String>,
    names: HashMap<String, String>,
}

impl AliasGroups {
    /// Every known id of the company `id` names, `id` first. Unknown ids stand alone.
    pub fn members(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_string()];
        for m in self.groups.get(&id.trim().to_ascii_lowercase()).into_iter().flatten() {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(m)) { out.push(m.clone()); }
        }
        out
    }
    pub fn has_book(&self, id: &str) -> bool { self.books.contains(&id.to_ascii_lowercase()) }
    /// The id holding this company's book, when an alias (not `id` itself) holds it.
    pub fn book_alias(&self, id: &str) -> Option<String> {
        if self.has_book(id) { return None; }
        self.members(id).into_iter().skip(1).find(|m| self.has_book(m))
    }
    pub fn name(&self, id: &str) -> Option<&str> { self.names.get(&id.to_ascii_lowercase()).map(String::as_str) }
}

/// One roster or book entry fed to [`AliasGroups::build`].
pub struct AliasInput<'a> { pub id: &'a str, pub name: Option<&'a str>, pub source_ref: Option<&'a str>, pub book: bool, pub archived: bool, pub stage: Option<&'a str> }

impl AliasGroups {
    pub fn build(items: &[AliasInput]) -> Self {
        let mut rows = Rows::default();
        let mut out = AliasGroups::default();
        for it in items.iter().filter(|i| !i.book) {
            rows.roster(it.id, it.name, it.source_ref);
            if let Some(n) = it.name { out.names.insert(it.id.to_ascii_lowercase(), n.to_string()); }
        }
        for it in items.iter().filter(|i| i.book) {
            rows.book(it.id, it.name, it.source_ref, it.stage);
            if !it.archived { out.books.insert(it.id.to_ascii_lowercase()); }
            if let Some(n) = it.name { out.names.entry(it.id.to_ascii_lowercase()).or_insert_with(|| n.to_string()); }
        }
        let mut by_key: HashMap<String, Vec<String>> = HashMap::new();
        for it in items.iter().filter(|i| !i.id.is_empty()) {
            if let Some(k) = rows.key(it.id) { let g = by_key.entry(k).or_default(); if !g.iter().any(|x| x == it.id) { g.push(it.id.to_string()); } }
        }
        for g in by_key.into_values().filter(|g| g.len() > 1) {
            for id in &g { out.groups.insert(id.to_ascii_lowercase(), g.clone()); }
        }
        out
    }
}

impl Directory {
    pub async fn alias_groups(&self) -> AliasGroups {
        let roster = self.company_books.roster_view().await.ok();
        let mut pages = vec![];
        let mut offset = 0;
        for _ in 0..BOOK_PAGES {
            let Ok(page) = self.company_books.list_page(offset, BOOK_PAGE).await else { break };
            let n = page["companies"].as_array().map_or(0, Vec::len);
            let more = page["has_more"] == json!(true);
            pages.push(page);
            if !more || n == 0 { break; }
            offset += n;
        }
        let mut items = vec![];
        for c in roster.iter().flat_map(|v| v["companies"].as_array().into_iter().flatten()) {
            items.push(AliasInput { id: c["company_id"].as_str().unwrap_or(""), name: c["company_name"].as_str(), source_ref: c["source_navigation_ref"].as_str(), book: false, archived: false, stage: None });
        }
        for b in pages.iter().flat_map(|p| p["companies"].as_array().into_iter().flatten()) {
            let company = &b["previews"][0]["company"];
            items.push(AliasInput { id: b["company_id"].as_str().unwrap_or(""), name: company["name"].as_str(), source_ref: company["source_company_id"].as_str(), book: true, archived: b["head"]["archived"] == json!(true), stage: b["stage"].as_str() });
        }
        AliasGroups::build(&items)
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
    let mut clients: Vec<&Row> = rows.rows.values().collect();
    clients.sort_by_key(|r| r.name.as_deref().unwrap_or(&r.client_ref).to_ascii_lowercase());
    let scoped = identity.is_some_and(|i| !i.all_clients());
    let clients: Vec<Value> = clients.into_iter().map(|r| { let mut v = summary(r); if let Some(k) = rows.key(&r.client_ref) { v["aliases"] = json!(rows.refs_of(&k).into_iter().filter(|a| !a.starts_with("qbo-selector:") && !a.contains(':')).collect::<Vec<_>>()); } v }).collect();
    Json(json!({"clients": clients, "unavailable": unavailable, "not_loaded": not_loaded(),
        "scoped": scoped, "source_authoritative": true, "live_cutover_allowed": false}))
}

async fn detail(State(d): S, Path(client): Path<String>) -> Result<Json<Value>, ApiError> {
    let (mut rows, unavailable) = d.collect().await;
    let key = rows.key(&client).unwrap_or_default();
    let refs = rows.refs_of(&key);
    let row = rows.rows.remove(&key)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no records for this client".into()))?;
    let same = |v: &Value, key: &str| v[key].as_str().is_some_and(|c| refs.contains(&c.trim().to_ascii_lowercase()));
    let pick = |v: &Value, keys: &[&str]| Value::Object(keys.iter().map(|k| (k.to_string(), v[*k].clone())).collect());
    let tax: Vec<Value> = d.tax.list(None).await.unwrap_or_default().iter().filter(|r| same(r, "client_ref"))
        .map(|r| pick(r, &["id", "program", "tax_year", "name", "status"])).collect();
    let engagements: Vec<Value> = d.engagements.list().await.unwrap_or_default().iter().filter(|e| same(e, "client_ref"))
        .map(|e| pick(e, &["id", "name", "status"])).collect();
    let envelopes: Vec<Value> = d.signatures.list().await.ok().and_then(|v| v["envelopes"].as_array().cloned()).unwrap_or_default().iter()
        .filter(|e| same(e, "client_ref")).map(|e| pick(e, &["id", "title", "status"])).collect();
    let snapshots: Vec<Value> = d.migrations.list().await.ok().and_then(|v| v["snapshots"].as_array().cloned()).unwrap_or_default().iter()
        .filter(|s| same(s, "cohort_ref")).map(|s| pick(s, &["id", "company_name"])).collect();
    let mut client = summary(&row);
    client["aliases"] = json!(refs.iter().filter(|a| !a.contains(':')).collect::<Vec<_>>());
    Ok(Json(json!({"client": client, "tax_returns": tax, "engagements": engagements, "signature_envelopes": envelopes,
        "source_snapshots": snapshots, "firm_work": d.firm.client_work(&row.client_ref, None).await.ok(),
        "unavailable": unavailable, "not_loaded": not_loaded(), "source_authoritative": true, "live_cutover_allowed": false})))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_hash_id_and_book_slug_id_are_one_row() {
        let mut rows = Rows::default();
        rows.roster("qbo-client-26e212deac6c0219bd4a", Some("1589548 ALBERTA LTD."), Some("qbo-selector:idsMenuItem4"));
        rows.book("qbo-1589548-alberta-ltd", Some("1589548 Alberta Ltd"), Some("quickbooks_online:1589548-alberta-ltd"), Some("import_committed"));
        rows.get("QBO-1589548-ALBERTA-LTD").unwrap().tax_returns += 1;
        assert_eq!(rows.rows.len(), 1);
        let row = rows.rows.values().next().unwrap();
        assert!(row.in_source_roster);
        assert_eq!(row.book_stage.as_deref(), Some("import_committed"));
        assert_eq!(row.tax_returns, 1);
        assert_eq!(row.client_ref, "qbo-client-26e212deac6c0219bd4a");
        let key = rows.key("qbo-1589548-alberta-ltd").unwrap();
        assert!(rows.refs_of(&key).contains("qbo-1589548-alberta-ltd"));
    }

    #[test]
    fn alias_groups_join_hash_and_slug_and_find_the_book() {
        let items = [
            AliasInput { id: "qbo-client-26e212deac6c0219bd4a", name: Some("1589548 ALBERTA LTD."), source_ref: Some("qbo-selector:idsMenuItem4"), book: false, archived: false, stage: None },
            AliasInput { id: "qbo-1589548-alberta-ltd", name: Some("1589548 Alberta Ltd"), source_ref: None, book: true, archived: false, stage: Some("import_committed") },
            AliasInput { id: "qbo-client-other", name: Some("Other Co"), source_ref: None, book: false, archived: false, stage: None },
        ];
        let g = AliasGroups::build(&items);
        assert_eq!(g.members("qbo-client-26e212deac6c0219bd4a"), vec!["qbo-client-26e212deac6c0219bd4a", "qbo-1589548-alberta-ltd"]);
        assert_eq!(g.book_alias("qbo-client-26e212deac6c0219bd4a").as_deref(), Some("qbo-1589548-alberta-ltd"));
        assert_eq!(g.book_alias("qbo-1589548-alberta-ltd"), None);
        assert_eq!(g.members("qbo-client-other"), vec!["qbo-client-other"]);
        assert_eq!(g.name("qbo-client-26e212deac6c0219bd4a"), Some("1589548 ALBERTA LTD."));
    }

    #[test]
    fn same_name_on_two_roster_entries_stays_separate_and_book_is_not_guessed() {
        let mut rows = Rows::default();
        rows.roster("qbo-client-a", Some("Synergy Welding & Cattle LTD"), Some("qbo-selector:idsMenuItem136"));
        rows.roster("qbo-client-b", Some("Synergy Welding & Cattle LTD."), Some("qbo-selector:idsMenuItem137"));
        rows.book("qbo-synergy-welding-and-cattle-ltd", Some("Synergy Welding & Cattle Ltd"), None, Some("import_committed"));
        assert_eq!(rows.rows.len(), 3);
    }
}
