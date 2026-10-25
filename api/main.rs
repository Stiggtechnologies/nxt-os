use nxt_os::{app_with, store::Store};
use tower::ServiceBuilder;
use vercel_runtime::{Error, axum::VercelLayer, run};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let router = app_with(Store::from_env);
    let service = ServiceBuilder::new().layer(VercelLayer::new()).service(router);
    run(service).await
}
