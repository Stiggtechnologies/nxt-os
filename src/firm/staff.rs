//! Firm staff identity: individual accounts, roles, TOTP MFA, per-client
//! scope, sessions, and the shared-key break-glass admin kept for migration.
//!
//! Every protected NXT route passes `authorize` (through
//! `referral::routes::require_admin`). A staff session puts an [`Identity`]
//! into request extensions, replaces responsibility labels in JSON bodies
//! with the verified account label, and checks the role policy.
use crate::{referral::routes::ApiError, store::Store};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, Request},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};

type SResult<T> = Result<T, ApiError>;
const SESSION_HOURS: i64 = 12;
const ENROLL_MINUTES: i64 = 15;
const LOCK_AFTER: u8 = 5;
const LOCK_MINUTES: i64 = 15;
const MIN_PASSWORD: usize = 12;
pub const LABEL_KEYS: [&str; 9] = ["operator_label", "operator", "reviewer", "reviewer_label", "preparer", "prepared_by", "approver", "actor_name", "decided_by"];

fn now() -> String { Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true) }
fn bad(m: impl Into<String>) -> ApiError { ApiError(StatusCode::BAD_REQUEST, m.into()) }
fn conflict(m: impl Into<String>) -> ApiError { ApiError(StatusCode::CONFLICT, m.into()) }
fn denied(m: impl Into<String>) -> ApiError { ApiError(StatusCode::FORBIDDEN, m.into()) }
fn unauth() -> ApiError { ApiError(StatusCode::UNAUTHORIZED, "invalid or missing API key".into()) }
fn st(e: impl std::fmt::Display) -> ApiError { ApiError(StatusCode::SERVICE_UNAVAILABLE, format!("staff storage unavailable: {e}")) }
fn sha_hex(b: &[u8]) -> String { hex::encode(Sha256::digest(b)) }
fn rand_hex(n: usize) -> String { (0..n.div_ceil(32)).map(|_| uuid::Uuid::new_v4().simple().to_string()).collect::<String>()[..n].to_string() }
fn rand_bytes(n: usize) -> Vec<u8> { (0..n.div_ceil(16)).flat_map(|_| *uuid::Uuid::new_v4().as_bytes()).take(n).collect() }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role { Admin, Preparer, Reviewer, FrontDesk, ReadOnly }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaffUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    /// Client refs this account may open. `*` means every client.
    pub clients: Vec<String>,
    pub disabled: bool,
    pub salt: String,
    pub password_hash: String,
    pub iterations: u32,
    pub must_change_password: bool,
    pub mfa_enabled: bool,
    pub totp_secret: Option<String>,
    pub pending_totp_secret: Option<String>,
    pub last_totp_step: u64,
    pub failed_logins: u8,
    pub locked_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl StaffUser {
    pub fn public(&self) -> Value {
        json!({"id":self.id,"username":self.username,"display_name":self.display_name,"role":self.role,"clients":self.clients,
            "disabled":self.disabled,"mfa_enabled":self.mfa_enabled,"must_change_password":self.must_change_password,"created_at":self.created_at,"updated_at":self.updated_at})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Session { user_id: String, mfa: bool, expires_at: String, created_at: String }

/// The verified actor for one request.
#[derive(Clone, Debug, Serialize)]
pub struct Identity {
    pub user_id: String,
    pub display_name: String,
    pub role: Role,
    pub clients: Vec<String>,
    pub break_glass: bool,
    pub mfa: bool,
}

impl Identity {
    pub fn break_glass() -> Self {
        Self { user_id: "break-glass".into(), display_name: "Break-glass admin".into(), role: Role::Admin, clients: vec!["*".into()], break_glass: true, mfa: false }
    }
    /// The label written into sign-offs, envelopes and audit logs.
    pub fn label(&self) -> String {
        if self.break_glass { "Break-glass admin (shared key)".into() } else { format!("{} [{}]", self.display_name, self.user_id) }
    }
    /// A verified staff user id, or None for the shared break-glass key.
    pub fn staff_id(&self) -> Option<&str> { (!self.break_glass).then_some(self.user_id.as_str()) }
    pub fn can_client(&self, client_ref: &str) -> bool {
        self.clients.iter().any(|c| c == "*" || (!client_ref.trim().is_empty() && c.eq_ignore_ascii_case(client_ref.trim())))
    }
    pub fn is_reviewer(&self) -> bool { matches!(self.role, Role::Admin | Role::Reviewer) }
}

/// Segregation of duties on verified labels: a staff-signed preparation can only be
/// reviewed by a different staff account (never by the shared break-glass key).
pub fn same_person(preparer: &str, reviewer: &str) -> Option<&'static str> {
    match (label_user_id(preparer), label_user_id(reviewer)) {
        (Some(a), Some(b)) if a == b => Some("reviewer must be a different staff account from the preparer"),
        // Staff sessions always carry a stamped id; an id-less label means the break-glass key.
        (Some(_), None) => Some("the break-glass key cannot review work prepared by a staff account"),
        _ => None,
    }
}
/// Extracts the staff user id that a verified label carries, e.g. `Pat [usr_1]`.
pub fn label_user_id(label: &str) -> Option<&str> {
    let l = label.trim();
    let start = l.rfind(" [usr_")? + 2;
    l.ends_with(']').then(|| &l[start..l.len() - 1])
}

// ---------- crypto ----------

fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let prf = |data: &[&[u8]]| { let mut m = Hmac::<Sha256>::new_from_slice(password).expect("hmac key"); for d in data { m.update(d); } m.finalize().into_bytes() };
    let mut u = prf(&[salt, &1u32.to_be_bytes()]);
    let mut out = [0u8; 32];
    out.copy_from_slice(&u);
    for _ in 1..iterations { u = prf(&[&u]); for (o, x) in out.iter_mut().zip(u.iter()) { *o ^= x; } }
    out
}
fn iterations() -> u32 {
    std::env::var("NXT_STAFF_PBKDF2_ITERATIONS").ok().and_then(|v| v.parse().ok()).filter(|n| *n >= 1000).unwrap_or(210_000)
}
fn ct_eq(a: &[u8], b: &[u8]) -> bool { crate::referral::routes::constant_time_eq(a, b) }

