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
    #[serde(default)] paystubs: Vec<Paystub>,
    #[serde(default)] recurring: Vec<Recurring>,
    #[serde(default)] schedules: Vec<super::schedules::StoredSchedule>,
    #[serde(default)] stock_movements: Vec<super::inventory::Movement>,
    #[serde(default)] crm: Vec<CrmRecord>,
    #[serde(default)] mileage: Vec<MileageTrip>,
    #[serde(default)] sales_docs: Vec<SalesDoc>,
}

/// A mileage trip. The rate is supplied by the firm; NXT does not claim a statutory rate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MileageTrip {
    id: String,
    date: String,
    purpose: String,
    kilometres_milli: i64,
    rate_cents_per_km: i64,
    amount_cents: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MileageInput {
    pub request_id: String,
    pub date: String,
    pub purpose: String,
    pub kilometres_milli: i64,
    pub rate_cents_per_km: i64,
}

/// A non-posting sales document: sales order, sales receipt, refund receipt, delayed charge
/// or delayed credit. One shape, distinguished by kind; nothing books or sends it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SalesDoc {
    id: String,
    kind: String,
    contact: String,
    document_number: String,
    lines: Vec<NonPostingLineInput>,
    total_cents: i64,
}

/// A pipeline record. Opportunities, proposals and contracts share one shape because they
/// differ only by kind and status; nothing here sends, signs or books anything.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrmRecord {
    id: String,
    kind: String,
    title: String,
    contact: String,
    value_cents: i64,
    status: String,
    date: String,
    note: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrmInput {
    pub request_id: String,
    pub kind: String,
    pub title: String,
    pub contact: String,
    #[serde(default)] pub value_cents: i64,
    pub status: String,
    pub date: String,
    #[serde(default)] pub note: String,
}

/// A recurring transaction template. NXT stores it and shows when it is next due; there is
/// no scheduler: a template never posts on its own.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recurring {
    id: String,
    kind: String,
    contact: String,
    document_number: String,
    amount_cents: i64,
    cadence: String,
    next_date: String,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecurringInput {
    pub request_id: String,
    pub kind: String,
    pub contact: String,
    #[serde(default)] pub document_number: String,
    pub amount_cents: i64,
    pub cadence: String,
    pub next_date: String,
    #[serde(default)] pub active: bool,
}

/// One stored paystub. Payroll runs are recorded inside NXT; nothing is remitted or filed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Paystub {
    id: String,
    employee_id: String,
    employee_name: String,
    pay_date: String,
    periods: u32,
    gross_cents: i64,
    cpp_cents: i64,
    cpp2_cents: i64,
    ei_cents: i64,
    federal_tax_cents: i64,
    provincial_tax_cents: i64,
    net_cents: i64,
    pensionable_cents: i64,
    insurable_cents: i64,
}

