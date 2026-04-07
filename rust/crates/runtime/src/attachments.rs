use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

// ─── Error Types ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum AttachmentError {
    Io(std::io::Error),
    InvalidMimeType(String),
}

impl Display for AttachmentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidMimeType(mime) => write!(f, "invalid mime type: {mime}"),
        }
    }
}

impl std::error::Error for AttachmentError {}

impl From<std::io::Error> for AttachmentError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

// ─── AttachmentData ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttachmentData {
    Base64 { data: String },
    FilePath { path: PathBuf },
}

// ─── Attachment ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub mime_type: String,
    pub name: String,
    pub data: AttachmentData,
}

impl Attachment {
    /// Construct an attachment from already base64-encoded data.
    #[must_use]
    pub fn from_base64(
        mime_type: impl Into<String>,
        name: impl Into<String>,
        data: impl Into<String>,
    ) -> Self {
        Self {
            mime_type: mime_type.into(),
            name: name.into(),
            data: AttachmentData::Base64 { data: data.into() },
        }
    }

    /// Read a file from disk and store its contents as base64-encoded data.
    pub fn from_file(
        mime_type: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<Self, AttachmentError> {
        let path = path.into();
        let bytes = std::fs::read(&path)?;
        let encoded = standard_base64_encode(&bytes);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        Ok(Self {
            mime_type: mime_type.into(),
            name,
            data: AttachmentData::Base64 { data: encoded },
        })
    }

    /// Create a file-reference attachment without reading the content.
    #[must_use]
    pub fn file_ref(
        mime_type: impl Into<String>,
        name: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            mime_type: mime_type.into(),
            name: name.into(),
            data: AttachmentData::FilePath { path: path.into() },
        }
    }
}

// ─── ToolOutput ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolOutput {
    pub text: String,
    pub attachments: Vec<Attachment>,
}

// ─── Minimal base64 encoding (no external dependency) ───────────────────────

fn standard_base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).map_or(0, |&b| u32::from(b));
        let b2 = chunk.get(2).map_or(0, |&b| u32::from(b));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{Attachment, AttachmentData, AttachmentError, ToolOutput};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn attachment_roundtrip_serialization() {
        let att = Attachment::from_base64("image/png", "screenshot.png", "iVBORw0KGgo=");
        let json = serde_json::to_string(&att).expect("serialize attachment");
        let restored: Attachment = serde_json::from_str(&json).expect("deserialize attachment");
        assert_eq!(att, restored);
        assert!(json.contains("\"mime_type\":\"image/png\""));
        assert!(json.contains("\"name\":\"screenshot.png\""));
        assert!(json.contains("\"type\":\"base64\""));
    }

    #[test]
    fn from_base64_constructor() {
        let att = Attachment::from_base64("text/x-diff", "changes.diff", "ZGlmZiAtLWdpdA==");
        assert_eq!(att.mime_type, "text/x-diff");
        assert_eq!(att.name, "changes.diff");
        assert!(matches!(att.data, AttachmentData::Base64 { .. }));
        if let AttachmentData::Base64 { data } = att.data {
            assert_eq!(data, "ZGlmZiAtLWdpdA==");
        }
    }

    #[test]
    fn from_file_reads_and_base64_encodes() {
        let tmp = std::env::temp_dir().join("runtime-attach-test.bin");
        fs::write(&tmp, b"hello world").expect("write temp file");
        let att = Attachment::from_file("application/octet-stream", &tmp)
            .expect("from_file should succeed");
        fs::remove_file(&tmp).expect("remove temp file");
        assert_eq!(att.mime_type, "application/octet-stream");
        assert_eq!(att.name, "runtime-attach-test.bin");
        if let AttachmentData::Base64 { data } = att.data {
            assert_eq!(data, "aGVsbG8gd29ybGQ=");
        } else {
            panic!("expected Base64 data");
        }
    }

    #[test]
    fn file_ref_does_not_read_content() {
        let non_existent = PathBuf::from("/nonexistent/path/image.png");
        let att = Attachment::file_ref("image/png", "image.png", &non_existent);
        assert_eq!(att.mime_type, "image/png");
        assert_eq!(att.name, "image.png");
        assert!(matches!(att.data, AttachmentData::FilePath { .. }));
        if let AttachmentData::FilePath { path } = att.data {
            assert_eq!(path, non_existent);
        }
    }

    #[test]
    fn tool_output_default() {
        let output = ToolOutput::default();
        assert_eq!(output.text, "");
        assert!(output.attachments.is_empty());
    }

    #[test]
    fn attachment_serializes_with_tag() {
        let att = Attachment::from_base64("image/png", "test.png", "abcd");
        let json = serde_json::to_string(&att).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        assert_eq!(value["data"]["type"], "base64");
        assert_eq!(value["mime_type"], "image/png");
        assert_eq!(value["name"], "test.png");
    }

    #[test]
    fn attachment_file_path_serializes_roundtrip() {
        let att = Attachment::file_ref(
            "text/x-diff",
            "patch.diff",
            PathBuf::from("/tmp/patch.diff"),
        );
        let json = serde_json::to_string(&att).expect("serialize");
        let restored: Attachment = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(att, restored);
        assert!(json.contains("\"type\":\"file_path\""));
    }

    #[test]
    fn tool_output_with_attachments_roundtrip() {
        let mut output = ToolOutput { text: "Here is the screenshot".to_string(), ..Default::default() };
        output.attachments.push(Attachment::from_base64(
            "image/png",
            "shot.png",
            "base64data",
        ));
        let json = serde_json::to_string(&output).expect("serialize tool output");
        let restored: ToolOutput = serde_json::from_str(&json).expect("deserialize tool output");
        assert_eq!(output, restored);
        assert_eq!(restored.attachments.len(), 1);
    }

    #[test]
    fn from_file_returns_io_error_for_missing_file() {
        let result = Attachment::from_file("image/png", "/nonexistent/file.png");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AttachmentError::Io(_)));
    }
}
