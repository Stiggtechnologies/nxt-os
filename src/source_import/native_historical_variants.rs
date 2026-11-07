//! Immutable original E11 source versions. No preferred version, overwrite,
//! recalculation, identity inference, deduplication or financial approval.
use super::native_module_reviews::NativeModuleReviewLookup;
use super::personal_source_info::PROFILE as PERSONAL_PROFILE;
use super::preserved_records::SourceFactPage;
use super::{ImportError, SourceImport, bad, conflict, missing, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE: &str = "641c2b5f56617882829d5049813e06789a3a661580fe6f6d3adf6f8dde5e03ba";
const CLOSED_SHA: &str = "d0033674c71f759c53b38c715c4fe46516dff0099fbcadd846f961c39b0004f8";
const ARCHIVE: &str = "499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c";
const PACKAGE: &str = "7c40cf617765e7ba4bda1e1cc2afb1bda133541a7cd9125d27dfe71292b21ea4";
const LINES: [(&str, u64); 6] = [
    ("15000", 150),
    ("23600", 236),
    ("26000", 260),
    ("42000", 420),
    ("48400", 484),
    ("48500", 485),
];
const MAX_BATCH: usize = 100;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVariantBatch {
    pub test_copy: bool,
    pub source_variant_profile_sha256: String,
    pub records: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalVariantLookup {
    pub test_copy: bool,
    pub record_ids: Vec<String>,
    #[serde(default)]
    pub require_present: bool,
}
#[derive(Deserialize)]
struct ClosedProfile {
    source_profile_sha256: String,
    source_package_sha256: String,
    source_archive_sha256: String,
    source_profile: Value,
    record_count: usize,
    record_summaries: BTreeMap<String, [usize; 5]>,
}
fn profile() -> &'static ClosedProfile {
    static P: OnceLock<ClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("native_historical_variant_profile.json");
        assert_eq!(
            hex::encode(Sha256::digest(raw)),
            CLOSED_SHA,
            "closed historical version profile bytes differ"
        );
        let p: ClosedProfile =
            serde_json::from_slice(raw).expect("pinned historical version profile");
        assert_eq!(p.source_profile_sha256, PROFILE);
        assert_eq!(p.source_package_sha256, PACKAGE);
        assert_eq!(p.source_archive_sha256, ARCHIVE);
        assert_eq!(p.record_count, 32744);
        assert_eq!(p.record_summaries.len(), 32744);
        assert_eq!(
            hex::encode(Sha256::digest(
                canonical(&p.source_profile).expect("source version profile canonicalizes")
            )),
            PROFILE
        );
        p
    })
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn number(s: &str) -> bool {
    s.parse::<u32>()
        .is_ok_and(|n| n > 0 && n <= i32::MAX as u32 && n.to_string() == s)
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| bad("Historical version text missing"))
}
fn integer(v: &Value) -> bool {
    v.as_u64().is_some()
}
fn identity(v: &Value) -> Result<String> {
    Ok(format!(
        "{}:{}:{}",
        string(v, "client_number")?,
        v["source_return_year"]
            .as_u64()
            .ok_or_else(|| bad("Historical version year missing"))?,
        string(v, "source_archive_sha256")?
    ))
}
fn valid_id(id: &str) -> bool {
    let p: Vec<_> = id.split(':').collect();
    p.len() == 3
        && number(p[0])
        && p[1]
            .parse::<u16>()
            .is_ok_and(|y| (1990..=2100).contains(&y) && y.to_string() == p[1])
        && hash(p[2])
}
fn key(c: &str, id: &str) -> String {
    format!("dtmaxtc:{c}:historical_variants:{PROFILE}:record:{id}")
}
fn all(c: &str) -> String {
    format!("dtmaxtc:{c}:historical_variants:{PROFILE}:all")
}
fn digests(c: &str) -> String {
    format!("dtmaxtc:{c}:historical_variants:{PROFILE}:digests")
}
fn member(c: &str, client: &str) -> String {
    format!("dtmaxtc:{c}:historical_variants:{PROFILE}:member:{client}")
}
fn history(c: &str, id: &str) -> String {
    format!("dtmaxtc:{c}:historical_lines:{id}")
}
fn personal(c: &str, id: &str) -> String {
    format!("dtmaxtc:{c}:personal_source:{PERSONAL_PROFILE}:record:{id}")
}
fn parse(raw: Option<String>, why: &str) -> Result<Value> {
    serde_json::from_str(&raw.ok_or_else(|| missing(why))?)
        .map_err(|_| storage("Historical version dependency unreadable"))
}

