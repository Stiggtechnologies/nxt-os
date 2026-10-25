//! Key-value storage. Uses Upstash Redis over its REST API when
//! `KV_REST_API_URL` / `KV_REST_API_TOKEN` (or the `UPSTASH_REDIS_REST_*`
//! equivalents) are set, which is what the Vercel Marketplace Upstash
//! integration injects. Otherwise falls back to process memory, which is fine
//! for local development and tests but is NOT durable on serverless.

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub type StoreResult<T> = Result<T, String>;

#[derive(Default)]
pub struct Memory {
    strings: HashMap<String, String>,
    sets: HashMap<String, HashSet<String>>,
    lists: HashMap<String, Vec<String>>,
}

pub enum Store {
    Memory(Mutex<Memory>),
    Redis {
        url: String,
        token: String,
        http: reqwest::Client,
    },
}

const PREFIX: &str = "nxt:";

impl Store {
    pub fn memory() -> Self {
        Store::Memory(Mutex::new(Memory::default()))
    }

    pub fn from_env() -> Self {
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
            _ => Store::memory(),
        }
    }

    pub fn backend_name(&self) -> &'static str {
        match self {
            Store::Memory(_) => "memory",
            Store::Redis { .. } => "redis",
        }
    }

    async fn redis(&self, cmd: Vec<Value>) -> StoreResult<Value> {
        let Store::Redis { url, token, http } = self else {
            unreachable!("redis command on memory store");
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
            Store::Redis { .. } => Ok(self
                .redis(vec![json!("GET"), json!(key)])
                .await?
                .as_str()
                .map(str::to_string)),
        }
    }

    pub async fn set(&self, key: &str, value: &str) -> StoreResult<()> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                m.lock().unwrap().strings.insert(key, value.to_string());
                Ok(())
            }
            Store::Redis { .. } => self
                .redis(vec![json!("SET"), json!(key), json!(value)])
                .await
                .map(|_| ()),
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
                m.strings.insert(key, value.to_string());
                Ok(true)
            }
            Store::Redis { .. } => Ok(!self
                .redis(vec![json!("SET"), json!(key), json!(value), json!("NX")])
                .await?
                .is_null()),
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
                m.strings.insert(key, value.to_string());
                Ok(true)
            }
            Store::Redis { .. } => {
                let script = "local current = redis.call('GET', KEYS[1]); if (ARGV[1] == 'missing' and not current) or (ARGV[1] == 'present' and current == ARGV[2]) then redis.call('SET', KEYS[1], ARGV[3]); return 1; end; return 0;";
                let result = self.redis(vec![json!("EVAL"), json!(script), json!(1), json!(key), json!(if expected.is_some() { "present" } else { "missing" }), json!(expected.unwrap_or("")), json!(value)]).await?;
                Ok(result.as_i64() == Some(1))
            }
        }
    }

    pub async fn incr_by(&self, key: &str, by: i64) -> StoreResult<i64> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                let cur: i64 = m.strings.get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);
                m.strings.insert(key, (cur + by).to_string());
                Ok(cur + by)
            }
            Store::Redis { .. } => Ok(self
                .redis(vec![json!("INCRBY"), json!(key), json!(by)])
                .await?
                .as_i64()
                .unwrap_or(0)),
        }
    }

    pub async fn sadd(&self, key: &str, member: &str) -> StoreResult<()> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                m.lock().unwrap().sets.entry(key).or_default().insert(member.to_string());
                Ok(())
            }
            Store::Redis { .. } => self
                .redis(vec![json!("SADD"), json!(key), json!(member)])
                .await
                .map(|_| ()),
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
            Store::Redis { .. } => Ok(strings(self.redis(vec![json!("SMEMBERS"), json!(key)]).await?)),
        }
    }

    /// Prepends to a list (newest first), keeping at most `cap` entries.
    pub async fn lpush_capped(&self, key: &str, value: &str, cap: usize) -> StoreResult<()> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                let list = m.lists.entry(key).or_default();
                list.insert(0, value.to_string());
                list.truncate(cap);
                Ok(())
            }
            Store::Redis { .. } => {
                self.redis(vec![json!("LPUSH"), json!(key), json!(value)]).await?;
                self.redis(vec![json!("LTRIM"), json!(key), json!(0), json!(cap as i64 - 1)])
                    .await
                    .map(|_| ())
            }
        }
    }

    pub async fn lrange(&self, key: &str, limit: usize) -> StoreResult<Vec<String>> {
        let key = format!("{PREFIX}{key}");
        match self {
            Store::Memory(m) => Ok(m
                .lock()
                .unwrap()
                .lists
                .get(&key)
                .map(|l| l.iter().take(limit).cloned().collect())
                .unwrap_or_default()),
            Store::Redis { .. } => Ok(strings(
                self.redis(vec![json!("LRANGE"), json!(key), json!(0), json!(limit as i64 - 1)])
                    .await?,
            )),
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
            Store::Redis { .. } => {
                let mut out = Vec::with_capacity(full.len());
                for chunk in full.chunks(500) {
                    let mut cmd = vec![json!("MGET")];
                    cmd.extend(chunk.iter().map(|k| json!(k)));
                    let v = self.redis(cmd).await?;
                    let arr = v.as_array().cloned().unwrap_or_default();
                    out.extend(arr.into_iter().map(|x| x.as_str().map(str::to_string)));
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

fn strings(v: Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
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
