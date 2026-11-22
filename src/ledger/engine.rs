//! NXT Ledger: double-entry posting engine. Every business event (invoice,
//! bill, payment, bank transaction, journal) becomes a balanced `Entry`;
//! reports are computed from entries, so the books always tie out.

use super::models::*;
use crate::store::Store;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::HashMap;

#[derive(Debug)]
pub enum LedgerError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Storage(String),
}

impl From<String> for LedgerError {
    fn from(e: String) -> Self {
        LedgerError::Storage(e)
    }
}

pub type LResult<T> = Result<T, LedgerError>;

fn bad<T>(msg: impl Into<String>) -> LResult<T> {
    Err(LedgerError::BadRequest(msg.into()))
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn today() -> String {
    chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

fn advance_date(date: &str, frequency: &str) -> LResult<String> {
    let d = parse_date(date)?;
    let months: i32 = match frequency {
        "weekly" => 0,
        "monthly" => 1,
        "quarterly" => 3,
        "yearly" => 12,
        _ => return bad("frequency must be weekly, monthly, quarterly or yearly"),
    };
    let next = if months == 0 {
        d + chrono::Duration::days(7)
    } else {
        let total = d.year() * 12 + d.month0() as i32 + months;
        let (year, month) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
        let last = days_in_month(year, month);
        NaiveDate::from_ymd_opt(year, month, d.day().min(last)).ok_or_else(|| LedgerError::BadRequest("the next date is out of range".into()))?
    };
    Ok(next.format("%Y-%m-%d").to_string())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    (NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap() - NaiveDate::from_ymd_opt(year, month, 1).unwrap()).num_days() as u32
}

pub fn parse_date(s: &str) -> LResult<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| LedgerError::BadRequest(format!("invalid date '{s}', expected YYYY-MM-DD")))
}

/// Rounds half away from zero to whole cents.
pub fn round_cents(v: f64) -> i64 {
    v.round() as i64
}

/// Tax on a pre-tax amount.
pub fn tax_on(amount: i64, rate_bp: i64) -> i64 {
    round_cents(amount as f64 * rate_bp as f64 / 10_000.0)
}

/// Splits a tax-inclusive gross amount into (net, tax).
pub fn split_gross(gross: i64, rate_bp: i64) -> (i64, i64) {
    let net = round_cents(gross as f64 * 10_000.0 / (10_000 + rate_bp) as f64);
    (net, gross - net)
}

pub struct Ledger {
    pub store: Store,
    /// Key prefix of this book: `gl:` for the firm, per company otherwise.
    keys: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JournalInput {
    pub date: String,
    #[serde(default)]
    pub memo: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Deserialize)]
