//! Guided T1 interview / spouse proposals / one-page T2 intake. Synthetic data only.
//! Nothing here can file: proposals never write to a return and the filing route still rejects.
use chrono::NaiveDate;
use nxt_os::{store::Store, tax::{models::ReturnInput, spouse_optimize::{self, Side}, t2_intake::{self, IntakeAnswers}, Tax}};
use serde_json::{Value, json};

fn t1(name: &str, inputs: Value) -> ReturnInput {
    serde_json::from_value(json!({"program":"T1","tax_year":2025,"jurisdiction":"AB","name":name,"inputs":inputs})).unwrap()
}
fn answers(v: Value) -> IntakeAnswers { serde_json::from_value(v).unwrap() }
fn day(s: &str) -> NaiveDate { NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() }

#[tokio::test]
async fn spouse_optimizer_proposes_only_and_audits_acceptance() {
    let tax = Tax::new(Store::memory());
    let base = json!({"scope_confirmed":true,"full_year_resident":true});
    let mut a_in = base.clone(); a_in["pension_income_cents"] = json!(6_000_000);
    let mut b_in = base.clone(); b_in["employment_income_cents"] = json!(1_000_000);
    let a = tax.save(None, t1("Example Person 001", a_in.clone())).await.ok().unwrap();
    let b = tax.save(None, t1("Example Person 002", b_in.clone())).await.ok().unwrap();
    let fam = tax.link_family("Example family", &a.id, &b.id).await.ok().unwrap();
    let fid = fam["id"].as_str().unwrap().to_string();
    let out = tax.optimize_family(&fid).await.ok().unwrap();
    assert_eq!(out["applied"], false); assert_eq!(out["write_back"], false); assert_eq!(out["filing_enabled"], false);
    let split = out["proposals"].as_array().unwrap().iter().find(|p| p["kind"] == "pension_split_t1032").expect("pension split proposal").clone();
    assert_eq!(split["from_return_id"], json!(a.id));
    let amount = split["amount_cents"].as_i64().unwrap();
    assert!(amount > 0 && amount <= 3_000_000, "never above the 50% T1032 cap");
    // The delta is exactly the engine's combined tax with the split minus without it.
    let (sa, sb) = (Side { id: &a.id, year: 2025, jurisdiction: "AB", inputs: &a_in }, Side { id: &b.id, year: 2025, jurisdiction: "AB", inputs: &b_in });
    let mut a2 = a_in.clone(); a2["pension_split_out_cents"] = json!(amount);
    let mut b2 = b_in.clone(); b2["pension_split_in_cents"] = json!(amount);
    let before = spouse_optimize::income_tax(&sa, &a_in).unwrap() + spouse_optimize::income_tax(&sb, &b_in).unwrap();
    let after = spouse_optimize::income_tax(&sa, &a2).unwrap() + spouse_optimize::income_tax(&sb, &b2).unwrap();
    assert_eq!(split["tax_delta_cents"].as_i64().unwrap(), after - before);
    assert!(after < before);
    // Accepting writes an audit event only.
    let acc = tax.accept_proposal(&fid, "pension_split_t1032").await.ok().unwrap();
    assert_eq!(acc["applied"], false);
    assert_eq!(tax.get(&a.id).await.ok().unwrap().revision, a.revision);
    assert_eq!(tax.get(&b.id).await.ok().unwrap().inputs, b_in);
    assert!(tax.audit().await.ok().unwrap().iter().any(|e| e.action == "spouse_proposal_accepted" && e.return_id == a.id));
    assert_eq!(tax.accept_proposal(&fid, "schedule2_tuition_transfer").await.unwrap_err().0, 409);
    assert_eq!(tax.accept_proposal(&fid, "made_up").await.unwrap_err().0, 400);
    assert_eq!(tax.family_for_return(&b.id).await.ok().unwrap()["id"], json!(fid));
}

#[test]
fn t2_intake_routes_and_period_rules() {
    let today = day("2026-11-04");
    assert_eq!(t2_intake::evaluate(&answers(json!({})), today)["next"], "incorporated");
    assert_eq!(t2_intake::evaluate(&answers(json!({"incorporated":false})), today)["outcome"]["route"], "t1_self_employment");
    for loc in ["QC", "MULTI", "OUTSIDE"] { assert_eq!(t2_intake::evaluate(&answers(json!({"incorporated":true,"location":loc})), today)["outcome"]["route"], "out_of_scope"); }
    let early = t2_intake::evaluate(&answers(json!({"incorporated":true,"location":"AB","period_start":"2022-06-01","period_end":"2023-05-31"})), today);
    assert_eq!(early["next"], "period"); assert!(early["errors"]["period_start"].is_string());
    let future = t2_intake::evaluate(&answers(json!({"incorporated":true,"location":"AB","period_start":"2026-01-01","period_end":"2026-12-31"})), today);
    assert!(future["errors"]["period_end"].is_string());
    let long = t2_intake::evaluate(&answers(json!({"incorporated":true,"location":"AB","period_start":"2024-01-01","period_end":"2025-03-31"})), today);
    assert!(long["errors"]["period_end"].is_string());
    let p = json!({"incorporated":true,"location":"AB","period_start":"2025-01-01","period_end":"2025-12-31"});
    assert_eq!(t2_intake::evaluate(&answers(p.clone()), today)["next"], "income");
    let mut full = p.clone(); full["had_income"] = json!(true);
    assert_eq!(t2_intake::evaluate(&answers(full), today)["outcome"]["route"], "full_t2");
    let mut q = p.clone(); q["had_income"] = json!(false);
    assert_eq!(t2_intake::evaluate(&answers(q.clone()), today)["next"], "expenses");
    q["other_expenses"] = json!(false);
    assert_eq!(t2_intake::evaluate(&answers(q.clone()), today)["outcome"]["route"], "nil_t2");
    q["other_expenses"] = json!(true);
    assert_eq!(t2_intake::evaluate(&answers(q), today)["outcome"]["route"], "full_t2");
}

