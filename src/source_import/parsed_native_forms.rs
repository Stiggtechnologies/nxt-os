//! Immutable saved parsed-source contexts. No native executable, activation or calculation.
//! A closed Merkle root binds every original context line and compressed transport.
use super::preserved_records::SourceFactPage;
use super::{ImportError, SourceImport, bad, conflict, storage};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::OnceLock};

type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE: &str = "4cf13412be53c8419ccceef8b0cf600e99ea30529c8bef4c0b5d13fa6bb321af";
const ORIGINAL: &str = "499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c";
const RAW: &str = "10d5e77f536d2b94a28a22823076f76279148bc397962ad6b930be708d37b656";

fn hash(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(raw))
}
fn profile() -> &'static Value {
    static P: OnceLock<Value> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("parsed_native_form_profile.json");
        assert_eq!(hash(raw), PROFILE);
        serde_json::from_slice(raw).expect("closed parsed-source profile")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    #[test]
    fn closed_profile_is_client_free_and_preserves_source_holds() {
        let p = profile();
        assert_eq!(p["counts"]["contexts"], 32751);
        assert_eq!(p["counts"]["parsed_contexts"], 32744);
        assert_eq!(p["counts"]["parse_errors"], 7);
        assert_eq!(p["counts"]["data_origin_text_Q_fields"], 667917);
        assert_eq!(p["not_complete_all_plan_native_decoder"], true);
        assert_eq!(p["financial_values_approved"], false);
        assert!(p.get("entries").is_none());
    }
    #[test]
    fn merkle_path_binds_owner_ordinal_and_odd_nodes() {
        let a = json!({"client":"100","line":1});
        let b = json!({"client":"200","line":2});
        let c = json!({"client":"300","line":3});
        let leaf = |v: &Value| {
            hex::decode(hash(
                &[b"\x00".as_slice(), encode(v).unwrap().as_bytes()].concat(),
            ))
            .unwrap()
        };
        let node = |a: &[u8], b: &[u8]| hash(&[b"\x01".as_slice(), a, b].concat());
        let la = leaf(&a);
        let lb = leaf(&b);
        let lc = leaf(&c);
        let left = node(&la, &lb);
        let right = node(&lc, &lc);
        let root = node(&hex::decode(&left).unwrap(), &hex::decode(&right).unwrap());
        let proof = vec![
            MerkleSibling {
                side: "right".into(),
                sha256: hex::encode(&lc),
            },
            MerkleSibling {
                side: "left".into(),
                sha256: left,
            },
        ];
        assert_eq!(merkle(&c, &proof, 3, 3, &root).unwrap(), hex::encode(&lc));
        let mut wrong = c.clone();
        wrong["client"] = json!("999");
        assert!(merkle(&wrong, &proof, 3, 3, &root).is_err());
        assert!(merkle(&c, &proof, 2, 3, &root).is_err());
        let mut wrong = proof.clone();
        wrong[0].sha256 = "a".repeat(64);
        assert!(merkle(&c, &wrong, 3, 3, &root).is_err());
        assert!(merkle(&c, &proof[..1], 3, 3, &root).is_err());
    }
    #[tokio::test]
    async fn wrong_scope_profile_and_unclosed_records_never_create_source_rows() {
        let s = SourceImport::new(Store::memory());
        for (flag, profile) in [(false, PROFILE), (true, "unreviewed")] {
            assert!(
                s.test_copy_parsed_native_forms(
                    "parsed-source-qa",
                    ParsedNativeBatch {
                        test_copy: flag,
                        profile_sha256: profile.into(),
                        records: vec![]
                    }
                )
                .await
                .is_err()
            );
        }
        assert_eq!(
            s.test_copy_parsed_native_form_summary("parsed-source-qa")
                .await
                .unwrap()["source_context_records_received"],
            0
        );
    }
    // No private data is committed. With NXT_PARSED_SOURCE_MANIFEST set, this
    // executes every actual source record; the local proof requires its marker.
    #[tokio::test]
    async fn private_actual_context_replay_is_lossless_immutable_and_preserves_original_rows() {
        let Ok(path) = std::env::var("NXT_PARSED_SOURCE_MANIFEST") else {
            return;
        };
        let manifest_raw = std::fs::read(&path).unwrap();
        assert_eq!(
            hash(&manifest_raw),
            "b87580b785c2789beb0e5f037649e1f2cf24b5309ed134fe8ac76bcb4c9dcc49"
        );
        let manifest: Value = serde_json::from_slice(&manifest_raw).unwrap();
        let directory = std::path::Path::new(&path).parent().unwrap();
        let store = Store::memory();
        let s = SourceImport::new(store.clone());
        let company = "parsed-source-actual-private-qa";
        let profiles: Value = serde_json::from_slice(
            &std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap(),
        )
        .unwrap();
        let mut before = Vec::new();
        for row in profiles["clients"].as_array().unwrap() {
            let client = row["client_number"].as_str().unwrap();
            let key = format!("dtmaxtc:{company}:client:{client}");
            let raw = row.to_string();
            store.set(&key, &raw).await.unwrap();
            store
                .sadd(&format!("dtmaxtc:{company}:clients"), client)
                .await
                .unwrap();
            before.push((key, raw));
        }
        let native = std::fs::read(std::env::var("NXT_PRESERVED_NATIVE_FIXTURE").unwrap()).unwrap();
        let histories: Vec<Value> = native
            .split(|b| *b == b'\n')
            .filter(|r| !r.is_empty())
            .map(|r| serde_json::from_slice(r).unwrap())
            .collect();
        assert_eq!(histories.len(), 32744);
        for row in &histories {
            let id = format!(
                "{}:{}:{}",
                row["client_number"].as_str().unwrap(),
                row["tax_year"].as_u64().unwrap(),
                row["archive_sha256"].as_str().unwrap()
            );
            let key = format!("dtmaxtc:{company}:historical_lines:{id}");
            let raw = row.to_string();
            store.set(&key, &raw).await.unwrap();
            before.push((key, raw));
        }
        let entries = manifest["entries"].as_array().unwrap();
        let mut total = 0;
        let mut first = None;
        let mut pending = Vec::new();
        let mut bytes = 0;
        for entry in entries {
            let id = entry["context_id"].as_str().unwrap();
            let raw = std::fs::read(directory.join("contexts").join(format!("{id}.json"))).unwrap();
            assert_eq!(hash(&raw), entry["packet_sha256"].as_str().unwrap());
            let mut value: Value = serde_json::from_slice(&raw).unwrap();
            value["merkle_proof"] = entry["merkle_proof"].clone();
            let record: ParsedNativeContext = serde_json::from_value(value).unwrap();
            assert_eq!(validate(&record).unwrap(), id);
            if first.is_none() {
                let mut invalid = record.clone();
                invalid.metadata["client_number"] = json!("999999999");
                assert!(
                    s.test_copy_parsed_native_forms(
                        company,
                        ParsedNativeBatch {
                            test_copy: true,
                            profile_sha256: PROFILE.into(),
                            records: vec![record.clone(), invalid]
                        }
                    )
                    .await
                    .is_err()
                );
                assert_eq!(
                    s.test_copy_parsed_native_form_summary(company)
                        .await
                        .unwrap()["source_context_records_received"],
                    0
                );
                let mut invalid = record.clone();
                invalid.content_base64 = "AAAA".into();
                assert!(validate(&invalid).is_err());
                first = Some(record.clone());
            }
            let size = encode(&record).unwrap().len() + 1;
            if !pending.is_empty() && (pending.len() == 50 || bytes + size > 1_490_000) {
                let r = s
                    .test_copy_parsed_native_forms(
                        company,
                        ParsedNativeBatch {
                            test_copy: true,
                            profile_sha256: PROFILE.into(),
                            records: std::mem::take(&mut pending),
                        },
                    )
                    .await
                    .unwrap();
                total += r["created"].as_u64().unwrap();
                bytes = 0;
            }
            bytes += size;
            pending.push(record);
        }
        if !pending.is_empty() {
            total += s
                .test_copy_parsed_native_forms(
                    company,
                    ParsedNativeBatch {
                        test_copy: true,
                        profile_sha256: PROFILE.into(),
                        records: pending,
                    },
                )
                .await
                .unwrap()["created"]
                .as_u64()
                .unwrap();
        }
        assert_eq!(total, 32751);
        let first = first.unwrap();
        let id = validate(&first).unwrap();
        let replay = s
            .test_copy_parsed_native_forms(
                company,
                ParsedNativeBatch {
                    test_copy: true,
                    profile_sha256: PROFILE.into(),
                    records: vec![first.clone()],
                },
            )
            .await
            .unwrap();
        assert_eq!(replay["unchanged_replays"], 1);
        assert_eq!(replay["created"], 0);
        let found = s
            .test_copy_read_parsed_native_forms(
                company,
                ParsedNativeLookup {
                    test_copy: true,
                    profile_sha256: PROFILE.into(),
                    record_ids: vec![id.clone()],
                },
            )
            .await
            .unwrap();
        assert_eq!(found["read_only"], true);
        assert_eq!(serde_json::to_value(&first).unwrap(), found["records"][0]);
        assert!(
            s.test_copy_read_parsed_native_forms(
                company,
                ParsedNativeLookup {
                    test_copy: true,
                    profile_sha256: PROFILE.into(),
                    record_ids: vec!["a".repeat(64)]
                }
            )
            .await
            .is_err()
        );
        let m = &first.metadata;
        let p:SourceFactPage=serde_json::from_value(json!({"kind":"parsed-native-forms","offset":0,"limit":25,"source_return_year":m["source_return_year"]})).unwrap();
        let page = s
            .test_copy_client_source_facts(company, m["client_number"].as_str().unwrap(), p)
            .await
            .unwrap();
        assert!(page["records"].as_array().unwrap().iter().any(|r| {
            serde_json::from_value::<ParsedNativeContext>(r.clone())
                .is_ok_and(|r| validate(&r).unwrap() == id)
        }));
        assert_eq!(
            s.test_copy_parsed_native_form_summary(company)
                .await
                .unwrap()["closed_source_inventory_complete"],
            true
        );
        for chunk in before.chunks(250) {
            let keys: Vec<_> = chunk.iter().map(|(k, _)| k.clone()).collect();
            let rows = store.mget(&keys).await.unwrap();
            for (row, (_, expected)) in rows.iter().zip(chunk) {
                assert_eq!(row.as_deref(), Some(expected.as_str()));
            }
        }
        println!(
            "PARSED_SOURCE_EXACT: contexts=32751; parsed=32744; errors=7; data_forms=165982; text_fields=667917; raw_Q=18423580; profiles_and_history_unchanged=true; vendor_activation=false"
        );
    }
}
fn encode(v: &impl Serialize) -> Result<String> {
    serde_json::to_string(v).map_err(|_| bad("Cannot encode parsed source evidence"))
}
fn number(v: &str) -> bool {
    !v.is_empty() && v.len() <= 12 && v.bytes().all(|b| b.is_ascii_digit())
}
fn key(company: &str, suffix: &str) -> String {
    format!("dtmaxtc:{company}:parsed_native_forms:{PROFILE}:{suffix}")
}
fn record_key(company: &str, id: &str) -> String {
    key(company, &format!("record:{id}"))
}
fn envelope(company: &str) -> Value {
    json!({"test_copy":true,"source_scope":company,"parsed_source_profile_sha256":PROFILE,
        "source_file_sha256":RAW,"vendor_activation_required":false,"native_contexts_created":0,
        "current_profiles_modified":0,"earlier_figures_or_slips_modified":0,"tax_cases_imported":0,
        "financial_values_approved":false,"full_all_plan_native_decode_claimed":false})
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerkleSibling {
    pub side: String,
    pub sha256: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedNativeContext {
    pub metadata: Value,
    pub content_base64: String,
    pub merkle_proof: Vec<MerkleSibling>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedNativeBatch {
    pub test_copy: bool,
    pub profile_sha256: String,
    pub records: Vec<ParsedNativeContext>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedNativeLookup {
    pub test_copy: bool,
    pub profile_sha256: String,
    pub record_ids: Vec<String>,
}

fn merkle(
    metadata: &Value,
    proof: &[MerkleSibling],
    line: usize,
    count: usize,
    root: &str,
) -> Result<String> {
    let raw = encode(metadata)?;
    if raw.len() > 4096 || line == 0 || line > count {
        return Err(bad("Parsed source metadata exceeds closed scope"));
    }
    let mut value = Sha256::digest([b"\x00".as_slice(), raw.as_bytes()].concat()).to_vec();
    let identity = hex::encode(&value);
    let (mut position, mut width, mut used) = (line - 1, count, 0);
    for sibling in proof {
        if width <= 1
            || sibling.sha256.len() != 64
            || sibling.side != if position % 2 == 1 { "left" } else { "right" }
        {
            return Err(bad("Parsed source proof path differs"));
        }
        let other =
            hex::decode(&sibling.sha256).map_err(|_| bad("Invalid parsed source proof digest"))?;
        if position % 2 == 0 && position + 1 >= width && other != value {
            return Err(bad("Invalid odd source proof node"));
        }
        let pair = if position % 2 == 1 {
            [other, value].concat()
        } else {
            [value, other].concat()
        };
        value = Sha256::digest([b"\x01".as_slice(), &pair].concat()).to_vec();
        position /= 2;
        width = width.div_ceil(2);
        used += 1;
    }
    if width != 1 || used == 0 || hex::encode(value) != root {
        return Err(bad("Parsed source does not match the closed evidence root"));
    }
    Ok(identity)
}

fn validate(record: &ParsedNativeContext) -> Result<String> {
    let m = &record.metadata;
    let p = profile();
    let client = m["client_number"]
        .as_str()
        .ok_or_else(|| bad("Parsed source owner missing"))?;
    let family = m["family_file_id"]
        .as_str()
        .ok_or_else(|| bad("Parsed source family missing"))?;
    let year = m["source_return_year"]
        .as_u64()
        .ok_or_else(|| bad("Parsed source year missing"))?;
    let line = m["source_line_number"]
        .as_u64()
        .ok_or_else(|| bad("Parsed source ordinal missing"))? as usize;
    if !number(client)
        || !number(family)
        || !(1990..=2100).contains(&year)
        || m["source_file_sha256"] != RAW
        || m["original_extract_sha256"] != ORIGINAL
        || record.merkle_proof.len() != 15
        || m["compression"] != "zlib"
        || m["source_line_bytes"]
            .as_u64()
            .is_none_or(|v| v == 0 || v > 1_313_305)
        || record.content_base64.len() > 150_340
    {
        return Err(bad("Parsed source context exceeds closed source bounds"));
    }
    let id = merkle(
        m,
        &record.merkle_proof,
        line,
        32_751,
        p["context_merkle_root"].as_str().expect("profile root"),
    )?;
    let body = base64::engine::general_purpose::STANDARD
        .decode(&record.content_base64)
        .map_err(|_| bad("Invalid parsed source transport"))?;
    if body.len() > 112_753
        || m["compressed_bytes"] != body.len()
        || m["compressed_sha256"] != hash(&body)
        || base64::engine::general_purpose::STANDARD.encode(&body) != record.content_base64
    {
        return Err(bad("Compressed parsed source bytes differ"));
    }
    Ok(id)
}

impl SourceImport {
    async fn parsed_source_dependencies(
        &self,
        company: &str,
        records: &[ParsedNativeContext],
    ) -> Result<()> {
        let keys: Vec<_> = records
            .iter()
            .map(|r| {
                format!(
                    "dtmaxtc:{company}:client:{}",
                    r.metadata["client_number"].as_str().unwrap()
                )
            })
            .collect();
        let clients = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if clients.len() != records.len() || clients.iter().any(Option::is_none) {
            return Err(bad("Parsed source requires the registered original client"));
        }
        let active: Vec<_> = records
            .iter()
            .filter(|r| r.metadata["parse_error_type"].is_null())
            .collect();
        let keys: Vec<_> = active
            .iter()
            .map(|r| {
                format!(
                    "dtmaxtc:{company}:historical_lines:{}:{}:{ORIGINAL}",
                    r.metadata["client_number"].as_str().unwrap(),
                    r.metadata["source_return_year"].as_u64().unwrap()
                )
            })
            .collect();
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != active.len() {
            return Err(storage("Parsed source dependency count differs"));
        }
        for (r, raw) in active.iter().zip(rows) {
            let v: Value =
                serde_json::from_str(&raw.ok_or_else(|| {
                    bad("Parsed source requires its existing historical context")
                })?)
                .map_err(|_| storage("Original historical context unreadable"))?;
            let m = &r.metadata;
            if v["client_number"] != m["client_number"]
                || v["family_file_id"] != m["family_file_id"]
                || v["tax_year"] != m["source_return_year"]
                || v["archive_sha256"] != ORIGINAL
                || v["vendor_module"] != m["reader_module"]
            {
                return Err(bad(
                    "Parsed source client, family, year or reader module differs",
                ));
            }
        }
        Ok(())
    }

    pub async fn test_copy_parsed_native_forms(
        &self,
        company: &str,
        batch: ParsedNativeBatch,
    ) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.profile_sha256 != PROFILE
            || batch.records.is_empty()
            || batch.records.len() > 50
            || encode(&batch.records)?.len() > 1_500_000
        {
            return Err(bad("Parsed source batch/profile exceeds bound"));
        }
        // Validate the entire batch, original dependencies, and immutable conflicts before writes.
        let mut seen = BTreeSet::new();
        let mut validated = Vec::new();
        for record in &batch.records {
            let id = validate(record)?;
            if !seen.insert(id.clone()) {
                return Err(bad("Duplicate parsed source in batch"));
            }
            validated.push((id, encode(record)?));
        }
        self.parsed_source_dependencies(company, &batch.records)
            .await?;
        let keys: Vec<_> = validated
            .iter()
            .map(|(id, _)| record_key(company, id))
            .collect();
        let old = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if old.len() != validated.len() {
            return Err(storage("Parsed source immutable preflight count differs"));
        }
        for (row, (_, raw)) in old.iter().zip(&validated) {
            if row.as_ref().is_some_and(|old| old != raw) {
                return Err(conflict("Parsed native source is immutable"));
            }
        }
        let mut created = 0;
        for (record, (id, raw)) in batch.records.iter().zip(validated) {
            let added = self
                .store
                .set_nx(&record_key(company, &id), &raw)
                .await
                .map_err(|e| storage(&e))?;
            if self
                .store
                .get(&record_key(company, &id))
                .await
                .map_err(|e| storage(&e))?
                .as_deref()
                != Some(&raw)
            {
                return Err(conflict("Parsed native source is immutable"));
            }
            let m = &record.metadata;
            let client = m["client_number"].as_str().unwrap();
            let year = m["source_return_year"].as_u64().unwrap();
            for suffix in [
                "all".to_owned(),
                format!("client:{client}"),
                format!("client_year:{client}:{year}"),
            ] {
                self.store
                    .sadd(&key(company, &suffix), &id)
                    .await
                    .map_err(|e| storage(&e))?;
            }
            if added {
                created += 1;
            }
        }
        let mut response = envelope(company);
        response["created"] = json!(created);
        response["unchanged_replays"] = json!(batch.records.len() - created);
        Ok(response)
    }

    pub async fn test_copy_parsed_native_form_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let mut ids = self
            .store
            .smembers(&key(company, "all"))
            .await
            .map_err(|e| storage(&e))?;
        if ids.len() > 32_751 {
            return Err(storage("Parsed source inventory exceeds closed profile"));
        }
        ids.sort();
        let mut response = envelope(company);
        response["source_context_records_received"] = json!(ids.len());
        response["closed_source_inventory_complete"] = json!(ids.len() == 32_751);
        response["context_id_inventory_sha256"] = json!(hash(ids.join("\n").as_bytes()));
        response["expected_source_counts"] = profile()["counts"].clone();
        response["received_source_counts"] = if ids.len() == 32_751 {
            profile()["counts"].clone()
        } else {
            Value::Null
        };
        Ok(response)
    }

    /// Bounded exact readback, avoiding one request per historical context.
    pub async fn test_copy_read_parsed_native_forms(
        &self,
        company: &str,
        lookup: ParsedNativeLookup,
    ) -> Result<Value> {
        self.test_copy_guard(company, lookup.test_copy)?;
        if lookup.profile_sha256 != PROFILE
            || lookup.record_ids.is_empty()
            || lookup.record_ids.len() > 50
            || lookup.record_ids.iter().any(|id| {
                id.len() != 64
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            || lookup.record_ids.iter().collect::<BTreeSet<_>>().len() != lookup.record_ids.len()
        {
            return Err(bad("Invalid parsed source exact lookup"));
        }
        let keys: Vec<_> = lookup
            .record_ids
            .iter()
            .map(|id| record_key(company, id))
            .collect();
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != lookup.record_ids.len() {
            return Err(storage("Parsed source lookup count differs"));
        }
        let mut records = Vec::new();
        let mut bytes = 0;
        for (id, raw) in lookup.record_ids.iter().zip(rows) {
            let raw = raw.ok_or_else(|| bad("Parsed source lookup record is absent"))?;
            bytes += raw.len();
            if bytes > 1_600_000 {
                return Err(bad("Parsed source exact read exceeds response bound"));
            }
            let r: ParsedNativeContext = serde_json::from_str(&raw)
                .map_err(|_| storage("Parsed source lookup record unreadable"))?;
            if validate(&r).map_err(|_| storage("Stored parsed source lookup differs"))? != *id {
                return Err(storage("Parsed source lookup identity differs"));
            }
            records.push(r);
        }
        let mut response = envelope(company);
        response["record_ids"] = json!(lookup.record_ids);
        response["records"] = json!(records);
        response["read_only"] = json!(true);
        response["source_records_modified"] = json!(0);
        Ok(response)
    }

    pub(super) async fn test_copy_client_parsed_native_forms(
        &self,
        company: &str,
        client: &str,
        page: SourceFactPage,
    ) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        if !number(client)
            || page.limit == 0
            || page.limit > 25
            || page
                .source_return_year
                .is_some_and(|y| !(1990..=2100).contains(&y))
        {
            return Err(bad("Invalid parsed source owner/year page"));
        }
        let suffix = if let Some(year) = page.source_return_year {
            format!("client_year:{client}:{year}")
        } else {
            format!("client:{client}")
        };
        let mut ids = self
            .store
            .smembers(&key(company, &suffix))
            .await
            .map_err(|e| storage(&e))?;
        if ids.len() > 100 {
            return Err(storage("Parsed source client index exceeds bound"));
        }
        ids.sort();
        let selected: Vec<_> = ids.iter().skip(page.offset).take(page.limit).collect();
        let keys: Vec<_> = selected.iter().map(|id| record_key(company, id)).collect();
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != selected.len() {
            return Err(storage("Parsed source page count differs"));
        }
        let mut records = Vec::new();
        for (id, raw) in selected.iter().zip(rows) {
            let r: ParsedNativeContext =
                serde_json::from_str(&raw.ok_or_else(|| storage("Parsed source record missing"))?)
                    .map_err(|_| storage("Parsed source unreadable"))?;
            if validate(&r).map_err(|_| storage("Stored parsed source proof differs"))?
                != id.as_str()
                || r.metadata["client_number"] != client
                || page
                    .source_return_year
                    .is_some_and(|y| r.metadata["source_return_year"] != y)
            {
                return Err(storage("Stored parsed source owner/year differs"));
            }
            records.push(r);
        }
        let mut response = envelope(company);
        response["client_number"] = json!(client);
        response["kind"] = json!("parsed-native-forms");
        response["source_return_year"] = json!(page.source_return_year);
        response["total"] = json!(ids.len());
        response["offset"] = json!(page.offset);
        response["limit"] = json!(page.limit);
        response["records"] = json!(records);
        Ok(response)
    }
}
