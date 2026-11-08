use axum::{body::Body, http::Request};
use nxt_os::{app_with, source_import::source_paper_documents::PROFILE_SHA, store::Store};
pub use nxt_os::{source_import, store};
use serde_json::json;
use tower::ServiceExt;
#[path = "support/paper_source_replay.rs"]
mod replay;
#[tokio::test]
async fn paper_document_routes_require_admin_and_reject_unknown_input() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "paper-local-test");
    }
    let app = app_with(Store::memory);
    for (method, path) in [
        ("POST", "source-paper-documents"),
        ("GET", "source-paper-documents/summary"),
        (
            "GET",
            "clients/100/source-facts?kind=source-paper-documents&source_return_year=2005",
        ),
    ] {
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!(
                        "/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/{path}"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
    }
    let r=app.oneshot(Request::builder().method("POST").uri("/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/source-paper-documents").header("authorization","Bearer paper-local-test").header("content-type","application/json").body(Body::from(json!({"test_copy":true,"profile_sha256":PROFILE_SHA,"record":{}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(r.status(), 400);
}
