//! Message content 校验(由 im-protocol::content 提供 schema,这里只做业务侧补充)
//!
//! 依据: aux-13 §4.1 + ImplementationSpec §3.1.3

use im_common::AppError;
use im_protocol::content::{ContentError, MessageContent};

/// 业务侧校验:序列化为 JSON 后字节数 ≤ max
pub fn validate_serialized_size(
    content: &MessageContent,
    max_bytes: usize,
) -> Result<(), AppError> {
    content
        .validate_size(max_bytes)
        .map_err(|e| AppError::MessageTooLarge(0, max_bytes))
}

/// 业务侧校验:必填字段范围
pub fn validate_fields(content: &MessageContent) -> Result<(), AppError> {
    content
        .validate()
        .map_err(|e: ContentError| AppError::Validation(format!("content: {}", e)))
}
