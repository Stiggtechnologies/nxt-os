//! Ask NXT end to end on the provider chain, against local mock providers (nothing leaves the box):
//! OpenAI fails with a 503, DeepSeek on OpenRouter answers with a gods_eye tool call, then text.
//! No Anthropic key is set. One test in its own binary because it sets process env.
use axum::{Json, Router, body::Body, http::{Request, StatusCode}, routing::post};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ask_nxt_runs_on_openai_then_deepseek_then_nvidia_without_anthropic() {
    let seen: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(vec![]));
    let s1 = seen.clone();
    let openai = Router::new().route("/v1/chat/completions", post(move |Json(b): Json<Value>| {
        let s = s1.clone();
        async move { s.lock().unwrap().push(json!({"host": "openai", "body": b})); (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": {"message": "overloaded"}}))) }
    }));
    let s2 = seen.clone();
    let router = Router::new().route("/api/v1/chat/completions", post(move |Json(b): Json<Value>| {
        let s = s2.clone();
        async move {
            let first = !b["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool");
            s.lock().unwrap().push(json!({"host": "openrouter", "body": b.clone()}));
            if b["model"] != "deepseek/deepseek-v4.1-flash-1017" {
                return (StatusCode::BAD_REQUEST, Json(json!({"error": {"message": "unexpected model"}})));
            }
            let reply = if first {
                json!({"model": b["model"], "choices": [{"finish_reason": "tool_calls", "message": {"content": null,
                    "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "gods_eye", "arguments": "{\"as_of\":\"2026-11-18\"}"}}]}}]})
            } else {
                json!({"model": b["model"], "choices": [{"finish_reason": "stop", "message": {"content": "God's Eye shows no findings."}}]})
            };
            (StatusCode::OK, Json(reply))
        }
    }));
    let a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (pa, pb) = (a.local_addr().unwrap().port(), b.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(a, openai).await.unwrap() });
    tokio::spawn(async move { axum::serve(b, router).await.unwrap() });
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::set_var("OPENAI_API_KEY", "test-openai");
        std::env::set_var("OPENAI_BASE_URL", format!("http://127.0.0.1:{pa}"));
        std::env::set_var("OPENROUTER_API_KEY", "test-openrouter");
        std::env::set_var("OPENROUTER_BASE_URL", format!("http://127.0.0.1:{pb}/api/v1"));
    }
    let shared = Store::memory();
    let app = app_with(move || shared.clone());
    let call = |m: &str, uri: &str, body: Option<Value>| {
        let req = Request::builder().method(m).uri(uri).header("authorization", "Bearer k").header("content-type", "application/json")
            .body(body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap();
        let app = app.clone();
        async move {
            let resp = app.oneshot(req).await.unwrap();
            let status = resp.status().as_u16();
            (status, serde_json::from_slice::<Value>(&axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap_or(Value::Null))
        }
    };
    let (status, st) = call("GET", "/api/os/status", None).await;
    assert_eq!(status, 200);
    assert_eq!(st["ai_enabled"], true);
    assert_eq!(st["model"], "gpt-4o-mini");

    let (status, v) = call("POST", "/api/os/ask", Some(json!({"messages": [{"role": "user", "content": "What does God's Eye see?"}]}))).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["reply"], "God's Eye shows no findings.");
    assert_eq!(v["tools_used"], json!(["gods_eye"]));
    let log = seen.lock().unwrap().clone();
    let hosts: Vec<&str> = log.iter().map(|e| e["host"].as_str().unwrap()).collect();
    assert_eq!(hosts, ["openai", "openrouter", "openai", "openrouter"], "OpenAI is tried first on every turn, then DeepSeek");
    assert_eq!(log[0]["body"]["model"], "gpt-4o-mini");
    let routed = &log[1]["body"];
    assert_eq!(routed["provider"]["data_collection"], "deny");
    assert!(routed.get("max_completion_tokens").is_none() && routed["max_tokens"].is_number());
    assert!(routed["tools"].as_array().unwrap().iter().any(|t| t["function"]["name"] == "gods_eye"));
    let tool_msg = log[3]["body"]["messages"].as_array().unwrap().iter().find(|m| m["role"] == "tool").unwrap().clone();
    assert!(tool_msg["content"].as_str().unwrap().contains("read_only"), "the God's Eye result went back to the model");
}
