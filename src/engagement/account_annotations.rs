//! Original NXT local account-context policy; no source applications or Ledger I/O.
use super::models::{Annotation, Workpaper};
use super::models::{EngagementInput, WorkpaperType};
use super::storage_models::{AnnotationBindingReceipt, EngagementRecord};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTEXT_FORMAT: &str = "nxt-account-annotation-context/v1";
pub const OPTIONS_FORMAT: &str = "nxt-account-annotation-options/v1";
pub const BINDING_FORMAT: &str = "nxt-account-annotation-binding/v1";
pub const REGISTRY_MAX: usize = 2000;
pub const SCOPE: &str = "Original NXT local account context plus exact manual workpaper text; not a cell/PDF annotation, assurance conclusion, authenticated identity or verified Caseware behavior. Context includes owning engagement ID, native ruleset, period/currency, paper ID/reference, selected complete account and linked group definition (not group aggregates), and complete touching local adjustments in saved order. No Ledger/Tax reads or postings.";

fn account_binding(
    owner: &str,
    paper: &Workpaper,
    annotation: &Annotation,
) -> Result<Option<String>, String> {
    let Some(anchor) = &annotation.account_anchor else {
        return Ok(None);
    };
    let value = json!({"format":BINDING_FORMAT,"engagement_id":owner,"annotation_id":annotation.id,"workpaper_id":paper.id,"kind":annotation.kind,"custom_mark":annotation.custom_mark,"account_anchor":anchor,"body_sha256":annotation.body_sha256});
    let bytes =
        serde_json::to_vec(&value).map_err(|_| "Cannot encode immutable annotation binding")?;
    Ok(Some(hex::encode(Sha256::digest(bytes))))
}
fn owned_documents(record: &EngagementRecord) -> Vec<&EngagementInput> {
    let mut docs = vec![&record.document];
    if let Some(snapshot) = &record.snapshot {
        docs.push(&snapshot.input_document);
    }
    docs.extend(record.snapshots.iter().map(|s| &s.input_document));
    // An origin is owned by its source engagement, not this target registry.
    docs
}
fn scoped_binding(
    owner: &str,
    paper: &Workpaper,
    annotation: &Annotation,
) -> Result<(Option<String>, Option<String>), String> {
    if annotation.account_anchor.is_some() && annotation.text_range_anchor.is_some() {
        return Err("Account and text range scopes are mutually exclusive".into());
    }
    Ok((
        account_binding(owner, paper, annotation)?,
        super::text_ranges::binding(owner, paper, annotation)?,
    ))
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn validate_registry(record: &EngagementRecord) -> Result<(), String> {
    if record.annotation_bindings.len() > REGISTRY_MAX {
        return Err("Lifetime annotation ID capacity exceeded (2000)".into());
    }
    let mut known = BTreeMap::new();
    for receipt in &record.annotation_bindings {
        if !identifier(&receipt.annotation_id)
            || receipt
                .account_binding_sha256
                .as_ref()
                .is_some_and(|h| !digest(h))
            || receipt
                .text_range_binding_sha256
                .as_ref()
                .is_some_and(|h| !digest(h))
            || (receipt.account_binding_sha256.is_some()
                && receipt.text_range_binding_sha256.is_some())
            || known
                .insert(
                    receipt.annotation_id.as_str(),
                    (
                        receipt.account_binding_sha256.as_ref(),
                        receipt.text_range_binding_sha256.as_ref(),
                    ),
                )
                .is_some()
        {
            return Err("Invalid or duplicate lifetime annotation binding receipt".into());
        }
    }
    for doc in owned_documents(record) {
        for paper in &doc.workpapers {
            for annotation in &paper.annotations {
                let binding = scoped_binding(&record.id, paper, annotation)?;
                match known.get(annotation.id.as_str()) {
                    Some(known) if *known != (binding.0.as_ref(),binding.1.as_ref()) => return Err("Annotation ID differs from its lifetime scoped binding or body-only reservation".into()),
                    None if binding.0.is_some()||binding.1.is_some() => return Err("Scoped annotation is missing its lifetime binding receipt".into()),
                    _ => {}, // Registry-less body-only marks remain loadable legacy inputs.
                }
            }
        }
    }
    Ok(())
}
fn remember(
    record: &mut EngagementRecord,
    annotation_id: &str,
    binding: (Option<String>, Option<String>),
) -> Result<(), String> {
    if let Some(prior) = record
        .annotation_bindings
        .iter()
        .find(|r| r.annotation_id == annotation_id)
    {
        if prior.account_binding_sha256 != binding.0 || prior.text_range_binding_sha256 != binding.1
        {
            return Err("Annotation ID has a lifetime account binding or body-only reservation; use a new ID to change scope, text binding, target, owner, paper, kind or custom meaning".into());
        }
        return Ok(());
    }
    if record.annotation_bindings.len() >= REGISTRY_MAX {
        return Err("Lifetime annotation ID capacity reached (2000); preserve this engagement and continue separately".into());
    }
    record.annotation_bindings.push(AnnotationBindingReceipt {
        annotation_id: annotation_id.into(),
        account_binding_sha256: binding.0,
        text_range_binding_sha256: binding.1,
    });
    Ok(())
}
/// Reserve known old body-only IDs before a save can remove them. Account marks
/// without a receipt are rejected by validate_registry, never silently seeded.
pub(super) fn seed_legacy_body_only(record: &mut EngagementRecord) -> Result<(), String> {
    let ids: Vec<_> = owned_documents(record)
        .iter()
        .flat_map(|d| &d.workpapers)
        .flat_map(|p| &p.annotations)
        .filter(|a| a.account_anchor.is_none() && a.text_range_anchor.is_none())
        .map(|a| a.id.clone())
        .collect();
    for id in ids {
        remember(record, &id, (None, None))?;
    }
    Ok(())
}
pub(super) fn register_saved_document(record: &mut EngagementRecord) -> Result<(), String> {
    let bindings = record
        .document
        .workpapers
        .iter()
        .flat_map(|p| p.annotations.iter().map(move |a| (p, a)))
        .map(|(p, a)| Ok((a.id.clone(), scoped_binding(&record.id, p, a)?)))
        .collect::<Result<Vec<_>, String>>()?;
    for (id, binding) in bindings {
        remember(record, &id, binding)?;
    }
    Ok(())
}

pub(super) fn validate_owners(owner: &str, document: &EngagementInput) -> Result<(), String> {
    if document
        .workpapers
        .iter()
        .flat_map(|p| &p.annotations)
        .any(|a| {
            a.account_anchor
                .as_ref()
                .is_some_and(|v| v.engagement_id != owner)
                || a.text_range_anchor
                    .as_ref()
                    .is_some_and(|v| v.engagement_id != owner)
        })
    {
        return Err("Account annotation owner differs from its engagement; acquire new saved options for this engagement".into());
    }
    Ok(())
}

/// Hash a sorted-key UTF-8 JSON Value. Full touching adjustments retain both their
/// original order and every line, including counterpart lines for other accounts.
/// Workpaper body is deliberately excluded: the annotation binds it separately.
pub fn context_sha256(
    input: &EngagementInput,
    engagement_id: &str,
    paper_id: &str,
    account_id: &str,
) -> Result<String, String> {
    let paper = input
        .workpapers
        .iter()
        .find(|p| p.id == paper_id)
        .ok_or("Unknown account-annotation workpaper")?;
    if paper.document_type != WorkpaperType::Manual {
        return Err("Account annotations require a manual workpaper".into());
    }
    if !paper.account_ids.iter().any(|id| id == account_id) {
        return Err("Account annotation must target an account referenced by its workpaper".into());
    }
    let account = input
        .accounts
        .iter()
        .find(|a| a.id == account_id)
        .ok_or("Unknown account-annotation account")?;
    let group = match account.group_id.as_ref() {
        Some(id) => Some(
            input
                .groups
                .iter()
                .find(|g| &g.id == id)
                .ok_or("Unknown account-annotation group")?,
        ),
        None => None,
    };
    let adjustments: Vec<_> = input
        .adjustments
        .iter()
        .filter(|a| a.lines.iter().any(|line| line.account_id == account_id))
        .collect();
    let context = json!({
        "format": CONTEXT_FORMAT,
        "ruleset": super::analysis::RULESET,
        "engagement_id": engagement_id,
        "currency": input.currency,
        "period_start": input.period_start,
        "period_end": input.period_end,
        "prior_period_start": input.prior_period_start,
        "prior_period_end": input.prior_period_end,
        "workpaper": {"id": paper.id, "reference": paper.reference},
        "account": account,
        "group": group,
        "adjustments": adjustments,
    });
    let bytes =
        serde_json::to_vec(&context).map_err(|_| "Cannot encode account-annotation context")?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
