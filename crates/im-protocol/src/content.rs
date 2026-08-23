//! Message.content 6 种 kind 的 JSON Schema
//!
//! 依据: aux-13 §4.1
//!
//! 6 种 kind: text / image / file / sticker / system / custom

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message.content —— 按 kind 区分内容
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageContent {
    Text {
        text: String, // 1..=4000
    },
    Image {
        media_id: Uuid,
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        thumbnail_media_id: Option<Uuid>,
    },
    File {
        media_id: Uuid,
        file_name: String, // 1..=255
        size_bytes: u64,   // > 0
        #[serde(default)]
        mime_type: Option<String>,
    },
    Sticker {
        sticker_id: String, // 1..=64
    },
    System {
        event: String, // join | leave | kicked | renamed | ...
        #[serde(default)]
        actor_user_id: Option<Uuid>,
        #[serde(default)]
        target_user_id: Option<Uuid>,
    },
    Custom {
        schema: String, // 1..=64
        data: serde_json::Value,
    },
}

impl MessageContent {
    /// 校验 content 大小(序列化字节数 ≤ max_bytes)
    pub fn validate_size(&self, max_bytes: usize) -> Result<(), ContentError> {
        let serialized = serde_json::to_vec(self)
            .map_err(|e| ContentError::Serialize(e.to_string()))?;
        if serialized.len() > max_bytes {
            return Err(ContentError::TooLarge {
                actual: serialized.len(),
                max: max_bytes,
            });
        }
        Ok(())
    }

    /// 按 kind 校验必填字段范围
    pub fn validate(&self) -> Result<(), ContentError> {
        match self {
            MessageContent::Text { text } => {
                if text.is_empty() || text.len() > 4000 {
                    return Err(ContentError::TextLength(text.len()));
                }
            }
            MessageContent::Image { media_id, .. } => {
                if media_id.is_nil() {
                    return Err(ContentError::InvalidUuid("image.media_id"));
                }
            }
            MessageContent::File {
                media_id,
                file_name,
                size_bytes,
                ..
            } => {
                if media_id.is_nil() {
                    return Err(ContentError::InvalidUuid("file.media_id"));
                }
                if file_name.is_empty() || file_name.len() > 255 {
                    return Err(ContentError::FileNameLength(file_name.len()));
                }
                if *size_bytes == 0 {
                    return Err(ContentError::FileSizeZero);
                }
            }
            MessageContent::Sticker { sticker_id } => {
                if sticker_id.is_empty() || sticker_id.len() > 64 {
                    return Err(ContentError::StickerIdLength(sticker_id.len()));
                }
            }
            MessageContent::System { event, .. } => {
                if event.is_empty() {
                    return Err(ContentError::SystemEventEmpty);
                }
            }
            MessageContent::Custom { schema, .. } => {
                if schema.is_empty() || schema.len() > 64 {
                    return Err(ContentError::CustomSchemaLength(schema.len()));
                }
            }
        }
        Ok(())
    }

    /// kind 字符串
    pub fn kind_name(&self) -> &'static str {
        match self {
            MessageContent::Text { .. } => "text",
            MessageContent::Image { .. } => "image",
            MessageContent::File { .. } => "file",
            MessageContent::Sticker { .. } => "sticker",
            MessageContent::System { .. } => "system",
            MessageContent::Custom { .. } => "custom",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("content too large: {actual} bytes (max {max})")]
    TooLarge { actual: usize, max: usize },
    #[error("text length invalid: {0} (must be 1..=4000)")]
    TextLength(usize),
    #[error("file_name length invalid: {0} (must be 1..=255)")]
    FileNameLength(usize),
    #[error("file size_bytes must be > 0")]
    FileSizeZero,
    #[error("sticker_id length invalid: {0} (must be 1..=64)")]
    StickerIdLength(usize),
    #[error("system event must not be empty")]
    SystemEventEmpty,
    #[error("custom schema length invalid: {0} (must be 1..=64)")]
    CustomSchemaLength(usize),
    #[error("invalid uuid: {0}")]
    InvalidUuid(&'static str),
    #[error("serialize error: {0}")]
    Serialize(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_validation() {
        let c = MessageContent::Text {
            text: "hi".into(),
        };
        assert!(c.validate().is_ok());

        let c = MessageContent::Text {
            text: "".into(),
        };
        assert!(c.validate().is_err());

        let c = MessageContent::Text {
            text: "x".repeat(4001),
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn text_serialize_roundtrip() {
        let c = MessageContent::Text {
            text: "hello".into(),
        };
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(s, r#"{"kind":"text","text":"hello"}"#);
    }

    #[test]
    fn file_validation() {
        let ok = MessageContent::File {
            media_id: Uuid::new_v4(),
            file_name: "doc.pdf".into(),
            size_bytes: 1024,
            mime_type: None,
        };
        assert!(ok.validate().is_ok());

        let bad = MessageContent::File {
            media_id: Uuid::new_v4(),
            file_name: "".into(),
            size_bytes: 1024,
            mime_type: None,
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn size_limit() {
        let big = MessageContent::Text {
            text: "x".repeat(100_000),
        };
        assert!(big.validate_size(1024).is_err());
    }
}
