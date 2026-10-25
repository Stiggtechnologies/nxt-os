//! NXT OS services: the "Ask NXT" assistant (Claude with read-only tools
//! over the ledger and referral apps) and rule-based insights for the home
//! screen.

use crate::ledger::engine::{Ledger, fmt_money};
use crate::ledger::models::DocKind;
use crate::ledger::routes::{Range, run_report};
use crate::referral::engine::Engine as Referrals;
use crate::referral::routes::{ApiError, require_admin};
use axum::{Json, Router, extract::State, http::StatusCode, middleware, routing::{get, post}};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct Os {
    pub ledger: Arc<Ledger>,
    pub referrals: Arc<Referrals>,
}

const MAX_TOOL_ROUNDS: usize = 8;
const MAX_TOOL_RESULT_CHARS: usize = 40_000;

fn model() -> String {
    std::env::var("NXT_AI_MODEL").ok().filter(|m| !m.is_empty()).unwrap_or_else(|| "claude-opus-5-5".into())
}

fn api_url() -> String {
    let base = std::env::var("ANTHROPIC_BASE_URL").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| "https://api.anthropic.com".into());
    format!("{}/v1/messages", base.trim_end_matches('/'))
}

fn api_key() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty())
}

pub fn router(os: Os) -> Router {
    Router::new()
        .route("/api/os/status", get(status))
        .route("/api/os/insights", get(insights))
        .route("/api/os/ask", post(ask))
        .route("/api/os/sample-data", post(sample_data))
        .layer(middleware::from_fn(require_admin))
        .with_state(os)
}

async fn status(State(os): State<Os>) -> Json<Value> {
    Json(json!({
        "ai_enabled": api_key().is_some(),
        "model": model(),
        "storage": os.ledger.store.backend_name(),
        "apps": [
            { "id": "ledger", "name": "NXT Ledger", "path": "/apps/ledger/" },
            { "id": "referrals", "name": "NXT Referrals", "path": "/apps/referrals/" },
        ],
    }))
}

// ---- insights ---------------------------------------------------------------

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

async fn insights(State(os): State<Os>) -> Result<Json<Value>, ApiError> {
    let d = os.ledger.dashboard().await.map_err(ApiError::from)?;
    let r = os.referrals.analytics().await.map_err(ApiError::from)?;
    let n = |k: &str| d[k].as_i64().unwrap_or(0);
    let mut out = Vec::new();
    let mut push = |level: &str, title: String, body: String, app: &str, view: &str| {
        out.push(json!({ "level": level, "title": title, "body": body, "app": app, "view": view }));
    };
    if n("overdue_invoices") > 0 {
        push(
            "warn",
            format!("{} overdue to collect", fmt_money(n("overdue_receivable"))),
            format!("{} past due. Follow up today to protect cash flow.", plural(n("overdue_invoices"), "invoice is", "invoices are")),
            "ledger",
            "invoices",
        );
    }
    if n("pending_bank_transactions") > 0 {
        push(
            "info",
            format!("{} to review", plural(n("pending_bank_transactions"), "bank transaction", "bank transactions")),
            "Categorize imported transactions to keep your books current.".into(),
            "ledger",
            "banking",
        );
    }
    if n("gst_hst_owing") > 0 {
        push(
            "info",
            format!("{} GST/HST owing", fmt_money(n("gst_hst_owing"))),
            "Set this aside for your next remittance to the CRA.".into(),
            "ledger",
            "reports/sales-tax",
        );
    }
    if n("overdue_bills") > 0 {
        push("warn", format!("{} overdue", plural(n("overdue_bills"), "bill", "bills")), "Pay or reschedule to keep vendors on side.".into(), "ledger", "bills");
    }
    let months = d["months"].as_array().cloned().unwrap_or_default();
    if let [.., prev, cur] = months.as_slice() {
        let (p, c) = (prev["net"].as_i64().unwrap_or(0), cur["net"].as_i64().unwrap_or(0));
        if p != 0 || c != 0 {
            let up = c >= p;
            push(
                if up { "good" } else { "info" },
                format!("Net income {} month over month", if up { "up" } else { "down" }),
                format!("{} this month vs {} last month.", fmt_money(c), fmt_money(p)),
                "ledger",
                "reports/profit-loss",
            );
        }
    }
    let rev = r["totals"]["referred_revenue"].as_f64().unwrap_or(0.0);
    if rev > 0.0 {
        push(
            "good",
            format!("{} in referred revenue", fmt_money((rev * 100.0).round() as i64)),
            format!("{} from {}.", plural(r["totals"]["referrals"].as_i64().unwrap_or(0), "referral", "referrals"), plural(r["totals"]["advocates"].as_i64().unwrap_or(0), "advocate", "advocates")),
            "referrals",
            "overview",
        );
    }
    Ok(Json(json!({ "dashboard": d, "referrals": r, "insights": out })))
}

