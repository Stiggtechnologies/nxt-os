//! XLSX download interoperability checks, independent of the export writer.
use axum::{body::Body, http::{HeaderMap, Request}};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use std::collections::HashMap;
use tower::ServiceExt;

async fn request(router: &axum::Router, method: &str, uri: &str, body: Option<Value>, auth: bool) -> (u16, HeaderMap, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if auth { request = request.header("authorization", "Bearer k"); }
    let request = match body {
        Some(value) => request.header("content-type", "application/json").body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    }.unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024).await.unwrap().to_vec();
    (status, headers, bytes)
}

async fn post(router: &axum::Router, uri: &str, body: Value) -> Value {
    let (status, _, bytes) = request(router, "POST", uri, Some(body), true).await;
    assert_eq!(status, 200, "{uri}: {}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}

fn u16_at(bytes: &[u8], offset: usize) -> usize {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize
}

fn u32_at(bytes: &[u8], offset: usize) -> usize {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize
}

/// Read the generated uncompressed ZIP records using ZIP's public file format.
/// The parent build also opens a downloaded workbook with an independent XLSX
/// reader; this helper checks the HTTP payload without another crate dependency.
fn zip_files(bytes: &[u8]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let mut offset = 0;
    while bytes.get(offset..offset + 4) == Some(b"PK\x03\x04".as_slice()) {
        assert!(offset + 30 <= bytes.len(), "truncated ZIP header");
        assert_eq!(u16_at(bytes, offset + 8), 0, "test reader expects generated store-mode ZIP entries");
        assert_eq!(u16_at(bytes, offset + 6) & 9, 0, "entries must not be encrypted or require data descriptors");
        let size = u32_at(bytes, offset + 18);
        assert_eq!(size, u32_at(bytes, offset + 22));
        let name_len = u16_at(bytes, offset + 26);
        let extra_len = u16_at(bytes, offset + 28);
        let start = offset + 30;
        let content_start = start + name_len + extra_len;
        let content_end = content_start.checked_add(size).unwrap();
        assert!(content_end <= bytes.len(), "truncated ZIP content");
        let name = std::str::from_utf8(&bytes[start..start + name_len]).unwrap().to_string();
        let content = std::str::from_utf8(&bytes[content_start..content_end]).unwrap().to_string();
        assert!(result.insert(name, content).is_none(), "duplicate ZIP member");
        offset = content_end;
    }
    assert_eq!(bytes.get(offset..offset + 4), Some(b"PK\x01\x02".as_slice()), "missing ZIP central directory");
    assert!(bytes.windows(4).any(|window| window == b"PK\x05\x06"), "missing ZIP end record");
    result
}

fn xml_decode(value: &str) -> String {
    value.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

fn cell_values(sheet: &str) -> Vec<(String, bool)> {
    let mut values = Vec::new();
    for fragment in sheet.split("<c ").skip(1) {
        let end = fragment.find("</c>").expect("cell has closing tag");
        let cell = &fragment[..end];
        if let Some(start) = cell.find("<t") {
            let text_start = start + cell[start..].find('>').unwrap() + 1;
            let text_end = text_start + cell[text_start..].find("</t>").unwrap();
            values.push((xml_decode(&cell[text_start..text_end]), false));
        } else if let Some(start) = cell.find("<v>") {
            let text_start = start + 3;
            let text_end = text_start + cell[text_start..].find("</v>").unwrap();
            values.push((xml_decode(&cell[text_start..text_end]), !cell.contains("t=\"s\"") && !cell.contains("t=\"str\"")));
        }
    }
    values
}

#[tokio::test]
async fn native_report_downloads_are_real_safe_workbooks() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);
    let account = post(&router, "/api/ledger/accounts", json!({
        "code": "0012", "name": "Reserve <&> Account", "kind": "asset"
    })).await;
    for (memo, amount) in [("Reviewed, \"receipt\"\nline <&>", 12345), ("=SUM(1,1)", 5)] {
        post(&router, "/api/ledger/entries", json!({
            "date": "2026-10-15", "memo": memo,
            "lines": [{"account_id": account["id"], "debit": amount}, {"account_id": "acc_3000", "credit": amount}]
        })).await;
    }
    let balance = post(&router, "/api/firm/exports", json!({"report": "trial-balance", "to": "2026-10-31"})).await;
    let download = balance["download_url"].as_str().unwrap();
    let (_, csv_headers, csv_before) = request(&router, "GET", download, None, true).await;
    assert!(csv_headers["content-type"].to_str().unwrap().starts_with("text/csv"));
    let xlsx_url = format!("{download}?format=xlsx");
    assert_eq!(request(&router, "GET", &xlsx_url, None, false).await.0, 401);
    let (status, headers, workbook) = request(&router, "GET", &xlsx_url, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
    assert!(headers["content-disposition"].to_str().unwrap().ends_with(".xlsx\""));
    assert_eq!(headers["cache-control"], "no-store");
    assert!(workbook.starts_with(b"PK\x03\x04"));
    let files = zip_files(&workbook);
    for name in ["[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels", "xl/worksheets/sheet1.xml"] {
        assert!(files.contains_key(name), "missing XLSX part {name}");
    }
    let sheet = &files["xl/worksheets/sheet1.xml"];
    assert!(!sheet.contains("<f>"), "report text must not become formulas");
    let values = cell_values(sheet);
    assert!(values.iter().any(|(value, numeric)| value == "0012" && !numeric), "account-code leading zeroes must survive as text");
    assert!(values.iter().filter(|(value, numeric)| value == "12350" && *numeric).count() >= 4, "native debit/credit amounts and totals must be numeric");
    assert!(values.iter().any(|(value, _)| value == "Reserve <&> Account"), "XML escaping must preserve the original name");

    let journal = post(&router, "/api/firm/exports", json!({"report": "journal", "from": "2026-10-01", "to": "2026-10-31"})).await;
    let (_, _, journal_bytes) = request(&router, "GET", &format!("{}?format=xlsx", journal["download_url"].as_str().unwrap()), None, true).await;
    let journal_files = zip_files(&journal_bytes);
    let journal_sheet = &journal_files["xl/worksheets/sheet1.xml"];
    assert!(!journal_sheet.contains("<f>"), "formula-like memo must remain inert text");
    let values = cell_values(journal_sheet);
    assert!(values.iter().any(|(value, numeric)| value == "Reviewed, \"receipt\"\nline <&>" && !numeric), "quoted multiline text must round-trip");
    assert!(values.iter().any(|(value, numeric)| (value == "'=SUM(1,1)" || value == "=SUM(1,1)") && !numeric), "formula-like text must remain a string");

    let (status, _, csv_after) = request(&router, "GET", download, None, true).await;
    assert_eq!(status, 200);
    assert_eq!(csv_before, csv_after, "requesting XLSX must not change the retained CSV artifact");
    assert_eq!(request(&router, "GET", &format!("{download}?format=csv"), None, true).await.2, csv_before);
    let (status, _, bytes) = request(&router, "GET", &format!("{download}?format=pdf"), None, true).await;
    assert_eq!(status, 400);
    assert!(serde_json::from_slice::<Value>(&bytes).unwrap()["error"].is_string());

    // Excel cannot represent arbitrarily large integer cents exactly. Reject
    // an unsafe workbook while preserving the exact native CSV alternative.
    post(&router, "/api/ledger/entries", json!({
        "date": "2026-10-16", "memo": "Synthetic precision boundary",
        "lines": [{"account_id": account["id"], "debit": 1_000_000_000_000_000_i64}, {"account_id": "acc_3000", "credit": 1_000_000_000_000_000_i64}]
    })).await;
    let precise = post(&router, "/api/firm/exports", json!({"report": "trial-balance", "to": "2026-10-31"})).await;
    let precise_url = precise["download_url"].as_str().unwrap();
    let (status, _, exact_csv) = request(&router, "GET", precise_url, None, true).await;
    assert_eq!(status, 200);
    assert!(String::from_utf8(exact_csv).unwrap().contains("1000000000012350"));
    let (status, _, bytes) = request(&router, "GET", &format!("{precise_url}?format=xlsx"), None, true).await;
    assert_eq!(status, 400);
    assert!(serde_json::from_slice::<Value>(&bytes).unwrap()["error"].as_str().unwrap().contains("CSV"));

    let long_memo = "a".repeat(32768);
    post(&router, "/api/ledger/entries", json!({
        "date": "2026-11-01", "memo": long_memo,
        "lines": [{"account_id": account["id"], "debit": 1}, {"account_id": "acc_3000", "credit": 1}]
    })).await;
    let long_text = post(&router, "/api/firm/exports", json!({"report": "journal", "from": "2026-11-01", "to": "2026-11-01"})).await;
    let long_url = long_text["download_url"].as_str().unwrap();
    assert_eq!(request(&router, "GET", long_url, None, true).await.0, 200);
    let (status, _, bytes) = request(&router, "GET", &format!("{long_url}?format=xlsx"), None, true).await;
    assert_eq!(status, 400, "oversized Excel cells must not be silently truncated");
    assert!(serde_json::from_slice::<Value>(&bytes).unwrap()["error"].as_str().unwrap().contains("CSV"));
}