#[tokio::test]
async fn t2_intake_dialogs_and_start() {
    let tax = Tax::new(Store::memory());
    let today = day("2026-12-31");
    let nil = json!({"incorporated":true,"location":"AB","period_start":"2025-01-01","period_end":"2025-12-31","had_income":false,"other_expenses":false,"client_ref":"CORP-SYN-001"});
    let started = tax.t2_intake_start(&answers(nil.clone()), "Example Corporation 001 T2", None, today).await.ok().unwrap();
    let first = started["return"].clone();
    assert_eq!(first["program"], "T2"); assert_eq!(first["tax_year"], 2025); assert_eq!(started["outcome"]["route"], "nil_t2"); assert_eq!(started["filing_enabled"], false);
    // Overlapping period for the same corporation blocks.
    let mut overlap = nil.clone(); overlap["period_start"] = json!("2025-07-01"); overlap["period_end"] = json!("2026-06-30");
    let v = tax.t2_intake(&answers(overlap.clone()), today).await.ok().unwrap();
    assert!(v["dialogs"].as_array().unwrap().iter().any(|d| d["kind"] == "overlapping_period" && d["blocking"] == true));
    assert_eq!(tax.t2_intake_start(&answers(overlap), "dup", None, today).await.unwrap_err().0, 409);
    // Next year: carry-forward offered from the prior-year T2.
    let mut next = nil.clone(); next["period_start"] = json!("2026-01-01"); next["period_end"] = json!("2026-12-31"); next["had_income"] = json!(true);
    let v = tax.t2_intake(&answers(next.clone()), today).await.ok().unwrap();
    let cf = v["dialogs"].as_array().unwrap().iter().find(|d| d["kind"] == "carry_forward").unwrap().clone();
    assert_eq!(cf["return_id"], first["id"]); assert_eq!(cf["blocking"], false);
    assert_eq!(v["gt2_import"]["supported"], false);
    let second = tax.t2_intake_start(&answers(next.clone()), "Example Corporation 001 T2 2026", first["id"].as_str(), today).await.ok().unwrap();
    assert_eq!(second["return"]["rollforward_from"], first["id"]); assert_eq!(second["outcome"]["route"], "full_t2");
    // Reopening the 2025 return with 2026 dates hits the locked-tax-year dialog.
    let mut edit = next.clone(); edit["editing_return_id"] = first["id"].clone(); edit["client_ref"] = json!("");
    let v = tax.t2_intake(&answers(edit), today).await.ok().unwrap();
    assert!(v["dialogs"].as_array().unwrap().iter().any(|d| d["kind"] == "locked_tax_year" && d["blocking"] == true));
    // Out-of-scope routes cannot start a return.
    assert_eq!(tax.t2_intake_start(&answers(json!({"incorporated":true,"location":"QC"})), "x", None, today).await.unwrap_err().0, 400);
    assert!(tax.audit().await.ok().unwrap().iter().any(|e| e.action == "t2_intake"));
}

#[test]
fn slip_catalog_lists_every_kind_with_calculated_boxes() {
    let c = nxt_os::tax::slips::catalog();
    let kinds: Vec<&str> = c["slips"].as_array().unwrap().iter().map(|s| s["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["T4", "T4A", "T5", "T3", "T5008", "RRSP", "T2202"]);
    assert!(c["slips"][0]["boxes"].as_array().unwrap().iter().any(|b| b["box"] == "14" && b["status"] == "calculated"));
}

/// Runs the guided-intake node tests (node is installed before `cargo test` in CI).
#[test]
fn guided_intake_node_tests_pass() {
    let files = ["tests/t1_interview.test.mjs", "tests/t1_slips_search.test.mjs", "tests/t1_review.test.mjs", "tests/t2_intake.test.mjs", "tests/turbotax_no_cra_network.test.mjs"];
    let out = std::process::Command::new("node").arg("--test").args(files).current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("node must be installed to run the guided-intake UI tests");
    assert!(out.status.success(), "node tests failed:\n{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
