//! Synthetic source contacts only: no payroll profiles, wages, source or production access.
use axum::{Router, body::Body, http::Request};
use nxt_os::{
    company_books::source_lists::{self, PublishInput, SourceLists},
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
fn sha(v: &[u8]) -> String {
    hex::encode(Sha256::digest(v))
}
fn cell(value: Option<&str>, formula: Option<&str>) -> Value {
    json!({"present":value.is_some(),"type":if value.is_some(){Some("s")}else{None},"value":value,"formula":formula})
}
async fn fixture() -> (Arc<SourceLists>, Value, String) {
    let migrations = Arc::new(Migrations::new(Store::memory()));
    let archive = b"PK\x03\x04synthetic-source-only";
    let archive_sha = sha(archive);
    let encoded_archive = hex::encode(archive);
    let archive_ref = sha(encoded_archive.as_bytes());
    migrations
        .upload_chunk(ChunkInput {
            sha256: archive_ref.clone(),
            content: encoded_archive,
        })
        .await
        .ok()
        .unwrap();
    let journal_sha = "a".repeat(64);
    let rows=json!({"format":"nxt-shadow-report-rows/v1","cohort_ref":"synthetic-A","source_archive_sha256":archive_sha,"source_report_sha256":journal_sha,"report_id":"journal","rows":[[5,[["A","s","Synthetic source",null,null]]]]}).to_string();
    let row_sha = sha(rows.as_bytes());
    migrations
        .upload_chunk(ChunkInput {
            sha256: row_sha.clone(),
            content: rows,
        })
        .await
        .ok()
        .unwrap();
    let manifest:SnapshotManifest=serde_json::from_value(json!({"format":"nxt-shadow-snapshot/v1","id":"snapshot-A","cohort_ref":"synthetic-A","company_name":"Synthetic company A","source_system":"quickbooks","captured_at":"2026-10-27T00:00:00Z","cutoff_date":"2026-10-27","reviewer_label":"Synthetic reviewer","source_of_truth":"original_system","mode":"parallel_test","source_archive":{"file_name":"synthetic.zip","sha256":archive_sha,"bytes":archive.len(),"encoding":"hex","chunks":[archive_ref]},"reports":[{"id":"journal","title":"Synthetic journal","sheet_name":"Sheet1","source_sha256":journal_sha,"source_bytes":10,"columns":["A"],"row_count":1,"parts":[{"sha256":row_sha,"row_start":5,"row_end":5,"row_count":1}]}],"exceptions":[],"limitations":["Synthetic snapshot; source employee workbook is a separate operator proof."]})).unwrap();
    let published = migrations.publish(manifest).await.ok().unwrap();
    let manifest_sha = published["manifest_sha256"].as_str().unwrap().to_string();
    let report_sha = "b".repeat(64);
    let sheet = "xl/worksheets/sheet1.xml";
    let alias = format!(
        "employee_{}",
        sha(
            serde_json::to_vec(&json!(["synthetic-A", archive_sha, report_sha, sheet, 3]))
                .unwrap()
                .as_slice()
        )
    );
    let fields = json!({"display_name":cell(Some("Synthetic <person>"),None),"phone_numbers":cell(Some(""),None),"email":cell(None,None),"address":cell(Some("Original cached text"),Some("HYPERLINK(\"javascript:synthetic\")"))});
    let raw = json!({"B":fields["display_name"],"C":fields["phone_numbers"],"D":fields["email"],"E":fields["address"]});
    let record = json!({"cohort_ref":"synthetic-A","record_type":"employee","source_record_alias":alias,"source_id":null,"source_id_coverage":"archive_row","source_identity_verified":false,"operational_object_created":false,"linked_to_journal_contacts":false,"source_archive_sha256":archive_sha,"source_report_sha256":report_sha,"source_member":"Lists/Employees.xlsx","source_worksheet_name":"Sheet1","source_worksheet_file":sheet,"source_row":3,"source_fields":fields,"raw_source_cells":raw,"formula_fields_present":true});
    let capture = json!({"format":"nxt-company-source-list-capture/v1","company_id":"synthetic-A","record_type":"employee","source_system":"quickbooks_online","captured_at":"2026-10-27T00:00:00Z","source_archive_sha256":archive_sha,"source_archive_bytes":archive.len(),"source_archive_unchanged":true,"source_report_sha256":report_sha,"source_report_bytes":100,"source_company_heading_sha256":"c".repeat(64),"source_member":"Lists/Employees.xlsx","source_worksheet_name":"Sheet1","source_worksheet_file":sheet,"source_headers":{"B":"Employee","C":"Phone Numbers","D":"Email","E":"Address"},"source_id_coverage":"archive_row","source_identity_verified":false,"source_field_equality_verified":true,"records_total":1,"nonrecord_rows_total":1,"records":[record],"other_source_rows":[{"classification":"original_heading","source_row":1,"raw_cells":{"A":cell(Some("Synthetic company A"),None)}}],"formula_count":1,"source_snapshot_ref":{"snapshot_id":"snapshot-A","cohort_ref":"synthetic-A","source_archive_sha256":archive_sha,"local_manifest_file_sha256":"d".repeat(64),"server_canonical_manifest_sha256":null,"published_snapshot_binding_verified_by_this_projection":false},"source_coverage":{"employee_contact_list_rows":1,"employee_roster_complete":false,"all_active_and_inactive_employees_coverage_verified":false,"payroll_subscription_or_API_entitlement_verified":false},"controls":{"source_authoritative":true,"read_only":true,"operational_import":false,"operational_objects_created":false,"payroll_migrated":false,"live_cutover_allowed":false}});
    (
        Arc::new(SourceLists::new(Store::memory(), migrations)),
        capture,
        manifest_sha,
    )
}
fn input(id: &str, capture: &Value, manifest: &str, expected: Option<&str>) -> PublishInput {
    serde_json::from_value(json!({"expected_index_head":expected,"capture_sha256":id,"expected_source_manifest_sha256":manifest,"source_operator_proof":{"format":"nxt-source-list-operator-equality/v1","company_id":"synthetic-A","capture_sha256":id,"source_archive_sha256":capture["source_archive_sha256"],"source_report_sha256":capture["source_report_sha256"],"comparison_manifest_sha256":"e".repeat(64),"source_field_equality_verified":true}})).unwrap()
}
#[tokio::test]
async fn source_contact_publication_preserves_exact_cells_and_real_snapshot_binding() {
    let (s, c, m) = fixture().await;
    let up = s.upload("synthetic-A", c.clone()).await.ok().unwrap();
    let id = up["capture_sha256"].as_str().unwrap();
    assert_eq!(
        s.records("synthetic-A", "employee", id, 0, 100, 32768, false)
            .await
            .unwrap_err()
            .0,
        404
    );
    let first = s
        .publish("synthetic-A", input(id, &c, &m, None))
        .await
        .ok()
        .unwrap();
    assert_eq!(first["idempotent_replay"], false);
    let replay = s
        .publish("synthetic-A", input(id, &c, &m, None))
        .await
        .ok()
        .unwrap();
    assert_eq!(replay["idempotent_replay"], true);
    let page = s
        .records("synthetic-A", "employee", id, 0, 100, 32768, false)
        .await
        .ok()
        .unwrap();
    assert_eq!(page["records"], c["records"]);
    assert_eq!(page["capture"]["source_manifest_sha256"], m);
    assert_ne!(
        page["capture"]["source_manifest_sha256"],
        c["source_snapshot_ref"]["local_manifest_file_sha256"]
    );
    assert_eq!(page["capture"]["published_snapshot_binding_verified"], true);
    assert_eq!(page["records"][0]["source_id"], Value::Null);
    assert_eq!(
        page["records"][0]["source_fields"]["email"]["present"],
        false
    );
    assert_eq!(
        page["records"][0]["source_fields"]["phone_numbers"]["value"],
        ""
    );
    assert_eq!(page["controls"]["payroll_migrated"], false);
    assert_eq!(page["coverage"]["payroll"].as_array().unwrap().len(), 14);
    assert!(
        page["coverage"]["payroll"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["source_record_count"].is_null() && r["verified_source_scope"] == false)
    );
    let retained = s
        .records("synthetic-A", "employee", id, 0, 100, 32768, true)
        .await
        .ok()
        .unwrap();
    assert_eq!(retained["records"], c["other_source_rows"]);
}
#[tokio::test]
async fn company_archive_manifest_proof_and_raw_cell_changes_fail_closed() {
    let (s, c, m) = fixture().await;
    assert_eq!(s.upload("synthetic-B", c.clone()).await.unwrap_err().0, 400);
    let id = s.upload("synthetic-A", c.clone()).await.ok().unwrap()["capture_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.publish("synthetic-A", input(&id, &c, &"f".repeat(64), None))
            .await
            .unwrap_err()
            .0,
        409
    );
    let mut p = input(&id, &c, &m, None);
    p.source_operator_proof.source_report_sha256 = "f".repeat(64);
    assert_eq!(s.publish("synthetic-A", p).await.unwrap_err().0, 400);
    let mut altered = c.clone();
    altered["records"][0]["source_fields"]["email"]["value"] = json!("Invented value");
    assert_eq!(s.upload("synthetic-A", altered).await.unwrap_err().0, 400);
    let mut altered = c.clone();
    altered["source_snapshot_ref"]["server_canonical_manifest_sha256"] = json!(m);
    assert_eq!(s.upload("synthetic-A", altered).await.unwrap_err().0, 400);
    let mut altered = c.clone();
    altered["source_archive_sha256"] = json!("f".repeat(64));
    altered["source_snapshot_ref"]["source_archive_sha256"] =
        altered["source_archive_sha256"].clone();
    altered["records"] = json!([]);
    altered["records_total"] = json!(0);
    altered["formula_count"] = json!(0);
    altered["source_coverage"]["employee_contact_list_rows"] = json!(0);
    let bad_id = s.upload("synthetic-A", altered.clone()).await.ok().unwrap()["capture_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.publish("synthetic-A", input(&bad_id, &altered, &m, None))
            .await
            .unwrap_err()
            .0,
        409
    );
    assert!(
        s.list("synthetic-A").await.ok().unwrap()["captures"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn publication_cas_and_corruption_cannot_return_partial_lists() {
    let (s, c, m) = fixture().await;
    let mut second = c.clone();
    second["captured_at"] = json!("2026-10-27T01:00:00Z");
    let a = s.upload("synthetic-A", c.clone()).await.ok().unwrap()["capture_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let b = s.upload("synthetic-A", second.clone()).await.ok().unwrap()["capture_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let (a_result, b_result) = tokio::join!(
        s.publish("synthetic-A", input(&a, &c, &m, None)),
        s.publish("synthetic-A", input(&b, &second, &m, None))
    );
    assert_ne!(a_result.is_ok(), b_result.is_ok());
    let list = s.list("synthetic-A").await.ok().unwrap();
    assert_eq!(list["captures"].as_array().unwrap().len(), 1);
    let published = list["captures"][0]["capture_sha256"].as_str().unwrap();
    s.store
        .set(
            &format!("company-source-lists:synthetic-A:capture:{published}"),
            "{}",
        )
        .await
        .ok()
        .unwrap();
    assert_eq!(s.list("synthetic-A").await.unwrap_err().0, 500);
    assert_eq!(
        s.records("synthetic-A", "employee", published, 0, 100, 32768, false)
            .await
            .unwrap_err()
            .0,
        500
    );
}
#[tokio::test]
async fn empty_export_is_explicit_and_byte_pages_preserve_all_contacts() {
    let (s, mut c, m) = fixture().await;
    c["records"] = json!([]);
    c["records_total"] = json!(0);
    c["formula_count"] = json!(0);
    c["source_coverage"]["employee_contact_list_rows"] = json!(0);
    let id = s.upload("synthetic-A", c.clone()).await.ok().unwrap()["capture_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    s.publish("synthetic-A", input(&id, &c, &m, None))
        .await
        .ok()
        .unwrap();
    let p = s
        .records("synthetic-A", "employee", &id, 0, 100, 32768, false)
        .await
        .ok()
        .unwrap();
    assert_eq!(p["total"], 0);
    assert_eq!(p["has_more"], false);
    assert_eq!(
        p["coverage"]["operational_employee_import"],
        "pending_actual_source_profiles_and_identity_mapping"
    );
    let mut c = c.clone();
    let (_, source, _) = fixture().await;
    let original = source["records"][0].clone();
    let mut records = vec![];
    for i in 0..8u64 {
        let mut r = original.clone();
        let number = i + 3;
        r["source_row"] = json!(number);
        r["source_record_alias"] = json!(format!(
            "employee_{}",
            sha(serde_json::to_vec(&json!([
                "synthetic-A",
                c["source_archive_sha256"],
                c["source_report_sha256"],
                c["source_worksheet_file"],
                number
            ]))
            .unwrap()
            .as_slice())
        ));
        for key in ["source_fields", "raw_source_cells"] {
            let field = if key == "source_fields" {
                "address"
            } else {
                "E"
            };
            r[key][field]["value"] = json!("X".repeat(6000));
        }
        records.push(r);
    }
    c["records"] = json!(records);
    c["records_total"] = json!(8);
    c["formula_count"] = json!(8);
    c["source_coverage"]["employee_contact_list_rows"] = json!(8);
    let new = s.upload("synthetic-A", c.clone()).await.ok().unwrap()["capture_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let head = s.list("synthetic-A").await.ok().unwrap()["index_head_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    s.publish("synthetic-A", input(&new, &c, &m, Some(&head)))
        .await
        .ok()
        .unwrap();
    let mut offset = 0;
    let mut count = 0;
    loop {
        let p = s
            .records("synthetic-A", "employee", &new, offset, 100, 32768, false)
            .await
            .ok()
            .unwrap();
        let n = p["records"].as_array().unwrap().len();
        assert!(n > 0);
        assert!(p["page_bytes"].as_u64().unwrap() <= 32768);
        count += n;
        if !p["has_more"].as_bool().unwrap() {
            break;
        }
        let next = p["next_offset"].as_u64().unwrap() as usize;
        assert_eq!(next, offset + n);
        offset = next;
    }
    assert_eq!(count, 8);
}
#[tokio::test]
async fn source_list_http_is_protected_and_private() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
    }
    let (s, c, m) = fixture().await;
    let router: Router = source_lists::router(s.clone());
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/company-books/synthetic-A/source-lists")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .ok()
        .unwrap();
    assert_eq!(response.status(), 401);
    let up = s.upload("synthetic-A", c.clone()).await.ok().unwrap();
    let id = up["capture_sha256"].as_str().unwrap();
    s.publish("synthetic-A", input(id, &c, &m, None))
        .await
        .ok()
        .unwrap();
    let response = router
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/company-books/synthetic-A/source-lists/employee/{id}/records?limit=1"
                ))
                .header("authorization", "Bearer k")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .ok()
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
}

#[tokio::test]
async fn incomplete_or_oversized_capture_never_publishes_and_null_presence_is_explicit() {
    let (s, c, m) = fixture().await;
    assert_eq!(
        s.publish("synthetic-A", input(&"f".repeat(64), &c, &m, None))
            .await
            .unwrap_err()
            .0,
        409
    );
    for missing_key in ["source_id", "source_record_alias"] {
        let mut altered = c.clone();
        altered["records"][0]
            .as_object_mut()
            .unwrap()
            .remove(missing_key);
        assert_eq!(s.upload("synthetic-A", altered).await.unwrap_err().0, 400);
    }
    let mut altered = c.clone();
    altered["source_member"] = json!("Lists/EmployeeDetails.xls");
    altered["records"][0]["source_member"] = altered["source_member"].clone();
    assert_eq!(s.upload("synthetic-A", altered).await.unwrap_err().0, 400);
    let mut altered = c.clone();
    altered["source_snapshot_ref"]
        .as_object_mut()
        .unwrap()
        .remove("server_canonical_manifest_sha256");
    assert_eq!(s.upload("synthetic-A", altered).await.unwrap_err().0, 400);
    let mut altered = c.clone();
    altered["records"][0]["source_fields"]["display_name"]["value"] = json!("X".repeat(8193));
    altered["records"][0]["raw_source_cells"]["B"]["value"] =
        altered["records"][0]["source_fields"]["display_name"]["value"].clone();
    assert_eq!(s.upload("synthetic-A", altered).await.unwrap_err().0, 400);
    assert!(
        s.list("synthetic-A").await.ok().unwrap()["captures"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