const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
pub fn base32(bytes: &[u8]) -> String {
    let (mut out, mut buf, mut bits) = (String::new(), 0u32, 0u32);
    for b in bytes { buf = (buf << 8) | *b as u32; bits += 8; while bits >= 5 { bits -= 5; out.push(B32[((buf >> bits) & 31) as usize] as char); } }
    if bits > 0 { out.push(B32[((buf << (5 - bits)) & 31) as usize] as char); }
    out
}
pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut buf, mut bits) = (vec![], 0u32, 0u32);
    for c in s.trim().trim_end_matches('=').bytes().filter(|c| *c != b' ') {
        let v = B32.iter().position(|x| *x == c.to_ascii_uppercase())? as u32;
        buf = (buf << 5) | v; bits += 5;
        if bits >= 8 { bits -= 8; out.push((buf >> bits) as u8); }
    }
    Some(out)
}
/// RFC 6238 TOTP (SHA-1, 30 s, 6 digits).
pub fn totp_at(secret: &[u8], step: u64) -> String {
    let mut m = Hmac::<Sha1>::new_from_slice(secret).expect("hmac key");
    m.update(&step.to_be_bytes());
    let h = m.finalize().into_bytes();
    let o = (h[19] & 0x0f) as usize;
    let bin = ((h[o] as u32 & 0x7f) << 24) | ((h[o + 1] as u32) << 16) | ((h[o + 2] as u32) << 8) | h[o + 3] as u32;
    format!("{:06}", bin % 1_000_000)
}
pub fn totp_step(unix: i64) -> u64 { (unix / 30) as u64 }
/// Current code for a base32 secret (used by tests and by enrollment checks).
pub fn totp_now(secret_b32: &str) -> Option<String> { base32_decode(secret_b32).map(|s| totp_at(&s, totp_step(Utc::now().timestamp()))) }
fn totp_match(secret_b32: &str, code: &str, last_step: u64) -> Option<u64> {
    let secret = base32_decode(secret_b32)?;
    let code = code.trim();
    if code.len() != 6 { return None; }
    let now = totp_step(Utc::now().timestamp());
    [now.saturating_sub(1), now, now + 1].into_iter().find(|s| *s > last_step && ct_eq(totp_at(&secret, *s).as_bytes(), code.as_bytes()))
}

// ---------- registry ----------

