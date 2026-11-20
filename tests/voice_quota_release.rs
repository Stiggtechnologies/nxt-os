//! SyncAI `sync-realtime-session` `releaseQuota`, transferred into Nixon voice: a daily
//! Realtime slot or TTS character reservation that the provider never used is handed back.
//! The provider URL is an unreachable local port and the key is a dummy, so nothing leaves
//! the machine and no real key is read.
use nxt_os::ledger::engine::{today, Ledger};
use nxt_os::nixon_finance::speech;
use nxt_os::store::Store;
use serde_json::json;

const NOWHERE: &str = "http://127.0.0.1:9/unreachable";

fn offer() -> speech::RealtimeOffer {
    serde_json::from_value(json!({"sdp": "v=0", "context": {"route": "/nixon/gods-eye"}})).unwrap()
}

#[tokio::test]
async fn an_unreachable_realtime_provider_does_not_burn_the_daily_session() {
    let ledger = Ledger::new(Store::memory());
    let key = speech::realtime_quota_key(&today());
    for _ in 0..3 {
        let err = speech::negotiate_with(&ledger, offer(), Some("dummy".into()), NOWHERE).await.unwrap_err();
        assert_eq!((err.status, err.message.as_str()), (502, "voice_provider_unavailable"));
    }
    assert_eq!(ledger.store.incr_by(&key, 0).await.unwrap(), 0, "failed negotiations are released");
}

#[tokio::test]
async fn the_daily_realtime_cap_still_refuses_and_holds_at_the_cap() {
    let ledger = Ledger::new(Store::memory());
    let key = speech::realtime_quota_key(&today());
    ledger.store.incr_by(&key, speech::REALTIME_DAILY_SESSIONS).await.unwrap();
    let err = speech::negotiate_with(&ledger, offer(), Some("dummy".into()), NOWHERE).await.unwrap_err();
    assert_eq!((err.status, err.message.as_str()), (429, "realtime_quota"));
    assert_eq!(ledger.store.incr_by(&key, 0).await.unwrap(), speech::REALTIME_DAILY_SESSIONS);
}

#[tokio::test]
async fn no_key_reserves_nothing() {
    let ledger = Ledger::new(Store::memory());
    let err = speech::negotiate_with(&ledger, offer(), None, NOWHERE).await.unwrap_err();
    assert_eq!((err.status, err.message.as_str()), (503, "realtime_unconfigured"));
    assert_eq!(ledger.store.incr_by(&speech::realtime_quota_key(&today()), 0).await.unwrap(), 0);
}

#[tokio::test]
async fn an_unreachable_speech_provider_hands_the_characters_back() {
    let ledger = Ledger::new(Store::memory());
    let key = speech::tts_quota_key(&today());
    let req = speech::SpeakRequest { text: "Two findings need a person to look at them.".into() };
    let err = speech::synthesize_with(&ledger, req, Some("dummy".into()), NOWHERE).await.unwrap_err();
    assert_eq!((err.status, err.message.as_str()), (502, "voice_provider_unavailable"));
    assert_eq!(ledger.store.incr_by(&key, 0).await.unwrap(), 0);
}

/// A one-shot local HTTP server answering every request with `reply`.
async fn local(reply: &'static str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = [0u8; 65536];
            let _ = socket.read(&mut buf).await;
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    format!("http://{addr}/realtime")
}

#[tokio::test]
async fn a_provider_error_status_is_refunded_once() {
    let ledger = Ledger::new(Store::memory());
    let url = local("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    let err = speech::negotiate_with(&ledger, offer(), Some("dummy".into()), &url).await.unwrap_err();
    assert_eq!(err.status, 502);
    assert_eq!(ledger.store.incr_by(&speech::realtime_quota_key(&today()), 0).await.unwrap(), 0);
}

#[tokio::test]
async fn a_2xx_the_provider_may_have_opened_is_not_refunded() {
    // Abuse guard: a success status may already be a live upstream session, so it stays spent
    // even when its body is unusable. Refunds cannot be farmed into extra sessions.
    let ledger = Ledger::new(Store::memory());
    let url = local("HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
    let err = speech::negotiate_with(&ledger, offer(), Some("dummy".into()), &url).await.unwrap_err();
    assert_eq!(err.status, 502);
    assert_eq!(ledger.store.incr_by(&speech::realtime_quota_key(&today()), 0).await.unwrap(), 1);
}
