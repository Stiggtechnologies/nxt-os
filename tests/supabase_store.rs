//! Local HTTP acceptance tests for the server-side Supabase Store adapter.
//! The mock implements the documented RPC boundary, not Supabase/Postgres.
//! SQL atomicity/permissions require the migration's database-side tests too.
use axum::{Json, Router, body::Body, extract::State, http::{HeaderMap, Request, StatusCode}, response::{IntoResponse, Response}, routing::post};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use std::{collections::{BTreeSet, HashMap}, sync::{Arc, Mutex, atomic::{AtomicU8, Ordering}}};
use tower::ServiceExt;

const TEST_SECRET: &str = "synthetic-service-role-key-never-a-browser-key";
const MODERN_SECRET: &str = "sb_secret_synthetic_server_only";

#[derive(Clone)]
struct Observation {
    namespace: String,
    command: Vec<Value>,
    authenticated: bool,
    modern_key: bool,
}

#[derive(Default)]
struct Database {
    strings: HashMap<(String, String), String>,
    sets: HashMap<(String, String), BTreeSet<String>>,
    lists: HashMap<(String, String), Vec<String>>,
}

#[derive(Default)]
struct Mock {
    database: Mutex<Database>,
    observations: Mutex<Vec<Observation>>,
    response_mode: AtomicU8,
}

fn argument(command: &[Value], index: usize) -> String {
    command[index].as_str().expect("RPC text argument").to_string()
}

fn number(command: &[Value], index: usize) -> i64 {
    command[index].as_i64().or_else(|| command[index].as_str().and_then(|value| value.parse().ok())).expect("RPC integer argument")
}

async fn rpc(State(mock): State<Arc<Mock>>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let namespace = body["p_namespace"].as_str().unwrap_or("").to_string();
    let command = body["p_command"].as_array().cloned().unwrap_or_default();
    let key = headers.get("apikey").and_then(|header| header.to_str().ok());
    let authorization = headers.get("authorization").and_then(|header| header.to_str().ok());
    let modern_key = key == Some(MODERN_SECRET) && authorization.is_none();
    let authenticated = modern_key || (key == Some(TEST_SECRET) && authorization == Some(&format!("Bearer {TEST_SECRET}")));
    mock.observations.lock().unwrap().push(Observation { namespace: namespace.clone(), command: command.clone(), authenticated, modern_key });
    if !authenticated { return (StatusCode::UNAUTHORIZED, Json(json!({"message": "missing server-side service credentials"}))).into_response(); }
    match mock.response_mode.load(Ordering::SeqCst) {
        1 => return (StatusCode::FORBIDDEN, Json(json!({"message": format!("Upstream diagnostic includes {TEST_SECRET}")}))).into_response(),
        2 => return (StatusCode::OK, "not JSON").into_response(),
        3 => return (StatusCode::OK, Json(json!({"unexpected": true}))).into_response(),
        4 => return (StatusCode::OK, Json(json!({"result": {"wrong": "type"}}))).into_response(),
        5 => return (StatusCode::OK, Json(json!({"result": ["only-one-result"]}))).into_response(),
        6 => return (StatusCode::OK, Json(json!({"result": 2}))).into_response(),
        _ => {},
    }
    let Some(operation) = command.first().and_then(Value::as_str) else { return (StatusCode::BAD_REQUEST, Json(json!({"message": "missing command"}))).into_response(); };
    let mut database = mock.database.lock().unwrap();
    let key = || (namespace.clone(), argument(&command, 1));
    let result = match operation {
        "GET" => database.strings.get(&key()).cloned().map_or(Value::Null, Value::String),
        "SET" => {
            let record = key();
            if command.get(3).and_then(Value::as_str) == Some("NX") && database.strings.contains_key(&record) { Value::Null }
            else { database.strings.insert(record, argument(&command, 2)); json!("OK") }
        },
        "COMPARE_EXCHANGE" => {
            let record = key();
            let expected = command[2].as_str();
            if database.strings.get(&record).map(String::as_str) == expected {
                database.strings.insert(record, argument(&command, 3)); json!(1)
            } else { json!(0) }
        },
        "INCRBY" => {
            let record = key();
            let value = database.strings.get(&record).map_or(0, |value| value.parse::<i64>().unwrap()) + number(&command, 2);
            database.strings.insert(record, value.to_string()); json!(value)
        },
        "SADD" => json!(if database.sets.entry(key()).or_default().insert(argument(&command, 2)) {1} else {0}),
        "SMEMBERS" => json!(database.sets.get(&key()).cloned().unwrap_or_default().into_iter().collect::<Vec<_>>()),
        "LPUSH" => {
            let list = database.lists.entry(key()).or_default();
            list.insert(0, argument(&command, 2)); json!(list.len())
        },
        "LPUSH_CAPPED" => {
            let list = database.lists.entry(key()).or_default();
            list.insert(0, argument(&command, 2)); list.truncate(number(&command, 3).max(0) as usize); json!(list.len())
        },
        "LTRIM" => {
            let list = database.lists.entry(key()).or_default();
            let start = number(&command, 2).max(0) as usize;
            let end = number(&command, 3);
            let end = if end < 0 { list.len() } else {(end as usize + 1).min(list.len())};
            *list = if start >= end {Vec::new()} else {list[start..end].to_vec()}; json!("OK")
        },
        "LRANGE" => {
            let list = database.lists.get(&key()).cloned().unwrap_or_default();
            let start = number(&command, 2).max(0) as usize;
            let end = number(&command, 3);
            let end = if end < 0 {list.len()} else {(end as usize + 1).min(list.len())};
            if start >= end {json!([])} else {json!(&list[start..end])}
        },
        "MGET" => json!(command.iter().skip(1).map(|value| database.strings.get(&(namespace.clone(), value.as_str().unwrap().to_string())).cloned()).collect::<Vec<_>>()),
        _ => return (StatusCode::BAD_REQUEST, Json(json!({"message": "unexpected command"}))).into_response(),
    };
    (StatusCode::OK, Json(json!({"result": result}))).into_response()
}

