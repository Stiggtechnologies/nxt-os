//! Stored-preview consistency, not a signature or independent source attestation.
use super::{EResult, EngagementRecord, ImportPreview, conflict, hash};
use serde_json::json;

pub(super) fn digest(preview: &ImportPreview) -> EResult<String> {
    hash(&json!({
        "format": "nxt-engagement-preview-binding/v1",
        "id": preview.id,
        "engagement_id": preview.engagement_id,
        "expected_revision": preview.expected_revision,
        "target_input_sha256": preview.target_input_sha256,
        "source_hash": preview.source_hash,
        "canonical_hash": preview.canonical_hash,
        "created_at": preview.created_at,
        "expires_at": preview.expires_at,
        "summary": preview.summary,
        "warnings": preview.warnings,
        "accounts": preview.accounts,
        "checks": preview.checks,
        "provenance": preview.provenance,
    }))
}

pub(super) fn verify(preview: &ImportPreview) -> EResult<()> {
    let stored = preview.binding_sha256.as_deref().ok_or_else(|| {
        conflict("this legacy preview has no complete binding; generate a fresh preview before committing")
    })?;
    if !preview.target_input_sha256.as_ref().is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    }) {
        return Err(conflict(
            "preview target-input binding is missing or invalid; generate a fresh preview",
        ));
    }
    if stored != digest(preview)? {
        return Err(conflict(
            "stored preview metadata or checks changed; recovery or a fresh source preview is required",
        ));
    }
    Ok(())
}

pub(super) fn verify_receipt(preview: &ImportPreview, record: &EngagementRecord) -> EResult<()> {
    let matching = record
        .import_history
        .iter()
        .filter(|receipt| receipt.preview_id == preview.id)
        .collect::<Vec<_>>();
    let committed = preview
        .committed_revision
        .ok_or_else(|| conflict("preview is not committed"))?;
    if record.id != preview.engagement_id
        || matching.len() != 1
        || committed <= preview.expected_revision
        || committed > record.revision
    {
        return Err(conflict(
            "committed preview has no consistent retained receipt; recovery required",
        ));
    }
    let receipt = matching[0];
    if receipt.source_hash != preview.source_hash
        || receipt.canonical_hash != preview.canonical_hash
        || receipt.provenance != preview.provenance
        || receipt.preview_binding_sha256 != preview.binding_sha256
        || receipt.input_revision == 0
        || receipt.input_revision > record.input_revision
    {
        return Err(conflict(
            "committed preview differs from its retained import receipt; recovery required",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engagement::{ImportReceipt, models::Account};

    fn must<T>(result: EResult<T>) -> T {
        result.unwrap_or_else(|error| panic!("{}", error.1))
    }

    fn fixture() -> ImportPreview {
        serde_json::from_value(json!({
            "id":"engpreview_synthetic", "engagement_id":"eng_synthetic", "expected_revision":1,
            "source_hash":"a".repeat(64), "canonical_hash":must(hash(&Vec::<Account>::new())),
            "target_input_sha256":"d".repeat(64),
            "created_at":"2026-10-26T00:00:00Z", "expires_at":"2026-10-26T00:30:00Z",
            "summary":{"accounts":0}, "warnings":["Synthetic review required"], "accounts":[],
            "checks":{"supported":false,"diagnostics":[]}, "committed_revision":null,
            "provenance":{"source":"synthetic_native_json"}
        }))
        .unwrap()
    }

    #[test]
    fn complete_preview_binding_covers_metadata_and_checks_without_freezing_commit_marker() {
        let mut preview = fixture();
        assert!(
            serde_json::to_value(&preview)
                .unwrap()
                .get("binding_sha256")
                .is_none()
        );
        assert!(verify(&preview).is_err());
        preview.binding_sha256 = Some(must(digest(&preview)));
        must(verify(&preview));
        let baseline = serde_json::to_value(&preview).unwrap();
        for field in [
            "id",
            "engagement_id",
            "expected_revision",
            "target_input_sha256",
            "source_hash",
            "canonical_hash",
            "created_at",
            "expires_at",
            "summary",
            "warnings",
            "accounts",
            "checks",
            "provenance",
        ] {
            let mut changed = baseline.clone();
            match field {
                "expected_revision" => changed[field] = json!(2),
                "summary" => changed[field] = json!({"accounts":999}),
                "warnings" => changed[field] = json!([]),
                "accounts" => {
                    changed[field] =
                        json!([{"id":"a1","code":"001","name":"Synthetic","kind":"asset"}])
                }
                "checks" => changed[field] = json!({"supported":true,"diagnostics":[]}),
                "provenance" => changed[field] = json!({"source":"changed_synthetic_source"}),
                _ => changed[field] = json!("changed"),
            }
            let decoded: ImportPreview = serde_json::from_value(changed).unwrap();
            assert!(
                verify(&decoded).is_err(),
                "{field} must participate in the stored binding"
            );
        }
        preview.committed_revision = Some(2);
        must(verify(&preview));
    }

    #[test]
    fn replay_requires_exact_receipt_and_does_not_fabricate_legacy_binding() {
        let mut preview = fixture();
        preview.binding_sha256 = Some(must(digest(&preview)));
        preview.committed_revision = Some(2);
        let receipt = ImportReceipt {
            preview_id: preview.id.clone(),
            source_hash: preview.source_hash.clone(),
            canonical_hash: preview.canonical_hash.clone(),
            provenance: preview.provenance.clone(),
            input_revision: 2,
            at: "2026-10-26T00:01:00Z".into(),
            preview_binding_sha256: preview.binding_sha256.clone(),
        };
        let mut record:EngagementRecord=serde_json::from_value(json!({
            "id":"eng_synthetic","revision":2,"input_revision":2,"status":"draft",
            "document":{"name":"Synthetic","period_start":"2025-01-01","period_end":"2025-12-31"},
            "snapshot":null,"snapshots":[],"signoffs":[],"signoff_history":[],"lock_history":[],
            "import_history":[receipt],"created_at":"2026-10-26T00:00:00Z","updated_at":"2026-10-26T00:01:00Z","rollforward_from":null
        })).unwrap();
        must(verify_receipt(&preview, &record));
        record.import_history[0].source_hash = "b".repeat(64);
        assert!(verify_receipt(&preview, &record).is_err());
        record.import_history[0].source_hash = preview.source_hash.clone();
        record.import_history[0].preview_binding_sha256 = None;
        assert!(verify_receipt(&preview, &record).is_err());
        preview.binding_sha256 = None;
        preview.target_input_sha256 = None;
        must(verify_receipt(&preview, &record));
        assert!(
            verify(&preview).is_err(),
            "A legacy receipt is not converted into a new binding"
        );
        record.import_history.push(record.import_history[0].clone());
        assert!(verify_receipt(&preview, &record).is_err());
    }
}