pub struct Staff { pub store: Store }
static REGISTRY: RwLock<Option<Arc<Staff>>> = RwLock::new(None);
/// Replaces the process-wide staff registry (the app installs one at start-up; tests install memory stores).
pub fn install(staff: Arc<Staff>) { *REGISTRY.write().unwrap_or_else(|e| e.into_inner()) = Some(staff); }
pub fn registry() -> Arc<Staff> {
    if let Some(s) = REGISTRY.read().unwrap_or_else(|e| e.into_inner()).clone() { return s; }
    let mut w = REGISTRY.write().unwrap_or_else(|e| e.into_inner());
    w.get_or_insert_with(|| Arc::new(Staff::new(Store::from_env()))).clone()
}

fn user_key(id: &str) -> String { format!("firm:staff:user:{id}") }
fn name_key(u: &str) -> String { format!("firm:staff:username:{}", u.trim().to_ascii_lowercase()) }
fn session_key(h: &str) -> String { format!("firm:staff:session:{h}") }
const USERS: &str = "firm:staff:users";
const AUDIT: &str = "firm:staff:audit";

pub fn break_glass_enabled() -> bool {
    !matches!(std::env::var("NXT_BREAK_GLASS").ok().as_deref().map(str::trim), Some("off") | Some("false") | Some("0"))
}
fn require_mfa() -> bool {
    !matches!(std::env::var("NXT_STAFF_REQUIRE_MFA").ok().as_deref().map(str::trim), Some("off") | Some("false") | Some("0"))
}
fn valid_username(u: &str) -> bool { (3..=64).contains(&u.len()) && u.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-@".contains(&c)) }
fn valid_password(p: &str) -> SResult<()> {
    if p.chars().count() < MIN_PASSWORD || p.len() > 256 { return Err(bad(format!("password must be {MIN_PASSWORD} to 256 characters"))); }
    Ok(())
}
fn clean_clients(c: &[String]) -> SResult<Vec<String>> {
    if c.len() > 500 { return Err(bad("at most 500 client refs per account")); }
    let mut out: Vec<String> = c.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    if out.iter().any(|x| x.len() > 120) { return Err(bad("client refs are at most 120 characters")); }
    out.sort(); out.dedup();
    Ok(out)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUser { pub username: String, pub display_name: String, pub role: Role, #[serde(default)] pub clients: Vec<String>, pub temporary_password: String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateUser { #[serde(default)] pub role: Option<Role>, #[serde(default)] pub clients: Option<Vec<String>>, #[serde(default)] pub disabled: Option<bool>,
    #[serde(default)] pub reset_mfa: bool, #[serde(default)] pub temporary_password: Option<String>, #[serde(default)] pub display_name: Option<String> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginInput { pub username: String, pub password: String, #[serde(default)] pub totp: Option<String> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeInput { pub code: String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordInput { pub current_password: String, pub new_password: String }

impl Staff {
    pub fn new(store: Store) -> Self { Self { store } }

    async fn audit(&self, actor: &str, action: &str, detail: &str) -> SResult<()> {
        let e = json!({"at":now(),"actor":actor,"action":action,"detail":detail});
        self.store.lpush_capped(AUDIT, &e.to_string(), 5000).await.map_err(st)
    }
    pub async fn audit_log(&self, limit: usize) -> SResult<Vec<Value>> {
        Ok(self.store.lrange(AUDIT, limit.min(1000)).await.map_err(st)?.iter().filter_map(|s| serde_json::from_str(s).ok()).collect())
    }
    pub async fn user(&self, id: &str) -> SResult<Option<StaffUser>> { self.store.get_json(&user_key(id)).await.map_err(st) }
    async fn put(&self, u: &StaffUser) -> SResult<()> { self.store.set_json(&user_key(&u.id), u).await.map_err(st) }
    pub async fn users(&self) -> SResult<Vec<StaffUser>> {
        let mut out = vec![];
        for id in self.store.smembers(USERS).await.map_err(st)? { if let Some(u) = self.user(&id).await? { out.push(u); } }
        out.sort_by(|a, b| a.username.cmp(&b.username));
        Ok(out)
    }

    pub async fn create_user(&self, by: &Identity, input: CreateUser) -> SResult<Value> {
        let username = input.username.trim().to_ascii_lowercase();
        if !valid_username(&username) { return Err(bad("username: 3-64 letters, digits, '.', '_', '-' or '@'")); }
        let display = input.display_name.trim();
        if display.is_empty() || display.len() > 120 || display.contains('[') || display.contains(']') { return Err(bad("display name: 1-120 characters, no brackets")); }
        valid_password(&input.temporary_password)?;
        let id = format!("usr_{}", rand_hex(16));
        if !self.store.set_nx(&name_key(&username), &id).await.map_err(st)? { return Err(conflict("that username is taken")); }
        let salt = rand_hex(32);
        let iters = iterations();
        let at = now();
        let u = StaffUser { id: id.clone(), username, display_name: display.into(), role: input.role, clients: clean_clients(&input.clients)?, disabled: false,
            password_hash: hex::encode(pbkdf2(input.temporary_password.as_bytes(), salt.as_bytes(), iters)), salt, iterations: iters, must_change_password: true,
            mfa_enabled: false, totp_secret: None, pending_totp_secret: None, last_totp_step: 0, failed_logins: 0, locked_until: None, created_at: at.clone(), updated_at: at };
        self.put(&u).await?;
        self.store.sadd(USERS, &id).await.map_err(st)?;
        self.audit(&by.label(), "user_created", &format!("{} role {:?}", u.id, u.role)).await?;
        Ok(u.public())
    }

    pub async fn update_user(&self, by: &Identity, id: &str, input: UpdateUser) -> SResult<Value> {
        let mut u = self.user(id).await?.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "staff user not found".into()))?;
        let mut changes = vec![];
        if let Some(r) = input.role { if by.user_id == u.id && r != Role::Admin { return Err(conflict("you cannot remove your own admin role")); } u.role = r; changes.push(format!("role {r:?}")); }
        if let Some(c) = input.clients { u.clients = clean_clients(&c)?; changes.push(format!("{} client refs", u.clients.len())); }
        if let Some(d) = input.disabled { if by.user_id == u.id && d { return Err(conflict("you cannot disable your own account")); } u.disabled = d; changes.push(format!("disabled {d}")); }
        if let Some(n) = input.display_name { let n = n.trim(); if n.is_empty() || n.len() > 120 || n.contains('[') || n.contains(']') { return Err(bad("display name: 1-120 characters, no brackets")); } u.display_name = n.into(); changes.push("display name".into()); }
        if input.reset_mfa { u.mfa_enabled = false; u.totp_secret = None; u.pending_totp_secret = None; changes.push("MFA reset".into()); }
        if let Some(p) = input.temporary_password {
            valid_password(&p)?;
            u.salt = rand_hex(32); u.iterations = iterations(); u.password_hash = hex::encode(pbkdf2(p.as_bytes(), u.salt.as_bytes(), u.iterations));
            u.must_change_password = true; u.failed_logins = 0; u.locked_until = None; changes.push("password reset".into());
        }
        if changes.is_empty() { return Err(bad("no changes")); }
        u.updated_at = now();
        self.put(&u).await?;
        self.audit(&by.label(), "user_updated", &format!("{}: {}", u.id, changes.join(", "))).await?;
        Ok(u.public())
    }

    pub async fn login(&self, input: LoginInput, ip: &str) -> SResult<Value> {
        let fail = || ApiError(StatusCode::UNAUTHORIZED, "username, password or code is wrong".into());
        let id = self.store.get(&name_key(&input.username)).await.map_err(st)?;
        let Some(mut u) = (match id { Some(id) => self.user(&id).await?, None => None }) else {
            let _ = pbkdf2(input.password.as_bytes(), b"timing-equalizer", iterations());
            return Err(fail());
        };
        if let Some(l) = u.locked_until.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()) {
            if l > Utc::now() { return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "account temporarily locked after failed sign-ins; try again later or ask an admin".into())); }
        }
        let pw_ok = ct_eq(hex::encode(pbkdf2(input.password.as_bytes(), u.salt.as_bytes(), u.iterations)).as_bytes(), u.password_hash.as_bytes());
        let totp_ok = if !u.mfa_enabled { Some(u.last_totp_step) } else {
            input.totp.as_deref().and_then(|c| u.totp_secret.as_deref().and_then(|s| totp_match(s, c, u.last_totp_step)))
        };
        if !pw_ok || totp_ok.is_none() || u.disabled {
            u.failed_logins = u.failed_logins.saturating_add(1);
            if u.failed_logins >= LOCK_AFTER { u.locked_until = Some((Utc::now() + chrono::Duration::minutes(LOCK_MINUTES)).to_rfc3339_opts(SecondsFormat::Secs, true)); u.failed_logins = 0; }
            self.put(&u).await?;
            self.audit(&u.id, "login_failed", &format!("from {ip}")).await?;
            return Err(if pw_ok && !u.disabled && u.mfa_enabled && input.totp.is_none() { ApiError(StatusCode::UNAUTHORIZED, "authenticator code required".into()) } else { fail() });
        }
        u.failed_logins = 0; u.locked_until = None; u.last_totp_step = totp_ok.unwrap_or(0);
        self.put(&u).await?;
        let full = u.mfa_enabled || !require_mfa();
        let token = format!("nxs_{}", rand_hex(64));
        let exp = Utc::now() + if full { chrono::Duration::hours(SESSION_HOURS) } else { chrono::Duration::minutes(ENROLL_MINUTES) };
        let s = Session { user_id: u.id.clone(), mfa: full, expires_at: exp.to_rfc3339_opts(SecondsFormat::Secs, true), created_at: now() };
        self.store.set_json(&session_key(&sha_hex(token.as_bytes())), &s).await.map_err(st)?;
        self.audit(&u.id, "login", &format!("from {ip}; mfa {}", u.mfa_enabled)).await?;
        Ok(json!({"token":token,"expires_at":s.expires_at,"mfa_enrollment_required":!full,"must_change_password":u.must_change_password,"user":u.public()}))
    }

    async fn session(&self, token: &str) -> SResult<Option<(Session, StaffUser)>> {
        let key = session_key(&sha_hex(token.as_bytes()));
        let Some(s) = self.store.get_json::<Session>(&key).await.map_err(st)? else { return Ok(None) };
        if chrono::DateTime::parse_from_rfc3339(&s.expires_at).map(|e| e <= Utc::now()).unwrap_or(true) { return Ok(None); }
        match self.user(&s.user_id).await? { Some(u) if !u.disabled => Ok(Some((s, u))), _ => Ok(None) }
    }
    pub async fn logout(&self, token: &str) -> SResult<()> {
        let key = session_key(&sha_hex(token.as_bytes()));
        if let Some(mut s) = self.store.get_json::<Session>(&key).await.map_err(st)? { s.expires_at = now(); self.store.set_json(&key, &s).await.map_err(st)?; }
        Ok(())
    }

    pub async fn mfa_enroll(&self, who: &Identity) -> SResult<Value> {
        let mut u = self.user(&who.user_id).await?.ok_or_else(unauth)?;
        if u.mfa_enabled { return Err(conflict("MFA is already on; ask an admin to reset it")); }
        let secret = base32(&rand_bytes(20));
        u.pending_totp_secret = Some(secret.clone()); u.updated_at = now();
        self.put(&u).await?;
        let uri = format!("otpauth://totp/NXT%20OS:{}?secret={secret}&issuer=NXT%20OS&digits=6&period=30", u.username);
        Ok(json!({"secret":secret,"otpauth_uri":uri,"next":"scan in an authenticator app, then confirm with a current code"}))
    }
    pub async fn mfa_confirm(&self, who: &Identity, token: &str, code: &str) -> SResult<Value> {
        let mut u = self.user(&who.user_id).await?.ok_or_else(unauth)?;
        let secret = u.pending_totp_secret.clone().ok_or_else(|| conflict("start enrollment first"))?;
        let step = totp_match(&secret, code, u.last_totp_step).ok_or_else(|| bad("that code does not match; check the device clock"))?;
        u.totp_secret = Some(secret); u.pending_totp_secret = None; u.mfa_enabled = true; u.last_totp_step = step; u.updated_at = now();
        self.put(&u).await?;
        let key = session_key(&sha_hex(token.as_bytes()));
        if let Some(mut s) = self.store.get_json::<Session>(&key).await.map_err(st)? {
            s.mfa = true; s.expires_at = (Utc::now() + chrono::Duration::hours(SESSION_HOURS)).to_rfc3339_opts(SecondsFormat::Secs, true);
            self.store.set_json(&key, &s).await.map_err(st)?;
        }
        self.audit(&u.id, "mfa_enabled", "TOTP confirmed").await?;
        Ok(json!({"mfa_enabled":true}))
    }
    pub async fn change_password(&self, who: &Identity, input: PasswordInput) -> SResult<Value> {
        let mut u = self.user(&who.user_id).await?.ok_or_else(unauth)?;
        if !ct_eq(hex::encode(pbkdf2(input.current_password.as_bytes(), u.salt.as_bytes(), u.iterations)).as_bytes(), u.password_hash.as_bytes()) { return Err(ApiError(StatusCode::UNAUTHORIZED, "current password is wrong".into())); }
        valid_password(&input.new_password)?;
        if input.new_password == input.current_password { return Err(bad("choose a new password")); }
        u.salt = rand_hex(32); u.iterations = iterations(); u.password_hash = hex::encode(pbkdf2(input.new_password.as_bytes(), u.salt.as_bytes(), u.iterations));
        u.must_change_password = false; u.updated_at = now();
        self.put(&u).await?;
        self.audit(&u.id, "password_changed", "").await?;
        Ok(json!({"changed":true}))
    }

    /// Resolves a bearer credential to an identity.
    pub async fn identify(&self, bearer: &str) -> Result<Identity, Response> {
        if bearer.starts_with("nxs_") {
            return match self.session(bearer).await {
                Ok(Some((s, u))) => Ok(Identity { user_id: u.id, display_name: u.display_name, role: u.role, clients: u.clients, break_glass: false, mfa: s.mfa }),
                Ok(None) => Err(auth_error(StatusCode::UNAUTHORIZED, "session expired or signed out; sign in again")),
                Err(e) => Err(auth_error(e.0, &e.1)),
            };
        }
        let key = std::env::var("ADMIN_API_KEY").ok().filter(|k| !k.trim().is_empty());
        let Some(key) = key else { return Err(auth_error(StatusCode::SERVICE_UNAVAILABLE, "Workspace sign-in is unavailable. Contact the administrator.")) };
        if !break_glass_enabled() { return Err(auth_error(StatusCode::UNAUTHORIZED, "the shared admin key is turned off; sign in with your staff account")); }
        if !ct_eq(bearer.as_bytes(), key.as_bytes()) { return Err(auth_error(StatusCode::UNAUTHORIZED, "invalid or missing API key")); }
        Ok(Identity::break_glass())
    }
}

