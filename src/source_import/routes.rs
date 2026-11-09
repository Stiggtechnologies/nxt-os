use super::files::{ClientLinkInput, FileInput, ResolveInput};
use super::{ImportError, OpenBatch, ParallelInput, SourceImport};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

type S = State<Arc<SourceImport>>;

impl IntoResponse for ImportError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1, "live_cutover_allowed": false, "filing_enabled": false, "write_back": false }))).into_response()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkBody {
    seq: u32,
    records: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitBody {
    request_id: String,
    reviewer_label: String,
}

pub fn router(state: Arc<SourceImport>) -> Router {
    Router::new()
        .route("/api/source-imports/coverage", get(coverage))
        .route("/api/source-imports/{lane}/batches", post(open))
        .route("/api/source-imports/{lane}/batches/{batch}", get(progress))
        .route("/api/source-imports/{lane}/batches/{batch}/chunks", post(chunk))
        .route("/api/source-imports/{lane}/batches/{batch}/commit", post(commit))
        .route("/api/source-imports/{lane}/batches/{batch}/recovery", get(recover))
        .route("/api/source-imports/{lane}/batches/{batch}/parallel", post(parallel).get(latest_parallel))
        .route("/api/source-imports/dtmax/companies/{company}/clients", post(register_clients))
        .route("/api/source-imports/dtmax/companies/{company}/clients/{client}/files", get(client_files))
        .route("/api/source-imports/dtmax/companies/{company}/files", post(preserve_file))
        .route("/api/source-imports/dtmax/companies/{company}/files/unmatched", get(unmatched))
        .route("/api/source-imports/dtmax/companies/{company}/files/{sha}", get(source_file))
        .route("/api/source-imports/dtmax/companies/{company}/files/{sha}/content", get(source_bytes))
        .route("/api/source-imports/dtmax/companies/{company}/files/{sha}/link", post(resolve))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients", post(test_copy_clients).get(test_copy_client_page))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/returns", post(test_copy_returns))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/t5008", post(test_copy_slips))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/summary", get(test_copy_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/legacy-records", get(test_copy_legacy_page))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/legacy-owner-links", post(test_copy_legacy_owner_links))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/legacy-owner-links/summary", get(test_copy_legacy_owner_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/legacy-family-lineage", post(test_copy_legacy_family_lineage))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/legacy-family-lineage/read", post(test_copy_read_legacy_family_lineage))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/legacy-family-lineage/summary", get(test_copy_legacy_family_lineage_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/recovered-native-sources", post(test_copy_recovered_native_sources).layer(axum::extract::DefaultBodyLimit::max(3 * 1024 * 1024)))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/recovered-native-sources/read", post(test_copy_read_recovered_native_sources))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/recovered-native-sources/summary", get(test_copy_recovered_native_source_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/recovered-native-sources/unassigned", get(test_copy_unassigned_recovered_native_sources))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/recovered-native-sources/evidence", get(test_copy_recovered_native_evidence))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-exceptions", post(test_copy_preserve_source_exception).get(test_copy_source_exceptions))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-exceptions/{archive}/{offset}", get(test_copy_source_exception_content))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-slip-views", post(test_copy_native_slip_views))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-slip-supplements", post(test_copy_native_slip_supplements))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-slip-supplements/summary", get(test_copy_native_supplement_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-merged-slip-fields", post(test_copy_native_merged_slip_fields))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-merged-slip-fields/summary", get(test_copy_native_merged_slip_field_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-merged-slip-fields/read", post(test_copy_read_native_merged_slip_fields))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-slip-views/summary", get(test_copy_native_slip_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/family-files", post(test_copy_family_files))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/historical-lines", post(test_copy_historical_lines))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/afr-slips", post(test_copy_afr_slips))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-fact-summary", get(test_copy_source_fact_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-facts/read", post(test_copy_read_source_facts))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/personal-source-info", post(test_copy_personal_source_info))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/personal-source-info/read", post(test_copy_read_personal_source_info))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/personal-source-info/summary", get(test_copy_personal_source_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-module-reviews", post(test_copy_native_module_reviews))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-module-reviews/read", post(test_copy_read_native_module_reviews))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/native-module-reviews/summary", get(test_copy_native_module_review_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/historical-variants", post(test_copy_native_historical_variants))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/historical-variants/read", post(test_copy_read_native_historical_variants))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/historical-variants/summary", get(test_copy_native_historical_variant_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-clients", get(test_copy_source_clients))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients/{client}/source-facts", get(test_copy_client_source_facts))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/return-pdfs", post(test_copy_preserve_return_pdf))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/reviewed-return-packets/chunks", post(test_copy_preserve_reviewed_packet_chunk))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/identification-source-archives/chunks", post(test_copy_preserve_source_archive_chunk))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/identification-source-archives/profile", get(test_copy_source_archive_profile))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/identification-source-archives/additional-profile", get(test_copy_additional_source_archive_profile))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/identification-source-archives/commit", post(test_copy_commit_source_archive))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients/{client}/return-pdfs/{year}/{sha}/archive-chunks/{index}", get(test_copy_source_archive_chunk))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/reviewed-return-packets/profile", get(test_copy_reviewed_packet_profile))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/reviewed-return-packets/commit", post(test_copy_commit_reviewed_packet))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/printed-cached-figures", post(test_copy_printed_cached_figures))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/preserved-pdf-figures", post(test_copy_preserved_pdf_figures))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-summary-figures", post(test_copy_source_summary_figures))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-paper-documents", post(test_copy_source_paper_documents))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-paper-documents/summary", get(test_copy_source_paper_document_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/parsed-native-forms", post(test_copy_parsed_native_forms))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/parsed-native-forms/summary", get(test_copy_parsed_native_form_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/parsed-native-forms/read", post(test_copy_read_parsed_native_forms))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/source-summary-figures/summary", get(test_copy_source_summary_figure_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/preserved-pdf-figures/read", post(test_copy_read_preserved_pdf_figures))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/preserved-pdf-figures/summary", get(test_copy_preserved_pdf_figure_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/printed-cached-figures/read", post(test_copy_read_printed_cached_figures))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/printed-cached-figures/summary", get(test_copy_printed_cached_figure_summary))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients/{client}/return-pdfs", get(test_copy_return_pdfs))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients/{client}/return-pdfs/{year}/{sha}", get(test_copy_return_pdf_content))
        .route("/api/source-imports/dtmax/companies/{company}/test-copy/clients/{client}/return-pdfs/{year}/{sha}/chunks/{index}", get(test_copy_reviewed_packet_chunk))
        .layer(axum::extract::DefaultBodyLimit::max(super::files::MAX_FILE_BYTES * 2 + 64 * 1024))
        .layer(middleware::from_fn(client_scope))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut response: Response| async move {
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            response
        }))
        .with_state(state)
}

