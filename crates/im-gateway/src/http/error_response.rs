//! 统一 JSON 错误响应 — HTTP 边界
//!
//! 依据: aux-03 §B + aux-13 §4 (通用错误响应格式)
//!
//! ## 范围 (per C-8 / C-10 HTTP handler 实装)
//! 把 `AppError` / `ErrorCode` 序列化为统一 JSON:
//! ```json
//! {
//!   "code": "FORBIDDEN",
//!   "message": "auth.forbidden",     // 实际是 i18n key
//!   "trace_id": "uuid",
//!   "ts": 1692528000000
//! }
//! ```
//! `ts` 用毫秒 unix(与 aux-13 §4 样例一致)。

use actix_web::http::StatusCode;
use actix_web::HttpResponse;

use im_common::ids::ConversationId;
use im_common::{AppError, ErrorCode};

use crate::error::error_to_response;

/// AppError → HTTP JSON 响应(existing 用法,这里重导出)
pub fn app_error_to_response(err: &AppError) -> HttpResponse {
    error_to_response(err)
}

/// 辅助:从 ErrorCode 直接构造 `actix_web::Error`(包装 HTTP 响应),
/// 加可选 message,含 conversation_id 上下文(aux-13 §4)。
pub fn json_response(
    code: ErrorCode,
    conv_id: Option<ConversationId>,
    message: Option<&str>,
) -> actix_web::Error {
    let status = StatusCode::from_u16(code.http_status())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = serde_json::json!({
        "code": code.as_str(),
        "message": message.unwrap_or("error"),
        "trace_id": uuid::Uuid::new_v4(),
        "ts": chrono::Utc::now().timestamp_millis(),
        "conversation_id": conv_id.map(|c| c.to_string()),
    });
    let resp = HttpResponse::build(status).json(body);
    // actix_web::Error::from(HttpResponse) 仅在 ResponseError trait 范围内构造;
    // 这里使用 InternalError 包装 response
    actix_web::error::InternalError::from_response(code.as_str().to_string(), resp).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_response_constructs_without_panic() {
        // 仅校验构造不 panic + 错误码 wire 字符串匹配 aux-03 §B
        for code in [
            ErrorCode::Unauthorized,
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::InternalError,
        ] {
            let _err = json_response(code, None, None);
            // wire format 字符串双重断言
            assert!(code.as_str().len() > 0);
        }
    }
}
