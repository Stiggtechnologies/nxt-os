//! Stripe Checkout for the guided T1 filing fee (pay-before-file).
//!
//! Default off (`NXT_STRIPE_ENABLED`). Keys come from env; `sk_live_` is refused unless
//! `NXT_STRIPE_LIVE_APPROVED=true` (never set in committed config). The webhook verifies
//! `STRIPE_WEBHOOK_SECRET`, marks the existing Filing Desk `FeeFlag` paid, and posts a
//! firm-ledger journal. Event handling is idempotent per Stripe event id.
use super::{Tax, TResult, bad, conflict, now, audit, filing_desk::FeeFlag};
use crate::firm::staff::{Identity, Role};
use crate::ledger::engine::{JournalInput, Ledger};
use crate::ledger::models::Line;
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::Arc;
use std::time::Duration;

const CASH: &str = "acc_1000";       // Chequing — cash/clearing
const REVENUE: &str = "acc_4100";    // Service Revenue
const FEE_EXP: &str = "acc_6100";    // Bank Charges & Fees
const EVT_PREFIX: &str = "tax:stripe:evt:";

#[derive(Clone, Debug)]
pub struct Config {
    pub enabled: bool,
    pub secret_key: String,
    pub webhook_secret: String,
    pub fee_cents: i64,
    pub currency: String,
    pub product_name: String,
    pub api_base: String,
    pub success_url: String,
    pub cancel_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let secret_key = std::env::var("STRIPE_SECRET_KEY").unwrap_or_default();
        let enabled = std::env::var("NXT_STRIPE_ENABLED").ok().as_deref() == Some("true");
        let fee_cents = std::env::var("NXT_STRIPE_FEE_CENTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(19_900);
        let currency = std::env::var("NXT_STRIPE_CURRENCY").unwrap_or_else(|_| "cad".into()).to_ascii_lowercase();
        let product_name = std::env::var("NXT_STRIPE_PRODUCT_NAME").unwrap_or_else(|_| "T1 filing fee".into());
        let api_base = std::env::var("NXT_STRIPE_API_BASE").unwrap_or_else(|_| "https://api.stripe.com".into());
        let public = std::env::var("NXT_PUBLIC_BASE_URL")
            .or_else(|_| std::env::var("GUARDVOICE_PUBLIC_HOST").map(|h| {
                if h.starts_with("http") { h } else { format!("https://{h}") }
            }))
            .unwrap_or_else(|_| "https://os.nxtcorporation.ca".into());
        let public = public.trim_end_matches('/').to_string();
        let success_url = std::env::var("NXT_STRIPE_SUCCESS_URL").unwrap_or_else(|_| {
            format!("{public}/apps/tax/#tt-t1/{{RETURN_ID}}?screen=pay&session_id={{CHECKOUT_SESSION_ID}}")
        });
        let cancel_url = std::env::var("NXT_STRIPE_CANCEL_URL").unwrap_or_else(|_| {
            format!("{public}/apps/tax/#tt-t1/{{RETURN_ID}}?screen=pay&cancelled=1")
        });
        Self {
            enabled,
            secret_key,
            webhook_secret: std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default(),
            fee_cents,
            currency,
            product_name,
            api_base: api_base.trim_end_matches('/').into(),
            success_url,
            cancel_url,
        }
    }

