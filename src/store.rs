//! Durable storage via Supabase PostgreSQL RPC or Upstash Redis REST.
//! Supabase requires a project origin, server secret, and deployment namespace;
//! partial configuration fails closed. Without Supabase settings, Redis is used
//! when the `KV_REST_API_*` or `UPSTASH_REDIS_REST_*` pair is configured.
//! Otherwise process memory supports local development and tests, but is not
//! durable on serverless.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, OnceLock};

pub type StoreResult<T> = Result<T, String>;

#[derive(Default)]
pub struct Memory {
    strings: HashMap<String, String>,
    sets: HashMap<String, HashSet<String>>,
    lists: HashMap<String, Vec<String>>,
    /// Optional append-only JSONL log (NXT_DEV_STORE_FILE) so a local test copy
    /// survives a restart. Each line records the full resulting state of one key.
    log: Option<std::fs::File>,
}

impl Memory {
    fn open(path: &str) -> StoreResult<Self> {
        let mut m = Memory::default();
        if let Ok(file) = std::fs::File::open(path) {
            for line in std::io::BufReader::new(file).lines() {
                let line = line.map_err(|_| "dev store file could not be read".to_string())?;
                if line.trim().is_empty() { continue; }
                let rec: Value = serde_json::from_str(&line).map_err(|_| "dev store file is corrupt".to_string())?;
                let key = rec["k"].as_str().ok_or("dev store record is missing its key")?.to_string();
                match (rec["t"].as_str(), &rec["v"]) {
                    (Some("s"), Value::String(v)) => { m.strings.insert(key, v.clone()); }
                    (Some("m"), Value::String(v)) => { m.sets.entry(key).or_default().insert(v.clone()); }
                    (Some("l"), Value::Array(v)) => { m.lists.insert(key, v.iter().filter_map(|x| x.as_str().map(String::from)).collect()); }
                    _ => return Err("dev store record is invalid".into()),
                }
            }
        }
        m.log = Some(std::fs::OpenOptions::new().create(true).append(true).open(path)
            .map_err(|_| "dev store file could not be opened".to_string())?);
        Ok(m)
    }

    fn record(&mut self, kind: &str, key: &str, value: Value) -> StoreResult<()> {
        let Some(log) = self.log.as_mut() else { return Ok(()) };
        let mut line = serde_json::to_vec(&json!({"t": kind, "k": key, "v": value})).map_err(|_| "dev store encode failed".to_string())?;
        line.push(b'\n');
        log.write_all(&line).and_then(|_| log.flush()).map_err(|_| "dev store file write failed".to_string())
    }

    fn put(&mut self, key: String, value: String) -> StoreResult<()> {
        self.record("s", &key, json!(value))?;
        self.strings.insert(key, value);
        Ok(())
    }
}

pub enum Store {
    Memory(Arc<Mutex<Memory>>),
    Redis {
        url: String,
        token: String,
        http: reqwest::Client,
    },
    Supabase {
        url: String,
        token: String,
        namespace: String,
        http: reqwest::Client,
    },
    Unavailable { reason: String },
}

const PREFIX: &str = "nxt:";

impl Store {
    pub fn memory() -> Self {
        Store::Memory(Arc::new(Mutex::new(Memory::default())))
    }

    /// Local test copy persisted to an append-only file. All stores in the
    /// process share it, matching a shared production backend.
    pub fn memory_file(path: &str) -> Self {
        static SHARED: OnceLock<Result<Arc<Mutex<Memory>>, String>> = OnceLock::new();
        match SHARED.get_or_init(|| Memory::open(path).map(|m| Arc::new(Mutex::new(m)))) {
            Ok(m) => Store::Memory(m.clone()),
            Err(reason) => Store::Unavailable { reason: reason.clone() },
        }
    }

