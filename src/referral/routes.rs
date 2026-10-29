//! Referral Suite: a referral + loyalty platform (advocate/friend rewards,
//! click & conversion attribution, fraud checks, points ledger, signed
//! webhooks) served as one Rust function on Vercel.


use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use super::engine::{EResult, Engine, EngineError, PurchaseInput};
use super::webhook;
use super::models::Settings;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub type AppState = Arc<Engine>;

pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<EngineError> for ApiError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::BadRequest(m) => ApiError(StatusCode::BAD_REQUEST, m),
            EngineError::NotFound(m) => ApiError(StatusCode::NOT_FOUND, m),
            EngineError::Conflict(m) => ApiError(StatusCode::CONFLICT, m),
            EngineError::Storage(m) => ApiError(StatusCode::INTERNAL_SERVER_ERROR, m),
        }
    }
}

impl From<String> for ApiError {
    fn from(e: String) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e)
    }
}

type ApiResult = Result<Json<Value>, ApiError>;

fn ok<T: serde::Serialize>(v: T) -> ApiResult {
    Ok(Json(json!(v)))
}

fn lift<T>(r: EResult<T>) -> Result<T, ApiError> {
    r.map_err(ApiError::from)
}

pub fn app(state: AppState) -> Router {
    let admin = Router::new()
        .route("/api/admin/settings", get(get_settings).put(put_settings))
        .route("/api/admin/advocates", get(list_advocates).post(admin_enroll))
        .route("/api/admin/conversions", get(list_conversions))
        .route("/api/admin/rewards", get(list_rewards))
        .route("/api/admin/analytics", get(analytics))
        .route("/api/admin/customers/{email}", get(get_customer))
        .route("/api/admin/webhook/test", post(test_webhook))
        .route("/api/track/purchase", post(track_purchase))
        .route("/api/track/signup", post(track_signup))
        .route("/api/loyalty/redeem", post(redeem))
        .layer(middleware::from_fn(require_admin));
    let public = Router::new()
        .route("/api/public/offer", get(public_offer))
        .route("/api/public/advocate", post(public_enroll))
        .route("/api/public/share", post(public_share))
        .route("/api/public/claim", post(public_claim))
        .route("/api/public/referrer/{code}", get(public_referrer))
        .layer(middleware::from_fn(cors));
    Router::new()
        .route("/api/health", get(health))
        .route("/r/{code}", get(referral_redirect))
        .merge(admin)
        .merge(public)
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "no such route".into()) })
        .with_state(state)
}

// ---- middleware ------------------------------------------------------------

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Protected workspace APIs always require sign-in. A missing server key is
/// configuration failure, never an instruction to expose the workspace.
pub fn auth_required() -> bool {
    true
}

/// Shared protected-route guard: staff sessions or the break-glass admin key.
/// See `firm::staff` for roles, MFA, label stamping and policy.
pub async fn require_admin(req: Request, next: Next) -> Response {
    crate::firm::staff::authorize(req, next).await
}


/// Public endpoints are called by the widget from merchant storefronts.
async fn cors(req: Request, next: Next) -> Response {
    let mut resp = if req.method() == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(req).await
    };
    let h = resp.headers_mut();
    h.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    h.insert("access-control-allow-methods", HeaderValue::from_static("GET, POST, OPTIONS"));
    h.insert("access-control-allow-headers", HeaderValue::from_static("content-type"));
    resp
}

// ---- public ---------------------------------------------------------------

async fn health(State(e): State<AppState>) -> ApiResult {
    ok(json!({ "ok": true, "storage": e.store.backend_name() }))
}

fn public_offer_json(s: &Settings) -> Value {
    let advocate = s.advocate_reward.label(&s.currency_symbol);
    let friend = s.friend_incentive.label(&s.currency_symbol);
    json!({
        "brand_name": s.brand_name,
        "advocate_reward": s.advocate_reward,
        "friend_incentive": s.friend_incentive,
        "headline": format!("Give {friend}, Get {advocate}"),
        "advocate_copy": format!("Give your friends {friend} off their first order. You'll get {advocate} when they buy."),
        "friend_copy": format!("You've been gifted {friend} off your first {} order.", s.brand_name),
        "loyalty_enabled": s.loyalty_enabled,
    })
}

async fn public_offer(State(e): State<AppState>) -> ApiResult {
    ok(public_offer_json(&e.settings().await?))
}

#[derive(Deserialize)]
struct EnrollBody {
    email: String,
    #[serde(default)]
    name: String,
    /// HMAC-SHA256(WIDGET_SIGNING_SECRET, lowercase email) as hex. Required
    /// only when WIDGET_SIGNING_SECRET is set, which stops anyone from
    /// fetching another customer's referral link by guessing their email.
    #[serde(default)]
    signature: Option<String>,
}

fn share_url(headers: &HeaderMap, code: &str) -> String {
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:3000");
    let scheme = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).unwrap_or("https");
    format!("{scheme}://{host}/r/{code}")
}

async fn public_enroll(State(e): State<AppState>, headers: HeaderMap, Json(b): Json<EnrollBody>) -> ApiResult {
    if let Ok(secret) = std::env::var("WIDGET_SIGNING_SECRET") {
        if !secret.is_empty() {
            let expected = webhook::sign(&secret, b.email.trim().to_lowercase().as_bytes());
            let expected = expected.trim_start_matches("sha256=");
            let given = b.signature.as_deref().unwrap_or("");
            if !constant_time_eq(given.as_bytes(), expected.as_bytes()) {
                return Err(ApiError(StatusCode::FORBIDDEN, "invalid customer signature".into()));
            }
        }
    }
    let (adv, created) = lift(e.enroll_advocate(&b.email, &b.name).await)?;
    let s = e.settings().await?;
    ok(json!({
        "code": adv.code,
        "share_url": share_url(&headers, &adv.code),
        "created": created,
        "stats": { "clicks": adv.clicks, "referrals": adv.referrals, "rewards_earned": adv.rewards_earned },
        "offer": public_offer_json(&s),
    }))
}