pub struct DocLineInput {
    pub description: String,
    #[serde(default = "one")]
    pub qty: f64,
    pub rate: i64,
    pub account_id: String,
    #[serde(default)]
    pub tax_code: Option<String>,
    #[serde(default)]
    pub class_id: Option<String>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct DocInput {
    pub contact_id: String,
    pub date: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub memo: String,
    pub lines: Vec<DocLineInput>,
}

#[derive(Debug, Deserialize)]
pub struct PaymentInput {
    pub contact_id: String,
    pub date: String,
    pub account_id: String,
    pub amount: i64,
    #[serde(default)]
    pub applications: Vec<Application>,
    #[serde(default)]
    pub memo: String,
}

#[derive(Debug, Deserialize)]
pub struct MoneyLineInput {
    pub account_id: String,
    pub amount: i64,
    #[serde(default)]
    pub tax_code: Option<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct MoneyInput {
    /// Bank or credit card account.
    pub account_id: String,
    pub date: String,
    #[serde(default)]
    pub contact_id: Option<String>,
    #[serde(default)]
    pub memo: String,
    pub lines: Vec<MoneyLineInput>,
    /// When true, line amounts already include tax (e.g. a card receipt).
    #[serde(default)]
    pub amounts_include_tax: bool,
}

#[derive(Debug, Deserialize)]
pub struct TransferInput {
    pub from_account_id: String,
    pub to_account_id: String,
    pub amount: i64,
    pub date: String,
    #[serde(default)]
    pub memo: String,
}

#[derive(Debug, Deserialize)]
pub struct TaxPaymentInput {
    /// Bank account the remittance is paid from (or refund deposited to).
    pub account_id: String,
    /// Positive = remittance to the CRA, negative = refund received.
    pub amount: i64,
    pub date: String,
    #[serde(default)]
    pub memo: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountInput {
    pub code: String,
    pub name: String,
    pub kind: AccountKind,
    #[serde(default)]
    pub role: AccountRole,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct ContactInput {
    pub kind: ContactKind,
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub terms_days: Option<i64>,
    #[serde(default)]
    pub default_tax_code: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct CategorizeInput {
    pub account_id: String,
    #[serde(default)]
    pub tax_code: Option<String>,
    #[serde(default)]
    pub contact_id: Option<String>,
    /// Remember this choice for future feed items with the same keyword.
    #[serde(default)]
    pub create_rule: bool,
}

#[derive(Debug, Deserialize)]
pub struct BankRuleInput {
    /// Keyword matched against the lowercased feed description.
    pub keyword: String,
    pub account_id: String,
    #[serde(default)]
    pub tax_code: Option<String>,
    #[serde(default)]
    pub contact_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchInput {
    pub entries: Vec<JournalInput>,
}

#[derive(Debug, Deserialize)]
pub struct SavedReportInput {
    pub name: String,
    pub report: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SalesReceiptInput {
    pub contact_id: String,
    pub date: String,
    /// Bank or credit-card account the money landed in.
    pub account_id: String,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub memo: String,
    pub lines: Vec<DocLineInput>,
}

#[derive(Debug, Deserialize)]
pub struct ClassInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct BudgetInput {
    pub account_id: String,
    /// YYYY-MM
    pub month: String,
    pub amount: i64,
}

#[derive(Debug, Deserialize)]
pub struct ApplyCreditInput {
    pub invoice_id: String,
    pub amount: i64,
}

#[derive(Debug, Deserialize)]
pub struct RecurringInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "monthly_frequency")]
    pub frequency: String,
    pub next_date: String,
    #[serde(default)]
    pub memo: String,
    pub lines: Vec<Line>,
}

fn monthly_frequency() -> String {
    "monthly".into()
}

#[derive(Debug, Deserialize)]
pub struct ReconcileInput {
    pub account_id: String,
    pub statement_date: String,
    pub statement_balance: i64,
    /// Line references "entry_id:line_index" to mark cleared.
    pub line_refs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterRow {
    pub entry_id: String,
    pub line_index: usize,
    pub number: i64,
    pub date: String,
    pub source: String,
    pub memo: String,
    pub contact: Option<String>,
    pub debit: i64,
    pub credit: i64,
    pub balance: i64,
    pub cleared: bool,
}

impl Ledger {
    pub fn new(store: Store) -> Self {
        Ledger { store, keys: "gl:".into() }
    }

    /// One client's own book. It shares no accounts, entries, sequences or settings with any other book.
    pub fn for_company(store: Store, company: &str) -> Self {
        Ledger { store, keys: format!("company-ledger:{company}:") }
    }

    fn key(&self, name: &str) -> String {
        format!("{}{name}", self.keys)
    }

    // ---- generic collection helpers ---------------------------------------

    async fn put<T: Serialize>(&self, coll: &str, id: &str, v: &T) -> LResult<()> {
        self.store.set_json(&self.key(&format!("{coll}:{id}")), v).await?;
        self.store.sadd(&self.key(coll), id).await?;
        Ok(())
    }

    async fn get<T: DeserializeOwned>(&self, coll: &str, id: &str) -> LResult<Option<T>> {
        Ok(self.store.get_json(&self.key(&format!("{coll}:{id}"))).await?)
    }

    async fn must<T: DeserializeOwned>(&self, coll: &str, id: &str) -> LResult<T> {
        self.get(coll, id)
            .await?
            .ok_or_else(|| LedgerError::NotFound(format!("{} '{id}' not found", coll.trim_end_matches('s'))))
    }

    async fn all<T: DeserializeOwned>(&self, coll: &str) -> LResult<Vec<T>> {
        let ids = self.store.smembers(&self.key(coll)).await?;
        let keys: Vec<String> = ids.iter().map(|id| self.key(&format!("{coll}:{id}"))).collect();
        // A report must never silently leave out a record it cannot read.
        self.store
            .mget(&keys)
            .await?
            .into_iter()
            .map(|raw| {
                let raw = raw.ok_or_else(|| LedgerError::Storage(format!("a {coll} record is missing")))?;
                serde_json::from_str(&raw).map_err(|_| LedgerError::Storage(format!("a {coll} record is unreadable")))
            })
            .collect()
    }

    pub async fn audit(&self, action: &str, detail: String) -> LResult<()> {
        let ev = AuditEvent { at: now(), action: action.into(), detail };
        self.store.lpush_capped(&self.key("audit"), &serde_json::to_string(&ev).unwrap(), 2000).await?;
        Ok(())
    }

    pub async fn audit_log(&self, limit: usize) -> LResult<Vec<AuditEvent>> {
        Ok(self.store.list_json(&self.key("audit"), limit).await?)
    }

    // ---- company & chart ---------------------------------------------------

    pub async fn ensure_seeded(&self) -> LResult<()> {
        if self.store.set_nx(&self.key("seeded"), "1").await? {
            for a in default_chart() {
                self.put("accounts", &a.id, &a).await?;
            }
        }
        Ok(())
    }

    pub async fn company(&self) -> LResult<Company> {
        Ok(self.store.get_json(&self.key("company")).await?.unwrap_or_default())
    }

    pub async fn save_company(&self, c: &Company) -> LResult<()> {
        if !(1..=12).contains(&c.fiscal_year_start_month) {
            return bad("fiscal_year_start_month must be 1-12");
        }
        if let Some(d) = &c.closing_date {
            if !d.is_empty() {
                parse_date(d)?;
            }
        }
        if tax_rate(&c.default_tax_code).is_none() {
            return bad("unknown default_tax_code");
        }
        self.store.set_json(&self.key("company"), c).await?;
        self.audit("company.updated", c.name.clone()).await
    }

    pub async fn accounts(&self) -> LResult<Vec<Account>> {
        self.ensure_seeded().await?;
        let mut v: Vec<Account> = self.all("accounts").await?;
        v.sort_by(|a, b| a.code.cmp(&b.code));
        Ok(v)
    }

    /// The chart as [`Self::accounts`] would return it, without seeding. A pure read: on a
    /// ledger that was never seeded it returns the default chart in memory and writes nothing.
    /// Read-only surfaces (reports, Nixon, God's Eye) use this so a view never writes the books.
    pub async fn accounts_read(&self) -> LResult<Vec<Account>> {
        let mut v: Vec<Account> = self.all("accounts").await?;
        if self.store.get(&self.key("seeded")).await?.is_none() {
            // Seeding would put the default chart over any same-id record; mirror that.
            let defaults = default_chart();
            v.retain(|a| !defaults.iter().any(|d| d.id == a.id));
            v.extend(defaults);
        }
        v.sort_by(|a, b| a.code.cmp(&b.code));
        Ok(v)
    }

    pub async fn account(&self, id: &str) -> LResult<Account> {
        self.ensure_seeded().await?;
        self.must("accounts", id).await
    }

    async fn account_by_role(&self, role: AccountRole) -> LResult<Account> {
        self.accounts()
            .await?
            .into_iter()
            .find(|a| a.role == role && !a.archived)
            .ok_or_else(|| LedgerError::Conflict(format!("no active account with role {role:?}")))
    }

    pub async fn save_account(&self, id: Option<&str>, input: AccountInput) -> LResult<Account> {
        let code = input.code.trim().to_string();
        let name = input.name.trim().to_string();
        if code.is_empty() || name.is_empty() {
            return bad("code and name are required");
        }
        let all = self.accounts().await?;
        if all.iter().any(|a| a.code == code && Some(a.id.as_str()) != id) {
            return Err(LedgerError::Conflict(format!("account code {code} is already used")));
        }
        let acct = match id {
            Some(id) => {
                let mut a: Account = self.must("accounts", id).await?;
                if a.kind != input.kind && self.account_has_activity(id).await? {
                    return Err(LedgerError::Conflict("can't change the type of an account with transactions".into()));
                }
                a.code = code;
                a.name = name;
                a.kind = input.kind;
                a.role = input.role;
                a.description = input.description;
                a.archived = input.archived;
                a
            }
            None => Account {
                id: new_id("acc"),
                code,
                name,
                kind: input.kind,
                role: input.role,
                description: input.description,
                archived: input.archived,
            },
        };
        self.put("accounts", &acct.id, &acct).await?;
        self.audit("account.saved", format!("{} {}", acct.code, acct.name)).await?;
        Ok(acct)
    }

    async fn account_has_activity(&self, id: &str) -> LResult<bool> {
        Ok(self.entries().await?.iter().any(|e| e.lines.iter().any(|l| l.account_id == id)))
    }

    // ---- contacts ------------------------------------------------------------

    pub async fn contacts(&self, kind: Option<ContactKind>) -> LResult<Vec<Contact>> {
        let mut v: Vec<Contact> = self.all("contacts").await?;
        v.retain(|c| kind.is_none_or(|k| c.kind == k));
        v.sort_by_key(|c| c.name.to_lowercase());
        Ok(v)
    }

    pub async fn contact(&self, id: &str) -> LResult<Contact> {
        self.must("contacts", id).await
    }

    pub async fn save_contact(&self, id: Option<&str>, input: ContactInput) -> LResult<Contact> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return bad("name is required");
        }
        if let Some(t) = &input.default_tax_code {
            if tax_rate(t).is_none() {
                return bad(format!("unknown tax code {t}"));
            }
        }
        let mut c = match id {
            Some(id) => self.contact(id).await?,
            None => Contact {
                id: new_id(if input.kind == ContactKind::Customer { "cus" } else { "ven" }),
                kind: input.kind,
                name: String::new(),
                email: String::new(),
                phone: String::new(),
                address: String::new(),
                terms_days: 30,
                default_tax_code: None,
                archived: false,
                created_at: now(),
            },
        };
        c.name = name;
        c.email = input.email.trim().to_string();
        c.phone = input.phone.trim().to_string();
        c.address = input.address.trim().to_string();
        c.terms_days = input.terms_days.unwrap_or(c.terms_days).clamp(0, 365);
        c.default_tax_code = input.default_tax_code;
        c.archived = input.archived;
        self.put("contacts", &c.id, &c).await?;
        Ok(c)
    }

    // ---- posting ---------------------------------------------------------------

    pub async fn entries(&self) -> LResult<Vec<Entry>> {
        let mut v: Vec<Entry> = self.all("entries").await?;
        v.sort_by(|a, b| a.date.cmp(&b.date).then(a.number.cmp(&b.number)));
        Ok(v)
    }

    pub async fn entry(&self, id: &str) -> LResult<Entry> {
        self.must("entries", id).await
    }

    async fn check_period(&self, date: &str) -> LResult<()> {
        parse_date(date)?;
        if let Some(close) = self.company().await?.closing_date.filter(|d| !d.is_empty()) {
            if date <= close.as_str() {
                return Err(LedgerError::Conflict(format!("books are closed through {close}")));
            }
        }
        Ok(())
    }

    /// Validates and saves a balanced entry. All postings go through here.
    /// Drops zero lines and rejects unbalanced or unpostable lines. Every posting
    /// and every saved recurring template goes through this check.
    async fn clean_lines(&self, mut lines: Vec<Line>) -> LResult<Vec<Line>> {
        lines.retain(|l| l.debit != 0 || l.credit != 0);
        if lines.len() < 2 {
            return bad("an entry needs at least two non-zero lines");
        }
        let accounts: HashMap<String, Account> = self.accounts().await?.into_iter().map(|a| (a.id.clone(), a)).collect();
        let (mut dr, mut cr) = (0i64, 0i64);
        for l in &lines {
            if l.debit < 0 || l.credit < 0 || (l.debit != 0 && l.credit != 0) {
                return bad("each line must have either a positive debit or a positive credit");
            }
            match accounts.get(&l.account_id) {
                None => return bad(format!("unknown account {}", l.account_id)),
                Some(a) if a.archived => return bad(format!("account {} {} is archived", a.code, a.name)),
                _ => {}
            }
            if let Some(class_id) = l.class_id.as_deref().filter(|id| !id.is_empty()) {
                if !self.known_class(class_id).await? {
                    return bad(format!("unknown or archived class {class_id}"));
                }
            }
            dr += l.debit;
            cr += l.credit;
        }
        if dr != cr {
            return bad(format!("entry is out of balance: debits {dr} ≠ credits {cr}"));
        }
        Ok(lines)
    }

    async fn post(&self, date: &str, memo: &str, lines: Vec<Line>, source: &str, source_id: Option<String>) -> LResult<Entry> {
        self.check_period(date).await?;
        let lines = self.clean_lines(lines).await?;
        let number = self.store.incr_by(&self.key("seq:entry"), 1).await?;
        let entry = Entry {
            id: new_id("je"),
            number,
            date: date.to_string(),
            memo: memo.trim().to_string(),
            lines,
            source: source.to_string(),
            source_id,
            voided: false,
            created_at: now(),
        };
        self.put("entries", &entry.id, &entry).await?;
        self.audit("entry.posted", format!("#{} {} {}", entry.number, entry.source, entry.memo)).await?;
        Ok(entry)
    }

    /// Writes one imported account as it stands; the caller has chosen its id and code.
    pub(super) async fn import_account(&self, account: &Account) -> LResult<()> {
        self.put("accounts", &account.id, account).await
    }

    /// True for the first caller only.
    pub(super) async fn claim(&self, name: &str) -> LResult<bool> {
        Ok(self.store.set_nx(&self.key(name), "1").await?)
    }

    pub(super) async fn named_get(&self, name: &str) -> LResult<Option<serde_json::Value>> {
        Ok(self.store.get_json(&self.key(name)).await?)
    }

    pub(super) async fn named_set(&self, name: &str, value: &serde_json::Value) -> LResult<()> {
        Ok(self.store.set_json(&self.key(name), value).await?)
    }

    pub(super) async fn post_opening(&self, date: &str, memo: &str, lines: Vec<Line>, source_id: &str) -> LResult<Entry> {
        self.post(date, memo, lines, "opening_balance", Some(source_id.to_string())).await
    }

    async fn void_entry_record(&self, id: &str) -> LResult<Entry> {
        let mut e = self.entry(id).await?;
        self.check_period(&e.date).await?;
        if e.voided {
            return Err(LedgerError::Conflict("entry is already void".into()));
        }
        e.voided = true;
        self.put("entries", &e.id, &e).await?;
        self.audit("entry.voided", format!("#{}", e.number)).await?;
        Ok(e)
    }

    pub async fn create_journal(&self, input: JournalInput) -> LResult<Entry> {
        self.post(&input.date, &input.memo, input.lines, "journal", None).await
    }

    pub async fn void_journal(&self, id: &str) -> LResult<Entry> {
        let e = self.entry(id).await?;
        if !["journal", "money_in", "money_out", "transfer", "tax_payment"].contains(&e.source.as_str()) {
            return Err(LedgerError::Conflict(format!("void the {} itself instead of its entry", e.source.replace('_', " "))));
        }
        self.void_entry_record(id).await
    }

    // ---- invoices & bills ------------------------------------------------------

    pub async fn docs(&self, kind: Option<DocKind>) -> LResult<Vec<Doc>> {
        let mut v: Vec<Doc> = self.all("docs").await?;
        v.retain(|d| kind.is_none_or(|k| d.kind == k));
        v.sort_by(|a, b| b.date.cmp(&a.date).then(b.number.cmp(&a.number)));
        Ok(v)
    }

    pub async fn doc(&self, id: &str) -> LResult<Doc> {
        self.must("docs", id).await
    }

    pub async fn create_doc(&self, kind: DocKind, input: DocInput) -> LResult<Doc> {
        let contact = self.contact(&input.contact_id).await?;
        let (want, label) = match kind {
            DocKind::Invoice => (ContactKind::Customer, "invoice"),
            DocKind::Bill => (ContactKind::Vendor, "bill"),
            DocKind::CreditMemo => (ContactKind::Customer, "credit memo"),
        };
        if contact.kind != want {
            return bad(format!("{label} must be for a {want:?}").to_lowercase());
        }
        if input.lines.is_empty() {
            return bad("add at least one line");
        }
        let company = self.company().await?;
        let date = parse_date(&input.date)?;
        let due = match &input.due_date {
            Some(d) if !d.is_empty() => parse_date(d)?,
            _ => date + chrono::Duration::days(contact.terms_days),
        };
        // A credit memo is not something the customer pays on a due date.
        let due = if kind == DocKind::CreditMemo { date } else { due };
        if due < date {
            return bad("due date can't be before the document date");
        }
        let default_tax = contact.default_tax_code.clone().unwrap_or(company.default_tax_code.clone());
        let mut lines = Vec::new();
        for l in input.lines {
            let acct = self.account(&l.account_id).await?;
            let ok = match kind {
                DocKind::Invoice | DocKind::CreditMemo => acct.kind == AccountKind::Income,
                DocKind::Bill => matches!(acct.kind, AccountKind::Expense | AccountKind::Asset) && !acct.is_money(),
            };
            if !ok {
                return bad(format!("account {} {} can't be used on a {:?} line", acct.code, acct.name, kind).to_lowercase());
            }
            if !(l.qty.is_finite() && l.qty > 0.0) || l.rate < 0 {
                return bad("quantity must be positive and rate non-negative");
            }
            if let Some(class_id) = l.class_id.as_deref().filter(|id| !id.is_empty()) {
                if !self.known_class(class_id).await? {
                    return bad(format!("unknown or archived class {class_id}"));
                }
            }
            let tax_code = l.tax_code.filter(|t| !t.is_empty()).unwrap_or(default_tax.clone());
            let rate_bp = tax_rate(&tax_code).ok_or_else(|| LedgerError::BadRequest(format!("unknown tax code {tax_code}")))?;
            let amount = round_cents(l.qty * l.rate as f64);
            lines.push(DocLine { description: l.description, qty: l.qty, rate: l.rate, account_id: acct.id, tax: tax_on(amount, rate_bp), tax_code, amount, class_id: l.class_id });
        }
        let subtotal: i64 = lines.iter().map(|l| l.amount).sum();
        let tax: i64 = lines.iter().map(|l| l.tax).sum();
        let total = subtotal + tax;
        if total <= 0 {
            return bad("document total must be greater than zero");
        }
        let number = match input.number.filter(|n| !n.trim().is_empty()) {
            Some(n) => n.trim().to_string(),
            None if kind == DocKind::Invoice => {
                format!("{}{:04}", company.invoice_prefix, self.store.incr_by(&self.key("seq:invoice"), 1).await? + 1000)
            }
            None if kind == DocKind::CreditMemo => {
                format!("CM-{:04}", self.store.incr_by(&self.key("seq:credit_memo"), 1).await?)
            }
            None => format!("BILL-{:04}", self.store.incr_by(&self.key("seq:bill"), 1).await?),
        };
        let doc_id = new_id(match kind {
            DocKind::Invoice => "inv",
            DocKind::Bill => "bill",
            DocKind::CreditMemo => "cm",
        });
        let cid = Some(contact.id.clone());
        let mut glines = Vec::new();
        match kind {
            DocKind::Invoice => {
                let ar = self.account_by_role(AccountRole::Receivable).await?;
                glines.push(Line { account_id: ar.id, debit: total, contact_id: cid.clone(), memo: format!("Invoice {number}"), ..Default::default() });
                for l in &lines {
                    glines.push(Line { account_id: l.account_id.clone(), credit: l.amount, memo: l.description.clone(), contact_id: cid.clone(), class_id: l.class_id.clone(), ..Default::default() });
                }
                if tax > 0 {
                    let tp = self.account_by_role(AccountRole::TaxPayable).await?;
                    glines.push(Line { account_id: tp.id, credit: tax, memo: "GST/HST collected".into(), contact_id: cid.clone(), ..Default::default() });
                }
            }
            DocKind::CreditMemo => {
                let ar = self.account_by_role(AccountRole::Receivable).await?;
                glines.push(Line { account_id: ar.id, credit: total, contact_id: cid.clone(), memo: format!("Credit memo {number}"), ..Default::default() });
                for l in &lines {
                    glines.push(Line { account_id: l.account_id.clone(), debit: l.amount, memo: l.description.clone(), contact_id: cid.clone(), class_id: l.class_id.clone(), ..Default::default() });
                }
                if tax > 0 {
                    let tp = self.account_by_role(AccountRole::TaxPayable).await?;
                    glines.push(Line { account_id: tp.id, debit: tax, memo: "GST/HST reversed".into(), contact_id: cid.clone(), ..Default::default() });
                }
            }
            DocKind::Bill => {
                let ap = self.account_by_role(AccountRole::Payable).await?;
                for l in &lines {
                    glines.push(Line { account_id: l.account_id.clone(), debit: l.amount, memo: l.description.clone(), contact_id: cid.clone(), class_id: l.class_id.clone(), ..Default::default() });
                }
                if tax > 0 {
                    let tr = self.account_by_role(AccountRole::TaxRecoverable).await?;
                    glines.push(Line { account_id: tr.id, debit: tax, memo: "GST/HST paid (ITC)".into(), contact_id: cid.clone(), ..Default::default() });
                }
                glines.push(Line { account_id: ap.id, credit: total, contact_id: cid.clone(), memo: format!("Bill {number}"), ..Default::default() });
            }
        }
        let (src, doc_label) = match kind {
            DocKind::Invoice => ("invoice", "Invoice"),
            DocKind::Bill => ("bill", "Bill"),
            DocKind::CreditMemo => ("credit_memo", "Credit memo"),
        };
        let entry = self
            .post(&input.date, &format!("{doc_label} {number} · {}", contact.name), glines, src, Some(doc_id.clone()))
            .await?;
        let doc = Doc {
            id: doc_id,
            kind,
            number,
            contact_id: contact.id,
            date: input.date,
            due_date: due.format("%Y-%m-%d").to_string(),
            lines,
            memo: input.memo,
            subtotal,
            tax,
            total,
            paid: 0,
            voided: false,
            entry_id: entry.id,
            created_at: now(),
            applications: Vec::new(),
        };
        self.put("docs", &doc.id, &doc).await?;
        Ok(doc)
    }

    /// Apply an unapplied credit memo to an open invoice for the same customer.
    /// The memo already posted Dr income / Cr AR and the invoice Dr AR / Cr income,
    /// so applying them is an allocation, not another posting.
    pub async fn apply_credit(&self, credit_id: &str, input: ApplyCreditInput) -> LResult<Doc> {
        if input.amount <= 0 {
            return bad("amount must be positive");
        }
        let mut credit = self.doc(credit_id).await?;
        if credit.kind != DocKind::CreditMemo {
            return bad("only a credit memo can be applied");
        }
        if credit.voided {
            return Err(LedgerError::Conflict("this credit memo is void".into()));
        }
        if input.amount > credit.balance() {
            return bad(format!("the credit memo has {} left", fmt_money(credit.balance())));
        }
        let mut inv = self.doc(&input.invoice_id).await?;
        if inv.kind != DocKind::Invoice || inv.voided {
            return bad("apply the credit to an open invoice");
        }
        if inv.contact_id != credit.contact_id {
            return bad("the credit memo and the invoice must belong to the same customer");
        }
        if input.amount > inv.balance() {
            return bad(format!("{} can take at most {}", inv.number, fmt_money(inv.balance())));
        }
        inv.paid += input.amount;
        credit.paid += input.amount;
        if let Some(app) = credit.applications.iter_mut().find(|a| a.doc_id == inv.id) {
            app.amount += input.amount;
        } else {
            credit.applications.push(Application { doc_id: inv.id.clone(), amount: input.amount });
        }
        self.put("docs", &inv.id, &inv).await?;
        self.put("docs", &credit.id, &credit).await?;
        self.audit("credit_memo.applied", format!("{} -> {} {}", credit.number, inv.number, fmt_money(input.amount))).await?;
        Ok(credit)
    }

    pub async fn void_doc(&self, id: &str) -> LResult<Doc> {
        let mut d = self.doc(id).await?;
        if d.voided {
            return Err(LedgerError::Conflict("already void".into()));
        }
        if d.kind == DocKind::CreditMemo {
            // Voiding a credit memo takes it back off the invoices it was applied to.
            for app in &d.applications {
                if let Some(mut inv) = self.get::<Doc>("docs", &app.doc_id).await? {
                    inv.paid = (inv.paid - app.amount).max(0);
                    self.put("docs", &inv.id, &inv).await?;
                }
            }
            d.paid = 0;
            d.applications.clear();
        } else if d.paid > 0 {
            return Err(LedgerError::Conflict("void the payments applied to this document first".into()));
        }
        self.void_entry_record(&d.entry_id).await?;
        d.voided = true;
        self.put("docs", &d.id, &d).await?;
        Ok(d)
    }

    // ---- payments --------------------------------------------------------------

    pub async fn payments(&self) -> LResult<Vec<Payment>> {
        let mut v: Vec<Payment> = self.all("payments").await?;
        v.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(v)
    }

    async fn money_account(&self, id: &str) -> LResult<Account> {
        let a = self.account(id).await?;
        if !a.is_money() {
            return bad(format!("{} {} isn't a bank or credit card account", a.code, a.name));
        }
        Ok(a)
    }

    /// direction: "received" pays invoices, "made" pays bills. With no explicit
    /// applications the amount is applied to the oldest open documents.
    pub async fn record_payment(&self, direction: &str, input: PaymentInput) -> LResult<Payment> {
        let kind = match direction {
            "received" => DocKind::Invoice,
            "made" => DocKind::Bill,
            _ => return bad("direction must be 'received' or 'made'"),
        };
        if input.amount <= 0 {
            return bad("amount must be positive");
        }
        let contact = self.contact(&input.contact_id).await?;
        let bank = self.money_account(&input.account_id).await?;
        let mut open: Vec<Doc> = self
            .docs(Some(kind))
            .await?
            .into_iter()
            .filter(|d| d.contact_id == contact.id && d.balance() > 0)
            .collect();
        open.sort_by(|a, b| a.due_date.cmp(&b.due_date));
        let apps = if input.applications.is_empty() {
            let mut left = input.amount;
            let mut apps = Vec::new();
            for d in &open {
                if left == 0 {
                    break;
                }
                let amt = left.min(d.balance());
                apps.push(Application { doc_id: d.id.clone(), amount: amt });
                left -= amt;
            }
            if left > 0 {
                return bad(format!("payment exceeds {}'s open balance by {}", contact.name, fmt_money(left)));
            }
            apps
        } else {
            input.applications
        };
        let applied: i64 = apps.iter().map(|a| a.amount).sum();
        if applied != input.amount {
            return bad(format!("applied {} but payment is {}", fmt_money(applied), fmt_money(input.amount)));
        }
        let mut touched = Vec::new();
        for a in &apps {
            let Some(d) = open.iter().find(|d| d.id == a.doc_id) else {
                return bad(format!("document {} isn't open for {}", a.doc_id, contact.name));
            };
            if a.amount <= 0 || a.amount > d.balance() {
                return bad(format!("{} can take at most {}", d.number, fmt_money(d.balance())));
            }
            touched.push((d.clone(), a.amount));
        }
        let pay_id = new_id("pay");
        let cid = Some(contact.id.clone());
        let numbers: Vec<String> = touched.iter().map(|(d, _)| d.number.clone()).collect();
        let lines = if kind == DocKind::Invoice {
            let ar = self.account_by_role(AccountRole::Receivable).await?;
            vec![
                Line { account_id: bank.id.clone(), debit: input.amount, contact_id: cid.clone(), memo: input.memo.clone(), ..Default::default() },
                Line { account_id: ar.id, credit: input.amount, contact_id: cid.clone(), memo: numbers.join(", "), ..Default::default() },
            ]
        } else {
            let ap = self.account_by_role(AccountRole::Payable).await?;
            vec![
                Line { account_id: ap.id, debit: input.amount, contact_id: cid.clone(), memo: numbers.join(", "), ..Default::default() },
                Line { account_id: bank.id.clone(), credit: input.amount, contact_id: cid.clone(), memo: input.memo.clone(), ..Default::default() },
            ]
        };
        let src = if kind == DocKind::Invoice { "payment_received" } else { "payment_made" };
        let label = if kind == DocKind::Invoice { "Payment from" } else { "Payment to" };
        let entry = self.post(&input.date, &format!("{label} {}", contact.name), lines, src, Some(pay_id.clone())).await?;
        for (mut d, amt) in touched {
            d.paid += amt;
            self.put("docs", &d.id, &d).await?;
        }
        let p = Payment {
            id: pay_id,
            direction: direction.into(),
            contact_id: contact.id,
            date: input.date,
            account_id: bank.id,
            amount: input.amount,
            applications: apps,
            memo: input.memo,
            entry_id: entry.id,
            voided: false,
            created_at: now(),
        };
        self.put("payments", &p.id, &p).await?;
        Ok(p)
    }

    pub async fn void_payment(&self, id: &str) -> LResult<Payment> {
        let mut p: Payment = self.must("payments", id).await?;
        if p.voided {
            return Err(LedgerError::Conflict("already void".into()));
        }
        self.void_entry_record(&p.entry_id).await?;
        for a in &p.applications {
            if let Some(mut d) = self.get::<Doc>("docs", &a.doc_id).await? {
                d.paid = (d.paid - a.amount).max(0);
                self.put("docs", &d.id, &d).await?;
            }
        }
        p.voided = true;
        self.put("payments", &p.id, &p).await?;
        Ok(p)
    }

    // ---- banking ---------------------------------------------------------------

    /// Money out (expense/cheque/card charge) or money in (deposit/sale).
    pub async fn money(&self, direction: &str, input: MoneyInput) -> LResult<Entry> {
        let out = match direction {
            "out" => true,
            "in" => false,
            _ => return bad("direction must be 'in' or 'out'"),
        };
        let bank = self.money_account(&input.account_id).await?;
        if input.lines.is_empty() {
            return bad("add at least one line");
        }
        let company = self.company().await?;
        let contact_name = match &input.contact_id {
            Some(c) if !c.is_empty() => Some(self.contact(c).await?.name),
            _ => None,
        };
        let cid = input.contact_id.clone().filter(|c| !c.is_empty());
        let mut lines = Vec::new();
        let (mut total, mut tax_total) = (0i64, 0i64);
        for l in &input.lines {
            if l.amount <= 0 {
                return bad("line amounts must be positive");
            }
            let acct = self.account(&l.account_id).await?;
            if acct.id == bank.id {
                return bad("a line can't post to the same bank account");
            }
            let code = l.tax_code.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| if out { company.default_tax_code.clone() } else { "NONE".into() });
            let rate = tax_rate(&code).ok_or_else(|| LedgerError::BadRequest(format!("unknown tax code {code}")))?;
            let (net, tax) = if input.amounts_include_tax { split_gross(l.amount, rate) } else { (l.amount, tax_on(l.amount, rate)) };
            total += net + tax;
            tax_total += tax;
            let line = if out {
                Line { account_id: acct.id, debit: net, memo: l.description.clone(), contact_id: cid.clone(), ..Default::default() }
            } else {
                Line { account_id: acct.id, credit: net, memo: l.description.clone(), contact_id: cid.clone(), ..Default::default() }
            };
            lines.push(line);
        }
        if tax_total > 0 {
            let acct = self.account_by_role(if out { AccountRole::TaxRecoverable } else { AccountRole::TaxPayable }).await?;
            lines.push(if out {
                Line { account_id: acct.id, debit: tax_total, memo: "GST/HST paid (ITC)".into(), contact_id: cid.clone(), ..Default::default() }
            } else {
                Line { account_id: acct.id, credit: tax_total, memo: "GST/HST collected".into(), contact_id: cid.clone(), ..Default::default() }
            });
        }
        lines.push(if out {
            Line { account_id: bank.id, credit: total, memo: input.memo.clone(), contact_id: cid.clone(), ..Default::default() }
        } else {
            Line { account_id: bank.id, debit: total, memo: input.memo.clone(), contact_id: cid.clone(), ..Default::default() }
        });
        let memo = match (&contact_name, input.memo.is_empty()) {
            (Some(n), true) => n.clone(),
            (Some(n), false) => format!("{n} · {}", input.memo),
            (None, _) => input.memo.clone(),
        };
        self.post(&input.date, &memo, lines, if out { "money_out" } else { "money_in" }, None).await
    }