pub fn auth_error(status: StatusCode, message: &str) -> Response {
    let mut r = ApiError(status, message.into()).into_response();
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    r.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    r
}

pub fn client_ip(h: &HeaderMap) -> String {
    h.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).map(str::trim).filter(|v| !v.is_empty())
        .or_else(|| h.get("x-real-ip").and_then(|v| v.to_str().ok()).map(str::trim))
        .unwrap_or("unknown").chars().take(64).collect()
}

// ---------- role policy ----------

fn self_service(path: &str) -> bool { path.starts_with("/api/staff/me") || path.starts_with("/api/staff/mfa/") || path == "/api/staff/logout" }

/// Reviewer-only actions: approvals and final sign-offs.
fn reviewer_action(method: &Method, path: &str, body: &Value) -> bool {
    if method != Method::POST { return false; }
    let seg: Vec<&str> = path.trim_matches('/').split('/').collect();
    match seg.as_slice() {
        ["api", "tax", "returns", _, "transition"] => matches!(body["status"].as_str(), Some("approved") | Some("filed")),
        ["api", "engagements", _, "signoffs"] => body["stage"] == "reviewed",
        ["api", "signatures", _, "transition"] => body["action"] == "approve",
        ["api", "signatures", "esign", "bulk-approve"] => true,
        _ => false,
    }
}

