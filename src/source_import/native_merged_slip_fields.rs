//! Immutable appended native-form evidence. Original views and supplements stay intact.
//! Full sidecars are closed-profile authenticated; values remain unapproved source text.
use super::native_slip_views::{ActiveValue, NativeSlipView};
use super::{bad, conflict, storage, ImportError, SourceImport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE: &str = "bd98c9f6157e9cd3cb26befa00957c80a639575ba9bb987c45947fce8ae28358";
const CORE: &str = "4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SelectorReference {
    pub binding_ordinal: u32,
    pub selector_binding_sha256: String,
    pub source_call: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_parent_index: Option<ParentIndexAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_equal_parent_index: Option<ParentIndexAnchor>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ParentIndexAnchor {
    pub merged_field_index: u32,
    pub source_component_index: u32,
    pub decoded_form_index: u32,
    pub source_form_field_index: u32,
    pub source_field_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MergedField {
    pub field_id: u32,
    pub merged_field_index: u32,
    pub selector_references: Vec<SelectorReference>,
    pub source_component_index: u32,
    pub source_field_record_position: u32,
    pub source_field_sha256: String,
    pub source_form_field_index: u32,
    pub source_group_marker: i32,
    pub source_occurrence_marker: i32,
    pub source_type_tag: i32,
    pub value: ActiveValue,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceComponent {
    pub decoded_form_index: u32,
    pub form_name: String,
    pub from_data_segment: bool,
    pub matching_form_occurrence: u32,
    pub original_field_count: u32,
    pub q_plan: i32,
    pub selector_keyword_id: Option<u32>,
    pub selector_option_number: Option<i32>,
    pub source_component_index: u32,
    pub source_component_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeMergedFields {
    pub schema: String,
    pub merged_component_profile_sha256: String,
    pub source_archive_sha256: String,
    pub source_view_file_sha256: String,
    pub source_view_line_sha256: String,
    pub source_view_number: u32,
    pub client_number: String,
    pub family_file_id: String,
    pub source_return_year: u16,
    pub vendor_module: String,
    pub vendor_slip_kind: String,
    pub vendor_slip_ordinal: u32,
    pub vendor_form: String,
    pub from_data_segment: bool,
    pub original_field_count: u32,
    pub primary_view: String,
    pub primary_decoded_form_index: u32,
    pub raw_forms_file_sha256: String,
    pub raw_forms_line_number: u32,
    pub raw_forms_line_sha256: String,
    pub source_components: Vec<SourceComponent>,
    pub financial_fields: Vec<MergedField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergedFieldBatch {
    pub test_copy: bool,
    pub records: Vec<NativeMergedFields>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergedFieldLookup {
    pub test_copy: bool,
    pub source_view_numbers: Vec<u32>,
}
#[derive(Deserialize)]
struct ClosedProfile {
    financial_profile_sha256: String,
    financial_profile_canonical_sha256: String,
    financial_profile: Value,
    source_archive_sha256: String,
    source_view_file_sha256: String,
    raw_forms_file_sha256: String,
    allowed_full_sidecar_sha256: BTreeSet<String>,
}
fn digest(v: &impl Serialize) -> String {
    // Value maps are recursively key-sorted by serde_json's default BTreeMap.
    let value = serde_json::to_value(v).expect("serializable native merged evidence");
    hex::encode(Sha256::digest(serde_json::to_vec(&value).expect("canonical evidence JSON")))
}
fn profile() -> &'static ClosedProfile {
    static P: OnceLock<ClosedProfile> = OnceLock::new();
    P.get_or_init(|| {
        let p: ClosedProfile = serde_json::from_str(include_str!("native_merged_slip_profiles.json"))
            .expect("reviewed closed native merged-field profile");
        assert_eq!(p.financial_profile_sha256, PROFILE);
        assert_eq!(p.financial_profile_canonical_sha256, PROFILE);
        assert_eq!(digest(&p.financial_profile), PROFILE);
        assert_eq!(p.allowed_full_sidecar_sha256.len(), 953);
        p
    })
}
fn sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn identity(s: &NativeMergedFields) -> String {
    format!("{}:{:06}", s.source_view_file_sha256, s.source_view_number)
}
fn key(company: &str, id: &str) -> String {
    format!("dtmaxtc:{company}:native_merged_fields:{PROFILE}:record:{id}")
}
fn index(company: &str, suffix: &str) -> String {
    format!("dtmaxtc:{company}:native_merged_fields:{PROFILE}:{suffix}")
}
fn core_key(company: &str, s: &NativeMergedFields) -> String {
    format!("dtmaxtc:{company}:native_slip:record:{}:{:06}:{CORE}", s.source_view_file_sha256, s.source_view_number)
}
fn expected_selectors(year: u16, field: u32) -> Vec<SelectorReference> {
    profile().financial_profile["profiles"][year.to_string()]["T5008"]["financial_selectors"]
        .as_array().into_iter().flatten()
        .filter(|v| v["field_id"].as_u64() == Some(field as u64)).enumerate()
        .map(|(i, v)| SelectorReference {
            binding_ordinal: i as u32,
            selector_binding_sha256: v["selector_binding_sha256"].as_str().expect("reviewed selector hash").to_owned(),
            source_call: v["source_call"].clone(),
            selected_parent_index: None,
            effective_equal_parent_index: None,
        }).collect()
}
fn parent_anchor_valid(p: &ParentIndexAnchor, s: &NativeMergedFields) -> bool {
    p.source_component_index == 0 && p.decoded_form_index == s.primary_decoded_form_index
        && p.merged_field_index == p.source_form_field_index && p.source_form_field_index < s.original_field_count
        && sha(&p.source_field_sha256)
}
fn selectors_valid(actual: &[SelectorReference], expected: &[SelectorReference], s: &NativeMergedFields) -> bool {
    actual.len() == expected.len() && actual.iter().zip(expected).all(|(a,e)| {
        let base = a.binding_ordinal == e.binding_ordinal && a.selector_binding_sha256 == e.selector_binding_sha256 && a.source_call == e.source_call;
        let anchors = if s.source_return_year <= 2016 {
            a.selected_parent_index.as_ref().is_some_and(|p|parent_anchor_valid(p,s))
                && a.effective_equal_parent_index.as_ref().is_some_and(|p|parent_anchor_valid(p,s))
        } else { a.selected_parent_index.is_none() && a.effective_equal_parent_index.is_none() };
        base && anchors
    })
}
fn validate_structure(s: &NativeMergedFields, v: &NativeSlipView) -> Result<()> {
    let p = profile();
    if s.schema != "dtmax-native-merged-component-financial-sidecar/v1"
        || s.merged_component_profile_sha256 != PROFILE || v.financial_profile_sha256 != CORE
        || s.source_archive_sha256 != v.source_archive_sha256 || s.source_archive_sha256 != p.source_archive_sha256
        || s.source_view_file_sha256 != v.source_view_file_sha256 || s.source_view_file_sha256 != p.source_view_file_sha256
        || s.source_view_line_sha256 != v.source_view_line_sha256 || s.source_view_number != v.source_view_number
        || s.client_number != v.client_number || s.family_file_id != v.family_file_id
        || s.source_return_year != v.source_return_year || !(2015..=2018).contains(&s.source_return_year)
        || s.vendor_module != v.vendor_module || s.vendor_module != "T1"
        || s.vendor_slip_kind != v.vendor_slip_kind || s.vendor_slip_kind != "T5008"
        || s.vendor_slip_ordinal != v.vendor_slip_ordinal || s.vendor_slip_ordinal >= 10_000
        || s.vendor_form != v.vendor_form || s.vendor_form != "92"
        || s.from_data_segment != v.from_data_segment || !s.from_data_segment
        || s.original_field_count != v.original_field_count || s.original_field_count > 10_000
        || s.primary_view != "vendor_GetT1TaxSlip_preferred_view" || s.primary_decoded_form_index >= 100_000
        || s.raw_forms_file_sha256 != p.raw_forms_file_sha256 || s.raw_forms_line_number == 0 || s.raw_forms_line_number > 100_000
        || ![&s.source_view_line_sha256, &s.raw_forms_line_sha256].iter().all(|x| sha(x))
        || s.source_components.len() != 1 || s.financial_fields.is_empty() || s.financial_fields.len() > 512 {
        return Err(bad("Merged financial source context, core anchor or profile differs"));
    }
    let c = &s.source_components[0];
    if c.source_component_index != 1 || c.form_name != "171"
        || c.matching_form_occurrence != s.vendor_slip_ordinal
        || c.decoded_form_index == s.primary_decoded_form_index || c.decoded_form_index >= 100_000
        || c.original_field_count == 0 || c.original_field_count > 10_000
        || c.selector_keyword_id.is_some() || c.selector_option_number.is_some() || !sha(&c.source_component_sha256) {
        return Err(bad("Merged form is not the reviewed same-ordinal component 171"));
    }
    let mut previous = None;
    for f in &s.financial_fields {
        let expected = expected_selectors(s.source_return_year, f.field_id);
        if !matches!(f.field_id, 14485 | 14486 | 14488) || expected.is_empty() || !selectors_valid(&f.selector_references,&expected,s)
            || f.source_component_index != c.source_component_index || f.source_form_field_index >= c.original_field_count
            || s.original_field_count.checked_add(f.source_form_field_index) != Some(f.merged_field_index)
            || previous.is_some_and(|i| i >= f.merged_field_index)
            || f.source_field_record_position > 64 * 1024 * 1024 || !sha(&f.source_field_sha256)
            || f.source_type_tag != 4
            || !matches!(&f.value, ActiveValue::Text { text: Some(t) } if !t.trim().is_empty() && t.len() <= 4096) {
            return Err(bad("Merged source field, ordering, value or exact selector reference differs"));
        }
        previous = Some(f.merged_field_index);
    }
    Ok(())
}
fn validate(s: &NativeMergedFields, v: &NativeSlipView, approved: &BTreeSet<String>) -> Result<()> {
    validate_structure(s, v)?;
    if !approved.contains(&digest(s)) {
        return Err(bad("Complete merged sidecar evidence is not in the reviewed closed source package"));
    }
    Ok(())
}
impl SourceImport {
    pub async fn test_copy_read_native_merged_slip_fields(&self, company: &str, lookup: MergedFieldLookup) -> Result<Value> {
        self.read_native_merged_fields(company, lookup, &profile().allowed_full_sidecar_sha256).await
    }
    async fn read_native_merged_fields(&self, company: &str, lookup: MergedFieldLookup, approved: &BTreeSet<String>) -> Result<Value> {
        self.test_copy_guard(company, lookup.test_copy)?;
        if lookup.source_view_numbers.is_empty() || lookup.source_view_numbers.len() > 250
            || lookup.source_view_numbers.iter().any(|n| *n == 0 || *n > 100_000) {
            return Err(bad("Merged source lookup requires 1-250 valid source view numbers"));
        }
        let keys: Vec<_> = lookup.source_view_numbers.iter().map(|n| key(company, &format!("{}:{n:06}", profile().source_view_file_sha256))).collect();
        let raws = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if raws.len() != keys.len() { return Err(storage("Merged source lookup cardinality differs")); }
        let mut sidecars = Vec::new();
        for (number, raw) in lookup.source_view_numbers.iter().zip(raws) {
            let sidecar = raw.map(|r| serde_json::from_str::<NativeMergedFields>(&r)
                .map_err(|_| storage("Merged source lookup record is corrupt"))).transpose()?;
            if sidecar.as_ref().is_some_and(|s| s.source_view_number != *number || s.source_view_file_sha256 != profile().source_view_file_sha256) {
                return Err(storage("Merged source lookup identity differs"));
            }
            sidecars.push(sidecar);
        }
        let core_keys: Vec<_> = sidecars.iter().flatten().map(|s| core_key(company, s)).collect();
        let originals = self.store.mget(&core_keys).await.map_err(|e| storage(&e))?;
        if originals.len() != core_keys.len() { return Err(storage("Merged source lookup original cardinality differs")); }
        for (s, raw) in sidecars.iter().flatten().zip(originals) {
            let v: NativeSlipView = serde_json::from_str(&raw.ok_or_else(|| storage("Merged source lookup original missing"))?)
                .map_err(|_| storage("Merged source lookup original is corrupt"))?;
            validate(s, &v, approved).map_err(|_| storage("Merged source lookup validation failed"))?;
        }
        Ok(json!({"test_copy":true,"source_scope":company,"merged_component_profile_sha256":PROFILE,
            "source_view_file_sha256":profile().source_view_file_sha256,"source_view_numbers":lookup.source_view_numbers,
            "records":sidecars,"source_fields_only":true,"financial_values_approved":false}))
    }
    pub async fn test_copy_native_merged_slip_fields(&self, company: &str, batch: MergedFieldBatch) -> Result<Value> {
        self.preserve_native_merged_fields(company, batch, &profile().allowed_full_sidecar_sha256).await
    }
    async fn preserve_native_merged_fields(&self, company: &str, batch: MergedFieldBatch, approved: &BTreeSet<String>) -> Result<Value> {
        self.test_copy_guard(company, batch.test_copy)?;
        if batch.records.is_empty() || batch.records.len() > 250 {
            return Err(bad("Merged financial source batch must contain 1-250 records"));
        }
        let keys: Vec<_> = batch.records.iter().map(|s| core_key(company, s)).collect();
        let originals = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if originals.len() != batch.records.len() { return Err(storage("Merged source original cardinality differs")); }
        let mut seen = BTreeMap::new();
        // All anchors, evidence hashes, duplicate identities and existing immutable rows preflight before writes.
        for (s, raw) in batch.records.iter().zip(originals) {
            let v: NativeSlipView = serde_json::from_str(&raw.ok_or_else(|| bad("Merged fields require their original preserved native view"))?)
                .map_err(|_| storage("Original merged-source view is corrupt"))?;
            validate(s, &v, approved)?;
            if seen.insert(identity(s), s).is_some_and(|old| old != s) {
                return Err(conflict("Conflicting merged financial evidence in one batch"));
            }
            let raw = serde_json::to_string(s).map_err(|_| storage("Merged source encoding failed"))?;
            if self.store.get(&key(company, &identity(s))).await.map_err(|e| storage(&e))?.is_some_and(|old| old != raw) {
                return Err(conflict("Merged financial source evidence is immutable"));
            }
        }
        let (mut created, mut unchanged) = (0, 0);
        for s in &batch.records {
            let id = identity(s);
            let raw = serde_json::to_string(s).map_err(|_| storage("Merged source encoding failed"))?;
            let k = key(company, &id);
            let added = self.store.set_nx(&k, &raw).await.map_err(|e| storage(&e))?;
            if self.store.get(&k).await.map_err(|e| storage(&e))?.as_deref() != Some(&raw) {
                return Err(conflict("Merged financial source evidence is immutable"));
            }
            for suffix in ["all".to_owned(), format!("client:{}", s.client_number), format!("client_year:{}:{}", s.client_number, s.source_return_year)] {
                self.store.sadd(&index(company, &suffix), &id).await.map_err(|e| storage(&e))?;
            }
            if added { created += 1; } else { unchanged += 1; }
        }
        Ok(json!({"test_copy":true,"source_scope":company,"created":created,"unchanged_replays":unchanged,
            "original_native_views_modified":0,"existing_supplements_modified":0,"normalized_slips_created":0,"tax_cases_imported":0}))
    }
    pub(super) async fn attach_native_merged_slip_fields(&self, company: &str, views: &[NativeSlipView], records: &mut [Value]) -> Result<()> {
        let keys: Vec<_> = views.iter().map(|v| key(company, &format!("{}:{:06}", v.source_view_file_sha256, v.source_view_number))).collect();
        let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
        if rows.len() != views.len() || records.len() != views.len() { return Err(storage("Merged source read cardinality differs")); }
        for ((v, record), raw) in views.iter().zip(records).zip(rows) {
            if let Some(raw) = raw {
                let s: NativeMergedFields = serde_json::from_str(&raw).map_err(|_| storage("Merged source evidence is corrupt"))?;
                validate(&s, v, &profile().allowed_full_sidecar_sha256).map_err(|_| storage("Stored merged source evidence fails validation"))?;
                // Full component/field/selector provenance survives readback, separate from original fields.
                record["merged_component_financial_source"] = json!(s);
                record["merged_component_field_review"] = json!({"profile_sha256":PROFILE,
                    "source_fields_only":true,"box_assignment_approved":false,"normalization_approved":false,
                    "aggregation_approved":false,"allocation_approved":false,"financial_values_approved":false});
            }
        }
        Ok(())
    }
    pub async fn test_copy_native_merged_slip_field_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let mut ids = self.store.smembers(&index(company, "all")).await.map_err(|e| storage(&e))?;
        if ids.len() > profile().allowed_full_sidecar_sha256.len() { return Err(storage("Merged source index exceeds reviewed package")); }
        ids.sort();
        let mut fields = 0;
        let mut clients = BTreeSet::new();
        for chunk in ids.chunks(250) {
            let keys: Vec<_> = chunk.iter().map(|id| key(company, id)).collect();
            let rows = self.store.mget(&keys).await.map_err(|e| storage(&e))?;
            if rows.len() != chunk.len() { return Err(storage("Merged summary cardinality differs")); }
            let mut sidecars = Vec::new();
            for raw in rows {
                sidecars.push(serde_json::from_str::<NativeMergedFields>(&raw.ok_or_else(|| storage("Merged source index record missing"))?)
                    .map_err(|_| storage("Merged source evidence is corrupt"))?);
            }
            let original_keys: Vec<_> = sidecars.iter().map(|s| core_key(company, s)).collect();
            let originals = self.store.mget(&original_keys).await.map_err(|e| storage(&e))?;
            if originals.len() != sidecars.len() { return Err(storage("Merged summary original cardinality differs")); }
            for ((id, s), raw) in chunk.iter().zip(sidecars).zip(originals) {
                let v: NativeSlipView = serde_json::from_str(&raw.ok_or_else(|| storage("Merged source original missing"))?)
                    .map_err(|_| storage("Merged source original is corrupt"))?;
                validate(&s, &v, &profile().allowed_full_sidecar_sha256).map_err(|_| storage("Merged source summary validation failed"))?;
                if *id != identity(&s) { return Err(storage("Merged source index identity differs")); }
                fields += s.financial_fields.len();
                clients.insert(s.client_number);
            }
        }
        Ok(json!({"test_copy":true,"source_scope":company,"merged_component_profile_sha256":PROFILE,
            "merged_component_source_views":ids.len(),"merged_component_financial_fields":fields,"affected_clients":clients.len(),
            "original_native_views_modified":0,"existing_supplements_modified":0,"normalized_slips_created":0,"tax_cases_imported":0,
            "financial_values_approved":false,"box_assignment_approved":false,"aggregation_approved":false,"allocation_approved":false}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (NativeMergedFields, NativeSlipView) {
        let p = profile();
        let s = NativeMergedFields {
            schema:"dtmax-native-merged-component-financial-sidecar/v1".into(), merged_component_profile_sha256:PROFILE.into(),
            source_archive_sha256:p.source_archive_sha256.clone(), source_view_file_sha256:p.source_view_file_sha256.clone(),
            source_view_line_sha256:"a".repeat(64), source_view_number:1, client_number:"42".into(), family_file_id:"7".into(),
            source_return_year:2018, vendor_module:"T1".into(), vendor_slip_kind:"T5008".into(), vendor_slip_ordinal:2,
            vendor_form:"92".into(), from_data_segment:true, original_field_count:8, primary_view:"vendor_GetT1TaxSlip_preferred_view".into(),
            primary_decoded_form_index:10, raw_forms_file_sha256:p.raw_forms_file_sha256.clone(), raw_forms_line_number:1,
            raw_forms_line_sha256:"b".repeat(64), source_components:vec![SourceComponent {decoded_form_index:11,form_name:"171".into(),
                from_data_segment:false,matching_form_occurrence:2,original_field_count:2,q_plan:0,selector_keyword_id:None,
                selector_option_number:None,source_component_index:1,source_component_sha256:"c".repeat(64)}],
            financial_fields:(0..2).map(|i| MergedField {field_id:14485,merged_field_index:8+i,selector_references:expected_selectors(2018,14485),
                source_component_index:1,source_field_record_position:100+i,source_field_sha256:"d".repeat(64),source_form_field_index:i,
                source_group_marker:0,source_occurrence_marker:0,source_type_tag:4,value:ActiveValue::Text{text:Some("12.00".into())}}).collect(),
        };
        let v = NativeSlipView {financial_profile_sha256:CORE.into(),source_archive_sha256:s.source_archive_sha256.clone(),
            source_view_file_sha256:s.source_view_file_sha256.clone(),source_view_line_sha256:s.source_view_line_sha256.clone(),
            source_view_number:s.source_view_number,client_number:s.client_number.clone(),family_file_id:s.family_file_id.clone(),
            source_return_year:s.source_return_year,vendor_module:s.vendor_module.clone(),vendor_slip_kind:s.vendor_slip_kind.clone(),
            vendor_slip_ordinal:s.vendor_slip_ordinal,vendor_form:s.vendor_form.clone(),from_data_segment:s.from_data_segment,
            original_field_count:s.original_field_count,financial_fields:vec![]};
        (s,v)
    }
    #[test]
    fn closed_package_binds_all_values_and_provenance() {
        let (s,v)=fixture();
        assert!(validate_structure(&s,&v).is_ok());
        assert!(validate(&s,&v,&profile().allowed_full_sidecar_sha256).is_err());
        let allowed=BTreeSet::from([digest(&s)]);
        assert!(validate(&s,&v,&allowed).is_ok());
        for change in [0,1,2,3] {
            let mut t=s.clone();
            match change {0=>t.financial_fields[0].value=ActiveValue::Text{text:Some("13.00".into())},
                1=>t.raw_forms_line_sha256="e".repeat(64),2=>t.source_components[0].source_component_sha256="e".repeat(64),
                _=>t.financial_fields[0].source_field_sha256="e".repeat(64)}
            assert!(validate_structure(&t,&v).is_ok());
            assert!(validate(&t,&v,&allowed).is_err());
        }
    }
    #[test]
    fn every_core_context_anchor_is_required() {
        let (s,v)=fixture();
        let mutations:Vec<fn(&mut NativeSlipView)>=vec![|v|v.financial_profile_sha256="e".repeat(64),|v|v.source_archive_sha256="e".repeat(64),
            |v|v.source_view_file_sha256="e".repeat(64),|v|v.source_view_line_sha256="e".repeat(64),|v|v.source_view_number+=1,
            |v|v.client_number="43".into(),|v|v.family_file_id="8".into(),|v|v.source_return_year=2017,|v|v.vendor_module="T3".into(),
            |v|v.vendor_slip_kind="T5".into(),|v|v.vendor_slip_ordinal+=1,|v|v.vendor_form="171".into(),|v|v.from_data_segment=false,
            |v|v.original_field_count+=1];
        for mutation in mutations {let mut q=v.clone();mutation(&mut q);assert!(validate_structure(&s,&q).is_err());}
    }
    #[test]
    fn no_cache_primary_wrong_component_ordinal_or_selector_approximation() {
        let (s,v)=fixture();
        for n in 0..7 {let mut t=s.clone();let mut q=v.clone();match n {
            0=>{t.from_data_segment=false;q.from_data_segment=false},1=>t.source_components[0].matching_form_occurrence+=1,
            2=>t.source_components[0].form_name="170".into(),3=>{t.financial_fields[0].selector_references.pop();},
            4=>t.financial_fields[0].selector_references[0].source_call["optionNumber"]=json!(0),
            5=>t.financial_fields[0].merged_field_index=7,_=>{t.source_return_year=2019;q.source_return_year=2019}}
            assert!(validate_structure(&t,&q).is_err());}
        // Repeated equal field IDs and values are distinct ordered source occurrences.
        assert_eq!(s.financial_fields.len(),2);assert!(validate_structure(&s,&v).is_ok());
        let mut t=s.clone();t.financial_fields.swap(0,1);assert!(validate_structure(&t,&v).is_err());
        // Ordinals are local to each field's selector bindings, not the entire profile array.
        let references=expected_selectors(2018,14486);
        assert_eq!(references.iter().map(|r|r.binding_ordinal).collect::<Vec<_>>(),vec![0,1,2,3]);
        assert_eq!(references[0].selector_binding_sha256,"152fbc76d33510d435c3befab70a753c47195746b9c606c08d86eeafc4a14228");
    }
    #[test]
    fn parent_child_selectors_retain_both_independent_parent_anchors() {
        let (mut s,mut v)=fixture();s.source_return_year=2015;v.source_return_year=2015;
        let anchor=ParentIndexAnchor{merged_field_index:4,source_component_index:0,decoded_form_index:10,
            source_form_field_index:4,source_field_sha256:"e".repeat(64)};
        for f in &mut s.financial_fields {
            f.selector_references=expected_selectors(2015,14485);
            f.selector_references[0].selected_parent_index=Some(anchor.clone());
            f.selector_references[0].effective_equal_parent_index=Some(anchor.clone());
        }
        assert!(validate_structure(&s,&v).is_ok());
        let serialized=serde_json::to_value(&s).unwrap();
        assert!(serialized["financial_fields"][0]["selector_references"][0]["selected_parent_index"].is_object());
        assert!(serialized["financial_fields"][0]["selector_references"][0]["effective_equal_parent_index"].is_object());
        let approved=BTreeSet::from([digest(&s)]);
        let mut t=s.clone();t.financial_fields[0].selector_references[0].effective_equal_parent_index=None;
        assert!(validate_structure(&t,&v).is_err());
        let mut t=s.clone();t.financial_fields[0].selector_references[0].selected_parent_index.as_mut().unwrap().source_component_index=1;
        assert!(validate_structure(&t,&v).is_err());
        let mut t=s.clone();t.financial_fields[0].selector_references[0].effective_equal_parent_index.as_mut().unwrap().source_field_sha256="f".repeat(64);
        assert!(validate_structure(&t,&v).is_ok());assert!(validate(&t,&v,&approved).is_err());
    }
    #[test]
    fn optional_complete_private_package_matches_all_original_anchors() {
        let Ok(sidecar_path)=std::env::var("NXT_DTMAX_MERGED_SOURCE_SIDECARS") else {return};
        let staging=std::env::var("NXT_DTMAX_NATIVE_VIEW_STAGING").expect("private original-view staging directory required");
        let bytes=std::fs::read(sidecar_path).expect("private merged sidecar file");
        let public:Value=serde_json::from_str(include_str!("native_merged_slip_profiles.json")).unwrap();
        assert_eq!(hex::encode(Sha256::digest(&bytes)),public["verified_private_sidecar_file_sha256"].as_str().unwrap());
        let mut originals=BTreeMap::new();
        for entry in std::fs::read_dir(staging).unwrap() {
            let path=entry.unwrap().path();
            if !path.file_name().unwrap().to_string_lossy().starts_with("native-views-") || path.extension().and_then(|x|x.to_str())!=Some("json") {continue}
            let batch:super::super::native_slip_views::NativeSlipBatch=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            for v in batch.records {
                let key=(v.source_view_file_sha256.clone(),v.source_view_number);
                assert!(originals.insert(key,v).is_none(),"duplicate original source view");
            }
        }
        assert_eq!(originals.len(),47_969);
        let mut hashes=BTreeSet::new();let mut fields=0;let mut references=0;
        for line in bytes.split(|b|*b==b'\n').filter(|line|!line.is_empty()) {
            let s:NativeMergedFields=serde_json::from_slice(line).expect("strict private merged sidecar");
            let v=originals.get(&(s.source_view_file_sha256.clone(),s.source_view_number)).expect("original source anchor");
            validate(&s,v,&profile().allowed_full_sidecar_sha256).unwrap_or_else(|e|panic!("private merged package failed source validation: {}",e.1));
            assert!(hashes.insert(digest(&s)),"duplicate complete sidecar");
            fields+=s.financial_fields.len();references+=s.financial_fields.iter().map(|f|f.selector_references.len()).sum::<usize>();
        }
        assert_eq!(hashes,profile().allowed_full_sidecar_sha256);
        assert_eq!(fields,1_743);assert_eq!(references,5_385);
        println!("Validated 953 complete merged sidecars / 1743 fields / 5385 selector references against 47969 immutable original views");
    }
    #[tokio::test]
    async fn preflight_immutable_replay_and_original_preservation() {
        let store=crate::store::Store::memory();let service=SourceImport::new(store.clone());
        let (s,v)=fixture();let approved=BTreeSet::from([digest(&s)]);
        let original=serde_json::to_string(&v).unwrap();
        store.set(&core_key("dtmax-test-copy",&s),&original).await.unwrap();
        let supplement_key="dtmaxtc:dtmax-test-copy:native_supplement:existing-evidence";
        store.set(supplement_key,"preserved existing supplement").await.unwrap();
        let bad_record={let mut t=s.clone();t.source_components[0].matching_form_occurrence+=1;t};
        assert!(service.preserve_native_merged_fields("dtmax-test-copy",MergedFieldBatch{test_copy:true,records:vec![s.clone(),bad_record]},&approved).await.is_err());
        assert!(store.get(&key("dtmax-test-copy",&identity(&s))).await.unwrap().is_none());
        let first=service.preserve_native_merged_fields("dtmax-test-copy",MergedFieldBatch{test_copy:true,records:vec![s.clone()]},&approved).await.unwrap();
        assert_eq!(first["created"],1);
        let replay=service.preserve_native_merged_fields("dtmax-test-copy",MergedFieldBatch{test_copy:true,records:vec![s.clone()]},&approved).await.unwrap();
        assert_eq!(replay["unchanged_replays"],1);
        let saved:NativeMergedFields=serde_json::from_str(&store.get(&key("dtmax-test-copy",&identity(&s))).await.unwrap().unwrap()).unwrap();
        assert_eq!(saved,s);assert_eq!(store.get(&core_key("dtmax-test-copy",&s)).await.unwrap(),Some(original));
        assert_eq!(store.get(supplement_key).await.unwrap().as_deref(),Some("preserved existing supplement"));
        let mut changed=s.clone();changed.raw_forms_line_sha256="f".repeat(64);
        let changed_approved=BTreeSet::from([digest(&changed)]);
        assert!(service.preserve_native_merged_fields("dtmax-test-copy",MergedFieldBatch{test_copy:true,records:vec![changed]},&changed_approved).await.is_err());
    }
    #[tokio::test]
    async fn bounded_bulk_read_preserves_order_nulls_and_revalidates_core() {
        let store=crate::store::Store::memory();let service=SourceImport::new(store.clone());let (s,mut v)=fixture();
        let approved=BTreeSet::from([digest(&s)]);
        store.set(&core_key("dtmax-test-copy",&s),&serde_json::to_string(&v).unwrap()).await.unwrap();
        service.preserve_native_merged_fields("dtmax-test-copy",MergedFieldBatch{test_copy:true,records:vec![s.clone()]},&approved).await.unwrap();
        let response=service.read_native_merged_fields("dtmax-test-copy",MergedFieldLookup{test_copy:true,source_view_numbers:vec![2,1,1]},&approved).await.unwrap();
        assert!(response["records"][0].is_null());
        assert_eq!(response["records"][1],json!(s));assert_eq!(response["records"][2],json!(s));
        for numbers in [vec![],vec![0],vec![1;251]] {
            assert!(service.read_native_merged_fields("dtmax-test-copy",MergedFieldLookup{test_copy:true,source_view_numbers:numbers},&approved).await.is_err());
        }
        assert!(service.read_native_merged_fields("dtmax-test-copy",MergedFieldLookup{test_copy:false,source_view_numbers:vec![1]},&approved).await.is_err());
        v.family_file_id="8".into();store.set(&core_key("dtmax-test-copy",&s),&serde_json::to_string(&v).unwrap()).await.unwrap();
        assert!(service.read_native_merged_fields("dtmax-test-copy",MergedFieldLookup{test_copy:true,source_view_numbers:vec![1]},&approved).await.is_err());
    }
}