/// Client scoping: only `.../clients/{client}/files` names a client, and the coverage catalog
/// holds no client data. Batches, unmatched files and the test copy are firm-wide.
async fn client_scope(req: axum::extract::Request, next: middleware::Next) -> Response {
    let Some(id) = crate::firm::staff::scoped(&req) else { return next.run(req).await };
    let path = req.uri().path();
    if path == "/api/source-imports/coverage" { return next.run(req).await; }
    let client = path.strip_prefix("/api/source-imports/dtmax/companies/")
        .and_then(|rest| rest.split_once("/clients/"))
        .and_then(|(_, rest)| rest.strip_suffix("/files"));
    match client {
        Some(c) if id.can_client(c) => next.run(req).await,
        Some(_) => (axum::http::StatusCode::FORBIDDEN, Json(json!({"error": "you are not assigned to this client"}))).into_response(),
        None => crate::firm::staff::firm_wide_only(req, next).await,
    }
}

async fn coverage() -> Json<Value> {
    Json(SourceImport::coverage())
}
async fn test_copy_legacy_owner_links(State(state): S, Path(company): Path<String>, Json(body): Json<super::legacy_owner_links::LegacyOwnerBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_legacy_owner_links(&company, body).await?))
}
async fn test_copy_legacy_owner_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_legacy_owner_summary(&company).await?))
}
async fn test_copy_legacy_family_lineage(State(state):S,Path(company):Path<String>,Json(body):Json<super::legacy_family_lineage::FamilyLineageBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_legacy_family_lineage(&company,body).await?))}
async fn test_copy_read_legacy_family_lineage(State(state):S,Path(company):Path<String>,Json(body):Json<super::legacy_family_lineage::FamilyLineageLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_legacy_family_lineage(&company,body).await?))}
async fn test_copy_legacy_family_lineage_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_legacy_family_lineage_summary(&company).await?))}

