//! Immutable source contact projections. Never operational employees or payroll.
use super::{Result, bad, digest, encoded, hash, page_rows, storage, text, valid_id};
use crate::{migrations::Migrations, referral::routes::ApiError, store::Store};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};

const MAX_CAPTURE: usize = 512 * 1024;
const MAX_CAPTURES: usize = 64;
pub struct SourceLists {
    pub store: Store,
    migrations: Arc<Migrations>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProof {
    pub format: String,
    pub company_id: String,
    pub capture_sha256: String,
    pub source_archive_sha256: String,
    pub source_report_sha256: String,
    pub comparison_manifest_sha256: String,
    pub source_field_equality_verified: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishInput {
    pub expected_index_head: Option<String>,
    pub capture_sha256: String,
    pub expected_source_manifest_sha256: String,
    pub source_operator_proof: SourceProof,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    format: String,
    company_id: String,
    capture_sha256: String,
    source_snapshot_id: String,
    source_manifest_sha256: String,
    source_operator_proof: SourceProof,
    published_at: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    publications: Vec<String>,
}
fn fault() -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Source list integrity or publication binding failed; no partial list returned".into(),
    )
}
fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
fn missing() -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        "Published source contact capture not found for this company".into(),
    )
}
fn flag(v: &Value, key: &str, expected: bool) -> bool {
    v.get(key).and_then(Value::as_bool) == Some(expected)
}
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn row(v: &Value) -> Option<u64> {
    v.get("source_row")
        .and_then(Value::as_u64)
        .filter(|v| *v > 0 && *v <= 1_048_576)
}
fn boundary() -> Value {
    json!({"source_authoritative":true,"read_only":true,"operational_import":false,"operational_objects_created":false,"payroll_migrated":false,"live_cutover_allowed":false,"identity_limit":"Archive-row aliases and shared workspace access are not durable employee identities or individual company authorization.","equality_basis":"Pinned operator-provided original-ZIP/workbook equality proof; server validates capture and published archive binding, not Excel contents."})
}
fn cell(value: &Value) -> bool {
    let Some(o) = value.as_object() else {
        return false;
    };
    if o.len() != 4
        || !["present", "type", "value", "formula"]
            .iter()
            .all(|k| o.contains_key(*k))
    {
        return false;
    }
    let present = o["present"].as_bool();
    let nullable =
        |key: &str, max: usize| o[key].is_null() || o[key].as_str().is_some_and(|v| text(v, max));
    present.is_some()
        && nullable("type", 40)
        && nullable("value", 8192)
        && nullable("formula", 8192)
        && (present == Some(true) || ["type", "value", "formula"].iter().all(|k| o[*k].is_null()))
}
fn safe_tree(value: &Value, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if depth > 12 || *nodes > 100_000 {
        return false;
    }
    match value {
        Value::String(v) => text(v, 8192),
        Value::Number(v) => v.is_u64() || v.is_i64(),
        Value::Array(v) => v.iter().all(|v| safe_tree(v, depth + 1, nodes)),
        Value::Object(v) => v
            .iter()
            .all(|(k, v)| text(k, 200) && safe_tree(v, depth + 1, nodes)),
        _ => true,
    }
}
fn validate(company: &str, capture: &Value) -> Result<()> {
    let records = capture
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("Missing source records array"))?;
    let other = capture
        .get("other_source_rows")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("Missing retained source rows array"))?;
    let mut nodes = 0;
    if !valid_id(company)
        || s(capture, "format") != "nxt-company-source-list-capture/v1"
        || s(capture, "company_id") != company
        || s(capture, "record_type") != "employee"
        || s(capture, "source_system") != "quickbooks_online"
        || records.len() > 1000
        || other.len() > 2048
        || capture["records_total"].as_u64() != Some(records.len() as u64)
        || capture["nonrecord_rows_total"].as_u64() != Some(other.len() as u64)
        || !safe_tree(capture, 0, &mut nodes)
        || encoded(capture)?.len() > MAX_CAPTURE
        || !digest(s(capture, "source_archive_sha256"))
        || !digest(s(capture, "source_report_sha256"))
        || !digest(s(capture, "source_company_heading_sha256"))
        || !flag(capture, "source_archive_unchanged", true)
        || !flag(capture, "source_field_equality_verified", true)
        || !flag(capture, "source_identity_verified", false)
        || s(capture, "source_id_coverage") != "archive_row"
        || capture["source_archive_bytes"]
            .as_u64()
            .is_none_or(|n| n == 0 || n > 4 * 1024 * 1024)
        || capture["source_report_bytes"]
            .as_u64()
            .is_none_or(|n| n == 0 || n > 32 * 1024 * 1024)
        || s(capture, "source_member").is_empty()
        || s(capture, "source_member").rsplit(['/', '\\']).next() != Some("Employees.xlsx")
        || s(capture, "source_worksheet_name").is_empty()
        || s(capture, "source_worksheet_file").is_empty()
    {
        return Err(bad(
            "Invalid, oversized or unsupported source employee projection",
        ));
    }
    let expected_controls = json!({"live_cutover_allowed":false,"operational_import":false,"operational_objects_created":false,"payroll_migrated":false,"read_only":true,"source_authoritative":true});
    let expected_headers = json!({"B":"Employee","C":"Phone Numbers","D":"Email","E":"Address"});
    if capture["controls"] != expected_controls
        || capture["source_headers"] != expected_headers
        || !flag(
            &capture["source_coverage"],
            "employee_roster_complete",
            false,
        )
        || !flag(
            &capture["source_coverage"],
            "all_active_and_inactive_employees_coverage_verified",
            false,
        )
        || !flag(
            &capture["source_coverage"],
            "payroll_subscription_or_API_entitlement_verified",
            false,
        )
        || capture["source_coverage"]["employee_contact_list_rows"].as_u64()
            != Some(records.len() as u64)
    {
        return Err(bad(
            "Source contacts cannot assert operational employee or payroll completeness",
        ));
    }
    let reference = &capture["source_snapshot_ref"];
    if s(reference, "cohort_ref") != company
        || s(reference, "source_archive_sha256") != s(capture, "source_archive_sha256")
        || !valid_id(s(reference, "snapshot_id"))
        || !digest(s(reference, "local_manifest_file_sha256"))
        || !reference
            .get("server_canonical_manifest_sha256")
            .is_some_and(Value::is_null)
        || !flag(
            reference,
            "published_snapshot_binding_verified_by_this_projection",
            false,
        )
    {
        return Err(bad(
            "Source snapshot reference must retain its original local/unverified hash distinction",
        ));
    }
    let mut rows = HashSet::new();
    let mut aliases = HashSet::new();
    let mut formulas = 0usize;
    for record in records {
        let number = row(record).ok_or_else(|| bad("Invalid source worksheet row"))?;
        let expected_alias = format!(
            "employee_{}",
            hash(
                encoded(&json!([
                    company,
                    s(capture, "source_archive_sha256"),
                    s(capture, "source_report_sha256"),
                    s(capture, "source_worksheet_file"),
                    number
                ]))?
                .as_bytes()
            )
        );
        if !rows.insert(number)
            || !aliases.insert(s(record, "source_record_alias"))
            || s(record, "source_record_alias") != expected_alias
            || s(record, "cohort_ref") != company
            || s(record, "record_type") != "employee"
            || !record.get("source_id").is_some_and(Value::is_null)
            || s(record, "source_id_coverage") != "archive_row"
            || !flag(record, "source_identity_verified", false)
            || !flag(record, "operational_object_created", false)
            || !flag(record, "linked_to_journal_contacts", false)
            || [
                "source_archive_sha256",
                "source_report_sha256",
                "source_member",
                "source_worksheet_name",
                "source_worksheet_file",
            ]
            .iter()
            .any(|key| record[*key] != capture[*key])
        {
            return Err(bad(
                "Source record company, archive, row alias or operational status is invalid",
            ));
        }
        let raw = record["raw_source_cells"]
            .as_object()
            .ok_or_else(|| bad("Original source cells missing"))?;
        let fields = record["source_fields"]
            .as_object()
            .ok_or_else(|| bad("Typed source fields missing"))?;
        let mapping = [
            ("B", "display_name"),
            ("C", "phone_numbers"),
            ("D", "email"),
            ("E", "address"),
        ];
        if raw.len() != 4
            || fields.len() != 4
            || mapping.iter().any(|(column, key)| {
                raw.get(*column).is_none_or(|c| !cell(c)) || raw.get(*column) != fields.get(*key)
            })
        {
            return Err(bad(
                "Raw source field presence/value/formula representation changed",
            ));
        }
        let count = raw.values().filter(|c| !c["formula"].is_null()).count();
        formulas += count;
        if !flag(record, "formula_fields_present", count > 0) {
            return Err(bad("Source formula flags are inconsistent"));
        }
    }
    for retained in other {
        let number = row(retained).ok_or_else(|| bad("Invalid retained source row"))?;
        let raw = retained["raw_cells"]
            .as_object()
            .ok_or_else(|| bad("Retained source cells missing"))?;
        if !rows.insert(number)
            || s(retained, "classification").is_empty()
            || raw.len() > 26
            || raw.iter().any(|(key, value)| {
                key.len() > 3 || !key.bytes().all(|c| c.is_ascii_uppercase()) || !cell(value)
            })
        {
            return Err(bad(
                "Retained source rows overlap or contain unsupported cells",
            ));
        }
    }
    if capture["formula_count"].as_u64() != Some(formulas as u64) {
        return Err(bad("Source formula count differs"));
    }
    Ok(())
}
impl SourceLists {
    pub fn new(store: Store, migrations: Arc<Migrations>) -> Self {
        Self { store, migrations }
    }
    fn key(company: &str, kind: &str, id: &str) -> Result<String> {
        if !valid_id(company) {
            return Err(bad("Invalid company reference"));
        }
        Ok(format!("company-source-lists:{company}:{kind}:{id}"))
    }
    async fn immutable<T: Serialize>(&self, key: &str, value: &T) -> Result<String> {
        let raw = encoded(value)?;
        self.store.set_nx(key, &raw).await.map_err(storage)?;
        if self.store.get(key).await.map_err(storage)?.as_deref() != Some(raw.as_str()) {
            return Err(fault());
        }
        Ok(hash(raw.as_bytes()))
    }
    async fn object<T: serde::de::DeserializeOwned + Serialize>(
        &self,
        key: &str,
        id: &str,
    ) -> Result<T> {
        let raw = self
            .store
            .get(key)
            .await
            .map_err(storage)?
            .ok_or_else(fault)?;
        if raw.len() > MAX_CAPTURE || hash(raw.as_bytes()) != id {
            return Err(fault());
        }
        let value: T = serde_json::from_str(&raw).map_err(|_| fault())?;
        if encoded(&value)? != raw {
            return Err(fault());
        }
        Ok(value)
    }
    async fn index(&self, company: &str) -> Result<(Option<String>, Index)> {
        let raw = self
            .store
            .get(&Self::key(company, "index", "v1")?)
            .await
            .map_err(storage)?;
        let Some(raw) = raw else {
            return Ok((None, Index::default()));
        };
        if raw.len() > 32 * 1024 {
            return Err(fault());
        }
        let index: Index = serde_json::from_str(&raw).map_err(|_| fault())?;
        let mut seen = HashSet::new();
        if encoded(&index)? != raw
            || index.publications.len() > MAX_CAPTURES
            || index
                .publications
                .iter()
                .any(|id| !digest(id) || !seen.insert(id))
        {
            return Err(fault());
        }
        Ok((Some(raw), index))
    }
    async fn capture(&self, company: &str, id: &str) -> Result<Value> {
        if !digest(id) {
            return Err(bad("Invalid source capture hash"));
        }
        let c: Value = self.object(&Self::key(company, "capture", id)?, id).await?;
        validate(company, &c).map_err(|_| fault())?;
        Ok(c)
    }
    async fn bind(&self, company: &str, c: &Value, expected: &str) -> Result<String> {
        if !digest(expected) {
            return Err(bad("The actual server source manifest SHA-256 is required"));
        }
        let snapshot = s(&c["source_snapshot_ref"], "snapshot_id");
        let detail = self.migrations.detail(snapshot).await?;
        let m = &detail["manifest"];
        if detail["manifest_sha256"] != expected
            || m["cohort_ref"] != company
            || m["source_archive"]["sha256"] != c["source_archive_sha256"]
            || m["source_archive"]["bytes"] != c["source_archive_bytes"]
            || m["source_of_truth"] != "original_system"
            || m["mode"] != "parallel_test"
        {
            return Err(conflict(
                "Published source snapshot company, archive or canonical manifest binding differs",
            ));
        }
        Ok(snapshot.into())
    }
    fn proof(company: &str, capture_id: &str, c: &Value, p: &SourceProof) -> Result<()> {
        if p.format != "nxt-source-list-operator-equality/v1"
            || p.company_id != company
            || p.capture_sha256 != capture_id
            || p.source_archive_sha256 != s(c, "source_archive_sha256")
            || p.source_report_sha256 != s(c, "source_report_sha256")
            || !digest(&p.comparison_manifest_sha256)
            || !p.source_field_equality_verified
        {
            return Err(bad(
                "Operator equality proof does not bind this company, capture and original workbook",
            ));
        }
        Ok(())
    }
    pub async fn upload(&self, company: &str, capture: Value) -> Result<Value> {
        validate(company, &capture)?;
        let id = hash(encoded(&capture)?.as_bytes());
        self.immutable(&Self::key(company, "capture", &id)?, &capture)
            .await?;
        Ok(
            json!({"capture_sha256":id,"sha256":id,"bytes":encoded(&capture)?.len(),"records_total":capture["records_total"],"controls":boundary()}),
        )
    }
    async fn publication(&self, company: &str, id: &str) -> Result<(Publication, Value)> {
        let p: Publication = self
            .object(&Self::key(company, "publication", id)?, id)
            .await?;
        if p.format != "nxt-company-source-list-publication/v1" || p.company_id != company {
            return Err(fault());
        }
        let c = self.capture(company, &p.capture_sha256).await?;
        if self.bind(company, &c, &p.source_manifest_sha256).await? != p.source_snapshot_id {
            return Err(fault());
        }
        Self::proof(company, &p.capture_sha256, &c, &p.source_operator_proof)
            .map_err(|_| fault())?;
        Ok((p, c))
    }
    fn summary(id: &str, p: &Publication, c: &Value) -> Value {
        json!({"publication_sha256":id,"capture_sha256":p.capture_sha256,"record_type":"employee","records_total":c["records_total"],"nonrecord_rows_total":c["nonrecord_rows_total"],"source_archive_sha256":c["source_archive_sha256"],"source_report_sha256":c["source_report_sha256"],"source_member":c["source_member"],"source_worksheet_name":c["source_worksheet_name"],"source_worksheet_file":c["source_worksheet_file"],"source_headers":c["source_headers"],"source_id_coverage":"archive_row","source_identity_verified":false,"source_field_equality_verified":true,"source_snapshot_id":p.source_snapshot_id,"source_manifest_sha256":p.source_manifest_sha256,"published_snapshot_binding_verified":true,"comparison_manifest_sha256":p.source_operator_proof.comparison_manifest_sha256,"published_at":p.published_at,"operational_import":false,"payroll_migrated":false})
    }
    pub async fn publish(&self, company: &str, input: PublishInput) -> Result<Value> {
        if input
            .expected_index_head
            .as_ref()
            .is_some_and(|v| !digest(v))
        {
            return Err(bad("Invalid expected source list index hash"));
        }
        if !digest(&input.capture_sha256) {
            return Err(bad("Invalid source capture hash"));
        }
        if self
            .store
            .get(&Self::key(company, "capture", &input.capture_sha256)?)
            .await
            .map_err(storage)?
            .is_none()
        {
            return Err(conflict(
                "Upload the complete immutable source capture before publication",
            ));
        }
        let c = self.capture(company, &input.capture_sha256).await?;
        let snapshot = self
            .bind(company, &c, &input.expected_source_manifest_sha256)
            .await?;
        Self::proof(
            company,
            &input.capture_sha256,
            &c,
            &input.source_operator_proof,
        )?;
        let (raw, mut index) = self.index(company).await?;
        for id in &index.publications {
            let (p, prior) = self.publication(company, id).await?;
            if p.capture_sha256 == input.capture_sha256 {
                if p.source_manifest_sha256 != input.expected_source_manifest_sha256
                    || encoded(&p.source_operator_proof)? != encoded(&input.source_operator_proof)?
                {
                    return Err(conflict(
                        "Capture already has a different immutable publication proof",
                    ));
                }
                return Ok(
                    json!({"company_id":company,"index_head_sha256":raw.as_ref().map(|s|hash(s.as_bytes())),"capture":Self::summary(id,&p,&prior),"idempotent_replay":true,"controls":boundary()}),
                );
            }
        }
        if raw.as_ref().map(|s| hash(s.as_bytes())) != input.expected_index_head {
            return Err(conflict(
                "Source list index changed; refresh before publishing",
            ));
        }
        if index.publications.len() >= MAX_CAPTURES {
            return Err(conflict("Source list capture capacity reached"));
        }
        let p = Publication {
            format: "nxt-company-source-list-publication/v1".into(),
            company_id: company.into(),
            capture_sha256: input.capture_sha256,
            source_snapshot_id: snapshot,
            source_manifest_sha256: input.expected_source_manifest_sha256,
            source_operator_proof: input.source_operator_proof,
            published_at: chrono::Utc::now().to_rfc3339(),
        };
        let id = hash(encoded(&p)?.as_bytes());
        self.immutable(&Self::key(company, "publication", &id)?, &p)
            .await?;
        index.publications.push(id.clone());
        let next = encoded(&index)?;
        if !self
            .store
            .compare_exchange(&Self::key(company, "index", "v1")?, raw.as_deref(), &next)
            .await
            .map_err(storage)?
        {
            return Err(conflict(
                "Source list publication raced; retry the same capture and proof",
            ));
        }
        Ok(
            json!({"company_id":company,"index_head_sha256":hash(next.as_bytes()),"capture":Self::summary(&id,&p,&c),"idempotent_replay":false,"controls":boundary()}),
        )
    }
    pub async fn list(&self, company: &str) -> Result<Value> {
        let (raw, index) = self.index(company).await?;
        let mut captures = vec![];
        for id in index.publications {
            let (p, c) = self.publication(company, &id).await?;
            captures.push(Self::summary(&id, &p, &c));
        }
        Ok(
            json!({"company_id":company,"index_head_sha256":raw.map(|s|hash(s.as_bytes())),"captures":captures,"list_inventory_complete":false,"coverage":coverage(),"controls":boundary()}),
        )
    }
    pub async fn records(
        &self,
        company: &str,
        kind: &str,
        capture_id: &str,
        offset: usize,
        limit: usize,
        max_bytes: usize,
        retained: bool,
    ) -> Result<Value> {
        if kind != "employee" || !digest(capture_id) {
            return Err(bad("Select a published employee source contact capture"));
        }
        let (_, index) = self.index(company).await?;
        let mut selected = None;
        for id in index.publications {
            let (p, c) = self.publication(company, &id).await?;
            if p.capture_sha256 == capture_id {
                selected = Some((id, p, c));
            }
        }
        let (id, p, c) = selected.ok_or_else(missing)?;
        let records = c[if retained {
            "other_source_rows"
        } else {
            "records"
        }]
        .as_array()
        .ok_or_else(fault)?
        .clone();
        let (rows, total, next, page_bytes) = page_rows(records, offset, limit, max_bytes)?;
        Ok(
            json!({"company_id":company,"record_type":kind,"capture_sha256":capture_id,"capture":Self::summary(&id,&p,&c),"offset":offset,"limit":limit,"max_bytes":max_bytes,"page_bytes":page_bytes,"total":total,"next_offset":next,"has_more":next.is_some(),"records":rows,"row_set":if retained{"other_source_rows"}else{"records"},"coverage":coverage(),"controls":boundary()}),
        )
    }
}
pub fn coverage() -> Value {
    let requirements = [
        (
            "payroll_source_and_entitlement",
            "Provider, country/region, subscription/access, provider changes and paygroup scope",
        ),
        (
            "operational_employee_profiles",
            "Durable employee IDs, identity mapping, active/inactive history, dates and source HR/tax fields",
        ),
        (
            "payroll_runs",
            "All original payruns, statuses, periods, pay dates, adjustments and reversals",
        ),
        (
            "pay_history_and_paystubs",
            "Employee paycheques/paystubs, historical and YTD details",
        ),
        (
            "earnings_and_pay_items",
            "Earnings, hours, rates, effective dates and source item codes",
        ),
        (
            "deductions_and_withholdings",
            "Deduction/tax items, elections and historical amounts",
        ),
        (
            "employer_contributions_and_benefits",
            "Employer contributions/benefits, balances and effective history",
        ),
        (
            "leave_and_time",
            "Leave policies, accruals, usage, balances and payroll time links",
        ),
        (
            "remittances_and_liabilities",
            "Tax/benefit liabilities, due/paid dates, settlements and outstanding balances",
        ),
        (
            "payroll_payments",
            "Payment IDs, methods, statuses and bank/GL links",
        ),
        (
            "t4_t4a_rl1_and_summaries",
            "Original/amended/cancelled slips and summaries, years, XML/PDF and acknowledgements",
        ),
        (
            "roe_and_employment_documents",
            "Existing original/amended ROE, documents and statuses",
        ),
        (
            "attachments_and_source_audit",
            "Original payroll/HR documents, permissions, audit and retention evidence",
        ),
        (
            "payroll_reconciliation",
            "Gross-to-net, employer costs, liabilities/payments, tax forms and source GL controls",
        ),
    ];
    json!({"employee_contacts_only":true,"operational_employee_import":"pending_actual_source_profiles_and_identity_mapping","payroll":requirements.into_iter().map(|(category,required)|json!({"category":category,"status":"pending_actual_source_exports_and_reconciliation","required_source":required,"source_record_count":null,"verified_source_scope":false,"missing_values_imputed":false})).collect::<Vec<_>>()})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    capture: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
    #[serde(default = "budget")]
    max_bytes: usize,
    #[serde(default, rename = "path")]
    transport_path: Option<String>,
}
fn limit() -> usize {
    100
}
fn budget() -> usize {
    768 * 1024
}
pub fn router(state: Arc<SourceLists>) -> Router {
    Router::new()
        .route(
            "/api/company-books/{company}/source-lists",
            get(index).post(publish),
        )
        .route(
            "/api/company-books/{company}/source-lists/chunks",
            post(upload),
        )
        .route(
            "/api/company-books/{company}/source-lists/{kind}/{capture}/records",
            get(records),
        )
        .route(
            "/api/company-books/{company}/source-lists/{kind}/{capture}/other-rows",
            get(other_rows),
        )
        .layer(DefaultBodyLimit::max(600 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            r.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            r
        }))
        .with_state(state)
}
async fn index(State(s): State<Arc<SourceLists>>, Path(c): Path<String>) -> Result<Json<Value>> {
    Ok(Json(s.list(&c).await?))
}
async fn upload(
    State(s): State<Arc<SourceLists>>,
    Path(c): Path<String>,
    Json(v): Json<Upload>,
) -> Result<Json<Value>> {
    Ok(Json(s.upload(&c, v.capture).await?))
}
async fn publish(
    State(s): State<Arc<SourceLists>>,
    Path(c): Path<String>,
    Json(v): Json<PublishInput>,
) -> Result<Json<Value>> {
    Ok(Json(s.publish(&c, v).await?))
}
async fn read(
    s: Arc<SourceLists>,
    c: String,
    k: String,
    id: String,
    p: Page,
    retained: bool,
) -> Result<Json<Value>> {
    let suffix = if retained { "other-rows" } else { "records" };
    if p.transport_path.as_deref().is_some_and(|v| {
        v.trim_start_matches('/')
            .strip_prefix("api/")
            .unwrap_or(v.trim_start_matches('/'))
            != format!("company-books/{c}/source-lists/{k}/{id}/{suffix}")
    }) {
        return Err(bad("Transport path mismatch"));
    }
    Ok(Json(
        s.records(&c, &k, &id, p.offset, p.limit, p.max_bytes, retained)
            .await?,
    ))
}
async fn records(
    State(s): State<Arc<SourceLists>>,
    Path((c, k, id)): Path<(String, String, String)>,
    Query(p): Query<Page>,
) -> Result<Json<Value>> {
    read(s, c, k, id, p, false).await
}
async fn other_rows(
    State(s): State<Arc<SourceLists>>,
    Path((c, k, id)): Path<(String, String, String)>,
    Query(p): Query<Page>,
) -> Result<Json<Value>> {
    read(s, c, k, id, p, true).await
}