// ---- assistant ----------------------------------------------------------------

const SYSTEM_PROMPT: &str = "You are NXT, the assistant built into NXT OS: an operating system for running a Canadian business. \
Two apps are installed: NXT Ledger (double-entry accounting: invoices, bills, banking, GST/HST, financial reports) \
and NXT Referrals (customer referral and loyalty programs).

How you work:
- Answer from the user's live data. Use the tools to look things up; never estimate or invent figures.
- Every money value returned by tools is an integer number of cents in CAD. Present amounts in dollars, e.g. $12,345.67.
- Your access is read-only. When the user wants to create or change something (send an invoice, record a payment, \
categorize a transaction), call open_app to take them to the right screen and tell them what to do there.
- Dates are ISO (YYYY-MM-DD). If the user gives a relative period like \"last quarter\", resolve it against today's date from get_dashboard.
- GST/HST figures are working papers, not filed returns; say so when asked about filing.

Voice: direct, confident and plain-spoken. Lead with the answer, then the key numbers, then one practical next step \
when it helps. Keep replies short; use a compact list or table only when comparing several figures.";

fn tools() -> Value {
    json!([
        {
            "name": "get_dashboard",
            "description": "Snapshot of the business today: today's date, cash, receivables, payables, overdue counts, GST/HST owing, pending bank transactions, fiscal year-to-date income/expenses/net, and the last six months of income and expenses.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "run_report",
            "description": "Runs a ledger report. profit-loss and sales-tax (GST/HST working paper) use from/to; balance-sheet, trial-balance, ar-aging and ap-aging use to as the as-of date; general-ledger uses from/to and optionally account_code. Omitted dates default to the calendar year to date.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "report": { "type": "string", "enum": ["profit-loss", "balance-sheet", "trial-balance", "ar-aging", "ap-aging", "sales-tax", "general-ledger"] },
                    "from": { "type": "string", "description": "Start date YYYY-MM-DD" },
                    "to": { "type": "string", "description": "End or as-of date YYYY-MM-DD" },
                    "account_code": { "type": "string", "description": "Chart of accounts code for general-ledger, e.g. 1000" }
                },
                "required": ["report"]
            }
        },
        {
            "name": "list_documents",
            "description": "Lists invoices (money owed to the business) or bills (money the business owes), newest first, with contact name, totals, balance and status.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["invoice", "bill"] },
                    "status": { "type": "string", "enum": ["all", "open", "overdue", "paid", "void"], "description": "open includes partial and overdue" },
                    "contact": { "type": "string", "description": "Optional case-insensitive filter on customer/vendor name" }
                },
                "required": ["kind"]
            }
        },
        {
            "name": "list_accounts",
            "description": "The chart of accounts: code, name, type and role for every account.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "search_contacts",
            "description": "Finds customers and vendors by name or email.",
            "input_schema": {
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }
        },
        {
            "name": "referral_overview",
            "description": "Referral program performance: totals (advocates, clicks, shares, referrals, rejected, revenue in dollars), share channels, conversion rates and the top advocates.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "open_app",
            "description": "Opens an NXT OS app at a screen so the user can act. Ledger views: dashboard, invoices, invoices/new, bills, bills/new, customers, vendors, banking, accounts, journal, reports/profit-loss, reports/balance-sheet, reports/trial-balance, reports/ar-aging, reports/ap-aging, reports/sales-tax, reports/general-ledger, settings. Referrals views: overview, advocates, conversions, rewards, loyalty, settings.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "app": { "type": "string", "enum": ["ledger", "referrals"] },
                    "view": { "type": "string" }
                },
                "required": ["app"]
            }
        }
    ])
}