    pub async fn transfer(&self, input: TransferInput) -> LResult<Entry> {
        if input.amount <= 0 {
            return bad("amount must be positive");
        }
        if input.from_account_id == input.to_account_id {
            return bad("choose two different accounts");
        }
        let from = self.money_account(&input.from_account_id).await?;
        let to = self.money_account(&input.to_account_id).await?;
        let memo = if input.memo.is_empty() { format!("Transfer {} → {}", from.name, to.name) } else { input.memo };
        self.post(
            &input.date,
            &memo,
            vec![
                Line { account_id: to.id, debit: input.amount, ..Default::default() },
                Line { account_id: from.id, credit: input.amount, ..Default::default() },
            ],
            "transfer",
            None,
        )
        .await
    }

    /// Records a GST/HST remittance (positive) or refund (negative): clears
    /// the payable and recoverable balances against the bank.
    pub async fn tax_payment(&self, input: TaxPaymentInput) -> LResult<Entry> {
        if input.amount == 0 {
            return bad("amount can't be zero");
        }
        let bank = self.money_account(&input.account_id).await?;
        let payable = self.account_by_role(AccountRole::TaxPayable).await?;
        let (d, c) = if input.amount > 0 { (input.amount, 0) } else { (0, -input.amount) };
        let memo = if input.memo.is_empty() { "GST/HST remittance".to_string() } else { input.memo };
        self.post(
            &input.date,
            &memo,
            vec![
                Line { account_id: payable.id, debit: d, credit: c, ..Default::default() },
                Line { account_id: bank.id, debit: c, credit: d, ..Default::default() },
            ],
            "tax_payment",
            None,
        )
        .await
    }

