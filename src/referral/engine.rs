//! Referral attribution, fraud checks, reward issuance and the loyalty ledger.

use super::models::*;
use crate::store::{Store, StoreResult};
use super::webhook;
use serde::Deserialize;
use serde_json::json;

const LIST_CAP: usize = 5000;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", &uuid::Uuid::new_v4().simple().to_string()[..16])
}

/// Normalizes an email so `Jane.Doe+promo@gmail.com` and `janedoe@gmail.com`
/// count as the same person for self-referral checks.
pub fn normalize_email(email: &str) -> String {
    let email = email.trim().to_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return email;
    };
    let local = local.split('+').next().unwrap_or(local);
    let domain = if domain == "googlemail.com" { "gmail.com" } else { domain };
    let local = if domain == "gmail.com" { local.replace('.', "") } else { local.to_string() };
    format!("{local}@{domain}")
}

pub fn valid_email(email: &str) -> bool {
    let e = email.trim();
    match e.split_once('@') {
        Some((l, d)) => !l.is_empty() && d.contains('.') && !d.starts_with('.') && !e.contains(' '),
        None => false,
    }
}

fn make_code(name: &str) -> String {
    let stem: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_uppercase();
    let stem = if stem.is_empty() { "FRIEND".to_string() } else { stem };
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..4].to_uppercase();
    format!("{stem}{suffix}")
}

fn coupon_code() -> String {
    let raw = uuid::Uuid::new_v4().simple().to_string().to_uppercase();
    format!("RS-{}-{}", &raw[..4], &raw[4..8])
}

pub struct Engine {
    pub store: Store,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseInput {
    pub order_id: String,
    pub email: String,
    pub amount: f64,
    #[serde(default)]
    pub referral_code: Option<String>,
    /// A friend coupon used at checkout also attributes the referral.
    #[serde(default)]
    pub coupon_code: Option<String>,
}

#[derive(Debug)]
pub enum EngineError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Storage(String),
}

impl From<String> for EngineError {
    fn from(e: String) -> Self {
        EngineError::Storage(e)
    }
}

pub type EResult<T> = Result<T, EngineError>;

impl Engine {
    pub fn new(store: Store) -> Self {
        Engine { store }
    }

    pub async fn settings(&self) -> StoreResult<Settings> {
        Ok(self.store.get_json("settings").await?.unwrap_or_default())
    }

    pub async fn save_settings(&self, s: &Settings) -> StoreResult<()> {
        self.store.set_json("settings", s).await
    }

    async fn emit(&self, settings: &Settings, kind: &str, data: serde_json::Value) {
        webhook::send(settings, kind, data).await;
    }

    async fn log_event(&self, kind: &str, code: Option<&str>, detail: Option<String>) -> StoreResult<()> {
        let ev = Event {
            kind: kind.into(),
            referral_code: code.map(str::to_string),
            detail,
            created_at: now(),
        };
        self.store.lpush_capped("events", &serde_json::to_string(&ev).unwrap(), LIST_CAP).await
    }

    // ---- advocates -------------------------------------------------------

    pub async fn advocate_by_code(&self, code: &str) -> StoreResult<Option<Advocate>> {
        let code = code.trim().to_uppercase();
        match self.store.get(&format!("code:{code}")).await? {
            Some(email) => self.store.get_json(&format!("adv:{email}")).await,
            None => Ok(None),
        }
    }

    async fn save_advocate(&self, a: &Advocate) -> StoreResult<()> {
        self.store.set_json(&format!("adv:{}", normalize_email(&a.email)), a).await
    }

    /// Returns the advocate for this email, enrolling them if new.
    pub async fn enroll_advocate(&self, email: &str, name: &str) -> EResult<(Advocate, bool)> {
        if !valid_email(email) {
            return Err(EngineError::BadRequest("valid email required".into()));
        }
        let key = normalize_email(email);
        if let Some(a) = self.store.get_json::<Advocate>(&format!("adv:{key}")).await? {
            return Ok((a, false));
        }
        let name = name.trim();
        let mut code = make_code(if name.is_empty() { email } else { name });
        while !self.store.set_nx(&format!("code:{code}"), &key).await? {
            code = make_code(name);
        }
        let adv = Advocate {
            email: email.trim().to_lowercase(),
            name: name.to_string(),
            code,
            created_at: now(),
            clicks: 0,
            shares: 0,
            referrals: 0,
            rewards_earned: 0,
            referred_revenue: 0.0,
        };
        self.save_advocate(&adv).await?;
        self.store.sadd("advocates", &key).await?;
        self.store.incr_by("stat:advocates", 1).await?;
        let settings = self.settings().await?;
        self.emit(&settings, "advocate.enrolled", json!(adv)).await;
        Ok((adv, true))
    }

