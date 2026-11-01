//! DT Max original client file preservation.
//!
//! Each original file is kept as immutable bytes, keyed by SHA-256 inside one
//! company scope, with its original path, tax year and file type. The same
//! bytes are stored once; later paths for those bytes are recorded as aliases.
//! Client linkage is a deterministic rule against a per-company linkage table.
//! A file with no single match goes to the unmatched queue until a reviewer
//! links it. Nothing here decodes DT Max content, imports tax cases or files.
//! Pattern follows the engagement attachment lane (hex bytes, hash verified,
//! retained versions never overwritten) and the QuickBooks source lists
//! (company-scoped keys behind the admin gate).

use super::{ImportError, bad, conflict, missing, storage, valid_token};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const FIRST_YEAR: u16 = 2013;
pub const LAST_YEAR: u16 = 2024;
const MAX_PATH: usize = 1024;
const MAX_LIST: usize = 1_000;

type Result<T> = std::result::Result<T, ImportError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClientLinkInput {
    pub client_id: String,
    /// DT Max client file stems (the file name without extension), e.g. "SMITHJ".
    pub dtmax_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileInput {
    pub synthetic: bool,
    pub original_path: String,
    pub year: u16,
    pub file_type: String,
    pub sha256: String,
    pub content_hex: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResolveInput {
    pub client_id: String,
    pub reviewer_label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct FileRecord {
    format: String,
    company_id: String,
    sha256: String,
    bytes: usize,
    original_path: String,
    year: u16,
    file_type: String,
    linkage_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Link {
    client_id: String,
    rule: String,
    reviewer_label: Option<String>,
}

/// Linkage key rule: the file name stem after the last `/` or `\`, upper-cased.
/// A key links only when the company has registered exactly that key to one client.
pub fn linkage_key(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?;
    let stem = match name.rfind('.') {
        Some(0) | None => return None,
        Some(dot) => &name[..dot],
    };
    let key = stem.trim().to_ascii_uppercase();
    valid_token(&key).then_some(key)
}

pub fn extension(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?;
    let dot = name.rfind('.').filter(|d| *d > 0)?;
    let ext = name[dot + 1..].to_ascii_lowercase();
    (!ext.is_empty() && ext.len() <= 8 && ext.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(ext)
}

fn company_ok(company: &str) -> Result<()> {
    if valid_token(company) { Ok(()) } else { Err(bad("Invalid company id")) }
}
fn digest(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn k(company: &str, rest: &str) -> String { format!("dtmaxfile:{company}:{rest}") }
fn s(error: &str) -> ImportError { storage(error) }

impl super::SourceImport {
    pub async fn register_client_links(&self, company: &str, input: ClientLinkInput) -> Result<Value> {
        company_ok(company)?;
        if !valid_token(&input.client_id) || input.dtmax_keys.is_empty() || input.dtmax_keys.len() > 64 {
            return Err(bad("Client id or DT Max keys are invalid"));
        }
        let mut linked = Vec::new();
        for raw in &input.dtmax_keys {
            let key = raw.trim().to_ascii_uppercase();
            if !valid_token(&key) {
                return Err(bad("Invalid DT Max key"));
            }
            let slot = k(company, &format!("key:{key}"));
            self.store.set_nx(&slot, &input.client_id).await.map_err(|e| s(&e))?;
            let owner = self.store.get(&slot).await.map_err(|e| s(&e))?.ok_or_else(|| s("missing key"))?;
            if owner != input.client_id {
                return Err(conflict("DT Max key is already linked to a different client"));
            }
            self.store.sadd(&k(company, "clients"), &input.client_id).await.map_err(|e| s(&e))?;
            linked.push(key);
        }
        Ok(json!({"company_id": company, "client_id": input.client_id, "dtmax_keys": linked}))
    }

    pub async fn preserve_file(&self, company: &str, input: FileInput) -> Result<Value> {
        company_ok(company)?;
        if !input.synthetic {
            return Err(conflict("real_sample_required: real DT Max client files stay blocked until Orville approves a load"));
        }
        let path = input.original_path.as_str();
        if path.is_empty() || path.len() > MAX_PATH || path.chars().any(char::is_control) {
            return Err(bad("Invalid original path"));
        }
        if !(FIRST_YEAR..=LAST_YEAR).contains(&input.year) {
            return Err(bad("Year is outside the 2013-2024 DT Max range"));
        }
        let file_type = input.file_type.to_ascii_lowercase();
        if extension(path).as_deref() != Some(file_type.as_str()) {
            return Err(bad("File type must match the original file extension"));
        }
        let key = linkage_key(path).ok_or_else(|| bad("Original path has no usable client file name"))?;
        if !digest(&input.sha256) {
            return Err(bad("sha256 must be 64 lowercase hex characters"));
        }
        if input.content_hex.len() > MAX_FILE_BYTES * 2 {
            return Err(bad("File exceeds the per-file preservation bound"));
        }
        let bytes = hex::decode(&input.content_hex).map_err(|_| bad("content_hex is not valid hex"))?;
        if hex::encode(&bytes) != input.content_hex {
            return Err(bad("content_hex must be lowercase canonical hex"));
        }
        let actual = hex::encode(Sha256::digest(&bytes));
        if actual != input.sha256 {
            return Err(bad("Declared sha256 does not match the file bytes"));
        }
        let record = FileRecord {
            format: "nxt-dtmax-source-file/v1".into(),
            company_id: company.into(),
            sha256: actual.clone(),
            bytes: bytes.len(),
            original_path: path.into(),
            year: input.year,
            file_type,
            linkage_key: key.clone(),
        };
        let raw = serde_json::to_string(&record).map_err(|_| s("encode"))?;
        // Blob then record, both write-once. Dedupe is by hash within the company.
        self.store.set_nx(&k(company, &format!("blob:{actual}")), &input.content_hex).await.map_err(|e| s(&e))?;
        let created = self.store.set_nx(&k(company, &format!("file:{actual}")), &raw).await.map_err(|e| s(&e))?;
        let saved: FileRecord = self.load(company, &actual).await?;
        let duplicate = !created;
        let path_entry = json!({"original_path": path, "year": input.year}).to_string();
        let alias = duplicate && saved.original_path != path;
        self.store.sadd(&k(company, &format!("paths:{actual}")), &path_entry).await.map_err(|e| s(&e))?;
        self.store.sadd(&k(company, "files"), &actual).await.map_err(|e| s(&e))?;
        let link = self.link_for(company, &saved).await?;
        Ok(self.describe(company, &saved, link, duplicate, alias).await?)
    }

    async fn load(&self, company: &str, sha: &str) -> Result<FileRecord> {
        let raw = self.store.get(&k(company, &format!("file:{sha}"))).await.map_err(|e| s(&e))?.ok_or_else(|| missing("Preserved file not found for this company"))?;
        serde_json::from_str(&raw).map_err(|_| s("corrupt file record"))
    }

    /// Apply the linkage rule once and persist it; an existing link is never replaced.
    async fn link_for(&self, company: &str, record: &FileRecord) -> Result<Option<Link>> {
        let link_key = k(company, &format!("link:{}", record.sha256));
        if let Some(raw) = self.store.get(&link_key).await.map_err(|e| s(&e))? {
            return serde_json::from_str(&raw).map(Some).map_err(|_| s("corrupt link"));
        }
        let owner = self.store.get(&k(company, &format!("key:{}", record.linkage_key))).await.map_err(|e| s(&e))?;
        match owner {
            Some(client_id) => {
                let link = Link { client_id, rule: "dtmax_key_exact".into(), reviewer_label: None };
                let raw = serde_json::to_string(&link).map_err(|_| s("encode"))?;
                self.store.set_nx(&link_key, &raw).await.map_err(|e| s(&e))?;
                let saved = self.store.get(&link_key).await.map_err(|e| s(&e))?.ok_or_else(|| s("missing link"))?;
                Ok(Some(serde_json::from_str(&saved).map_err(|_| s("corrupt link"))?))
            }
            None => {
                self.store.sadd(&k(company, "unmatched"), &record.sha256).await.map_err(|e| s(&e))?;
                Ok(None)
            }
        }
    }

    async fn describe(&self, company: &str, record: &FileRecord, link: Option<Link>, duplicate: bool, alias: bool) -> Result<Value> {
        let mut paths = self.store.smembers(&k(company, &format!("paths:{}", record.sha256))).await.map_err(|e| s(&e))?;
        paths.sort();
        let paths: Vec<Value> = paths.iter().filter_map(|p| serde_json::from_str(p).ok()).collect();
        Ok(json!({
            "company_id": company,
            "sha256": record.sha256,
            "bytes": record.bytes,
            "original_path": record.original_path,
            "year": record.year,
            "file_type": record.file_type,
            "linkage_key": record.linkage_key,
            "client_id": link.as_ref().map(|l| l.client_id.clone()),
            "linkage_rule": link.as_ref().map(|l| l.rule.clone()),
            "linked_by": link.as_ref().and_then(|l| l.reviewer_label.clone()),
            "unmatched": link.is_none(),
            "duplicate_hash": duplicate,
            "path_alias_recorded": alias,
            "observed_paths": paths,
            "immutable": true,
            "decoder_verified": false,
            "tax_cases_imported": 0,
            "filing_enabled": false,
            "write_back": false,
        }))
    }

    pub async fn source_file(&self, company: &str, sha: &str) -> Result<Value> {
        company_ok(company)?;
        if !digest(sha) { return Err(bad("Invalid sha256")); }
        let record = self.load(company, sha).await?;
        let link = self.store.get(&k(company, &format!("link:{sha}"))).await.map_err(|e| s(&e))?
            .map(|raw| serde_json::from_str::<Link>(&raw)).transpose().map_err(|_| s("corrupt link"))?;
        self.describe(company, &record, link, false, false).await
    }

    /// Returns verified bytes; a stored blob whose hash no longer matches is refused.
    pub async fn source_file_bytes(&self, company: &str, sha: &str) -> Result<Vec<u8>> {
        company_ok(company)?;
        if !digest(sha) { return Err(bad("Invalid sha256")); }
        self.load(company, sha).await?;
        let raw = self.store.get(&k(company, &format!("blob:{sha}"))).await.map_err(|e| s(&e))?.ok_or_else(|| s("missing blob"))?;
        let bytes = hex::decode(raw).map_err(|_| s("corrupt blob"))?;
        if hex::encode(Sha256::digest(&bytes)) != sha { return Err(s("Preserved bytes failed hash verification")); }
        Ok(bytes)
    }

    pub async fn unmatched_files(&self, company: &str) -> Result<Value> {
        company_ok(company)?;
        let mut shas = self.store.smembers(&k(company, "unmatched")).await.map_err(|e| s(&e))?;
        shas.sort();
        let mut items = Vec::new();
        for sha in &shas {
            if self.store.get(&k(company, &format!("link:{sha}"))).await.map_err(|e| s(&e))?.is_some() { continue; }
            if items.len() >= MAX_LIST { break; }
            items.push(self.source_file(company, sha).await?);
        }
        Ok(json!({"company_id": company, "unmatched": items.len(), "files": items}))
    }

    /// Manual resolution from the unmatched queue. Write-once: a linked file cannot be relinked.
    pub async fn resolve_file(&self, company: &str, sha: &str, input: ResolveInput) -> Result<Value> {
        company_ok(company)?;
        if !digest(sha) || !valid_token(&input.client_id) || !valid_token(&input.reviewer_label) {
            return Err(bad("Invalid sha256, client id or reviewer label"));
        }
        let record = self.load(company, sha).await?;
        let clients = self.store.smembers(&k(company, "clients")).await.map_err(|e| s(&e))?;
        if !clients.contains(&input.client_id) {
            return Err(missing("Client is not registered for this company"));
        }
        let link = Link { client_id: input.client_id.clone(), rule: "reviewer_resolved".into(), reviewer_label: Some(input.reviewer_label) };
        let raw = serde_json::to_string(&link).map_err(|_| s("encode"))?;
        let link_key = k(company, &format!("link:{sha}"));
        self.store.set_nx(&link_key, &raw).await.map_err(|e| s(&e))?;
        let saved: Link = serde_json::from_str(&self.store.get(&link_key).await.map_err(|e| s(&e))?.ok_or_else(|| s("missing link"))?).map_err(|_| s("corrupt link"))?;
        if saved.client_id != input.client_id {
            return Err(conflict("File is already linked to a different client"));
        }
        self.describe(company, &record, Some(saved), false, false).await
    }

    pub async fn client_files(&self, company: &str, client: &str) -> Result<Value> {
        company_ok(company)?;
        if !valid_token(client) { return Err(bad("Invalid client id")); }
        let mut shas = self.store.smembers(&k(company, "files")).await.map_err(|e| s(&e))?;
        shas.sort();
        let mut items = Vec::new();
        for sha in &shas {
            let Some(raw) = self.store.get(&k(company, &format!("link:{sha}"))).await.map_err(|e| s(&e))? else { continue };
            let link: Link = serde_json::from_str(&raw).map_err(|_| s("corrupt link"))?;
            if link.client_id == client && items.len() < MAX_LIST {
                items.push(self.source_file(company, sha).await?);
            }
        }
        Ok(json!({"company_id": company, "client_id": client, "files": items}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linkage_key_rule() {
        assert_eq!(linkage_key("Y:\\DTMax\\2019\\smithj.t19").as_deref(), Some("SMITHJ"));
        assert_eq!(linkage_key("Y:/DTMax/2013/A-1.dt13").as_deref(), Some("A-1"));
        assert_eq!(linkage_key("Y:\\DTMax\\.hidden"), None);
        assert_eq!(linkage_key("Y:\\DTMax\\noext"), None);
        assert_eq!(extension("Y:\\x\\SMITHJ.T19").as_deref(), Some("t19"));
    }
}
