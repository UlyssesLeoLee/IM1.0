//! 统一错误类型 —— IM1.0 错误码单一来源
//!
//! 依据: docs/templates/04-detailed-design/aux/aux-03-error-code-registry.md §B (21 项)
//!       docs/ImplementationSpec.md §5.1
//!
//! 关键原则:
//! - `ErrorCode` 枚举即 wire format 的 code 字符串(由 strum AsRefStr 自动生成)
//! - `AppError::code()` 是 im-gateway 边界统一转换的唯一点
//! - 任何 `match err.code { ... }` 必须有 `default` 分支(防止枚举扩展后遗漏)
//! - CI 由 `scripts/check_error_codes.sh` 扫描所有错误码字符串与枚举一致性

use strum::AsRefStr;
use thiserror::Error;

/// IM1.0 全部错误码枚举(21 项,2026-08-23 自审新增 `FriendRequestNotFound`)
///
/// 字符串形式即 wire format 的 `code` 字段值
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsRefStr)]
pub enum ErrorCode {
    // ---- 鉴权 / 通用 ----
    Unauthorized,                // 401
    Forbidden,                   // 403
    NotFound,                    // 404

    // ---- 消息 / 业务 ----
    IdempotencyConflict,         // 200 特殊语义(WS/REST 视为成功)
    RateLimited,                 // 429
    InvalidStateTransition,      // 409
    RecallWindowExpired,         // 409
    AccountBanned,               // 403
    AccountSuspended,            // 403
    AccountMergeConflict,        // 409
    /// 2026-09-20 C-3 WBS: register dup username (409 Conflict)
    AccountAlreadyExists,        // 409
    FriendRequestExists,         // 409
    FriendRequestNotFound,       // 404
    UserBlocked,                 // 403
    ValidationError,             // 400
    InternalError,               // 500
    ServiceUnavailable,          // 503
    ConversationNotFound,        // 404
    MessageNotFound,             // 404
    MessageTooLarge,             // 413
    InvalidIdempotencyKey,       // 400
    EnvironmentDisabled,         // 403
}

impl ErrorCode {
    /// wire format 字符串(与 strum::AsRefStr 配合使用)
    #[inline]
    pub fn as_str(&self) -> &'static str {
        // strum::AsRefStr 生成的 as_ref 返回 &str,这里 leak 到 'static 不可行
        // 但 ErrorCode 全是 enum 字面量,字符串字面量本身就是 'static
        // 故此转换是安全的
        match self {
            ErrorCode::Unauthorized => "UNAUTHORIZED",
            ErrorCode::Forbidden => "FORBIDDEN",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            ErrorCode::RateLimited => "RATE_LIMITED",
            ErrorCode::InvalidStateTransition => "INVALID_STATE_TRANSITION",
            ErrorCode::RecallWindowExpired => "RECALL_WINDOW_EXPIRED",
            ErrorCode::AccountBanned => "ACCOUNT_BANNED",
            ErrorCode::AccountSuspended => "ACCOUNT_SUSPENDED",
            ErrorCode::AccountMergeConflict => "ACCOUNT_MERGE_CONFLICT",
            ErrorCode::AccountAlreadyExists => "ACCOUNT_ALREADY_EXISTS",
            ErrorCode::FriendRequestExists => "FRIEND_REQUEST_EXISTS",
            ErrorCode::FriendRequestNotFound => "FRIEND_REQUEST_NOT_FOUND",
            ErrorCode::UserBlocked => "USER_BLOCKED",
            ErrorCode::ValidationError => "VALIDATION_ERROR",
            ErrorCode::InternalError => "INTERNAL_ERROR",
            ErrorCode::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            ErrorCode::ConversationNotFound => "CONVERSATION_NOT_FOUND",
            ErrorCode::MessageNotFound => "MESSAGE_NOT_FOUND",
            ErrorCode::MessageTooLarge => "MESSAGE_TOO_LARGE",
            ErrorCode::InvalidIdempotencyKey => "INVALID_IDEMPOTENCY_KEY",
            ErrorCode::EnvironmentDisabled => "ENVIRONMENT_DISABLED",
        }
    }

    /// 对应 HTTP 状态码
    pub fn http_status(self) -> u16 {
        use ErrorCode::*;
        match self {
            Unauthorized => 401,
            Forbidden | AccountBanned | AccountSuspended | UserBlocked => 403,
            NotFound | ConversationNotFound | MessageNotFound | FriendRequestNotFound => 404,
            ValidationError | InvalidIdempotencyKey => 400,
            MessageTooLarge => 413,
            InvalidStateTransition
            | RecallWindowExpired
            | AccountMergeConflict
            | FriendRequestExists
            | AccountAlreadyExists => 409,
            RateLimited => 429,
            InternalError => 500,
            ServiceUnavailable => 503,
            IdempotencyConflict => 200, // 特殊语义
            EnvironmentDisabled => 403,
        }
    }

    /// 对应 `tonic::Code`(gRPC 错误码)
    pub fn grpc_code_name(self) -> &'static str {
        use ErrorCode::*;
        match self {
            Unauthorized => "UNAUTHENTICATED",
            Forbidden | AccountBanned | AccountSuspended | UserBlocked | EnvironmentDisabled => {
                "PERMISSION_DENIED"
            }
            NotFound | ConversationNotFound | MessageNotFound | FriendRequestNotFound => {
                "NOT_FOUND"
            }
            ValidationError | InvalidIdempotencyKey | MessageTooLarge => "INVALID_ARGUMENT",
            InvalidStateTransition
            | RecallWindowExpired
            | AccountMergeConflict
            | FriendRequestExists
            | AccountAlreadyExists => "FAILED_PRECONDITION",
            RateLimited => "RESOURCE_EXHAUSTED",
            InternalError => "INTERNAL",
            ServiceUnavailable => "UNAVAILABLE",
            IdempotencyConflict => "OK", // 特殊语义
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ErrorCode {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 与 aux-03 §B 双向映射;未注册错误码 → 兜底为 INTERNAL_ERROR
        use ErrorCode as EC;
        Ok(match s {
            "UNAUTHORIZED" => EC::Unauthorized,
            "FORBIDDEN" => EC::Forbidden,
            "NOT_FOUND" => EC::NotFound,
            "IDEMPOTENCY_CONFLICT" => EC::IdempotencyConflict,
            "RATE_LIMITED" => EC::RateLimited,
            "INVALID_STATE_TRANSITION" => EC::InvalidStateTransition,
            "RECALL_WINDOW_EXPIRED" => EC::RecallWindowExpired,
            "ACCOUNT_BANNED" => EC::AccountBanned,
            "ACCOUNT_SUSPENDED" => EC::AccountSuspended,
            "ACCOUNT_MERGE_CONFLICT" => EC::AccountMergeConflict,
            "ACCOUNT_ALREADY_EXISTS" => EC::AccountAlreadyExists,
            "FRIEND_REQUEST_EXISTS" => EC::FriendRequestExists,
            "FRIEND_REQUEST_NOT_FOUND" => EC::FriendRequestNotFound,
            "USER_BLOCKED" => EC::UserBlocked,
            "VALIDATION_ERROR" => EC::ValidationError,
            "INTERNAL_ERROR" => EC::InternalError,
            "SERVICE_UNAVAILABLE" => EC::ServiceUnavailable,
            "CONVERSATION_NOT_FOUND" => EC::ConversationNotFound,
            "MESSAGE_NOT_FOUND" => EC::MessageNotFound,
            "MESSAGE_TOO_LARGE" => EC::MessageTooLarge,
            "INVALID_IDEMPOTENCY_KEY" => EC::InvalidIdempotencyKey,
            "ENVIRONMENT_DISABLED" => EC::EnvironmentDisabled,
            _ => EC::InternalError, // 兜底
        })
    }
}