    pub async fn list_advocates(&self) -> StoreResult<Vec<Advocate>> {
        let mut out = Vec::new();
        for key in self.store.smembers("advocates").await? {
            if let Some(a) = self.store.get_json::<Advocate>(&format!("adv:{key}")).await? {
                out.push(a);
            }
        }
        out.sort_by(|a, b| b.referrals.cmp(&a.referrals).then(b.created_at.cmp(&a.created_at)));
        Ok(out)
    }

    pub async fn record_click(&self, code: &str, detail: Option<String>) -> StoreResult<Option<Advocate>> {
        let Some(mut adv) = self.advocate_by_code(code).await? else {
            return Ok(None);
        };
        adv.clicks += 1;
        self.save_advocate(&adv).await?;
        self.store.incr_by("stat:clicks", 1).await?;
        self.log_event("click", Some(&adv.code), detail).await?;
        Ok(Some(adv))
    }

    pub async fn record_share(&self, code: &str, channel: &str) -> EResult<()> {
        let Some(mut adv) = self.advocate_by_code(code).await? else {
            return Err(EngineError::NotFound("unknown referral code".into()));
        };
        adv.shares += 1;
        self.save_advocate(&adv).await?;
        self.store.incr_by("stat:shares", 1).await?;
        self.store.incr_by(&format!("stat:share:{channel}"), 1).await?;
        self.log_event("share", Some(&adv.code), Some(channel.to_string())).await?;
        Ok(())
    }

    // ---- customers & loyalty ---------------------------------------------

    pub async fn customer(&self, email: &str) -> StoreResult<Customer> {
        let key = normalize_email(email);
        Ok(self
            .store
            .get_json(&format!("cust:{key}"))
            .await?
            .unwrap_or_else(|| Customer { email: email.trim().to_lowercase(), ..Default::default() }))
    }

    async fn save_customer(&self, c: &Customer) -> StoreResult<()> {
        let key = normalize_email(&c.email);
        self.store.sadd("customers", &key).await?;
        self.store.set_json(&format!("cust:{key}"), c).await
    }

    async fn add_points(&self, cust: &mut Customer, delta: i64, reason: &str) -> StoreResult<()> {
        if delta == 0 {
            return Ok(());
        }
        cust.points += delta;
        let entry = LedgerEntry { delta, balance: cust.points, reason: reason.into(), created_at: now() };
        self.store
            .lpush_capped(
                &format!("ledger:{}", normalize_email(&cust.email)),
                &serde_json::to_string(&entry).unwrap(),
                LIST_CAP,
            )
            .await?;
        self.store.incr_by(if delta > 0 { "stat:points_issued" } else { "stat:points_redeemed" }, delta.abs()).await?;
        Ok(())
    }

    pub async fn ledger(&self, email: &str) -> StoreResult<Vec<LedgerEntry>> {
        self.store.list_json(&format!("ledger:{}", normalize_email(email)), 200).await
    }

    pub async fn signup(&self, email: &str) -> EResult<Customer> {
        if !valid_email(email) {
            return Err(EngineError::BadRequest("valid email required".into()));
        }
        let settings = self.settings().await?;
        let mut cust = self.customer(email).await?;
        if !cust.signed_up {
            cust.signed_up = true;
            if settings.loyalty_enabled {
                self.add_points(&mut cust, settings.signup_points, "signup bonus").await?;
            }
            self.save_customer(&cust).await?;
            self.store.incr_by("stat:signups", 1).await?;
        }
        Ok(cust)
    }

    pub async fn redeem_points(&self, email: &str, points: i64) -> EResult<RewardGrant> {
        let settings = self.settings().await?;
        if !settings.loyalty_enabled {
            return Err(EngineError::BadRequest("loyalty program is disabled".into()));
        }
        let unit = settings.points_per_redemption_unit.max(1);
        if points <= 0 || points % unit != 0 {
            return Err(EngineError::BadRequest(format!("points must be a positive multiple of {unit}")));
        }
        let mut cust = self.customer(email).await?;
        if cust.points < points {
            return Err(EngineError::Conflict(format!("insufficient points: balance is {}", cust.points)));
        }
        self.add_points(&mut cust, -points, "redeemed for coupon").await?;
        self.save_customer(&cust).await?;
        let reward = Reward { kind: RewardKind::CouponFixed, amount: (points / unit) as f64 };
        Ok(self.grant(&settings, &cust.email, RewardRole::Loyalty, reward, None, None).await?)
    }

    // ---- rewards -----------------------------------------------------------

