//! 错误响应体 — REST / WS 通用
//!
//! 依据: aux-13 §3.7 + ImplementationSpec §5.2

use serde::{Deserialize, Serialize};

/// REST 错误响应体 / WS `ack.ok=false` 的 error 字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// 错误码字符串,与 aux-03 §B 一致
    pub code: String,
    /// i18n key,**非最终用户文案**
    pub message: String,
    /// 链路追踪 ID
    pub trace_id: String,
    /// 错误发生时间(unix ms)
    pub ts: i64,
    /// 字段级错误(仅 VALIDATION_ERROR 出现)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<FieldError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub reason: String,
}

impl ErrorBody {
    /// 成功响应体(用于 IDEMPOTENCY_CONFLICT 特殊语义)
    pub fn new(code: impl Into<String>, message: impl Into<String>, trace_id: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            trace_id: trace_id.into(),
            ts: chrono::Utc::now().timestamp_millis(),
            details: vec![],
        }
    }

    pub fn with_details(mut self, details: Vec<FieldError>) -> Self {
        self.details = details;
        self
    }
}
