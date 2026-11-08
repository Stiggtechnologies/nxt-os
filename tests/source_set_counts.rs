use axum::{Json,Router,extract::State,http::{HeaderMap,StatusCode},response::{IntoResponse,Response},routing::post};
use nxt_os::store::Store;
use serde_json::{Value,json};
use std::sync::{Arc,Mutex,atomic::{AtomicU8,Ordering}};
const SECRET:&str="synthetic-counter-server-secret";
const MODERN:&str="sb_secret_synthetic_counter_only";
#[derive(Default)]struct Mock{calls:Mutex<Vec<Value>>,mode:AtomicU8}
async fn rpc(State(s):State<Arc<Mock>>,h:HeaderMap,Json(v):Json<Value>)->Response{
    let key=h.get("apikey").and_then(|h|h.to_str().ok());let bearer=h.get("authorization").and_then(|h|h.to_str().ok());
    assert!((key==Some(SECRET)&&bearer==Some("Bearer synthetic-counter-server-secret"))||(key==Some(MODERN)&&bearer.is_none()));
    assert_eq!(v["p_keys"],json!(["nxt:dtmaxtc:counter-test:family_files","nxt:dtmaxtc:counter-test:afr_contents","nxt:dtmaxtc:counter-test:family_files"]));
    s.calls.lock().unwrap().push(v.clone());
    let result=match s.mode.load(Ordering::SeqCst){
        1=>json!([2,0]),2=>json!([2,-1,2]),3=>json!([2,0.5,2]),4=>json!([2,"0",2]),5=>json!([2,null,2]),6=>json!([2,u64::MAX,2]),
        7=>return (StatusCode::FORBIDDEN,Json(json!({"upstream_message":SECRET}))).into_response(),
        8=>return Json(json!({"unrelated":true})).into_response(),
        _=>if v["p_namespace"]=="counter-isolated"{json!([0,0,0])}else{json!([2,0,2])}
    };Json(json!({"result":result})).into_response()
}
fn keys()->Vec<String>{["family_files","afr_contents","family_files"].map(|k|format!("dtmaxtc:counter-test:{k}")).to_vec()}
async fn server()->(String,Arc<Mock>,tokio::task::JoinHandle<()>){
    let state=Arc::new(Mock::default());let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}",listener.local_addr().unwrap());let router=Router::new().route("/rest/v1/rpc/nxt_source_set_counts",post(rpc)).with_state(state.clone());let task=tokio::spawn(async move{axum::serve(listener,router).await.unwrap()});(url,state,task)
}
#[tokio::test]
async fn source_counts_preserve_missing_repeated_keys_and_namespace_without_member_payloads(){
    let(url,state,task)=server().await;
    for secret in[SECRET,MODERN]{let s=Store::supabase(&url,secret,"counter-main").unwrap();assert_eq!(s.source_set_counts(&keys()).await.unwrap(),vec![2,0,2]);}
    let other=Store::supabase(&url,SECRET,"counter-isolated").unwrap();assert_eq!(other.source_set_counts(&keys()).await.unwrap(),vec![0,0,0]);assert_eq!(state.calls.lock().unwrap().len(),3);task.abort();
}
#[tokio::test]
async fn source_counts_reject_invalid_requests_and_malformed_upstream_without_secret_echo(){
    let(url,state,task)=server().await;let s=Store::supabase(&url,SECRET,"counter-main").unwrap();assert!(s.source_set_counts(&[]).await.unwrap().is_empty());
    for keys in[vec!["company:ledger".into()],vec!["dtmaxtc:counter-test:return_pdf".into()],vec!["dtmaxtc: bad:family_files".into()],vec!["dtmaxtc:counter-test:family_files".into();17]]{assert!(s.source_set_counts(&keys).await.is_err());}
    assert!(state.calls.lock().unwrap().is_empty());
    for mode in 1..=8{state.mode.store(mode,Ordering::SeqCst);let e=s.source_set_counts(&keys()).await.unwrap_err();assert!(!e.contains(SECRET));}
    task.abort();
}
#[tokio::test]
async fn source_counts_memory_matches_members_and_never_changes_values_or_indexes(){
    let s=Store::memory();let k=keys();s.sadd(&k[0],"synthetic-a").await.unwrap();s.sadd(&k[0],"synthetic-b").await.unwrap();let before=s.smembers(&k[0]).await.unwrap();assert_eq!(s.source_set_counts(&k).await.unwrap(),vec![2,0,2]);assert_eq!(s.smembers(&k[0]).await.unwrap(),before);s.set(&k[1],"synthetic-string").await.unwrap();assert!(s.source_set_counts(&k).await.is_err());assert_eq!(s.get(&k[1]).await.unwrap().as_deref(),Some("synthetic-string"));
}
