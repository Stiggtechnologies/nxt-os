//! Synthetic HTTP acceptance for independent NXT tax workpapers.
//! These tests prove the bounded native contract, never filing certification.
use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn raw(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    authenticated: bool,
) -> (u16, String, String, String, String) {
    let mut request = Request::builder().method(method).uri(uri);
    if authenticated { request = request.header("authorization", "Bearer k"); }
    let request = match body {
        Some(value) => request.header("content-type", "application/json").body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    }.unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let csp = response.headers().get("content-security-policy").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let cache_control = response.headers().get("cache-control").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, content_type, csp, cache_control, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let (status, _, _, _, text) = raw(router, method, uri, body, true).await;
    (status, serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw":text})))
}

async fn ok(router: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status, value) = call(router, method, uri, body).await;
    assert_eq!(status, 200, "{method} {uri}: {value}");
    value
}

async fn fails(router: &axum::Router, method: &str, uri: &str, body: Option<Value>, expected: u16) {
    let (status, value) = call(router, method, uri, body).await;
    assert_eq!(status, expected, "{method} {uri}: {value}");
}

fn id(value: &Value) -> &str { value["id"].as_str().expect("record id") }
fn revision(value: &Value) -> u64 { value["revision"].as_u64().expect("revision") }
fn endpoint(value: &Value, suffix: &str) -> String { format!("/api/tax/returns/{}{suffix}", id(value)) }

fn input(program: &str, jurisdiction: &str, inputs: Value) -> Value {
    let mut result = json!({
        "program":program, "tax_year":2025, "jurisdiction":jurisdiction,
        "name":format!("Synthetic {program} acceptance"), "client_ref":"synthetic-only",
        "inputs":inputs, "notes":"No real taxpayer or company data."
    });
    if program == "T2" {
        result["period_start"] = json!("2025-01-01");
        result["period_end"] = json!("2025-12-31");
    }
    result
}

fn editable(record: &Value) -> Value {
    let mut result = json!({});
    for field in ["program", "tax_year", "jurisdiction", "name", "client_ref", "period_start", "period_end", "inputs", "parties", "corporate", "notes"] {
        result[field] = record[field].clone();
    }
    result["expected_revision"] = record["revision"].clone();
    result
}

async fn calculate(router: &axum::Router, record: &Value, key: &str) -> Value {
    ok(router, "POST", &endpoint(record, "/calculate"), Some(json!({
        "expected_revision":revision(record), "idempotency_key":key
    }))).await
}

async fn transition(router: &axum::Router, record: &Value, status: &str, note: &str, reviewer: &str) -> Value {
    ok(router, "POST", &endpoint(record, "/transition"), Some(json!({
        "expected_revision":revision(record), "status":status, "note":note, "reviewer":reviewer
    }))).await
}

