//! Original NXT recorded UTF-8 text selections; no reconstruction from changed text.
use super::models::{Annotation, Workpaper, WorkpaperType};
use serde_json::json;
use sha2::{Digest, Sha256};
pub const BINDING_FORMAT: &str = "nxt-text-range-annotation-binding/v1";
pub const MAX_SELECTION_BYTES: usize = 4096;
pub const MAX_BODY_BYTES: u32 = 16384;
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn validate(paper: &Workpaper, annotation: &Annotation) -> Result<(), String> {
    let Some(anchor) = &annotation.text_range_anchor else {
        return Ok(());
    };
    if annotation.account_anchor.is_some() {
        return Err("Account and text range scopes are mutually exclusive".into());
    }
    if paper.document_type != WorkpaperType::Manual {
        return Err("Text range annotations require a manual workpaper".into());
    }
    if paper.body.len() > MAX_BODY_BYTES as usize {
        return Err("Text range workpaper body exceeds 16 KiB".into());
    }
    if anchor.engagement_id.is_empty()
        || anchor.engagement_id.len() > 64
        || !anchor
            .engagement_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err("Invalid text range engagement owner".into());
    }
    let bytes = anchor.selected_text.as_bytes();
    if anchor.start_utf8_byte >= anchor.end_utf8_byte
        || anchor.end_utf8_byte > MAX_BODY_BYTES
        || bytes.is_empty()
        || bytes.len() > MAX_SELECTION_BYTES
        || anchor.end_utf8_byte - anchor.start_utf8_byte != bytes.len() as u32
    {
        return Err(
            "Text range must record 1..4096 exact UTF-8 bytes within the 16 KiB workpaper bound"
                .into(),
        );
    }
    if anchor
        .selected_text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("Recorded selection contains unsupported body control characters".into());
    }
    if !digest(&annotation.body_sha256)
        || !digest(&anchor.selection_sha256)
        || hex::encode(Sha256::digest(bytes)) != anchor.selection_sha256
    {
        return Err("Text range copy and body hashes must be valid lowercase SHA-256; selection hash must match the exact copy".into());
    }
    // Never index a shorter or otherwise changed current body for an old mark.
    if hex::encode(Sha256::digest(paper.body.as_bytes())) == annotation.body_sha256 {
        let start = anchor.start_utf8_byte as usize;
        let end = anchor.end_utf8_byte as usize;
        if !paper.body.is_char_boundary(start)
            || !paper.body.is_char_boundary(end)
            || paper.body.get(start..end) != Some(anchor.selected_text.as_str())
        {
            return Err("Fresh text range must lie on UTF-8 character boundaries and equal the exact copied selection".into());
        }
    }
    Ok(())
}
pub(super) fn binding(
    owner: &str,
    paper: &Workpaper,
    annotation: &Annotation,
) -> Result<Option<String>, String> {
    let Some(anchor) = &annotation.text_range_anchor else {
        return Ok(None);
    };
    let value = json!({"format":BINDING_FORMAT,"engagement_id":owner,"workpaper_id":paper.id,"annotation_id":annotation.id,"kind":annotation.kind,"custom_mark":annotation.custom_mark,"text_range_anchor":anchor,"body_sha256":annotation.body_sha256});
    Ok(Some(hex::encode(Sha256::digest(
        serde_json::to_vec(&value).map_err(|_| "Cannot encode recorded text range binding")?,
    ))))
}