#[derive(Deserialize)]
struct ChatTurn {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AskBody {
    messages: Vec<ChatTurn>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string).filter(|x| !x.is_empty())
}

async fn run_tool(os: &Os, name: &str, input: &Value, actions: &mut Vec<Value>) -> Result<Value, String> {
    let l = &os.ledger;
    match name {
        "get_dashboard" => l.dashboard().await.map_err(|e| format!("{e:?}")),
        "run_report" => {
            let report = s(input, "report").ok_or("report is required")?;
            let account_id = match s(input, "account_code") {
                Some(code) => Some(
                    l.accounts().await.map_err(|e| format!("{e:?}"))?.into_iter().find(|a| a.code == code).ok_or(format!("no account with code {code}"))?.id,
                ),
                None => None,
            };
            run_report(l, &report, &Range::new(s(input, "from"), s(input, "to"), account_id)).await.map_err(|e| format!("{e:?}"))
        }
        "list_documents" => {
            let kind = if s(input, "kind").as_deref() == Some("bill") { DocKind::Bill } else { DocKind::Invoice };
            let want = s(input, "status").unwrap_or("all".into());
            let filter = s(input, "contact").map(|c| c.to_lowercase());
            let today = crate::ledger::engine::today();
            let names: std::collections::HashMap<String, String> =
                l.contacts(None).await.map_err(|e| format!("{e:?}"))?.into_iter().map(|c| (c.id, c.name)).collect();
            let docs: Vec<Value> = l
                .docs(Some(kind))
                .await
                .map_err(|e| format!("{e:?}"))?
                .into_iter()
                .filter_map(|d| {
                    let status = d.status(&today);
                    let keep = match want.as_str() {
                        "open" => matches!(status, "open" | "partial" | "overdue"),
                        "all" => true,
                        w => status == w,
                    };
                    let name = names.get(&d.contact_id).cloned().unwrap_or_default();
                    let name_ok = filter.as_ref().is_none_or(|f| name.to_lowercase().contains(f));
                    (keep && name_ok).then(|| {
                        json!({ "number": d.number, "contact": name, "date": d.date, "due_date": d.due_date, "total": d.total, "balance": d.balance(), "status": status })
                    })
                })
                .take(100)
                .collect();
            Ok(json!({ "count": docs.len(), "documents": docs }))
        }
        "list_accounts" => l
            .accounts()
            .await
            .map(|v| json!(v.iter().map(|a| json!({ "code": a.code, "name": a.name, "kind": a.kind, "role": a.role, "archived": a.archived })).collect::<Vec<_>>()))
            .map_err(|e| format!("{e:?}")),
        "search_contacts" => {
            let q = s(input, "query").unwrap_or_default().to_lowercase();
            let v = l.contacts(None).await.map_err(|e| format!("{e:?}"))?;
            Ok(json!(v
                .into_iter()
                .filter(|c| c.name.to_lowercase().contains(&q) || c.email.to_lowercase().contains(&q))
                .take(25)
                .map(|c| json!({ "name": c.name, "kind": c.kind, "email": c.email, "phone": c.phone, "terms_days": c.terms_days }))
                .collect::<Vec<_>>()))
        }
        "referral_overview" => {
            let a = os.referrals.analytics().await.map_err(|e| e.to_string())?;
            let top: Vec<Value> = os
                .referrals
                .list_advocates()
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .take(10)
                .map(|x| json!({ "name": x.name, "email": x.email, "referrals": x.referrals, "clicks": x.clicks, "referred_revenue_dollars": x.referred_revenue }))
                .collect();
            Ok(json!({ "analytics": a, "top_advocates": top, "note": "Referral amounts are in dollars, not cents." }))
        }
        "open_app" => {
            let app = s(input, "app").ok_or("app is required")?;
            if app != "ledger" && app != "referrals" {
                return Err("app must be ledger or referrals".into());
            }
            actions.push(json!({ "type": "open_app", "app": app, "view": s(input, "view") }));
            Ok(json!({ "opened": true }))
        }
        other => Err(format!("unknown tool {other}")),
    }
}