/// A pay run for one employee. Amounts are the employee's annual figures; the response and
/// the stored stub carry the per-period split.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayRunInput {
    pub request_id: String,
    pub employee_id: String,
    pub pay_date: String,
    pub annual_gross_cents: i64,
    #[serde(default)] pub periods: Option<u32>,
    pub province: String,
    #[serde(default)] pub federal_claim_cents: i64,
    #[serde(default)] pub provincial_claim_cents: i64,
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    /// Stores a fixed-asset or prepaid schedule. The schedule is a record: nothing posts.
    /// Records a mileage trip. The deduction is kilometres times the supplied rate.
    pub async fn add_mileage(&self, company: &str, input: MileageInput) -> Result<Value> {
        if !valid_id(&input.request_id) { return Err(bad("A mileage trip needs a request id")); }
        if !date(&input.date) { return Err(bad("A mileage trip needs a valid date")); }
        if input.kilometres_milli <= 0 { return Err(bad("A mileage trip needs a positive distance")); }
        if input.rate_cents_per_km < 0 { return Err(bad("A mileage rate cannot be negative")); }
        if !text(&input.purpose, 200) { return Err(bad("Mileage purpose is too long")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.mileage.iter().any(|row| row.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_to_ledger": false, "write_back": false}));
        }
        if state.mileage.len() >= 2000 { return Err(bad("Mileage history is full")); }
        let raw_amount = (input.kilometres_milli as i128) * (input.rate_cents_per_km as i128);
        let amount = ((raw_amount + 500) / 1000) as i64;
        state.mileage.push(MileageTrip {
            id: input.request_id.clone(),
            date: input.date,
            purpose: input.purpose.trim().into(),
            kilometres_milli: input.kilometres_milli,
            rate_cents_per_km: input.rate_cents_per_km,
            amount_cents: amount,
        });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "amount_cents": amount, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Mileage trips with a total.
    pub async fn mileage_list(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        let mut rows = state.mileage.clone();
        rows.sort_by(|a, b| b.date.cmp(&a.date));
        let total: i64 = rows.iter().map(|row| row.amount_cents).sum();
        let kilometres: i64 = rows.iter().map(|row| row.kilometres_milli).sum();
        Ok(json!({"trips": rows, "total_amount_cents": total, "total_kilometres_milli": kilometres, "rate_supplied_by_firm": true, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Records a non-posting sales document.
    pub async fn add_sales_doc(&self, company: &str, kind: &str, input: NonPostingInput) -> Result<Value> {
        if !matches!(kind, "sales_order" | "sales_receipt" | "refund_receipt" | "delayed_charge" | "delayed_credit") {
            return Err(bad("Unknown sales document kind"));
        }
        if !valid_id(&input.request_id) { return Err(bad("A sales document needs a request id")); }
        if input.contact.trim().is_empty() || !text(&input.contact, 120) { return Err(bad("A sales document needs a contact")); }
        if input.lines.is_empty() || input.lines.len() > 40 { return Err(bad("A sales document needs 1–40 lines")); }
        let number = input.document_number.trim();
        if !number.is_empty() && (!text(number, 40) || number.contains('/') || number.contains('\\')) {
            return Err(bad("Document number is invalid"));
        }
        let mut total = 0_i64;
        for line in &input.lines {
            if line.amount_cents <= 0 { return Err(bad("Sales document line amounts must be positive cents")); }
            total = total.checked_add(line.amount_cents).ok_or_else(|| bad("Amount overflow"))?;
        }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.sales_docs.iter().any(|doc| doc.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_to_ledger": false, "write_back": false}));
        }
        if state.sales_docs.len() >= 500 { return Err(bad("Sales document list is full")); }
        state.sales_docs.push(SalesDoc {
            id: input.request_id.clone(),
            kind: kind.into(),
            contact: input.contact.trim().into(),
            document_number: number.into(),
            lines: input.lines,
            total_cents: total,
        });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "kind": kind, "total_cents": total, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Every non-posting sales document, newest kind order preserved.
    pub async fn sales_docs_list(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        Ok(json!({"documents": state.sales_docs, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Records a pipeline entry. Nothing is sent, signed or posted.
    pub async fn add_crm(&self, company: &str, input: CrmInput) -> Result<Value> {
        if !valid_id(&input.request_id) { return Err(bad("A pipeline record needs a request id")); }
        if !matches!(input.kind.as_str(), "opportunity" | "proposal" | "contract" | "appointment" | "review") {
            return Err(bad("A pipeline record is an opportunity, a proposal, a contract, an appointment or a review"));
        }
        if input.title.trim().is_empty() || !text(&input.title, 120) { return Err(bad("A pipeline record needs a title")); }
        if !matches!(input.status.as_str(), "open" | "won" | "lost" | "sent" | "accepted" | "declined" | "draft" | "active" | "ended" | "scheduled" | "held" | "cancelled" | "received") {
            return Err(bad("That status is not one the pipeline understands"));
        }
        if !date(&input.date) { return Err(bad("A pipeline record needs a valid date")); }
        if input.value_cents < 0 { return Err(bad("A pipeline value cannot be negative")); }
        if !text(&input.note, 500) { return Err(bad("Pipeline note is too long")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.crm.iter().any(|row| row.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "sent": false, "signed": false, "write_back": false}));
        }
        if state.crm.len() >= 500 { return Err(bad("Pipeline list is full")); }
        state.crm.push(CrmRecord {
            id: input.request_id.clone(),
            kind: input.kind,
            title: input.title.trim().into(),
            contact: input.contact.trim().into(),
            value_cents: input.value_cents,
            status: input.status,
            date: input.date,
            note: input.note,
        });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "sent": false, "signed": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Pipeline records, newest first, with totals per kind.
    pub async fn crm_list(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        let mut rows = state.crm.clone();
        rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));
        let totals: Vec<Value> = ["opportunity", "proposal", "contract", "appointment", "review"].iter().map(|kind| {
            let of: Vec<&CrmRecord> = rows.iter().filter(|row| row.kind == *kind).collect();
            let open_value: i64 = of.iter().filter(|row| matches!(row.status.as_str(), "open" | "sent" | "draft" | "active")).map(|row| row.value_cents).sum();
            json!({"kind": kind, "count": of.len(), "open_value_cents": open_value})
        }).collect();
        Ok(json!({"records": rows, "totals": totals, "sent": false, "signed": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Records a stock movement against an existing item. Quantities are milli-units.
    pub async fn add_movement(&self, company: &str, input: super::inventory::MovementInput) -> Result<Value> {
        use super::inventory::MovementError;
        if !valid_id(&input.request_id) { return Err(bad("A stock movement needs a request id")); }
        let signed = super::inventory::validate(&input).map_err(|error: MovementError| bad(error.message()))?;
        if !text(&input.memo, 300) { return Err(bad("Movement memo is too long")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.stock_movements.iter().any(|row| row.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_to_ledger": false, "write_back": false}));
        }
        if !state.items.iter().any(|item| item.id == input.item_id) {
            return Err(bad("A stock movement needs an existing item"));
        }
        if state.stock_movements.len() >= 2000 { return Err(bad("Stock movement history is full")); }
        // An issue cannot take stock below zero.
        let current: i64 = state.stock_movements.iter().filter(|row| row.item_id == input.item_id)
            .map(|row| super::inventory::signed_quantity(&row.kind, row.quantity_milli).unwrap_or(0)).sum();
        if current + signed < 0 {
            return Err(bad("That issue would take the item below zero on hand"));
        }
        state.stock_movements.push(super::inventory::Movement {
            id: input.request_id.clone(),
            item_id: input.item_id.clone(),
            kind: input.kind.clone(),
            quantity_milli: input.quantity_milli,
            unit_cost_cents: input.unit_cost_cents,
            date: input.date.clone(),
            memo: input.memo.clone(),
        });
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "item_id": input.item_id, "on_hand_delta_milli": signed, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Items with their on-hand quantity, latest unit cost and value.
    pub async fn inventory_list(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        let rows = super::inventory::stock(&state.stock_movements);
        let items: Vec<Value> = state.items.iter().map(|item| {
            let stock = rows.iter().find(|row| row.item_id == item.id);
            json!({
                "item_id": item.id,
                "name": item.name,
                "on_hand_milli": stock.map(|row| row.on_hand_milli).unwrap_or(0),
                "unit_cost_cents": stock.map(|row| row.unit_cost_cents).unwrap_or(0),
                "value_cents": stock.map(|row| row.value_cents).unwrap_or(0),
                "movements": stock.map(|row| row.movements).unwrap_or(0)
            })
        }).collect();
        Ok(json!({"items": items, "movements": state.stock_movements.len(), "cost_basis": "latest_unit_cost", "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    pub async fn add_schedule(&self, company: &str, input: super::schedules::ScheduleInput) -> Result<Value> {
        use super::schedules::ScheduleError;
        if !valid_id(&input.request_id) { return Err(bad("A schedule needs a request id")); }
        if !matches!(input.kind.as_str(), "fixed_asset" | "prepaid" | "revenue_recognition") { return Err(bad("Schedule kind must be fixed_asset, prepaid or revenue_recognition")); }
        if input.name.trim().is_empty() || !text(&input.name, 120) { return Err(bad("A schedule needs a name")); }
        if input.total_cents <= 0 { return Err(bad("A schedule total must be positive")); }
        if !(1..=600).contains(&input.months) { return Err(bad("A schedule needs between 1 and 600 months")); }
        if super::schedules::months_elapsed(&input.start_date, &input.start_date) != 0 || !date(&input.start_date) {
            return Err(bad("A schedule needs a valid start date"));
        }
        if !valid_id(&input.account_id) { return Err(bad("A schedule needs an account")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.schedules.iter().any(|row| row.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "posts_to_ledger": false, "write_back": false}));
        }
        if state.schedules.len() >= 200 { return Err(bad("Schedule list is full")); }
        let record = super::schedules::StoredSchedule {
            id: input.request_id.clone(),
            name: input.name.trim().into(),
            kind: input.kind,
            total_cents: input.total_cents,
            months: input.months,
            start_date: input.start_date,
            account_id: input.account_id,
        };
        // Prove the schedule computes before storing it.
        super::schedules::summary(record.total_cents, record.months, &record.start_date, &record.start_date)
            .map_err(|error: ScheduleError| bad(error.message()))?;
        state.schedules.push(record);
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Stored schedules with their amortisation as of a date (defaults to the latest start date).
    pub async fn schedule_list(&self, company: &str, as_of: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        let mut rows = Vec::new();
        for record in &state.schedules {
            let summary = super::schedules::summary(record.total_cents, record.months, &record.start_date, as_of)
                .map_err(|error| bad(error.message()))?;
            rows.push(json!({
                "id": record.id, "name": record.name, "kind": record.kind, "account_id": record.account_id,
                "start_date": record.start_date, "months": record.months, "total_cents": record.total_cents,
                "monthly_cents": summary.monthly_cents, "amortised_cents": summary.amortised_cents,
                "remaining_cents": summary.remaining_cents, "months_posted": summary.months_posted
            }));
        }
        Ok(json!({"schedules": rows, "as_of": as_of, "posts_to_ledger": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    pub async fn add_recurring(&self, company: &str, input: RecurringInput) -> Result<Value> {
        if !valid_id(&input.request_id) { return Err(bad("A recurring template needs a request id")); }
        if !matches!(input.kind.as_str(), "invoice" | "bill") { return Err(bad("A recurring template is an invoice or a bill")); }
        if !matches!(input.cadence.as_str(), "weekly" | "monthly" | "quarterly" | "yearly") { return Err(bad("Cadence must be weekly, monthly, quarterly or yearly")); }
        if !date(&input.next_date) { return Err(bad("A recurring template needs a valid next date")); }
        if input.amount_cents <= 0 { return Err(bad("A recurring template needs a positive amount")); }
        if input.contact.trim().is_empty() { return Err(bad("A recurring template needs a contact")); }
        let (raw, mut state) = self.daily_load(company).await?;
        if state.recurring.iter().any(|row| row.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "scheduled_by_nxt": false, "write_back": false}));
        }
        if state.recurring.len() >= 200 { return Err(bad("Recurring template list is full")); }
        let template = Recurring {
            id: input.request_id.clone(),
            kind: input.kind,
            contact: input.contact.trim().into(),
            document_number: input.document_number.trim().into(),
            amount_cents: input.amount_cents,
            cadence: input.cadence,
            next_date: input.next_date,
            active: input.active,
        };
        state.recurring.push(template);
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({"id": input.request_id, "scheduled_by_nxt": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    /// Stored recurring templates, soonest first.
    pub async fn recurring_list(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        let mut rows = state.recurring.clone();
        rows.sort_by(|a, b| a.next_date.cmp(&b.next_date));
        Ok(json!({"recurring": rows, "scheduled_by_nxt": false, "write_back": false, "authoritative_system": "quickbooks"}))
    }

    pub async fn payroll_run(&self, company: &str, input: PayRunInput) -> Result<Value> {
        if !valid_id(&input.request_id) || !date(&input.pay_date) { return Err(bad("A pay run needs a request id and a valid pay date")); }
        if input.annual_gross_cents <= 0 { return Err(bad("A pay run needs a positive annual gross")); }
        let periods = input.periods.unwrap_or(26);
        if !(1..=52).contains(&periods) { return Err(bad("Pay periods must be 1-52")); }
        let Some(province) = crate::payroll::Province::parse(&input.province) else {
            return Err(bad("Payroll is limited to Alberta employees; other provinces are blocked"));
        };
        let (raw, mut state) = self.daily_load(company).await?;
        if state.paystubs.iter().any(|row| row.id == input.request_id) {
            return Ok(json!({"id": input.request_id, "idempotent_replay": true, "remitted": false, "filed": false, "write_back": false}));
        }
        let Some(employee) = state.employees.iter().find(|row| row.id == input.employee_id && row.active).cloned() else {
            return Err(bad("A pay run needs an active parallel employee"));
        };
        if state.paystubs.len() >= 400 { return Err(bad("Payroll history is full")); }
        let options = crate::payroll::Options {
            province,
            federal_claim_cents: input.federal_claim_cents,
            provincial_claim_cents: input.provincial_claim_cents,
            canada_employment_cents: crate::payroll::constants::CANADA_EMPLOYMENT_CENTS,
        };
        let annual = crate::payroll::annual(input.annual_gross_cents, &options).map_err(|block| bad(block.message()))?;
        let split = crate::payroll::per_period(&annual, periods).map_err(|block| bad(block.message()))?;
        let period = split.first().cloned().ok_or_else(|| bad("Pay run produced no periods"))?;
        let stub = Paystub {
            id: input.request_id.clone(),
            employee_id: employee.id.clone(),
            employee_name: employee.name.clone(),
            pay_date: input.pay_date.clone(),
            periods,
            gross_cents: period.gross_cents,
            cpp_cents: period.cpp_cents,
            cpp2_cents: period.cpp2_cents,
            ei_cents: period.ei_cents,
            federal_tax_cents: period.federal_tax_cents,
            provincial_tax_cents: period.provincial_tax_cents,
            net_cents: period.net_cents,
            pensionable_cents: period.pensionable_cents,
            insurable_cents: period.insurable_cents,
        };
        state.paystubs.push(stub);
        self.daily_save(company, raw.as_deref(), &state).await?;
        Ok(json!({
            "id": input.request_id,
            "employee": employee.name,
            "periods": periods,
            "per_period": {
                "gross_cents": period.gross_cents,
                "cpp_cents": period.cpp_cents,
                "cpp2_cents": period.cpp2_cents,
                "ei_cents": period.ei_cents,
                "federal_tax_cents": period.federal_tax_cents,
                "provincial_tax_cents": period.provincial_tax_cents,
                "net_cents": period.net_cents
            },
            "annual": {
                "gross_cents": annual.gross_cents,
                "cpp_cents": annual.cpp_cents,
                "cpp2_cents": annual.cpp2_cents,
                "ei_cents": annual.ei_cents,
                "federal_tax_cents": annual.federal_tax_cents,
                "provincial_tax_cents": annual.provincial_tax_cents,
                "net_cents": annual.net_cents
            },
            "t4_projection": crate::payroll::t4(&annual),
            "remitted": false,
            "filed": false,
            "posts_to_ledger": false,
            "write_back": false,
            "authoritative_system": "quickbooks"
        }))
    }

    /// Stored paystubs and the projected T4 totals they add up to.
    pub async fn payroll_history(&self, company: &str) -> Result<Value> {
        let (_, state) = self.daily_load(company).await?;
        let rows: Vec<&Paystub> = state.paystubs.iter().collect();
        let box14: i64 = rows.iter().map(|r| r.gross_cents * r.periods as i64).sum();
        let box16: i64 = rows.iter().map(|r| r.cpp_cents * r.periods as i64).sum();
        let box16a: i64 = rows.iter().map(|r| r.cpp2_cents * r.periods as i64).sum();
        let box18: i64 = rows.iter().map(|r| r.ei_cents * r.periods as i64).sum();
        Ok(json!({
            "paystubs": rows,
            "t4_ytd_projection": {"box14_employment_income_cents": box14, "box16_cpp_cents": box16, "box16a_cpp2_cents": box16a, "box18_ei_cents": box18},
            "remitted": false,
            "filed": false,
            "posts_to_ledger": false,
            "write_back": false,
            "authoritative_system": "quickbooks"
        }))
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