pub fn check_policy(id: &Identity, method: &Method, path: &str, body: &Value) -> Result<(), String> {
    if self_service(path) { return Ok(()); }
    if !id.break_glass && !id.mfa { return Err("finish MFA enrollment before using the workspace".into()); }
    let read = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
    if path.starts_with("/api/staff/") { return if id.role == Role::Admin { Ok(()) } else { Err("staff accounts are managed by an admin".into()) }; }
    if read { return Ok(()); }
    match id.role {
        Role::Admin => Ok(()),
        Role::ReadOnly => Err("read-only accounts cannot change anything".into()),
        Role::FrontDesk => if path.contains("/esign") && !path.ends_with("/bulk-approve") { Ok(()) } else { Err("front desk accounts can send and manage e-sign requests only".into()) },
        Role::Preparer | Role::Reviewer => {
            if path.starts_with("/api/migrations") { return Err("migrations need an admin".into()); }
            if id.role == Role::Preparer && reviewer_action(method, path, body) { return Err("this approval needs a reviewer or admin account".into()); }
            Ok(())
        }
    }
}

/// Replaces top-level responsibility labels with the verified account label.
pub fn stamp_labels(body: &mut Value, id: &Identity) -> bool {
    if id.break_glass { return false; }
    let Some(o) = body.as_object_mut() else { return false };
    let mut changed = false;
    for k in LABEL_KEYS { if let Some(v) = o.get_mut(k) { if v.is_string() || v.is_null() { *v = json!(id.label()); changed = true; } } }
    changed
}

