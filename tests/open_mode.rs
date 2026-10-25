use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use tower::ServiceExt;

/// Without ADMIN_API_KEY the workspace is open: admin APIs work with no key.
#[tokio::test]
async fn open_without_admin_key() {
    unsafe { std::env::remove_var("ADMIN_API_KEY") };
    let r = app_with(Store::memory);
    for uri in ["/api/os/status", "/api/ledger/accounts", "/api/admin/settings"] {
        let resp = r.clone().oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status().as_u16(), 200, "{uri}");
    }
    let resp = r.clone().oneshot(Request::builder().uri("/api/os/status").body(Body::empty()).unwrap()).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["auth_required"], false);
}