    pub fn from_env() -> Self {
        let supabase_url = std::env::var("SUPABASE_URL").ok();
        let secret = std::env::var("SUPABASE_SECRET_KEY").ok()
            .or_else(|| std::env::var("SUPABASE_SERVICE_ROLE_KEY").ok());
        let namespace = std::env::var("NXT_STORE_NAMESPACE").ok();
        if supabase_url.is_some() || secret.is_some() || namespace.is_some() {
            return match (supabase_url, secret, namespace) {
                (Some(url), Some(token), Some(namespace)) => Self::supabase(&url, &token, &namespace)
                    .unwrap_or_else(|reason| Store::Unavailable { reason }),
                _ => Store::Unavailable { reason: "Supabase storage requires SUPABASE_URL, a server secret, and NXT_STORE_NAMESPACE".into() },
            };
        }
        let url = std::env::var("KV_REST_API_URL")
            .or_else(|_| std::env::var("UPSTASH_REDIS_REST_URL"))
            .ok();
        let token = std::env::var("KV_REST_API_TOKEN")
            .or_else(|_| std::env::var("UPSTASH_REDIS_REST_TOKEN"))
            .ok();
        match (url, token) {
            (Some(url), Some(token)) if !url.is_empty() && !token.is_empty() => Store::Redis {
                url: url.trim_end_matches('/').to_string(),
                token,
                http: reqwest::Client::new(),
            },
            _ => match std::env::var("NXT_DEV_STORE_FILE") {
                Ok(path) if !path.is_empty() => Store::memory_file(&path),
                _ => Store::memory(),
            },
        }
    }

