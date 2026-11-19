//! Response compression changes transport bytes, never stored financial values.
use axum::{Router,body::Body,extract::State,http::{HeaderMap,StatusCode},response::Response,routing::post};
use flate2::{Compression,write::GzEncoder};
use nxt_os::store::Store;
use serde_json::json;
use std::{io::Write,sync::{Arc,atomic::{AtomicBool,AtomicU8,Ordering}}};
struct Mock{mode:AtomicU8,requested:AtomicBool,value:String}
async fn rpc(State(s):State<Arc<Mock>>,headers:HeaderMap)->Response{
 s.requested.store(headers.get("accept-encoding").is_some_and(|v|v.to_str().unwrap_or("").contains("gzip")),Ordering::SeqCst);
 let mode=s.mode.load(Ordering::SeqCst);
 let body=if mode==1{b"not a gzip response".to_vec()}else{
  let value=if mode==2{"x".repeat(24*1024*1024+1)}else{s.value.clone()};
  let raw=serde_json::to_vec(&json!({"result":value})).unwrap();
  let mut gzip=GzEncoder::new(Vec::new(),Compression::fast());gzip.write_all(&raw).unwrap();let compressed=gzip.finish().unwrap();
  assert!(compressed.len()<raw.len()/10,"fixture must exercise substantial transfer reduction");compressed
 };
 Response::builder().status(StatusCode::OK).header("content-type","application/json").header("content-encoding","gzip").header("content-length",body.len()).body(Body::from(body)).unwrap()
}
#[tokio::test]
async fn compressed_rpc_preserves_exact_values_and_enforces_decoded_size_and_integrity(){
 let expected="Original source: Montréal\r\nAmount 1,234.50; negative -0.01; blank: \n".repeat(5000);
 let state=Arc::new(Mock{mode:AtomicU8::new(0),requested:AtomicBool::new(false),value:expected.clone()});
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
 let app=Router::new().route("/rest/v1/rpc/nxt_store_command",post(rpc)).with_state(state.clone());
 let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});
 let store=Store::supabase(&format!("http://{address}"),"synthetic-server-secret","compression-test").unwrap();
 assert_eq!(store.get("source").await.unwrap().as_deref(),Some(expected.as_str()));assert!(state.requested.load(Ordering::SeqCst));
 state.mode.store(1,Ordering::SeqCst);assert!(store.get("source").await.unwrap_err().contains("could not be read"));
 state.mode.store(2,Ordering::SeqCst);assert!(store.get("source").await.unwrap_err().contains("exceeded its limit"));
 server.abort();
}
