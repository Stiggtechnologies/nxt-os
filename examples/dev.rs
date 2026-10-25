//! Local dev server: `ADMIN_API_KEY=dev cargo run --example dev`, then open
//! http://localhost:3000. Serves the API plus the static files in public/.
use axum::{extract::Path, http::header, response::IntoResponse};
use nxt_os::{app_with, store::Store};

async fn static_file(path: Option<Path<String>>) -> impl IntoResponse {
    let mut name = path.map(|p| p.0).unwrap_or_default();
    if name.is_empty() || name.ends_with('/') {
        name.push_str("index.html");
    }
    if name.contains("..") {
        return (axum::http::StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], Vec::new());
    }
    let ct = match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };
    match std::fs::read(format!("public/{name}")) {
        Ok(bytes) => (axum::http::StatusCode::OK, [(header::CONTENT_TYPE, ct)], bytes),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "text/plain")], Vec::new()),
    }
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let router = app_with(Store::from_env)
        .route("/", axum::routing::get(static_file))
        .route("/{*file}", axum::routing::get(static_file));
    let addr = std::env::var("PORT").map(|p| format!("0.0.0.0:{p}")).unwrap_or("0.0.0.0:3000".into());
    println!("NXT OS dev server on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(&addr).await.unwrap(), router).await.unwrap();
}