/// The shared protected-route middleware.
pub async fn authorize(req: Request, next: Next) -> Response {
    let staff = registry();
    let bearer = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("").trim().to_string();
    let id = match staff.identify(&bearer).await { Ok(i) => i, Err(r) => return r };
    let (mut parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.clone();
    let is_json = parts.headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("application/json"));
    let (body, parsed) = if is_json && !matches!(method, Method::GET | Method::HEAD) {
        let bytes = match axum::body::to_bytes(body, 8 << 20).await { Ok(b) => b, Err(_) => return auth_error(StatusCode::PAYLOAD_TOO_LARGE, "request body too large") };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(mut v) => {
                if let Err(m) = check_policy(&id, &method, &path, &v) { return auth_error(StatusCode::FORBIDDEN, &m); }
                let b = if stamp_labels(&mut v, &id) { let s = v.to_string(); parts.headers.remove(header::CONTENT_LENGTH); Body::from(s) } else { Body::from(bytes) };
                (b, true)
            }
            Err(_) => (Body::from(bytes), false),
        }
    } else { (body, false) };
    if !parsed { if let Err(m) = check_policy(&id, &method, &path, &Value::Null) { return auth_error(StatusCode::FORBIDDEN, &m); } }
    if id.break_glass && !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        let _ = staff.audit("break-glass", "shared_key_write", &format!("{method} {path} from {}", client_ip(&parts.headers))).await;
    }
    parts.extensions.insert(id);
    next.run(Request::from_parts(parts, body)).await
}