    pub async fn register(&self, account_id: &str) -> LResult<Vec<RegisterRow>> {
        let acct = self.account(account_id).await?;
        let contacts: HashMap<String, String> = self.contacts(None).await?.into_iter().map(|c| (c.id, c.name)).collect();
        let mut bal = 0i64;
        let mut rows = Vec::new();
        for e in self.entries().await?.into_iter().filter(|e| !e.voided) {
            for (i, l) in e.lines.iter().enumerate().filter(|(_, l)| l.account_id == acct.id) {
                bal += if acct.kind.debit_normal() { l.debit - l.credit } else { l.credit - l.debit };
                rows.push(RegisterRow {
                    entry_id: e.id.clone(),
                    line_index: i,
                    number: e.number,
                    date: e.date.clone(),
                    source: e.source.clone(),
                    memo: if l.memo.is_empty() { e.memo.clone() } else { format!("{} · {}", e.memo, l.memo) },
                    contact: l.contact_id.as_ref().and_then(|c| contacts.get(c).cloned()),
                    debit: l.debit,
                    credit: l.credit,
                    balance: bal,
                    cleared: l.cleared,
                });
            }
        }
        Ok(rows)
    }

    pub async fn reconcile(&self, input: ReconcileInput) -> LResult<Reconciliation> {
        let acct = self.money_account(&input.account_id).await?;
        parse_date(&input.statement_date)?;
        let mut entries: HashMap<String, Entry> = self.entries().await?.into_iter().map(|e| (e.id.clone(), e)).collect();
        let sign = |l: &Line| if acct.kind.debit_normal() { l.debit - l.credit } else { l.credit - l.debit };
        let mut cleared: i64 = entries
            .values()
            .filter(|e| !e.voided)
            .flat_map(|e| e.lines.iter())
            .filter(|l| l.account_id == acct.id && l.cleared)
            .map(sign)
            .sum();
        let mut to_mark: Vec<(String, usize)> = Vec::new();
        for r in &input.line_refs {
            let (eid, idx) = r
                .rsplit_once(':')
                .and_then(|(e, i)| i.parse::<usize>().ok().map(|i| (e.to_string(), i)))
                .ok_or_else(|| LedgerError::BadRequest(format!("bad line ref {r}")))?;
            let e = entries.get(&eid).ok_or_else(|| LedgerError::BadRequest(format!("unknown entry {eid}")))?;
            let l = e.lines.get(idx).ok_or_else(|| LedgerError::BadRequest(format!("bad line ref {r}")))?;
            if e.voided || l.account_id != acct.id || l.cleared || e.date > input.statement_date {
                return bad(format!("line {r} can't be cleared in this reconciliation"));
            }
            cleared += sign(l);
            to_mark.push((eid, idx));
        }
        if cleared != input.statement_balance {
            return Err(LedgerError::Conflict(format!(
                "difference of {}: cleared balance {} vs statement {}",
                fmt_money(input.statement_balance - cleared),
                fmt_money(cleared),
                fmt_money(input.statement_balance)
            )));
        }
        let n = to_mark.len();
        for (eid, idx) in to_mark {
            let e = entries.get_mut(&eid).unwrap();
            e.lines[idx].cleared = true;
            let e = e.clone();
            self.put("entries", &e.id, &e).await?;
        }
        let rec = Reconciliation {
            id: new_id("rec"),
            account_id: acct.id.clone(),
            statement_date: input.statement_date,
            statement_balance: input.statement_balance,
            cleared_count: n,
            created_at: now(),
        };
        self.put("recs", &rec.id, &rec).await?;
        self.audit("account.reconciled", format!("{} through {}", acct.name, rec.statement_date)).await?;
        Ok(rec)
    }

