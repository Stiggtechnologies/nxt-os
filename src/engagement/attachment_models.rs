use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentVersion {
    pub id: String,
    pub version: u32,
    pub engagement_id: String,
    pub workpaper_id: String,
    pub file_name: String,
    pub bytes: usize,
    pub sha256: String,
    pub uploaded_at: String,
    pub operator_label: String,
    pub note: String,
    pub upload_id: String,
    pub upload_expected_revision: u64,
    pub request_sha256: String,
    pub metadata_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentBlob {
    pub engagement_id: String,
    pub sha256: String,
    pub content_hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentUpload {
    pub expected_revision: u64,
    pub upload_id: String,
    pub file_name: String,
    pub operator_label: String,
    pub note: String,
    pub content_hex: String,
}