// Compact UTF8, recursively lexical object keys, array order unchanged. The
// independently reread package contains integer numbers only; floats refused.
fn canonical(v: &Value) -> Result<Vec<u8>> {
    fn write(v: &Value, out: &mut Vec<u8>, depth: usize) -> Result<()> {
        if depth > 24 || out.len() > 16384 {
            return Err(bad("Historical version payload exceeds bound"));
        }
        match v {
            Value::Object(m) => {
                out.push(b'{');
                let sorted: BTreeMap<_, _> = m.iter().collect();
                for (i, (k, v)) in sorted.into_iter().enumerate() {
                    if i > 0 {
                        out.push(b',')
                    }
                    serde_json::to_writer(&mut *out, k)
                        .map_err(|_| bad("Historical version key encoding failed"))?;
                    out.push(b':');
                    write(v, out, depth + 1)?
                }
                out.push(b'}')
            }
            Value::Array(a) => {
                out.push(b'[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(b',')
                    }
                    write(v, out, depth + 1)?
                }
                out.push(b']')
            }
            Value::String(s) => serde_json::to_writer(&mut *out, s)
                .map_err(|_| bad("Historical version text encoding failed"))?,
            Value::Number(n) => {
                if !n.is_i64() && !n.is_u64() {
                    return Err(bad("Floating historical wire value refused"));
                }
                out.extend_from_slice(n.to_string().as_bytes())
            }
            Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
            Value::Null => out.extend_from_slice(b"null"),
        }
        if out.len() > 16384 {
            return Err(bad("Historical version payload exceeds bound"));
        }
        Ok(())
    }
    let mut out = Vec::new();
    write(v, &mut out, 0)?;
    Ok(out)
}
fn pinned(v: &Value) -> Result<(String, String, [usize; 5])> {
    let raw = canonical(v)?;
    let digest = hex::encode(Sha256::digest(&raw));
    let counts = profile().record_summaries.get(&digest).ok_or_else(|| {
        bad("Historical version outside independently verified closed context pins")
    })?;
    Ok((
        String::from_utf8(raw).map_err(|_| bad("Historical version encoding failed"))?,
        digest,
        *counts,
    ))
}
fn decimal(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let n = cents.abs();
    format!("{sign}{}.{:02}", n / 100, n % 100)
}
fn validate(v: &Value, h: &Value, p: &Value) -> Result<(String, String, [usize; 5])> {
    let pinned = pinned(v)?;
    let id = identity(v)?;
    if !valid_id(&id)
        || !number(string(v, "family_file_id")?)
        || v["schema"] != "dtmax-native-E11-source-variants/v1"
        || v["source_variant_profile_sha256"] != PROFILE
        || v["source_archive_sha256"] != ARCHIVE
        || v["native_T1_origin_guard_verified"] != true
        || !["T1", "T3"].contains(&string(v, "reader_module")?)
    {
        return Err(bad(
            "Historical version profile or native source origin differs",
        ));
    }
    if h["client_number"] != v["client_number"]
        || h["family_file_id"] != v["family_file_id"]
        || h["tax_year"] != v["source_return_year"]
        || h["archive_sha256"] != v["source_archive_sha256"]
        || h["vendor_module"] != v["reader_module"]
        || h["mapping_state"] != v["legacy_form_mapping"]["original_CSV_mapping_state"]
    {
        return Err(bad(
            "Historical version must match original immutable historical context",
        ));
    }
    if p["client_number"] != v["client_number"]
        || p["family_file_id"] != v["family_file_id"]
        || p["source_year"] != v["source_return_year"]
        || p["archive_sha256"] != v["source_archive_sha256"]
        || p["reader_module"] != v["reader_module"]
        || p["source_context_offset"] != v["source_context_offset"]
        || p["source_EXC_sha256"] != v["source_EXC_sha256"]
        || p["info_record_state"] != "present_nonempty_crc_verified"
    {
        return Err(bad(
            "Historical version differs from original personal context frame",
        ));
    }
    let legacy = &v["legacy_form_mapping"];
    if legacy["native_personal_prefix"] != 182
        || legacy["original_history_or_CSV_modified"] != false
        || legacy["original_reader_module"] != v["reader_module"]
        || legacy["personal_prefix_differs_from_legacy_reader_heuristic"]
            != (v["reader_module"] == "T3")
    {
        return Err(bad("Historical version original reader metadata differs"));
    }
    let variants = v["source_variants"]
        .as_array()
        .ok_or_else(|| bad("Historical version array missing"))?;
    let mut counts = [variants.len(), 0, 0, 0, 0];
    let mut names = BTreeSet::new();
    for variant in variants {
        let (q, primary, continuation) = match variant["variant_name"].as_str() {
            Some("Production") => (0, 30, 31),
            Some("SnapshotFederal") => (7, 40, 41),
            _ => return Err(bad("Unsupported original source version")),
        };
        if variant["reader_q_plan"] != q
            || !integer(&variant["outer_plan"])
            || !integer(&variant["variant_occurrence"])
            || !variant["primary_record_present"].is_boolean()
            || !names.insert((
                q,
                variant["outer_plan"].as_u64(),
                variant["variant_occurrence"].as_u64(),
            ))
        {
            return Err(bad("Historical version selector or occurrence differs"));
        }
        let records = variant["source_records"]
            .as_array()
            .ok_or_else(|| bad("Historical version source records missing"))?;
        if records.is_empty() {
            return Err(bad(
                "Historical version requires original CRC source records",
            ));
        }
        let mut frame_keys = BTreeSet::new();
        for record in records {
            if ![primary, continuation]
                .iter()
                .any(|n| record["record_type"] == *n)
                || record["outer_plan"] != variant["outer_plan"]
                || record["personal_prefix_bytes"] != 182
                || record["CRC_verified"] != true
                || !integer(&record["frame_offset"])
                || !integer(&record["expanded_bytes"])
                || !hash(string(record, "frame_sha256")?)
                || !hash(string(record, "expanded_sha256")?)
            {
                return Err(bad("Historical version original CRC record differs"));
            }
            frame_keys.insert((
                record["frame_offset"].as_u64().unwrap(),
                string(record, "frame_sha256")?.to_owned(),
                string(record, "expanded_sha256")?.to_owned(),
            ));
        }
        if variant["primary_record_present"] != records.iter().any(|r| r["record_type"] == primary)
        {
            return Err(bad("Historical version primary record presence differs"));
        }
        let forms = variant["E11_forms"]
            .as_array()
            .ok_or_else(|| bad("Historical E11 form array missing"))?;
        counts[1] += forms.len();
        for (ordinal, form) in forms.iter().enumerate() {
            if form["form_name"] != "E11"
                || form["E11_form_ordinal"] != ordinal
                || form["reader_marker_field1_selector_eligible"] != true
                || !integer(&form["source_form_frame_offset"])
                || !integer(&form["source_form_offset"])
                || !hash(string(form, "source_form_header_sha256")?)
            {
                return Err(bad("Historical E11 form identity differs"));
            }
            let marker = form["marker_field1_occurrences"]
                .as_array()
                .ok_or_else(|| bad("Historical E11 marker missing"))?;
            if marker.is_empty() || marker.iter().any(|m| m["field_id"] != 1) {
                return Err(bad("Historical E11 reader marker differs"));
            }
            let fields = form["lines"]
                .as_object()
                .ok_or_else(|| bad("Historical E11 six-line map missing"))?;
            if fields.len() != 6 {
                return Err(bad("Historical E11 six-line map differs"));
            }
            for (line, fid) in LINES {
                let values = fields
                    .get(line)
                    .ok_or_else(|| bad("Historical E11 line presence missing"))?;
                if values.is_null() {
                    counts[4] += 1;
                    continue;
                }
                let values = values.as_array().filter(|a| !a.is_empty()).ok_or_else(|| {
                    bad("Present historical E11 line must retain ordered occurrences")
                })?;
                counts[2] += values.len();
                let mut previous = None;
                for (occurrence, f) in values.iter().enumerate() {
                    let wire = f["wire_cents"]
                        .as_i64()
                        .filter(|n| i32::try_from(*n).is_ok())
                        .ok_or_else(|| bad("Historical E11 wire cents outside signed int32"))?;
                    let at = f["expanded_record_offset"]
                        .as_u64()
                        .ok_or_else(|| bad("Historical E11 source field offset missing"))?;
                    let end = f["expanded_record_end"]
                        .as_u64()
                        .ok_or_else(|| bad("Historical E11 source field end missing"))?;
                    let ix = f["source_field_index"]
                        .as_u64()
                        .ok_or_else(|| bad("Historical E11 source field index missing"))?;
                    if f["field_id"] != fid
                        || f["field_id_occurrence"] != occurrence
                        || f["type_tag"] != 2
                        || f["wire_integer"] != wire
                        || f["source_decimal"] != decimal(wire)
                        || !f["source_text"].is_null()
                        || end <= at
                        || previous.is_some_and(|n| ix <= n)
                        || ix
                            >= form["original_field_count"]
                                .as_u64()
                                .ok_or_else(|| bad("Historical E11 field count missing"))?
                        || !hash(string(f, "source_field_sha256")?)
                    {
                        return Err(bad(
                            "Historical E11 financial source value or ordered occurrence differs",
                        ));
                    }
                    previous = Some(ix);
                    match f["state"].as_str() {
                        Some("explicit_zero") if wire == 0 => counts[3] += 1,
                        Some("present_wire_cents") if wire != 0 => {}
                        _ => return Err(bad("Historical E11 explicit zero state differs")),
                    }
                    let frame = (
                        f["source_EXR_frame_offset"]
                            .as_u64()
                            .ok_or_else(|| bad("Historical E11 source frame missing"))?,
                        string(f, "source_EXR_frame_sha256")?.to_owned(),
                        string(f, "source_expanded_record_sha256")?.to_owned(),
                    );
                    if !frame_keys.contains(&frame) {
                        return Err(bad(
                            "Historical E11 field is outside its original CRC record",
                        ));
                    }
                }
            }
        }
    }
    if counts != pinned.2 {
        return Err(bad(
            "Historical version aggregate differs from closed full payload",
        ));
    }
    Ok(pinned)
}