    /// Validates live-key policy. Returns an error message when the key must be refused.
    pub fn refuse_live_key(&self) -> Option<&'static str> {
        let live_approved = std::env::var("NXT_STRIPE_LIVE_APPROVED").ok().as_deref() == Some("true");
        if self.secret_key.starts_with("sk_live_") && !live_approved {
            Some("live Stripe secret keys are refused unless NXT_STRIPE_LIVE_APPROVED=true")
        } else {
            None
        }
    }

    pub fn ready(&self) -> Result<(), String> {
        if !self.enabled {
            return Err("Stripe checkout is disabled (NXT_STRIPE_ENABLED is not true)".into());
        }
        if self.secret_key.is_empty() {
            return Err("STRIPE_SECRET_KEY is not set".into());
        }
        if let Some(m) = self.refuse_live_key() {
            return Err(m.into());
        }
        if !(100..=10_000_000).contains(&self.fee_cents) {
            return Err("NXT_STRIPE_FEE_CENTS must be between 100 and 10000000".into());
        }
        if self.currency.len() != 3 || !self.currency.chars().all(|c| c.is_ascii_lowercase()) {
            return Err("NXT_STRIPE_CURRENCY must be a 3-letter lowercase code".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StripePayment {
    pub return_id: String,
    pub session_id: String,
    #[serde(default)]
    pub payment_intent_id: Option<String>,
    pub amount_cents: i64,
    pub currency: String,
    #[serde(default)]
    pub stripe_fee_cents: Option<i64>,
    #[serde(default)]
    pub journal_entry_id: Option<String>,
    #[serde(default)]
    pub refund_id: Option<String>,
    #[serde(default)]
    pub refund_journal_entry_id: Option<String>,
    pub status: String,
    pub at: String,
}

#[derive(Clone)]
pub struct StripePay {
    pub tax: Arc<Tax>,
    pub ledger: Arc<Ledger>,
    http: reqwest::Client,
}

impl StripePay {
    pub fn new(tax: Arc<Tax>, ledger: Arc<Ledger>) -> Self {
        // Same pattern as Store: rustls-no-provider needs an explicit crypto install.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { tax, ledger, http }
    }

    pub fn config(&self) -> Config {
        Config::from_env()
    }

    fn unavailable(m: impl Into<String>) -> ApiError {
        ApiError(StatusCode::SERVICE_UNAVAILABLE, m.into())
    }

    /// Public fee quote (amount is always server-side; never trust the client).
    pub fn quote(&self) -> Value {
        let c = self.config();
        json!({
            "enabled": c.enabled && c.ready().is_ok(),
            "disabled_reason": if !c.enabled { Some("NXT_STRIPE_ENABLED is not true".to_string()) } else { c.ready().err() },
            "fee_cents": c.fee_cents,
            "currency": c.currency,
            "product_name": c.product_name,
        })
    }

    pub async fn payment_status(&self, who: &Identity, rid: &str) -> TResult<Value> {
        let r = self.tax.get(rid).await?;
        if !who.all_clients() && !who.can_client(&r.client_ref) {
            return Err(ApiError(StatusCode::FORBIDDEN, "this return is outside your client scope".into()));
        }
        let s = self.tax_load().await?;
        let paid = s.desk.fees.iter().find(|f| f.return_id == rid).is_some_and(|f| f.paid);
        let payment = s.desk.stripe_payments.iter().rev().find(|p| p.return_id == rid).cloned();
        let q = self.quote();
        Ok(json!({
            "return_id": rid,
            "fee_paid": paid,
            "fee_cents": q["fee_cents"],
            "currency": q["currency"],
            "stripe_enabled": q["enabled"],
            "payment": payment,
        }))
    }

    async fn tax_load(&self) -> TResult<super::models::TaxState> {
        // Reuse Tax::load via get-on-missing pattern: store key is private, so go through mutate/get.
        // Expose via a thin helper on Tax.
        self.tax.load_state().await
    }

    /// Creates a Stripe Checkout Session for the filing fee. Amount is always from server config.
    pub async fn create_checkout(&self, who: &Identity, rid: &str) -> TResult<Value> {
        if who.break_glass {
            return Err(ApiError(StatusCode::FORBIDDEN, "sign in with your staff account to start checkout".into()));
        }
        let cfg = self.config();
        cfg.ready().map_err(Self::unavailable)?;
        let r = self.tax.get(rid).await?;
        if r.program != "T1" {
            return Err(bad("Stripe checkout is for T1 filing fees only"));
        }
        if !who.all_clients() && !who.can_client(&r.client_ref) {
            return Err(ApiError(StatusCode::FORBIDDEN, "this return is outside your client scope".into()));
        }
        let s = self.tax_load().await?;
        if s.desk.fees.iter().find(|f| f.return_id == rid).is_some_and(|f| f.paid) {
            return Err(conflict("the filing fee is already marked paid"));
        }
        // Reuse an open session if one was started recently and not completed.
        if let Some(p) = s.desk.stripe_payments.iter().rev().find(|p| p.return_id == rid && p.status == "open") {
            if let Ok(existing) = self.stripe_get(&cfg, &format!("/v1/checkout/sessions/{}", p.session_id)).await {
                if existing["status"] == "open" {
                    return Ok(json!({
                        "session_id": p.session_id,
                        "url": existing["url"],
                        "fee_cents": cfg.fee_cents,
                        "currency": cfg.currency,
                        "reused": true,
                    }));
                }
            }
        }
        let success = cfg.success_url.replace("{RETURN_ID}", rid);
        let cancel = cfg.cancel_url.replace("{RETURN_ID}", rid);
        let form = [
            ("mode", "payment".to_string()),
            ("success_url", success),
            ("cancel_url", cancel),
            ("client_reference_id", rid.to_string()),
            ("metadata[return_id]", rid.to_string()),
            ("metadata[product]", "t1_filing_fee".to_string()),
            ("line_items[0][quantity]", "1".to_string()),
            ("line_items[0][price_data][currency]", cfg.currency.clone()),
            ("line_items[0][price_data][unit_amount]", cfg.fee_cents.to_string()),
            ("line_items[0][price_data][product_data][name]", cfg.product_name.clone()),
        ];
        let session = self.stripe_form(&cfg, "/v1/checkout/sessions", &form).await?;
        let sid = session["id"].as_str().ok_or_else(|| bad("Stripe session response missing id"))?.to_string();
        let url = session["url"].as_str().unwrap_or("").to_string();
        let at = now();
        self.tax.mutate_state(|st| {
            st.desk.stripe_payments.retain(|p| !(p.return_id == rid && p.status == "open"));
            st.desk.stripe_payments.push(StripePayment {
                return_id: rid.into(),
                session_id: sid.clone(),
                payment_intent_id: session["payment_intent"].as_str().map(str::to_string),
                amount_cents: cfg.fee_cents,
                currency: cfg.currency.clone(),
                stripe_fee_cents: None,
                journal_entry_id: None,
                refund_id: None,
                refund_journal_entry_id: None,
                status: "open".into(),
                at: at.clone(),
            });
            audit(st, rid, "stripe_checkout_started", format!("session {sid} by {}", who.label()));
            Ok(())
        }).await?;
        Ok(json!({
            "session_id": sid,
            "url": url,
            "fee_cents": cfg.fee_cents,
            "currency": cfg.currency,
            "reused": false,
        }))
    }

    /// Verifies the Stripe-Signature header and handles supported events idempotently.
    pub async fn handle_webhook(&self, signature: Option<&str>, body: &[u8]) -> TResult<Value> {
        let cfg = self.config();
        if !cfg.enabled {
            return Err(Self::unavailable("Stripe webhook refused: NXT_STRIPE_ENABLED is not true"));
        }
        if cfg.webhook_secret.is_empty() {
            return Err(Self::unavailable("STRIPE_WEBHOOK_SECRET is not set"));
        }
        verify_signature(&cfg.webhook_secret, signature.unwrap_or(""), body, 300)
            .map_err(|m| ApiError(StatusCode::UNAUTHORIZED, m))?;
        let event: Value = serde_json::from_slice(body).map_err(|_| bad("webhook body is not JSON"))?;
        let event_id = event["id"].as_str().ok_or_else(|| bad("webhook missing event id"))?.to_string();
        let kind = event["type"].as_str().unwrap_or("");
        // Idempotency: claim the event id once.
        let claimed = self
            .tax
            .store
            .set_nx(&format!("{EVT_PREFIX}{event_id}"), &now())
            .await
            .map_err(|e| ApiError(StatusCode::SERVICE_UNAVAILABLE, e))?;
        if !claimed {
            return Ok(json!({"received": true, "duplicate": true, "event_id": event_id}));
        }
        match kind {
            "checkout.session.completed" => {
                let session = &event["data"]["object"];
                self.on_checkout_completed(&event_id, session).await?;
            }
            _ => {}
        }
        Ok(json!({"received": true, "event_id": event_id, "type": kind}))
    }

    async fn on_checkout_completed(&self, event_id: &str, session: &Value) -> TResult<()> {
        let rid = session["metadata"]["return_id"]
            .as_str()
            .or_else(|| session["client_reference_id"].as_str())
            .ok_or_else(|| bad("checkout session missing return_id metadata"))?
            .to_string();
        let session_id = session["id"].as_str().ok_or_else(|| bad("checkout session missing id"))?.to_string();
        let amount = session["amount_total"].as_i64().unwrap_or(0);
        let currency = session["currency"].as_str().unwrap_or("cad").to_string();
        let pi = session["payment_intent"].as_str().map(str::to_string);
        if amount <= 0 {
            return Err(bad("checkout session amount_total must be positive"));
        }
        // Ensure the return exists.
        let _ = self.tax.get(&rid).await?;
        // Already paid for this session? no-op journal.
        let s = self.tax_load().await?;
        if let Some(p) = s.desk.stripe_payments.iter().find(|p| p.session_id == session_id && p.status == "paid") {
            if p.journal_entry_id.is_some() {
                return Ok(());
            }
        }
        let stripe_fee = self.lookup_stripe_fee(pi.as_deref()).await;
        let entry = self
            .post_payment_journal(&rid, &session_id, amount, stripe_fee, &currency)
            .await?;
        let at = now();
        let by = format!("stripe:{session_id}");
        self.tax.mutate_state(|st| {
            st.desk.fees.retain(|f| f.return_id != rid);
            st.desk.fees.push(FeeFlag {
                return_id: rid.clone(),
                paid: true,
                by: by.clone(),
                at: at.clone(),
                stripe_session_id: Some(session_id.clone()),
            });
            if let Some(p) = st.desk.stripe_payments.iter_mut().find(|p| p.session_id == session_id) {
                p.status = "paid".into();
                p.amount_cents = amount;
                p.currency = currency.clone();
                p.payment_intent_id = pi.clone();
                p.stripe_fee_cents = stripe_fee;
                p.journal_entry_id = Some(entry.id.clone());
                p.at = at.clone();
            } else {
                st.desk.stripe_payments.push(StripePayment {
                    return_id: rid.clone(),
                    session_id: session_id.clone(),
                    payment_intent_id: pi.clone(),
                    amount_cents: amount,
                    currency: currency.clone(),
                    stripe_fee_cents: stripe_fee,
                    journal_entry_id: Some(entry.id.clone()),
                    refund_id: None,
                    refund_journal_entry_id: None,
                    status: "paid".into(),
                    at: at.clone(),
                });
            }
            audit(
                st,
                &rid,
                "filing_fee_paid",
                format!("stripe checkout {session_id}; event {event_id}; journal {}; fee_cents {:?}", entry.id, stripe_fee),
            );
            Ok(())
        })
        .await?;
        Ok(())
    }

    async fn lookup_stripe_fee(&self, payment_intent: Option<&str>) -> Option<i64> {
        let cfg = self.config();
        let pi = payment_intent?;
        let pi_obj = self.stripe_get(&cfg, &format!("/v1/payment_intents/{pi}?expand[]=latest_charge.balance_transaction")).await.ok()?;
        pi_obj["latest_charge"]["balance_transaction"]["fee"]
            .as_i64()
            .or_else(|| {
                // When expand returns an id string only, skip.
                None
            })
    }

    async fn post_payment_journal(
        &self,
        rid: &str,
        session_id: &str,
        amount: i64,
        stripe_fee: Option<i64>,
        currency: &str,
    ) -> TResult<crate::ledger::models::Entry> {
        let fee = stripe_fee.unwrap_or(0).max(0);
        if fee > amount {
            return Err(bad("Stripe fee exceeds payment amount"));
        }
        let net = amount - fee;
        let mut lines = vec![
            Line { account_id: CASH.into(), debit: net, memo: format!("Stripe {session_id}"), ..Default::default() },
            Line { account_id: REVENUE.into(), credit: amount, memo: format!("T1 filing fee {rid} {currency}"), ..Default::default() },
        ];
        if fee > 0 {
            lines.push(Line {
                account_id: FEE_EXP.into(),
                debit: fee,
                memo: format!("Stripe processing fee {session_id}"),
                ..Default::default()
            });
        }
        let date = crate::ledger::engine::today();
        self.ledger
            .create_journal(JournalInput {
                date,
                memo: format!("Stripe checkout {session_id} for {rid}"),
                lines,
            })
            .await
            .map_err(ApiError::from)
    }

    /// Staff-only refund: Stripe Refund API + reversing journal + clear FeeFlag + audit.
    pub async fn refund(&self, who: &Identity, rid: &str) -> TResult<Value> {
        if who.role != Role::Admin || who.break_glass {
            return Err(ApiError(StatusCode::FORBIDDEN, "refunds require an admin staff account".into()));
        }
        let cfg = self.config();
        cfg.ready().map_err(Self::unavailable)?;
        let _ = self.tax.get(rid).await?;
        let s = self.tax_load().await?;
        let payment = s
            .desk
            .stripe_payments
            .iter()
            .rev()
            .find(|p| p.return_id == rid && p.status == "paid")
            .cloned()
            .ok_or_else(|| conflict("no paid Stripe payment to refund for this return"))?;
        if payment.refund_id.is_some() {
            return Err(conflict("this Stripe payment was already refunded"));
        }
        let pi = payment
            .payment_intent_id
            .clone()
            .ok_or_else(|| conflict("payment has no payment_intent; cannot refund via Stripe"))?;
        let refund = self
            .stripe_form(&cfg, "/v1/refunds", &[("payment_intent", pi.clone())])
            .await?;
        let refund_id = refund["id"].as_str().unwrap_or("").to_string();
        if refund_id.is_empty() {
            return Err(bad("Stripe refund response missing id"));
        }
        let amount = payment.amount_cents;
        let fee = payment.stripe_fee_cents.unwrap_or(0).max(0);
        let net = amount - fee;
        let mut lines = vec![
            Line { account_id: REVENUE.into(), debit: amount, memo: format!("Refund filing fee {rid}"), ..Default::default() },
            Line { account_id: CASH.into(), credit: net, memo: format!("Stripe refund {refund_id}"), ..Default::default() },
        ];
        if fee > 0 {
            // Stripe typically keeps the processing fee on full refunds of Checkout; reverse our fee expense if we recorded it.
            lines.push(Line {
                account_id: FEE_EXP.into(),
                credit: fee,
                memo: format!("Reverse Stripe fee {refund_id}"),
                ..Default::default()
            });
        }
        let entry = self
            .ledger
            .create_journal(JournalInput {
                date: crate::ledger::engine::today(),
                memo: format!("Stripe refund {refund_id} for {rid}"),
                lines,
            })
            .await
            .map_err(ApiError::from)?;
        let at = now();
        let by = who.label();
        self.tax.mutate_state(|st| {
            st.desk.fees.retain(|f| f.return_id != rid);
            st.desk.fees.push(FeeFlag {
                return_id: rid.into(),
                paid: false,
                by: by.clone(),
                at: at.clone(),
                stripe_session_id: Some(payment.session_id.clone()),
            });
            if let Some(p) = st.desk.stripe_payments.iter_mut().find(|p| p.session_id == payment.session_id) {
                p.status = "refunded".into();
                p.refund_id = Some(refund_id.clone());
                p.refund_journal_entry_id = Some(entry.id.clone());
                p.at = at.clone();
            }
            audit(
                st,
                rid,
                "filing_fee_refunded",
                format!("stripe refund {refund_id} by {by}; journal {}", entry.id),
            );
            Ok(())
        })
        .await?;
        Ok(json!({
            "return_id": rid,
            "refund_id": refund_id,
            "journal_entry_id": entry.id,
            "fee_paid": false,
            "amount_cents": amount,
        }))
    }

    async fn stripe_form(&self, cfg: &Config, path: &str, fields: &[(&str, String)]) -> TResult<Value> {
        let url = format!("{}{path}", cfg.api_base);
        let body = fields
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding_lite(k), urlencoding_lite(v)))
            .collect::<Vec<_>>()
            .join("&");
        let resp = self
            .http
            .post(&url)
            .header("authorization", format!("Bearer {}", cfg.secret_key))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| Self::unavailable(format!("Stripe request failed: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            // Never echo the secret key; truncate Stripe error bodies.
            let snippet: String = text.chars().take(300).collect();
            return Err(ApiError(
                StatusCode::BAD_GATEWAY,
                format!("Stripe {path} returned {}: {snippet}", status.as_u16()),
            ));
        }
        serde_json::from_str(&text).map_err(|_| bad("Stripe response was not JSON"))
    }

    async fn stripe_get(&self, cfg: &Config, path: &str) -> TResult<Value> {
        let url = format!("{}{path}", cfg.api_base);
        let resp = self
            .http
            .get(&url)
            .header("authorization", format!("Bearer {}", cfg.secret_key))
            .send()
            .await
            .map_err(|e| Self::unavailable(format!("Stripe request failed: {e}")))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            let snippet: String = text.chars().take(300).collect();
            return Err(ApiError(
                StatusCode::BAD_GATEWAY,
                format!("Stripe GET {path} returned {}: {snippet}", status.as_u16()),
            ));
        }
        serde_json::from_str(&text).map_err(|_| bad("Stripe response was not JSON"))
    }
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Stripe-Signature: `t=<unix>,v1=<hex>` over `{t}.{body}` with HMAC-SHA256.
pub fn verify_signature(secret: &str, header: &str, body: &[u8], max_age_secs: i64) -> Result<(), String> {
    let mut ts: Option<i64> = None;
    let mut sigs: Vec<&str> = vec![];
    for part in header.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("t=") {
            ts = v.parse().ok();
        } else if let Some(v) = part.strip_prefix("v1=") {
            sigs.push(v);
        }
    }
    let ts = ts.ok_or_else(|| "Stripe-Signature missing t=".to_string())?;
    if sigs.is_empty() {
        return Err("Stripe-Signature missing v1=".into());
    }
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > max_age_secs {
        return Err("Stripe-Signature timestamp is too old".into());
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(format!("{ts}.").as_bytes());
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    if !sigs.iter().any(|s| constant_time_eq(s.as_bytes(), expected.as_bytes())) {
        return Err("Stripe-Signature mismatch".into());
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Staff checkout/status/refund behind `require_admin`; webhook is signature-only (no staff session).
pub fn router(pay: Arc<StripePay>) -> axum::Router {
    use axum::{
        Router, Extension, Json,
        body::Bytes,
        extract::{Path, State},
        http::{HeaderMap, header},
        middleware,
        response::Response,
        routing::{get, post},
    };
    use crate::firm::staff::Identity;
    use crate::referral::routes::require_admin;

    async fn quote(State(p): State<Arc<StripePay>>) -> Result<Json<Value>, ApiError> {
        Ok(Json(p.quote()))
    }
    async fn status(
        State(p): State<Arc<StripePay>>,
        Extension(who): Extension<Identity>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(p.payment_status(&who, &id).await?))
    }
    async fn checkout(
        State(p): State<Arc<StripePay>>,
        Extension(who): Extension<Identity>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(p.create_checkout(&who, &id).await?))
    }
    async fn refund(
        State(p): State<Arc<StripePay>>,
        Extension(who): Extension<Identity>,
        Path(id): Path<String>,
    ) -> Result<Json<Value>, ApiError> {
        Ok(Json(p.refund(&who, &id).await?))
    }
    async fn webhook(
        State(p): State<Arc<StripePay>>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<Json<Value>, ApiError> {
        let sig = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok());
        Ok(Json(p.handle_webhook(sig, &body).await?))
    }

    let staff = Router::new()
        .route("/api/tax/stripe/quote", get(quote))
        .route("/api/tax/returns/{id}/stripe", get(status))
        .route("/api/tax/returns/{id}/stripe/checkout", post(checkout))
        .route("/api/tax/returns/{id}/stripe/refund", post(refund))
        .layer(middleware::from_fn(require_admin))
        .layer(middleware::map_response(|mut response: Response| async move {
            response.headers_mut().insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
            response
        }))
        .with_state(pay.clone());

    let hook = Router::new()
        .route("/api/tax/stripe/webhook", post(webhook))
        .with_state(pay);

    staff.merge(hook)
}

#[cfg(test)]
mod sig_tests {
    use super::*;

    #[test]
    fn accepts_valid_signature_and_rejects_bad() {
        let secret = "whsec_test_secret";
        let body = br#"{"id":"evt_1","type":"checkout.session.completed"}"#;
        let ts = chrono::Utc::now().timestamp();
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{ts}.").as_bytes());
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={ts},v1={sig}");
        assert!(verify_signature(secret, &header, body, 300).is_ok());
        assert!(verify_signature(secret, &format!("t={ts},v1=deadbeef"), body, 300).is_err());
        assert!(verify_signature(secret, "v1=only", body, 300).is_err());
    }

    #[test]
    fn refuses_live_key_without_approval() {
        // Isolated: do not rely on process env left by other tests.
        let mut c = Config::from_env();
        c.secret_key = "sk_live_example".into();
        // refuse_live_key reads env for approval flag
        unsafe {
            std::env::remove_var("NXT_STRIPE_LIVE_APPROVED");
        }
        assert!(c.refuse_live_key().is_some());
        unsafe {
            std::env::set_var("NXT_STRIPE_LIVE_APPROVED", "true");
        }
        assert!(c.refuse_live_key().is_none());
        unsafe {
            std::env::remove_var("NXT_STRIPE_LIVE_APPROVED");
        }
        c.secret_key = "sk_test_example".into();
        assert!(c.refuse_live_key().is_none());
    }
}
