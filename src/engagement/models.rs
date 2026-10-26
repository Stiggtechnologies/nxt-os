use serde::{Deserialize, Serialize};

fn cad() -> String {
    "CAD".into()
}
fn internal() -> String {
    "internal".into()
}
fn adjusting() -> String {
    "adjusting".into()
}
fn medium() -> String {
    "medium".into()
}
fn open() -> String {
    "open".into()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub debit_cents: i64,
    #[serde(default)]
    pub credit_cents: i64,
    #[serde(default)]
    pub prior_debit_cents: i64,
    #[serde(default)]
    pub prior_credit_cents: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub code: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub gifi_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustmentLine {
    pub account_id: String,
    #[serde(default)]
    pub debit_cents: i64,
    #[serde(default)]
    pub credit_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjustment {
    pub id: String,
    pub reference: String,
    pub date: String,
    #[serde(default = "adjusting")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub lines: Vec<AdjustmentLine>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workpaper {
    pub id: String,
    pub reference: String,
    pub title: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub account_ids: Vec<String>,
    #[serde(default)]
    pub adjustment_ids: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default = "medium")]
    pub priority: String,
    #[serde(default)]
    pub blocking: bool,
    #[serde(default = "open")]
    pub status: String,
    #[serde(default)]
    pub workpaper_id: Option<String>,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub carry_forward: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngagementInput {
    pub name: String,
    #[serde(default)]
    pub client_ref: String,
    #[serde(default = "internal")]
    pub kind: String,
    pub period_start: String,
    pub period_end: String,
    #[serde(default)]
    pub prior_period_start: Option<String>,
    #[serde(default)]
    pub prior_period_end: Option<String>,
    #[serde(default = "cad")]
    pub currency: String,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub adjustments: Vec<Adjustment>,
    #[serde(default)]
    pub workpapers: Vec<Workpaper>,
    #[serde(default)]
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}
