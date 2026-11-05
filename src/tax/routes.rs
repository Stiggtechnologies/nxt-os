use super::{Tax,TResult,bad,conflict,models::*};
use crate::referral::routes::require_admin;
use axum::{Json, Router, body::Body, extract::{DefaultBodyLimit, Path, Query, State}, http::{StatusCode, header}, middleware, response::{IntoResponse, Response}, routing::{get,post}};
use serde::Deserialize;
use serde_json::{Value,json};
use std::sync::Arc;

type S = State<Arc<Tax>>;
type R = TResult<Json<Value>>;
fn ok(v: impl serde::Serialize) -> R { Ok(Json(json!(v))) }

pub fn router(tax: Arc<Tax>) -> Router {
    Router::new()
        .route("/api/tax/catalog",get(catalog))
        .route("/api/tax/capabilities",get(capabilities))
        .route("/api/tax/summary",get(summary))
        .route("/api/tax/returns",get(returns).post(create))
        .route("/api/tax/returns/{id}",get(detail).put(update))
        .route("/api/tax/returns/{id}/calculate",post(calculate))
        .route("/api/tax/returns/{id}/transition",post(transition))
        .route("/api/tax/returns/{id}/t183",get(t183_draft))
        .route("/api/tax/returns/{id}/rollforward",post(rollforward))
        .route("/api/tax/returns/{id}/package",get(package))
        .route("/api/tax/returns/{id}/print",get(print))
        .route("/api/tax/returns/{id}/filing",post(filing))
        .route("/api/tax/desk/me",get(desk_me))
        .route("/api/tax/desk/queue",get(desk_queue))
        .route("/api/tax/desk/log",get(desk_log))
        .route("/api/tax/desk/filers",get(desk_filers).post(desk_set_filer))
        .route("/api/tax/desk/launch",get(desk_launch).post(desk_set_launch))
        .route("/api/tax/desk/launch/test-run",post(desk_test_run))
        .route("/api/tax/desk/step-up/sms",post(desk_step_up))
        .route("/api/tax/returns/{id}/desk",get(desk_detail))
        .route("/api/tax/returns/{id}/desk/fee",post(desk_fee))
        .route("/api/tax/returns/{id}/desk/seen",post(desk_seen))
        .route("/api/tax/returns/{id}/desk/confirm",post(desk_confirm))
        .route("/api/tax/returns/{id}/parallel-compare",post(parallel_compare))
        .route("/api/tax/returns/{id}/prior-lines",post(prior_lines))
        .route("/api/tax/families",post(create_family))
        .route("/api/tax/families/{id}",get(family))
        .route("/api/tax/families/by-return/{id}",get(family_by_return))
        .route("/api/tax/families/{id}/optimize",post(family_optimize))
        .route("/api/tax/families/{id}/accept",post(family_accept))
        .route("/api/tax/slips/catalog",get(slips_catalog))
        .route("/api/tax/t2/intake",post(t2_intake))
        .route("/api/tax/t2/intake/start",post(t2_intake_start))
        .route("/api/tax/import/preview",post(preview))
        .route("/api/tax/import/previews/{id}",get(import_preview))
        .route("/api/tax/import/commit",post(commit))
        .route("/api/tax/audit",get(audit))
        .route("/api/tax/audit/verify",get(audit_verify))
        .route("/api/tax/rates",get(rates))
        .route("/api/tax/slips/aggregate",post(slips_aggregate))
        .route("/api/tax/gifi/from-trial-balance",post(gifi_from_tb))
        .route("/api/tax/t2/schedules",post(t2_schedules))
        .route("/api/tax/t5013/cca",post(t5013_cca))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn_with_state(tax.clone(), client_scope))
        .layer(middleware::from_fn(require_admin))
        .layer(middleware::map_response(|mut response: Response| async move {
            response.headers_mut().insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
            response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
            response
        }))
        .with_state(tax)
}

