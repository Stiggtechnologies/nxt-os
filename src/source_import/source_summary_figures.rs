//! Exact printed summary observations, independent of native returns and other PDF lanes.
use super::preserved_records::SourceFactPage;
use super::{ImportError, SourceImport, bad, conflict, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE_SHA: &str = "7d678108f4ba7c766029e6cdee96e4b270f1a20df7aa76d5e1f99a2cc0b11c91";
#[derive(Deserialize)]
struct Profile {
    allowed_record_sha256: String,
    #[serde(rename = "source_PDF_sha256")]
    source_pdf_sha256: String,
    source_descriptor_sha256: String,
    line_authority_sha256: String,
    geometry_review_sha256: String,
    source_return_year: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SummaryFigureInput {
    pub test_copy: bool,
    pub profile_sha256: String,
    pub record: Value,
}
fn digest(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}
fn profile() -> &'static Profile {
    static P: OnceLock<Profile> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("source_summary_figure_profile.json");
        assert_eq!(digest(raw), PROFILE_SHA);
        let p: Profile = serde_json::from_slice(raw).expect("closed summary figure profile");
        assert_eq!(
            digest(include_bytes!("source_summary_line_authority.json")),
            p.line_authority_sha256
        );
        p
    })
}
fn key(c: &str) -> String {
    format!("dtmaxtc:{c}:source_summary_figures:{PROFILE_SHA}:record")
}
fn encoded(v: &Value) -> Result<String> {
    let raw = serde_json::to_string(v).map_err(|_| bad("Cannot encode source summary"))?;
    let p = profile();
    if raw.len() > 32768
        || digest(raw.as_bytes()) != p.allowed_record_sha256
        || v["source_return_year"] != p.source_return_year
        || v["source_geometry_sha256"] != p.geometry_review_sha256
        || v["printed_line_authority_sha256"] != p.line_authority_sha256
    {
        return Err(bad(
            "Summary observations differ from exact closed source evidence",
        ));
    }
    Ok(raw)
}
fn envelope(c: &str) -> Value {
    json!({"test_copy":true,"source_scope":c,"summary_figure_profile_sha256":PROFILE_SHA,"native_contexts_created":0,"current_profiles_modified":0,"original_PDFs_modified":0,"earlier_figure_records_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"as_filed_verified":false,"full_T1_recovered":false})
}
impl SourceImport {
    async fn stored_summary_figure(&self, company: &str) -> Result<Option<Value>> {
        let Some(raw) = self
            .store
            .get(&key(company))
            .await
            .map_err(|e| storage(&e))?
        else {
            return Ok(None);
        };
        let v: Value = serde_json::from_str(&raw)
            .map_err(|_| storage("Invalid stored summary observation"))?;
        if encoded(&v).map_err(|_| storage("Stored summary observation differs"))? != raw {
            return Err(storage("Stored summary encoding differs"));
        }
        Ok(Some(v))
    }
    pub async fn test_copy_source_summary_figures(
        &self,
        company: &str,
        input: SummaryFigureInput,
    ) -> Result<Value> {
        self.test_copy_guard(company, input.test_copy)?;
        if input.profile_sha256 != PROFILE_SHA {
            return Err(bad("Summary profile differs"));
        }
        let raw = encoded(&input.record)?;
        let p = profile();
        let client = input.record["client_number"]
            .as_str()
            .ok_or_else(|| bad("Summary owner missing"))?;
        if self
            .store
            .get(&format!("dtmaxtc:{company}:client:{}", client))
            .await
            .map_err(|e| storage(&e))?
            .is_none()
        {
            return Err(bad("Summary owner is not registered"));
        }
        let response = self
            .test_copy_return_pdf_content(
                company,
                &client,
                p.source_return_year,
                &p.source_pdf_sha256,
            )
            .await?;
        let descriptor = serde_json::to_vec(&response["record"])
            .map_err(|_| bad("Cannot encode summary PDF descriptor"))?;
        if response["record"] != input.record["pdf"]
            || digest(&descriptor) != p.source_descriptor_sha256
        {
            return Err(bad("Summary PDF provenance or owner-year differs"));
        }
        // Three independently hashed original pieces are read before admission.
        let mut whole = Sha256::new();
        let mut bytes = 0;
        for i in 0..3 {
            let part = self
                .test_copy_source_archive_chunk(
                    company,
                    &client,
                    p.source_return_year,
                    &p.source_pdf_sha256,
                    i,
                )
                .await?;
            let raw = hex::decode(
                part["content_hex"]
                    .as_str()
                    .ok_or_else(|| storage("Summary piece missing"))?,
            )
            .map_err(|_| storage("Summary piece invalid"))?;
            bytes += raw.len();
            whole.update(raw);
        }
        if hex::encode(whole.finalize()) != p.source_pdf_sha256
            || input.record["pdf"]["bytes"] != bytes
        {
            return Err(bad("Summary original content differs"));
        }
        let k = key(company);
        if self
            .store
            .get(&k)
            .await
            .map_err(|e| storage(&e))?
            .is_some_and(|old| old != raw)
        {
            return Err(conflict("Summary observation is immutable"));
        }
        let created = self.store.set_nx(&k, &raw).await.map_err(|e| storage(&e))?;
        if self
            .store
            .get(&k)
            .await
            .map_err(|e| storage(&e))?
            .as_deref()
            != Some(raw.as_str())
        {
            return Err(conflict("Summary observation is immutable"));
        }
        let mut out = envelope(company);
        out["created"] = json!(usize::from(created));
        out["unchanged_replays"] = json!(usize::from(!created));
        out["record_sha256"] = json!(p.allowed_record_sha256);
        Ok(out)
    }
    pub async fn test_copy_client_source_summary_figures(
        &self,
        company: &str,
        client: &str,
        page: SourceFactPage,
    ) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        if client.is_empty()
            || client.len() > 12
            || !client.bytes().all(|b| b.is_ascii_digit())
            || page.limit == 0
            || page.limit > 100
        {
            return Err(bad("Invalid summary client or page bound"));
        }
        let mut rows = Vec::new();
        let p = profile();
        if page
            .source_return_year
            .is_none_or(|y| y == p.source_return_year)
        {
            if let Some(v) = self.stored_summary_figure(company).await? {
                if v["client_number"] == client {
                    rows.push(v)
                }
            }
        }
        let total = rows.len();
        let records: Vec<_> = rows
            .into_iter()
            .skip(page.offset)
            .take(page.limit)
            .collect();
        let mut out = envelope(company);
        out["client_number"] = json!(client);
        out["kind"] = json!("source-summary-figures");
        out["source_return_year"] = json!(page.source_return_year);
        out["offset"] = json!(page.offset);
        out["total"] = json!(total);
        out["records"] = json!(records);
        Ok(out)
    }
    pub async fn test_copy_source_summary_figure_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let count = usize::from(self.stored_summary_figure(company).await?.is_some());
        let mut out = envelope(company);
        out["PDF_observation_records"] = json!(count);
        out["printed_amount_observations"] = json!(count * 5);
        out["requested_lines_not_printed"] = json!(count);
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_source_shape_and_all_six_slots_remain_exact() {
        let Ok(path) = std::env::var("NXT_SUMMARY_FIGURE_OBSERVATION") else {
            return;
        };
        let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        encoded(&v).unwrap();
        assert_eq!(v["observations"].as_array().unwrap().len(), 6);
        assert_eq!(v["observations"][4]["state"], "requested_line_not_printed");
        assert!(v["observations"][4]["printed_cents"].is_null());
        for field in [
            "native_context_state",
            "client_number",
            "source_return_year",
            "source_geometry_sha256",
            "financial_values_approved",
            "pdf",
            "observations",
        ] {
            let mut bad = v.clone();
            bad[field] = Value::Null;
            assert!(encoded(&bad).is_err(), "unclosed source field admitted");
        }
    }
    #[test]
    fn closed_profile_does_not_modify_earlier_observation_profiles() {
        profile();
        assert_eq!(
            digest(include_bytes!("preserved_pdf_figure_profile.json")),
            "f296eec4a3f76f9956c1c563fa9ac2742c9702412e39020c0f5c0b04659e1267"
        );
        assert_eq!(
            digest(include_bytes!("printed_cached_figure_profile.json")),
            "f3593686959075ed6dd0c7224fd88d5b4e20ab6ca732519d166f4ab6fd4cda95"
        );
    }
}