// ---------- IP rate limiting (store counters, fixed windows) ----------

/// Counts one hit for `scope` from `ip` in the current window and refuses past `limit`.
/// Uses the existing store; no new infrastructure.
pub async fn rate_limit(store: &Store, scope: &str, ip: &str, limit: i64, window_secs: i64) -> Result<(), ApiError> {
    let w = Utc::now().timestamp() / window_secs.max(1);
    let key = format!("ratelimit:{scope}:{w}:{}", sha_hex(ip.as_bytes()));
    let n = store.incr_by(&key, 1).await.map_err(st)?;
    if n > limit { return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, "too many requests from this network; wait a few minutes and try again".into())); }
    Ok(())
}

// ---------- routes ----------

type R = SResult<Json<Value>>;
pub fn router() -> Router {
    let protected = Router::new()
        .route("/api/staff/me", get(me))
        .route("/api/staff/me/password", post(password))
        .route("/api/staff/mfa/enroll", post(enroll))
        .route("/api/staff/mfa/confirm", post(confirm))
        .route("/api/staff/logout", post(logout))
        .route("/api/staff/users", get(list).post(create))
        .route("/api/staff/users/{id}", post(update))
        .route("/api/staff/audit", get(audit))
        .layer(axum::middleware::from_fn(crate::referral::routes::require_admin));
    Router::new().route("/api/staff/login", post(login)).merge(protected)
        .layer(axum::middleware::map_response(|mut r: Response| async move { r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store")); r }))
}
fn bearer(h: &HeaderMap) -> String { h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("").trim().to_string() }
async fn login(h: HeaderMap, Json(v): Json<LoginInput>) -> R {
    let s = registry();
    let ip = client_ip(&h);
    rate_limit(&s.store, "staff-login", &ip, 30, 600).await?;
    Ok(Json(s.login(v, &ip).await?))
}
async fn me(Extension(i): Extension<Identity>) -> R { Ok(Json(json!({"identity":i,"label":i.label()}))) }
async fn enroll(Extension(i): Extension<Identity>) -> R { if i.break_glass { return Err(conflict("the shared key has no MFA; use a staff account")); } Ok(Json(registry().mfa_enroll(&i).await?)) }
async fn confirm(Extension(i): Extension<Identity>, h: HeaderMap, Json(v): Json<CodeInput>) -> R { Ok(Json(registry().mfa_confirm(&i, &bearer(&h), &v.code).await?)) }
async fn password(Extension(i): Extension<Identity>, Json(v): Json<PasswordInput>) -> R { if i.break_glass { return Err(conflict("use a staff account")); } Ok(Json(registry().change_password(&i, v).await?)) }
async fn logout(h: HeaderMap) -> R { registry().logout(&bearer(&h)).await?; Ok(Json(json!({"signed_out":true}))) }
async fn list(Extension(i): Extension<Identity>) -> R { admin(&i)?; Ok(Json(json!({"users":registry().users().await?.iter().map(StaffUser::public).collect::<Vec<_>>(),"break_glass_enabled":break_glass_enabled(),"mfa_required":require_mfa()}))) }
async fn create(Extension(i): Extension<Identity>, Json(v): Json<CreateUser>) -> R { admin(&i)?; Ok(Json(registry().create_user(&i, v).await?)) }
async fn update(Extension(i): Extension<Identity>, Path(id): Path<String>, Json(v): Json<UpdateUser>) -> R { admin(&i)?; Ok(Json(registry().update_user(&i, &id, v).await?)) }
async fn audit(Extension(i): Extension<Identity>) -> R { admin(&i)?; Ok(Json(json!({"events":registry().audit_log(500).await?}))) }
fn admin(i: &Identity) -> SResult<()> { if i.role == Role::Admin { Ok(()) } else { Err(denied("admin only")) } }

// ---- Per-client scoping -------------------------------------------------------------------

/// The caller's identity when it is limited to specific clients (None = unrestricted).
pub fn scoped(req: &Request) -> Option<Identity> {
    req.extensions().get::<Identity>().filter(|i| !i.clients.iter().any(|c| c == "*")).cloned()
}

/// Shared scope guard for modules whose records carry a `client_ref`.
/// `base` is the collection path (e.g. `/api/tax/returns`); `open` lists paths a scoped user
/// may still read (catalogs, rates). Everything else that is not a record path is refused.
pub async fn scope_guard<F, Fut>(req: Request, next: Next, base: &str, open: &[&str], lookup: F) -> Response
where F: FnOnce(String) -> Fut, Fut: std::future::Future<Output = Option<String>> {
    let Some(id) = scoped(&req) else { return next.run(req).await };
    let path = req.uri().path().to_string();
    if open.iter().any(|p| path == *p) { return next.run(req).await; }
    if path == base {
        if req.method() == Method::GET {
            let resp = next.run(req).await;
            return filter_list(resp, &id).await;
        }
        let (parts, body) = req.into_parts();
        let Ok(bytes) = axum::body::to_bytes(body, 8 * 1024 * 1024).await else { return denied("request body too large").into_response() };
        let cref = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| v["client_ref"].as_str().map(str::to_string)).unwrap_or_default();
        if !id.can_client(&cref) { return denied("you are not assigned to this client").into_response(); }
        return next.run(Request::from_parts(parts, Body::from(bytes))).await;
    }
    if let Some(rest) = path.strip_prefix(base).and_then(|r| r.strip_prefix('/')) {
        let rid = rest.split('/').next().unwrap_or("").to_string();
        return match lookup(rid).await {
            Some(c) if id.can_client(&c) => next.run(req).await,
            Some(_) => denied("you are not assigned to this client").into_response(),
            None => next.run(req).await, // unknown id: let the handler answer 404
        };
    }
    denied("firm-wide views need an account with access to all clients").into_response()
}

async fn filter_list(resp: Response, id: &Identity) -> Response {
    if !resp.status().is_success() { return resp; }
    let (mut parts, body) = resp.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 32 * 1024 * 1024).await else { return denied("response too large").into_response() };
    let Ok(Value::Array(items)) = serde_json::from_slice::<Value>(&bytes) else { return Response::from_parts(parts, Body::from(bytes)) };
    let kept: Vec<Value> = items.into_iter().filter(|v| id.can_client(v["client_ref"].as_str().unwrap_or(""))).collect();
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(serde_json::to_vec(&kept).unwrap_or_default()))
}
