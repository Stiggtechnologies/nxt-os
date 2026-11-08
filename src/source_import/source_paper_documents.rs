//! Closed printed supporting-document observations. Every copy and unavailable field stays distinct.
use super::preserved_records::SourceFactPage;
use super::{ImportError, SourceImport, bad, conflict, storage};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
type Result<T> = std::result::Result<T, ImportError>;
pub const PROFILE_SHA: &str = "efa1b7bb15c311ce9755863c165a25ce8954e6ced2f143d9f466b7877afb7a06";
#[derive(Deserialize)]
struct Profile {
    allowed_record_sha256: String,
    #[serde(rename = "source_PDF_sha256")]
    source_pdf_sha256: String,
    source_descriptor_sha256: String,
    source_review_sha256: String,
    source_archive_year: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperDocumentInput {
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
        let raw = include_bytes!("supporting_document_observation_profile.json");
        assert_eq!(digest(raw), PROFILE_SHA);
        let p: Profile = serde_json::from_slice(raw).expect("closed supporting-document profile");
        p
    })
}
fn key(c: &str) -> String {
    format!("dtmaxtc:{c}:source_paper_documents:{PROFILE_SHA}:record")
}
fn encoded(v: &Value) -> Result<String> {
    let raw = serde_json::to_string(v).map_err(|_| bad("Cannot encode supporting documents"))?;
    let p = profile();
    if raw.len() > 32768
        || digest(raw.as_bytes()) != p.allowed_record_sha256
        || v["source_archive_year"] != p.source_archive_year
        || v["source_review_sha256"] != p.source_review_sha256
    {
        return Err(bad(
            "Supporting document observations differ from exact closed source evidence",
        ));
    }
    Ok(raw)
}
fn envelope(c: &str) -> Value {
    json!({"test_copy":true,"source_scope":c,"paper_document_profile_sha256":PROFILE_SHA,"native_contexts_created":0,"current_profiles_modified":0,"original_PDFs_modified":0,"earlier_figure_records_modified":0,"tax_cases_imported":0,"financial_values_approved":false,"as_filed_verified":false,"full_T1_recovered":false,"native_slips_created":0,"economic_slips_created":0})
}
impl SourceImport {
    async fn stored_paper_document(&self, company: &str) -> Result<Option<Value>> {
        let Some(raw) = self
            .store
            .get(&key(company))
            .await
            .map_err(|e| storage(&e))?
        else {
            return Ok(None);
        };
        let v: Value = serde_json::from_str(&raw)
            .map_err(|_| storage("Invalid stored supporting document observation"))?;
        if encoded(&v).map_err(|_| storage("Stored supporting document observation differs"))?
            != raw
        {
            return Err(storage("Stored supporting document encoding differs"));
        }
        Ok(Some(v))
    }
    pub async fn test_copy_source_paper_documents(
        &self,
        company: &str,
        input: PaperDocumentInput,
    ) -> Result<Value> {
        self.test_copy_guard(company, input.test_copy)?;
        if input.profile_sha256 != PROFILE_SHA {
            return Err(bad("Supporting document profile differs"));
        }
        let raw = encoded(&input.record)?;
        let p = profile();
        let client = input.record["client_number"]
            .as_str()
            .ok_or_else(|| bad("Supporting document owner missing"))?;
        if self
            .store
            .get(&format!("dtmaxtc:{company}:client:{}", client))
            .await
            .map_err(|e| storage(&e))?
            .is_none()
        {
            return Err(bad("Supporting document owner is not registered"));
        }
        let response = self
            .test_copy_return_pdf_content(
                company,
                &client,
                p.source_archive_year,
                &p.source_pdf_sha256,
            )
            .await?;
        let descriptor = serde_json::to_vec(&response["record"])
            .map_err(|_| bad("Cannot encode supporting document PDF descriptor"))?;
        if response["record"] != input.record["pdf"]
            || digest(&descriptor) != p.source_descriptor_sha256
        {
            return Err(bad(
                "Supporting document PDF provenance or owner-year differs",
            ));
        }
        // Three independently hashed original pieces are read before admission.
        let mut whole = Sha256::new();
        let mut bytes = 0;
        for i in 0..3 {
            let part = self
                .test_copy_source_archive_chunk(
                    company,
                    &client,
                    p.source_archive_year,
                    &p.source_pdf_sha256,
                    i,
                )
                .await?;
            let raw = hex::decode(
                part["content_hex"]
                    .as_str()
                    .ok_or_else(|| storage("Supporting document piece missing"))?,
            )
            .map_err(|_| storage("Supporting document piece invalid"))?;
            bytes += raw.len();
            whole.update(raw);
        }
        if hex::encode(whole.finalize()) != p.source_pdf_sha256
            || input.record["pdf"]["bytes"] != bytes
        {
            return Err(bad("Supporting document original content differs"));
        }
        let k = key(company);
        if self
            .store
            .get(&k)
            .await
            .map_err(|e| storage(&e))?
            .is_some_and(|old| old != raw)
        {
            return Err(conflict("Supporting document observation is immutable"));
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
            return Err(conflict("Supporting document observation is immutable"));
        }
        let mut out = envelope(company);
        out["created"] = json!(usize::from(created));
        out["unchanged_replays"] = json!(usize::from(!created));
        out["record_sha256"] = json!(p.allowed_record_sha256);
        Ok(out)
    }
    pub async fn test_copy_client_source_paper_documents(
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
            return Err(bad("Invalid supporting-document client or page bound"));
        }
        let mut rows = Vec::new();
        let p = profile();
        if page
            .source_return_year
            .is_none_or(|y| y == p.source_archive_year)
        {
            if let Some(v) = self.stored_paper_document(company).await? {
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
        out["kind"] = json!("source-paper-documents");
        out["source_return_year"] = json!(page.source_return_year);
        out["offset"] = json!(page.offset);
        out["total"] = json!(total);
        out["records"] = json!(records);
        Ok(out)
    }
    pub async fn test_copy_source_paper_document_summary(&self, company: &str) -> Result<Value> {
        self.test_copy_guard(company, true)?;
        let count = usize::from(self.stored_paper_document(company).await?.is_some());
        let mut out = envelope(company);
        out["PDF_observation_records"] = json!(count);
        out["printed_document_groups"] = json!(count * 5);
        out["physical_printed_copies"] = json!(count * 11);
        out["physical_money_fields"] = json!(count * 63);
        out["printed_amount_observations"] = json!(count * 27);
        out["explicit_printed_zeros"] = json!(count * 2);
        out["identified_printed_blanks"] = json!(count * 34);
        out["fields_not_fully_visible"] = json!(count * 2);
        out["RRSP_taxation_year_assigned"] = json!(false);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_supporting_catalog_preserves_all_earlier_figure_profiles() {
        profile();
        assert_eq!(
            digest(include_bytes!("source_summary_figure_profile.json")),
            "7d678108f4ba7c766029e6cdee96e4b270f1a20df7aa76d5e1f99a2cc0b11c91"
        );
        assert_eq!(
            digest(include_bytes!("preserved_pdf_figure_profile.json")),
            "f296eec4a3f76f9956c1c563fa9ac2742c9702412e39020c0f5c0b04659e1267"
        );
        assert_eq!(
            digest(include_bytes!("printed_cached_figure_profile.json")),
            "f3593686959075ed6dd0c7224fd88d5b4e20ab6ca732519d166f4ab6fd4cda95"
        );
    }
    #[test]
    fn private_physical_copies_blanks_crops_and_unprinted_receipt_year_are_exact() {
        let Ok(path) = std::env::var("NXT_PAPER_DOCUMENT_OBSERVATIONS") else {
            return;
        };
        let v: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        encoded(&v).unwrap();
        let docs = v["documents"].as_array().unwrap();
        assert_eq!(docs.len(), 5);
        let mut copies = 0;
        let (mut fields, mut present, mut blanks, mut cropped, mut zeros) = (0, 0, 0, 0, 0);
        for doc in docs {
            if doc["printed_document_type"] == "RRSP_CONTRIBUTION_RECEIPT" {
                assert!(doc["printed_tax_year"].is_null());
                assert_eq!(
                    doc["printed_tax_year_state"],
                    "taxation_year_field_unfilled"
                );
                assert_eq!(doc["metadata"]["deduction_year_inferred"], false);
            }
            for c in doc["physical_printed_copies"].as_array().unwrap() {
                copies += 1;
                for f in c["financial_fields"].as_array().unwrap() {
                    fields += 1;
                    match f["state"].as_str().unwrap() {
                        "printed_amount_present" => {
                            present += 1;
                            if f["printed_cents"] == "0" {
                                zeros += 1;
                            }
                        }
                        "identified_printed_blank" => {
                            blanks += 1;
                            assert!(f["printed_cents"].is_null());
                        }
                        "field_not_fully_visible" => {
                            cropped += 1;
                            assert!(f["printed_cents"].is_null());
                        }
                        _ => panic!("Unexpected closed source state"),
                    }
                }
            }
        }
        assert_eq!(
            (copies, fields, present, blanks, cropped, zeros),
            (11, 63, 27, 34, 2, 2)
        );
        for field in [
            "client_number",
            "source_archive_year",
            "documents",
            "pdf",
            "source_review_sha256",
            "financial_values_approved",
        ] {
            let mut wrong = v.clone();
            wrong[field] = Value::Null;
            assert!(encoded(&wrong).is_err());
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/paper_source_replay.rs"]
mod source_replay_tests;
