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
    // The API router resolves company aliases before routing (its own fallback), so nothing may be routed
    // beside it: one fallback sends /api to the API untouched and everything else to the static files.
    let api = app_with(Store::from_env);
    let router = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let api = api.clone();
        async move {
            use axum::response::IntoResponse;
            use tower::ServiceExt;
            let path = req.uri().path().to_string();
            if path == "/api" || path.starts_with("/api/") {
                return api.oneshot(req).await.into_response();
            }
            let file = path.trim_start_matches('/').to_string();
            static_file(Some(Path(file))).await.into_response()
        }
    });
    let addr = std::env::var("PORT").map(|p| format!("127.0.0.1:{p}")).unwrap_or("127.0.0.1:3000".into());
    println!("NXT OS dev server on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(&addr).await.unwrap(), router).await.unwrap();
}