async fn catalog() -> R { ok(super::catalog::catalog()) }
async fn capabilities() -> R { ok(Tax::capabilities()) }
async fn summary(State(t):S) -> R { ok(t.summary().await?) }
#[derive(Deserialize)] struct Filter { program:Option<String> }
async fn returns(State(t):S,Query(q):Query<Filter>) -> R { ok(t.list(q.program.as_deref()).await?) }
async fn detail(State(t):S,Path(id):Path<String>) -> R { ok(t.get(&id).await?) }
async fn create(State(t):S,Json(input):Json<ReturnInput>) -> R { ok(t.save(None,input).await?) }
async fn update(State(t):S,Path(id):Path<String>,Json(input):Json<ReturnInput>) -> R { ok(t.save(Some(&id),input).await?) }
async fn calculate(State(t):S,Path(id):Path<String>,Json(input):Json<RevisionInput>) -> R { ok(t.calculate(&id,input).await?) }
async fn transition(State(t):S,Path(id):Path<String>,Json(input):Json<TransitionInput>) -> R { ok(t.transition(&id,input).await?) }
async fn rollforward(State(t):S,Path(id):Path<String>,Json(input):Json<RollforwardInput>) -> R { ok(t.rollforward(&id,input).await?) }
#[derive(Deserialize)] struct AuditQuery { #[serde(default)] offset:usize, limit:Option<usize> }
async fn audit(State(t):S,Query(q):Query<AuditQuery>) -> R {
    let limit=q.limit.unwrap_or(200);
    if !(1..=200).contains(&limit) { return Err(bad("audit limit must be between 1 and 200")); }
    ok(t.audit().await?.into_iter().skip(q.offset).take(limit).collect::<Vec<_>>())
}
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct PreviewInput { document:Value }
async fn preview(State(t):S,Json(input):Json<PreviewInput>) -> R { ok(t.preview(input.document).await?) }
async fn import_preview(State(t):S,Path(id):Path<String>) -> R { ok(t.import_preview(&id).await?) }
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct CommitInput { preview_id:String }
async fn commit(State(t):S,Json(input):Json<CommitInput>) -> R { ok(t.commit_import(&input.preview_id).await?) }
async fn filing(State(t):S,Path(id):Path<String>) -> R {
    // Always refuses (409 CONFLICT): NXT never transmits. The Filing Desk calls the same stub.
    Err(t.filing_refusal(&id).await)
}

