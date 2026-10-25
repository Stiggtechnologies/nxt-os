//! Outbound webhooks, signed with HMAC-SHA256 over the raw body.
//! Receivers verify `X-RS-Signature: sha256=<hex>` using the shared secret.

use super::models::Settings;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::json;
use sha2::Sha256;
use std::time::Duration;

pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Delivers an event; failures are logged, never surfaced to the caller.
pub async fn send(settings: &Settings, kind: &str, data: serde_json::Value) -> Option<u16> {
    let url = settings.webhook_url.trim();
    if url.is_empty() || !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let body = serde_json::to_vec(&json!({
        "type": kind,
        "created_at": super::engine::now(),
        "data": data,
    }))
    .unwrap();
    let mut req = reqwest::Client::new()
        .post(url)
        .timeout(Duration::from_secs(4))
        .header("content-type", "application/json")
        .header("x-rs-event", kind);
    if !settings.webhook_secret.is_empty() {
        req = req.header("x-rs-signature", sign(&settings.webhook_secret, &body));
    }
    match req.body(body).send().await {
        Ok(r) => Some(r.status().as_u16()),
        Err(e) => {
            eprintln!("webhook {kind} to {url} failed: {e}");
            None
        }
    }
}
