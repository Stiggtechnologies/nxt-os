//! Original bounded file preservation. Bytes are never loaded as code or rendered.
use super::attachment_models::*;
use super::*;
use chrono::DateTime;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const FILE_MAX: usize = 256 * 1024;
pub(super) const TOTAL_MAX: usize = 2 * 1024 * 1024;
const VERSIONS_MAX: usize = 128;
const PAPER_MAX: usize = 16;

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn file_name(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && value.len() <= 160
        && ![".", ".."].contains(&value)
        && !value
            .chars()
            .any(|c| c.is_control() || "/\\:\"".contains(c))
}
fn inspect_bytes(bytes: &[u8]) -> (&'static str, Option<u32>, Option<u32>, Option<u32>) {
    if bytes.starts_with(b"%PDF") {
        let text = String::from_utf8_lossy(bytes);
        let pages = text.matches("/Type /Page").count().saturating_sub(text.matches("/Type /Pages").count()) as u32;
        return ("pdf", None, None, Some(pages));
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) && bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return ("png", Some(width), Some(height), None);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let mut index = 2;
        while index + 9 < bytes.len() {
            if bytes[index] != 0xFF { break; }
            let marker = bytes[index + 1];
            if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 {
                let height = u32::from(u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]));
                let width = u32::from(u16::from_be_bytes([bytes[index + 7], bytes[index + 8]]));
                return ("jpeg", Some(width), Some(height), None);
            }
            if index + 3 >= bytes.len() { break; }
            let length = u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]) as usize;
            if length < 2 || index + 2 + length > bytes.len() { break; }
            index += 2 + length;
        }
        return ("jpeg", None, None, None);
    }
    ("opaque", None, None, None)
}
fn content(value: &str) -> EResult<Vec<u8>> {
    if value.is_empty()
        || value.len() > FILE_MAX * 2
        || value.len() % 2 != 0
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(bad(
            "file bytes must be lowercase hex for a nonempty file of at most 256 KiB",
        ));
    }
    hex::decode(value).map_err(|_| bad("invalid file encoding"))
}
fn request_hash(version: &AttachmentVersion) -> EResult<String> {
    hash(
        &json!({"format":"nxt-file-upload/v1","engagement_id":version.engagement_id,
        "workpaper_id":version.workpaper_id,"expected_revision":version.upload_expected_revision,
        "upload_id":version.upload_id,"file_name":version.file_name,"operator_label":version.operator_label,
        "note":version.note,"sha256":version.sha256,"bytes":version.bytes}),
    )
}
fn metadata_hash(version: &AttachmentVersion) -> EResult<String> {
    let mut copy = version.clone();
    copy.metadata_sha256.clear();
    hash(&json!({"format":"nxt-file-version/v1","version":copy}))
}
pub(super) fn validate_paper_shape(paper: &Workpaper) -> EResult<()> {
    super::analysis::validate_document_shape(paper).map_err(bad)?;
    for annotation in &paper.annotations {
        super::text_ranges::validate(paper, annotation).map_err(bad)?;
    }
    if paper.document_type == WorkpaperType::Placeholder
        && (!paper.body.is_empty() || !paper.annotations.is_empty() || !paper.attachment_versions.is_empty())
    {
        return Err(bad("Placeholder workpapers cannot retain body evidence, annotations or file versions"));
    }
    if paper.attachment_versions.len() > PAPER_MAX {
        return Err(bad("workpaper exceeds 16 retained file versions"));
    }
    let mut ids = BTreeSet::new();
    let mut uploads = BTreeSet::new();
    for (index, v) in paper.attachment_versions.iter().enumerate() {
        if !identifier(&v.id)
            || !identifier(&v.engagement_id)
            || !identifier(&v.workpaper_id)
            || v.workpaper_id != paper.id
            || !identifier(&v.upload_id)
            || v.version != index as u32 + 1
            || v.upload_expected_revision == 0
            || !file_name(&v.file_name)
            || v.bytes == 0
            || v.bytes > FILE_MAX
            || !digest(&v.sha256)
            || DateTime::parse_from_rfc3339(&v.uploaded_at).is_err()
            || !ids.insert(&v.id)
            || !uploads.insert(&v.upload_id)
        {
            return Err(bad("invalid file-version identity, order or metadata"));
        }
        label(&v.operator_label, "operator label", 120)?;
        label(&v.note, "attachment note", 4000)?;
        if v.request_sha256 != request_hash(v)? || v.metadata_sha256 != metadata_hash(v)? {
            return Err(conflict(
                "file-version metadata differs from its stored binding",
            ));
        }
    }
    Ok(())
}
fn documents(record: &EngagementRecord) -> Vec<(&str, &EngagementInput)> {
    let mut out = vec![(record.id.as_str(), &record.document)];
    if let Some(snapshot) = &record.snapshot {
        out.push((record.id.as_str(), &snapshot.input_document));
    }
    out.extend(
        record
            .snapshots
            .iter()
            .map(|s| (record.id.as_str(), &s.input_document)),
    );
    if let Some(origin) = &record.rollforward_origin {
        out.push((
            &origin.source_engagement_id,
            &origin.source_snapshot.input_document,
        ));
    }
    out
}
pub(super) fn validate_state(state: &EngagementState) -> EResult<()> {
    if state.attachment_blobs.len() > VERSIONS_MAX {
        return Err(conflict("file blob capacity exceeded"));
    }
    let mut blobs = BTreeMap::new();
    let mut total = 0usize;
    for blob in &state.attachment_blobs {
        let bytes = content(&blob.content_hex)
            .map_err(|_| conflict("stored file bytes are invalid; recovery required"))?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| conflict("file capacity overflow"))?;
        if !identifier(&blob.engagement_id)
            || !digest(&blob.sha256)
            || total > TOTAL_MAX
            || hex::encode(Sha256::digest(&bytes)) != blob.sha256
            || blobs
                .insert(
                    (blob.engagement_id.as_str(), blob.sha256.as_str()),
                    bytes.len(),
                )
                .is_some()
        {
            return Err(conflict(
                "stored file identity, content or capacity is invalid; recovery required",
            ));
        }
    }
    let mut versions = BTreeMap::new();
    let mut uploads = BTreeMap::new();
    let mut referenced = BTreeSet::new();
    for record in &state.engagements {
        if !identifier(&record.id) {
            return Err(conflict("stored engagement identity is invalid"));
        }
        for (owner, doc) in documents(record) {
            account_annotations::validate_owners(owner, doc).map_err(|_| conflict("stored account-annotation owner is invalid; recovery required"))?;
            for paper in &doc.workpapers {
                validate_paper_shape(paper).map_err(|_| {
                    conflict("stored file-version metadata is invalid; recovery required")
                })?;
                for v in &paper.attachment_versions {
                    let key = (owner, v.sha256.as_str());
                    if v.engagement_id != owner || blobs.get(&key) != Some(&v.bytes) {
                        return Err(conflict(
                            "referenced file bytes are unavailable or mismatched; recovery required",
                        ));
                    }
                    for (map, key) in [
                        (&mut versions, v.id.as_str()),
                        (&mut uploads, v.upload_id.as_str()),
                    ] {
                        if map
                            .insert(key, v.metadata_sha256.as_str())
                            .is_some_and(|prior| prior != v.metadata_sha256)
                        {
                            return Err(conflict(
                                "file-version identity was reused with different metadata",
                            ));
                        }
                    }
                    referenced.insert(key);
                }
            }
        }
    }
    for record in &state.engagements {
        account_annotations::validate_registry(record).map_err(|reason| conflict(format!("stored annotation binding registry is invalid; recovery required: {reason}")))?;
    }
    if versions.len() > VERSIONS_MAX || referenced.len() != blobs.len() {
        return Err(conflict(
            "file-version capacity or retained-reference integrity failed",
        ));
    }
    Ok(())
}
pub(super) fn preserve_versions(old: &EngagementInput, new: &EngagementInput) -> EResult<()> {
    for paper in &old.workpapers {
        if !paper.attachment_versions.is_empty()
            && new
                .workpapers
                .iter()
                .find(|p| p.id == paper.id)
                .is_none_or(|p| p.attachment_versions != paper.attachment_versions)
        {
            return Err(conflict(
                "retain workpapers and immutable file history; upload a new version through the file endpoint",
            ));
        }
    }
    for paper in &new.workpapers {
        if !paper.attachment_versions.is_empty()
            && old
                .workpapers
                .iter()
                .find(|p| p.id == paper.id)
                .is_none_or(|p| p.attachment_versions != paper.attachment_versions)
        {
            return Err(conflict(
                "file versions can only be added through the upload endpoint",
            ));
        }
    }
    Ok(())
}