async fn server() -> (String, Arc<Mock>, tokio::task::JoinHandle<()>) {
    let mock = Arc::new(Mock::default());
    let router = Router::new().route("/rest/v1/rpc/nxt_store_command", post(rpc)).with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
    (format!("http://{address}"), mock, handle)
}

async fn api(router: &Router, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
    let request = Request::builder().method(method).uri(uri).header("authorization", "Bearer k");
    let request = match body {
        Some(value) => request.header("content-type", "application/json").body(Body::from(value.to_string())),
        None => request.body(Body::empty()),
    }.unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn api_ok(router: &Router, method: &str, uri: &str, body: Option<Value>) -> Value {
    let (status, value) = api(router, method, uri, body).await;
    assert_eq!(status, 200, "{method} {uri}: {value}");
    value
}

const CONFIG_NAMES: &[&str] = &[
    "SUPABASE_URL", "SUPABASE_SECRET_KEY", "SUPABASE_SERVICE_ROLE_KEY", "NXT_STORE_NAMESPACE",
    "KV_REST_API_URL", "KV_REST_API_TOKEN", "UPSTASH_REDIS_REST_URL", "UPSTASH_REDIS_REST_TOKEN",
    "ADMIN_API_KEY",
];

fn isolate_environment() {
    // This test owns its integration-test process. Discard inherited provider
    // settings without reading credential values; the parent process is not
    // modified and no real provider is ever contacted.
    for name in CONFIG_NAMES { unsafe { std::env::remove_var(name) }; }
}

fn set(name: &str, value: &str) { unsafe { std::env::set_var(name, value) }; }
fn clear(name: &str) { unsafe { std::env::remove_var(name) }; }

fn store(url: &str, namespace: &str) -> Store {
    Store::supabase(url, TEST_SECRET, namespace).unwrap_or_else(|_| panic!("synthetic Supabase configuration must be valid"))
}

/// One test owns configuration variables in this integration-test executable;
/// all network traffic remains on its dynamically allocated loopback server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn supabase_rpc_preserves_store_and_firm_contracts_without_fallback() {
    isolate_environment();
    set("ADMIN_API_KEY", "k");
    let (url, mock, server) = server().await;
    let primary = store(&url, "group.production");
    let another_instance = store(&url, "group.production");
    let preview = store(&url, "group.preview");
    assert_eq!(primary.backend_name(), "supabase");

    // Shared durable records are visible to another adapter instance, but not
    // to an independent namespace using the same remote project.
    primary.set("record", "production value").await.unwrap();
    assert_eq!(another_instance.get("record").await.unwrap().as_deref(), Some("production value"));
    assert_eq!(preview.get("record").await.unwrap(), None);
    preview.set("record", "preview value").await.unwrap();
    assert_eq!(primary.get("record").await.unwrap().as_deref(), Some("production value"));
    assert_eq!(preview.get("record").await.unwrap().as_deref(), Some("preview value"));
    assert!(primary.set_nx("once", "first").await.unwrap());
    assert!(!another_instance.set_nx("once", "second").await.unwrap());
    assert_eq!(primary.get("once").await.unwrap().as_deref(), Some("first"));
    primary.set("empty", "").await.unwrap();
    assert!(!primary.compare_exchange("empty", None, "incorrect").await.unwrap());
    assert!(primary.compare_exchange("empty", Some(""), "present").await.unwrap());
    let (left, right) = tokio::join!(
        primary.compare_exchange("decision", None, "approved"),
        another_instance.compare_exchange("decision", None, "rejected")
    );
    assert_ne!(left.unwrap(), right.unwrap(), "the service, not a local GET/SET sequence, decides the CAS winner");
    assert!(preview.compare_exchange("decision", None, "independent").await.unwrap());

    assert_eq!(primary.incr_by("count", 3).await.unwrap(), 3);
    assert_eq!(another_instance.incr_by("count", -1).await.unwrap(), 2);
    primary.sadd("members", "second").await.unwrap();
    primary.sadd("members", "first").await.unwrap();
    primary.sadd("members", "first").await.unwrap();
    let members: BTreeSet<_> = primary.smembers("members").await.unwrap().into_iter().collect();
    assert_eq!(members, ["first".to_string(), "second".to_string()].into_iter().collect());
    for value in ["oldest", "middle", "newest"] { primary.lpush_capped("events", value, 2).await.unwrap(); }
    assert_eq!(another_instance.lrange("events", 10).await.unwrap(), vec!["newest", "middle"]);
    assert_eq!(primary.mget(&["record".into(), "missing".into(), "empty".into()]).await.unwrap(), vec![Some("production value".into()), None, Some("present".into())]);

    // Modern server secret keys use apikey, while legacy service-role keys
    // retain their bearer header. Neither goes into the RPC JSON command.
    let modern = Store::supabase(&url, MODERN_SECRET, "group.modern").unwrap();
    modern.set("record", "modern value").await.unwrap();
    let observations = mock.observations.lock().unwrap().clone();
    assert!(observations.iter().all(|request| request.authenticated));
    assert!(observations.iter().any(|request| request.modern_key && request.namespace == "group.modern"));
    assert!(observations.iter().any(|request| request.command.first().and_then(Value::as_str) == Some("COMPARE_EXCHANGE") && request.command.get(2) == Some(&Value::Null)));
    assert!(observations.iter().all(|request| !serde_json::to_string(&request.command).unwrap().contains(TEST_SECRET)));

    // Actual Firm API mutations use the server's CAS operation and persist
    // across app reconstruction. Approval is the execution boundary.
    let router = app_with(|| store(&url, "group.firm"));
    let accounts = api_ok(&router, "GET", "/api/ledger/accounts", None).await;
    assert!(accounts.as_array().unwrap().iter().any(|account| account["id"] == "acc_1000"));
    api_ok(&router, "POST", "/api/ledger/entries", Some(json!({
        "date": "2026-10-24", "memo": "Synthetic Supabase ledger check",
        "lines": [{"account_id": "acc_1000", "debit": 42}, {"account_id": "acc_3000", "credit": 42}]
    }))).await;
    let trial_balance = api_ok(&router, "GET", "/api/ledger/reports/trial-balance?to=2026-10-31", None).await;
    assert_eq!(trial_balance["balanced"], true);
    assert_eq!(trial_balance["total_debit"], 42);
    assert_eq!(trial_balance["total_credit"], 42);
    let project = api_ok(&router, "POST", "/api/firm/projects", Some(json!({"name": "Synthetic Supabase work", "tasks": []}))).await;
    let proposal = api_ok(&router, "POST", "/api/firm/proposals", Some(json!({
        "name": "Review Supabase-backed task", "idempotency_key": "supabase-event-1",
        "action": {"kind": "create_task", "title": "Only after review", "project_id": project["id"]}
    }))).await;
    assert_eq!(proposal["status"], "proposed");
    let persisted_router = app_with(|| store(&url, "group.firm"));
    assert_eq!(api_ok(&persisted_router, "GET", "/api/ledger/reports/trial-balance?to=2026-10-31", None).await, trial_balance);
    let projects = api_ok(&persisted_router, "GET", "/api/firm/projects", None).await;
    assert_eq!(projects[0]["tasks"], json!([]));
    let path = format!("/api/firm/approvals/{}/decide", proposal["approval_id"].as_str().unwrap());
    let decision = api_ok(&persisted_router, "POST", &path, Some(json!({"decision": "approve", "note": "Synthetic review"}))).await;
    assert_eq!(decision["status"], "executed");
    api_ok(&router, "POST", &path, Some(json!({"decision": "approve"}))).await;
    let projects = api_ok(&router, "GET", "/api/firm/projects", None).await;
    assert_eq!(projects[0]["tasks"].as_array().unwrap().len(), 1);
    let isolated = app_with(|| store(&url, "group.other-firm"));
    assert_eq!(api_ok(&isolated, "GET", "/api/firm/projects", None).await, json!([]));
    let observations = mock.observations.lock().unwrap().clone();
    let writes: Vec<_> = observations.iter().filter(|request| request.namespace == "group.firm" && request.command.get(1).and_then(Value::as_str).is_some_and(|key| key.contains("firm:state"))).collect();
    assert!(writes.iter().any(|request| request.command[0] == "COMPARE_EXCHANGE"));
    assert!(!writes.iter().any(|request| request.command[0] == "SET"), "firm approval state must not use a blind remote overwrite");

    // A denied/malformed service must fail the operation, never quietly turn
    // into process memory or treat a malformed successful response as data.
    for mode in [1, 2, 3, 4] {
        mock.response_mode.store(mode, Ordering::SeqCst);
        let error = primary.get("record").await.expect_err("invalid upstream response must fail closed");
        assert!(!error.contains(TEST_SECRET));
        assert!(!error.contains(&url));
        assert!(!error.contains("Upstream diagnostic"));
    }
    mock.response_mode.store(5, Ordering::SeqCst);
    assert!(primary.mget(&["record".into(), "missing".into()]).await.is_err(), "MGET must not silently lose positional results");
    mock.response_mode.store(6, Ordering::SeqCst);
    assert!(primary.compare_exchange("record", Some("production value"), "bad-result").await.is_err(), "CAS must accept only the documented 0/1 response");
    mock.response_mode.store(1, Ordering::SeqCst);
    assert!(primary.set("denied", "must not be saved").await.is_err());
    assert!(primary.compare_exchange("denied", None, "must not be saved").await.is_err());
    let (status, value) = api(&router, "GET", "/api/firm/summary", None).await;
    assert!(status >= 500, "storage outage must be visible to the API");
    assert!(!value.to_string().contains(TEST_SECRET));
    mock.response_mode.store(0, Ordering::SeqCst);
    assert_eq!(primary.get("denied").await.unwrap(), None);
    assert_eq!(primary.get("record").await.unwrap().as_deref(), Some("production value"));

    // Invalid configuration cannot silently select Redis or memory. The
    // synthetic Redis configuration is present to catch accidental fallback.
    set("KV_REST_API_URL", &format!("{url}/not-the-supabase-rpc"));
    set("KV_REST_API_TOKEN", "synthetic-redis-token");
    set("NXT_STORE_NAMESPACE", "group.config");
    let count = mock.observations.lock().unwrap().len();
    let namespace_only = Store::from_env();
    assert_eq!(namespace_only.backend_name(), "unavailable", "a namespace-only Supabase configuration must not select Redis or memory");
    assert!(namespace_only.get("record").await.is_err());
    assert_eq!(mock.observations.lock().unwrap().len(), count);
    set("SUPABASE_URL", &url);
    let unavailable = Store::from_env();
    assert_eq!(unavailable.backend_name(), "unavailable");
    assert!(unavailable.get("record").await.is_err());
    assert_eq!(mock.observations.lock().unwrap().len(), count);
    set("SUPABASE_SECRET_KEY", TEST_SECRET);
    clear("SUPABASE_URL");
    assert_eq!(Store::from_env().backend_name(), "unavailable");
    set("SUPABASE_URL", &url);
    set("SUPABASE_SECRET_KEY", "");
    assert_eq!(Store::from_env().backend_name(), "unavailable");
    set("SUPABASE_SECRET_KEY", TEST_SECRET);
    clear("NXT_STORE_NAMESPACE");
    assert_eq!(Store::from_env().backend_name(), "unavailable");
    set("NXT_STORE_NAMESPACE", "invalid/company");
    assert_eq!(Store::from_env().backend_name(), "unavailable");
    set("NXT_STORE_NAMESPACE", "group.config");
    set("SUPABASE_URL", "https://user:synthetic-password@project.invalid/?token=synthetic-query-secret");
    let invalid = Store::from_env();
    assert_eq!(invalid.backend_name(), "unavailable");
    let error = invalid.get("record").await.unwrap_err();
    for sensitive in [TEST_SECRET, "synthetic-password", "synthetic-query-secret"] { assert!(!error.contains(sensitive)); }
    let invalid_router = app_with(Store::from_env);
    assert!(api(&invalid_router, "GET", "/api/firm/summary", None).await.0 >= 500);
    assert!(Store::supabase(&url, "sb_publishable_synthetic", "group.config").is_err());
    assert!(Store::supabase(&url, "line\nbreak", "group.config").is_err());
    assert!(Store::supabase(&url, TEST_SECRET, "").is_err());
    assert!(Store::supabase(&url, TEST_SECRET, &"n".repeat(65)).is_err());

    // Verify both supported environment key forms actually select Supabase.
    set("SUPABASE_URL", &url);
    clear("SUPABASE_SECRET_KEY");
    set("SUPABASE_SERVICE_ROLE_KEY", TEST_SECRET);
    let legacy = Store::from_env();
    assert_eq!(legacy.backend_name(), "supabase");
    legacy.set("legacy", "configured").await.unwrap();
    set("SUPABASE_SECRET_KEY", MODERN_SECRET);
    let preferred = Store::from_env();
    assert_eq!(preferred.backend_name(), "supabase");
    preferred.set("preferred", "configured").await.unwrap();
    assert!(mock.observations.lock().unwrap().last().unwrap().modern_key);
    server.abort();
}