    pub async fn reconciliations(&self) -> LResult<Vec<Reconciliation>> {
        let mut v: Vec<Reconciliation> = self.all("recs").await?;
        v.sort_by(|a, b| b.statement_date.cmp(&a.statement_date));
        Ok(v)
    }

    // ---- bank feed ---------------------------------------------------------------

    /// Imports a bank CSV. Accepts `date,description,amount` or
    /// `date,description,debit,credit` (header row optional), with dates as
    /// YYYY-MM-DD or MM/DD/YYYY. Re-importing the same rows is a no-op.
    pub async fn import_feed(&self, account_id: &str, csv: &str) -> LResult<serde_json::Value> {
        let acct = self.money_account(account_id).await?;
        let (mut added, mut dupes, mut errors) = (0, 0, Vec::new());
        for (n, raw) in csv.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            let cols = split_csv(line);
            if n == 0 && cols.first().is_some_and(|c| c.to_lowercase().contains("date")) {
                continue;
            }
            let parsed = (|| -> Option<(String, String, i64)> {
                let date = normalize_date(cols.first()?)?;
                let desc = cols.get(1)?.trim().to_string();
                let amount = match cols.len() {
                    3 => parse_money(&cols[2])?,
                    _ => {
                        let debit = cols.get(2).and_then(|c| parse_money(c)).unwrap_or(0);
                        let credit = cols.get(3).and_then(|c| parse_money(c)).unwrap_or(0);
                        credit - debit.abs()
                    }
                };
                (amount != 0).then_some((date, desc, amount))
            })();
            let Some((date, description, amount)) = parsed else {
                errors.push(format!("row {}: couldn't read '{}'", n + 1, line.chars().take(60).collect::<String>()));
                continue;
            };
            let fp = self.key(&format!("feedfp:{}:{date}:{amount}:{}", acct.id, description.to_lowercase()));
            if !self.store.set_nx(&fp, "1").await? {
                dupes += 1;
                continue;
            }
            let item = FeedItem { id: new_id("feed"), account_id: acct.id.clone(), date, description, amount, status: "pending".into(), entry_id: None, created_at: now() };
            self.put("feed", &item.id, &item).await?;
            added += 1;
        }
        self.audit("feed.imported", format!("{added} rows into {}", acct.name)).await?;
        Ok(serde_json::json!({ "added": added, "duplicates": dupes, "errors": errors }))
    }

