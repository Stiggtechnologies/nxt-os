//! Synthetic acceptance: each client's own Ledger book, apart from the firm and other clients.
use axum::{body::Body, http::Request};
use nxt_os::{
    app_with,
    company_books::{CompanyBooks, models::*},
    firm::staff,
    ledger::engine::Ledger,
    store::Store,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

const PW: &str = "synthetic-pass-phrase-1";
const PW2: &str = "synthetic-pass-phrase-2";
/// These tests install the process-wide staff registry, so they take turns.
static STAFF: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn call(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> (u16, Value) {
    let mut q = Request::builder().method(m).uri(uri).header("x-forwarded-for", "10.2.0.1");
    if !tok.is_empty() { q = q.header("authorization", format!("Bearer {tok}")); }
    let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
    let resp = r.clone().oneshot(q).await.unwrap();
    let s = resp.status().as_u16();
    let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
async fn ok(r: &axum::Router, m: &str, uri: &str, tok: &str, body: Option<Value>) -> Value {
    let (s, v) = call(r, m, uri, tok, body).await;
    assert_eq!(s, 200, "{m} {uri}: {v}");
    v
}
async fn scoped_staff(r: &axum::Router, clients: Value) -> String {
    ok(r, "POST", "/api/staff/users", "k", Some(json!({"username":"scoped","display_name":"Sam Scoped","role":"preparer","clients":clients,"temporary_password":PW}))).await;
    let tok = ok(r, "POST", "/api/staff/login", "", Some(json!({"username":"scoped","password":PW}))).await["token"].as_str().unwrap().to_string();
    let secret = ok(r, "POST", "/api/staff/mfa/enroll", &tok, Some(json!({}))).await["secret"].as_str().unwrap().to_string();
    ok(r, "POST", "/api/staff/mfa/confirm", &tok, Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await;
    ok(r, "POST", "/api/staff/me/password", &tok, Some(json!({"current_password":PW,"new_password":PW2}))).await;
    tok
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_books_are_separate_and_scoped() {
    let _turn = STAFF.lock().await;
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let shared = Store::memory();
    let r = app_with(move || shared.clone());
    let a = "/api/company-ledger/client-a";
    let b = "/api/company-ledger/client-b";

    // A mistyped id cannot start a book, and a book needs a name.
    assert_eq!(call(&r, "GET", &format!("{a}/accounts"), "k", None).await.0, 404);
    assert_eq!(call(&r, "POST", &format!("{a}/open"), "k", Some(json!({"name":" "}))).await.0, 400);
    let opened = ok(&r, "POST", &format!("{a}/open"), "k", Some(json!({"name":"Client A Ltd","province":"AB"}))).await;
    assert_eq!(opened["already_open"], false);
    assert_eq!(opened["company"]["name"], "Client A Ltd");
    let again = ok(&r, "POST", &format!("{a}/open"), "k", Some(json!({"name":"Renamed"}))).await;
    assert_eq!(again["already_open"], true);
    assert_eq!(again["company"]["name"], "Client A Ltd");
    ok(&r, "POST", &format!("{b}/open"), "k", Some(json!({"name":"Client B Ltd"}))).await;
    assert_eq!(call(&r, "GET", "/api/company-ledger/bad%20id/accounts", "k", None).await.0, 400);

    // The journal, invoice, payment and report path, with matching account codes in both books.
    let accounts = ok(&r, "GET", &format!("{a}/accounts"), "k", None).await;
    let codes = |v: &Value| { let mut c: Vec<String> = v.as_array().unwrap().iter().map(|a| a["code"].as_str().unwrap().into()).collect(); c.sort(); c };
    assert_eq!(codes(&accounts), codes(&ok(&r, "GET", &format!("{b}/accounts"), "k", None).await));
    let (bank, income) = (accounts.as_array().unwrap().iter().find(|x| x["kind"] == "asset" && x["role"] == "bank").unwrap()["id"].as_str().unwrap().to_string(),
        accounts.as_array().unwrap().iter().find(|x| x["kind"] == "income").unwrap()["id"].as_str().unwrap().to_string());
    let customer = ok(&r, "POST", &format!("{a}/contacts"), "k", Some(json!({"kind":"customer","name":"Maple"}))).await;
    let invoice = ok(&r, "POST", &format!("{a}/invoices"), "k", Some(json!({"contact_id":customer["id"],"date":"2026-02-02","lines":[{"account_id":income,"description":"Work","qty":1.0,"rate":10000}]}))).await;
    assert_eq!(invoice["number"], "INV-1001");
    let total = invoice["total"].as_i64().unwrap();
    ok(&r, "POST", &format!("{a}/payments/received"), "k", Some(json!({"contact_id":customer["id"],"date":"2026-02-03","account_id":bank,"amount":total/2}))).await;
    let tb = ok(&r, "GET", &format!("{a}/reports/trial-balance?to=2026-12-31"), "k", None).await;
    assert_eq!(tb["balanced"], true, "{tb}");
    assert_eq!(tb["total_debit"].as_i64().unwrap() > 0, true, "{tb}");
    let aging = ok(&r, "GET", &format!("{a}/reports/ar-aging?to=2026-12-31"), "k", None).await;
    assert!(aging.to_string().contains("Maple"), "{aging}");

    // Nothing leaks to the other client or the firm, and sequences are per book.
    assert_eq!(ok(&r, "GET", &format!("{b}/entries"), "k", None).await, json!([]));
    assert_eq!(ok(&r, "GET", &format!("{b}/contacts"), "k", None).await, json!([]));
    assert_eq!(ok(&r, "GET", "/api/ledger/entries", "k", None).await, json!([]));
    assert_ne!(ok(&r, "GET", "/api/ledger/company", "k", None).await["name"], "Client A Ltd");
    let income_b = ok(&r, "GET", &format!("{b}/accounts"), "k", None).await;
    let income_b = income_b.as_array().unwrap().iter().find(|x| x["kind"] == "income").unwrap()["id"].as_str().unwrap().to_string();
    let customer_b = ok(&r, "POST", &format!("{b}/contacts"), "k", Some(json!({"kind":"customer","name":"Birch"}))).await;
    let first_b = ok(&r, "POST", &format!("{b}/invoices"), "k", Some(json!({"contact_id":customer_b["id"],"date":"2026-02-02","lines":[{"account_id":income_b,"description":"Work","qty":1.0,"rate":500}]}))).await;
    assert_eq!(first_b["number"], "INV-1001");

    // Period close is per book.
    let mut company = ok(&r, "GET", &format!("{a}/company"), "k", None).await;
    company["closing_date"] = json!("2026-03-31");
    ok(&r, "PUT", &format!("{a}/company"), "k", Some(company)).await;
    let late = json!({"contact_id":customer["id"],"date":"2026-03-01","lines":[{"account_id":income,"description":"Late","qty":1.0,"rate":100}]});
    assert_eq!(call(&r, "POST", &format!("{a}/invoices"), "k", Some(late)).await.0, 409);
    ok(&r, "POST", &format!("{b}/invoices"), "k", Some(json!({"contact_id":customer_b["id"],"date":"2026-03-01","lines":[{"account_id":income_b,"description":"Fine","qty":1.0,"rate":100}]}))).await;

    // A scoped account reaches only its client, including the source lists.
    let tok = scoped_staff(&r, json!(["client-a"])).await;
    ok(&r, "GET", &format!("{a}/reports/trial-balance?to=2026-12-31"), &tok, None).await;
    ok(&r, "POST", &format!("{a}/contacts"), &tok, Some(json!({"kind":"vendor","name":"Spruce"}))).await;
    assert_eq!(call(&r, "GET", &format!("{b}/entries"), &tok, None).await.0, 403);
    assert_eq!(call(&r, "POST", &format!("{b}/contacts"), &tok, Some(json!({"kind":"vendor","name":"Nope"}))).await.0, 403);
    assert_eq!(call(&r, "POST", "/api/company-ledger/client-c/open", &tok, Some(json!({"name":"Nope"}))).await.0, 403);
    // An encoded id is not a way around the guard.
    assert_eq!(call(&r, "GET", "/api/company-ledger/client%2Db/entries", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", "/api/company-books/client-b/source-lists", &tok, None).await.0, 403);
    assert_ne!(call(&r, "GET", "/api/company-books/client-a/source-lists", &tok, None).await.0, 403);
    assert_eq!(call(&r, "GET", &format!("{a}/entries"), "", None).await.0, 401);
}

#[tokio::test]
async fn firm_keys_are_unchanged_and_books_do_not_touch_them() {
    let store = Store::memory();
    let firm = Ledger::new(store.clone());
    let mut c = firm.company().await.unwrap();
    c.name = "Firm".into();
    firm.save_company(&c).await.unwrap();
    assert!(store.get("gl:company").await.unwrap().is_some());
    let book = Ledger::for_company(store.clone(), "client-a");
    c.name = "Client".into();
    book.save_company(&c).await.unwrap();
    assert!(store.get("company-ledger:client-a:company").await.unwrap().is_some());
    assert_eq!(firm.company().await.unwrap().name, "Firm");
    assert_eq!(book.company().await.unwrap().name, "Client");
}

#[tokio::test]
async fn an_unreadable_record_fails_the_read_instead_of_vanishing() {
    let store = Store::memory();
    let book = Ledger::for_company(store.clone(), "client-a");
    assert!(!book.accounts().await.unwrap().is_empty());
    store.sadd("company-ledger:client-a:accounts", "broken").await.unwrap();
    store.set("company-ledger:client-a:accounts:broken", "{not json").await.unwrap();
    assert!(book.accounts().await.is_err());
}

fn chunk(entries: usize) -> Chunk {
    let rows: Vec<Value> = (0..entries).map(|i| json!({"id":format!("src-{i}"),"date":"2026-01-01","source_id":format!("row:{i}"),"memo":"Opening","lines":[
        {"account_id":"cash","debit_cents":"100","credit_cents":"0","source_row":2*i+1,"memo":"Debit"},
        {"account_id":"equity","debit_cents":"0","credit_cents":"100","source_row":2*i+2,"memo":"Credit"}]})).collect();
    serde_json::from_value(json!({"accounts":[
        {"id":"cash","name":"Cash","kind":"asset"},{"id":"ar","name":"AR","kind":"asset"},
        {"id":"revenue","name":"Revenue","kind":"income"},{"id":"equity","name":"Equity","kind":"equity"}],"entries":rows})).unwrap()
}
async fn imported(books: &CompanyBooks, entries: usize) -> String {
    let e = |x: nxt_os::referral::routes::ApiError| format!("{} {}", x.0, x.1);
    let up = books.upload("co", chunk(entries)).await.map_err(e).unwrap();
    let total = (entries * 100).to_string();
    let control = |a: &str, d: &str, c: &str, row: u32| json!({"account_id":a,"debit_cents":d,"credit_cents":c,"basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":row}});
    let input: PreviewInput = serde_json::from_value(json!({"request_id":"preview","expected_head":null,
        "company":{"name":"Synthetic","source_company_id":"realm","currency":"CAD","identity_verified":true,"currency_verified":true},
        "source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},
        "chunks":[up["sha256"]],
        "trial_balance":[control("cash",&total,"0",1),control("equity","0",&total,2),control("ar","0","0",3),control("revenue","0","0",4)]})).unwrap();
    let preview = books.preview("co", input).await.map_err(e).unwrap();
    let head = books.commit("co", CommitInput { preview_id: preview["preview_id"].as_str().unwrap().into(), expected_head: None, request_id: "commit".into(), reviewer_label: "Synthetic".into(), needs_review: false, review_blockers: vec![], suspense_account_id: None, supersedes_head: None }).await.map_err(e).unwrap();
    head["head_sha256"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn invoice_and_payment_link_their_journals_on_a_book_over_one_page() {
    let e = |x: nxt_os::referral::routes::ApiError| format!("{} {}", x.0, x.1);
    let books = CompanyBooks::new(Store::memory());
    let head = imported(&books, 205).await;
    let invoice = || json!({"request_id":"inv-1","expected_head":head,"operator_label":"Synthetic","kind":"invoice","contact":"Maple","date":"2026-02-02","memo":"Invoice","control_account":"ar","lines":[{"account_id":"revenue","amount_cents":2500,"class_id":null}]});
    let opened = books.open_document("co", serde_json::from_value(invoice()).unwrap()).await.map_err(e).unwrap();
    assert_eq!(opened["open_cents"], 2500);
    let journal = opened["journal_entry_id"].as_str().unwrap();
    assert!(journal.starts_with("nxtj_"));
    let book = books.entries("co", 205, 10, None).await.map_err(e).unwrap();
    assert_eq!(book["rows"][0]["id"], journal);
    let paid = books.apply_payment("co", serde_json::from_value(json!({"request_id":"pay-1","expected_head":opened["head_sha256"],"operator_label":"Synthetic","document_id":"inv-1","cash_account":"cash","date":"2026-02-03","amount_cents":1000})).unwrap()).await.map_err(e).unwrap();
    assert_eq!(paid["open_cents"], 1500);
    assert_eq!(books.subledger("co").await.map_err(e).unwrap()["documents"][0]["open_cents"], 1500);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn imported_balances_carry_into_the_clients_ledger_once() {
    let _turn = STAFF.lock().await;
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let shared = Store::memory();
    let books = CompanyBooks::new(shared.clone());
    let head = imported(&books, 3).await;
    let shared_app = shared.clone();
    let r = app_with(move || shared_app.clone());
    let base = "/api/company-ledger/co";

    ok(&r, "POST", &format!("{base}/open"), "k", Some(json!({"name":"Synthetic"}))).await;
    let v = ok(&r, "POST", &format!("{base}/opening-balances/preview"), "k", Some(json!({}))).await;
    assert_eq!(v["posted"], false);
    let plan = &v["plan"];
    assert_eq!(plan["blockers"], json!([]), "{plan}");
    assert_eq!(plan["expected_head"], head);
    assert_eq!(plan["date"], "2026-12-31");
    assert_eq!(plan["total_debit"], 300);
    // Only the plan so far: nothing has been written to the books.
    assert_eq!(ok(&r, "GET", &format!("{base}/entries"), "k", None).await, json!([]));

    let tok = scoped_staff(&r, json!(["co"])).await;
    let post = json!({"expected_head": head, "reviewer": "Synthetic reviewer"});
    assert_eq!(call(&r, "POST", &format!("{base}/opening-balances"), &tok, Some(post.clone())).await.0, 403);
    assert_eq!(call(&r, "POST", &format!("{base}/opening-balances"), "k", Some(json!({"expected_head":"0".repeat(64),"reviewer":"x"}))).await.0, 409);
    assert_eq!(call(&r, "POST", &format!("{base}/opening-balances"), "k", Some(json!({"expected_head":head,"reviewer":" "}))).await.0, 400);

    let receipt = ok(&r, "POST", &format!("{base}/opening-balances"), "k", Some(post.clone())).await;
    assert_eq!(receipt["tied_out"], true, "{receipt}");
    assert_eq!(receipt["accounts_not_tied"], json!([]));
    assert_eq!(receipt["total_debit"], 300);

    let tb = ok(&r, "GET", &format!("{base}/reports/trial-balance?to=2026-12-31"), "k", None).await;
    assert_eq!(tb["balanced"], true);
    assert!(tb.to_string().contains("Equity") && tb.to_string().contains("Cash"), "{tb}");
    assert_eq!(ok(&r, "GET", &format!("{base}/company"), "k", None).await["closing_date"], "2026-12-31");

    // Once only, and the imported book and firm ledger are untouched.
    assert_eq!(call(&r, "POST", &format!("{base}/opening-balances"), "k", Some(post)).await.0, 409);
    assert_eq!(ok(&r, "POST", &format!("{base}/opening-balances/preview"), "k", Some(json!({}))).await["posted"], true);
    assert_eq!(ok(&r, "GET", &format!("{base}/entries"), "k", None).await.as_array().unwrap().len(), 1);
    assert_eq!(ok(&r, "GET", "/api/ledger/entries", "k", None).await, json!([]));
    assert_eq!(books.detail("co").await.map_err(|e| e.1).unwrap()["head_sha256"], head);
}

