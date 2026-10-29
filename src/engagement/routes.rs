use super::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
type S = State<Arc<Engagements>>;
type R = EResult<Json<Value>>;
fn ok(v: impl serde::Serialize) -> R {
    Ok(Json(json!(v)))
}
pub fn router(state: Arc<Engagements>) -> Router {
    Router::new()
        .route("/api/engagements/catalog", get(catalog))
        .route("/api/engagements/summary", get(summary))
        .route("/api/engagements/audit", get(events))
        .route("/api/engagements", get(list).post(create))
        .route("/api/engagements/{id}", get(detail).put(update))
        .route("/api/engagements/{id}/calculate", post(calculate))
        .route("/api/engagements/{id}/signoffs", post(signoff))
        .route("/api/engagements/{id}/transition", post(transition))
        .route("/api/engagements/{id}/lockdown", post(lockdown))
        .route("/api/engagements/{id}/gifi", get(gifi))
        .route("/api/engagements/{id}/statements", get(statements))
        .route("/api/engagements/{id}/file-tickmarks", post(file_tickmark))
        .route("/api/engagements/{id}/rollforward", post(rollforward))
        .route("/api/engagements/{id}/import/preview", post(preview))
        .route("/api/engagements/{id}/import/commit", post(commit))
        .route("/api/engagements/{id}/ledger/preview", post(ledger_preview))
        .route("/api/engagements/{id}/tax/preview", post(tax_preview))
        .route("/api/engagements/{id}/firm-proposal", post(firm_proposal))
        .route("/api/engagements/{id}/package", get(package))
        .route("/api/engagements/{id}/attachments/package", get(file_package))
        .route("/api/engagements/{id}/workpapers/{paper}/attachments", post(file_upload))
        .route("/api/engagements/{id}/workpapers/{paper}/account-anchors", get(account_anchor_options))
        .route("/api/engagements/{id}/workpapers/{paper}/attachments/{version}", get(file_download))
        .route("/api/engagements/{id}/workpapers/{paper}/attachments/{version}/describe", get(file_describe))
        .route("/api/engagements/{id}/print", get(print))
        .layer(DefaultBodyLimit::max(1024 * 1024))
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
    ok(Engagements::catalog())
}
async fn summary(State(e): S) -> R {
    ok(e.summary().await?)
}
async fn list(State(e): S) -> R {
    ok(e.list().await?)
}
async fn detail(State(e): S, Path(id): Path<String>) -> R {
    ok(e.get(&id).await?)
}
async fn account_anchor_options(State(e): S, Path((id, paper)): Path<(String, String)>) -> R {
    ok(e.account_anchor_options(&id, &paper).await?)
}
async fn create(State(e): S, Json(v): Json<EngagementInput>) -> R {
    ok(e.save(None, v).await?)
}
async fn update(State(e): S, Path(id): Path<String>, Json(v): Json<EngagementInput>) -> R {
    ok(e.save(Some(&id), v).await?)
}
async fn calculate(State(e): S, Path(id): Path<String>, Json(v): Json<RevisionInput>) -> R {
    ok(e.calculate(&id, v).await?)
}
async fn signoff(State(e): S, Path(id): Path<String>, Json(v): Json<SignoffInput>) -> R {
    ok(e.signoff(&id, v).await?)
}
async fn transition(State(e): S, Path(id): Path<String>, Json(v): Json<TransitionInput>) -> R {
    ok(e.transition(&id, v).await?)
}
async fn rollforward(State(e): S, Path(id): Path<String>, Json(v): Json<RollforwardInput>) -> R {
    ok(e.rollforward(&id, v).await?)
}
async fn preview(State(e): S, Path(id): Path<String>, Json(v): Json<PreviewInput>) -> R {
    ok(e.preview(&id, v).await?)
}
async fn commit(State(e): S, Path(id): Path<String>, Json(v): Json<CommitInput>) -> R {
    ok(e.commit(&id, v).await?)
}
#[derive(serde::Deserialize)]
struct AuditQuery {
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}
async fn events(State(e): S, Query(q): Query<AuditQuery>) -> R {
    let limit = q.limit.unwrap_or(200);
    if !(1..=200).contains(&limit) {
        return Err(bad("audit limit must be 1 to 200"));
    }
    ok(e.audit()
        .await?
        .into_iter()
        .skip(q.offset)
        .take(limit)
        .collect::<Vec<_>>())
}
async fn ledger_preview(State(e): S, Path(id): Path<String>, Json(v): Json<RevisionInput>) -> R {
    let r = e.get(&id).await?;
    check_revision(&r, v.expected_revision)?;
    editable(&r)?;
    let capture = bridges::ledger_document(&e.ledger, &r).await?;
    let accounts = serde_json::from_value(capture["accounts"].clone())
        .map_err(|_| conflict("Ledger capture could not be normalized"))?;
    ok(e.stage_accounts(
        &id,
        v.expected_revision,
        accounts,
        capture["source_hash"]
            .as_str()
            .ok_or_else(|| conflict("capture hash missing"))?
            .into(),
        capture["provenance"].clone(),
    )
    .await?)
}
async fn tax_preview(State(e): S, Path(id): Path<String>, Json(v): Json<TaxBridgeInput>) -> R {
    let r = e.get(&id).await?;
    check_revision(&r, v.expected_revision)?;
    if r.status != "locked" {
        return Err(conflict(
            "lock the reviewed engagement before preparing a Tax handoff",
        ));
    }
    ok(bridges::tax_preview(&e.tax, &r, &v.jurisdiction).await?)
}
async fn firm_proposal(State(e): S, Path(id): Path<String>, Json(v): Json<FirmBridgeInput>) -> R {
    let r = e.get(&id).await?;
    check_revision(&r, v.expected_revision)?;
    ok(bridges::firm_proposal(&e.firm, &r, v.project_id.as_deref()).await?)
}
async fn package(State(e): S, Path(id): Path<String>) -> EResult<Response> {
    let bytes = serde_json::to_vec_pretty(&e.package(&id).await?)
        .map_err(|_| bad("cannot encode export"))?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/json; charset=utf-8".to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"nxt-engagement-{id}.json\""),
            ),
        ],
        bytes,
    )
        .into_response())
}
async fn file_upload(State(e):S,Path((id,paper)):Path<(String,String)>,Json(input):Json<attachment_models::AttachmentUpload>)->R {
    ok(e.upload_attachment(&id,&paper,input).await?)
}
async fn lockdown(State(e): S, Path(id): Path<String>, Json(input): Json<LockdownInput>) -> R {
    ok(e.set_lockdown(&id, input).await?)
}
async fn statements(State(e): S, Path(id): Path<String>) -> R {
    ok(super::statements::generate(&e.get(&id).await?.document))
}
async fn gifi(State(e): S, Path(id): Path<String>) -> R { ok(e.gifi_lines(&id).await?) }
async fn file_tickmark(State(e): S, Path(id): Path<String>, Json(input): Json<FileTickmarkInput>) -> R {
    ok(e.add_file_tickmark(&id, input).await?)
}
async fn file_describe(State(e): S, Path((id, paper, version)): Path<(String, String, String)>) -> R {
    ok(e.describe_attachment(&id, &paper, &version).await?)
}
async fn file_download(State(e):S,Path((id,paper,version)):Path<(String,String,String)>)->EResult<Response> {
    let (metadata,bytes)=e.attachment(&id,&paper,&version).await?;
    let encoded=metadata.file_name.as_bytes().iter().map(|byte|{
        if byte.is_ascii_alphanumeric()||b"-_.~".contains(byte){(*byte as char).to_string()}else{format!("%{byte:02X}")}
    }).collect::<String>();
    Ok(([(header::CONTENT_TYPE,"application/octet-stream".to_string()),
        (header::CONTENT_DISPOSITION,format!("attachment; filename=\"nxt-{}.bin\"; filename*=UTF-8''{encoded}",metadata.id)),
        (header::CONTENT_SECURITY_POLICY,"sandbox; default-src 'none'; frame-ancestors 'none'".into()),
        (header::CONTENT_LENGTH,bytes.len().to_string())],bytes).into_response())
}
async fn file_package(State(e):S,Path(id):Path<String>)->EResult<Response> {
    let bytes=e.attachment_package(&id).await?;
    Ok(([(header::CONTENT_TYPE,"application/zip".to_string()),
        (header::CONTENT_DISPOSITION,format!("attachment; filename=\"nxt-engagement-{id}-files.zip\"")),
        (header::CONTENT_SECURITY_POLICY,"sandbox; default-src 'none'; frame-ancestors 'none'".into())],bytes).into_response())
}
fn esc(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn money(v: i64) -> String {
    let a = v.unsigned_abs();
    format!("{}{}.{:02}", if v < 0 { "−" } else { "" }, a / 100, a % 100)
}
fn money_table(value: &Value, title: &str) -> String {
    let mut html = format!(
        "<h2>{}</h2><table><tr><th>Control</th><th>CAD</th></tr>",
        esc(title)
    );
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            if !key.ends_with("_cents") {
                continue;
            }
            let display = value
                .as_i64()
                .map(money)
                .unwrap_or_else(|| "Unavailable".into());
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>",
                esc(&key.replace('_', " ")),
                display
            ));
        }
    }
    html.push_str("</table>");
    html
}
async fn print(State(e): S, Path(id): Path<String>) -> EResult<Response> {
    let r = e.get(&id).await?;
    let mut html = format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title><style>body{{font:14px system-ui;margin:36px;color:#18201e}}h1{{font-size:26px}}table{{border-collapse:collapse;width:100%}}th,td{{padding:8px;border-bottom:1px solid #ddd;text-align:left}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}.notice{{padding:16px;background:#fff5d8;border:1px solid #a78b38}}.hash{{font-size:11px;overflow-wrap:anywhere}}</style><h1>NXT Engagements · {}</h1><p>{} to {} · {} · Revision {}</p><p class=\"notice\">Internal working papers. No assurance report, licensed Caseware template or certified filing is produced. Operator labels are responsibility metadata, not verified identities.</p>",
        esc(&r.document.name),
        esc(&r.document.name),
        esc(&r.document.period_start),
        esc(&r.document.period_end),
        esc(&r.status),
        r.revision
    );
    if let Some(s) = &r.snapshot {
        html.push_str(&format!("<h2>Calculated working trial balance</h2><p>{}</p><table><tr><th>Code</th><th>Account</th><th>Adjusted debit</th><th>Adjusted credit</th></tr>",esc(s.result["scope"].as_str().unwrap_or("Bounded NXT workpapers"))));
        if let Some(rows) = s.result["accounts"].as_array() {
            for row in rows {
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(row["code"].as_str().unwrap_or("")),
                    esc(row["name"].as_str().unwrap_or("")),
                    money(row["adjusted_debit_cents"].as_i64().unwrap_or(0)),
                    money(row["adjusted_credit_cents"].as_i64().unwrap_or(0))
                ));
            }
        }
        html.push_str(&format!("</table><h2>Diagnostics</h2><pre>{}</pre><p class=\"hash\">Input SHA-256 {}<br>Output SHA-256 {}</p>",esc(&serde_json::to_string_pretty(&s.result["diagnostics"]).unwrap_or_default()),esc(&s.input_hash),esc(&s.output_hash)));
    } else {
        html.push_str(
            "<h2>Uncalculated draft</h2><p>No current verified working-paper result.</p>",
        );
    }
    if let Some(s) = &r.snapshot {
        html.push_str(&money_table(
            &s.result["totals"],
            "Trial balance control totals",
        ));
        html.push_str(&money_table(
            &s.result["financial_statements"]["current"],
            "Current financial summary",
        ));
        html.push_str(&money_table(
            &s.result["financial_statements"]["prior"],
            "Prior financial summary",
        ));
    }
    if !r.document.tickmark_catalogue.is_empty() {
        html.push_str("<h2>Current custom text tickmark catalogue</h2><p>Saved annotations keep their copied definitions when this catalogue changes. Text marks do not establish assurance or authenticated identity.</p><table><tr><th>ID</th><th>Code</th><th>Label</th><th>Description</th></tr>");
        for mark in &r.document.tickmark_catalogue {
            html.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",esc(&mark.id),esc(&mark.code),esc(&mark.label),esc(&mark.description)));
        }
        html.push_str("</table>");
    }
    for p in &r.document.workpapers {
        html.push_str(&format!(
            "<h2>{} · {}</h2><p>{}</p><pre>{}</pre>",
            esc(&p.reference),
            esc(&p.title),
            esc(&p.folder),
            esc(&p.body)
        ));
        if p.document_type == WorkpaperType::Placeholder {
            html.push_str("<p class=\"notice\">Placeholder · pending planning material, not evidence. No body evidence, annotations, file versions or sign-offs are permitted. Convert to manual before adding evidence.</p>");
        }
        if !p.annotations.is_empty() {
            html.push_str("<h3>Text-version tickmarks and notes</h3><p>Operator labels are unverified metadata. Account targets bind local account inputs and exact text, not group aggregates or certified balances. Check calculation diagnostics for stale text/account anchors. Custom meanings below are saved copies, independent of the current catalogue.</p><table><tr><th>Kind</th><th>Saved custom definition</th><th>Note</th><th>Operator label</th><th>Text SHA-256</th><th>Saved account target / context SHA-256</th></tr>");
            for annotation in &p.annotations {
                let saved=annotation.custom_mark.as_ref().map(|mark|format!("{} · {} · {} · {}",mark.id,mark.code,mark.label,mark.description)).unwrap_or_default();
                let account=annotation.account_anchor.as_ref().map(|anchor|format!("{} · {} · {}",anchor.engagement_id,anchor.account_id,anchor.context_sha256)).unwrap_or_default();
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(&annotation.kind),
                    esc(&saved),
                    esc(&annotation.note),
                    esc(&annotation.operator_label),
                    esc(&annotation.body_sha256),
                    esc(&account)
                ));
                if let Some(anchor)=&annotation.text_range_anchor {
                    let recorded=esc(&anchor.selected_text).replace("\r\n","&#13;\n").replace('\r',"&#13;<br>");
                    html.push_str(&format!("<tr><td colspan=\"6\"><strong>Recorded selected text — not reconstructed from the current body</strong><p>Owner {} · recorded UTF-8 bytes {}..{} · selection SHA-256 {}</p><pre><span>{}</span></pre></td></tr>",esc(&anchor.engagement_id),anchor.start_utf8_byte,anchor.end_utf8_byte,esc(&anchor.selection_sha256),recorded));
                }
            }
            html.push_str("</table>");
        }
    }
    html.push_str(&format!(
        "<h2>Notes</h2><pre>{}</pre><h2>Checkpoint metadata</h2><pre>{}</pre></html>",
        esc(&r.document.notes),
        esc(&serde_json::to_string_pretty(&r.signoffs).unwrap_or_default())
    ));
    if html.len() > 4 * 1024 * 1024 {
        return Err(conflict("print export exceeds 4 MiB"));
    }
    Ok((StatusCode::OK,[(header::CONTENT_TYPE,"text/html; charset=utf-8"),(header::CONTENT_SECURITY_POLICY,"default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'self'; base-uri 'none'")],html).into_response())
}