async fn open(State(state): S, Path(lane): Path<String>, Json(batch): Json<OpenBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.open(&lane, batch).await?))
}

async fn progress(State(state): S, Path((lane, batch)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.progress(&lane, &batch).await?))
}

async fn chunk(State(state): S, Path((lane, batch)): Path<(String, String)>, Json(body): Json<ChunkBody>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.chunk(&lane, &batch, body.seq, &body.records).await?))
}

async fn commit(State(state): S, Path((lane, batch)): Path<(String, String)>, Json(body): Json<CommitBody>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.commit(&lane, &batch, &body.request_id, &body.reviewer_label).await?))
}

async fn recover(State(state): S, Path((lane, batch)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.recover(&lane, &batch).await?))
}

async fn parallel(State(state): S, Path((lane, batch)): Path<(String, String)>, Json(body): Json<ParallelInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.parallel(&lane, &batch, body).await?))
}

async fn latest_parallel(State(state): S, Path((lane, batch)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.latest_parallel(&lane, &batch).await?))
}

async fn register_clients(State(state): S, Path(company): Path<String>, Json(body): Json<ClientLinkInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.register_client_links(&company, body).await?))
}

async fn preserve_file(State(state): S, Path(company): Path<String>, Json(body): Json<FileInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.preserve_file(&company, body).await?))
}

async fn unmatched(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.unmatched_files(&company).await?))
}

async fn source_file(State(state): S, Path((company, sha)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.source_file(&company, &sha).await?))
}

async fn source_bytes(State(state): S, Path((company, sha)): Path<(String, String)>) -> Result<Response, ImportError> {
    let bytes = state.source_file_bytes(&company, &sha).await?;
    Ok(([(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"))], bytes).into_response())
}

async fn resolve(State(state): S, Path((company, sha)): Path<(String, String)>, Json(body): Json<ResolveInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.resolve_file(&company, &sha, body).await?))
}

async fn client_files(State(state): S, Path((company, client)): Path<(String, String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.client_files(&company, &client).await?))
}

async fn test_copy_clients(State(state): S, Path(company): Path<String>, Json(body): Json<super::test_copy::ClientBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_clients(&company, body).await?))
}

#[derive(Deserialize)]
struct ClientPage {
    #[serde(default)]
    offset: usize,
    #[serde(default = "client_page_limit")]
    limit: usize,
}
fn client_page_limit() -> usize { 50 }

async fn test_copy_client_page(State(state): S, Path(company): Path<String>, Query(page): Query<ClientPage>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_client_page(&company, page.offset, page.limit).await?))
}

async fn test_copy_returns(State(state): S, Path(company): Path<String>, Json(body): Json<super::test_copy::ReturnBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_returns(&company, body).await?))
}

async fn test_copy_slips(State(state): S, Path(company): Path<String>, Json(body): Json<super::test_copy::SlipBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_slips(&company, body).await?))
}

async fn test_copy_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_summary(&company).await?))
}
async fn test_copy_legacy_page(State(state): S, Path(company): Path<String>, Query(page): Query<super::test_copy::LegacyPage>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_legacy_page(&company,page).await?))
}
async fn test_copy_preserve_source_exception(State(state): S, Path(company): Path<String>, Json(body): Json<super::source_exceptions::SourceExceptionInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_preserve_source_exception(&company,body).await?))
}
async fn test_copy_source_exceptions(State(state): S, Path(company): Path<String>, Query(page): Query<super::source_exceptions::ExceptionPage>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_source_exceptions(&company,page).await?))
}
async fn test_copy_source_exception_content(State(state): S, Path((company,archive,offset)): Path<(String,String,u64)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_source_exception_content(&company,&archive,offset).await?))
}
async fn test_copy_native_slip_views(State(state): S, Path(company): Path<String>, Json(body): Json<super::native_slip_views::NativeSlipBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_native_slip_views(&company,body).await?))
}
async fn test_copy_native_slip_supplements(State(state): S, Path(company): Path<String>, Json(body): Json<super::native_slip_supplements::SupplementBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_native_slip_supplements(&company,body).await?))
}
async fn test_copy_native_supplement_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_native_supplement_summary(&company).await?))
}
async fn test_copy_native_merged_slip_fields(State(state): S, Path(company): Path<String>, Json(body): Json<super::native_merged_slip_fields::MergedFieldBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_native_merged_slip_fields(&company,body).await?))
}
async fn test_copy_native_merged_slip_field_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_native_merged_slip_field_summary(&company).await?))
}
async fn test_copy_read_native_merged_slip_fields(State(state): S, Path(company): Path<String>, Json(body): Json<super::native_merged_slip_fields::MergedFieldLookup>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_read_native_merged_slip_fields(&company,body).await?))
}
async fn test_copy_native_slip_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_native_slip_summary(&company).await?))
}

