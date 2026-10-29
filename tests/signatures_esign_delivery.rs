//! Separate binary: mutates process env, must not race the ceremony tests.
use nxt_os::signatures::esign;

#[test]
fn real_delivery_is_refused_until_a_provider_exists() {
    unsafe { std::env::remove_var("NXT_ESIGN_DELIVERY"); }
    assert_eq!(esign::delivery_mode().ok().unwrap(), "test");
    for v in ["email", "smtp", "live"] {
        unsafe { std::env::set_var("NXT_ESIGN_DELIVERY", v); }
        assert!(esign::delivery_mode().is_err(), "{v}");
    }
    unsafe { std::env::set_var("NXT_ESIGN_DELIVERY", "test"); }
    assert_eq!(esign::delivery_mode().ok().unwrap(), "test");
}