    async fn grant(
        &self,
        settings: &Settings,
        email: &str,
        role: RewardRole,
        reward: Reward,
        referral_code: Option<&str>,
        conversion_id: Option<&str>,
    ) -> StoreResult<RewardGrant> {
        let coupon = matches!(reward.kind, RewardKind::CouponFixed | RewardKind::CouponPercent).then(coupon_code);
        let grant = RewardGrant {
            id: new_id("rwd"),
            email: email.to_string(),
            role,
            reward: reward.clone(),
            coupon_code: coupon.clone(),
            referral_code: referral_code.map(str::to_string),
            conversion_id: conversion_id.map(str::to_string),
            created_at: now(),
        };
        if reward.kind == RewardKind::Points {
            let mut cust = self.customer(email).await?;
            self.add_points(&mut cust, reward.amount as i64, &format!("{role:?} reward").to_lowercase()).await?;
            self.save_customer(&cust).await?;
        }
        if let (Some(c), Some(code)) = (&coupon, referral_code) {
            if role == RewardRole::Friend {
                self.store.set(&format!("coupon:{c}"), code).await?;
            }
        }
        self.store.lpush_capped("rewards", &serde_json::to_string(&grant).unwrap(), LIST_CAP).await?;
        self.store.incr_by("stat:rewards", 1).await?;
        self.emit(settings, "reward.issued", json!(grant)).await;
        Ok(grant)
    }

    pub async fn list_rewards(&self, limit: usize) -> StoreResult<Vec<RewardGrant>> {
        self.store.list_json("rewards", limit).await
    }

    /// A referred friend claims their incentive (once per email) on the landing page.
    pub async fn claim_friend_incentive(&self, code: &str, email: &str) -> EResult<RewardGrant> {
        if !valid_email(email) {
            return Err(EngineError::BadRequest("valid email required".into()));
        }
        let Some(adv) = self.advocate_by_code(code).await? else {
            return Err(EngineError::NotFound("unknown referral code".into()));
        };
        let friend = normalize_email(email);
        if friend == normalize_email(&adv.email) {
            return Err(EngineError::BadRequest("you can't use your own referral link".into()));
        }
        let cust = self.customer(email).await?;
        if cust.orders > 0 {
            return Err(EngineError::Conflict("this offer is for new customers only".into()));
        }
        if let Some(existing) = self.store.get(&format!("claim:{friend}")).await? {
            let grant: RewardGrant = serde_json::from_str(&existing).map_err(|e| e.to_string())?;
            return Ok(grant);
        }
        let settings = self.settings().await?;
        let grant = self
            .grant(&settings, email.trim(), RewardRole::Friend, settings.friend_incentive.clone(), Some(&adv.code), None)
            .await?;
        self.store.set(&format!("claim:{friend}"), &serde_json::to_string(&grant).unwrap()).await?;
        let mut cust = cust;
        cust.referred_by.get_or_insert(adv.code.clone());
        self.save_customer(&cust).await?;
        self.log_event("claim", Some(&adv.code), Some(email.trim().to_string())).await?;
        Ok(grant)
    }

    // ---- conversions -------------------------------------------------------