async fn test_copy_family_files(State(state): S, Path(company): Path<String>, Json(body): Json<super::preserved_records::SourceFactBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_family_files(&company, body).await?))
}
async fn test_copy_historical_lines(State(state): S, Path(company): Path<String>, Json(body): Json<super::preserved_records::SourceFactBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_historical_lines(&company, body).await?))
}
async fn test_copy_afr_slips(State(state): S, Path(company): Path<String>, Json(body): Json<super::preserved_records::SourceFactBatch>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_afr_slips(&company, body).await?))
}
async fn test_copy_source_fact_summary(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_source_fact_summary(&company).await?))
}
async fn test_copy_read_source_facts(State(state): S, Path(company): Path<String>, Json(body): Json<super::preserved_records::SourceFactLookup>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_read_source_facts(&company,body).await?))
}
async fn test_copy_personal_source_info(State(state):S,Path(company):Path<String>,Json(body):Json<super::personal_source_info::PersonalBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_personal_source_info(&company,body).await?))}
async fn test_copy_read_personal_source_info(State(state):S,Path(company):Path<String>,Json(body):Json<super::personal_source_info::PersonalLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_personal_source_info(&company,body).await?))}
async fn test_copy_personal_source_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_personal_source_summary(&company).await?))}
async fn test_copy_native_module_reviews(State(state):S,Path(company):Path<String>,Json(body):Json<super::native_module_reviews::NativeModuleReviewBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_native_module_reviews(&company,body).await?))}
async fn test_copy_read_native_module_reviews(State(state):S,Path(company):Path<String>,Json(body):Json<super::native_module_reviews::NativeModuleReviewLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_native_module_reviews(&company,body).await?))}
async fn test_copy_native_module_review_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_native_module_review_summary(&company).await?))}
async fn test_copy_native_historical_variants(State(state):S,Path(company):Path<String>,Json(body):Json<super::native_historical_variants::HistoricalVariantBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_native_historical_variants(&company,body).await?))}
async fn test_copy_read_native_historical_variants(State(state):S,Path(company):Path<String>,Json(body):Json<super::native_historical_variants::HistoricalVariantLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_native_historical_variants(&company,body).await?))}
async fn test_copy_native_historical_variant_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_native_historical_variant_summary(&company).await?))}
async fn test_copy_recovered_native_sources(State(state):S,Path(company):Path<String>,Json(body):Json<super::recovered_native_sources::RecoveredNativeBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_recovered_native_sources(&company,body).await?))}
async fn test_copy_read_recovered_native_sources(State(state):S,Path(company):Path<String>,Json(body):Json<super::recovered_native_sources::RecoveredNativeLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_recovered_native_sources(&company,body).await?))}
async fn test_copy_recovered_native_source_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_recovered_native_source_summary(&company).await?))}
async fn test_copy_unassigned_recovered_native_sources(State(state):S,Path(company):Path<String>,Query(page):Query<super::recovered_native_sources::RecoveredNativePage>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_unassigned_recovered_native_sources(&company,page).await?))}
async fn test_copy_recovered_native_evidence(State(state):S,Path(company):Path<String>,Query(page):Query<super::recovered_native_sources::RecoveredNativePage>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_recovered_native_evidence(&company,page).await?))}
async fn test_copy_source_clients(State(state): S, Path(company): Path<String>, Query(page): Query<super::preserved_records::SourceClientPage>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_source_clients(&company,page).await?))
}
async fn test_copy_client_source_facts(State(state): S, Path((company,client)): Path<(String,String)>, Query(page): Query<super::preserved_records::SourceFactPage>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_client_source_facts(&company,&client,page).await?))
}
async fn test_copy_preserve_return_pdf(State(state): S, Path(company): Path<String>, Json(body): Json<super::return_pdfs::ReturnPdfInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_preserve_return_pdf(&company,body).await?))
}
async fn test_copy_preserve_source_archive_chunk(State(state): S, Path(company): Path<String>, Json(body): Json<super::return_pdfs::source_archives::ArchiveChunkInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_preserve_source_archive_chunk(&company,body).await?))
}
async fn test_copy_source_archive_profile(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_source_archive_profile(&company).await?))
}
async fn test_copy_additional_source_archive_profile(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_additional_source_archive_profile(&company).await?))
}
async fn test_copy_commit_source_archive(State(state): S, Path(company): Path<String>, Json(body): Json<super::return_pdfs::source_archives::ArchiveCommitInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_commit_source_archive(&company,body).await?))
}
async fn test_copy_source_archive_chunk(State(state): S, Path((company,client,year,sha,index)): Path<(String,String,u16,String,usize)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_source_archive_chunk(&company,&client,year,&sha,index).await?))
}
async fn test_copy_preserve_reviewed_packet_chunk(State(state): S, Path(company): Path<String>, Json(body): Json<super::return_pdfs::reviewed_packets::PacketChunkInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_preserve_reviewed_packet_chunk(&company,body).await?))
}
async fn test_copy_reviewed_packet_profile(State(state): S, Path(company): Path<String>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_reviewed_packet_profile(&company).await?))
}
async fn test_copy_commit_reviewed_packet(State(state): S, Path(company): Path<String>, Json(body): Json<super::return_pdfs::reviewed_packets::PacketCommitInput>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_commit_reviewed_packet(&company,body).await?))
}
async fn test_copy_reviewed_packet_chunk(State(state): S, Path((company,client,year,sha,index)): Path<(String,String,u16,String,usize)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_reviewed_packet_chunk(&company,&client,year,&sha,index).await?))
}
async fn test_copy_printed_cached_figures(State(state):S,Path(company):Path<String>,Json(body):Json<super::printed_cached_figures::PrintedFigureBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_printed_cached_figures(&company,body).await?))}
async fn test_copy_read_printed_cached_figures(State(state):S,Path(company):Path<String>,Json(body):Json<super::printed_cached_figures::PrintedFigureLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_printed_cached_figures(&company,body).await?))}
async fn test_copy_printed_cached_figure_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_printed_cached_figure_summary(&company).await?))}
async fn test_copy_return_pdfs(State(state): S, Path((company,client)): Path<(String,String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_return_pdfs(&company,&client).await?))
}
async fn test_copy_return_pdf_content(State(state): S, Path((company,client,year,sha)): Path<(String,String,u16,String)>) -> Result<Json<Value>, ImportError> {
    Ok(Json(state.test_copy_return_pdf_content(&company,&client,year,&sha).await?))
}