#[derive(Deserialize)]
struct ShareBody {
    code: String,
    channel: String,
}

async fn public_share(State(e): State<AppState>, Json(b): Json<ShareBody>) -> ApiResult {
    const CHANNELS: [&str; 7] = ["copy", "email", "sms", "facebook", "x", "whatsapp", "linkedin"];
    let channel = b.channel.to_lowercase();
    if !CHANNELS.contains(&channel.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, format!("channel must be one of {CHANNELS:?}")));
    }
    lift(e.record_share(&b.code, &channel).await)?;
    ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ClaimBody {
    code: String,
    email: String,
}

async fn public_claim(State(e): State<AppState>, Json(b): Json<ClaimBody>) -> ApiResult {
    let g = lift(e.claim_friend_incentive(&b.code, &b.email).await)?;
    ok(json!({ "coupon_code": g.coupon_code, "reward": g.reward }))
}

async fn public_referrer(State(e): State<AppState>, Path(code): Path<String>) -> ApiResult {
    let Some(adv) = e.advocate_by_code(&code).await? else {
        return Err(ApiError(StatusCode::NOT_FOUND, "unknown referral code".into()));
    };
    let first_name = adv.name.split_whitespace().next().unwrap_or("").to_string();
    let s = e.settings().await?;
    ok(json!({ "code": adv.code, "referrer_first_name": first_name, "offer": public_offer_json(&s) }))
}

async fn referral_redirect(State(e): State<AppState>, Path(code): Path<String>, headers: HeaderMap) -> Response {
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(|s| s.chars().take(200).collect());
    let settings = e.settings().await.unwrap_or_default();
    let adv = e.record_click(&code, ua).await.ok().flatten();
    let target = &settings.landing_url;
    let sep = if target.contains('?') { '&' } else { '?' };
    let location = match &adv {
        Some(a) => format!("{target}{sep}ref={}", a.code),
        None => target.clone(),
    };
    let mut resp = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, location)
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(a) = adv {
        resp = resp.header(
            header::SET_COOKIE,
            format!("rs_ref={}; Path=/; Max-Age=2592000; SameSite=Lax; Secure", a.code),
        );
    }
    resp.body(Body::empty()).unwrap()
}

// ---- admin ------------------------------------------------------------------

async fn get_settings(State(e): State<AppState>) -> ApiResult {
    ok(e.settings().await?)
}

async fn put_settings(State(e): State<AppState>, Json(s): Json<Settings>) -> ApiResult {
    if s.advocate_reward.amount < 0.0 || s.friend_incentive.amount < 0.0 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "reward amounts must be non-negative".into()));
    }
    e.save_settings(&s).await?;
    ok(s)
}

#[derive(Deserialize)]
struct Limit {
    limit: Option<usize>,
}

async fn list_advocates(State(e): State<AppState>) -> ApiResult {
    ok(e.list_advocates().await?)
}

async fn admin_enroll(State(e): State<AppState>, headers: HeaderMap, Json(b): Json<EnrollBody>) -> ApiResult {
    let (adv, created) = lift(e.enroll_advocate(&b.email, &b.name).await)?;
    let url = share_url(&headers, &adv.code);
    ok(json!({ "advocate": adv, "share_url": url, "created": created }))
}

async fn list_conversions(State(e): State<AppState>, Query(q): Query<Limit>) -> ApiResult {
    ok(e.list_conversions(q.limit.unwrap_or(200).min(5000)).await?)
}

async fn list_rewards(State(e): State<AppState>, Query(q): Query<Limit>) -> ApiResult {
    ok(e.list_rewards(q.limit.unwrap_or(200).min(5000)).await?)
}

async fn analytics(State(e): State<AppState>) -> ApiResult {
    ok(e.analytics().await?)
}

async fn get_customer(State(e): State<AppState>, Path(email): Path<String>) -> ApiResult {
    let cust = e.customer(&email).await?;
    let ledger = e.ledger(&email).await?;
    ok(json!({ "customer": cust, "ledger": ledger }))
}

async fn test_webhook(State(e): State<AppState>) -> ApiResult {
    let s = e.settings().await?;
    if s.webhook_url.is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "set webhook_url in settings first".into()));
    }
    let status = webhook::send(&s, "webhook.test", json!({ "hello": "world" })).await;
    ok(json!({ "delivered": status.is_some(), "status": status }))
}

async fn track_purchase(State(e): State<AppState>, Json(p): Json<PurchaseInput>) -> ApiResult {
    ok(lift(e.track_purchase(p).await)?)
}

#[derive(Deserialize)]
struct EmailBody {
    email: String,
}

async fn track_signup(State(e): State<AppState>, Json(b): Json<EmailBody>) -> ApiResult {
    ok(lift(e.signup(&b.email).await)?)
}

#[derive(Deserialize)]
struct RedeemBody {
    email: String,
    points: i64,
}

async fn redeem(State(e): State<AppState>, Json(b): Json<RedeemBody>) -> ApiResult {
    ok(lift(e.redeem_points(&b.email, b.points).await)?)
}

