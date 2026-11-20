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
