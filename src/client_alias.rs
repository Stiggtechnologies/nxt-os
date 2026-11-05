//! Canonical company alias resolution. The source roster names a company `qbo-client-<hash>` and
//! its imported book `qbo-<slug>`; one company can arrive under either id. Before routing, a
//! per-company path segment under journeys, company books, the company ledger and the Nixon
//! client view is moved to the alias that already holds that module's record, so every screen
//! reads the same record whichever id the user picked. Nothing here creates or changes a record.
//! The original id travels as [`AliasFrom`] so staff assigned under either id keep access.

use crate::{client_directory::{AliasGroups, Directory}, client_journey::service::Journeys, ledger::routes::company_book_open, store::Store};
use axum::{extract::{Request, State}, http::Uri, middleware::Next, response::Response};
use std::{sync::{Arc, Mutex}, time::{Duration, Instant}};

const TTL: Duration = Duration::from_secs(15);

/// The id the caller asked for, when the resolver moved the request to an alias of it.
#[derive(Clone, Debug)]
pub struct AliasFrom(pub String);

pub struct Resolver {
    pub directory: Arc<Directory>,
    pub journeys: Arc<Journeys>,
    pub ledger_store: Store,
    cache: Mutex<Option<(Instant, AliasGroups)>>,
}

const BASES: [&str; 4] = ["/api/journeys/", "/api/company-books/", "/api/company-ledger/", "/api/nixon/clients/"];
/// First segments under those bases that are collections, not company ids.
const RESERVED: [&str; 3] = ["coverage", "reviews", "roster"];

impl Resolver {
    pub fn new(directory: Arc<Directory>, journeys: Arc<Journeys>, ledger_store: Store) -> Self {
        Self { directory, journeys, ledger_store, cache: Mutex::new(None) }
    }

    async fn groups(&self) -> AliasGroups {
        if let Some((at, g)) = self.cache.lock().unwrap().as_ref() { if at.elapsed() < TTL { return g.clone(); } }
        let g = self.directory.alias_groups().await;
        *self.cache.lock().unwrap() = Some((Instant::now(), g.clone()));
        g
    }

    async fn has_journey(&self, id: &str) -> bool { self.journeys.get(id).await.is_ok() }
    async fn has_ledger(&self, id: &str) -> bool { company_book_open(&self.ledger_store, id).await.unwrap_or(false) }

    /// The alias a request for `id` under `base` should read, or None to leave it as is.
    pub async fn resolve(&self, base: &str, id: &str) -> Option<String> {
        let g = self.groups().await;
        let members = g.members(id);
        if members.len() < 2 { return None; }
        let others = &members[1..];
        match base {
            "/api/company-books/" => g.book_alias(id),
            "/api/company-ledger/" => {
                if self.has_ledger(id).await { return None; }
                for m in others { if self.has_ledger(m).await { return Some(m.clone()); } }
                // No ledger yet: one is opened (and then found) under the id holding the imported book,
                // so the book and the ledger of one company never split.
                g.book_alias(id)
            }
            "/api/journeys/" => {
                if self.has_journey(id).await { return None; }
                for m in others { if self.has_journey(m).await { return Some(m.clone()); } }
                None
            }
            _ => {
                if g.has_book(id) || self.has_journey(id).await || self.has_ledger(id).await { return None; }
                if let Some(b) = g.book_alias(id) { return Some(b); }
                for m in others { if self.has_journey(m).await || self.has_ledger(m).await { return Some(m.clone()); } }
                None
            }
        }
    }
}

pub async fn rewrite(State(r): State<Arc<Resolver>>, mut req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let Some(base) = BASES.iter().find(|b| path.starts_with(**b)) else { return next.run(req).await };
    let rest = &path[base.len()..];
    let (seg, tail) = rest.split_once('/').map(|(a, b)| (a, Some(b))).unwrap_or((rest, None));
    let id = percent_decode(seg);
    if id.is_empty() || RESERVED.contains(&id.as_str()) { return next.run(req).await; }
    if let Some(to) = r.resolve(base, &id).await {
        let mut p = format!("{base}{to}");
        if let Some(t) = tail { p.push('/'); p.push_str(t); }
        if let Some(q) = req.uri().query() { p.push('?'); p.push_str(q); }
        if let Ok(uri) = p.parse::<Uri>() {
            *req.uri_mut() = uri;
            req.extensions_mut().insert(AliasFrom(id));
        }
    }
    next.run(req).await
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}