impl Engagements {
    pub async fn upload_attachment(
        &self,
        rid: &str,
        paper_id: &str,
        input: AttachmentUpload,
    ) -> EResult<EngagementRecord> {
        if !identifier(&input.upload_id)
            || !file_name(&input.file_name)
            || input.expected_revision == 0
        {
            return Err(bad("invalid upload identity, filename or revision"));
        }
        label(&input.operator_label, "operator label", 120)?;
        label(&input.note, "attachment note", 4000)?;
        let bytes = content(&input.content_hex)?;
        let mut version = AttachmentVersion {
            id: id("file"),
            version: 0,
            engagement_id: rid.into(),
            workpaper_id: paper_id.into(),
            file_name: input.file_name,
            bytes: bytes.len(),
            sha256: hex::encode(Sha256::digest(&bytes)),
            uploaded_at: now(),
            operator_label: input.operator_label,
            note: input.note,
            upload_id: input.upload_id,
            upload_expected_revision: input.expected_revision,
            request_sha256: String::new(),
            metadata_sha256: String::new(),
        };
        version.request_sha256 = request_hash(&version)?;
        self.mutate(|state| {
            let position=state.engagements.iter().position(|r|r.id==rid).ok_or_else(missing)?;
            let mut record=state.engagements[position].clone();
            if let Some(prior)=record.document.workpapers.iter().flat_map(|p|&p.attachment_versions).find(|v|v.upload_id==version.upload_id) {
                if prior.request_sha256!=version.request_sha256 {return Err(conflict("upload retry differs from its original request"));}
                return Ok(record);
            }
            check_revision(&record,input.expected_revision)?;editable(&record)?;
            let paper=record.document.workpapers.iter_mut().find(|p|p.id==paper_id).ok_or_else(||bad("unknown workpaper"))?;
            if paper.document_type != WorkpaperType::Manual {return Err(conflict("Only manual workpapers can receive file versions; retain a separate manual workpaper for placeholder or external-reference evidence"));}
            if paper.attachment_versions.len()>=PAPER_MAX {return Err(conflict("workpaper file-version capacity reached (16)"));}
            if paper.attachment_versions.iter().any(|v|v.sha256==version.sha256) {return Err(conflict("these bytes are already retained for this workpaper"));}
            let mut stored=version.clone();stored.version=paper.attachment_versions.len() as u32+1;
            stored.metadata_sha256=metadata_hash(&stored)?;paper.attachment_versions.push(stored);
            record.document=checked_input(record.document)?;
            record.input_revision+=1;invalidate(&mut record);touch(&mut record);
            if !state.attachment_blobs.iter().any(|blob|blob.engagement_id==rid&&blob.sha256==version.sha256) {
                state.attachment_blobs.push(AttachmentBlob{engagement_id:rid.into(),sha256:version.sha256.clone(),content_hex:input.content_hex.clone()});
            }
            state.engagements[position]=record.clone();
            audit(state,rid,"file_version_uploaded","Retained immutable bytes and metadata; active calculation and sign-offs invalidated. Operator label is unverified metadata.");
            Ok(record)
        }).await
    }
    pub async fn describe_attachment(&self, rid: &str, paper_id: &str, version_id: &str) -> EResult<Value> {
        let (metadata, bytes) = self.attachment(rid, paper_id, version_id).await?;
        let (kind, width, height, page_count) = inspect_bytes(&bytes);
        Ok(json!({
            "id": metadata.id, "file_name": metadata.file_name, "byte_len": bytes.len(), "sha256": metadata.sha256,
            "kind": kind, "width": width, "height": height, "page_count": page_count,
            "rendered": false, "scanned": false, "executed": false
        }))
    }
    pub async fn attachment(
        &self,
        rid: &str,
        paper_id: &str,
        version_id: &str,
    ) -> EResult<(AttachmentVersion, Vec<u8>)> {
        let state = self.load().await?;
        let record = state
            .engagements
            .iter()
            .find(|r| r.id == rid)
            .ok_or_else(missing)?;
        let version = record
            .document
            .workpapers
            .iter()
            .find(|p| p.id == paper_id)
            .and_then(|p| p.attachment_versions.iter().find(|v| v.id == version_id))
            .ok_or_else(missing)?;
        let blob = state
            .attachment_blobs
            .iter()
            .find(|b| b.engagement_id == rid && b.sha256 == version.sha256)
            .ok_or_else(missing)?;
        Ok((version.clone(), content(&blob.content_hex)?))
    }
    pub async fn attachment_package(&self, rid: &str) -> EResult<Vec<u8>> {
        let state = self.load().await?;
        let record = state
            .engagements
            .iter()
            .find(|r| r.id == rid)
            .ok_or_else(missing)?;
        let files = files_for_record(&state, record)?;
        if files.is_empty() {
            return Err(conflict(
                "no file versions are referenced by this engagement archive",
            ));
        }
        let package = package_value(record, &files);
        let mut entries = vec![(
            "engagement.json".to_string(),
            serde_json::to_vec_pretty(&package).map_err(|_| bad("cannot encode file bundle"))?,
        )];
        entries.extend(files.into_iter().map(|(path, bytes)| (path, bytes)));
        zip(&entries)
    }
}
pub(super) fn files_for_record(
    state: &EngagementState,
    record: &EngagementRecord,
) -> EResult<Vec<(String, Vec<u8>)>> {
    let mut keys = BTreeSet::new();
    for (owner, doc) in documents(record) {
        for p in &doc.workpapers {
            for v in &p.attachment_versions {
                keys.insert((owner, v.sha256.as_str()));
            }
        }
    }
    let mut files = vec![];
    for (owner, sha) in keys {
        let blob = state
            .attachment_blobs
            .iter()
            .find(|b| b.engagement_id == owner && b.sha256 == sha)
            .ok_or_else(|| conflict("file archive reference unavailable"))?;
        files.push((
            format!("files/{owner}/{sha}.bin"),
            content(&blob.content_hex)?,
        ));
    }
    Ok(files)
}
pub(super) fn package_value(record: &EngagementRecord, files: &[(String, Vec<u8>)]) -> Value {
    let mut package = json!({"format":"nxt-engagement/v1","scope":"Internal NXT workpaper archive; not a Caseware native file or assurance report","engagement":record});
    if !files.is_empty() {
        package["attachment_manifest"] = json!({"format":"nxt-engagement-files/v1","file_bytes_in_json":false,
            "bundle_route":format!("/api/engagements/{}/attachments/package",record.id),
            "files":files.iter().map(|(path,bytes)|json!({"path":path,"bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes))})).collect::<Vec<_>>(),
            "instruction":"Download the file bundle ZIP to retain this JSON and all referenced original bytes, including historical/origin references."});
    }
    package
}

// Store-only ZIP framing follows the existing original NXT workbook writer.
fn zip(files: &[(String, Vec<u8>)]) -> EResult<Vec<u8>> {
    fn w16(out: &mut Vec<u8>, n: u16) {
        out.extend(n.to_le_bytes());
    }
    fn w32(out: &mut Vec<u8>, n: u32) {
        out.extend(n.to_le_bytes());
    }
    fn crc(bytes: &[u8]) -> u32 {
        let mut c = !0u32;
        for b in bytes {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = (c >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(c & 1)));
            }
        }
        !c
    }
    let mut out = vec![];
    let mut directory = vec![];
    if files.len() > 129 {
        return Err(conflict("file bundle entry bound exceeded"));
    }
    for (name, bytes) in files {
        if name.len() > 256 || bytes.len() > 4 * 1024 * 1024 {
            return Err(conflict("file bundle entry bound exceeded"));
        }
        let offset = out.len() as u32;
        let n = bytes.len() as u32;
        let checksum = crc(bytes);
        w32(&mut out, 0x04034b50);
        w16(&mut out, 20);
        w16(&mut out, 0x0800);
        w16(&mut out, 0);
        w16(&mut out, 0);
        w16(&mut out, 0x0021);
        w32(&mut out, checksum);
        w32(&mut out, n);
        w32(&mut out, n);
        w16(&mut out, name.len() as u16);
        w16(&mut out, 0);
        out.extend(name.as_bytes());
        out.extend(bytes);
        w32(&mut directory, 0x02014b50);
        w16(&mut directory, 20);
        w16(&mut directory, 20);
        w16(&mut directory, 0x0800);
        w16(&mut directory, 0);
        w16(&mut directory, 0);
        w16(&mut directory, 0x0021);
        w32(&mut directory, checksum);
        w32(&mut directory, n);
        w32(&mut directory, n);
        w16(&mut directory, name.len() as u16);
        for _ in 0..4 {
            w16(&mut directory, 0);
        }
        w32(&mut directory, 0);
        w32(&mut directory, offset);
        directory.extend(name.as_bytes());
        if out.len() + directory.len() + 22 > 4 * 1024 * 1024 {
            return Err(conflict(
                "file bundle exceeds the 4 MiB response bound; preserve individual downloads and the protected backup",
            ));
        }
    }
    let start = out.len() as u32;
    let length = directory.len() as u32;
    out.extend(directory);
    w32(&mut out, 0x06054b50);
    w16(&mut out, 0);
    w16(&mut out, 0);
    w16(&mut out, files.len() as u16);
    w16(&mut out, files.len() as u16);
    w32(&mut out, length);
    w32(&mut out, start);
    w16(&mut out, 0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn must<T>(value: EResult<T>) -> T {
        value.unwrap_or_else(|e| panic!("{}", e.1))
    }
    fn engine() -> Engagements {
        let ledger = Arc::new(crate::ledger::engine::Ledger::new(Store::memory()));
        let tax = Arc::new(crate::tax::Tax::new(Store::memory()));
        let firm = Arc::new(crate::firm::Firm::new(Store::memory(), ledger.clone()));
        Engagements::new(Store::memory(), ledger, tax, firm)
    }
    async fn record(engine: &Engagements) -> EngagementRecord {
        let input=serde_json::from_value(json!({"name":"Synthetic capacity fixture","period_start":"2025-01-01","period_end":"2025-12-31","workpapers":[{"id":"p1","reference":"A","title":"Synthetic","body":"Synthetic only"}]})).unwrap();
        must(engine.save(None, input).await)
    }
    fn request(r: &EngagementRecord, slot: usize, bytes: &[u8]) -> AttachmentUpload {
        AttachmentUpload {
            expected_revision: r.revision,
            upload_id: format!("upload_{slot}"),
            file_name: "synthetic.bin".into(),
            operator_label: "Synthetic QA".into(),
            note: "Synthetic capacity test".into(),
            content_hex: hex::encode(bytes),
        }
    }
    #[tokio::test]
    async fn workspace_binary_capacity_rejects_without_partial_blob_or_version() {
        let engine = engine();
        let mut r = record(&engine).await;
        for slot in 0..8 {
            r = must(
                engine
                    .upload_attachment(&r.id, "p1", request(&r, slot, &vec![slot as u8; FILE_MAX]))
                    .await,
            );
        }
        let before = engine.store.get(STATE_KEY).await.unwrap().unwrap();
        let result = engine
            .upload_attachment(&r.id, "p1", request(&r, 8, &vec![8; FILE_MAX]))
            .await;
        assert_eq!(result.err().unwrap().0, StatusCode::CONFLICT);
        assert_eq!(engine.store.get(STATE_KEY).await.unwrap().unwrap(), before);
    }
    #[tokio::test]
    async fn paper_version_and_duplicate_content_limits_keep_prior_history() {
        let engine = engine();
        let mut r = record(&engine).await;
        r = must(
            engine
                .upload_attachment(&r.id, "p1", request(&r, 0, b"a"))
                .await,
        );
        let before = engine.store.get(STATE_KEY).await.unwrap().unwrap();
        assert!(
            engine
                .upload_attachment(&r.id, "p1", request(&r, 100, b"a"))
                .await
                .is_err()
        );
        assert_eq!(engine.store.get(STATE_KEY).await.unwrap().unwrap(), before);
        for slot in 1..16 {
            r = must(
                engine
                    .upload_attachment(&r.id, "p1", request(&r, slot, &[slot as u8]))
                    .await,
            );
        }
        let before = engine.store.get(STATE_KEY).await.unwrap().unwrap();
        assert!(
            engine
                .upload_attachment(&r.id, "p1", request(&r, 16, b"z"))
                .await
                .is_err()
        );
        assert_eq!(engine.store.get(STATE_KEY).await.unwrap().unwrap(), before);
    }
}