/// 应用层统一错误
#[derive(Debug, Error)]
pub enum AppError {
    // ---- 鉴权 / Token ----
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("rate limited: retry_after={0}s")]
    RateLimited(u64),

    // ---- 业务逻辑 ----
    #[error("validation: {0}")]
    Validation(String),

    #[error("idempotency conflict: existing message id={0}")]
    IdempotencyConflict(uuid::Uuid),

    #[error("invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("recall window expired (created_at={0})")]
    RecallWindowExpired(chrono::DateTime<chrono::Utc>),

    #[error("account banned")]
    AccountBanned,

    #[error("account suspended")]
    AccountSuspended,

    #[error("account merge conflict: target external identity already bound")]
    AccountMergeConflict,

    /// 2026-09-20 C-3 WBS: register 时 username 重复 (env, username) UNIQUE 命中
    /// 也涵盖 device_sessions 同 user 重复 active refresh hash 等其它 unique_violation
    /// (per crates/im-core/src/identity/pg.rs::map_sqlx_error)
    #[error("account already exists")]
    AccountAlreadyExists,

    #[error("friend request already exists")]
    FriendRequestExists,

    #[error("friend request not found: {0}")]
    FriendRequestNotFound(uuid::Uuid),

    #[error("user blocked by target")]
    UserBlocked,

    #[error("conversation not found: {0}")]
    ConversationNotFound(uuid::Uuid),

    #[error("message not found: {0}")]
    MessageNotFound(uuid::Uuid),

    #[error("message too large: {0} bytes (max {1})")]
    MessageTooLarge(usize, usize),

    #[error("invalid idempotency key: {0}")]
    InvalidIdempotencyKey(String),

    #[error("environment disabled: {0}")]
    EnvironmentDisabled(uuid::Uuid),

    // ---- 系统 / 依赖 ----
    #[error("internal error")]
    Internal(#[from] anyhow::Error),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl AppError {
    /// 错误码 —— 边界转换的唯一点
    #[inline]
    pub fn code(&self) -> ErrorCode {
        use AppError::*;
        match self {
            Unauthorized(_) => ErrorCode::Unauthorized,
            Forbidden(_) => ErrorCode::Forbidden,
            NotFound(_) => ErrorCode::NotFound,
            RateLimited(_) => ErrorCode::RateLimited,
            Validation(_) => ErrorCode::ValidationError,
            IdempotencyConflict(_) => ErrorCode::IdempotencyConflict,
            InvalidStateTransition { .. } => ErrorCode::InvalidStateTransition,
            RecallWindowExpired(_) => ErrorCode::RecallWindowExpired,
            AccountBanned => ErrorCode::AccountBanned,
            AccountSuspended => ErrorCode::AccountSuspended,
            AccountMergeConflict => ErrorCode::AccountMergeConflict,
            AccountAlreadyExists => ErrorCode::AccountAlreadyExists,
            FriendRequestExists => ErrorCode::FriendRequestExists,
            FriendRequestNotFound(_) => ErrorCode::FriendRequestNotFound,
            UserBlocked => ErrorCode::UserBlocked,
            ConversationNotFound(_) => ErrorCode::ConversationNotFound,
            MessageNotFound(_) => ErrorCode::MessageNotFound,
            MessageTooLarge(_, _) => ErrorCode::MessageTooLarge,
            InvalidIdempotencyKey(_) => ErrorCode::InvalidIdempotencyKey,
            EnvironmentDisabled(_) => ErrorCode::EnvironmentDisabled,
            Internal(_) => ErrorCode::InternalError,
            ServiceUnavailable(_) => ErrorCode::ServiceUnavailable,
        }
    }

    /// HTTP 状态码(由 code().http_status() 派生)
    #[inline]
    pub fn http_status(&self) -> u16 {
        self.code().http_status()
    }

    /// gRPC 状态码名称
    #[inline]
    pub fn grpc_code_name(&self) -> &'static str {
        self.code().grpc_code_name()
    }
}