async fn call_claude(http: &reqwest::Client, key: &str, messages: &[Value]) -> Result<Value, String> {
    let body = json!({
        "model": model(),
        "max_tokens": 16000,
        "output_config": { "effort": "medium" },
        "fallbacks": "default",
        "system": SYSTEM_PROMPT,
        "tools": tools(),
        "messages": messages,
    });
    let resp = http
        .post(api_url())
        .timeout(Duration::from_secs(120))
        .header("x-api-key", key)
        .header("anthropic-version", "2026-10-21")
        .header("anthropic-beta", "server-side-fallback-2026-07-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("couldn't reach the Claude API: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| format!("invalid Claude API response: {e}"))?;
    if !status.is_success() {
        let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
        return Err(format!("Claude API error ({status}): {msg}"));
    }
    Ok(v)
}

async fn ask(State(os): State<Os>, Json(b): Json<AskBody>) -> Result<Json<Value>, ApiError> {
    let Some(key) = api_key() else {
        return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, "Ask NXT needs an ANTHROPIC_API_KEY configured for this deployment.".into()));
    };
    let mut messages: Vec<Value> = b
        .messages
        .iter()
        .rev()
        .take(20)
        .rev()
        .filter(|t| (t.role == "user" || t.role == "assistant") && !t.content.trim().is_empty())
        .map(|t| json!({ "role": t.role, "content": t.content.chars().take(8000).collect::<String>() }))
        .collect();
    if messages.first().is_some_and(|m| m["role"] == "assistant") {
        messages.remove(0);
    }
    if messages.last().is_none_or(|m| m["role"] != "user") {
        return Err(ApiError(StatusCode::BAD_REQUEST, "the last message must be from the user".into()));
    }
    let http = reqwest::Client::new();
    let mut actions = Vec::new();
    let mut tools_used = Vec::new();
    for _ in 0..MAX_TOOL_ROUNDS {
        let resp = call_claude(&http, &key, &messages).await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
        let stop = resp["stop_reason"].as_str().unwrap_or("");
        if stop == "refusal" {
            return Ok(Json(json!({ "reply": "I can't help with that request.", "actions": actions, "tools_used": tools_used })));
        }
        let content = resp["content"].as_array().cloned().unwrap_or_default();
        if stop != "tool_use" {
            let reply: Vec<&str> = content.iter().filter(|c| c["type"] == "text").filter_map(|c| c["text"].as_str()).collect();
            let mut reply = reply.join("\n\n");
            if stop == "max_tokens" {
                reply.push_str("\n\n(Response was cut short.)");
            }
            return Ok(Json(json!({ "reply": reply, "actions": actions, "tools_used": tools_used, "model": resp["model"] })));
        }
        // Echo the assistant turn back unchanged (thinking blocks included).
        messages.push(json!({ "role": "assistant", "content": content }));
        let mut results = Vec::new();
        for block in content.iter().filter(|c| c["type"] == "tool_use") {
            let name = block["name"].as_str().unwrap_or("");
            tools_used.push(name.to_string());
            let input = block.get("input").cloned().unwrap_or(json!({}));
            let (text, is_error) = match run_tool(&os, name, &input, &mut actions).await {
                Ok(v) => {
                    let mut t = v.to_string();
                    if t.len() > MAX_TOOL_RESULT_CHARS {
                        t.truncate(MAX_TOOL_RESULT_CHARS);
                        t.push_str("…(truncated; narrow the date range or filter)");
                    }
                    (t, false)
                }
                Err(e) => (e, true),
            };
            results.push(json!({ "type": "tool_result", "tool_use_id": block["id"], "content": text, "is_error": is_error }));
        }
        messages.push(json!({ "role": "user", "content": results }));
    }
    Ok(Json(json!({ "reply": "That took more steps than I allow for one question. Try narrowing it down.", "actions": actions, "tools_used": tools_used })))
}