    pub async fn feed(&self) -> LResult<Vec<serde_json::Value>> {
        let rules = self.rules().await?;
        let mut items: Vec<FeedItem> = self.all("feed").await?;
        items.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(items
            .into_iter()
            .map(|i| {
                let desc = i.description.to_lowercase();
                let rule = rules
                    .iter()
                    .filter(|r| desc.contains(&r.keyword))
                    .max_by_key(|r| r.keyword.len());
                serde_json::json!({ "item": i, "suggestion": rule })
            })
            .collect())
    }

    pub async fn rules(&self) -> LResult<Vec<BankRule>> {
        Ok(self.store.get_json(&self.key("rules")).await?.unwrap_or_default())
    }

    /// Create or replace a bank rule. Keywords match case-insensitively against the
    /// feed description; the longest matching keyword wins.
    pub async fn save_rule(&self, input: BankRuleInput) -> LResult<BankRule> {
        let keyword = input.keyword.trim().to_lowercase();
        if keyword.is_empty() {
            return Err(LedgerError::BadRequest("Enter a keyword to match.".into()));
        }
        if keyword.chars().count() > 80 {
            return Err(LedgerError::BadRequest("Keywords are limited to 80 characters.".into()));
        }
        if keyword.chars().any(char::is_control) {
            return Err(LedgerError::BadRequest("Keywords cannot contain control characters.".into()));
        }
        let accounts = self.accounts_read().await?;
        let account = accounts
            .iter()
            .find(|a| a.id == input.account_id)
            .ok_or_else(|| LedgerError::BadRequest("Choose an existing account for the rule.".into()))?;
        if account.archived {
            return Err(LedgerError::BadRequest("That account is archived.".into()));
        }
        if matches!(account.role, AccountRole::Bank | AccountRole::CreditCard | AccountRole::Receivable | AccountRole::Payable) {
            return Err(LedgerError::BadRequest("A bank rule must categorize to a non-bank income, expense or other account.".into()));
        }
        let tax_code = input.tax_code.unwrap_or_else(|| "NONE".into());
        if !tax_codes().iter().any(|code| code.code == tax_code) {
            return Err(LedgerError::BadRequest("Choose a known tax code.".into()));
        }
        let rule = BankRule { keyword: keyword.clone(), account_id: input.account_id, tax_code, contact_id: input.contact_id };
        let mut rules = self.rules().await?;
        rules.retain(|r| r.keyword != keyword);
        rules.push(rule.clone());
        rules.sort_by(|a, b| b.keyword.len().cmp(&a.keyword.len()).then(a.keyword.cmp(&b.keyword)));
        self.store.set_json(&self.key("rules"), &rules).await?;
        self.audit("bank_rule.saved", format!("{keyword} -> {}", rule.account_id)).await?;
        Ok(rule)
    }

    pub async fn delete_rule(&self, keyword: &str) -> LResult<()> {
        let keyword = keyword.trim().to_lowercase();
        let mut rules = self.rules().await?;
        let before = rules.len();
        rules.retain(|r| r.keyword != keyword);
        if rules.len() == before {
            return Err(LedgerError::NotFound(format!("no bank rule matches '{keyword}'")));
        }
        self.store.set_json(&self.key("rules"), &rules).await?;
        self.audit("bank_rule.deleted", keyword).await?;
        Ok(())
    }

