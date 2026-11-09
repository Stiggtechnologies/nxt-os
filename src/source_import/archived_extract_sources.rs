//! Closed archived EXR source copies. Current/preferred source rows stay immutable.
use super::preserved_records::SourceFactPage;
use super::{ImportError, SourceImport, bad, conflict, storage};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    sync::OnceLock,
};
type Result<T> = std::result::Result<T, ImportError>;
const FILE_SHA: &str = "aac034f9cf91b2c58df9284cfb74079a9f0e258b2162a27e94d9b947c6352e03";
const PROFILE: &str = "0bee0a36b1f3a3770c28cd253ce686ec3c33e3a6790cb8a093c857679829adb0";
const ARCHIVE: &str = "43b6d06e2719d0842bf72ca082f2efd1b9e97fd981548211e2586dc10e2156a8";
fn sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}
fn digest(v: &Value) -> String {
    sha(&serde_json::to_vec(v).unwrap())
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str()
        .ok_or_else(|| bad("Archived source string missing"))
}
fn uint(v: &Value, k: &str) -> Result<u64> {
    v[k].as_u64()
        .ok_or_else(|| bad("Archived source integer missing"))
}
fn hash(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn number(v: &str) -> bool {
    !v.is_empty() && v.len() <= 12 && v.bytes().all(|b| b.is_ascii_digit())
}
fn profile() -> &'static Value {
    static P: OnceLock<Value> = OnceLock::new();
    P.get_or_init(|| {
        let b = include_bytes!("archived_extract_profile.json");
        assert_eq!(sha(b), FILE_SHA);
        let p: Value = serde_json::from_slice(b).unwrap();
        assert_eq!(digest(&p["contract"]), PROFILE);
        assert_eq!(p["profile_sha256"], PROFILE);
        assert_eq!(
            sha(include_bytes!("archived_extract_source_bound_profile.json")),
            "99a59e77e8f9f678473c437dd386a08f5d931417e43ce8200919162022594161"
        );
        p
    })
}
fn allowed(k: &str, h: &str) -> bool {
    profile()["contract"][k]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == h)
}
fn key(c: &str, k: &str) -> String {
    format!("dtmaxtc:{c}:archived_extract_sources:{PROFILE}:{k}")
}
fn base(c: &str, k: &str) -> String {
    format!("dtmaxtc:{c}:{k}")
}
fn id(r: &Value) -> String {
    digest(&r["source_locator"])
}
fn envelope(c: &str) -> Value {
    json!({"test_copy":true,"source_scope":c,"kind":"archived-extract-sources","archived_source_profile_sha256":PROFILE,"archived_source_profile_file_sha256":FILE_SHA,"source_archive_sha256":ARCHIVE,"source_only":true,"vendor_activation_required":false,"current_profiles_modified":0,"original_or_preferred_rows_modified":0,"native_contexts_created":0,"tax_cases_imported":0,"financial_values_approved":false,"preferred_version_selected":false})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedBatch {
    pub test_copy: bool,
    pub profile_sha256: String,
    pub records: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedLookup {
    pub test_copy: bool,
    pub profile_sha256: String,
    pub context_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedPage {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "one")]
    pub limit: usize,
    pub source_return_year: Option<u16>,
    #[serde(default, rename = "path")]
    pub _routing_path: Option<String>,
}
fn one() -> usize {
    1
}
fn unpack(r: &Value) -> Result<Value> {
    if r["schema"] != "archived-DTMax-extract-source-packet/v1"
        || !allowed("packet_sha256", &digest(r))
        || r["source_payload_encoding"] != "canonical-json-gzip-base64"
        || r["financial_approval"] != false
        || r["preferred_version"] != false
        || r["legal_identity_verified"] != false
        || r["native_current_context_created"] != false
    {
        return Err(bad("Archived packet exceeds closed source-only scope"));
    }
    let locator = &r["source_locator"];
    let year = uint(locator, "source_return_year")?;
    if locator["source_archive_sha256"] != ARCHIVE
        || ![2021, 2022].contains(&year)
        || !hash(text(locator, "source_EXC_sha256")?)
        || uint(locator, "source_context_offset")? < 56
    {
        return Err(bad("Archived source locator differs"));
    }
    let raw = STANDARD
        .decode(text(r, "source_payload_gzip_base64")?)
        .map_err(|_| bad("Archived gzip base64 differs"))?;
    if raw.len() > 44487
        || raw.len() as u64 != uint(r, "source_payload_gzip_bytes")?
        || sha(&raw) != text(r, "source_payload_gzip_sha256")?
        || STANDARD.encode(&raw) != text(r, "source_payload_gzip_base64")?
    {
        return Err(bad("Archived compressed bytes differ"));
    }
    let mut decoder = flate2::bufread::GzDecoder::new(Cursor::new(raw.as_slice()));
    let mut body = vec![];
    decoder
        .by_ref()
        .take(333676)
        .read_to_end(&mut body)
        .map_err(|_| bad("Archived gzip integrity failed"))?;
    if body.len() > 333675
        || body.len() as u64 != uint(r, "source_payload_bytes")?
        || sha(&body) != text(r, "source_payload_sha256")?
        || decoder.into_inner().position() != raw.len() as u64
    {
        return Err(bad("Archived expanded payload bound/hash/trailer differs"));
    }
    let p: Value =
        serde_json::from_slice(&body).map_err(|_| bad("Archived source payload JSON differs"))?;
    if p["source_member_sha256"] != ARCHIVE
        || p["source_year"] != year
        || p["source_context_offset"] != locator["source_context_offset"]
        || p["source_EXC_sha256"] != locator["source_EXC_sha256"]
        || p["client_number"] != locator["source_header_client_reference"]
        || p["family_file_id"] != locator["source_header_family_reference"]
        || p["source_file_id"] != locator["source_header_file_reference"]
        || p["native_T1_origin_guard_verified"] != true
        || p["financial_approval"] != false
        || p["preferred_version"] != false
        || p["source_context_is_not_current_profile"] != true
    {
        return Err(bad("Archived payload/header/year/source reference differs"));
    }
    let records = p["records"]
        .as_array()
        .ok_or_else(|| bad("Archived record list absent"))?;
    if records.is_empty() || records.len() > 100 {
        return Err(bad("Archived record count outside bound"));
    }
    let mut previous = uint(&p, "source_context_offset")?;
    let end = uint(&p, "source_record_scope_end_exclusive")?;
    for rr in records {
        let off = uint(rr, "frame_offset")?;
        let len = uint(rr, "frame_bytes")?;
        let typ = uint(rr, "record_type")?;
        if off <= previous
            || off.checked_add(len).is_none_or(|n| n > end)
            || uint(rr, "outer_plan")? != 0
            || rr["strict_endmarker_consumed_exact_length_CRC_verified"] != true
            || !hash(text(rr, "frame_sha256")?)
        {
            return Err(bad("Archived source frame occurrence/boundary differs"));
        }
        previous = off;
        let expanded = STANDARD
            .decode(text(rr, "expanded_record_base64")?)
            .map_err(|_| bad("Archived serialized bytes differ"))?;
        if expanded.len() > 263140
            || expanded.len() as u64 != uint(rr, "expanded_record_bytes")?
            || sha(&expanded) != text(rr, "expanded_record_sha256")?
            || format!("{:08x}", crc32fast::hash(&expanded)) != text(rr, "CRC32")?
        {
            return Err(bad(
                "Archived complete serialized record length/hash/CRC differs",
            ));
        }
        let data = if expanded.is_empty() {
            if typ != 19
                || rr["decoded_source_state"] != "verified_empty_type19_no_serialized_header"
                || !rr["native_serialized_header_base64"].is_null()
            {
                return Err(bad("Unqualified empty archived serialized record"));
            }
            &expanded[..]
        } else {
            if expanded.len() < 18 {
                return Err(bad("Archived serialized header too short"));
            }
            let h = STANDARD
                .decode(text(rr, "native_serialized_header_base64")?)
                .map_err(|_| bad("Archived native header base64 differs"))?;
            if h.as_slice() != &expanded[..18]
                || u16::from_le_bytes([h[0], h[1]]) as u64 != typ
                || (u16::from_le_bytes([h[2], h[3]]) as usize | ((h[7] as usize) << 16))
                    != expanded.len() - 18
                || rr["native_serialized_header_type_and_length_verified"] != true
            {
                return Err(bad("Archived serialized header type/BODY length differs"));
            }
            &expanded[18..]
        };
        if data.len() as u64 != uint(rr, "expanded_body_bytes")?
            || sha(data) != text(rr, "expanded_body_sha256")?
        {
            return Err(bad("Archived original BODY bytes differ"));
        }
    }
    match r["assigned_SOURCE_client_number"].as_str() {
        Some(n)
            if number(n)
                && r["assigned_SOURCE_profile_sha256"]
                    .as_str()
                    .is_some_and(hash) => {}
        None if r["assigned_SOURCE_client_number"].is_null()
            && r["assigned_SOURCE_profile_sha256"].is_null() => {}
        _ => return Err(bad("Archived nullable SOURCE target differs")),
    }
    Ok(p)
}
fn meta(r: &Value, p: &Value) -> Value {
    json!({"context_id":id(r),"packet_sha256":digest(r),"source_locator":r["source_locator"],"assigned_SOURCE_client_number":r["assigned_SOURCE_client_number"],"assigned_SOURCE_profile_sha256":r["assigned_SOURCE_profile_sha256"],"assignment_state":r["assignment_state"],"SOURCE_assignment_evidence_sha256":r["SOURCE_assignment_evidence_sha256"],"records":p["records"].as_array().unwrap().len(),"calculation_source_variants":p["calculation_source_variants"].as_array().unwrap().len(),"held_calculation_source_variants":p["calculation_source_variants"].as_array().unwrap().iter().filter(|v|v["semantic_projection_eligible"]!=true).count(),"input_form_projections":p["input_form_projections"].as_array().unwrap().len()})
}
fn check_meta(m: &Value) -> Result<()> {
    if !allowed("context_metadata_sha256", &digest(m))
        || !hash(text(m, "context_id")?)
        || !allowed("packet_sha256", text(m, "packet_sha256")?)
    {
        return Err(storage(
            "Archived metadata index differs from closed profile",
        ));
    }
    Ok(())
}
impl SourceImport {
    fn archived_guard(&self, c: &str, flag: bool, p: &str) -> Result<()> {
        self.test_copy_guard(c, flag)?;
        if c != "dtmax-test-copy" || p != PROFILE {
            return Err(conflict(
                "Archived source profile is closed to exact SOURCE scope",
            ));
        }
        Ok(())
    }
    async fn archived_anchors(&self, c: &str, rows: &[(Value, Value)]) -> Result<()> {
        let numbers: BTreeSet<_> = rows
            .iter()
            .filter_map(|(r, _)| r["assigned_SOURCE_client_number"].as_str())
            .collect();
        if numbers.is_empty() {
            return Ok(());
        }
        let registered = self
            .store
            .smembers(&base(c, "clients"))
            .await
            .map_err(|e| storage(&e))?;
        if registered.len() != 5856
            || numbers
                .iter()
                .any(|n| !registered.iter().any(|v| v.as_str() == *n))
        {
            return Err(conflict(
                "Archived SOURCE target absent from pinned5856 roster",
            ));
        }
        let mut years = BTreeMap::<String, BTreeSet<u16>>::new();
        for id in self
            .store
            .smembers(&base(c, "returns"))
            .await
            .map_err(|e| storage(&e))?
        {
            if let Some((n, y)) = id.split_once(':') {
                if let Ok(y) = y.parse() {
                    years.entry(n.into()).or_default().insert(y);
                }
            }
        }
        let keys: Vec<_> = numbers
            .iter()
            .map(|n| base(c, &format!("client:{n}")))
            .collect();
        let targets = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if targets.len() != numbers.len() {
            return Err(storage("Archived target profile cardinality differs"));
        }
        let mut profiles = BTreeMap::new();
        for (n, raw) in numbers.into_iter().zip(targets) {
            let value: Value = serde_json::from_str(
                &raw.ok_or_else(|| conflict("Archived target profile missing"))?,
            )
            .map_err(|_| storage("Archived target profile unreadable"))?;
            profiles.insert(n, value);
        }
        for (r, p) in rows {
            let Some(n) = r["assigned_SOURCE_client_number"].as_str() else {
                continue;
            };
            let mut target = profiles[n].clone();
            if digest(&target) != r["assigned_SOURCE_profile_sha256"] {
                target["return_years"] = json!(years.get(n).cloned().unwrap_or_default());
            }
            if target["client_number"] != n
                || digest(&target) != r["assigned_SOURCE_profile_sha256"]
            {
                return Err(conflict(
                    "Archived target no longer matches exact F3 profile",
                ));
            }
            let e = &r["SOURCE_assignment_evidence"];
            if digest(e) != r["SOURCE_assignment_evidence_sha256"]
                || e["target_source_client_number"] != n
                || e["target_source_profile_sha256"] != r["assigned_SOURCE_profile_sha256"]
                || e["same_year_family_unique_own_stable_pair_full_literal_names_verified"] != true
                || e["exact_current_F3_full_name_count"] != 1
                || e["required_current_F3_list_sha256"]
                    != profile()["contract"]["required_F3_canonical_sha256"]
                || sha(target["name"].as_str().unwrap_or("").as_bytes())
                    != e["target_source_full_name_UTF8_sha256"]
            {
                return Err(bad("Archived SOURCE binding evidence differs"));
            }
            let info = p["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["record_type"] == 1)
                .ok_or_else(|| bad("Archived own Info1 absent"))?;
            let exp = STANDARD
                .decode(text(info, "expanded_record_base64")?)
                .map_err(|_| bad("Archived Info1 differs"))?;
            if exp.len() < 130 {
                return Err(bad("Archived own Info1 too short"));
            }
            let b = &exp[18..];
            let field = |a: usize, z: usize| {
                b[a..z]
                    .iter()
                    .map(|b| char::from(*b))
                    .collect::<String>()
                    .trim_end_matches(' ')
                    .to_owned()
            };
            let sin: String = field(82, 93).chars().filter(char::is_ascii_digit).collect();
            let dob = chrono::NaiveDate::parse_from_str(&field(94, 104), "%d-%m-%Y")
                .map_err(|_| bad("Archived own DOB invalid"))?
                .format("%Y-%m-%d")
                .to_string();
            let name = format!("{}, {}", field(8, 38), field(38, 68));
            if sin.len() != 9
                || ["000000000", "999999999"].contains(&sin.as_str())
                || sha(sin.as_bytes()) != e["source_Info1_nonplaceholder_SIN_sha256"]
                || sha(dob.as_bytes()) != e["source_Info1_valid_ISO_DOB_sha256"]
            {
                return Err(bad("Archived own strong Info1 fields differ"));
            }
            if e["exact_sameyear_source_full_name_equals_current_F3"] == true {
                if sha(name.as_bytes()) != e["target_source_full_name_UTF8_sha256"] {
                    return Err(bad("Archived exact SOURCE name differs"));
                }
            } else if e["crossyear_current_name_witnesses"]
                .as_array()
                .is_none_or(|a| a.is_empty())
            {
                return Err(bad("Archived crossyear F3 name witness absent"));
            }
        }
        Ok(())
    }
    pub async fn test_copy_archived_extract_sources(
        &self,
        c: &str,
        b: ArchivedBatch,
    ) -> Result<Value> {
        self.archived_guard(c, b.test_copy, &b.profile_sha256)?;
        if b.records.is_empty()
            || b.records.len() > 16
            || serde_json::to_vec(&b.records).unwrap().len() > 1024 * 1024
        {
            return Err(bad("Archived batch outside1MiB/16context bound"));
        }
        let mut rows = vec![];
        let mut ids = BTreeSet::new();
        for r in b.records {
            let p = unpack(&r)?;
            if !ids.insert(id(&r)) {
                return Err(bad("Duplicate archived source context"));
            }
            let m = meta(&r, &p);
            check_meta(&m)?;
            rows.push((r, p));
        }
        self.archived_anchors(c, &rows).await?;
        let keys: Vec<_> = rows
            .iter()
            .map(|(r, _)| key(c, &format!("packet:{}", id(r))))
            .collect();
        let existing = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if existing.len() != rows.len() {
            return Err(storage("Archived packet prior-read cardinality differs"));
        }
        for ((r, _), old) in rows.iter().zip(existing) {
            if old.is_some_and(|v| v != r.to_string()) {
                return Err(conflict("Archived source packets are immutable"));
            }
        }
        for (r, p) in &rows {
            let m = meta(r, p);
            if self
                .store
                .get(&key(c, &format!("meta:{}", id(r))))
                .await
                .map_err(|e| storage(&e))?
                .is_some_and(|v| v != m.to_string())
            {
                return Err(conflict("Archived source metadata is immutable"));
            }
        }
        let (mut created, mut replay) = (0, 0);
        for (r, p) in &rows {
            let id = id(r);
            let pk = key(c, &format!("packet:{id}"));
            let raw = r.to_string();
            let fresh = self
                .store
                .set_nx(&pk, &raw)
                .await
                .map_err(|e| storage(&e))?;
            if self
                .store
                .get(&pk)
                .await
                .map_err(|e| storage(&e))?
                .as_deref()
                != Some(&raw)
            {
                return Err(conflict("Archived source changed concurrently"));
            }
            let m = meta(r, p).to_string();
            let mk = key(c, &format!("meta:{id}"));
            self.store.set_nx(&mk, &m).await.map_err(|e| storage(&e))?;
            if self
                .store
                .get(&mk)
                .await
                .map_err(|e| storage(&e))?
                .as_deref()
                != Some(&m)
            {
                return Err(conflict("Archived metadata changed concurrently"));
            }
            self.store
                .sadd(&key(c, "contexts"), &id)
                .await
                .map_err(|e| storage(&e))?;
            let index = r["assigned_SOURCE_client_number"]
                .as_str()
                .map(|n| format!("client:{n}"))
                .unwrap_or_else(|| "unassigned".into());
            self.store
                .sadd(&key(c, &index), &id)
                .await
                .map_err(|e| storage(&e))?;
            if fresh { created += 1 } else { replay += 1 }
        }
        let mut out = envelope(c);
        out["created"] = json!(created);
        out["unchanged_replays"] = json!(replay);
        out["submitted"] = json!(rows.len());
        Ok(out)
    }
    async fn archived_packets(&self, c: &str, ids: &[String]) -> Result<Vec<Value>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        if ids.len() > 16 || ids.iter().any(|s| !hash(s)) {
            return Err(bad("Read one to16 archived context IDs"));
        }
        let keys: Vec<_> = ids
            .iter()
            .map(|id| key(c, &format!("packet:{id}")))
            .collect();
        let raw = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if raw.len() != ids.len() {
            return Err(storage("Archived read cardinality differs"));
        }
        let mut rows = vec![];
        for (id, raw) in ids.iter().zip(raw) {
            let r: Value = serde_json::from_str(
                &raw.ok_or_else(|| conflict("Archived context not received"))?,
            )
            .map_err(|_| storage("Archived packet unreadable"))?;
            if self::id(&r) != *id {
                return Err(storage("Archived packet context differs"));
            }
            let p = unpack(&r)?;
            rows.push((r, p));
        }
        self.archived_anchors(c, &rows).await?;
        Ok(rows.into_iter().map(|(r, _)| r).collect())
    }
    async fn archived_metas_for_ids(&self, c: &str, mut ids: Vec<String>) -> Result<Vec<Value>> {
        ids.sort();
        if ids.len() > 448 || ids.iter().any(|s| !hash(s)) || ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(storage("Archived index exceeds closed context bounds"));
        }
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let keys: Vec<_> = ids.iter().map(|id| key(c, &format!("meta:{id}"))).collect();
        let raw = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if raw.len() != ids.len() {
            return Err(storage("Archived metadata cardinality differs"));
        }
        let mut out = vec![];
        for (id, raw) in ids.iter().zip(raw) {
            let m: Value =
                serde_json::from_str(&raw.ok_or_else(|| storage("Archived metadata missing"))?)
                    .map_err(|_| storage("Archived metadata unreadable"))?;
            check_meta(&m)?;
            if m["context_id"] != *id {
                return Err(storage("Archived metadata context differs"));
            }
            out.push(m);
        }
        Ok(out)
    }
    async fn archived_metas(&self, c: &str) -> Result<Vec<Value>> {
        let ids = self
            .store
            .smembers(&key(c, "contexts"))
            .await
            .map_err(|e| storage(&e))?;
        self.archived_metas_for_ids(c, ids).await
    }
    pub async fn test_copy_read_archived_extract_sources(
        &self,
        c: &str,
        b: ArchivedLookup,
    ) -> Result<Value> {
        self.archived_guard(c, b.test_copy, &b.profile_sha256)?;
        let rows = self.archived_packets(c, &b.context_ids).await?;
        let mut out = envelope(c);
        out["context_ids"] = json!(b.context_ids);
        out["records"] = json!(rows);
        Ok(out)
    }
    pub async fn test_copy_archived_extract_summary(&self, c: &str) -> Result<Value> {
        self.archived_guard(c, true, PROFILE)?;
        let index = self.archived_metas(c).await?;
        let mut out = envelope(c);
        let assigned = index
            .iter()
            .filter(|m| !m["assigned_SOURCE_client_number"].is_null())
            .count();
        out["received_contexts"] = json!(index.len());
        out["assigned_contexts"] = json!(assigned);
        out["unassigned_contexts"] = json!(index.len() - assigned);
        out["closed_inventory_complete"] = json!(index.len() == 448);
        out["expected_counts"] = profile()["contract"]["expected_counts"].clone();
        out["unchanged2431_source_sequence_proofs_sha256"] =
            profile()["contract"]["unchanged2431_proof_file_sha256"].clone();
        out["context_index"] = json!(index);
        Ok(out)
    }
    pub async fn test_copy_archived_extract_page(
        &self,
        c: &str,
        client: Option<&str>,
        p: ArchivedPage,
    ) -> Result<Value> {
        self.archived_guard(c, true, PROFILE)?;
        if p.limit != 1
            || p.source_return_year
                .is_some_and(|y| ![2021, 2022].contains(&y))
            || client.is_some_and(|n| !number(n))
        {
            return Err(bad(
                "Archived source page requires one context and explicit archive year scope",
            ));
        }
        let index = client
            .map(|n| format!("client:{n}"))
            .unwrap_or_else(|| "unassigned".into());
        let ids = self
            .store
            .smembers(&key(c, &index))
            .await
            .map_err(|e| storage(&e))?;
        let scoped = self.archived_metas_for_ids(c, ids).await?;
        if scoped.iter().any(|m| match client {
            Some(n) => m["assigned_SOURCE_client_number"] != n,
            None => !m["assigned_SOURCE_client_number"].is_null(),
        }) {
            return Err(storage(
                "Archived scoped index contains another SOURCE target",
            ));
        }
        let mut rows: Vec<_> = scoped
            .into_iter()
            .filter(|m| {
                p.source_return_year
                    .is_none_or(|y| m["source_locator"]["source_return_year"] == y)
            })
            .collect();
        rows.sort_by_key(|m| m["context_id"].as_str().unwrap().to_owned());
        let total = rows.len();
        let ids: Vec<_> = rows
            .into_iter()
            .skip(p.offset)
            .take(1)
            .map(|m| m["context_id"].as_str().unwrap().to_owned())
            .collect();
        let records = self.archived_packets(c, &ids).await?;
        let mut out = envelope(c);
        out["records"] = json!(records);
        out["total"] = json!(total);
        out["offset"] = json!(p.offset);
        out["limit"] = json!(1);
        out["source_return_year"] = json!(p.source_return_year);
        if let Some(n) = client {
            out["client_number"] = json!(n);
        }
        Ok(out)
    }
    pub async fn test_copy_client_archived_extract_sources(
        &self,
        c: &str,
        client: &str,
        p: SourceFactPage,
    ) -> Result<Value> {
        self.test_copy_archived_extract_page(
            c,
            Some(client),
            ArchivedPage {
                offset: p.offset,
                limit: p.limit,
                source_return_year: p.source_return_year,
                _routing_path: None,
            },
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    fn page() -> ArchivedPage {
        ArchivedPage {
            offset: 0,
            limit: 1,
            source_return_year: None,
            _routing_path: None,
        }
    }
    #[test]
    fn public_closure_is_hash_only() {
        let p = profile();
        assert_eq!(
            p["contract"]["packet_sha256"].as_array().unwrap().len(),
            448
        );
        assert_eq!(
            p["contract"]["context_metadata_sha256"]
                .as_array()
                .unwrap()
                .len(),
            448
        );
        assert_eq!(
            p["contract"]["expected_counts"]["held_calculation_source_variants"],
            195
        );
        assert!(unpack(&json!({})).is_err());
    }
    #[tokio::test]
    async fn empty_received_inventory_does_not_claim_full_archive() {
        let s = SourceImport::new(Store::memory());
        let r = s
            .test_copy_archived_extract_summary("dtmax-test-copy")
            .await
            .unwrap();
        assert_eq!(r["received_contexts"], 0);
        assert_eq!(r["closed_inventory_complete"], false);
        assert!(s.test_copy_archived_extract_summary("other").await.is_err());
        assert!(
            s.test_copy_archived_extract_page(
                "dtmax-test-copy",
                None,
                ArchivedPage { limit: 2, ..page() }
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn private_all448_packets_are_exact_source_copies_with_separate_queue() {
        let Ok(path) = std::env::var("NXT_ARCHIVED_EXTRACT_PACKETS") else {
            return;
        };
        let rows: Vec<Value> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 448);
        let f3: Value = serde_json::from_slice(
            &std::fs::read(std::env::var("NXT_DTMAX_OFFICE_PROFILES").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            digest(&f3["clients"]),
            profile()["contract"]["required_F3_canonical_sha256"]
        );
        let store = Store::memory();
        let s = SourceImport::new(store.clone());
        let c = "dtmax-test-copy";
        let mut before = vec![];
        for r in f3["clients"].as_array().unwrap() {
            let n = r["client_number"].as_str().unwrap();
            let k = base(c, &format!("client:{n}"));
            let raw = r.to_string();
            store.set(&k, &raw).await.unwrap();
            store.sadd(&base(c, "clients"), n).await.unwrap();
            for year in r["return_years"].as_array().unwrap() {
                store
                    .sadd(
                        &base(c, "returns"),
                        &format!("{n}:{}", year.as_u64().unwrap()),
                    )
                    .await
                    .unwrap();
            }
            before.push((k, raw));
        }
        for name in [
            "historical_lines:sentinel",
            "recovered_native_sources:sentinel",
            "recovered_native_identity_links:sentinel",
            "wire_exact_historical_lines:sentinel",
        ] {
            let k = base(c, name);
            store.set(&k, "immutable source sentinel").await.unwrap();
            before.push((k, "immutable source sentinel".into()));
        }
        let batch = |records: Vec<Value>| ArchivedBatch {
            test_copy: true,
            profile_sha256: PROFILE.into(),
            records,
        };
        let mut changed = rows[1].clone();
        changed["source_locator"]["source_return_year"] = json!(2020);
        assert!(
            s.test_copy_archived_extract_sources(c, batch(vec![rows[0].clone(), changed]))
                .await
                .is_err()
        );
        assert_eq!(
            s.test_copy_archived_extract_summary(c).await.unwrap()["received_contexts"],
            0
        );
        let (mut records, mut empty) = (0, 0);
        for row in &rows {
            let p = unpack(row).unwrap();
            check_meta(&meta(row, &p)).unwrap();
            records += p["records"].as_array().unwrap().len();
            empty += p["records"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    r["decoded_source_state"] == "verified_empty_type19_no_serialized_header"
                })
                .count();
        }
        assert_eq!(records, 2437);
        assert_eq!(empty, 1);
        for group in rows.chunks(16) {
            let r = s
                .test_copy_archived_extract_sources(c, batch(group.to_vec()))
                .await
                .unwrap();
            assert_eq!(r["created"], group.len());
        }
        let summary = s.test_copy_archived_extract_summary(c).await.unwrap();
        assert_eq!(summary["received_contexts"], 448);
        assert_eq!(summary["assigned_contexts"], 283);
        assert_eq!(summary["unassigned_contexts"], 165);
        assert_eq!(summary["closed_inventory_complete"], true);
        for group in rows.chunks(16) {
            let ids = group.iter().map(id).collect();
            let r = s
                .test_copy_read_archived_extract_sources(
                    c,
                    ArchivedLookup {
                        test_copy: true,
                        profile_sha256: PROFILE.into(),
                        context_ids: ids,
                    },
                )
                .await
                .unwrap();
            assert!(
                r["records"] == json!(group),
                "Archived packet bytes/assignment changed"
            );
        }
        let projection = std::env::var("NXT_ARCHIVED_RUST_PROJECTION_DIR")
            .ok()
            .map(std::path::PathBuf::from);
        if let Some(d) = &projection {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("summary.private.json"), summary.to_string()).unwrap();
        }
        let mut assigned_pages = 0;
        let mut groups = BTreeMap::<(String, u16), Vec<&Value>>::new();
        for row in &rows {
            if let Some(n) = row["assigned_SOURCE_client_number"].as_str() {
                groups
                    .entry((
                        n.into(),
                        row["source_locator"]["source_return_year"]
                            .as_u64()
                            .unwrap() as u16,
                    ))
                    .or_default()
                    .push(row);
            }
        }
        for ((n, y), mut expected) in groups {
            expected.sort_by_key(|r| id(r));
            for (at, want) in expected.iter().enumerate() {
                store.take_test_reads();
                let r = s
                    .test_copy_archived_extract_page(
                        c,
                        Some(&n),
                        ArchivedPage {
                            offset: at,
                            source_return_year: Some(y),
                            ..page()
                        },
                    )
                    .await
                    .unwrap();
                assert_eq!(r["records"], json!([want]));
                assert_eq!(r["total"], expected.len());
                let reads = store.take_test_reads();
                assert!(
                    reads
                        .iter()
                        .filter(|ks| ks.iter().any(|k| k.contains(":meta:")))
                        .all(|ks| ks.len() <= 2),
                    "Client page must not fetch all448 metadata rows"
                );
                if let Some(d) = &projection {
                    std::fs::write(
                        d.join(format!("assigned-{}.private.json", id(want))),
                        r.to_string(),
                    )
                    .unwrap();
                }
                assigned_pages += 1;
            }
        }
        let mut expected: Vec<_> = rows
            .iter()
            .filter(|r| r["assigned_SOURCE_client_number"].is_null())
            .collect();
        expected.sort_by_key(|r| id(r));
        for (at, want) in expected.iter().enumerate() {
            store.take_test_reads();
            let r = s
                .test_copy_archived_extract_page(
                    c,
                    None,
                    ArchivedPage {
                        offset: at,
                        ..page()
                    },
                )
                .await
                .unwrap();
            assert_eq!(r["records"], json!([want]));
            assert_eq!(r["total"], 165);
            assert!(
                store
                    .take_test_reads()
                    .iter()
                    .filter(|ks| ks.iter().any(|k| k.contains(":meta:")))
                    .all(|ks| ks.len() == 165),
                "Queue page reads only165 scoped metadata rows"
            );
            if let Some(d) = &projection {
                std::fs::write(
                    d.join(format!("unassigned-{at:03}.private.json")),
                    r.to_string(),
                )
                .unwrap();
            }
        }
        assert_eq!(assigned_pages, 283);

        assert_eq!(
            s.test_copy_archived_extract_sources(c, batch(rows[..16].to_vec()))
                .await
                .unwrap()["unchanged_replays"],
            16
        );
        // The production profile shape omits derived return_years; normalize only those.
        for (k, raw) in &before {
            if k.starts_with(&base(c, "client:")) {
                let mut value: Value = serde_json::from_str(raw).unwrap();
                value.as_object_mut().unwrap().remove("return_years");
                store.set(k, &value.to_string()).await.unwrap();
            }
        }
        let assigned = rows
            .iter()
            .find(|r| r["assigned_SOURCE_client_number"].is_string())
            .unwrap();
        s.test_copy_read_archived_extract_sources(
            c,
            ArchivedLookup {
                test_copy: true,
                profile_sha256: PROFILE.into(),
                context_ids: vec![id(assigned)],
            },
        )
        .await
        .unwrap();
        let k = base(
            c,
            &format!(
                "client:{}",
                assigned["assigned_SOURCE_client_number"].as_str().unwrap()
            ),
        );
        let raw = store.get(&k).await.unwrap().unwrap();
        let mut changed: Value = serde_json::from_str(&raw).unwrap();
        changed["name"] = json!("SYNTHETIC CHANGED SOURCE PROFILE");
        store.set(&k, &changed.to_string()).await.unwrap();
        assert!(
            s.test_copy_read_archived_extract_sources(
                c,
                ArchivedLookup {
                    test_copy: true,
                    profile_sha256: PROFILE.into(),
                    context_ids: vec![id(assigned)]
                }
            )
            .await
            .is_err()
        );
        store.set(&k, &raw).await.unwrap();
        for (k, raw) in before {
            if k.starts_with(&base(c, "client:")) {
                let mut expected: Value = serde_json::from_str(&raw).unwrap();
                expected.as_object_mut().unwrap().remove("return_years");
                assert_eq!(store.get(&k).await.unwrap().unwrap(), expected.to_string());
            } else {
                assert_eq!(store.get(&k).await.unwrap().as_deref(), Some(raw.as_str()));
            }
        }
        store
            .sadd(&key(c, "unassigned"), &id(assigned))
            .await
            .unwrap();
        assert!(
            s.test_copy_archived_extract_page(c, None, page())
                .await
                .is_err()
        );
        let key = key(c, &format!("packet:{}", id(&rows[0])));
        store.set(&key, "{}").await.unwrap();
        assert!(
            s.test_copy_read_archived_extract_sources(
                c,
                ArchivedLookup {
                    test_copy: true,
                    profile_sha256: PROFILE.into(),
                    context_ids: vec![id(&rows[0])]
                }
            )
            .await
            .is_err()
        );
    }
}