    pub fn supabase(url: &str, token: &str, namespace: &str) -> StoreResult<Self> {
        let parsed = url::Url::parse(url).map_err(|_| "Supabase URL must be a valid project origin".to_string())?;
        let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]" | "::1"));
        if !(parsed.scheme() == "https" || (parsed.scheme() == "http" && loopback))
            || parsed.host_str().is_none() || !parsed.username().is_empty() || parsed.password().is_some()
            || !matches!(parsed.path(), "" | "/") || parsed.query().is_some() || parsed.fragment().is_some() {
            return Err("Supabase URL must be an HTTPS origin without credentials, path, query, or fragment".into());
        }
        let valid_namespace = !namespace.is_empty() && namespace.len() <= 64
            && namespace.as_bytes()[0].is_ascii_alphanumeric()
            && namespace.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c));
        if !valid_namespace { return Err("NXT_STORE_NAMESPACE must be 1–64 letters, digits, dots, underscores, colons or hyphens, starting with a letter or digit".into()); }
        if token.trim().is_empty() || token != token.trim() || token.chars().any(char::is_control)
            || token.starts_with("sb_publishable_") {
            return Err("Supabase storage requires a nonempty server secret key".into());
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none()).build()
            .map_err(|_| "Supabase HTTP client could not be initialized".to_string())?;
        Ok(Store::Supabase { url: parsed.as_str().trim_end_matches('/').into(), token: token.into(), namespace: namespace.into(), http })
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Store::Memory(_) => "memory",
            Store::Redis { .. } => "redis",
            Store::Supabase { .. } => "supabase",
            Store::Unavailable { .. } => "unavailable",
        }
    }

    async fn remote(&self, cmd: Vec<Value>) -> StoreResult<Value> {
        if let Store::Unavailable { reason } = self { return Err(reason.clone()); }
        if let Store::Supabase { url, token, namespace, http } = self {
            // Service role is the intended server credential for this RPC. Company-books
            // keys are catalogued and appended to the company access audit inside it.
            let mut request = http.post(format!("{url}/rest/v1/rpc/nxt_store_command"))
                .header("apikey", token).json(&json!({"p_namespace": namespace, "p_command": cmd}));
            // Modern secret keys are API keys, not JWT bearer tokens.
            if !token.starts_with("sb_secret_") { request = request.bearer_auth(token); }
            let mut response = request.send().await.map_err(|_| "Supabase storage request failed".to_string())?;
            if !response.status().is_success() { return Err(format!("Supabase storage request returned HTTP {}", response.status().as_u16())); }
            const MAX_RESPONSE: usize = 24 * 1024 * 1024;
            if response.content_length().is_some_and(|n| n > MAX_RESPONSE as u64) { return Err("Supabase storage response exceeded its limit".into()); }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| "Supabase storage response could not be read".to_string())? {
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE { return Err("Supabase storage response exceeded its limit".into()); }
                bytes.extend_from_slice(&chunk);
            }
            let body: Value = serde_json::from_slice(&bytes).map_err(|_| "Supabase storage returned invalid JSON".to_string())?;
            return body.get("result").cloned().ok_or_else(|| "Supabase storage response is missing result".into());
        }
        let Store::Redis { url, token, http } = self else {
            return Err("remote storage is unavailable".into());
        };
        let resp = http
            .post(url)
            .bearer_auth(token)
            .json(&cmd)
            .send()
            .await
            .map_err(|e| format!("redis request failed: {e}"))?;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("redis response invalid: {e}"))?;
        if let Some(err) = body.get("error") {
            return Err(format!("redis error: {err}"));
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn get(&self, key: &str) -> StoreResult<Option<String>> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => Ok(m.lock().unwrap().strings.get(&key).cloned()),
            _ => optional_string(self.remote(vec![json!("GET"), json!(key)]).await?),
        }
    }

    pub async fn set(&self, key: &str, value: &str) -> StoreResult<()> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                m.lock().unwrap().put(key, value.to_string())
            }
            _ => {
                let result = self.remote(vec![json!("SET"), json!(key), json!(value)]).await?;
                if result == json!("OK") { Ok(()) } else { Err("storage SET response was invalid".into()) }
            },
        }
    }

    /// Sets the key only if it does not exist. Returns true if it was set.
    pub async fn set_nx(&self, key: &str, value: &str) -> StoreResult<bool> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                if m.strings.contains_key(&key) {
                    return Ok(false);
                }
                m.put(key, value.to_string())?;
                Ok(true)
            }
            _ => match self.remote(vec![json!("SET"), json!(key), json!(value), json!("NX")]).await? {
                Value::Null => Ok(false),
                value if value == json!("OK") => Ok(true),
                _ => Err("storage conditional SET response was invalid".into()),
            },
        }
    }

    /// Atomically publish an update only while the previously read value still
    /// matches. Used by Firm Operations to commit approvals and their effects
    /// together, including when several serverless instances handle requests.
    pub async fn compare_exchange(&self, key: &str, expected: Option<&str>, value: &str) -> StoreResult<bool> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().map_err(|_| "storage lock poisoned".to_string())?;
                if m.strings.get(&key).map(String::as_str) != expected {
                    return Ok(false);
                }
                m.put(key, value.to_string())?;
                Ok(true)
            }
            Store::Redis { .. } => {
                let script = "local current = redis.call('GET', KEYS[1]); if (ARGV[1] == 'missing' and not current) or (ARGV[1] == 'present' and current == ARGV[2]) then redis.call('SET', KEYS[1], ARGV[3]); return 1; end; return 0;";
                let result = self.remote(vec![json!("EVAL"), json!(script), json!(1), json!(key), json!(if expected.is_some() { "present" } else { "missing" }), json!(expected.unwrap_or("")), json!(value)]).await?;
                decision(result)
            }
            Store::Supabase { .. } => decision(self.remote(vec![json!("COMPARE_EXCHANGE"), json!(key), json!(expected), json!(value)]).await?),
            Store::Unavailable { reason } => Err(reason.clone()),
        }
    }

    pub async fn incr_by(&self, key: &str, by: i64) -> StoreResult<i64> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                let cur: i64 = m.strings.get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);
                let next = cur.checked_add(by).ok_or_else(|| "storage counter overflow".to_string())?;
                m.put(key, next.to_string())?;
                Ok(next)
            }
            _ => self.remote(vec![json!("INCRBY"), json!(key), json!(by)]).await?.as_i64().ok_or_else(|| "storage counter response was invalid".into()),
        }
    }

    pub async fn sadd(&self, key: &str, member: &str) -> StoreResult<()> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                m.record("m", &key, json!(member))?;
                m.sets.entry(key).or_default().insert(member.to_string());
                Ok(())
            }
            _ => {
                let result = self.remote(vec![json!("SADD"), json!(key), json!(member)]).await?;
                if result.as_i64().is_some_and(|n| n >= 0) { Ok(()) } else { Err("storage set response was invalid".into()) }
            },
        }
    }

    pub async fn smembers(&self, key: &str) -> StoreResult<Vec<String>> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => Ok(m
                .lock()
                .unwrap()
                .sets
                .get(&key)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default()),
            _ => strings(self.remote(vec![json!("SMEMBERS"), json!(key)]).await?),
        }
    }

    /// Prepends to a list (newest first), keeping at most `cap` entries.
    pub async fn lpush_capped(&self, key: &str, value: &str, cap: usize) -> StoreResult<()> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                let mut list = m.lists.get(&key).cloned().unwrap_or_default();
                list.insert(0, value.to_string());
                list.truncate(cap);
                m.record("l", &key, json!(list))?;
                m.lists.insert(key, list);
                Ok(())
            }
            Store::Redis { .. } => {
                self.remote(vec![json!("LPUSH"), json!(key), json!(value)]).await?;
                self.remote(vec![json!("LTRIM"), json!(key), json!(0), json!(cap as i64 - 1)])
                    .await
                    .map(|_| ())
            }
            Store::Supabase { .. } => {
                let value = self.remote(vec![json!("LPUSH_CAPPED"), json!(key), json!(value), json!(cap)]).await?;
                if value.as_i64().is_some_and(|n| n >= 0) { Ok(()) } else { Err("storage list response was invalid".into()) }
            },
            Store::Unavailable { reason } => Err(reason.clone()),
        }
    }

    pub async fn lrange(&self, key: &str, limit: usize) -> StoreResult<Vec<String>> {
        if limit == 0 { return Ok(Vec::new()); }
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => Ok(m
                .lock()
                .unwrap()
                .lists
                .get(&key)
                .map(|l| l.iter().take(limit).cloned().collect())
                .unwrap_or_default()),
            _ => strings(
                self.remote(vec![json!("LRANGE"), json!(key), json!(0), json!(limit as i64 - 1)])
                    .await?,
            ),
        }
    }

    pub async fn mget(&self, keys: &[String]) -> StoreResult<Vec<Option<String>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let full: Vec<String> = keys.iter().map(|k| format!("{PREFIX}{k}")).collect();
        match self {
            Store::Memory(m) => {
                let m = m.lock().unwrap();
                Ok(full.iter().map(|k| m.strings.get(k).cloned()).collect())
            }
            _ => {
                let mut out = Vec::with_capacity(full.len());
                for chunk in full.chunks(500) {
                    let mut cmd = vec![json!("MGET")];
                    cmd.extend(chunk.iter().map(|k| json!(k)));
                    let v = self.remote(cmd).await?;
                    let arr = v.as_array().ok_or_else(|| "storage MGET response was invalid".to_string())?;
                    if arr.len() != chunk.len() { return Err("storage MGET response length was invalid".into()); }
                    for value in arr { out.push(optional_string(value.clone())?); }
                }
                Ok(out)
            }
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> StoreResult<Option<T>> {
        match self.get(key).await? {
            Some(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|e| format!("corrupt record at {key}: {e}")),
            None => Ok(None),
        }
    }

    pub async fn set_json<T: Serialize>(&self, key: &str, value: &T) -> StoreResult<()> {
        self.set(key, &serde_json::to_string(value).unwrap()).await
    }

    pub async fn list_json<T: DeserializeOwned>(&self, key: &str, limit: usize) -> StoreResult<Vec<T>> {
        Ok(self
            .lrange(key, limit)
            .await?
            .iter()
            .filter_map(|raw| serde_json::from_str(raw).ok())
            .collect())
    }
}

