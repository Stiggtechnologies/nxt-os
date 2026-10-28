use super::{*,models::*};
use axum::{Router,Json,extract::{State,Path,Query,DefaultBodyLimit},routing::{get,post},middleware,http::{header,HeaderValue},response::Response};
use serde::Deserialize;
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {#[serde(default)]offset:usize,#[serde(default="limit")]limit:usize,#[serde(default="budget")]max_bytes:usize,#[serde(default,rename="path")]transport_path:Option<String>}
fn limit()->usize{100}fn budget()->usize{786432}
pub fn router(s:Arc<ContactLists>)->Router {
    Router::new()
        .route("/api/company-books/{company}/source-contact-lists",get(index).post(publish))
        .route("/api/company-books/{company}/source-contact-lists/parts",post(stage))
        .route("/api/company-books/{company}/source-contact-lists/captures/{capture}",get(detail))
        .route("/api/company-books/{company}/source-contact-lists/{kind}/{capture}/records",get(rows))
        .route("/api/company-books/{company}/source-contact-lists/{kind}/{capture}/other-rows",get(other))
        .layer(DefaultBodyLimit::max(600*1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r:Response|async move{r.headers_mut().insert(header::CACHE_CONTROL,HeaderValue::from_static("private, no-store"));r.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS,HeaderValue::from_static("nosniff"));r}))
        .with_state(s)
}
async fn index(State(s):State<Arc<ContactLists>>,Path(c):Path<String>)->Result<Json<Value>>{Ok(Json(s.list(&c).await?))}
async fn stage(State(s):State<Arc<ContactLists>>,Path(c):Path<String>,Json(p):Json<Part>)->Result<Json<Value>>{Ok(Json(s.stage(&c,p).await?))}
async fn publish(State(s):State<Arc<ContactLists>>,Path(c):Path<String>,Json(p):Json<Publish>)->Result<Json<Value>>{Ok(Json(s.publish(&c,p).await?))}
async fn detail(State(s):State<Arc<ContactLists>>,Path((c,h)):Path<(String,String)>)->Result<Json<Value>>{Ok(Json(s.detail(&c,&h).await?))}
fn kind(s:&str)->Result<Kind>{match s{"customer"=>Ok(Kind::Customer),"supplier"=>Ok(Kind::Supplier),_=>Err(bad("Unsupported source contact kind"))}}
fn transport_path(p:&Page,c:&str,k:&str,h:&str,tail:&str)->Result<()> {
    if p.transport_path.as_deref().is_some_and(|s|s.trim_start_matches('/').strip_prefix("api/").unwrap_or(s.trim_start_matches('/'))!=format!("company-books/{c}/source-contact-lists/{k}/{h}/{tail}")){return Err(bad("Source contact transport path mismatch"))}Ok(())
}
async fn rows(State(s):State<Arc<ContactLists>>,Path((c,k,h)):Path<(String,String,String)>,Query(p):Query<Page>)->Result<Json<Value>>{transport_path(&p,&c,&k,&h,"records")?;Ok(Json(s.rows(&c,kind(&k)?,&h,p.offset,p.limit,p.max_bytes,false).await?))}
async fn other(State(s):State<Arc<ContactLists>>,Path((c,k,h)):Path<(String,String,String)>,Query(p):Query<Page>)->Result<Json<Value>>{transport_path(&p,&c,&k,&h,"other-rows")?;Ok(Json(s.rows(&c,kind(&k)?,&h,p.offset,p.limit,p.max_bytes,true).await?))}
