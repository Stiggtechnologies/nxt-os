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
        .route("/api/tax/returns/{id}/rollforward",post(rollforward))
        .route("/api/tax/returns/{id}/package",get(package))
        .route("/api/tax/returns/{id}/print",get(print))
        .route("/api/tax/returns/{id}/filing",post(filing))
        .route("/api/tax/import/preview",post(preview))
        .route("/api/tax/import/previews/{id}",get(import_preview))
        .route("/api/tax/import/commit",post(commit))
        .route("/api/tax/audit",get(audit))
        .layer(DefaultBodyLimit::max(1024 * 1024))
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
    t.get(&id).await?;
    Err(conflict("Electronic filing is disabled. NXT is not certified and cannot transmit tax returns or mark them filed/accepted."))
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