fn strings(v: Value) -> StoreResult<Vec<String>> {
    let values = v.as_array().ok_or_else(|| "storage list response was invalid".to_string())?;
    values.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(|| "storage list item was invalid".into())).collect()
}

fn optional_string(value: Value) -> StoreResult<Option<String>> {
    match value { Value::Null => Ok(None), Value::String(s) => Ok(Some(s)), _ => Err("storage value response was invalid".into()) }
}

fn decision(value: Value) -> StoreResult<bool> {
    match value.as_i64() { Some(0) => Ok(false), Some(1) => Ok(true), _ => Err("storage comparison response was invalid".into()) }
}

#[cfg(test)]
mod compare_exchange_tests {
    use super::Store;
    use std::sync::Arc;

    #[tokio::test]
    async fn missing_empty_and_stale_values_are_distinct() {
        let store = Store::memory();
        assert!(store.compare_exchange("firm", None, "").await.unwrap());
        assert!(!store.compare_exchange("firm", None, "lost").await.unwrap());
        assert!(store.compare_exchange("firm", Some(""), "approved").await.unwrap());
        assert!(!store.compare_exchange("firm", Some(""), "rejected").await.unwrap());
        assert_eq!(store.get("firm").await.unwrap().as_deref(), Some("approved"));
    }

