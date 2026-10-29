//! Synthetic source archives only. No source-system connection or accounting import.
use axum::{Router, body::Body, http::Request};
use nxt_os::{
    app_with,
    migrations::{
        Migrations,
        models::{ChunkInput, SnapshotManifest},
    },
    store::Store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
async fn request(
    router: &Router,
    method: &str,
    url: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, String, String, String, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(url);
    if auth {
        request = request.header("authorization", "Bearer k");
    }
    let body = if let Some(value) = body {
        request = request.header("content-type", "application/json");
        Body::from(value.to_string())
    } else {
        Body::empty()
    };
    let response = router
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let header = |key: &str| {
        response
            .headers()
            .get(key)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let content_type = header("content-type");
    let cache = header("cache-control");
    let sniff = header("x-content-type-options");
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, content_type, cache, sniff, bytes)
}
async fn call(
    router: &Router,
    method: &str,
    url: &str,
    body: Option<Value>,
    expected: u16,
) -> Value {
    let (status, _, cache, sniff, bytes) = request(router, method, url, body, true).await;
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    assert_eq!(status, expected, "{method} {url}: {value}");
    if url.starts_with("/api/migrations") {
        assert!(cache.contains("no-store"));
        assert_eq!(sniff, "nosniff");
    }
    value
}
fn fixture() -> (Value, Vec<(String, String)>, Vec<u8>) {
    let archive = b"PK\x03\x04synthetic immutable original\x00archive".to_vec();
    let archive_sha = sha(&archive);
    let source_report_sha = sha(b"synthetic source xlsx bytes");
    let rows=json!({
        "format":"nxt-shadow-report-rows/v1","cohort_ref":"synthetic-cohort-A",
        "source_archive_sha256":archive_sha,"source_report_sha256":source_report_sha,"report_id":"journal",
        "rows":[
            [2,[["A","s","Synthetic & <b>name</b>",null,null],["B","n","",null,null]]],
            [3,[["A","inlineStr","zero",null,null],["B","n","0",null,"0"]]],
            [7,[["A","str","<img src=x onerror=alert(1)>","HYPERLINK(\"javascript:alert(1)\")",null],["B","n","1.25",null,"125"]]]
        ]
    }).to_string();
    let row_sha = sha(rows.as_bytes());
    let archive_hex = hex::encode(&archive);
    let chunks = vec![
        (sha(archive_hex[..20].as_bytes()), archive_hex[..20].into()),
        (sha(archive_hex[20..].as_bytes()), archive_hex[20..].into()),
        (row_sha.clone(), rows),
    ];
    let manifest = json!({
        "format":"nxt-shadow-snapshot/v1","id":"synthetic-shadow-A","cohort_ref":"synthetic-cohort-A",
        "company_name":"Synthetic company A","source_system":"quickbooks","captured_at":"2026-10-26T11:00:00Z",
        "cutoff_date":"2026-10-26","reviewer_label":"Synthetic reviewer label","source_of_truth":"original_system","mode":"parallel_test",
        "source_archive":{"file_name":"synthetic-export.zip","sha256":archive_sha,"bytes":archive.len(),"encoding":"hex","chunks":[chunks[0].0,chunks[1].0]},
        "reports":[{"id":"journal","title":"Synthetic Journal","sheet_name":"Sheet1","source_sha256":source_report_sha,"source_bytes":27,
            "columns":["A","B"],"row_count":3,"parts":[{"sha256":row_sha,"row_start":2,"row_end":7,"row_count":3}]}],
        "exceptions":[{"report_id":"journal","row":2,"code":"source_blank_debit_credit","note":"The source value is blank. No zero or void status is inferred."}],
        "limitations":["Synthetic source rows are an operator-supplied decoded view; no financial posting or source-row authenticity approval."]
    });
    (manifest, chunks, archive)
}
async fn upload(router: &Router, chunks: &[(String, String)]) {
    for (hash, content) in chunks {
        call(
            router,
            "POST",
            "/api/migrations/chunks",
            Some(json!({"sha256":hash,"content":content})),
            200,
        )
        .await;
    }
}

fn roster_fixture(client: usize) -> (Value, Vec<(String, String)>, Vec<u8>) {
    let (mut manifest, mut chunks, original) = fixture();
    let cohort = format!("synthetic-roster-{client:03}");
    manifest["id"] = json!(format!("synthetic-shadow-{client:03}"));
    manifest["cohort_ref"] = json!(cohort);
    manifest["company_name"] = json!(format!("Synthetic roster company {client:03}"));
    let mut rows: Value = serde_json::from_str(&chunks[2].1).unwrap();
    rows["cohort_ref"] = manifest["cohort_ref"].clone();
    chunks[2].1 = rows.to_string();
    chunks[2].0 = sha(chunks[2].1.as_bytes());
    manifest["reports"][0]["parts"][0]["sha256"] = json!(chunks[2].0);
    (manifest, chunks, original)
}

#[tokio::test]
async fn firm_roster_can_publish_154_separately_bound_read_only_snapshots() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
    }
    let router = app_with(Store::memory);
    let ledger_before = call(&router, "GET", "/api/ledger/entries", None, 200).await;
    let tax_before = call(&router, "GET", "/api/tax/returns", None, 200).await;
    let (_, archive_chunks, _) = fixture();
    upload(&router, &archive_chunks[..2]).await;
    for client in 0..154 {
        let (manifest, chunks, _) = roster_fixture(client);
        upload(&router, &chunks[2..]).await;
        let result = call(
            &router,
            "POST",
            "/api/migrations/snapshots",
            Some(manifest.clone()),
            200,
        )
        .await;
        assert_eq!(result["manifest"], manifest);
        assert_eq!(result["replayed"], false);
        assert_eq!(result["source_authoritative"], true);
        assert_eq!(result["live_cutover_allowed"], false);
    }
    let (_, _, _, _, listed_bytes) = request(&router, "GET", "/api/migrations", None, true).await;
    assert!(listed_bytes.len() <= 1024 * 1024);
    let listed: Value = serde_json::from_slice(&listed_bytes).unwrap();
    let summaries = listed["snapshots"].as_array().unwrap();
    assert_eq!(summaries.len(), 154);
    assert!(
        summaries
            .iter()
            .all(|summary| summary.get("manifest").is_none())
    );
    for client in [0, 20, 153] {
        let (manifest, _, original) = roster_fixture(client);
        let id = manifest["id"].as_str().unwrap();
        let detail = call(&router, "GET", &format!("/api/migrations/{id}"), None, 200).await;
        assert_eq!(detail["manifest"], manifest);
        let rows = call(
            &router,
            "GET",
            &format!("/api/migrations/{id}/reports/journal/rows?limit=3"),
            None,
            200,
        )
        .await;
        assert_eq!(rows["rows"].as_array().unwrap().len(), 3);
        let (status, _, _, _, archive) = request(
            &router,
            "GET",
            &format!("/api/migrations/{id}/archive"),
            None,
            true,
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(archive, original);
        let replay = call(
            &router,
            "POST",
            "/api/migrations/snapshots",
            Some(manifest),
            200,
        )
        .await;
        assert_eq!(replay["replayed"], true);
    }
    assert_eq!(
        call(&router, "GET", "/api/migrations", None, 200).await["snapshots"]
            .as_array()
            .unwrap()
            .len(),
        154,
    );
    assert_eq!(
        call(&router, "GET", "/api/ledger/entries", None, 200).await,
        ledger_before
    );
    assert_eq!(
        call(&router, "GET", "/api/tax/returns", None, 200).await,
        tax_before
    );
}

