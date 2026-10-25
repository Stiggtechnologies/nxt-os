use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewardKind {
    /// Fixed-amount coupon, e.g. $10 off.
    CouponFixed,
    /// Percentage coupon, e.g. 15% off.
    CouponPercent,
    /// Loyalty points credited to the recipient's ledger.
    Points,
    /// Store credit in the merchant's currency.
    Credit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reward {
    pub kind: RewardKind,
    pub amount: f64,
}

impl Reward {
    pub fn label(&self, currency: &str) -> String {
        match self.kind {
            RewardKind::CouponFixed | RewardKind::Credit => format!("{currency}{}", fmt_amount(self.amount)),
            RewardKind::CouponPercent => format!("{}%", fmt_amount(self.amount)),
            RewardKind::Points => format!("{} points", fmt_amount(self.amount)),
        }
    }
}

fn fmt_amount(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.2}") }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub brand_name: String,
    pub currency_symbol: String,
    /// Where `/r/{code}` redirects friends. `?ref={code}` is appended.
    pub landing_url: String,
    pub advocate_reward: Reward,
    pub friend_incentive: Reward,
    /// Orders below this amount don't earn the advocate a referral reward.
    pub min_order_amount: f64,
    /// Maximum referral rewards one advocate can earn (0 = unlimited).
    pub max_rewards_per_advocate: u32,
    pub loyalty_enabled: bool,
    pub points_per_currency_unit: f64,
    pub signup_points: i64,
    /// Points needed to redeem one currency unit of discount.
    pub points_per_redemption_unit: i64,
    pub webhook_url: String,
    pub webhook_secret: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            brand_name: "Acme Store".into(),
            currency_symbol: "$".into(),
            landing_url: "/welcome.html".into(),
            advocate_reward: Reward { kind: RewardKind::CouponFixed, amount: 10.0 },
            friend_incentive: Reward { kind: RewardKind::CouponFixed, amount: 10.0 },
            min_order_amount: 0.0,
            max_rewards_per_advocate: 0,
            loyalty_enabled: true,
            points_per_currency_unit: 1.0,
            signup_points: 50,
            points_per_redemption_unit: 100,
            webhook_url: String::new(),
            webhook_secret: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Advocate {
    pub email: String,
    pub name: String,
    pub code: String,
    pub created_at: String,
    pub clicks: u64,
    pub shares: u64,
    pub referrals: u64,
    pub rewards_earned: u32,
    pub referred_revenue: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Customer {
    pub email: String,
    pub orders: u64,
    pub total_spent: f64,
    pub points: i64,
    /// Referral code that brought this customer in, if any.
    pub referred_by: Option<String>,
    pub signed_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStatus {
    Approved,
    Rejected,
    /// Purchase without a referral; tracked for loyalty only.
    Organic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversion {
    pub id: String,
    pub order_id: String,
    pub email: String,
    pub amount: f64,
    pub referral_code: Option<String>,
    pub advocate_email: Option<String>,
    pub status: ConversionStatus,
    pub reason: Option<String>,
    pub points_awarded: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RewardRole {
    Advocate,
    Friend,
    Loyalty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardGrant {
    pub id: String,
    pub email: String,
    pub role: RewardRole,
    pub reward: Reward,
    /// Coupon code for coupon rewards; the merchant creates it in their
    /// store from the `reward.issued` webhook.
    pub coupon_code: Option<String>,
    pub referral_code: Option<String>,
    pub conversion_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub delta: i64,
    pub balance: i64,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub kind: String,
    pub referral_code: Option<String>,
    pub detail: Option<String>,
    pub created_at: String,
}