// ---- sample data ----------------------------------------------------------------

fn days_ago(n: i64) -> String {
    (chrono::Utc::now().date_naive() - chrono::Duration::days(n)).format("%Y-%m-%d").to_string()
}

/// Seeds a realistic six-month history into an empty workspace.
async fn sample_data(State(os): State<Os>) -> Result<Json<Value>, ApiError> {
    use crate::ledger::engine::*;
    use crate::ledger::models::ContactKind;
    let l = &os.ledger;
    if !l.entries().await?.is_empty() {
        return Err(ApiError(StatusCode::CONFLICT, "sample data can only be loaded into empty books".into()));
    }
    let contact = |kind, name: &str, email: &str, terms| ContactInput {
        kind, name: name.into(), email: email.into(), phone: String::new(), address: String::new(),
        terms_days: Some(terms), default_tax_code: None, archived: false,
    };
    let mut customers = Vec::new();
    for (n, e, t) in [("Northwind Logistics", "ap@northwind.example", 30), ("Maple & Pine Studio", "hello@mapleandpine.example", 15), ("Harbourfront Dental", "office@harbourfront.example", 30), ("Lakeshore Builders", "accounts@lakeshore.example", 45)] {
        customers.push(l.save_contact(None, contact(ContactKind::Customer, n, e, t)).await?);
    }
    let mut vendors = Vec::new();
    for (n, e) in [("Cloudline Hosting", "billing@cloudline.example"), ("Bay Street Legal", "billing@baystreet.example"), ("Metro Office Supply", "orders@metro.example"), ("Union Workspace", "rent@union.example")] {
        vendors.push(l.save_contact(None, contact(ContactKind::Vendor, n, e, 30)).await?);
    }
    l.create_journal(JournalInput {
        date: days_ago(185),
        memo: "Founders' investment".into(),
        lines: vec![
            crate::ledger::models::Line { account_id: "acc_1000".into(), debit: 4_000_000, ..Default::default() },
            crate::ledger::models::Line { account_id: "acc_3000".into(), credit: 4_000_000, ..Default::default() },
        ],
    }).await?;
    let line = |d: &str, qty: f64, rate: i64, acct: &str| DocLineInput { description: d.into(), qty, rate, account_id: acct.into(), tax_code: Some("HST13".into()) };
    // Monthly invoices; older ones paid, recent ones open, one overdue.
    for m in 0..6i64 {
        let base = 170 - m * 30;
        for (i, c) in customers.iter().enumerate() {
            if (m + i as i64) % 3 == 2 { continue; }
            let hours = 18.0 + ((m * 7 + i as i64 * 5) % 22) as f64;
            let inv = l.create_doc(DocKind::Invoice, DocInput {
                contact_id: c.id.clone(), date: days_ago(base - i as i64 * 3), due_date: None, number: None, memo: String::new(),
                lines: vec![line("Strategy & delivery (hours)", hours, 16_500, "acc_4100"), line("Platform licence", 1.0, 120_000, "acc_4000")],
            }).await?;
            let paid = base - i as i64 * 3 > 50 && !(i == 2 && m == 3);
            if paid {
                l.record_payment("received", PaymentInput {
                    contact_id: c.id.clone(), date: days_ago(base - i as i64 * 3 - 12), account_id: "acc_1000".into(),
                    amount: inv.total, applications: vec![], memo: "E-transfer".into(),
                }).await?;
            }
        }
        // Monthly costs.
        let bill = |v: usize, d: &str, amt: i64, acct: &str| DocInput {
            contact_id: vendors[v].id.clone(), date: days_ago(base + 2), due_date: None, number: None, memo: String::new(),
            lines: vec![line(d, 1.0, amt, acct)],
        };
        for b in [bill(0, "Cloud infrastructure", 84_000, "acc_6600"), bill(3, "Office membership", 260_000, "acc_6500")] {
            let doc = l.create_doc(DocKind::Bill, b).await?;
            if base > 35 {
                l.record_payment("made", PaymentInput { contact_id: doc.contact_id.clone(), date: days_ago(base - 10), account_id: "acc_1000".into(), amount: doc.total, applications: vec![], memo: String::new() }).await?;
            }
        }
        l.money("out", MoneyInput {
            account_id: "acc_2200".into(), date: days_ago(base - 6), contact_id: None, memo: "Team expenses".into(), amounts_include_tax: true,
            lines: vec![
                MoneyLineInput { account_id: "acc_6200".into(), amount: 18_650 + m * 1_200, tax_code: Some("HST13".into()), description: "Client lunches".into() },
                MoneyLineInput { account_id: "acc_6000".into(), amount: 45_000, tax_code: Some("HST13".into()), description: "Paid social".into() },
            ],
        }).await?;
        l.money("out", MoneyInput {
            account_id: "acc_1000".into(), date: days_ago(base - 14), contact_id: None, memo: "Payroll".into(), amounts_include_tax: false,
            lines: vec![MoneyLineInput { account_id: "acc_7000".into(), amount: 820_000, tax_code: Some("NONE".into()), description: "Salaries".into() }],
        }).await?;
    }
    let legal = l.create_doc(DocKind::Bill, DocInput {
        contact_id: vendors[1].id.clone(), date: days_ago(48), due_date: None, number: Some("BSL-2231".into()), memo: String::new(),
        lines: vec![line("Commercial agreements review", 1.0, 340_000, "acc_6400")],
    }).await?;
    let _ = legal;
    let csv = format!(
        "Date,Description,Amount\n{},ADOBE *CREATIVE CLOUD,-56.49\n{},ROGERS BUSINESS INTERNET,-124.30\n{},STRIPE PAYOUT,2840.00\n{},UBER *TRIP TORONTO,-23.85\n{},ADOBE *CREATIVE CLOUD,-56.49\n",
        days_ago(34), days_ago(20), days_ago(9), days_ago(6), days_ago(4)
    );
    l.import_feed("acc_1000", &csv).await?;

    // Referral program activity.
    let r = &os.referrals;
    for (i, (name, email)) in [("Avery Chen", "avery@example.com"), ("Jordan Patel", "jordan@example.com"), ("Sam Okafor", "sam@example.com")].iter().enumerate() {
        let (adv, _) = r.enroll_advocate(email, name).await.map_err(ApiError::from)?;
        for k in 0..(4 - i) {
            r.record_click(&adv.code, None).await?;
            if k % 2 == 0 {
                r.track_purchase(crate::referral::engine::PurchaseInput {
                    order_id: format!("S-{i}{k}"), email: format!("friend{i}{k}@example.com"), amount: 89.0 + 40.0 * k as f64,
                    referral_code: Some(adv.code.clone()), coupon_code: None,
                }).await.map_err(ApiError::from)?;
            }
        }
    }
    Ok(Json(json!({ "ok": true })))
}