/// 统一 Result
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_count_is_22() {
        // 防止新增错误码时忘了更新总数 — 22 = 2026-09-20 C-3 WBS 新增 AccountAlreadyExists 后的最终值
        let count = [
            ErrorCode::Unauthorized,
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::IdempotencyConflict,
            ErrorCode::RateLimited,
            ErrorCode::InvalidStateTransition,
            ErrorCode::RecallWindowExpired,
            ErrorCode::AccountBanned,
            ErrorCode::AccountSuspended,
            ErrorCode::AccountMergeConflict,
            ErrorCode::AccountAlreadyExists,
            ErrorCode::FriendRequestExists,
            ErrorCode::FriendRequestNotFound,
            ErrorCode::UserBlocked,
            ErrorCode::ValidationError,
            ErrorCode::InternalError,
            ErrorCode::ServiceUnavailable,
            ErrorCode::ConversationNotFound,
            ErrorCode::MessageNotFound,
            ErrorCode::MessageTooLarge,
            ErrorCode::InvalidIdempotencyKey,
            ErrorCode::EnvironmentDisabled,
        ]
        .len();
        assert_eq!(count, 22, "错误码总数与 aux-03 §B 不一致");
    }

    #[test]
    fn code_strings_match_aux03() {
        // 关键代码字符串必须与 aux-03 §B 完全一致
        assert_eq!(ErrorCode::AccountBanned.as_str(), "ACCOUNT_BANNED");
        assert_eq!(
            ErrorCode::FriendRequestNotFound.as_str(),
            "FRIEND_REQUEST_NOT_FOUND"
        );
        assert_eq!(ErrorCode::IdempotencyConflict.as_str(), "IDEMPOTENCY_CONFLICT");
    }

    #[test]
    fn http_status_mapping() {
        assert_eq!(ErrorCode::Unauthorized.http_status(), 401);
        assert_eq!(ErrorCode::AccountBanned.http_status(), 403);
        assert_eq!(ErrorCode::NotFound.http_status(), 404);
        assert_eq!(ErrorCode::MessageTooLarge.http_status(), 413);
        assert_eq!(ErrorCode::IdempotencyConflict.http_status(), 200); // 特殊
    }

    #[test]
    fn from_str_roundtrip() {
        for code in [
            ErrorCode::Unauthorized,
            ErrorCode::FriendRequestNotFound,
            ErrorCode::IdempotencyConflict,
        ] {
            let s = code.as_str();
            let parsed: ErrorCode = s.parse().unwrap();
            assert_eq!(parsed, code);
        }
    }

    #[test]
    fn unknown_code_falls_back_to_internal() {
        let parsed: ErrorCode = "NOT_A_REAL_CODE".parse().unwrap();
        assert_eq!(parsed, ErrorCode::InternalError);
    }

    #[test]
    fn app_error_to_code_mapping() {
        assert_eq!(AppError::AccountBanned.code(), ErrorCode::AccountBanned);
        assert_eq!(
            AppError::FriendRequestNotFound(uuid::Uuid::nil()).code(),
            ErrorCode::FriendRequestNotFound
        );
        assert_eq!(
            AppError::MessageTooLarge(100, 64).code(),
            ErrorCode::MessageTooLarge
        );
    }
}