#[tokio::test]
async fn full_catalog_keeps_capacity_replay_and_manifest_integrity_guards() {
    let service = Migrations::new(Store::memory());
    let (fixture_manifest, _, _) = fixture();
    let mut records = Vec::new();
    for client in 0..500 {
        let mut manifest = fixture_manifest.clone();
        manifest["id"] = json!(format!("synthetic-capacity-{client:03}"));
        // Maximal escaped display metadata still fits the summary response bound.
        manifest["company_name"] = json!("\"".repeat(240));
        manifest["reviewer_label"] = json!("\\".repeat(240));
        manifest["cohort_ref"] = json!("\"".repeat(128));
        manifest["limitations"] = json!(["Bounded synthetic catalog metadata. ".repeat(100)]);
        let manifest: SnapshotManifest = serde_json::from_value(manifest).unwrap();
        records.push(json!({
            "manifest_sha256": sha(&serde_json::to_vec(&manifest).unwrap()),
            "manifest": manifest,
            "published_at": "2026-10-26T11:00:00Z",
        }));
    }
    let catalog = json!({"snapshots": records});
    let original = catalog.to_string();
    assert!(original.len() > 2 * 1024 * 1024);
    assert!(original.len() <= 8 * 1024 * 1024);
    assert!(json!({"result": original}).to_string().len() < 24 * 1024 * 1024);
    service
        .store
        .set("migrations:catalog:v1", &original)
        .await
        .unwrap();
    let listed = service
        .list()
        .await
        .unwrap_or_else(|_| panic!("valid full catalog must list"));
    assert_eq!(listed["snapshots"].as_array().unwrap().len(), 500);
    assert!(serde_json::to_vec(&listed).unwrap().len() <= 1024 * 1024);
    let existing: SnapshotManifest =
        serde_json::from_value(catalog["snapshots"][0]["manifest"].clone()).unwrap();
    assert_eq!(
        service
            .publish(existing.clone())
            .await
            .unwrap_or_else(|_| panic!("identical manifest must replay at capacity"))["replayed"],
        true
    );
    let mut changed = existing.clone();
    changed.company_name = "Changed immutable company".into();
    assert_eq!(
        service.publish(changed).await.err().unwrap().0.as_u16(),
        409
    );
    let mut next = existing;
    next.id = "synthetic-capacity-overflow".into();
    let failure = service.publish(next).await.err().unwrap();
    assert_eq!(failure.0.as_u16(), 409);
    assert!(failure.1.contains("(500)"));
    assert_eq!(
        service.store.get("migrations:catalog:v1").await.unwrap(),
        Some(original)
    );

    // An intact early record cannot hide corruption later in the expanded catalog.
    let mut corrupt = catalog.clone();
    corrupt["snapshots"][499]["manifest"]["company_name"] = json!("Tampered company");
    service
        .store
        .set("migrations:catalog:v1", &corrupt.to_string())
        .await
        .unwrap();
    assert_eq!(
        service
            .detail("synthetic-capacity-000")
            .await
            .err()
            .unwrap()
            .0
            .as_u16(),
        409
    );
    let mut invalid = catalog.clone();
    invalid["snapshots"][499]["manifest"]["source_of_truth"] = json!("nxt");
    service
        .store
        .set("migrations:catalog:v1", &invalid.to_string())
        .await
        .unwrap();
    assert_eq!(service.list().await.err().unwrap().0.as_u16(), 409);
    let mut duplicate = catalog.clone();
    duplicate["snapshots"][499] = duplicate["snapshots"][0].clone();
    service
        .store
        .set("migrations:catalog:v1", &duplicate.to_string())
        .await
        .unwrap();
    assert_eq!(service.list().await.err().unwrap().0.as_u16(), 409);
    let mut overflow = catalog;
    let extra = overflow["snapshots"][0].clone();
    overflow["snapshots"].as_array_mut().unwrap().push(extra);
    service
        .store
        .set("migrations:catalog:v1", &overflow.to_string())
        .await
        .unwrap();
    assert_eq!(service.list().await.err().unwrap().0.as_u16(), 409);
}