/// A single test avoids racing process-global authentication environment setup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_tax_workpaper_lifecycle() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);
    for uri in ["/api/tax/catalog", "/api/tax/returns", "/api/tax/summary", "/api/tax/audit"] {
        assert_eq!(raw(&router, "GET", uri, None, false).await.0, 401);
    }
    let t1_input = input("T1", "AB", json!({"scope_confirmed":true,"full_year_resident":true,"employment_income_cents":5_000_000}));
    assert_eq!(raw(&router, "POST", "/api/tax/returns", Some(t1_input.clone()), false).await.0, 401);
    assert_eq!(ok(&router, "GET", "/api/tax/returns", None).await, json!([]));
    let catalog = ok(&router, "GET", "/api/tax/catalog", None).await;
    assert_eq!(catalog["filing_supported"], false);
    assert_eq!(catalog["certified"], false);
    assert_eq!(catalog["sdk_runtime_required"], false);
    assert_eq!(catalog["programs"].as_array().unwrap().len(), 4);
    let capabilities = ok(&router, "GET", "/api/tax/capabilities", None).await;
    assert_eq!(capabilities["engine"], "independent");
    assert_eq!(capabilities["sdk"]["connected"], false);
    assert_eq!(capabilities["certification"], "not_certified");

    // Four programs perform real native arithmetic with program-specific scope.
    let fixtures = [
        (t1_input.clone(), json!({"taxable_income":5_000_000,"federal_tax":469_800,"provincial_tax":221_416,"income_tax":691_216,"balance":691_216})),
        (input("T2", "AB", json!({"scope_confirmed":true,"taxable_income_cents":10_000_000})), json!({"federal_tax":1_500_000,"provincial_tax":800_000,"income_tax":2_300_000})),
        (input("T3", "CA-FED", json!({"scope_confirmed":true,"income_cents":10_000_000})), json!({"federal_tax":3_300_000,"federal_balance":3_300_000})),
        (input("T5013", "CA", json!({"scope_confirmed":true,"allocation_verified":true,"revenue_cents":101,"partners":[{"id":"a","name":"Synthetic A","share_bps":5000},{"id":"b","name":"Synthetic B","share_bps":5000}]})), json!({"net_income":101,"allocated_income":101,"unallocated_income":0,"partner.a":51,"partner.b":50})),
    ];
    let mut calculated = Vec::new();
    for (source, expected) in fixtures {
        let created = ok(&router, "POST", "/api/tax/returns", Some(source)).await;
        assert_eq!(created["status"], "draft");
        assert_eq!(created["revision"], 1);
        let result = calculate(&router, &created, "initial-native-calculation").await;
        assert_eq!(result["status"], "calculated");
        assert_eq!(result["revision"], 2);
        assert_eq!(result["calculation"]["result"]["supported"], true, "{result}");
        assert_eq!(result["calculation"]["result"]["ruleset"], "nxt-ca-2025.1");
        for (key, amount) in expected.as_object().unwrap() {
            assert_eq!(&result["calculation"]["result"]["totals"][key], amount, "{} {key}", created["program"]);
        }
        if created["program"] == "T3" {
            assert!(result["calculation"]["result"]["totals"].get("income_tax").is_none());
            assert!(result["calculation"]["result"]["totals"].get("provincial_tax").is_none());
        }
        for key in ["input_hash", "output_hash"] {
            let digest = result["calculation"][key].as_str().unwrap();
            assert_eq!(digest.len(), 64);
            assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_eq!(calculate(&router, &created, "initial-native-calculation").await, result, "same request is replay-safe");
        assert_eq!(result["calculations"].as_array().unwrap().len(), 1);
        let mut expected_document = editable(&created);
        expected_document["expected_revision"] = Value::Null;
        assert_eq!(result["calculation"]["input_document"], expected_document, "snapshot retains the complete exact input preimage");
        calculated.push(result);
    }
    assert_eq!(ok(&router, "GET", "/api/tax/returns?program=t2", None).await.as_array().unwrap().len(), 1);
    let list = ok(&router, "GET", "/api/tax/returns", None).await;
    for row in list.as_array().unwrap() {
        for private_field in ["inputs", "parties", "corporate", "notes", "calculations", "reviews", "review"] {
            assert!(row.get(private_field).is_none(), "list must project metadata without {private_field}");
        }
        assert!(row["calculation"].get("input_document").is_none());
        assert!(row["calculation"].get("idempotency_key").is_none());
    }
    let (_, _, _, cache, _) = raw(&router, "GET", "/api/tax/returns", None, true).await;
    assert!(cache.contains("no-store"));
    let (_, _, _, cache, corporate_html) = raw(&router, "GET", &endpoint(&calculated[1], "/print"), None, true).await;
    assert!(cache.contains("no-store"));
    assert!(corporate_html.contains("Workpaper totals"));
    assert!(corporate_html.contains("income_tax"));
    assert!(corporate_html.contains("payments"));
    assert!(corporate_html.contains("balance"));
    assert!(corporate_html.contains("$23000.00"), "printed totals must include the combined tax, not just federal/provincial lines");

    // Review binds exact inputs/output, requires a reviewer note, and locks edits.
    let t1 = calculated[0].clone();
    fails(&router, "POST", &endpoint(&t1, "/transition"), Some(json!({"expected_revision":2,"status":"approved","reviewer":"Reviewer","note":"Skip review"})), 409).await;
    fails(&router, "POST", &endpoint(&t1, "/transition"), Some(json!({"expected_revision":1,"status":"in_review"})), 409).await;
    let in_review = transition(&router, &t1, "in_review", "", "").await;
    fails(&router, "POST", &endpoint(&in_review, "/transition"), Some(json!({"expected_revision":revision(&in_review),"status":"approved","note":"Checked"})), 400).await;
    let approved = transition(&router, &in_review, "approved", "Checked the declared bounded scope and source amounts.", "Synthetic reviewer").await;
    assert_eq!(approved["review"]["input_hash"], approved["calculation"]["input_hash"]);
    assert_eq!(approved["review"]["output_hash"], approved["calculation"]["output_hash"]);
    assert_eq!(approved["reviews"].as_array().unwrap().len(), 1);
    fails(&router, "PUT", &endpoint(&approved, ""), Some(editable(&approved)), 409).await;
    fails(&router, "POST", &endpoint(&approved, "/calculate"), Some(json!({"expected_revision":revision(&approved),"idempotency_key":"locked-new-calculation"})), 409).await;
    fails(&router, "POST", &endpoint(&approved, "/transition"), Some(json!({"expected_revision":revision(&approved),"status":"draft"})), 400).await;
    let reopened = transition(&router, &approved, "draft", "Reopen to reconsider the workpaper.", "").await;
    assert!(reopened["review"].is_null());
    assert!(reopened["calculation"].is_null());
    assert_eq!(reopened["reviews"], approved["reviews"]);
    assert_eq!(reopened["calculations"], approved["calculations"]);
    let recalculated = calculate(&router, &reopened, "reopened-same-inputs").await;
    assert_eq!(recalculated["status"], "calculated", "reopening must allow recalculation without editing inputs");
    assert_eq!(recalculated["calculations"].as_array().unwrap().len(), 2);

    // Concurrent writers cannot overwrite one another. Editing invalidates only
    // active approval/calculation and retains immutable history for inspection.
    let mut change_a = editable(&recalculated);
    change_a["inputs"]["employment_income_cents"] = json!(6_000_000);
    let mut change_b = change_a.clone();
    change_b["notes"] = json!("Concurrent synthetic change");
    let path = endpoint(&recalculated, "");
    let (a, b) = tokio::join!(call(&router, "PUT", &path, Some(change_a)), call(&router, "PUT", &path, Some(change_b)));
    assert!((a.0 == 200 && b.0 == 409) || (a.0 == 409 && b.0 == 200), "CAS must admit exactly one edit: {a:?}, {b:?}");
    let edited = if a.0 == 200 { a.1 } else { b.1 };
    assert_eq!(edited["status"], "draft");
    assert_eq!(edited["input_revision"], 2);
    assert!(edited["calculation"].is_null());
    assert!(edited["review"].is_null());
    assert_eq!(edited["calculations"], recalculated["calculations"]);
    assert_eq!(edited["calculations"][0]["input_document"]["inputs"]["employment_income_cents"], 5_000_000);
    assert_eq!(edited["inputs"]["employment_income_cents"], 6_000_000);
    assert_eq!(edited["reviews"], approved["reviews"]);
    fails(&router, "POST", &endpoint(&edited, "/calculate"), Some(json!({"expected_revision":revision(&edited),"idempotency_key":"initial-native-calculation"})), 409).await;
    fails(&router, "POST", &endpoint(&edited, "/transition"), Some(json!({"expected_revision":revision(&edited),"status":"in_review"})), 409).await;
    let calc_path = endpoint(&edited, "/calculate");
    let request = json!({"expected_revision":revision(&edited),"idempotency_key":"concurrent-calculation"});
    let (a, b) = tokio::join!(call(&router, "POST", &calc_path, Some(request.clone())), call(&router, "POST", &calc_path, Some(request)));
    assert_eq!(a.0, 200, "{a:?}"); assert_eq!(b.0, 200, "{b:?}");
    assert_eq!(a.1["calculation"]["id"], b.1["calculation"]["id"]);
    assert_eq!(a.1["calculations"].as_array().unwrap().len(), 3);

    // Unsupported situations remain visible drafts/workpapers and cannot gain
    // review approval merely because the arithmetic endpoint returned HTTP 200.
    let unsupported = [
        input("T1", "AB", json!({"employment_income_cents":100})),
        input("T1", "ON", json!({"scope_confirmed":true,"full_year_resident":true,"other_tax_situations":true})),
        input("T3", "CA-FED", json!({"scope_confirmed":true,"other_tax_situations":true})),
        json!({"program":"T1","tax_year":2026,"jurisdiction":"AB","name":"Unsupported future scope","inputs":{}}),
        json!({"program":"T2","tax_year":2025,"jurisdiction":"AB","name":"Missing corporate fiscal dates","inputs":{"scope_confirmed":true}}),
    ];
    for source in unsupported {
        let created = ok(&router, "POST", "/api/tax/returns", Some(source)).await;
        let result = calculate(&router, &created, "unsupported-workpaper").await;
        assert_eq!(result["calculation"]["result"]["supported"], false);
        assert!(result["calculation"]["result"]["diagnostics"].as_array().unwrap().iter().any(|d| d["blocks_review"] == true));
        fails(&router, "POST", &endpoint(&result, "/transition"), Some(json!({"expected_revision":revision(&result),"status":"in_review"})), 409).await;
    }
    let invalid = ok(&router, "POST", "/api/tax/returns", Some(input("T1", "AB", json!({"scope_confirmed":true,"full_year_resident":true,"unknown_tax_credit":10})))).await;
    fails(&router, "POST", &endpoint(&invalid, "/calculate"), Some(json!({"expected_revision":1})), 400).await;
    let mut float_input = t1_input.clone(); float_input["inputs"]["employment_income_cents"] = json!(1.25);
    fails(&router, "POST", "/api/tax/returns", Some(float_input), 400).await;
    let mut bad_date = input("T2", "AB", json!({})); bad_date["period_start"] = json!("2025-02-30");
    fails(&router, "POST", "/api/tax/returns", Some(bad_date), 400).await;
    let mut mismatched = input("T2", "AB", json!({"scope_confirmed":true,"fiscal_days":180}));
    mismatched["name"] = json!("Fiscal duration mismatch");
    let mismatched = ok(&router, "POST", "/api/tax/returns", Some(mismatched)).await;
    let mismatch_calc = calculate(&router, &mismatched, "period-mismatch").await;
    assert_eq!(mismatch_calc["calculation"]["result"]["supported"], false);
    assert!(mismatch_calc["calculation"]["result"]["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "fiscal_period"));

    // Native export/import is staged, immutable and retry-safe. Calculations,
    // reviews and external filing claims in an import are never trusted.
    let package = ok(&router, "GET", &endpoint(&approved, "/package"), None).await;
    assert_eq!(package["format"], "nxt-tax-return/v1");
    assert_eq!(package["filing_enabled"], false);
    let before = ok(&router, "GET", "/api/tax/returns", None).await.as_array().unwrap().len();
    let mut forged = package.clone();
    forged["review"] = json!({"reviewer":"Forged","note":"Pretend approved"});
    forged["calculation_snapshot"] = json!({"result":{"supported":true,"totals":{"income_tax":0}}});
    forged["filing_enabled"] = json!(true); forged["status"] = json!("accepted");
    let preview = ok(&router, "POST", "/api/tax/import/preview", Some(json!({"document":forged}))).await;
    assert!(preview.get("document").is_none());
    assert!(!preview["warnings"].as_array().unwrap().is_empty());
    assert_eq!(preview["source_hash"].as_str().unwrap().len(), 64);
    assert_eq!(ok(&router, "GET", "/api/tax/returns", None).await.as_array().unwrap().len(), before);
    let commit_request = json!({"preview_id":id(&preview)});
    let imported = ok(&router, "POST", "/api/tax/import/commit", Some(commit_request.clone())).await;
    assert_ne!(imported["id"], approved["id"]);
    assert_eq!(imported["status"], "draft");
    assert_eq!(imported["revision"], 1);
    assert!(imported["calculation"].is_null()); assert!(imported["review"].is_null());
    assert_eq!(imported["calculations"], json!([])); assert_eq!(imported["reviews"], json!([]));
    assert_eq!(ok(&router, "POST", "/api/tax/import/commit", Some(commit_request)).await, imported);
    assert_eq!(ok(&router, "GET", "/api/tax/returns", None).await.as_array().unwrap().len(), before + 1);
    fails(&router, "POST", "/api/tax/import/preview", Some(json!({"document":{"format":"dtmax-xml","return":t1_input}})), 400).await;
    fails(&router, "POST", "/api/tax/import/commit", Some(json!({"preview_id":"missing-preview"})), 400).await;
    let mut malformed_package = package; malformed_package["return"]["unrecognized_field"] = json!(true);
    fails(&router, "POST", "/api/tax/import/preview", Some(json!({"document":malformed_package})), 400).await;

    // Rollforward carries profile metadata, never last year's dollars, approval
    // or apparent support for a year without an implemented ruleset.
    let mut with_party = input("T5013", "CA", json!({"scope_confirmed":true,"allocation_verified":true,"revenue_cents":100,"partners":[{"id":"a","name":"Synthetic A","share_bps":10000}]}));
    with_party["parties"] = json!([{"role":"partner","name":"Synthetic partner metadata","allocation_cents":100,"share_basis_points":10000}]);
    let prior = ok(&router, "POST", "/api/tax/returns", Some(with_party)).await;
    let prior = calculate(&router, &prior, "rollforward-source").await;
    fails(&router, "POST", &endpoint(&prior, "/rollforward"), Some(json!({"expected_revision":revision(&prior),"tax_year":2027})), 400).await;
    let next = ok(&router, "POST", &endpoint(&prior, "/rollforward"), Some(json!({"expected_revision":revision(&prior),"tax_year":2026}))).await;
    assert_eq!(next["rollforward_from"], prior["id"]);
    assert_eq!(next["inputs"], json!({}));
    assert_eq!(next["status"], "draft"); assert_eq!(next["revision"], 1);
    assert_eq!(next["parties"][0]["allocation_cents"], 0);
    assert_ne!(next["parties"][0]["id"], prior["parties"][0]["id"]);
    assert_eq!(next["parties"][0]["name"], prior["parties"][0]["name"]);
    assert!(next["calculation"].is_null()); assert!(next["review"].is_null());
    assert_eq!(next["calculations"], json!([])); assert_eq!(next["reviews"], json!([]));
    assert_eq!(ok(&router, "GET", &endpoint(&prior, ""), None).await, prior);
    fails(&router, "POST", &endpoint(&prior, "/rollforward"), Some(json!({"expected_revision":revision(&prior),"tax_year":2026})), 409).await;
    assert_eq!(calculate(&router, &next, "future-year-draft").await["calculation"]["result"]["supported"], false);

    // Printable output escapes user-controlled text; every route still forbids
    // treating an internal workpaper as transmitted or authority-accepted.
    let mut injection = editable(&imported);
    injection["name"] = json!("Synthetic <script>alert('name')</script> & <b>tax</b>");
    injection["notes"] = json!("<img src=x onerror=alert('notes')>");
    let injection = ok(&router, "PUT", &endpoint(&imported, ""), Some(injection)).await;
    let (status, content_type, csp, cache, html) = raw(&router, "GET", &endpoint(&injection, "/print"), None, true).await;
    assert_eq!(status, 200); assert!(content_type.starts_with("text/html"));
    assert!(cache.contains("no-store"));
    assert!(csp.contains("default-src 'none'"));
    assert!(html.contains("&lt;script&gt;")); assert!(html.contains("&lt;img"));
    assert!(!html.contains("<script>")); assert!(!html.contains("<img src=x"));
    assert!(html.contains("Not a certified tax return or filing"));
    fails(&router, "POST", &endpoint(&injection, "/filing"), None, 409).await;
    fails(&router, "POST", &endpoint(&injection, "/transition"), Some(json!({"expected_revision":revision(&injection),"status":"filed"})), 400).await;
    let archived = transition(&router, &injection, "archived", "Synthetic acceptance complete.", "").await;
    fails(&router, "PUT", &endpoint(&archived, ""), Some(editable(&archived)), 409).await;
    let summary = ok(&router, "GET", "/api/tax/summary", None).await;
    assert_eq!(summary["filing_enabled"], false); assert_eq!(summary["auth_required"], true);
    assert_eq!(summary["calculation_years"], json!([2025]));
    let audit = ok(&router, "GET", "/api/tax/audit", None).await;
    for action in ["created", "calculated", "edited", "transition", "imported", "rolled_forward"] {
        assert!(audit.as_array().unwrap().iter().any(|e| e["action"] == action), "missing {action} audit event");
    }
    for reason in ["Reopen to reconsider the workpaper.", "Synthetic acceptance complete."] {
        assert!(audit.as_array().unwrap().iter().any(|e| e["action"] == "transition" && e["detail"].as_str().unwrap().contains(reason)), "transition audit must retain its reason: {reason}");
    }
    fails(&router, "GET", "/api/tax/audit?limit=201", None, 400).await;
    let audit_page = ok(&router, "GET", "/api/tax/audit?offset=1&limit=2", None).await;
    assert_eq!(audit_page, json!([audit[1].clone(), audit[2].clone()]));
    corporate_profile_lifecycle(&router).await;
}

fn corporate_profile() -> Value {
    // Deliberately omit BN/RC identifiers: no real company identifier is needed
    // to verify financial reconciliation or snapshot provenance.
    json!({
        "legal_name":"Entirely synthetic tax test corporation", "corporation_type":"ccpc",
        "province":"AB", "address":"Synthetic address used only by acceptance tests",
        "officer_name":"Synthetic test officer", "officer_title":"Test preparer",
        "reconciliation_confirmed":true,
        "gifi":[
            {"section":"balance_sheet","code":"2599","current_cents":10_000_000,"prior_cents":8_000_000},
            {"section":"balance_sheet","code":"3499","current_cents":3_000_000,"prior_cents":2_000_000},
            {"section":"balance_sheet","code":"3620","current_cents":7_000_000,"prior_cents":6_000_000},
            {"section":"income_statement","code":"8299","current_cents":20_000_000,"prior_cents":15_000_000},
            {"section":"income_statement","code":"9368","current_cents":10_000_000,"prior_cents":8_000_000},
            {"section":"income_statement","code":"9970","current_cents":10_000_000,"prior_cents":7_000_000},
            {"section":"income_statement","code":"9990","current_cents":2_300_000,"prior_cents":1_610_000},
            {"section":"income_statement","code":"9999","current_cents":7_700_000,"prior_cents":5_390_000}
        ],
        "adjustments":[{"id":"tax-provision","label":"Verified income-tax provision addback","amount_cents":2_300_000}],
        "notes":"Synthetic reconciliation; no corporate registration or filing claim."
    })
}

async fn corporate_profile_lifecycle(router: &axum::Router) {
    let mut source = input("T2", "AB", json!({
        "scope_confirmed":true, "taxable_income_cents":10_000_000,
        "active_business_income_cents":10_000_000, "small_business_eligible":false
    }));
    source["corporate"] = corporate_profile();
    let mut wrong_program = input("T1", "AB", json!({}));
    wrong_program["corporate"] = corporate_profile();
    fails(router, "POST", "/api/tax/returns", Some(wrong_program), 400).await;
    let created = ok(router, "POST", "/api/tax/returns", Some(source)).await;
    assert_eq!(created["corporate"]["legal_name"], "Entirely synthetic tax test corporation");
    let calculated = calculate(router, &created, "corporate-initial").await;
    let analysis = &calculated["calculation"]["result"]["corporate_analysis"];
    assert_eq!(calculated["calculation"]["result"]["supported"], true, "{calculated}");
    assert_eq!(analysis["supported"], true);
    assert_eq!(analysis["totals"]["book_income"], 7_700_000);
    assert_eq!(analysis["totals"]["adjustments"], 2_300_000);
    assert_eq!(analysis["totals"]["reconciled_taxable_income"], 10_000_000);
    assert_eq!(analysis["reconciliation"]["book_to_tax"]["source_code"], "9999");
    assert_eq!(analysis["reconciliation"]["book_to_tax"]["matches"], true);
    assert_eq!(analysis["reconciliation"]["balance_sheets"][0]["balanced"], true);
    assert_eq!(calculated["calculation"]["input_document"]["corporate"], created["corporate"]);
    let in_review = transition(router, &calculated, "in_review", "", "").await;
    let approved = transition(router, &in_review, "approved", "Checked synthetic financial totals and explicit signed tax adjustments.", "Synthetic corporate reviewer").await;
    let reopened = transition(router, &approved, "draft", "Reopen synthetic corporate profile for a financial correction.", "").await;
    let mut unbalanced = editable(&reopened);
    unbalanced["corporate"]["legal_name"] = json!("Changed synthetic corporate profile");
    unbalanced["corporate"]["gifi"][0]["current_cents"] = json!(10_000_001);
    let changed = ok(router, "PUT", &endpoint(&reopened, ""), Some(unbalanced)).await;
    assert_eq!(changed["status"], "draft");
    assert!(changed["calculation"].is_null()); assert!(changed["review"].is_null());
    assert_eq!(changed["calculations"][0]["input_document"]["corporate"], created["corporate"], "historical profile must not change with the current profile");
    assert_eq!(changed["reviews"], approved["reviews"]);
    let blocked = calculate(router, &changed, "corporate-unbalanced").await;
    assert_eq!(blocked["calculation"]["result"]["supported"], false);
    assert!(blocked["calculation"]["result"]["corporate_analysis"]["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "gifi_unbalanced"));
    fails(router, "POST", &endpoint(&blocked, "/transition"), Some(json!({"expected_revision":revision(&blocked),"status":"in_review"})), 409).await;
    let mut repair = editable(&blocked);
    repair["corporate"]["gifi"][0]["current_cents"] = json!(10_000_000);
    let repair = ok(router, "PUT", &endpoint(&blocked, ""), Some(repair)).await;
    let repaired = calculate(router, &repair, "corporate-repaired").await;
    assert_eq!(repaired["calculation"]["result"]["supported"], true);
    assert_ne!(repaired["calculation"]["input_hash"], calculated["calculation"]["input_hash"], "profile metadata is bound into the input hash");
    assert_eq!(repaired["calculation"]["input_document"]["corporate"], repaired["corporate"]);

    let mut package = ok(router, "GET", &endpoint(&repaired, "/package"), None).await;
    assert_eq!(package["return"]["corporate"], repaired["corporate"]);
    package["calculation_snapshot"] = json!({"result":{"supported":true,"corporate_analysis":{"supported":true}}});
    package["review"] = json!({"reviewer":"Untrusted importer","note":"Pretend this is approved"});
    let preview = ok(router, "POST", "/api/tax/import/preview", Some(json!({"document":package}))).await;
    let imported = ok(router, "POST", "/api/tax/import/commit", Some(json!({"preview_id":id(&preview)}))).await;
    assert_eq!(imported["corporate"], repaired["corporate"]);
    assert_eq!(imported["status"], "draft");
    assert!(imported["calculation"].is_null()); assert!(imported["review"].is_null());
    assert_eq!(imported["calculations"], json!([]));
    assert_eq!(imported["reviews"], json!([]));
    let next = ok(router, "POST", &endpoint(&repaired, "/rollforward"), Some(json!({"expected_revision":revision(&repaired),"tax_year":2026}))).await;
    for field in ["legal_name", "corporation_type", "province", "address", "officer_name"] {
        assert_eq!(next["corporate"][field], repaired["corporate"][field], "rollforward preserves profile {field}");
    }
    assert_eq!(next["inputs"], json!({}));
    assert_eq!(next["corporate"]["gifi"], json!([]));
    assert_eq!(next["corporate"]["adjustments"], json!([]));
    assert_eq!(next["corporate"]["reconciliation_confirmed"], false);
    assert_eq!(next["corporate"]["notes"], "");
    assert!(next["calculation"].is_null()); assert!(next["review"].is_null());
    assert_eq!(next["calculations"], json!([]));
    let listed = ok(router, "GET", "/api/tax/returns?program=T2", None).await;
    assert!(listed.as_array().unwrap().iter().all(|r| r.get("corporate").is_none() && r.get("inputs").is_none() && r.get("calculations").is_none()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_source_lines_and_ontario_family_do_not_file_or_transfer_credits() {
    unsafe { std::env::set_var("ADMIN_API_KEY", "k") };
    let router = app_with(Store::memory);
    let inputs = json!({"scope_confirmed":true,"full_year_resident":true,"interest_income_cents":4_000_000});
    let created = ok(&router, "POST", "/api/tax/returns", Some(input("T1", "ON", inputs.clone()))).await;
    let calculated = calculate(&router, &created, "on-base").await;
    assert_eq!(calculated["calculation"]["result"]["supported"], true);
    assert_eq!(calculated["calculation"]["result"]["totals"]["provincial_tax"], 182_628);
    let compare = ok(&router, "POST", &endpoint(&calculated, "/parallel-compare"), Some(json!({
        "source_system": "webtax",
        "program": "T1",
        "tax_year": 2025,
        "jurisdiction": "ON",
        "lines": [
            {"code":"26000","amount_cents":4_000_000},
            {"code":"42800","amount_cents":182_628},
            {"code":"VENDOR_ONLY","amount_cents":1}
        ]
    }))).await;
    assert_eq!(compare["authoritative_system"], "webtax");
    assert_eq!(compare["filing_enabled"], false);
    assert_eq!(compare["certification"], "not_claimed");
    assert_eq!(compare["write_back"], false);
    assert_eq!(compare["paired_to_vendor_bytes"], false);
    assert_eq!(compare["matches"], 2);
    assert!(compare["rows"].as_array().unwrap().iter().any(|row| row["code"]=="VENDOR_ONLY" && row["status"]=="only_source"));
    fails(&router, "POST", &endpoint(&calculated, "/filing"), None, 409).await;
    let spouse = ok(&router, "POST", "/api/tax/returns", Some(input("T1", "AB", inputs))).await;
    let spouse = calculate(&router, &spouse, "ab-spouse").await;
    let family = ok(&router, "POST", "/api/tax/families", Some(json!({
        "label": "Synthetic pair",
        "primary_return_id": id(&calculated),
        "spouse_return_id": id(&spouse)
    }))).await;
    assert_eq!(family["credit_transfer"], false);
    assert_eq!(family["optimization"], "not_performed");
    assert_eq!(family["filing_enabled"], false);
    assert!(family["primary"]["totals"]["provincial_tax"].as_i64().unwrap() != family["spouse"]["totals"]["provincial_tax"].as_i64().unwrap());
    let again = ok(&router, "GET", &format!("/api/tax/families/{}", family["id"].as_str().unwrap()), None).await;
    assert_eq!(again["id"], family["id"]);
}