// ---- Filing Desk (internal staff only; see src/tax/filing_desk.rs) ----
use super::filing_desk as fd;
use axum::Extension;
use crate::firm::staff::Identity;
type W = Extension<Identity>;
#[derive(Deserialize)] struct LogQuery { day:Option<String>, filer:Option<String> }
async fn desk_me(State(t):S,Extension(w):W) -> R { ok(t.desk_me(&w).await?) }
async fn desk_queue(State(t):S,Extension(w):W) -> R { ok(t.desk_queue(&w).await?) }
async fn desk_log(State(t):S,Extension(w):W,Query(q):Query<LogQuery>) -> R { ok(t.desk_log(&w,q.day.as_deref(),q.filer.as_deref()).await?) }
async fn desk_filers(State(t):S,Extension(w):W) -> R { ok(t.desk_filers(&w).await?) }
async fn desk_set_filer(State(t):S,Extension(w):W,Json(i):Json<fd::FilerInput>) -> R { ok(t.desk_set_filer(&w,i).await?) }
async fn desk_launch(State(t):S,Extension(_w):W) -> R { ok(t.desk_launch().await?) }
async fn desk_set_launch(State(t):S,Extension(w):W,Json(i):Json<fd::LaunchInput>) -> R { ok(t.desk_set_launch(&w,i).await?) }
async fn desk_test_run(State(t):S,Extension(w):W,Json(i):Json<fd::TestRunInput>) -> R { ok(t.desk_record_test_run(&w,i).await?) }
async fn desk_step_up(State(t):S,Extension(w):W,Json(i):Json<fd::StepUpSmsInput>) -> R { ok(t.desk_step_up_sms(&w,i).await?) }
async fn desk_detail(State(t):S,Extension(w):W,Path(id):Path<String>) -> R { ok(t.desk_detail(&w,&id).await?) }
async fn desk_fee(State(t):S,Extension(w):W,Path(id):Path<String>,Json(i):Json<fd::FeeInput>) -> R { ok(t.desk_fee(&w,&id,i).await?) }
async fn desk_seen(State(t):S,Extension(w):W,Path(id):Path<String>,Json(i):Json<fd::SeenInput>) -> R { ok(t.desk_seen(&w,&id,i).await?) }
async fn desk_confirm(State(t):S,Extension(w):W,Path(id):Path<String>,Json(i):Json<fd::ConfirmInput>) -> R { ok(t.desk_confirm(&w,&id,i).await?) }
async fn parallel_compare(State(t):S,Path(id):Path<String>,Json(source):Json<super::parallel::ParallelSource>) -> R {
    ok(t.compare_parallel(&id, source).await?)
}
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct FamilyInput { label: String, primary_return_id: String, spouse_return_id: String }
async fn create_family(State(t):S,Json(input):Json<FamilyInput>) -> R {
    ok(t.link_family(&input.label, &input.primary_return_id, &input.spouse_return_id).await?)
}
async fn family(State(t):S,Path(id):Path<String>) -> R { ok(t.family(&id).await?) }
async fn family_by_return(State(t):S,Path(id):Path<String>) -> R { ok(t.family_for_return(&id).await?) }
async fn family_optimize(State(t):S,Path(id):Path<String>) -> R { ok(t.optimize_family(&id).await?) }
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct AcceptInput { kind: String }
async fn family_accept(State(t):S,Path(id):Path<String>,Json(input):Json<AcceptInput>) -> R { ok(t.accept_proposal(&id,&input.kind).await?) }
async fn slips_catalog() -> R { ok(super::slips::catalog()) }
fn today() -> chrono::NaiveDate { chrono::Local::now().date_naive() }
async fn t2_intake(State(t):S,Json(input):Json<super::t2_intake::IntakeAnswers>) -> R { ok(t.t2_intake(&input,today()).await?) }
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct IntakeStart { answers: super::t2_intake::IntakeAnswers, name: String, #[serde(default)] carry_forward_from: Option<String> }
async fn t2_intake_start(State(t):S,Json(input):Json<IntakeStart>) -> R { ok(t.t2_intake_start(&input.answers,&input.name,input.carry_forward_from.as_deref(),today()).await?) }
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct PriorInput { expected_revision: u64, lines: Vec<super::models::PriorLine> }
async fn prior_lines(State(t):S,Path(id):Path<String>,Json(input):Json<PriorInput>) -> R {
    ok(t.set_prior_lines(&id, input.expected_revision, input.lines).await?)
}

async fn package(State(t):S,Path(id):Path<String>) -> TResult<Response> {
    let value=t.package(&id).await?;
    let bytes=serde_json::to_vec_pretty(&value).map_err(|_|bad("workpaper could not be encoded"))?;
    if bytes.len()>4*1024*1024 { return Err(conflict("workpaper export exceeds its 4 MiB download limit")); }
    Ok(([(header::CONTENT_TYPE,"application/json; charset=utf-8".to_string()),(header::CONTENT_DISPOSITION,format!("attachment; filename=\"nxt-tax-{id}.json\"")),(header::CACHE_CONTROL,"no-store".into())],bytes).into_response())
}

fn esc(s:&str)->String { s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&#39;") }
fn money(v:i64)->String { let sign=if v<0{"−"}else{""};let n=v.unsigned_abs();format!("{sign}${}.{:02}",n/100,n%100) }
async fn print(State(t):S,Path(id):Path<String>) -> TResult<Response> {
    let r=t.get(&id).await?;
    let mut body=format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{} — NXT tax workpaper</title><style>body{{font:14px system-ui;margin:40px;color:#17201e}}h1{{font-size:26px}}.notice{{padding:16px;border:2px solid #957126;background:#fff7df}}table{{border-collapse:collapse;width:100%;margin:20px 0}}th,td{{text-align:left;padding:9px;border-bottom:1px solid #ddd}}td:last-child{{text-align:right;font-variant-numeric:tabular-nums}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;background:#f4f5f3;padding:16px}}.hash{{font:11px monospace;overflow-wrap:anywhere}}@media print{{body{{margin:0}}}}</style></head><body><h1>NXT Tax · {} · {}</h1><p>{} · {} · Revision {} · {}</p><p class=\"notice\"><strong>Internal workpaper. Not a certified tax return or filing.</strong><br>Review does not establish CRA/Revenu Québec acceptance. Calculation scope and diagnostics below must be read with this workpaper.</p>",esc(&r.name),esc(&r.program),r.tax_year,esc(&r.name),esc(&r.jurisdiction),r.revision,esc(&r.status));
    if let Some(c)=&r.calculation {
        body.push_str(&format!("<h2>Calculation {}</h2><p>{}</p><p>Ruleset: {}</p><table><thead><tr><th>Line</th><th>Description</th><th>CAD</th></tr></thead><tbody>",esc(&c.id),esc(c.result["scope"].as_str().unwrap_or("Scope not provided")),esc(c.result["ruleset"].as_str().unwrap_or("unknown"))));
        if let Some(lines)=c.result["lines"].as_array(){ for line in lines { body.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td></tr>",esc(line["code"].as_str().unwrap_or("")),esc(line["label"].as_str().unwrap_or("")),money(line["amount"].as_i64().unwrap_or(0)))); } }
        body.push_str("</tbody></table><h2>Workpaper totals</h2><table><thead><tr><th>Total</th><th>CAD</th></tr></thead><tbody>");
        if let Some(totals)=c.result["totals"].as_object(){for (key,value) in totals{body.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>",esc(key),money(value.as_i64().unwrap_or(0))));}}
        body.push_str("</tbody></table><h2>Diagnostics</h2><ul>");
        if let Some(diags)=c.result["diagnostics"].as_array(){for d in diags{body.push_str(&format!("<li><strong>{}</strong> {} {}</li>",esc(d["severity"].as_str().unwrap_or("")),esc(d["code"].as_str().unwrap_or("")),esc(d["message"].as_str().unwrap_or(""))));}}
        body.push_str(&format!("</ul><p class=\"hash\">Input SHA-256: {}<br>Output SHA-256: {}</p>",esc(&c.input_hash),esc(&c.output_hash)));
    }else{body.push_str("<h2>Uncalculated draft</h2><p>No calculation or tax amount has been verified.</p>");}
    if let Some(review)=&r.review{body.push_str(&format!("<h2>Internal review</h2><p>{} · {} (workspace responsibility label)</p><pre>{}</pre>",esc(&review.reviewer),esc(&review.at),esc(&review.note)));}
    body.push_str(&format!("<h2>Inputs</h2><pre>{}</pre><h2>Notes</h2><pre>{}</pre><p>Generated by the independent NXT engine. No DT Max runtime, CRA transmission or certified form is used.</p></body></html>",esc(&serde_json::to_string_pretty(&r.inputs).unwrap_or_default()),esc(&r.notes)));
    if body.len()>4*1024*1024 { return Err(conflict("printable workpaper exceeds its 4 MiB limit")); }
    Ok((StatusCode::OK,[(header::CONTENT_TYPE,"text/html; charset=utf-8"),(header::CACHE_CONTROL,"no-store"),(header::CONTENT_SECURITY_POLICY,"default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'self'; base-uri 'none'")],Body::from(body)).into_response())
}

async fn audit_verify(State(t):S) -> R { ok(super::verify_audit(&t.audit().await?)) }
async fn rates() -> R { ok(super::rates::catalog()) }
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct SlipsInput { slips: Vec<super::slips::Slip> }
async fn slips_aggregate(Json(input):Json<SlipsInput>) -> R { ok(super::slips::aggregate(&input.slips).map_err(super::bad)?) }
/// Body: the `rows` from GET /api/company-books/{company}/trial-balance plus an explicit account -> GIFI mapping.
#[derive(Deserialize)] #[serde(deny_unknown_fields)] struct GifiInput { balances: Vec<crate::company_books::models::Balance>, mapping: Vec<super::gifi_map::Mapping> }
async fn gifi_from_tb(Json(input):Json<GifiInput>) -> R {
    if input.balances.len()>5000 { return Err(super::bad("at most 5000 trial-balance rows")); }
    ok(super::gifi_map::build(&input.balances,&input.mapping).map_err(super::bad)?)
}

async fn t5013_cca(Json(input):Json<super::t5013_cca::T5013CcaInput>) -> R { ok(super::t5013_cca::build(&input).map_err(super::bad)?) }
async fn t2_schedules(Json(input):Json<super::t2_schedules::T2Input>) -> R { ok(super::t2_schedules::build(&input).map_err(super::bad)?) }

/// Returns a src/signatures Draft (context module "tax"); POST it to /api/signatures to prepare the envelope.
async fn t183_draft(State(t):S,Path(id):Path<String>) -> R { let r=t.get(&id).await?; ok(super::t183::draft(&r).map_err(|e|conflict(&e))?) }

async fn client_scope(State(t): S, req: axum::extract::Request, next: middleware::Next) -> Response {
    crate::firm::staff::scope_guard(req, next, "/api/tax/returns", &["/api/tax/catalog", "/api/tax/capabilities", "/api/tax/rates", "/api/tax/t2/schedules", "/api/tax/t5013/cca", "/api/tax/slips/catalog", "/api/tax/slips/aggregate", "/api/tax/desk/me", "/api/tax/desk/queue", "/api/tax/desk/log", "/api/tax/desk/step-up/sms", "/api/tax/desk/launch"],
        |id| async move { t.get(&id).await.ok().map(|r| r.client_ref) }).await
}