async fn test_copy_preserved_pdf_figures(State(state):S,Path(company):Path<String>,Json(body):Json<super::preserved_pdf_figures::PreservedPdfFigureBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_preserved_pdf_figures(&company,body).await?))}
async fn test_copy_read_preserved_pdf_figures(State(state):S,Path(company):Path<String>,Json(body):Json<super::preserved_pdf_figures::PreservedPdfFigureLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_preserved_pdf_figures(&company,body).await?))}
async fn test_copy_preserved_pdf_figure_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_preserved_pdf_figure_summary(&company).await?))}
async fn test_copy_source_summary_figures(State(state):S,Path(company):Path<String>,Json(body):Json<super::source_summary_figures::SummaryFigureInput>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_source_summary_figures(&company,body).await?))}
async fn test_copy_source_summary_figure_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_source_summary_figure_summary(&company).await?))}
async fn test_copy_source_paper_documents(State(state):S,Path(company):Path<String>,Json(body):Json<super::source_paper_documents::PaperDocumentInput>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_source_paper_documents(&company,body).await?))}
async fn test_copy_source_paper_document_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_source_paper_document_summary(&company).await?))}
async fn test_copy_parsed_native_forms(State(state):S,Path(company):Path<String>,Json(body):Json<super::parsed_native_forms::ParsedNativeBatch>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_parsed_native_forms(&company,body).await?))}
async fn test_copy_parsed_native_form_summary(State(state):S,Path(company):Path<String>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_parsed_native_form_summary(&company).await?))}
async fn test_copy_read_parsed_native_forms(State(state):S,Path(company):Path<String>,Json(body):Json<super::parsed_native_forms::ParsedNativeLookup>)->Result<Json<Value>,ImportError>{Ok(Json(state.test_copy_read_parsed_native_forms(&company,body).await?))}