impl SourceImport {
    async fn prepare_native_historical_variants(
        &self,
        company: &str,
        rows: &[Value],
    ) -> Result<Vec<(String, String, [usize; 5])>> {
        let ids: Vec<_> = rows.iter().map(identity).collect::<Result<_>>()?;
        let mut keys: Vec<_> = ids.iter().map(|id| history(company, id)).collect();
        keys.extend(ids.iter().map(|id| personal(company, id)));
        let deps = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if deps.len() != rows.len() * 2 {
            return Err(storage("Historical version dependency cardinality differs"));
        }
        let mut prepared = Vec::new();
        let mut held = Vec::new();
        for (i, r) in rows.iter().enumerate() {
            let h = parse(deps[i].clone(), "Original historical context missing")?;
            let p = parse(
                deps[rows.len() + i].clone(),
                "Original personal context missing",
            )?;
            prepared.push(validate(r, &h, &p)?);
            if r["reader_module"] == "T3" {
                held.push(ids[i].clone())
            }
        }
        if !held.is_empty() {
            // Reuse the existing native-origin reader's full payload pins and
            // original-history/personal hold validation, never the old heuristic.
            let proof = self
                .test_copy_read_native_module_reviews(
                    company,
                    NativeModuleReviewLookup {
                        test_copy: true,
                        record_ids: held,
                    },
                )
                .await?;
            let proofs: BTreeMap<_, _> = proof["records"]
                .as_array()
                .ok_or_else(|| storage("Native origin proof response missing"))?
                .iter()
                .map(|r| {
                    let v = &r["native_module_review"];
                    (
                        format!(
                            "{}:{}:{}",
                            v["client_number"].as_str().unwrap_or(""),
                            v["source_year"],
                            v["archive_sha256"].as_str().unwrap_or("")
                        ),
                        v,
                    )
                })
                .collect();
            for (i, r) in rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r["reader_module"] == "T3")
            {
                let p = proofs
                    .get(&ids[i])
                    .ok_or_else(|| storage("Native module1 source proof missing"))?;
                if p["native_source_module"] != "T1"
                    || p["family_file_id"] != r["family_file_id"]
                    || p["source_frames"]["EXC"]["frame_offset"] != r["source_context_offset"]
                    || p["source_frames"]["EXC"]["frame_sha256"] != r["source_EXC_sha256"]
                {
                    return Err(bad(
                        "Historical source version native T1 proof context differs",
                    ));
                }
            }
        }
        Ok(prepared)
    }
    pub async fn test_copy_native_historical_variants(
        &self,
        company: &str,
        batch: HistoricalVariantBatch,
    ) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.source_variant_profile_sha256 != PROFILE
            || batch.records.is_empty()
            || batch.records.len() > MAX_BATCH
        {
            return Err(bad("Historical version profile or batch bound differs"));
        }
        let mut seen = BTreeSet::new();
        let ids: Vec<_> = batch
            .records
            .iter()
            .map(|r| {
                pinned(r)?;
                let id = identity(r)?;
                if !valid_id(&id) || !seen.insert(id.clone()) {
                    return Err(bad("Duplicate historical version context"));
                }
                Ok(id)
            })
            .collect::<Result<_>>()?;
        let prepared = self
            .prepare_native_historical_variants(company, &batch.records)
            .await?;
        let keys: Vec<_> = ids.iter().map(|id| key(company, id)).collect();
        let old = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if old.len() != prepared.len() {
            return Err(storage("Historical version preflight cardinality differs"));
        }
        for ((raw, _, _), old) in prepared.iter().zip(old) {
            if old.as_ref().is_some_and(|old| old != raw) {
                return Err(conflict("Historical source versions are immutable"));
            }
        }
        let (mut created, mut unchanged) = (0, 0);
        for start in (0..prepared.len()).step_by(4) {
            let mut jobs = tokio::task::JoinSet::new();
            for i in start..(start + 4).min(prepared.len()) {
                let store = self.store.clone();
                let (raw, digest, _) = prepared[i].clone();
                let id = ids[i].clone();
                let k = keys[i].clone();
                let all = all(company);
                let digests = digests(company);
                let member = member(company, string(&batch.records[i], "client_number")?);
                jobs.spawn(async move {
                    let added = store.set_nx(&k, &raw).await.map_err(|e| storage(&e))?;
                    if store.get(&k).await.map_err(|e| storage(&e))?.as_deref() != Some(&raw) {
                        return Err(conflict("Historical source versions are immutable"));
                    }
                    store.sadd(&member, &id).await.map_err(|e| storage(&e))?;
                    store.sadd(&all, &id).await.map_err(|e| storage(&e))?;
                    store
                        .sadd(&digests, &digest)
                        .await
                        .map_err(|e| storage(&e))?;
                    Ok(added)
                });
            }
            while let Some(r) = jobs.join_next().await {
                if r.map_err(|_| storage("Historical version write interrupted"))?? {
                    created += 1
                } else {
                    unchanged += 1
                }
            }
        }
        Ok(
            json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,"source_variant_profile_sha256":PROFILE,"original_history_modified":0,"personal_source_flags_modified":0,"current_profiles_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false}),
        )
    }
    pub async fn test_copy_read_native_historical_variants(
        &self,
        company: &str,
        lookup: HistoricalVariantLookup,
    ) -> Result<Value> {
        self.test_copy_guard(company, lookup.test_copy)?;
        if lookup.record_ids.is_empty()
            || lookup.record_ids.len() > MAX_BATCH
            || lookup.record_ids.iter().any(|id| !valid_id(id))
        {
            return Err(bad("Historical version lookup identities or bound differ"));
        }
        let keys: Vec<_> = lookup
            .record_ids
            .iter()
            .map(|id| key(company, id))
            .collect();
        let raw = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if raw.len() != keys.len() {
            return Err(storage("Historical version lookup cardinality differs"));
        }
        let rows: Vec<_> = raw
            .into_iter()
            .map(|r| match r {
                Some(raw) => serde_json::from_str(&raw)
                    .map_err(|_| storage("Stored historical version unreadable")),
                None if lookup.require_present => {
                    Err(missing("Historical source versions not imported"))
                }
                None => Ok(Value::Null),
            })
            .collect::<Result<_>>()?;
        for (id, row) in lookup.record_ids.iter().zip(&rows) {
            if !row.is_null() && identity(row)? != *id {
                return Err(storage("Historical source version key identity differs"));
            }
        }
        let present: Vec<_> = rows.iter().filter(|r| !r.is_null()).cloned().collect();
        if !present.is_empty() {
            self.prepare_native_historical_variants(company, &present)
                .await
                .map_err(|_| storage("Stored historical version source provenance differs"))?;
        }
        Ok(
            json!({"test_copy":true,"source_scope":company,"source_variant_profile_sha256":PROFILE,"record_ids":lookup.record_ids,"records":rows,"original_history_modified":0,"personal_source_flags_modified":0,"current_profiles_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false}),
        )
    }
    pub async fn test_copy_client_native_historical_variants(
        &self,
        company: &str,
        client: &str,
        page: SourceFactPage,
    ) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        if !number(client)
            || page.kind != "historical-variants"
            || page.limit == 0
            || page.limit > MAX_BATCH
            || page
                .source_return_year
                .is_some_and(|y| !(1990..=2100).contains(&y))
        {
            return Err(bad("Historical version client page bound or year differs"));
        }
        let mut ids = self
            .store
            .smembers(&member(company, client))
            .await
            .map_err(|e| storage(&e))?;
        if ids.len() > 1000
            || ids
                .iter()
                .any(|id| !valid_id(id) || id.split(':').next() != Some(client))
        {
            return Err(storage("Historical version client index differs"));
        }
        ids.retain(|id| {
            page.source_return_year
                .is_none_or(|year| id.split(':').nth(1) == Some(year.to_string().as_str()))
        });
        ids.sort();
        let total = ids.len();
        let ids: Vec<_> = ids.into_iter().skip(page.offset).take(page.limit).collect();
        let rows = if ids.is_empty() {
            vec![]
        } else {
            self.test_copy_read_native_historical_variants(
                company,
                HistoricalVariantLookup {
                    test_copy: true,
                    record_ids: ids,
                    require_present: true,
                },
            )
            .await?["records"]
                .as_array()
                .ok_or_else(|| storage("Historical version page response missing"))?
                .clone()
        };
        Ok(
            json!({"test_copy":true,"source_scope":company,"client_number":client,"kind":"historical-variants","source_variant_profile_sha256":PROFILE,"source_return_year":page.source_return_year,"total":total,"offset":page.offset,"records":rows,"original_history_modified":0,"personal_source_flags_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false}),
        )
    }
    pub async fn test_copy_native_historical_variant_summary(
        &self,
        company: &str,
    ) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let all_key = all(company);
        let digest_key = digests(company);
        let (ids, pins) = tokio::join!(
            self.store.smembers(&all_key),
            self.store.smembers(&digest_key)
        );
        let ids = ids.map_err(|e| storage(&e))?;
        let pins = pins.map_err(|e| storage(&e))?;
        if ids.len() > 32744 || ids.len() != pins.len() || ids.iter().any(|id| !valid_id(id)) {
            return Err(storage(
                "Historical version summary index incomplete; replay interrupted batch",
            ));
        }
        let mut counts = [0usize; 5];
        let mut empty = 0;
        for pin in pins {
            let c = profile()
                .record_summaries
                .get(&pin)
                .ok_or_else(|| storage("Historical version summary outside closed pins"))?;
            for i in 0..5 {
                counts[i] += c[i]
            }
            empty += usize::from(c[0] == 0)
        }
        Ok(
            json!({"test_copy":true,"source_scope":company,"source_variant_profile_sha256":PROFILE,"source_package_sha256":PACKAGE,"source_contexts":ids.len(),"contexts_without_actual_calc_variants":empty,"source_variants":counts[0],"E11_forms":counts[1],"present_field_occurrences":counts[2],"explicit_zero_occurrences":counts[3],"absent_form_line_slots":counts[4],"original_history_modified":0,"personal_source_flags_modified":0,"current_profiles_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::io::{BufRead, BufReader};
    #[test]
    fn closed_profile_pins_complete_contexts_and_all_presence_counts() {
        assert_eq!(
            hex::encode(Sha256::digest(include_bytes!(
                "native_historical_variant_profile.json"
            ))),
            CLOSED_SHA
        );
        let p = profile();
        assert_eq!(p.source_profile_sha256, PROFILE);
        assert_eq!(p.source_package_sha256, PACKAGE);
        assert_eq!(p.source_archive_sha256, ARCHIVE);
        assert_eq!(p.record_count, 32744);
        assert_eq!(p.record_summaries.len(), 32744);
        assert!(p.record_summaries.keys().all(|k| hash(k)));
        assert_eq!(
            hex::encode(Sha256::digest(canonical(&p.source_profile).unwrap())),
            PROFILE
        );
        let mut sums = [0; 5];
        for c in p.record_summaries.values() {
            for i in 0..5 {
                sums[i] += c[i]
            }
        }
        assert_eq!(sums, [26862, 21945, 101740, 19570, 29930]);
        assert_eq!(
            p.record_summaries.values().filter(|c| c[0] == 0).count(),
            13315
        );
    }
    #[test]
    fn refuses_unpinned_records_floats_and_unsupported_shapes() {
        assert!(pinned(&json!({"client_number":"1"})).is_err());
        assert!(canonical(&json!({"wire":1.2})).is_err());
        assert_eq!(decimal(-1), "-0.01");
        assert_eq!(decimal(0), "0.00");
        assert_eq!(decimal(i32::MIN as i64), "-21474836.48");
    }
    #[tokio::test]
    async fn batch_bounds_and_explicit_copy_scope_fail_closed() {
        let s = SourceImport::new(Store::memory());
        for (test_copy, records) in [
            (false, vec![]),
            (true, vec![]),
            (true, vec![Value::Null; 101]),
        ] {
            assert!(
                s.test_copy_native_historical_variants(
                    "variant-qa",
                    HistoricalVariantBatch {
                        test_copy,
                        source_variant_profile_sha256: PROFILE.into(),
                        records
                    }
                )
                .await
                .is_err()
            );
        }
        assert!(
            s.test_copy_read_native_historical_variants(
                "variant-qa",
                HistoricalVariantLookup {
                    test_copy: true,
                    record_ids: vec!["malformed".into()],
                    require_present: false
                }
            )
            .await
            .is_err()
        );
        assert_eq!(
            s.test_copy_native_historical_variant_summary("variant-qa")
                .await
                .unwrap()["source_contexts"],
            0
        );
    }
    #[tokio::test]
    async fn optional_all32744_actual_sources_import_replay_read_and_preserve_originals() {
        let Ok(input) = std::env::var("NXT_DTMAX_HISTORICAL_VARIANTS_INPUT") else {
            return;
        };
        let dependencies = std::env::var("NXT_DTMAX_VARIANT_DEPENDENCIES")
            .expect("private exact original history/personal fixtures required");
        let origin = std::env::var("NXT_DTMAX_NATIVE_MODULE_REVIEWS_INPUT")
            .expect("88 pinned native origin reviews required");
        let store = Store::memory();
        let service = SourceImport::new(store.clone());
        let company = "all-private-native-versions";
        let mut original_hash = Sha256::new();
        let mut ids = Vec::new();
        for line in BufReader::new(std::fs::File::open(dependencies).unwrap()).lines() {
            let v: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let h = &v["historical"];
            let p = &v["personal"];
            let id = format!(
                "{}:{}:{}",
                h["client_number"].as_str().unwrap(),
                h["tax_year"],
                h["archive_sha256"].as_str().unwrap()
            );
            let h = serde_json::to_string(h).unwrap();
            let p = serde_json::to_string(p).unwrap();
            original_hash.update(h.as_bytes());
            original_hash.update(p.as_bytes());
            store.set(&history(company, &id), &h).await.unwrap();
            store.set(&personal(company, &id), &p).await.unwrap();
            ids.push(id);
        }
        assert_eq!(ids.len(), 32744);
        let expected_original = original_hash.finalize();
        let origin: Vec<Value> = BufReader::new(std::fs::File::open(origin).unwrap())
            .lines()
            .map(|s| serde_json::from_str(&s.unwrap()).unwrap())
            .collect();
        assert_eq!(origin.len(), 88);
        let before = service
            .test_copy_native_historical_variant_summary(company)
            .await
            .unwrap();
        assert_eq!(before["source_contexts"], 0);
        service
            .test_copy_native_module_reviews(
                company,
                super::super::native_module_reviews::NativeModuleReviewBatch {
                    test_copy: true,
                    binding_profile_sha256: super::super::native_module_reviews::PROFILE.into(),
                    records: origin,
                },
            )
            .await
            .unwrap();
        store
            .set(
                &format!("dtmaxtc:{company}:client:baseline-sentinel"),
                "unchanged",
            )
            .await
            .unwrap();
        let mut digest = Sha256::new();
        let mut chunk = Vec::new();
        let mut total = 0;
        async fn check(service: &SourceImport, company: &str, rows: Vec<Value>) {
            let n = rows.len();
            let result = service
                .test_copy_native_historical_variants(
                    company,
                    HistoricalVariantBatch {
                        test_copy: true,
                        source_variant_profile_sha256: PROFILE.into(),
                        records: rows.clone(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(result["created"], n);
            let result = service
                .test_copy_native_historical_variants(
                    company,
                    HistoricalVariantBatch {
                        test_copy: true,
                        source_variant_profile_sha256: PROFILE.into(),
                        records: rows.clone(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(result["unchanged_replays"], n);
            let ids = rows.iter().map(identity).collect::<Result<_>>().unwrap();
            let result = service
                .test_copy_read_native_historical_variants(
                    company,
                    HistoricalVariantLookup {
                        test_copy: true,
                        record_ids: ids,
                        require_present: true,
                    },
                )
                .await
                .unwrap();
            assert!(
                result["records"].as_array().unwrap() == &rows,
                "private source readback differs"
            );
        }
        for line in BufReader::new(std::fs::File::open(input).unwrap()).lines() {
            let line = line.unwrap();
            digest.update(line.as_bytes());
            digest.update(b"\n");
            let row: Value = serde_json::from_str(&line).unwrap();
            total += 1;
            chunk.push(row);
            if chunk.len() == 100 {
                check(&service, company, std::mem::take(&mut chunk)).await
            }
        }
        if !chunk.is_empty() {
            check(&service, company, chunk).await
        }
        assert_eq!(total, 32744);
        assert_eq!(hex::encode(digest.finalize()), PACKAGE);
        let summary = service
            .test_copy_native_historical_variant_summary(company)
            .await
            .unwrap();
        for (key, n) in [
            ("source_contexts", 32744),
            ("contexts_without_actual_calc_variants", 13315),
            ("source_variants", 26862),
            ("E11_forms", 21945),
            ("present_field_occurrences", 101740),
            ("explicit_zero_occurrences", 19570),
            ("absent_form_line_slots", 29930),
        ] {
            assert_eq!(summary[key], n)
        }
        let mut after = Sha256::new();
        for id in &ids {
            after.update(
                store
                    .get(&history(company, id))
                    .await
                    .unwrap()
                    .unwrap()
                    .as_bytes(),
            );
            after.update(
                store
                    .get(&personal(company, id))
                    .await
                    .unwrap()
                    .unwrap()
                    .as_bytes(),
            )
        }
        assert!(
            after.finalize() == expected_original,
            "original private historical/personal bytes changed"
        );
        assert_eq!(
            store
                .get(&format!("dtmaxtc:{company}:client:baseline-sentinel"))
                .await
                .unwrap()
                .unwrap(),
            "unchanged"
        );
        let client = ids[0].split(':').next().unwrap();
        let page = service
            .test_copy_client_native_historical_variants(
                company,
                client,
                SourceFactPage {
                    kind: "historical-variants".into(),
                    offset: 0,
                    limit: 100,
                    source_return_year: None,
                    _routing_path: None,
                },
            )
            .await
            .unwrap();
        assert!(
            page["records"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["client_number"] == client)
        );
        assert!(page["total"].as_u64().unwrap() > 0);
    }
}
