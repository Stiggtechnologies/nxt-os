use super::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::header,
    middleware,
    response::Response,
    routing::{get, post},
};
use std::sync::Arc;
type S = State<Arc<Signatures>>;
type R = SResult<Json<Value>>;
pub fn router(state: Arc<Signatures>) -> Router {
    core_router(state.clone()).merge(esign_staff_router(state.clone())).merge(signer_router(state))
}
fn no_store(mut r: Response) -> Response {
    r.headers_mut().insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    r.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    r.headers_mut().insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    r
}
fn core_router(state: Arc<Signatures>) -> Router {
    Router::new()
        .route("/api/signatures/catalog", get(catalog))
        .route("/api/signatures", get(list).post(create))
        .route("/api/signatures/audit", get(audit))
        .route("/api/signatures/{id}", get(detail).put(update))
        .route("/api/signatures/{id}/transition", post(transition))
        .route("/api/signatures/{id}/folder", post(assign_folder))
        .route("/api/signatures/{id}/lock", post(lock_envelope))
        .route("/api/signatures/{id}/unlock", post(unlock_envelope))
        .route("/api/signatures/{id}/bulk-rehearsal", post(bulk))
        .route("/api/signatures/folders", post(create_folder))
        .route("/api/signatures/{id}/template", post(create_template))
        .route("/api/signatures/{id}/package", get(package))
        .route("/api/signatures/templates/{id}", get(template))
        .route(
            "/api/signatures/templates/{id}/instantiate",
            post(instantiate),
        )
        .route("/api/signatures/templates/{id}/favorite", post(favorite))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut()
                .insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
            r.headers_mut()
                .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
            r
        }))
        .with_state(state)
}
async fn catalog() -> R {
    Ok(Json(Signatures::catalog()))
}
async fn list(State(s): S) -> R {
    Ok(Json(s.list().await?))
}
async fn detail(State(s): S, Path(id): Path<String>) -> R {
    Ok(Json(s.get(&id).await?))
}
async fn create(State(s): S, Json(v): Json<SaveInput>) -> R {
    Ok(Json(s.save(None, v).await?))
}
async fn update(State(s): S, Path(id): Path<String>, Json(v): Json<SaveInput>) -> R {
    Ok(Json(s.save(Some(&id), v).await?))
}
async fn transition(State(s): S, Path(id): Path<String>, Json(v): Json<TransitionInput>) -> R {
    Ok(Json(s.transition(&id, v).await?))
}
async fn create_template(State(s): S, Path(id): Path<String>, Json(v): Json<TemplateInput>) -> R {
    Ok(Json(s.create_template(&id, v).await?))
}
async fn template(State(s): S, Path(id): Path<String>) -> R {
    Ok(Json(s.template(&id).await?))
}
async fn instantiate(State(s): S, Path(id): Path<String>, Json(v): Json<InstantiateInput>) -> R {
    Ok(Json(s.instantiate(&id, v).await?))
}
async fn package(State(s): S, Path(id): Path<String>) -> R {
    Ok(Json(s.package(&id).await?))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderName { name: String }
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FavoriteInput { favorite: bool }
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkInput { expected_revision: u64, expected_content_hash: String, operator_label: String, note: String, copies: u8 }
async fn create_folder(State(s): S, Json(v): Json<FolderName>) -> R { Ok(Json(s.create_folder(&v.name).await?)) }
async fn assign_folder(State(s): S, Path(id): Path<String>, Json(v): Json<models::TransitionInput>) -> R { Ok(Json(s.assign_folder(&id, v).await?)) }
async fn lock_envelope(State(s): S, Path(id): Path<String>, Json(v): Json<models::TransitionInput>) -> R { Ok(Json(s.set_lock(&id, v, true).await?)) }
async fn unlock_envelope(State(s): S, Path(id): Path<String>, Json(v): Json<models::TransitionInput>) -> R { Ok(Json(s.set_lock(&id, v, false).await?)) }
async fn favorite(State(s): S, Path(id): Path<String>, Json(v): Json<FavoriteInput>) -> R { Ok(Json(s.favorite_template(&id, v.favorite).await?)) }
async fn bulk(State(s): S, Path(id): Path<String>, Json(v): Json<BulkInput>) -> R {
    let input = models::TransitionInput { expected_revision: v.expected_revision, expected_content_hash: v.expected_content_hash, operator_label: v.operator_label, action: "parallel_bulk".into(), note: v.note, field_values: Vec::new() };
    Ok(Json(s.bulk_rehearse(&id, input, v.copies).await?))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditQuery {
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
    #[serde(default, rename = "path")]
    transport_path: Option<String>,
}
async fn audit(State(s): S, Query(q): Query<AuditQuery>) -> R {
    if let Some(path) = &q.transport_path {
        let path = path.trim_start_matches('/');
        let path = path.strip_prefix("api/").unwrap_or(path);
        if path != "signatures/audit" {
            return Err(bad("Transport path does not match the requested audit"));
        }
    }
    Ok(Json(s.audit(q.offset, q.limit.unwrap_or(100)).await?))
}

// ---------- e-sign ----------
use super::esign::{self, ClientMeta};
use axum::{http::HeaderMap, response::IntoResponse};
fn meta(h: &HeaderMap) -> ClientMeta {
    let get = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let ip = get("x-forwarded-for").split(',').next().unwrap_or("").trim().to_string();
    let ip = if ip.is_empty() { get("x-real-ip") } else { ip };
    ClientMeta { ip: if ip.is_empty() { "unknown".into() } else { ip }, user_agent: get("user-agent") }
}
fn pdf(bytes: Vec<u8>, name: &str) -> Response {
    ([(header::CONTENT_TYPE, "application/pdf".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))], bytes).into_response()
}
fn esign_staff_router(state: Arc<Signatures>) -> Router {
    Router::new()
        .route("/api/signatures/esign/catalog", get(|| async { Json(Signatures::esign_catalog()) }))
        .route("/api/signatures/esign/builtin/{key}", post(builtin_new))
        .route("/api/signatures/{id}/esign", get(es_status))
        .route("/api/signatures/{id}/esign/pdf", post(es_pdf))
        .route("/api/signatures/{id}/esign/send", post(es_send))
        .route("/api/signatures/{id}/esign/link", post(es_link))
        .route("/api/signatures/{id}/esign/void", post(es_void))
        .route("/api/signatures/{id}/esign/document/{which}", get(es_doc))
        .route("/api/signatures/{id}/esign/outbox", get(es_outbox))
        .route("/api/signatures/esign/bulk/{key}", post(es_bulk_prepare))
        .route("/api/signatures/esign/bulk-approve", post(es_bulk_approve))
        .route("/api/signatures/esign/bulk-send", post(es_bulk_send))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024 + 64 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|r: Response| async move { no_store(r) }))
        .with_state(state)
}
async fn builtin_new(State(s): S, Path(k): Path<String>, Json(v): Json<esign::BuiltinInput>) -> R { Ok(Json(s.instantiate_builtin(&k, v).await?)) }
async fn es_status(State(s): S, Path(id): Path<String>) -> R { Ok(Json(s.esign_status(&id).await?)) }
async fn es_pdf(State(s): S, Path(id): Path<String>, h: HeaderMap, Json(v): Json<esign::PdfInput>) -> R { Ok(Json(s.attach_pdf(&id, v, meta(&h)).await?)) }
async fn es_send(State(s): S, Path(id): Path<String>, h: HeaderMap, Json(v): Json<esign::SendInput>) -> R { Ok(Json(s.send(&id, v, meta(&h)).await?)) }
async fn es_link(State(s): S, Path(id): Path<String>, h: HeaderMap, Json(v): Json<esign::LinkInput>) -> R { Ok(Json(s.reissue_link(&id, v, meta(&h)).await?)) }
async fn es_void(State(s): S, Path(id): Path<String>, h: HeaderMap, Json(v): Json<esign::StaffInput>) -> R { Ok(Json(s.void(&id, v, meta(&h)).await?)) }
async fn es_doc(State(s): S, Path((id, which)): Path<(String, String)>) -> SResult<Response> {
    if !["original", "signed", "certificate"].contains(&which.as_str()) { return Err(bad("document must be original, signed or certificate")); }
    Ok(pdf(s.completed_pdf(&id, &which).await?, &format!("{id}-{which}.pdf")))
}
/// Signer ceremony. The bearer of the unguessable link is the only credential,
/// plus the email one-time code. No staff key.
fn signer_router(state: Arc<Signatures>) -> Router {
    Router::new()
        .route("/api/sign/{token}", get(sg_get))
        .route("/api/sign/{token}/otp", post(sg_otp))
        .route("/api/sign/{token}/verify", post(sg_verify))
        .route("/api/sign/{token}/consent", post(sg_consent))
        .route("/api/sign/{token}/adopt", post(sg_adopt))
        .route("/api/sign/{token}/sign", post(sg_sign))
        .route("/api/sign/{token}/decline", post(sg_decline))
        .route("/api/sign/{token}/document", get(sg_doc))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::map_response(|r: Response| async move { no_store(r) }))
        .with_state(state)
}
async fn sg_get(State(s): S, Path(t): Path<String>) -> R { Ok(Json(s.signer_get(&t).await?)) }
async fn sg_otp(State(s): S, Path(t): Path<String>, h: HeaderMap) -> R { Ok(Json(s.otp_request(&t, meta(&h)).await?)) }
async fn sg_verify(State(s): S, Path(t): Path<String>, h: HeaderMap, Json(v): Json<esign::CodeInput>) -> R { Ok(Json(s.otp_verify(&t, v, meta(&h)).await?)) }
async fn sg_consent(State(s): S, Path(t): Path<String>, h: HeaderMap, Json(v): Json<esign::ConsentInput>) -> R { Ok(Json(s.consent(&t, v, meta(&h)).await?)) }
async fn sg_adopt(State(s): S, Path(t): Path<String>, h: HeaderMap, Json(v): Json<esign::AdoptInput>) -> R { Ok(Json(s.adopt(&t, v, meta(&h)).await?)) }
async fn sg_sign(State(s): S, Path(t): Path<String>, h: HeaderMap, Json(v): Json<esign::SignInput>) -> R { Ok(Json(s.sign(&t, v, meta(&h)).await?)) }
async fn sg_decline(State(s): S, Path(t): Path<String>, h: HeaderMap, Json(v): Json<esign::DeclineInput>) -> R { Ok(Json(s.decline(&t, v, meta(&h)).await?)) }
async fn sg_doc(State(s): S, Path(t): Path<String>) -> SResult<Response> { Ok(pdf(s.signer_pdf(&t).await?, "document.pdf")) }
async fn es_outbox(State(s): S, Path(id): Path<String>) -> R { Ok(Json(s.outbox(&id).await?)) }
async fn es_bulk_prepare(State(s): S, Path(k): Path<String>, h: HeaderMap, Json(v): Json<esign::BulkPrepareInput>) -> R { Ok(Json(s.bulk_prepare(&k, v, meta(&h)).await?)) }
async fn es_bulk_approve(State(s): S, Json(v): Json<esign::BulkIdsInput>) -> R { Ok(Json(s.bulk_approve(v).await?)) }
async fn es_bulk_send(State(s): S, h: HeaderMap, Json(v): Json<esign::BulkIdsInput>) -> R { Ok(Json(s.bulk_send(v, meta(&h)).await?)) }