    pub async fn track_purchase(&self, input: PurchaseInput) -> EResult<Conversion> {
        if input.order_id.trim().is_empty() {
            return Err(EngineError::BadRequest("order_id required".into()));
        }
        if !valid_email(&input.email) {
            return Err(EngineError::BadRequest("valid email required".into()));
        }
        if !input.amount.is_finite() || input.amount < 0.0 {
            return Err(EngineError::BadRequest("amount must be a non-negative number".into()));
        }
        let conv_id = new_id("conv");
        if !self.store.set_nx(&format!("order:{}", input.order_id.trim()), &conv_id).await? {
            return Err(EngineError::Conflict("order already tracked".into()));
        }
        let settings = self.settings().await?;
        let mut cust = self.customer(&input.email).await?;
        let first_order = cust.orders == 0;

        // Attribution: explicit code, then a friend coupon, then the code the
        // customer was tagged with when they claimed their incentive.
        let mut code = input.referral_code.clone().filter(|c| !c.trim().is_empty());
        if code.is_none() {
            if let Some(c) = &input.coupon_code {
                code = self.store.get(&format!("coupon:{}", c.trim().to_uppercase())).await?;
            }
        }
        if code.is_none() && first_order {
            code = cust.referred_by.clone();
        }
        let advocate = match &code {
            Some(c) => self.advocate_by_code(c).await?,
            None => None,
        };

        let (status, reason) = match &advocate {
            None if code.is_some() => (ConversionStatus::Rejected, Some("unknown referral code".to_string())),
            None => (ConversionStatus::Organic, None),
            Some(a) if normalize_email(&a.email) == normalize_email(&input.email) => {
                (ConversionStatus::Rejected, Some("self-referral".to_string()))
            }
            Some(_) if !first_order => (ConversionStatus::Rejected, Some("not a new customer".to_string())),
            Some(_) if input.amount < settings.min_order_amount => (
                ConversionStatus::Rejected,
                Some(format!("order below minimum of {}", settings.min_order_amount)),
            ),
            Some(a) if settings.max_rewards_per_advocate > 0 && a.rewards_earned >= settings.max_rewards_per_advocate => {
                (ConversionStatus::Rejected, Some("advocate reward cap reached".to_string()))
            }
            Some(_) => (ConversionStatus::Approved, None),
        };

        // Loyalty points accrue on every purchase regardless of referral outcome.
        let points = if settings.loyalty_enabled {
            (input.amount * settings.points_per_currency_unit).floor() as i64
        } else {
            0
        };
        cust.orders += 1;
        cust.total_spent += input.amount;
        if status == ConversionStatus::Approved {
            cust.referred_by = advocate.as_ref().map(|a| a.code.clone());
        }
        self.add_points(&mut cust, points, &format!("order {}", input.order_id.trim())).await?;
        self.save_customer(&cust).await?;

        let conv = Conversion {
            id: conv_id,
            order_id: input.order_id.trim().to_string(),
            email: input.email.trim().to_lowercase(),
            amount: input.amount,
            referral_code: advocate.as_ref().map(|a| a.code.clone()).or(code),
            advocate_email: advocate.as_ref().map(|a| a.email.clone()),
            status: status.clone(),
            reason,
            points_awarded: points,
            created_at: now(),
        };
        self.store.lpush_capped("conversions", &serde_json::to_string(&conv).unwrap(), LIST_CAP).await?;
        self.store.incr_by("stat:orders", 1).await?;
        self.store.incr_by("stat:revenue_cents", (input.amount * 100.0).round() as i64).await?;

        if let (ConversionStatus::Approved, Some(mut adv)) = (&status, advocate) {
            adv.referrals += 1;
            adv.rewards_earned += 1;
            adv.referred_revenue += input.amount;
            self.save_advocate(&adv).await?;
            self.store.incr_by("stat:referrals", 1).await?;
            self.store.incr_by("stat:referred_revenue_cents", (input.amount * 100.0).round() as i64).await?;
            self.grant(&settings, &adv.email, RewardRole::Advocate, settings.advocate_reward.clone(), Some(&adv.code), Some(&conv.id))
                .await?;
        } else if status == ConversionStatus::Rejected {
            self.store.incr_by("stat:rejected", 1).await?;
        }
        self.emit(&settings, "conversion.tracked", json!(conv)).await;
        Ok(conv)
    }

    pub async fn list_conversions(&self, limit: usize) -> StoreResult<Vec<Conversion>> {
        self.store.list_json("conversions", limit).await
    }

    pub async fn analytics(&self) -> StoreResult<serde_json::Value> {
        let mut stats = serde_json::Map::new();
        for k in [
            "advocates", "clicks", "shares", "signups", "orders", "referrals", "rejected", "rewards",
            "points_issued", "points_redeemed",
        ] {
            let v: i64 = self.store.get(&format!("stat:{k}")).await?.and_then(|v| v.parse().ok()).unwrap_or(0);
            stats.insert(k.into(), json!(v));
        }
        for k in ["revenue", "referred_revenue"] {
            let cents: i64 =
                self.store.get(&format!("stat:{k}_cents")).await?.and_then(|v| v.parse().ok()).unwrap_or(0);
            stats.insert(k.into(), json!(cents as f64 / 100.0));
        }
        let mut channels = serde_json::Map::new();
        for ch in ["copy", "email", "sms", "facebook", "x", "whatsapp", "linkedin"] {
            let v: i64 = self.store.get(&format!("stat:share:{ch}")).await?.and_then(|v| v.parse().ok()).unwrap_or(0);
            channels.insert(ch.into(), json!(v));
        }
        let get = |k: &str| stats.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let click_to_referral = if get("clicks") > 0.0 { get("referrals") / get("clicks") } else { 0.0 };
        let referred_share = if get("revenue") > 0.0 { get("referred_revenue") / get("revenue") } else { 0.0 };
        Ok(json!({
            "totals": stats,
            "share_channels": channels,
            "rates": { "click_to_referral": click_to_referral, "referred_revenue_share": referred_share },
            "storage": self.store.backend_name(),
        }))
    }
}
