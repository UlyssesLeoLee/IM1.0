//! AppError → HTTP Response 转换
//!
//! 依据: ImplementationSpec §5 + aux-03

use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use im_common::{AppError, ErrorCode};
use serde_json::json;

pub fn error_to_response(err: &AppError) -> HttpResponse {
    let code = err.code();
    let status = StatusCode::from_u16(code.http_status())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // 链路追踪 ID(从 tracing span 中提取,MVP 暂时用随机)
    let trace_id = uuid::Uuid::new_v4().to_string();
    let ts = chrono::Utc::now().timestamp_millis();

    let body = json!({
        "code": code.as_str(),
        "message": i18n_key_for(&code),
        "trace_id": trace_id,
        "ts": ts,
    });

    tracing::warn!(
        error_code = code.as_str(),
        http_status = status.as_u16(),
        "request error"
    );

    HttpResponse::build(status).json(body)
}

/// 错误码 → i18n key(MVP 阶段直接硬编码,前端按 key 翻译)
fn i18n_key_for(code: &ErrorCode) -> &'static str {
    use ErrorCode::*;
    match code {
        Unauthorized => "auth.unauthorized",
        Forbidden => "auth.forbidden",
        NotFound => "resource.not_found",
        IdempotencyConflict => "message.idempotency_conflict",
        RateLimited => "rate_limit.exceeded",
        InvalidStateTransition => "message.invalid_state_transition",
        RecallWindowExpired => "message.recall_window_expired",
        AccountBanned => "auth.account.banned",
        AccountSuspended => "auth.account.suspended",
        AccountMergeConflict => "auth.account.merge_conflict",
        FriendRequestExists => "friend.request_exists",
        FriendRequestNotFound => "friend.request_not_found",
        UserBlocked => "user.blocked",
        ValidationError => "request.validation_error",
        InternalError => "server.internal_error",
        ServiceUnavailable => "server.service_unavailable",
        ConversationNotFound => "conversation.not_found",
        MessageNotFound => "message.not_found",
        MessageTooLarge => "message.too_large",
        InvalidIdempotencyKey => "message.invalid_idempotency_key",
        EnvironmentDisabled => "environment.disabled",
    }
}
