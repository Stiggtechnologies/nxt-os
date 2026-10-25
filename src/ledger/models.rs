//! NXT Ledger data model. All money is integer cents (CAD by default).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Asset,
    Liability,
    Equity,
    Income,
    Expense,
}

impl AccountKind {
    /// Assets and expenses carry debit balances; the rest carry credits.
    pub fn debit_normal(self) -> bool {
        matches!(self, AccountKind::Asset | AccountKind::Expense)
    }
}

/// System roles some accounts play in automatic postings.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountRole {
    #[default]
    None,
    Bank,
    CreditCard,
    Receivable,
    Payable,
    TaxPayable,
    TaxRecoverable,
    RetainedEarnings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
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

impl Account {
    pub fn is_money(&self) -> bool {
        matches!(self.role, AccountRole::Bank | AccountRole::CreditCard)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxCode {
    pub code: String,
    pub name: String,
    /// Rate in basis points: 1300 = 13.00%.
    pub rate_bp: i64,
}

pub fn tax_codes() -> Vec<TaxCode> {
    [
        ("NONE", "No tax / out of scope", 0),
        ("EXEMPT", "Exempt / zero-rated", 0),
        ("GST", "GST 5% (AB, BC, MB, QC, SK, territories)", 500),
        ("HST13", "HST 13% (ON)", 1300),
        ("HST14", "HST 14% (NS)", 1400),
        ("HST15", "HST 15% (NB, NL, PE)", 1500),
    ]
    .into_iter()
    .map(|(c, n, r)| TaxCode { code: c.into(), name: n.into(), rate_bp: r })
    .collect()
}

pub fn tax_rate(code: &str) -> Option<i64> {
    tax_codes().into_iter().find(|t| t.code == code).map(|t| t.rate_bp)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Line {
    pub account_id: String,
    #[serde(default)]
    pub debit: i64,
    #[serde(default)]
    pub credit: i64,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub contact_id: Option<String>,
    #[serde(default)]
    pub cleared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub number: i64,
    /// ISO date, YYYY-MM-DD.
    pub date: String,
    pub memo: String,
    pub lines: Vec<Line>,
    /// journal, invoice, bill, payment_received, payment_made, money_in,
    /// money_out, transfer.
    pub source: String,
    pub source_id: Option<String>,
    #[serde(default)]
    pub voided: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContactKind {
    Customer,
    Vendor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub kind: ContactKind,
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub address: String,
    #[serde(default = "default_terms")]
    pub terms_days: i64,
    #[serde(default)]
    pub default_tax_code: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub created_at: String,
}

fn default_terms() -> i64 {
    30
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Invoice,
    Bill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocLine {
    pub description: String,
    pub qty: f64,
    /// Unit price in cents.
    pub rate: i64,
    pub account_id: String,
    #[serde(default = "default_tax")]
    pub tax_code: String,
    /// Computed: round(qty * rate).
    #[serde(default)]
    pub amount: i64,
    /// Computed: tax on `amount`.
    #[serde(default)]
    pub tax: i64,
}

fn default_tax() -> String {
    "NONE".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub id: String,
    pub kind: DocKind,
    pub number: String,
    pub contact_id: String,
    pub date: String,
    pub due_date: String,
    pub lines: Vec<DocLine>,
    pub memo: String,
    pub subtotal: i64,
    pub tax: i64,
    pub total: i64,
    pub paid: i64,
    pub voided: bool,
    pub entry_id: String,
    pub created_at: String,
}

impl Doc {
    pub fn balance(&self) -> i64 {
        if self.voided { 0 } else { self.total - self.paid }
    }

    pub fn status(&self, today: &str) -> &'static str {
        if self.voided {
            "void"
        } else if self.balance() <= 0 {
            "paid"
        } else if self.due_date.as_str() < today {
            "overdue"
        } else if self.paid > 0 {
            "partial"
        } else {
            "open"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub doc_id: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub id: String,
    /// "received" (from a customer) or "made" (to a vendor).
    pub direction: String,
    pub contact_id: String,
    pub date: String,
    /// Bank or credit card account the money moved through.
    pub account_id: String,
    pub amount: i64,
    pub applications: Vec<Application>,
    pub memo: String,
    pub entry_id: String,
    #[serde(default)]
    pub voided: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub id: String,
    pub account_id: String,
    pub date: String,
    pub description: String,
    /// Positive = money in, negative = money out.
    pub amount: i64,
    /// pending, added, excluded
    pub status: String,
    pub entry_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reconciliation {
    pub id: String,
    pub account_id: String,
    pub statement_date: String,
    pub statement_balance: i64,
    pub cleared_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Company {
    pub name: String,
    pub legal_name: String,
    pub business_number: String,
    pub province: String,
    pub currency: String,
    /// 1-12; the month the fiscal year starts.
    pub fiscal_year_start_month: u32,
    /// Books are locked on and before this date (YYYY-MM-DD), if set.
    pub closing_date: Option<String>,
    pub default_tax_code: String,
    pub invoice_prefix: String,
    pub invoice_footer: String,
}

impl Default for Company {
    fn default() -> Self {
        Company {
            name: "NXT Corporation".into(),
            legal_name: String::new(),
            business_number: String::new(),
            province: "ON".into(),
            currency: "CAD".into(),
            fiscal_year_start_month: 1,
            closing_date: None,
            default_tax_code: "HST13".into(),
            invoice_prefix: "INV-".into(),
            invoice_footer: "Thank you for your business.".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankRule {
    /// Lowercased keyword matched against feed descriptions.
    pub keyword: String,
    pub account_id: String,
    pub tax_code: String,
    pub contact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub at: String,
    pub action: String,
    pub detail: String,
}

/// Default Canadian small-business chart of accounts.
pub fn default_chart() -> Vec<Account> {
    use AccountKind::*;
    use AccountRole as R;
    [
        ("1000", "Chequing", Asset, R::Bank),
        ("1010", "Savings", Asset, R::Bank),
        ("1100", "Accounts Receivable", Asset, R::Receivable),
        ("1200", "Inventory", Asset, R::None),
        ("1300", "Prepaid Expenses", Asset, R::None),
        ("1350", "GST/HST Recoverable (ITCs)", Asset, R::TaxRecoverable),
        ("1500", "Equipment", Asset, R::None),
        ("1510", "Accumulated Depreciation", Asset, R::None),
        ("2000", "Accounts Payable", Liability, R::Payable),
        ("2100", "GST/HST Payable", Liability, R::TaxPayable),
        ("2200", "Corporate Credit Card", Liability, R::CreditCard),
        ("2300", "Payroll Liabilities", Liability, R::None),
        ("2500", "Loan Payable", Liability, R::None),
        ("3000", "Share Capital", Equity, R::None),
        ("3100", "Owner Draws / Dividends", Equity, R::None),
        ("3900", "Retained Earnings", Equity, R::RetainedEarnings),
        ("4000", "Sales", Income, R::None),
        ("4100", "Service Revenue", Income, R::None),
        ("4900", "Other Income", Income, R::None),
        ("5000", "Cost of Goods Sold", Expense, R::None),
        ("6000", "Advertising & Marketing", Expense, R::None),
        ("6100", "Bank Charges & Fees", Expense, R::None),
        ("6200", "Meals & Entertainment", Expense, R::None),
        ("6300", "Office Supplies", Expense, R::None),
        ("6400", "Professional Fees", Expense, R::None),
        ("6500", "Rent", Expense, R::None),
        ("6600", "Software & Subscriptions", Expense, R::None),
        ("6700", "Telephone & Internet", Expense, R::None),
        ("6800", "Travel", Expense, R::None),
        ("6900", "Utilities", Expense, R::None),
        ("7000", "Wages & Salaries", Expense, R::None),
        ("7100", "Insurance", Expense, R::None),
        ("7900", "Depreciation", Expense, R::None),
    ]
    .into_iter()
    .map(|(code, name, kind, role)| Account {
        id: format!("acc_{code}"),
        code: code.into(),
        name: name.into(),
        kind,
        role,
        description: String::new(),
        archived: false,
    })
    .collect()
}
