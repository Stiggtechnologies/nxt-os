//! God's Eye: firm-wide watchers over synthetic books. No real client data.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn call(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let req = Request::builder().method(method).uri(uri).header("authorization", "Bearer k").header("content-type", "application/json");
    let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let resp = r.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn ok(r: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, method, uri, body).await;
    assert_eq!(s, 200, "{method} {uri} -> {v}");
    v
}

async fn books_snapshot(r: &axum::Router) -> Value {
    json!({
        "firm_entries": ok(r, "GET", "/api/ledger/entries", None).await,
        "firm_invoices": ok(r, "GET", "/api/ledger/invoices", None).await,
        "co_entries": ok(r, "GET", "/api/company-ledger/synthetic-co/entries", None).await,
        "co_invoices": ok(r, "GET", "/api/company-ledger/synthetic-co/invoices", None).await,
    })
}

#[tokio::test]
async fn gods_eye_watches_flags_and_drafts_without_touching_the_books() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let r = app_with(Store::memory);

    // Firm ledger: Q3 HST collected and remitted in October -> no GST finding.
    let firm_c = ok(&r, "POST", "/api/ledger/contacts", Some(json!({"kind":"customer","name":"Synthetic Firm Client","terms_days":15}))).await;
    ok(&r, "POST", "/api/ledger/invoices", Some(json!({"contact_id": firm_c["id"], "date": "2026-08-10",
        "lines": [{"description":"Synthetic service","qty":1,"rate":100000,"account_id":"acc_4100","tax_code":"HST13"}]}))).await;
    let q3 = ok(&r, "GET", "/api/ledger/reports/sales-tax?from=2026-07-01&to=2026-09-30", None).await;
    ok(&r, "POST", "/api/ledger/tax-payments", Some(json!({"account_id":"acc_1000","date":"2026-10-20","amount":q3["line_109_net_tax"]}))).await;

    // A client's company ledger: Q3 HST never remitted, and a May invoice still open.
    ok(&r, "POST", "/api/company-ledger/synthetic-co/open", Some(json!({"name":"Synthetic Co Ltd"}))).await;
    let c = ok(&r, "POST", "/api/company-ledger/synthetic-co/contacts", Some(json!({"kind":"customer","name":"Synthetic Buyer","terms_days":30}))).await;
    for date in ["2026-05-01", "2026-08-15"] {
        ok(&r, "POST", "/api/company-ledger/synthetic-co/invoices", Some(json!({"contact_id": c["id"], "date": date,
            "lines": [{"description":"Synthetic goods","qty":1,"rate":250000,"account_id":"acc_4100","tax_code":"HST13"}]}))).await;
    }

    // An unfiled synthetic T1 past its deadline.
    ok(&r, "POST", "/api/tax/returns", Some(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":"Synthetic T1","client_ref":"synthetic-co","inputs":{},"parties":[]}))).await;

    let before = books_snapshot(&r).await;
    let view = ok(&r, "GET", "/api/nixon/gods-eye?as_of=2026-11-12", None).await;
    assert_eq!(view["guardrails"]["books"], "read_only");
    assert_eq!(view["guardrails"]["client_contact"], "none");
    assert_eq!(view["ai"]["enabled"], false, "AI drafting must be off with no env set");
    let watcher = |id: &str| view["watchers"].as_array().unwrap().iter().find(|w| w["id"] == id).cloned().unwrap();
    assert_eq!(watcher("cra_notices")["status"], "no_source");

    let findings = view["findings"].as_array().unwrap();
    let has = |company: &str, w: &str| findings.iter().any(|f| f["company_id"] == company && f["watcher"] == w);
    assert!(has("synthetic-co", "gst_remittance"), "{view}");
    assert!(!has("firm", "gst_remittance"), "remitted quarter must not flag: {view}");
    assert!(has("synthetic-co", "ar_aging"), "{view}");
    assert!(has("synthetic-co", "filing_deadlines"), "{view}");
    let gst = findings.iter().find(|f| f["company_id"] == "synthetic-co" && f["watcher"] == "gst_remittance").unwrap();
    assert_eq!(gst["finding"]["risk_level"], "high");
    assert!(gst["draft_fix"].as_str().unwrap().contains("NXT does not file or pay"));
    let co = view["companies"].as_array().unwrap().iter().find(|c| c["company_id"] == "synthetic-co").unwrap();
    assert_eq!(co["name"], "Synthetic Co Ltd");
    assert_eq!(books_snapshot(&r).await, before, "the live view must not write the books");

    // A run stores proposals; nothing executes.
    let run = ok(&r, "POST", "/api/nixon/gods-eye/runs", Some(json!({"as_of":"2026-11-12"}))).await;
    assert_eq!(run["scope"], "gods_eye");
    assert_eq!(run["posting"], "disabled");
    assert_eq!(run["filing"], "disabled");
    let props = run["proposals"].as_array().unwrap();
    assert_eq!(props.len(), run["findings"].as_array().unwrap().len());
    assert!(props.iter().all(|p| p["status"] == "pending_approval" && p["execution"] == "disabled" && p["changes_books"] == false && p["files_tax"] == false));
    let id = run["id"].as_str().unwrap();
    let gst_p = props.iter().find(|p| p["payload"]["watcher"] == "gst_remittance").unwrap()["id"].as_str().unwrap().to_string();
    let ar_p = props.iter().find(|p| p["payload"]["watcher"] == "ar_aging").unwrap()["id"].as_str().unwrap().to_string();

    let decide = |pid: &str| format!("/api/nixon/reviews/{id}/proposals/{pid}/decide");
    assert_eq!(call(&r, "POST", &decide(&gst_p), Some(json!({"actor_name":"Nixon","decision":"approve","note":""}))).await.0, 400);
    assert_eq!(call(&r, "POST", &decide(&gst_p), Some(json!({"actor_name":"God's Eye watcher","decision":"approve","note":""}))).await.0, 403);
    let approved = ok(&r, "POST", &decide(&gst_p), Some(json!({"actor_name":"Casey Quinn","decision":"approve","note":"Checking CRA account"}))).await;
    let p = approved["proposals"].as_array().unwrap().iter().find(|p| p["id"] == gst_p.as_str()).unwrap();
    assert_eq!(p["status"], "approved_no_execution");
    assert_eq!(p["applied"], false);
    assert_eq!(approved["decisions"][0]["actor_name"], "Casey Quinn");
    let dismissed = ok(&r, "POST", &decide(&ar_p), Some(json!({"actor_name":"Casey Quinn","decision":"dismiss","note":"Disputed; on hold"}))).await;
    assert!(dismissed["proposals"].as_array().unwrap().iter().any(|p| p["id"] == ar_p.as_str() && p["status"] == "dismissed"));
    assert_eq!(call(&r, "POST", &decide(&gst_p), Some(json!({"actor_name":"Casey Quinn","decision":"approve","note":""}))).await.0, 409);
    let (s, _) = call(&r, "POST", &format!("/api/nixon/reviews/{id}/proposals/{gst_p}/execute"), Some(json!({"actor_name":"Casey Quinn"}))).await;
    assert!(s >= 400, "execution must stay refused");

    // AI drafting is off by default and fails cleanly.
    let (s, v) = call(&r, "POST", &format!("/api/nixon/gods-eye/runs/{id}/proposals/{gst_p}/ai-draft"), None).await;
    assert_eq!(s, 409, "{v}");

    assert_eq!(books_snapshot(&r).await, before, "runs and decisions must not write the books");
    let reviews = ok(&r, "GET", "/api/nixon/reviews", None).await;
    assert!(reviews.as_array().unwrap().iter().any(|v| v["id"] == id && v["scope"] == "gods_eye"));
}

fn line(acct: &str, d: i64, c: i64, row: u64) -> Value {
    json!({"account_id": acct, "debit_cents": d.to_string(), "credit_cents": c.to_string(), "source_row": row, "memo": "Synthetic line"})
}

#[tokio::test]
async fn gods_eye_reads_imported_books_for_payroll_and_large_entries() {
    use nxt_os::company_books::{CompanyBooks, models::*};
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let store = Store::memory();
    let books = CompanyBooks::new(store.clone());
    let mut entries = vec![];
    let mut row = 1u64;
    let mut push = |id: String, date: &str, lines: Vec<(&str, i64, i64)>| {
        let ls: Vec<Value> = lines.into_iter().map(|(a, d, c)| { row += 1; line(a, d, c, row) }).collect();
        entries.push(json!({"id": id, "date": date, "source_id": format!("archive-row:synthetic:{row}:1"), "memo": "Synthetic entry", "lines": ls}));
    };
    push("e0".into(), "2026-07-01", vec![("cash", 10_000_000, 0), ("equity", 0, 10_000_000)]);
    for i in 1..=12 {
        push(format!("e{i}"), &format!("2026-08-{:02}", i), vec![("cash", 10_000, 0), ("sales", 0, 10_000)]);
    }
    push("pay-sep".into(), "2026-09-30", vec![("wages", 500_000, 0), ("cash", 0, 400_000), ("sdp", 0, 100_000)]);
    push("big".into(), "2026-10-05", vec![("cash", 5_000_000, 0), ("sales", 0, 5_000_000)]);
    let chunk: Chunk = serde_json::from_value(json!({
        "accounts": [
            {"id":"cash","name":"Synthetic chequing","kind":"asset"},
            {"id":"sdp","name":"Source deductions payable","kind":"liability"},
            {"id":"wages","name":"Wages","kind":"expense"},
            {"id":"sales","name":"Synthetic sales","kind":"income"},
            {"id":"equity","name":"Synthetic equity","kind":null}],
        "entries": entries})).unwrap();
    let up = books.upload("synthetic-books", chunk).await.ok().unwrap();
    let tb = |a: &str, d: i64, c: i64, r: u64| json!({"account_id":a,"debit_cents":d.to_string(),"credit_cents":c.to_string(),"basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":r}});
    let input: PreviewInput = serde_json::from_value(json!({"request_id":"preview-ge","expected_head":null,
        "company":{"name":"Synthetic Books Inc","source_company_id":"synthetic-realm-ge","currency":"CAD","identity_verified":true,"currency_verified":true},
        "source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},
        "chunks":[up["sha256"]],
        "trial_balance":[tb("cash",14_720_000,0,10), tb("sdp",0,100_000,11), tb("wages",500_000,0,12), tb("sales",0,5_120_000,13), tb("equity",0,10_000_000,14)]})).unwrap();
    let p = books.preview("synthetic-books", input).await.ok().unwrap();
    assert_eq!(p["stage"], "preview_ready", "{p}");
    let commit = CommitInput { preview_id: p["preview_id"].as_str().unwrap().into(), expected_head: None, request_id: "commit-ge".into(),
        reviewer_label: "Synthetic reviewer".into(), needs_review: false, review_blockers: vec![], suspense_account_id: None, supersedes_head: None };
    books.commit("synthetic-books", commit).await.ok().unwrap();

    let r = app_with(|| store.clone());
    let before = ok(&r, "GET", "/api/company-books/synthetic-books/entries?offset=0&limit=100", None).await;
    let view = ok(&r, "GET", "/api/nixon/gods-eye?as_of=2026-11-12", None).await;
    let f: Vec<&Value> = view["findings"].as_array().unwrap().iter().filter(|f| f["company_id"] == "synthetic-books").collect();
    let payroll = f.iter().find(|f| f["watcher"] == "payroll_remittance").unwrap_or_else(|| panic!("{view}"));
    assert_eq!(payroll["finding"]["risk_level"], "high");
    assert!(payroll["finding"]["title"].as_str().unwrap().contains("2026-09"));
    let large = f.iter().find(|f| f["watcher"] == "unusual_transactions").unwrap_or_else(|| panic!("{view}"));
    assert!(large["finding"]["evidence"].as_array().unwrap().iter().any(|e| e["reference"].as_str().unwrap().ends_with("/entries/big")));
    assert!(!f.iter().any(|f| f["finding"]["id"].as_str().unwrap().contains("tb_unbalanced")));
    let co = view["companies"].as_array().unwrap().iter().find(|c| c["company_id"] == "synthetic-books").unwrap();
    assert_eq!(co["name"], "Synthetic Books Inc");
    assert_eq!(ok(&r, "GET", "/api/company-books/synthetic-books/entries?offset=0&limit=100", None).await, before);
}
