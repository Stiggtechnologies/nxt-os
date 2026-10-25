use axum::{body::Body, http::Request};
use nxt_os::{app_with, store::Store};
use serde_json::{Value, json};
use tower::ServiceExt;

const KEY: &str = "test-key";

async fn call(router: &axum::Router, method: &str, uri: &str, body: Option<Value>, admin: bool) -> (u16, Value) {
    let mut req = Request::builder().method(method).uri(uri).header("host", "example.test");
    if admin {
        req = req.header("authorization", format!("Bearer {KEY}"));
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let loc = resp.headers().get("location").map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v = serde_json::from_slice(&bytes).unwrap_or(json!({ "location": loc }));
    (status, v)
}

fn router() -> axum::Router {
    unsafe { std::env::set_var("ADMIN_API_KEY", KEY) };
    app_with(Store::memory)
}

#[tokio::test]
async fn full_referral_and_loyalty_flow() {
    let r = router();

    // Advocate enrolls through the widget.
    let (s, adv) = call(&r, "POST", "/api/public/advocate", Some(json!({"email":"Ann.Lee@gmail.com","name":"Ann Lee"})), false).await;
    assert_eq!(s, 200);
    let code = adv["code"].as_str().unwrap().to_string();
    assert!(code.starts_with("ANNLEE"));
    assert_eq!(adv["share_url"], format!("https://example.test/r/{code}"));
    assert_eq!(adv["offer"]["headline"], "Give $10, Get $10");

    // Enrolling again returns the same code.
    let (_, again) = call(&r, "POST", "/api/public/advocate", Some(json!({"email":"ann.lee@gmail.com"})), false).await;
    assert_eq!(again["code"], code);

    // Friend clicks the link and is redirected to the landing page.
    let (s, redirect) = call(&r, "GET", &format!("/r/{code}"), None, false).await;
    assert_eq!(s, 302);
    assert_eq!(redirect["location"], format!("/welcome.html?ref={code}"));

    call(&r, "POST", "/api/public/share", Some(json!({"code":code,"channel":"email"})), false).await;

    // Friend claims the incentive, gets a coupon; claiming twice is idempotent.
    let (s, claim) = call(&r, "POST", "/api/public/claim", Some(json!({"code":code,"email":"bob@example.com"})), false).await;
    assert_eq!(s, 200);
    let coupon = claim["coupon_code"].as_str().unwrap().to_string();
    let (_, claim2) = call(&r, "POST", "/api/public/claim", Some(json!({"code":code,"email":"bob@example.com"})), false).await;
    assert_eq!(claim2["coupon_code"], coupon);

    // Advocate can't claim via own link (gmail dot/plus normalization).
    let (s, _) = call(&r, "POST", "/api/public/claim", Some(json!({"code":code,"email":"annlee+x@gmail.com"})), false).await;
    assert_eq!(s, 400);

    // Purchase API needs the admin key.
    let purchase = json!({"order_id":"1001","email":"bob@example.com","amount":80.0,"coupon_code":coupon});
    let (s, _) = call(&r, "POST", "/api/track/purchase", Some(purchase.clone()), false).await;
    assert_eq!(s, 401);

    // Friend's purchase is attributed through the coupon and approved.
    let (s, conv) = call(&r, "POST", "/api/track/purchase", Some(purchase.clone()), true).await;
    assert_eq!(s, 200);
    assert_eq!(conv["status"], "approved");
    assert_eq!(conv["referral_code"], code);
    assert_eq!(conv["points_awarded"], 80);

    // Duplicate order rejected.
    let (s, _) = call(&r, "POST", "/api/track/purchase", Some(purchase), true).await;
    assert_eq!(s, 409);

    // Repeat customer with the code doesn't earn the advocate another reward.
    let (_, conv2) = call(&r, "POST", "/api/track/purchase", Some(json!({"order_id":"1002","email":"bob@example.com","amount":20.0,"referral_code":code})), true).await;
    assert_eq!(conv2["status"], "rejected");
    assert_eq!(conv2["reason"], "not a new customer");

    // Self-referral rejected.
    let (_, conv3) = call(&r, "POST", "/api/track/purchase", Some(json!({"order_id":"1003","email":"a.n.n.lee@gmail.com","amount":50.0,"referral_code":code})), true).await;
    assert_eq!(conv3["reason"], "self-referral");

    // Rewards: friend coupon + advocate coupon.
    let (_, rewards) = call(&r, "GET", "/api/admin/rewards", None, true).await;
    let roles: Vec<&str> = rewards.as_array().unwrap().iter().map(|g| g["role"].as_str().unwrap()).collect();
    assert_eq!(roles, vec!["advocate", "friend"]);

    // Loyalty: signup bonus + order points, then redeem 100 points for $1.
    call(&r, "POST", "/api/track/signup", Some(json!({"email":"bob@example.com"})), true).await;
    let (_, c) = call(&r, "GET", "/api/admin/customers/bob@example.com", None, true).await;
    assert_eq!(c["customer"]["points"], 80 + 20 + 50);
    let (s, red) = call(&r, "POST", "/api/loyalty/redeem", Some(json!({"email":"bob@example.com","points":100})), true).await;
    assert_eq!(s, 200);
    assert_eq!(red["reward"]["amount"], 1.0);
    let (s, _) = call(&r, "POST", "/api/loyalty/redeem", Some(json!({"email":"bob@example.com","points":100})), true).await;
    assert_eq!(s, 409);

    // Analytics roll up.
    let (_, a) = call(&r, "GET", "/api/admin/analytics", None, true).await;
    assert_eq!(a["totals"]["clicks"], 1);
    assert_eq!(a["totals"]["referrals"], 1);
    assert_eq!(a["totals"]["rejected"], 2);
    assert_eq!(a["totals"]["referred_revenue"], 80.0);
    assert_eq!(a["share_channels"]["email"], 1);

    let (_, advs) = call(&r, "GET", "/api/admin/advocates", None, true).await;
    assert_eq!(advs[0]["referrals"], 1);
    assert_eq!(advs[0]["clicks"], 1);
}

#[tokio::test]
async fn settings_and_caps() {
    let r = router();
    let (_, mut s) = call(&r, "GET", "/api/admin/settings", None, true).await;
    s["advocate_reward"] = json!({"kind":"points","amount":500});
    s["friend_incentive"] = json!({"kind":"coupon_percent","amount":15});
    s["max_rewards_per_advocate"] = json!(1);
    s["min_order_amount"] = json!(25);
    let (st, _) = call(&r, "PUT", "/api/admin/settings", Some(s), true).await;
    assert_eq!(st, 200);
    let (_, offer) = call(&r, "GET", "/api/public/offer", None, false).await;
    assert_eq!(offer["headline"], "Give 15%, Get 500 points");

    let (_, adv) = call(&r, "POST", "/api/public/advocate", Some(json!({"email":"cy@example.com","name":"Cy"})), false).await;
    let code = adv["code"].as_str().unwrap();
    let (_, small) = call(&r, "POST", "/api/track/purchase", Some(json!({"order_id":"a","email":"d1@example.com","amount":10,"referral_code":code})), true).await;
    assert_eq!(small["status"], "rejected");
    let (_, ok1) = call(&r, "POST", "/api/track/purchase", Some(json!({"order_id":"b","email":"d2@example.com","amount":30,"referral_code":code})), true).await;
    assert_eq!(ok1["status"], "approved");
    let (_, capped) = call(&r, "POST", "/api/track/purchase", Some(json!({"order_id":"c","email":"d3@example.com","amount":30,"referral_code":code})), true).await;
    assert_eq!(capped["reason"], "advocate reward cap reached");

    // Points reward landed on the advocate's ledger.
    let (_, c) = call(&r, "GET", "/api/admin/customers/cy@example.com", None, true).await;
    assert_eq!(c["customer"]["points"], 500);
}