    /// A sale paid at the counter: one invoice, immediately settled by a received
    /// payment for its full total. Both halves are ordinary documents, so the
    /// customer statement and the payment register see them.
    pub async fn create_sales_receipt(&self, input: SalesReceiptInput) -> LResult<serde_json::Value> {
        let doc = self
            .create_doc(
                DocKind::Invoice,
                DocInput {
                    contact_id: input.contact_id.clone(),
                    date: input.date.clone(),
                    due_date: None,
                    number: input.number,
                    memo: input.memo.clone(),
                    lines: input.lines,
                },
            )
            .await?;
        let payment = self
            .record_payment(
                "received",
                PaymentInput {
                    contact_id: input.contact_id,
                    date: input.date,
                    account_id: input.account_id,
                    amount: doc.total,
                    applications: vec![Application { doc_id: doc.id.clone(), amount: doc.total }],
                    memo: if input.memo.is_empty() { format!("Sales receipt {}", doc.number) } else { input.memo },
                },
            )
            .await?;
        // record_payment applies the money to the stored document; return that state.
        let doc = self.doc(&doc.id).await?;
        Ok(serde_json::json!({ "doc": doc, "payment": payment }))
    }

    /// Post several journals as one unit: every entry is validated before any of
    /// them reaches the books, so a bad row cannot leave a half-entered batch.
    pub async fn post_batch(&self, input: BatchInput) -> LResult<Vec<Entry>> {
        if input.entries.is_empty() {
            return bad("add at least one entry");
        }
        if input.entries.len() > 50 {
            return bad("a batch is limited to 50 entries");
        }
        for entry in &input.entries {
            self.check_period(&entry.date).await?;
            self.clean_lines(entry.lines.clone()).await?;
        }
        let mut posted = Vec::with_capacity(input.entries.len());
        for entry in input.entries {
            posted.push(self.create_journal(entry).await?);
        }
        self.audit("batch.posted", format!("{} entries", posted.len())).await?;
        Ok(posted)
    }

    // ---- saved report views ---------------------------------------------------

    /// Report keys the browser can open; a saved view may only point at one of these.
    const REPORT_KEYS: [&'static str; 21] = [
        "profit-loss", "profit-loss-by-month", "profit-loss-comparison", "profit-loss-by-class",
        "balance-sheet", "trial-balance", "ar-aging", "ap-aging", "sales-tax", "general-ledger",
        "cash-flow", "forecast", "cash-plan", "performance", "budget-vs-actual", "item-profitability",
        "customer-balances", "vendor-balances", "payroll-remittance", "time", "payroll",
    ];

    pub async fn saved_reports(&self) -> LResult<Vec<SavedReport>> {
        let mut saved: Vec<SavedReport> = self.store.get_json(&self.key("saved_reports")).await?.unwrap_or_default();
        saved.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(saved)
    }

