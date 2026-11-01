//! Parallel lists that do not sync to QuickBooks and do not run payroll.
use super::{bad, date, err, storage, text, valid_id, CompanyBooks, Result};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DailyState {
    #[serde(default)] departments: Vec<Named>,
    #[serde(default)] items: Vec<Item>,
    #[serde(default)] employees: Vec<Employee>,
    #[serde(default)] estimates: Vec<NonPosting>,
    #[serde(default)] purchase_orders: Vec<NonPosting>,
    #[serde(default)] time: Vec<TimeActivity>,
    #[serde(default)] budgets: Vec<Budget>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Named { id: String, name: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Item { id: String, name: String, income_account: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Employee { id: String, name: String, active: bool }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct NonPostingLine { item_id: String, amount_cents: i64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct NonPosting { id: String, contact: String, document_number: String, lines: Vec<NonPostingLine>, total_cents: i64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TimeActivity { id: String, employee_id: String, date: String, minutes: i64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Budget { id: String, account_id: String, period: String, amount_cents: i64 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedInput { pub request_id: String, pub name: String, #[serde(default = "yes")] pub active: bool, pub income_account: Option<String> }
fn yes() -> bool { true }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonPostingLineInput { pub item_id: String, pub amount_cents: i64 }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonPostingInput {
    pub request_id: String,
    pub contact: String,
    #[serde(default)] pub document_number: String,
    pub lines: Vec<NonPostingLineInput>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeInput { pub request_id: String, pub employee_id: String, pub date: String, pub minutes: i64 }
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetInput { pub request_id: String, pub account_id: String, pub period: String, pub amount_cents: i64 }

fn key(company: &str) -> String { format!("company-books:{company}:parallel-daily:v1") }
fn period(value: &str) -> bool {
    let Some((year, month)) = value.split_once('-') else { return false };
    year.len() == 4 && month.len() == 2 && date(&format!("{value}-01"))
}

impl CompanyBooks {
    async fn daily_load(&self, company: &str) -> Result<(Option<String>, DailyState)> {
        let raw = self.store.get(&key(company)).await.map_err(|_| storage(String::new()))?;
        let state = match &raw {
            None => DailyState::default(),
            Some(text) => serde_json::from_str(text).map_err(|_| super::corrupt())?,
        };
        Ok((raw, state))
    }
    async fn daily_save(&self, company: &str, expected: Option<&str>, state: &DailyState) -> Result<()> {
        let next = serde_json::to_string(state).map_err(|_| bad("Parallel lists cannot be encoded"))?;
        if next.len() > 1024 * 1024 { return Err(bad("Parallel lists exceed 1 MiB")); }
        if self.store.compare_exchange(&key(company), expected, &next).await.map_err(|_| storage(String::new()))? { return Ok(()); }
        Err(err(StatusCode::CONFLICT, "Parallel lists changed; retry"))
    }
    pub async fn add_department(&self, company: &str, input: NamedInput) -> Result<Value> {
        self.add_named(company, "department", input).await
    }
    pub async fn add_item(&self, company: &str, input: NamedInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !text(&input.name, 80) || input.name.trim().is_empty() { return Err(bad("Item name and request id are required")); }
        let Some(account) = input.income_account.as_deref().filter(|v| valid_id(v)) else { return Err(bad("Item income account is required")); };
        let (raw, mut state) = self.daily_load(company).await?;
        if state.items.iter().any(|row| row.id == input.request_id) { return Ok(json!({"id": input.request_id, "idempotent_replay": true, "write_back": false})); }
        if state.items.len() >= 200 { return Err(bad("Item list is full")); }
        state.items.push(Item { id: input.request_id.clone(), name: input.name.trim().into(), income_account: account.into() });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn add_employee(&self, company: &str, input: NamedInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !text(&input.name, 80) || input.name.trim().is_empty() { return Err(bad("Employee name and request id are required")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.employees.iter().any(|row| row.id == input.request_id) { return Ok(json!({"id": input.request_id, "idempotent_replay": true, "payroll_enabled": false, "write_back": false})); }
        if state.employees.len() >= 200 { return Err(bad("Employee list is full")); }
        state.employees.push(Employee { id: input.request_id.clone(), name: input.name.trim().into(), active: input.active });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "payroll_enabled": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    async fn add_named(&self, company: &str, kind: &str, input: NamedInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !text(&input.name, 80) || input.name.trim().is_empty() { return Err(bad("Name and request id are required")); }
        let (raw, mut state) = self.daily_load(company).await?;
        let list = match kind { "department" => &mut state.departments, _ => return Err(bad("Unknown list")) };
        if list.iter().any(|row| row.id == input.request_id) { return Ok(json!({"id": input.request_id, "idempotent_replay": true, "write_back": false})); }
        if list.len() >= 200 { return Err(bad("List is full")); }
        list.push(Named { id: input.request_id.clone(), name: input.name.trim().into() });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "kind": kind, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn add_estimate(&self, company: &str, input: NonPostingInput) -> Result<Value> { self.add_nonposting(company, "estimate", input).await }
    pub async fn add_purchase_order(&self, company: &str, input: NonPostingInput) -> Result<Value> { self.add_nonposting(company, "purchase_order", input).await }
    async fn add_nonposting(&self, company: &str, kind: &str, input: NonPostingInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !text(&input.contact, 120) || input.contact.trim().is_empty() { return Err(bad("Contact and request id are required")); }
        if !input.document_number.is_empty() && (!text(&input.document_number, 40) || input.document_number.contains('/')) { return Err(bad("Document number is invalid")); }
        if input.lines.is_empty() || input.lines.len() > 40 { return Err(bad("A non-posting document needs 1–40 lines")); }
        let (raw, mut state) = self.daily_load(company).await?;
        let list = if kind == "estimate" { &state.estimates } else { &state.purchase_orders };
        if list.iter().any(|row| row.id == input.request_id) { return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_to_ledger": false, "write_back": false})); }
        let mut total = 0_i64;
        let mut lines = Vec::new();
        for line in &input.lines {
            if line.amount_cents <= 0 || !state.items.iter().any(|item| item.id == line.item_id) { return Err(bad("Non-posting lines need a known item and a positive amount")); }
            total = total.checked_add(line.amount_cents).ok_or_else(|| bad("Amount overflow"))?;
            lines.push(NonPostingLine { item_id: line.item_id.clone(), amount_cents: line.amount_cents });
        }
        let doc = NonPosting { id: input.request_id.clone(), contact: input.contact.trim().into(), document_number: input.document_number.trim().into(), lines, total_cents: total };
        if kind == "estimate" { if state.estimates.len() >= 200 { return Err(bad("Estimate list is full")); } state.estimates.push(doc); }
        else { if state.purchase_orders.len() >= 200 { return Err(bad("Purchase-order list is full")); } state.purchase_orders.push(doc); }
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "kind": kind, "total_cents": total, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn add_time(&self, company: &str, input: TimeInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !date(&input.date) || !(1..=24 * 60).contains(&input.minutes) { return Err(bad("Time activity needs a date and 1–1440 minutes")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if !state.employees.iter().any(|row| row.id == input.employee_id && row.active) { return Err(bad("Time needs an active parallel employee")); }
        if state.time.iter().any(|row| row.id == input.request_id) { return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_payroll": false, "write_back": false})); }
        if state.time.len() >= 400 { return Err(bad("Time list is full")); }
        state.time.push(TimeActivity { id: input.request_id.clone(), employee_id: input.employee_id, date: input.date, minutes: input.minutes });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "posts_payroll": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn add_budget(&self, company: &str, input: BudgetInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !valid_id(&input.account_id) || !period(&input.period) { return Err(bad("Budget needs an account and YYYY-MM period")); }
        if input.amount_cents < 0 { return Err(bad("Budget amount must be zero or positive cents")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.budgets.iter().any(|row| row.id == input.request_id) { return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_to_ledger": false, "write_back": false})); }
        if state.budgets.len() >= 400 { return Err(bad("Budget list is full")); }
        state.budgets.push(Budget { id: input.request_id.clone(), account_id: input.account_id, period: input.period, amount_cents: input.amount_cents });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }
    pub async fn payroll_run(&self, _company: &str) -> Result<Value> {
        Err(err(StatusCode::CONFLICT, "Payroll calculation, remittance, and T4s are not enabled. Employee names stay inside NXT and nothing is filed or written back."))
    }
    pub async fn daily(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        Ok(json!({
            "departments": state.departments, "items": state.items, "employees": state.employees,
            "estimates": state.estimates, "purchase_orders": state.purchase_orders,
            "time": state.time, "budgets": state.budgets,
            "payroll_enabled": false, "multi_currency": false, "posts_to_ledger": false,
            "write_back": false, "authoritative_system": "quickbooks"
        }))
    }
}
