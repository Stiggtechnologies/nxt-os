//! Synthetic cents only. Flat general ledger and journal-derived payroll view.
use nxt_os::{company_books::{CompanyBooks, models::*}, store::Store};
use serde_json::json;

fn chunk() -> Chunk {
    serde_json::from_value(json!({"accounts":[
        {"id":"bank","name":"Synthetic bank","kind":"asset"},
        {"id":"payx","name":"Payroll Expenses","kind":null},
        {"id":"payl","name":"Payroll Liabilities","kind":null}],
      "entries":[
        {"id":"e1","date":"2023-05-01","source_id":"archive-row:s:1:2","memo":"Cheque","lines":[
          {"account_id":"payx","debit_cents":"100000","credit_cents":"0","source_row":1,"memo":""},
          {"account_id":"bank","debit_cents":"0","credit_cents":"100000","source_row":2,"memo":""}]},
        {"id":"e2","date":"2024-05-01","source_id":"archive-row:s:3:5","memo":"General Journal","lines":[
          {"account_id":"payx","debit_cents":"5000","credit_cents":"0","source_row":3,"memo":""},
          {"account_id":"payl","debit_cents":"0","credit_cents":"2000","source_row":4,"memo":""},
          {"account_id":"bank","debit_cents":"0","credit_cents":"3000","source_row":5,"memo":""}]},
        {"id":"e3","date":"2024-06-01","source_id":"archive-row:s:6:7","memo":"Deposit","lines":[
          {"account_id":"bank","debit_cents":"700","credit_cents":"0","source_row":6,"memo":""},
          {"account_id":"bank","debit_cents":"0","credit_cents":"700","source_row":7,"memo":""}]}]})).unwrap()
}

async fn committed() -> CompanyBooks {
    let s = CompanyBooks::new(Store::memory());
    let c = s.upload("co", chunk()).await.ok().unwrap();
    let h = c["sha256"].as_str().unwrap();
    let tb = |a: &str, d: &str, c: &str, r: u64| json!({"account_id":a,"debit_cents":d,"credit_cents":c,"basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":r}});
    let p: PreviewInput = serde_json::from_value(json!({"request_id":"p","expected_head":null,"company":{"name":"Synthetic","source_company_id":"s","currency":"CAD","identity_verified":true,"currency_verified":true},"source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2023-01-01","period_end":"2024-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},"chunks":[h],
      "trial_balance":[tb("bank","0","103000",1),tb("payx","105000","0",2),tb("payl","0","2000",3)]})).unwrap();
    let pv = s.preview("co", p).await.ok().unwrap();
    s.commit("co", CommitInput{preview_id: pv["preview_id"].as_str().unwrap().into(), expected_head: None, request_id: "c".into(), reviewer_label: "Synthetic reviewer".into(), needs_review: false, review_blockers: vec![], suspense_account_id: None}).await.ok().unwrap();
    s
}

#[tokio::test]
async fn general_ledger_is_flat_named_complete_and_paged() {
    let s = committed().await;
    let all = s.general_ledger_page("co", 0, 1000).await.ok().unwrap();
    assert_eq!(all["lines_total"], 7);
    assert_eq!(all["entries_total"], 3);
    assert_eq!(all["debit_total_cents"], all["credit_total_cents"]);
    assert_eq!(all["rows"][0]["account_name"], "Payroll Expenses");
    assert_eq!(all["rows"][0]["transaction_type"], "Cheque");
    let mut seen = 0; let mut off = 0;
    loop {
        let p = s.general_ledger_page("co", off, 3).await.ok().unwrap();
        seen += p["rows"].as_array().unwrap().len();
        match p["next_offset"].as_u64() { Some(n) => off = n as usize, None => break }
    }
    assert_eq!(seen, 7);
    assert_eq!(s.general_ledger_page("co", 0, 0).await.unwrap_err().0, 400);
    assert_eq!(s.general_ledger_page("co", 0, 1001).await.unwrap_err().0, 400);
}

#[tokio::test]
async fn payroll_view_counts_only_payroll_account_transactions() {
    let s = committed().await;
    let v = s.payroll_view("co").await.ok().unwrap();
    assert_eq!(v["transactions_total"], 2);
    assert_eq!(v["pay_stubs_available"], false);
    assert_eq!(v["by_year"][0]["year"], "2023");
    assert_eq!(v["by_year"][0]["debit_cents"], "100000");
    assert_eq!(v["by_year"][1]["transactions"], 1);
    assert_eq!(v["by_year"][1]["credit_cents"], "2000");
    assert_eq!(v["payroll_accounts"].as_array().unwrap().len(), 2);
}