    pub async fn save_saved_report(&self, input: SavedReportInput) -> LResult<SavedReport> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return bad("give the saved view a name");
        }
        if !Self::REPORT_KEYS.contains(&input.report.as_str()) {
            return bad(format!("'{}' is not a report NXT can save", input.report));
        }
        parse_date(&input.from)?;
        parse_date(&input.to)?;
        let mut saved = self.saved_reports().await?;
        if saved.iter().any(|view| view.name.eq_ignore_ascii_case(&name)) {
            return bad(format!("a saved view named '{name}' already exists"));
        }
        let view = SavedReport {
            id: new_id("view"),
            name,
            report: input.report,
            from: input.from,
            to: input.to,
            account_id: input.account_id.filter(|id| !id.is_empty()),
            created_at: now(),
        };
        saved.push(view.clone());
        self.store.set_json(&self.key("saved_reports"), &saved).await?;
        self.audit("saved_report.created", format!("{} {}", view.name, view.report)).await?;
        Ok(view)
    }

    pub async fn delete_saved_report(&self, id: &str) -> LResult<()> {
        let mut saved = self.saved_reports().await?;
        let before = saved.len();
        saved.retain(|view| view.id != id);
        if saved.len() == before {
            return Err(LedgerError::NotFound(format!("no saved view {id}")));
        }
        self.store.set_json(&self.key("saved_reports"), &saved).await?;
        self.audit("saved_report.deleted", id.to_string()).await?;
        Ok(())
    }

    // ---- cash plan ------------------------------------------------------------

    pub async fn cash_plan(&self) -> LResult<CashPlan> {
        Ok(self.store.get_json(&self.key("plan")).await?.unwrap_or_default())
    }

    pub async fn save_cash_plan(&self, input: CashPlan) -> LResult<CashPlan> {
        if input.monthly_in_cents < 0 || input.monthly_out_cents < 0 {
            return bad("planned monthly amounts cannot be negative");
        }
        let plan = CashPlan {
            monthly_in_cents: input.monthly_in_cents,
            monthly_out_cents: input.monthly_out_cents,
            updated_at: now(),
        };
        self.store.set_json(&self.key("plan"), &plan).await?;
        self.audit("cash_plan.saved", format!("in {} out {}", plan.monthly_in_cents, plan.monthly_out_cents)).await?;
        Ok(plan)
    }

    // ---- classes --------------------------------------------------------------

    pub async fn classes(&self) -> LResult<Vec<Class>> {
        let mut classes: Vec<Class> = self.store.get_json(&self.key("classes")).await?.unwrap_or_default();
        classes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(classes)
    }

    pub async fn save_class(&self, input: ClassInput) -> LResult<Class> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return bad("give the class a name");
        }
        if name.chars().count() > 60 {
            return bad("class names are limited to 60 characters");
        }
        let mut classes = self.classes().await?;
        let id = input.id.filter(|id| !id.is_empty()).unwrap_or_else(|| new_id("class"));
        if classes.iter().any(|c| c.id != id && c.name.eq_ignore_ascii_case(&name)) {
            return bad(format!("a class named '{name}' already exists"));
        }
        classes.retain(|c| c.id != id);
        let class = Class { id: id.clone(), name, archived: input.archived };
        classes.push(class.clone());
        self.store.set_json(&self.key("classes"), &classes).await?;
        self.audit("class.saved", format!("{id} {}", class.name)).await?;
        Ok(class)
    }

    /// Remove a class. Refused while any posting or document line still references it,
    /// so a report can never point at a tag that no longer exists.
    pub async fn delete_class(&self, id: &str) -> LResult<()> {
        let mut classes = self.classes().await?;
        if !classes.iter().any(|c| c.id == id) {
            return Err(LedgerError::NotFound(format!("no class {id}")));
        }
        let used = self.all::<Entry>("entries").await?.iter()
            .any(|entry| entry.lines.iter().any(|line| line.class_id.as_deref() == Some(id)))
            || self.all::<Doc>("docs").await?.iter()
                .any(|doc| doc.lines.iter().any(|line| line.class_id.as_deref() == Some(id)));
        if used {
            return Err(LedgerError::Conflict(format!("class {id} is used by postings; archive it instead")));
        }
        classes.retain(|c| c.id != id);
        self.store.set_json(&self.key("classes"), &classes).await?;
        self.audit("class.deleted", id.to_string()).await?;
        Ok(())
    }

    async fn known_class(&self, class_id: &str) -> LResult<bool> {
        Ok(self.classes().await?.iter().any(|c| c.id == class_id && !c.archived))
    }

    // ---- budgets --------------------------------------------------------------

    pub async fn budgets(&self) -> LResult<Vec<BudgetCell>> {
        let mut cells: Vec<BudgetCell> = self.store.get_json(&self.key("budgets")).await?.unwrap_or_default();
        cells.sort_by(|a, b| a.month.cmp(&b.month).then(a.account_id.cmp(&b.account_id)));
        Ok(cells)
    }

    pub async fn set_budget(&self, input: BudgetInput) -> LResult<BudgetCell> {
        let month = input.month.trim().to_string();
        NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
            .map_err(|_| LedgerError::BadRequest("month must be YYYY-MM".into()))?;
        if input.amount < 0 {
            return bad("budget amounts cannot be negative");
        }
        let account = match self.account(&input.account_id).await {
            Ok(account) => account,
            Err(LedgerError::NotFound(_)) => return bad("choose an existing account for the budget"),
            Err(error) => return Err(error),
        };
        if account.archived {
            return bad(format!("account {} {} is archived", account.code, account.name));
        }
        if account.is_money() {
            return bad("set budgets on income or expense accounts, not bank or credit-card accounts");
        }
        let cell = BudgetCell { account_id: account.id, month: month.clone(), amount: input.amount };
        let mut cells = self.budgets().await?;
        cells.retain(|c| !(c.account_id == cell.account_id && c.month == month));
        cells.push(cell.clone());
        self.store.set_json(&self.key("budgets"), &cells).await?;
        self.audit("budget.saved", format!("{} {}", cell.account_id, month)).await?;
        Ok(cell)
    }

    pub async fn delete_budget(&self, account_id: &str, month: &str) -> LResult<()> {
        let mut cells = self.budgets().await?;
        let before = cells.len();
        cells.retain(|c| !(c.account_id == account_id && c.month == month));
        if cells.len() == before {
            return Err(LedgerError::NotFound(format!("no budget for {account_id} in {month}")));
        }
        self.store.set_json(&self.key("budgets"), &cells).await?;
        self.audit("budget.deleted", format!("{account_id} {month}")).await?;
        Ok(())
    }

    // ---- recurring / memorized entries -----------------------------------------

    pub async fn recurring(&self) -> LResult<Vec<RecurringTemplate>> {
        let mut list: Vec<RecurringTemplate> = self.store.get_json(&self.key("recurring")).await?.unwrap_or_default();
        list.sort_by(|a, b| a.next_date.cmp(&b.next_date).then(a.name.cmp(&b.name)));
        Ok(list)
    }

    /// Create or replace a memorized template. Lines are validated as if they were
    /// about to be posted, so an unbalanced template cannot be saved.
    pub async fn save_recurring(&self, input: RecurringInput) -> LResult<RecurringTemplate> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return bad("give the template a name");
        }
        if name.chars().count() > 120 {
            return bad("template names are limited to 120 characters");
        }
        if !["weekly", "monthly", "quarterly", "yearly"].contains(&input.frequency.as_str()) {
            return bad("frequency must be weekly, monthly, quarterly or yearly");
        }
        parse_date(&input.next_date)?;
        if input.next_date.as_str() < today().as_str() {
            return bad("the next date cannot be in the past");
        }
        let lines = self.clean_lines(input.lines).await?;
        let mut list = self.recurring().await?;
        let id = input.id.filter(|id| !id.is_empty()).unwrap_or_else(|| new_id("rec"));
        let (created_at, last_posted) = list
            .iter()
            .find(|t| t.id == id)
            .map(|t| (t.created_at.clone(), t.last_posted.clone()))
            .unwrap_or_else(|| (now(), None));
        let template = RecurringTemplate {
            id: id.clone(),
            name,
            frequency: input.frequency,
            next_date: input.next_date,
            memo: input.memo.trim().to_string(),
            lines,
            active: true,
            created_at,
            last_posted,
        };
        list.retain(|t| t.id != id);
        list.push(template.clone());
        list.sort_by(|a, b| a.next_date.cmp(&b.next_date).then(a.name.cmp(&b.name)));
        self.store.set_json(&self.key("recurring"), &list).await?;
        self.audit("recurring.saved", format!("{} {}", id, template.name)).await?;
        Ok(template)
    }

    pub async fn delete_recurring(&self, id: &str) -> LResult<()> {
        let mut list = self.recurring().await?;
        let before = list.len();
        list.retain(|t| t.id != id);
        if list.len() == before {
            return Err(LedgerError::NotFound(format!("no recurring template {id}")));
        }
        self.store.set_json(&self.key("recurring"), &list).await?;
        self.audit("recurring.deleted", id.to_string()).await?;
        Ok(())
    }

    /// Post the template at its next date and advance the schedule by one period.
    /// Nothing is posted automatically: this is the only way a template reaches the books.
    pub async fn post_recurring(&self, id: &str) -> LResult<Entry> {
        let mut list = self.recurring().await?;
        let idx = list
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| LedgerError::NotFound(format!("no recurring template {id}")))?;
        let date = list[idx].next_date.clone();
        let memo = if list[idx].memo.is_empty() { list[idx].name.clone() } else { list[idx].memo.clone() };
        let entry = self.create_journal(JournalInput { date: date.clone(), memo, lines: list[idx].lines.clone() }).await?;
        let next = advance_date(&date, &list[idx].frequency)?;
        list[idx].next_date = next;
        list[idx].last_posted = Some(date);
        self.store.set_json(&self.key("recurring"), &list).await?;
        Ok(entry)
    }

    pub async fn categorize(&self, id: &str, input: CategorizeInput) -> LResult<FeedItem> {
        let mut item: FeedItem = self.must("feed", id).await?;
        if item.status != "pending" {
            return Err(LedgerError::Conflict("this transaction was already handled".into()));
        }
        let entry = self
            .money(
                if item.amount < 0 { "out" } else { "in" },
                MoneyInput {
                    account_id: item.account_id.clone(),
                    date: item.date.clone(),
                    contact_id: input.contact_id.clone(),
                    memo: item.description.clone(),
                    lines: vec![MoneyLineInput {
                        account_id: input.account_id.clone(),
                        amount: item.amount.abs(),
                        tax_code: Some(input.tax_code.clone().unwrap_or("NONE".into())),
                        description: item.description.clone(),
                    }],
                    amounts_include_tax: true,
                },
            )
            .await?;
        item.status = "added".into();
        item.entry_id = Some(entry.id);
        self.put("feed", &item.id, &item).await?;
        if input.create_rule {
            let keyword = rule_keyword(&item.description);
            if !keyword.is_empty() {
                let mut rules = self.rules().await?;
                rules.retain(|r| r.keyword != keyword);
                rules.push(BankRule { keyword, account_id: input.account_id, tax_code: input.tax_code.unwrap_or("NONE".into()), contact_id: input.contact_id });
                self.store.set_json(&self.key("rules"), &rules).await?;
            }
        }
        Ok(item)
    }

    pub async fn exclude_feed(&self, id: &str) -> LResult<FeedItem> {
        let mut item: FeedItem = self.must("feed", id).await?;
        if item.status != "pending" {
            return Err(LedgerError::Conflict("this transaction was already handled".into()));
        }
        item.status = "excluded".into();
        self.put("feed", &item.id, &item).await?;
        Ok(item)
    }

    pub fn fiscal_year_start(company: &Company, as_of: NaiveDate) -> NaiveDate {
        let m = company.fiscal_year_start_month;
        let y = if as_of.month() >= m { as_of.year() } else { as_of.year() - 1 };
        NaiveDate::from_ymd_opt(y, m, 1).unwrap()
    }
}

pub fn fmt_money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let c = cents.abs();
    let dollars = (c / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in dollars.chars().enumerate() {
        if i > 0 && (dollars.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}${grouped}.{:02}", c % 100)
}

fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

fn normalize_date(s: &str) -> Option<String> {
    let s = s.trim();
    ["%Y-%m-%d", "%m/%d/%Y", "%Y/%m/%d", "%d-%b-%Y", "%b %d, %Y"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(s, f).ok())
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Parses "1,234.56", "$-12.00", "(45.00)" into cents.
pub fn parse_money(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let neg = t.starts_with('(') && t.ends_with(')') || t.contains('-');
    let cleaned: String = t.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let v: f64 = cleaned.parse().ok()?;
    let cents = round_cents(v * 100.0);
    Some(if neg { -cents } else { cents })
}

/// First meaningful word of a bank description, used as a rule keyword.
fn rule_keyword(desc: &str) -> String {
    desc.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| w.len() >= 3 && !w.chars().all(|c| c.is_ascii_digit()) && !["pos", "purchase", "debit", "credit", "payment", "the"].contains(w))
        .unwrap_or("")
        .to_string()
}