    #[tokio::test]
    async fn only_one_concurrent_approval_can_publish() {
        let store = Arc::new(Store::memory());
        store.set("proposal", "pending").await.unwrap();
        let a = store.clone();
        let b = store.clone();
        let (a, b) = tokio::join!(
            async move { a.compare_exchange("proposal", Some("pending"), "approved").await.unwrap() },
            async move { b.compare_exchange("proposal", Some("pending"), "rejected").await.unwrap() }
        );
        assert_ne!(a, b, "one decision must win without overwriting it");
    }
}

#[cfg(test)]
mod dev_store_file_tests {
    use super::*;

    fn open(path: &str) -> Store { Store::Memory(Arc::new(Mutex::new(Memory::open(path).unwrap()))) }

    #[tokio::test]
    async fn dev_store_file_survives_restart() {
        let dir = std::env::temp_dir().join(format!("nxt-dev-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("store.jsonl");
        let path = path.to_str().unwrap();
        {
            let s = open(path);
            s.set("a", "1").await.unwrap();
            assert!(s.set_nx("b", "x").await.unwrap());
            assert!(!s.set_nx("b", "y").await.unwrap());
            assert!(s.compare_exchange("a", Some("1"), "2").await.unwrap());
            assert_eq!(s.incr_by("n", 5).await.unwrap(), 5);
            s.sadd("set", "m1").await.unwrap();
            s.lpush_capped("l", "v1", 2).await.unwrap();
            s.lpush_capped("l", "v2", 2).await.unwrap();
            s.lpush_capped("l", "v3", 2).await.unwrap();
        }
        let s = open(path);
        assert_eq!(s.get("a").await.unwrap().as_deref(), Some("2"));
        assert_eq!(s.get("b").await.unwrap().as_deref(), Some("x"));
        assert_eq!(s.incr_by("n", 1).await.unwrap(), 6);
        assert_eq!(s.smembers("set").await.unwrap(), vec!["m1".to_string()]);
        assert_eq!(s.lrange("l", 10).await.unwrap(), vec!["v3".to_string(), "v2".to_string()]);
        std::fs::write(dir.join("bad.jsonl"), "not json\n").unwrap();
        assert!(Memory::open(dir.join("bad.jsonl").to_str().unwrap()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