#[tokio::test]
async fn parallel_source_archives_are_immutable_scoped_and_never_financial_writes() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
    }
    let router = app_with(Store::memory);
    let (status, _, cache, sniff, _) =
        request(&router, "GET", "/api/migrations", None, false).await;
    assert_eq!(status, 401);
    assert!(cache.contains("no-store"));
    assert_eq!(sniff, "nosniff");
    let empty = call(&router, "GET", "/api/migrations", None, 200).await;
    assert_eq!(empty["snapshots"], json!([]));
    assert_eq!(empty["shadow_only"], true);
    assert_eq!(empty["live_cutover_allowed"], false);
    let ledger_before = call(&router, "GET", "/api/ledger/entries", None, 200).await;
    let tax_before = call(&router, "GET", "/api/tax/returns", None, 200).await;
    let firm_before = call(&router, "GET", "/api/firm/projects", None, 200).await;
    let engagements_before = call(&router, "GET", "/api/engagements", None, 200).await;
    let (manifest, chunks, original) = fixture();
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(manifest.clone()),
        409,
    )
    .await;
    call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A",
        None,
        404,
    )
    .await;
    upload(&router, &chunks[..2]).await;
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(manifest.clone()),
        409,
    )
    .await;
    assert_eq!(
        call(&router, "GET", "/api/migrations", None, 200).await["snapshots"],
        json!([]),
        "Partial staged content must not publish a snapshot"
    );
    let wrong = json!({"sha256":chunks[2].0,"content":"changed source row content"});
    call(&router, "POST", "/api/migrations/chunks", Some(wrong), 400).await;
    upload(&router, &chunks[2..]).await;
    let replay = call(
        &router,
        "POST",
        "/api/migrations/chunks",
        Some(json!({"sha256":chunks[2].0,"content":chunks[2].1})),
        200,
    )
    .await;
    assert_eq!(replay["replayed"], true);
    let published = call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(manifest.clone()),
        200,
    )
    .await;
    assert_eq!(published["replayed"], false);
    assert_eq!(published["manifest"], manifest);
    assert_eq!(published["source_authoritative"], true);
    assert_eq!(published["live_cutover_allowed"], false);
    assert!(
        published["identity_limit"]
            .as_str()
            .unwrap()
            .contains("not verified")
    );
    let repeat = call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(manifest.clone()),
        200,
    )
    .await;
    assert_eq!(repeat["replayed"], true);
    assert_eq!(repeat["manifest_sha256"], published["manifest_sha256"]);
    assert_eq!(repeat["published_at"], published["published_at"]);
    let mut changed = manifest.clone();
    changed["company_name"] = json!("Changed company label");
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(changed),
        409,
    )
    .await;
    let detail = call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A",
        None,
        200,
    )
    .await;
    assert_eq!(detail["manifest"], manifest);
    assert_eq!(detail["manifest_sha256"], published["manifest_sha256"]);
    let page = call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/reports/journal/rows?from_row=1&limit=1",
        None,
        200,
    )
    .await;
    assert_eq!(page["rows"][0][0], 2);
    assert_eq!(page["rows"][0][1][1][2], "");
    assert!(page["rows"][0][1][1][4].is_null());
    assert_eq!(page["next_from_row"], 3);
    assert_eq!(page["has_more"], true);
    // Vercel's wildcard rewrite appends a reserved path query parameter.
    // Its normalized value must identify this exact snapshot/report route;
    // accepting it must not relax pagination or other query-field validation.
    for rewrite_path in [
        "migrations%2Fsynthetic-shadow-A%2Freports%2Fjournal%2Frows",
        "%2Fmigrations%2Fsynthetic-shadow-A%2Freports%2Fjournal%2Frows",
        "api%2Fmigrations%2Fsynthetic-shadow-A%2Freports%2Fjournal%2Frows",
        "%2Fapi%2Fmigrations%2Fsynthetic-shadow-A%2Freports%2Fjournal%2Frows",
    ] {
        let url = format!(
            "/api/migrations/synthetic-shadow-A/reports/journal/rows?from_row=1&limit=1&path={rewrite_path}"
        );
        let rewritten = call(&router, "GET", &url, None, 200).await;
        assert_eq!(rewritten, page, "rewrite path must preserve the exact page");
    }
    for query in [
        "path=migrations%2Fother-snapshot%2Freports%2Fjournal%2Frows",
        "path=migrations%2Fsynthetic-shadow-A%2Freports%2Fother-report%2Frows",
        "path=migrations%2Fsynthetic-shadow-A%2Farchive",
        "path=migrations%2Fsynthetic-shadow-A%2Freports%2Fjournal%2Frows%2Fextra",
        "unexpected=1",
        "path=migrations%2Fsynthetic-shadow-A%2Freports%2Fjournal%2Frows&unexpected=1",
    ] {
        let url = format!(
            "/api/migrations/synthetic-shadow-A/reports/journal/rows?from_row=1&limit=1&{query}"
        );
        call(&router, "GET", &url, None, 400).await;
    }
    let zeros = call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/reports/journal/rows?from_row=3&limit=1",
        None,
        200,
    )
    .await;
    assert_eq!(zeros["rows"][0][1][1][2], "0");
    assert_eq!(zeros["rows"][0][1][1][4], "0");
    assert_eq!(zeros["next_from_row"], 7);
    let (status, content_type, _, _, bytes) = request(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/reports/journal/rows?from_row=7&limit=100",
        None,
        true,
    )
    .await;
    assert_eq!(status, 200);
    assert!(content_type.starts_with("application/json"));
    let last: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(last["rows"][0][1][0][2], "<img src=x onerror=alert(1)>");
    assert_eq!(
        last["rows"][0][1][0][3],
        "HYPERLINK(\"javascript:alert(1)\")"
    );
    assert_eq!(last["has_more"], false);
    let (status, content_type, cache, sniff, bytes) = request(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/archive",
        None,
        true,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(content_type, "application/zip");
    assert!(cache.contains("no-store"));
    assert_eq!(sniff, "nosniff");
    assert_eq!(bytes, original);
    let listed = call(&router, "GET", "/api/migrations", None, 200).await;
    assert_eq!(listed["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(listed["snapshots"][0]["exception_count"], 1);
    assert!(listed["snapshots"][0].get("manifest").is_none());
    call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/reports/other/rows",
        None,
        404,
    )
    .await;
    call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/reports/journal/rows?limit=201",
        None,
        400,
    )
    .await;
    call(
        &router,
        "GET",
        "/api/migrations/synthetic-shadow-A/reports/journal/rows?from_row=0",
        None,
        400,
    )
    .await;
    assert_eq!(
        request(
            &router,
            "POST",
            "/api/migrations/synthetic-shadow-A/approve",
            Some(json!({})),
            true
        )
        .await
        .0,
        404
    );
    assert_eq!(
        call(&router, "GET", "/api/ledger/entries", None, 200).await,
        ledger_before
    );
    assert_eq!(
        call(&router, "GET", "/api/tax/returns", None, 200).await,
        tax_before
    );
    assert_eq!(
        call(&router, "GET", "/api/firm/projects", None, 200).await,
        firm_before
    );
    assert_eq!(
        call(&router, "GET", "/api/engagements", None, 200).await,
        engagements_before
    );

    // Wrong cohort, original archive, report source or ordered range cannot be
    // relinked to an existing source-row chunk even with a new snapshot ID.
    for (key, value) in [
        ("cohort_ref", json!("synthetic-cohort-B")),
        ("mode", json!("live")),
        ("source_of_truth", json!("nxt")),
        ("approve", json!(true)),
    ] {
        let mut invalid = manifest.clone();
        invalid["id"] = json!(format!("invalid-{key}"));
        invalid[key] = value;
        let expected = if key == "approve" { 422 } else { 400 };
        call(
            &router,
            "POST",
            "/api/migrations/snapshots",
            Some(invalid),
            expected,
        )
        .await;
    }
    let mut invalid = manifest.clone();
    invalid["id"] = json!("invalid-report-source");
    invalid["reports"][0]["source_sha256"] = json!(sha(b"other report"));
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(invalid),
        400,
    )
    .await;
    let mut invalid = manifest.clone();
    invalid["id"] = json!("invalid-range");
    invalid["reports"][0]["parts"][0]["row_start"] = json!(1);
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(invalid),
        400,
    )
    .await;
    let mut invalid = manifest.clone();
    invalid["id"] = json!("invalid-column-order");
    invalid["reports"][0]["columns"] = json!(["B", "A"]);
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(invalid),
        400,
    )
    .await;
    let mut invalid = manifest.clone();
    invalid["id"] = json!("invalid-exception");
    invalid["exceptions"][0]["row"] = json!(4);
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(invalid),
        400,
    )
    .await;
    let mut invalid = manifest.clone();
    invalid["id"] = json!("invalid-archive");
    invalid["source_archive"]["sha256"] = json!(sha(b"other archive"));
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(invalid),
        409,
    )
    .await;
    let mut invalid = manifest.clone();
    invalid["id"] = json!("reordered-archive");
    invalid["source_archive"]["chunks"] = json!([chunks[1].0, chunks[0].0]);
    call(
        &router,
        "POST",
        "/api/migrations/snapshots",
        Some(invalid),
        409,
    )
    .await;
    assert_eq!(
        call(&router, "GET", "/api/migrations", None, 200).await["snapshots"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Deliberately corrupt a synthetic backing store: public upload and reads
    // fail closed instead of repairing or rewriting immutable source content.
    let service = Arc::new(Migrations::new(Store::memory()));
    service
        .store
        .set(
            &format!("migrations:blob:{}", chunks[0].0),
            "corrupt synthetic bytes",
        )
        .await
        .unwrap();
    let failure = service
        .upload_chunk(ChunkInput {
            sha256: chunks[0].0.clone(),
            content: chunks[0].1.clone(),
        })
        .await;
    assert_eq!(failure.err().unwrap().0.as_u16(), 409);
}

#[tokio::test]
async fn client_scoped_staff_see_only_their_cohort_snapshots() {
    use nxt_os::firm::staff;
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::set_var("NXT_STAFF_PBKDF2_ITERATIONS", "1000");
    }
    staff::install(Arc::new(staff::Staff::new(Store::memory())));
    let router = app_with(Store::memory);
    let (_, archive_chunks, _) = fixture();
    upload(&router, &archive_chunks[..2]).await;
    for client in 0..2 {
        let (manifest, chunks, _) = roster_fixture(client);
        upload(&router, &chunks[2..]).await;
        call(&router, "POST", "/api/migrations/snapshots", Some(manifest), 200).await;
    }
    let pw = "synthetic-pass-phrase-1";
    let pw2 = "synthetic-pass-phrase-2";
    call(&router, "POST", "/api/staff/users", Some(json!({"username":"mig","display_name":"Synthetic Scoped","role":"preparer","clients":["synthetic-roster-000"],"temporary_password":pw})), 200).await;
    let as_staff = |tok: String| {
        let router = router.clone();
        move |m: &'static str, url: String, body: Option<Value>| {
            let (router, tok) = (router.clone(), tok.clone());
            async move {
                let mut q = Request::builder().method(m).uri(url).header("x-forwarded-for", "10.1.0.2");
                if !tok.is_empty() { q = q.header("authorization", format!("Bearer {tok}")); }
                let q = match body { Some(v) => q.header("content-type", "application/json").body(Body::from(v.to_string())), None => q.body(Body::empty()) }.unwrap();
                let resp = router.oneshot(q).await.unwrap();
                let s = resp.status().as_u16();
                let b = axum::body::to_bytes(resp.into_body(), 8 << 20).await.unwrap();
                (s, serde_json::from_slice::<Value>(&b).unwrap_or(Value::Null))
            }
        }
    };
    let anon = as_staff(String::new());
    let tok = anon("POST", "/api/staff/login".into(), Some(json!({"username":"mig","password":pw}))).await.1["token"].as_str().unwrap().to_string();
    let st = as_staff(tok);
    let secret = st("POST", "/api/staff/mfa/enroll".into(), Some(json!({}))).await.1["secret"].as_str().unwrap().to_string();
    assert_eq!(st("POST", "/api/staff/mfa/confirm".into(), Some(json!({"code":staff::totp_now(&secret).unwrap()}))).await.0, 200);
    assert_eq!(st("POST", "/api/staff/me/password".into(), Some(json!({"current_password":pw,"new_password":pw2}))).await.0, 200);

    let (s, list) = st("GET", "/api/migrations".into(), None).await;
    assert_eq!(s, 200);
    let ids: Vec<_> = list["snapshots"].as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids, vec!["synthetic-shadow-000"]);
    assert_eq!(st("GET", "/api/migrations/synthetic-shadow-000".into(), None).await.0, 200);
    assert_eq!(st("GET", "/api/migrations/synthetic-shadow-001".into(), None).await.0, 403);
    assert_eq!(st("GET", "/api/migrations/synthetic-shadow-001/archive".into(), None).await.0, 403);
    assert_eq!(st("GET", "/api/migrations/synthetic-shadow-001/reports/r1/rows".into(), None).await.0, 403);
    assert_eq!(st("POST", "/api/migrations/chunks".into(), Some(json!({"sha256":"00","content":"x"}))).await.0, 403);
    assert_eq!(st("POST", "/api/migrations/snapshots".into(), Some(json!({}))).await.0, 403);
    // Break-glass still sees both.
    assert_eq!(call(&router, "GET", "/api/migrations", None, 200).await["snapshots"].as_array().unwrap().len(), 2);
}
