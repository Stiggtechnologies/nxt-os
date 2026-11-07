//! Observations of amounts actually printed in a verified cached native PDF.
//! Separate from extract values: no replacement, version preference or approval.
use super::preserved_records::SourceFactPage;
use super::{ImportError, SourceImport, bad, conflict, missing, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
type Result<T> = std::result::Result<T, ImportError>;
const CLOSED_SHA: &str = "f3593686959075ed6dd0c7224fd88d5b4e20ab6ca732519d166f4ab6fd4cda95";
const MAX_BATCH: usize = 25;
const MAX_RECORDS: usize = 4000;
const LINES: [&str; 6] = ["15000", "23600", "26000", "42000", "48400", "48500"];
const ARCHIVE: &str = "499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c";
const CSV: &str = "c16c0bdc3036932f54e09cdd698513344c945cc4e8ba006cca54cd2fe8f93a94";
const BACKUP: &str = "7b760c8406ac29f6942898818c28be03c823128fdb164b5b03885f9f37b0eed5";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedFigureBatch {
    pub test_copy: bool,
    pub printed_figure_profile_sha256: String,
    pub records: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedFigureLookup {
    pub test_copy: bool,
    pub record_ids: Vec<String>,
    #[serde(default)]
    pub require_present: bool,
}
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Counts {
    nonnull: usize,
    zero: usize,
    blank: usize,
    held: usize,
    notlocated: usize,
}
#[derive(Deserialize)]
struct ClosedProfile {
    contract: Value,
    profile_sha256: String,
    source_package_sha256: String,
    record_count: usize,
    allowed_record_sha256: BTreeMap<String, Counts>,
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn number(s: &str) -> bool {
    !s.is_empty() && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit())
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| bad("Printed observation text missing"))
}
fn canonical(v: &Value) -> Result<Vec<u8>> {
    fn bound(v: &Value, depth: usize) -> bool {
        depth <= 24
            && match v {
                Value::Number(n) => n.is_i64() || n.is_u64(),
                Value::Array(a) => a.len() <= 1000 && a.iter().all(|x| bound(x, depth + 1)),
                Value::Object(m) => m.len() <= 100 && m.values().all(|x| bound(x, depth + 1)),
                Value::String(s) => s.len() <= 131072,
                _ => true,
            }
    }
    if !bound(v, 0) {
        return Err(bad("Printed observation structure exceeds bound"));
    }
    let raw = serde_json::to_vec(v).map_err(|_| bad("Cannot encode printed observation"))?;
    if raw.len() > 512 * 1024 {
        return Err(bad("Printed observation exceeds byte bound"));
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::io::{BufRead, BufReader};
    #[test]
    fn closed_profile_and_decimal_null_bounds_are_explicit() {
        let p = profile();
        assert_eq!(p.record_count, 2527);
        let sums = p
            .allowed_record_sha256
            .values()
            .fold([0usize; 5], |mut a, c| {
                for (i, n) in [c.nonnull, c.zero, c.blank, c.held, c.notlocated]
                    .iter()
                    .enumerate()
                {
                    a[i] += n;
                }
                a
            });
        assert_eq!(sums, [11734, 3691, 3424, 4, 0]);
        assert_eq!(decimal(0), "0.00");
        assert_eq!(decimal(-1), "-0.01");
        assert_eq!(decimal(i64::MIN), "-92233720368547758.08");
        for s in ["-0", "01", "1.00", "9223372036854775808"] {
            assert!(amount(s).is_err());
        }
        assert!(canonical(&json!({"coordinate_outside_preserved_JSON_string":1.5})).is_err());
        assert!(pinned(&json!({"client_number":"100"})).is_err());
    }
    #[tokio::test]
    async fn new_lane_is_empty_and_rejects_invalid_batch_or_lookup_without_source_writes() {
        let s = SourceImport::new(Store::memory());
        let company = "printed-qa";
        assert_eq!(
            s.test_copy_printed_cached_figure_summary(company)
                .await
                .unwrap()["PDF_observation_records"],
            0
        );
        for (test_copy, rows) in [
            (false, vec![]),
            (true, vec![]),
            (true, vec![Value::Null; 26]),
        ] {
            assert!(
                s.test_copy_printed_cached_figures(
                    company,
                    PrintedFigureBatch {
                        test_copy,
                        printed_figure_profile_sha256: profile().profile_sha256.clone(),
                        records: rows
                    }
                )
                .await
                .is_err()
            );
        }
        assert!(
            s.test_copy_read_printed_cached_figures(
                company,
                PrintedFigureLookup {
                    test_copy: true,
                    record_ids: vec!["bad-id".into()],
                    require_present: true
                }
            )
            .await
            .is_err()
        );
        let id = format!("100:2024:{}", "a".repeat(64));
        let r = s
            .test_copy_read_printed_cached_figures(
                company,
                PrintedFigureLookup {
                    test_copy: true,
                    record_ids: vec![id.clone()],
                    require_present: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(r["records"], json!([null]));
        assert!(
            s.test_copy_read_printed_cached_figures(
                company,
                PrintedFigureLookup {
                    test_copy: true,
                    record_ids: vec![id],
                    require_present: true
                }
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn valid_year_without_closed_observations_returns_explicit_empty_page() {
        let s = SourceImport::new(Store::memory());
        for year in [1990, 2025, 2026, 2100] {
            let page = SourceFactPage {
                kind: "printed-cached-figures".into(),
                offset: 0,
                limit: 25,
                source_return_year: Some(year),
                _routing_path: None,
            };
            let result = s
                .test_copy_client_printed_cached_figures("printed-qa", "100", page)
                .await
                .unwrap();
            assert_eq!(result["source_return_year"], year);
            assert_eq!(result["total"], 0);
            assert_eq!(result["records"], json!([]));
            assert_eq!(result["tax_cases_imported"], 0);
        }
        for year in [1989, 2101] {
            let page = SourceFactPage {
                kind: "printed-cached-figures".into(),
                offset: 0,
                limit: 25,
                source_return_year: Some(year),
                _routing_path: None,
            };
            assert!(
                s.test_copy_client_printed_cached_figures("printed-qa", "100", page)
                    .await
                    .is_err()
            );
        }
        assert!(!valid_id(&format!("100:2026:{}", "a".repeat(64))));
    }
    async fn controls(s: &SourceImport, c: &str) -> Vec<Value> {
        vec![
            s.test_copy_summary(c).await.unwrap(),
            s.test_copy_source_fact_summary(c).await.unwrap(),
            s.test_copy_native_slip_summary(c).await.unwrap(),
            s.test_copy_native_supplement_summary(c).await.unwrap(),
            s.test_copy_legacy_owner_summary(c).await.unwrap(),
            s.test_copy_personal_source_summary(c).await.unwrap(),
        ]
    }
    async fn seed_profiles(store: &Store, c: &str, profiles: &[Value]) {
        for p in profiles {
            let n = p["client_number"].as_str().unwrap();
            store
                .set(
                    &format!("dtmaxtc:{c}:client:{n}"),
                    &serde_json::to_string(p).unwrap(),
                )
                .await
                .unwrap();
            store
                .sadd(&format!("dtmaxtc:{c}:clients"), n)
                .await
                .unwrap();
        }
    }
    async fn check_profiles(store: &Store, c: &str, profiles: &[Value]) {
        for chunk in profiles.chunks(500) {
            let keys: Vec<_> = chunk
                .iter()
                .map(|p| {
                    format!(
                        "dtmaxtc:{c}:client:{}",
                        p["client_number"].as_str().unwrap()
                    )
                })
                .collect();
            let raw = store.mget(&keys).await.unwrap();
            for (p, b) in chunk.iter().zip(raw) {
                assert!(
                    b == Some(serde_json::to_string(p).unwrap()),
                    "Private current profile changed"
                );
            }
        }
    }
    fn pdf_record(v: &Value) -> String {
        let mut p = v["pdf"].clone();
        p.as_object_mut().unwrap().remove("page_count");
        serde_json::to_string(&p).unwrap()
    }
    async fn seed_pdf(
        store: &Store,
        c: &str,
        v: &Value,
        files: &Value,
    ) -> (String, String, String) {
        let p = &v["pdf"];
        let h = p["sha256"].as_str().unwrap();
        let fixture = &files["PDFs"][h];
        assert!(
            fixture["descriptor"] == *p,
            "Private PDF descriptor differs from staged upload"
        );
        let raw = std::fs::read(fixture["file"].as_str().unwrap()).unwrap();
        assert!(
            hex::encode(Sha256::digest(&raw)) == h,
            "Private PDF file digest differs"
        );
        assert!(
            raw.len() == p["bytes"].as_u64().unwrap() as usize,
            "Private PDF size differs"
        );
        let id = identity(v).unwrap();
        let record = pdf_record(v);
        let content = hex::encode(raw);
        store
            .set(&format!("dtmaxtc:{c}:return_pdf:record:{id}"), &record)
            .await
            .unwrap();
        store
            .set(&format!("dtmaxtc:{c}:return_pdf:blob:{h}"), &content)
            .await
            .unwrap();
        store
            .sadd(
                &format!(
                    "dtmaxtc:{c}:return_pdf:client:{}",
                    p["client_number"].as_str().unwrap()
                ),
                &id,
            )
            .await
            .unwrap();
        (id, record, hex::encode(Sha256::digest(content.as_bytes())))
    }
    async fn mutations_are_rejected(s: &SourceImport, c: &str, row: &Value) {
        for variant in [
            "owner",
            "year",
            "PDF",
            "amount",
            "bbox",
            "candidate_sha",
            "approval",
            "line_order",
            "extra_field",
        ] {
            let mut changed = row.clone();
            match variant {
                "owner" => {
                    changed["client_number"] = json!("999999999");
                    changed["pdf"]["client_number"] = json!("999999999")
                }
                "year" => changed["source_return_year"] = json!(2022),
                "PDF" => changed["pdf"]["sha256"] = json!("f".repeat(64)),
                "amount" => changed["observations"][0]["printed_cents"] = json!("999999999"),
                "bbox" => {
                    let o = &mut changed["observations"][0];
                    let mut a: Value =
                        serde_json::from_str(o["printed_candidates_json"].as_str().unwrap())
                            .unwrap();
                    if !a.as_array().unwrap().is_empty() {
                        let k = if a[0].get("line_code_bbox").is_some() {
                            "line_code_bbox"
                        } else {
                            "code_bbox"
                        };
                        a[0][k][0] = json!(20001);
                    }
                    let raw = serde_json::to_string(&a).unwrap();
                    o["printed_candidates_sha256"] =
                        json!(hex::encode(Sha256::digest(raw.as_bytes())));
                    o["printed_candidates_json"] = json!(raw)
                }
                "candidate_sha" => {
                    changed["observations"][0]["printed_candidates_sha256"] = json!("0".repeat(64))
                }
                "approval" => changed["financial_values_approved"] = json!(true),
                "line_order" => changed["observations"].as_array_mut().unwrap().swap(0, 1),
                _ => changed["unreviewed_field"] = json!(true),
            }
            assert!(
                s.test_copy_printed_cached_figures(
                    c,
                    PrintedFigureBatch {
                        test_copy: true,
                        printed_figure_profile_sha256: profile().profile_sha256.clone(),
                        records: vec![changed]
                    }
                )
                .await
                .is_err(),
                "Accepted a mutated private printed record"
            );
        }
    }
    #[tokio::test]
    async fn optional_all_actual_closed_records_import_replay_exact_readback_and_keep_all5856profiles_and_pdfs()
     {
        let Ok(input) = std::env::var("NXT_DTMAX_PRINTED_OBSERVATIONS_INPUT") else {
            return;
        };
        let fixture = std::env::var("NXT_DTMAX_PRINTED_PDF_FIXTURE")
            .expect("Private staged PDF anchors required");
        let profiles_path = std::env::var("NXT_DTMAX_PRINTED_PROFILE_FIXTURE")
            .expect("Private current profiles required");
        let raw = std::fs::read(&input).unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&raw)),
            profile().source_package_sha256
        );
        let rows: Vec<Value> = BufReader::new(raw.as_slice())
            .lines()
            .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
            .collect();
        assert_eq!(rows.len(), profile().record_count);
        let fixtures: Value = serde_json::from_slice(&std::fs::read(fixture).unwrap()).unwrap();
        let baseline: Value =
            serde_json::from_slice(&std::fs::read(profiles_path).unwrap()).unwrap();
        let profiles = baseline["clients"].as_array().unwrap();
        assert_eq!(profiles.len(), 5856);
        assert_eq!(
            hex::encode(Sha256::digest(serde_json::to_vec(profiles).unwrap())),
            "f3e8ecfb623139f87314965377aeff624ec9b32392c96aaf3c7c714c17dfd8d2"
        );
        let company = "all-private-printed-figures";
        let mut count = 0;
        // Bounded batches avoid retaining 1.6GiB of hex PDF blobs in the unit-test
        // process. Every actual record is imported, replayed and fully reread.
        for chunk in rows.chunks(MAX_BATCH) {
            if count % 250 == 0 {
                println!(
                    "PRIVATE_PRINTED_OBSERVATION_PROGRESS: records_checked={count}; total={}",
                    rows.len()
                );
            }
            let store = Store::memory();
            let s = SourceImport::new(store.clone());
            seed_profiles(&store, company, profiles).await;
            let before = controls(&s, company).await;
            for (i, v) in chunk.iter().enumerate() {
                if let Err(e) = pinned(v) {
                    panic!(
                        "Private printed shape/pin failure at position {}: {}",
                        count + i,
                        e.1
                    );
                }
            }
            if count == 0 {
                assert!(
                    s.test_copy_printed_cached_figures(
                        company,
                        PrintedFigureBatch {
                            test_copy: true,
                            printed_figure_profile_sha256: profile().profile_sha256.clone(),
                            records: vec![chunk[0].clone()]
                        }
                    )
                    .await
                    .is_err(),
                    "Missing uploaded PDF accepted"
                );
                assert_eq!(
                    s.test_copy_printed_cached_figure_summary(company)
                        .await
                        .unwrap()["PDF_observation_records"],
                    0
                );
            }
            let mut pdf_before = Vec::new();
            for row in chunk {
                pdf_before.push(seed_pdf(&store, company, row, &fixtures).await);
            }
            if count == 0 {
                mutations_are_rejected(&s, company, &chunk[0]).await;
                let row = &chunk[0];
                let id = identity(row).unwrap();
                let rk = format!("dtmaxtc:{company}:return_pdf:record:{id}");
                let original = store.get(&rk).await.unwrap().unwrap();
                let mut changed: Value = serde_json::from_str(&original).unwrap();
                changed["client_number"] = json!("999999999");
                store.set(&rk, &changed.to_string()).await.unwrap();
                assert!(
                    s.test_copy_printed_cached_figures(
                        company,
                        PrintedFigureBatch {
                            test_copy: true,
                            printed_figure_profile_sha256: profile().profile_sha256.clone(),
                            records: vec![row.clone()]
                        }
                    )
                    .await
                    .is_err(),
                    "Wrong uploaded PDF owner accepted"
                );
                store.set(&rk, &original).await.unwrap();
                let bk = format!(
                    "dtmaxtc:{company}:return_pdf:blob:{}",
                    row["pdf"]["sha256"].as_str().unwrap()
                );
                let blob = store.get(&bk).await.unwrap().unwrap();
                store.set(&bk, "00").await.unwrap();
                assert!(
                    s.test_copy_printed_cached_figures(
                        company,
                        PrintedFigureBatch {
                            test_copy: true,
                            printed_figure_profile_sha256: profile().profile_sha256.clone(),
                            records: vec![row.clone()]
                        }
                    )
                    .await
                    .is_err(),
                    "Altered uploaded PDF bytes accepted"
                );
                store.set(&bk, &blob).await.unwrap();
            }
            let batch = || PrintedFigureBatch {
                test_copy: true,
                printed_figure_profile_sha256: profile().profile_sha256.clone(),
                records: chunk.to_vec(),
            };
            let imported = s
                .test_copy_printed_cached_figures(company, batch())
                .await
                .unwrap();
            assert_eq!(imported["created"], chunk.len());
            assert_eq!(
                s.test_copy_printed_cached_figures(company, batch())
                    .await
                    .unwrap()["unchanged_replays"],
                chunk.len()
            );
            let ids = chunk
                .iter()
                .map(identity)
                .collect::<Result<Vec<_>>>()
                .unwrap();
            let read = s
                .test_copy_read_printed_cached_figures(
                    company,
                    PrintedFigureLookup {
                        test_copy: true,
                        record_ids: ids,
                        require_present: true,
                    },
                )
                .await
                .unwrap();
            assert!(
                read["records"].as_array().unwrap() == chunk,
                "Private complete printed observation readback differs"
            );
            for row in chunk {
                let page = SourceFactPage {
                    kind: "printed-cached-figures".into(),
                    offset: 0,
                    limit: 25,
                    source_return_year: Some(row["source_return_year"].as_u64().unwrap() as u16),
                    _routing_path: None,
                };
                let result = s
                    .test_copy_client_printed_cached_figures(
                        company,
                        row["client_number"].as_str().unwrap(),
                        page,
                    )
                    .await
                    .unwrap();
                assert!(
                    result["records"].as_array().unwrap().contains(row),
                    "Private client-year projection missing exact observation"
                );
            }
            for (id, record, blob_digest) in pdf_before {
                assert!(
                    store
                        .get(&format!("dtmaxtc:{company}:return_pdf:record:{id}"))
                        .await
                        .unwrap()
                        .as_deref()
                        == Some(&record),
                    "Original PDF metadata changed"
                );
                let sha = id.rsplit(':').next().unwrap();
                assert!(
                    hex::encode(Sha256::digest(
                        store
                            .get(&format!("dtmaxtc:{company}:return_pdf:blob:{sha}"))
                            .await
                            .unwrap()
                            .unwrap()
                            .as_bytes()
                    )) == blob_digest,
                    "Original PDF bytes changed"
                );
            }
            assert!(
                controls(&s, company).await == before,
                "Original six control summaries changed"
            );
            check_profiles(&store, company, profiles).await;
            assert_eq!(
                s.test_copy_printed_cached_figure_summary(company)
                    .await
                    .unwrap()["PDF_observation_records"],
                chunk.len()
            );
            count += chunk.len();
            store.take_test_reads();
        }
        assert_eq!(count, profile().record_count);
        println!(
            "PRIVATE_PRINTED_OBSERVATIONS_ALL_EXACT: records={count}; current_profiles=5856; bounded_batch=25; original_PDF_bytes_and_metadata_unchanged=true; six_controls_unchanged=true"
        );
    }
}
fn profile() -> &'static ClosedProfile {
    static P: OnceLock<ClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("printed_cached_figure_profile.json");
        assert_eq!(
            hex::encode(Sha256::digest(raw)),
            CLOSED_SHA,
            "Closed printed-figure profile differs"
        );
        let p: ClosedProfile = serde_json::from_slice(raw).expect("closed printed-figure profile");
        assert!(hash(&p.profile_sha256) && hash(&p.source_package_sha256));
        assert!(p.record_count > 0 && p.record_count <= MAX_RECORDS);
        assert_eq!(p.allowed_record_sha256.len(), p.record_count);
        assert_eq!(
            hex::encode(Sha256::digest(
                canonical(&p.contract).expect("printed contract canonicalizes")
            )),
            p.profile_sha256
        );
        assert!(p.allowed_record_sha256.iter().all(|(h, c)| hash(h)
            && c.nonnull + c.blank + c.held + c.notlocated == 6
            && c.zero <= c.nonnull));
        p
    })
}
fn identity(v: &Value) -> Result<String> {
    Ok(format!(
        "{}:{}:{}",
        text(v, "client_number")?,
        v["source_return_year"]
            .as_u64()
            .ok_or_else(|| bad("Printed observation year missing"))?,
        text(&v["pdf"], "sha256")?
    ))
}
fn valid_id(id: &str) -> bool {
    let p: Vec<_> = id.split(':').collect();
    p.len() == 3 && number(p[0]) && matches!(p[1], "2023" | "2024") && hash(p[2])
}
fn key(c: &str, id: &str) -> String {
    format!(
        "dtmaxtc:{c}:printed_cached_figures:{}:record:{id}",
        profile().profile_sha256
    )
}
fn index(c: &str, s: &str) -> String {
    format!(
        "dtmaxtc:{c}:printed_cached_figures:{}:{s}",
        profile().profile_sha256
    )
}
fn member(c: &str, client: &str) -> String {
    index(c, &format!("client:{client}"))
}
fn flags() -> Value {
    json!({"original_history_modified":0,"original_PDFs_modified":0,"current_profiles_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false,"as_filed_verified":false})
}
fn envelope(company: &str) -> Value {
    let mut v = flags();
    v["test_copy"] = json!(true);
    v["source_scope"] = json!(company);
    v["printed_figure_profile_sha256"] = json!(profile().profile_sha256);
    v
}
fn decimal(n: i64) -> String {
    let a = n.unsigned_abs();
    format!("{}{}.{:02}", if n < 0 { "-" } else { "" }, a / 100, a % 100)
}
fn amount(s: &str) -> Result<i64> {
    let n = s
        .parse::<i64>()
        .map_err(|_| bad("Printed amount outside signed integer range"))?;
    if n.to_string() != s {
        return Err(bad("Printed amount is not canonical decimal cents"));
    }
    Ok(n)
}
fn candidates(o: &Value, pages: usize) -> Result<Vec<Value>> {
    let raw = text(o, "printed_candidates_json")?;
    if raw.len() > 65536
        || hex::encode(Sha256::digest(raw.as_bytes())) != text(o, "printed_candidates_sha256")?
    {
        return Err(bad("Printed candidate evidence digest differs"));
    }
    let c: Vec<Value> =
        serde_json::from_str(raw).map_err(|_| bad("Printed candidate evidence is invalid JSON"))?;
    if c.len() > 100 {
        return Err(bad("Printed candidate count exceeds bound"));
    }
    for v in &c {
        if v["page_index"].as_u64().is_none_or(|p| p >= pages as u64)
            || v["printed_code"] != o["line_code"]
        {
            return Err(bad("Printed candidate page or line differs"));
        }
        let b = v
            .get("line_code_bbox")
            .or_else(|| v.get("code_bbox"))
            .and_then(Value::as_array)
            .ok_or_else(|| bad("Printed candidate bounding box missing"))?;
        let a: Vec<f64> = b.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect();
        if a.len() != 4
            || a.iter().any(|n| !n.is_finite() || *n < 0.0 || *n > 20000.0)
            || a[0] >= a[2]
            || a[1] >= a[3]
        {
            return Err(bad("Printed candidate bounding box is invalid"));
        }
        if !matches!(
            v["state"].as_str(),
            Some(
                "printed_amount_present"
                    | "identified_printed_amount_blank"
                    | "held_ambiguous_money_tokens"
                    | "held_invalid_money_tokens"
                    | "held_money_token_crosses_box_boundary"
            )
        ) {
            return Err(bad("Printed candidate state differs"));
        }
    }
    Ok(c)
}
fn validate_shape(v: &Value) -> Result<Counts> {
    if v["schema"] != "dtmax-printed-source-figure-observation/v1"
        || !number(text(v, "client_number")?)
        || !number(text(v, "family_file_id")?)
        || !matches!(v["source_return_year"].as_u64(), Some(2023 | 2024))
        || v["source_fields_only"] != true
        || [
            "recalculated",
            "as_filed_verified",
            "preferred_version_selected",
            "financial_values_approved",
        ]
        .iter()
        .any(|k| v[*k] != false)
    {
        return Err(bad("Printed observation context or immutable flags differ"));
    }
    let p = &v["pdf"];
    let pages = p["page_count"]
        .as_u64()
        .ok_or_else(|| bad("Printed PDF page count missing"))? as usize;
    if p["client_number"] != v["client_number"]
        || p["tax_year"] != v["source_return_year"]
        || !hash(text(p, "sha256")?)
        || !(1..=500).contains(&pages)
        || p["bytes"]
            .as_u64()
            .is_none_or(|b| b == 0 || b > super::return_pdfs::MAX_PDF_BYTES as u64)
    {
        return Err(bad("Printed PDF owner, year, size or digest differs"));
    }
    let labels = p["page_labels"]
        .as_array()
        .ok_or_else(|| bad("Printed PDF page labels missing"))?;
    if labels.len() != pages
        || labels.iter().any(|x| {
            x.as_str()
                .is_none_or(|s| s.is_empty() || s.len() > 120 || s.chars().any(char::is_control))
        })
    {
        return Err(bad("Printed PDF page labels differ"));
    }
    let a = &v["anchors"];
    for k in [
        "source_backup_sha256",
        "source_archive_sha256",
        "source_figures_csv_sha256",
        "native_family_file_sha256",
        "pristine_manifest_sha256",
        "source_context_row_sha256",
        "source_qa_file_sha256",
        "source_qa_record_sha256",
        "source_geometry_sha256",
        "geometry_verifier_sha256",
    ] {
        if !hash(text(a, k)?) {
            return Err(bad("Printed source proof hash missing"));
        }
    }
    for k in [
        "official_cra_form_sha256",
        "supplemental_identity_proof_sha256",
    ] {
        if !a[k].is_null() && !hash(text(a, k)?) {
            return Err(bad("Printed optional proof hash differs"));
        }
    }
    let path = text(a, "native_family_file_relative_path")?;
    if path.is_empty()
        || path.len() > 200
        || path.contains(':')
        || path.starts_with(['/', '\\'])
        || path
            .split(['/', '\\'])
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(bad("Printed native family reference differs"));
    }
    let provenance = &p["export_provenance"];
    if a["source_backup_sha256"] != BACKUP
        || a["source_archive_sha256"] != ARCHIVE
        || a["source_figures_csv_sha256"] != CSV
        || provenance["kind"] != "cached_native_export"
        || provenance["source_backup_sha256"] != a["source_backup_sha256"]
        || provenance["native_family_file_sha256"] != a["native_family_file_sha256"]
        || provenance["figures_csv_sha256"] != a["source_figures_csv_sha256"]
        || provenance["recalculated"] != false
        || provenance["as_filed_verified"] != false
    {
        return Err(bad("Printed observation export provenance differs"));
    }
    let geometry = text(v, "geometry_evidence_json")?;
    if geometry.len() > 131072
        || hex::encode(Sha256::digest(geometry.as_bytes())) != a["source_geometry_sha256"]
    {
        return Err(bad("Printed geometry evidence digest differs"));
    }
    if !serde_json::from_str::<Value>(geometry).is_ok_and(|g| g.is_object()) {
        return Err(bad("Printed geometry evidence is invalid JSON"));
    }
    let obs = v["observations"]
        .as_array()
        .ok_or_else(|| bad("Printed observations missing"))?;
    if obs.len() != 6 {
        return Err(bad("Printed observations must contain six ordered lines"));
    }
    let mut count = Counts::default();
    for (o, line) in obs.iter().zip(LINES) {
        if o["line_code"] != line
            || o["source_comparison_state"] != provenance["figure_comparisons"][line]
            || !matches!(
                o["source_presence_state"].as_str(),
                Some("present" | "absent")
            )
        {
            return Err(bad("Printed comparison context or line order differs"));
        }
        let c = candidates(o, pages)?;
        match text(o, "state")? {
            "single_distinct_printed_amount" => {
                let n = amount(text(o, "printed_cents")?)?;
                if o["printed_decimal"] != decimal(n)
                    || c.is_empty()
                    || c.iter().any(|c| c["state"] != "printed_amount_present")
                {
                    return Err(bad("Printed amount evidence differs"));
                }
                count.nonnull += 1;
                count.zero += usize::from(n == 0);
            }
            state => {
                if !o["printed_cents"].is_null() || !o["printed_decimal"].is_null() {
                    return Err(bad(
                        "Absent or held printed observation cannot carry an amount",
                    ));
                }
                match state {
                    "identified_printed_blank" => {
                        if c.is_empty()
                            || c.iter()
                                .any(|c| c["state"] != "identified_printed_amount_blank")
                        {
                            return Err(bad("Printed blank evidence differs"));
                        }
                        count.blank += 1
                    }
                    "printed_line_not_located" => {
                        if !c.is_empty() {
                            return Err(bad("Unlocated printed line carries candidates"));
                        }
                        count.notlocated += 1
                    }
                    "held_ambiguous_printed_amount" | "held_invalid_printed_money_token" => {
                        if c.is_empty() {
                            return Err(bad("Held printed amount lacks original candidates"));
                        }
                        count.held += 1
                    }
                    _ => return Err(bad("Unsupported printed observation state")),
                }
            }
        }
    }
    Ok(count)
}
fn pinned(v: &Value) -> Result<(String, String, Counts)> {
    let raw = canonical(v)?;
    let digest = hex::encode(Sha256::digest(&raw));
    let counts = *profile()
        .allowed_record_sha256
        .get(&digest)
        .ok_or_else(|| bad("Printed observation outside independently verified closed PDF pins"))?;
    if v["printed_figure_profile_sha256"] != profile().profile_sha256 {
        return Err(bad("Printed observation profile differs"));
    }
    let found = validate_shape(v)?;
    if [
        found.nonnull,
        found.zero,
        found.blank,
        found.held,
        found.notlocated,
    ] != [
        counts.nonnull,
        counts.zero,
        counts.blank,
        counts.held,
        counts.notlocated,
    ] {
        return Err(bad("Printed observation presence counts differ"));
    }
    Ok((
        String::from_utf8(raw).map_err(|_| bad("Printed observation encoding differs"))?,
        digest,
        counts,
    ))
}
impl SourceImport {
    async fn prepare_printed_cached_figures(
        &self,
        company: &str,
        rows: &[Value],
    ) -> Result<Vec<(String, String, String)>> {
        let mut out = Vec::new();
        let mut ids = BTreeSet::new();
        for v in rows {
            let (raw, digest, _) = pinned(v)?;
            let id = identity(v)?;
            if !valid_id(&id) || !ids.insert(id.clone()) {
                return Err(bad("Printed observation identity is invalid or repeated"));
            }
            let client = text(v, "client_number")?;
            if self
                .store
                .get(&format!("dtmaxtc:{company}:client:{client}"))
                .await
                .map_err(|e| storage(&e))?
                .is_none()
            {
                return Err(missing(
                    "Printed PDF owner is not registered in this source scope",
                ));
            }
            let pdf = &v["pdf"];
            let source = self
                .test_copy_return_pdf_content(
                    company,
                    client,
                    pdf["tax_year"].as_u64().unwrap() as u16,
                    text(pdf, "sha256")?,
                )
                .await?;
            let uploaded = &source["record"];
            if [
                "client_number",
                "tax_year",
                "sha256",
                "bytes",
                "page_count",
                "page_labels",
                "export_provenance",
            ]
            .iter()
            .any(|k| uploaded[*k] != pdf[*k])
            {
                return Err(bad(
                    "Printed observation must match the uploaded original PDF and cached export provenance",
                ));
            }
            // Existing PDF reader verifies actual stored bytes against SHA and size.
            out.push((id, raw, digest));
        }
        Ok(out)
    }
    pub async fn test_copy_printed_cached_figures(
        &self,
        company: &str,
        batch: PrintedFigureBatch,
    ) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.printed_figure_profile_sha256 != profile().profile_sha256
            || batch.records.is_empty()
            || batch.records.len() > MAX_BATCH
        {
            return Err(bad("Printed observation batch profile or size differs"));
        }
        let prepared = self
            .prepare_printed_cached_figures(company, &batch.records)
            .await?;
        // Validate the complete batch and existing immutable keys before writes.
        for (id, raw, _) in &prepared {
            if self
                .store
                .get(&key(company, id))
                .await
                .map_err(|e| storage(&e))?
                .is_some_and(|old| old != *raw)
            {
                return Err(conflict("Printed PDF observations are immutable"));
            }
        }
        let (mut created, mut replay) = (0, 0);
        for ((id, raw, digest), v) in prepared.into_iter().zip(&batch.records) {
            if self
                .store
                .set_nx(&key(company, &id), &raw)
                .await
                .map_err(|e| storage(&e))?
            {
                created += 1
            } else {
                replay += 1
            }
            if self
                .store
                .get(&key(company, &id))
                .await
                .map_err(|e| storage(&e))?
                .as_deref()
                != Some(&raw)
            {
                return Err(conflict("Printed PDF observations are immutable"));
            }
            self.store
                .sadd(&index(company, "all"), &id)
                .await
                .map_err(|e| storage(&e))?;
            self.store
                .sadd(&member(company, text(v, "client_number")?), &id)
                .await
                .map_err(|e| storage(&e))?;
            self.store
                .sadd(&index(company, "digests"), &digest)
                .await
                .map_err(|e| storage(&e))?;
        }
        let mut r = envelope(company);
        r["created"] = json!(created);
        r["unchanged_replays"] = json!(replay);
        Ok(r)
    }
    pub async fn test_copy_read_printed_cached_figures(
        &self,
        company: &str,
        lookup: PrintedFigureLookup,
    ) -> Result<Value> {
        self.test_copy_guard(company, lookup.test_copy)?;
        if lookup.record_ids.is_empty()
            || lookup.record_ids.len() > MAX_BATCH
            || lookup.record_ids.iter().any(|id| !valid_id(id))
            || lookup.record_ids.iter().collect::<BTreeSet<_>>().len() != lookup.record_ids.len()
        {
            return Err(bad("Printed observation lookup identities or bound differ"));
        }
        let keys: Vec<_> = lookup
            .record_ids
            .iter()
            .map(|id| key(company, id))
            .collect();
        let raw = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if raw.len() != keys.len() {
            return Err(storage("Printed observation lookup cardinality differs"));
        }
        let rows: Vec<Value> = raw
            .into_iter()
            .map(|s| match s {
                Some(s) => serde_json::from_str(&s)
                    .map_err(|_| storage("Stored printed observations unreadable")),
                None if lookup.require_present => {
                    Err(missing("Printed PDF observations not imported"))
                }
                None => Ok(Value::Null),
            })
            .collect::<Result<_>>()?;
        for (id, v) in lookup.record_ids.iter().zip(&rows) {
            if !v.is_null() && identity(v)? != *id {
                return Err(storage("Stored printed observation identity differs"));
            }
        }
        let present: Vec<_> = rows.iter().filter(|v| !v.is_null()).cloned().collect();
        self.prepare_printed_cached_figures(company, &present)
            .await
            .map_err(|_| {
                storage("Stored printed observation or uploaded PDF provenance differs")
            })?;
        let mut r = envelope(company);
        r["record_ids"] = json!(lookup.record_ids);
        r["records"] = json!(rows);
        Ok(r)
    }
    pub async fn test_copy_client_printed_cached_figures(
        &self,
        company: &str,
        client: &str,
        page: SourceFactPage,
    ) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        if !number(client)
            || page.kind != "printed-cached-figures"
            || page.limit == 0
            || page.limit > MAX_BATCH
            || page
                .source_return_year
                .is_some_and(|y| !(1990..=2100).contains(&y))
        {
            return Err(bad("Printed observation client page or year differs"));
        }
        let mut ids = self
            .store
            .smembers(&member(company, client))
            .await
            .map_err(|e| storage(&e))?;
        ids.sort();
        if ids.len() > MAX_RECORDS
            || ids
                .iter()
                .any(|id| !valid_id(id) || id.split(':').next() != Some(client))
        {
            return Err(storage("Printed observation client index differs"));
        }
        if let Some(y) = page.source_return_year {
            ids.retain(|id| id.split(':').nth(1) == Some(y.to_string().as_str()))
        }
        let total = ids.len();
        let selected: Vec<_> = ids.into_iter().skip(page.offset).take(page.limit).collect();
        let mut r = if selected.is_empty() {
            let mut r = envelope(company);
            r["records"] = json!([]);
            r
        } else {
            self.test_copy_read_printed_cached_figures(
                company,
                PrintedFigureLookup {
                    test_copy: true,
                    record_ids: selected,
                    require_present: true,
                },
            )
            .await?
        };
        r.as_object_mut().unwrap().remove("record_ids");
        r["client_number"] = json!(client);
        r["kind"] = json!(page.kind);
        r["source_return_year"] = json!(page.source_return_year);
        r["offset"] = json!(page.offset);
        r["total"] = json!(total);
        Ok(r)
    }
    pub async fn test_copy_printed_cached_figure_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let ids = self
            .store
            .smembers(&index(company, "all"))
            .await
            .map_err(|e| storage(&e))?;
        let pins = self
            .store
            .smembers(&index(company, "digests"))
            .await
            .map_err(|e| storage(&e))?;
        if ids.len() > profile().record_count
            || ids.len() != pins.len()
            || ids.iter().any(|id| !valid_id(id))
        {
            return Err(storage("Printed observation summary index differs"));
        }
        let mut c = Counts::default();
        for pin in pins {
            let n = profile()
                .allowed_record_sha256
                .get(&pin)
                .ok_or_else(|| storage("Printed observation summary outside closed pins"))?;
            c.nonnull += n.nonnull;
            c.zero += n.zero;
            c.blank += n.blank;
            c.held += n.held;
            c.notlocated += n.notlocated;
        }
        let mut r = envelope(company);
        r["PDF_observation_records"] = json!(ids.len());
        r["printed_amount_observations"] = json!(c.nonnull);
        r["explicit_printed_zeros"] = json!(c.zero);
        r["identified_printed_blanks"] = json!(c.blank);
        r["held_printed_amounts"] = json!(c.held);
        r["unlocated_printed_lines"] = json!(c.notlocated);
        r["source_package_sha256"] = json!(profile().source_package_sha256);
        Ok(r)
    }
}
