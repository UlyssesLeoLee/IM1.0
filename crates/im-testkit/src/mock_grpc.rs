//! gRPC RPC mock 数据(per aux-13 §2)
//!
//! 4 业务 RPC(per aux-13 §2.1-§2.4):
//! 1. `SendMessage`(§2.1)
//! 2. `ListMessages`(§2.2)
//! 3. `ValidateAccessToken`(§2.3)
//! 4. `MarkRead`(§2.4)
//!
//! im-proto 当前未生成实际可调用类型(由 build.rs 从 `core.proto` 编译期生成),
//! 故 mock 以 `serde_json::Value` 表达 wire-format 字段(与 protobuf JSON mapping 一致),
//! 调用方如需强类型可用 `prost` 或 `tonic` 转换。

use serde_json::{json, Value};
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, TenantId, UserId};

// =============================================================================
// 稳定 ID —— 对齐 aux-13 §2 样例
// =============================================================================

/// aux-13 §1.1/§1.2 沿用 `conversation_id`(DM)
pub const CONV_ID_DM: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
/// message_id for SendMessage response
pub const MSG_ID_TEXT: &str = "8a7e6679-7425-40de-944b-e07fc1f90ae7";
/// user_id (sender / token sub)
pub const USER_ID_AUTH: &str = "1a2e6679-7425-40de-944b-e07fc1f90ae7";
/// environment_id
pub const ENV_ID_DEFAULT: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
/// tenant_id(由 environment 反推;测试用稳定值)
pub const TENANT_ID_DEFAULT: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
/// access_token expires_at(per aux-13 §3.1 expires_in=900, ts≈2026-08-23)
pub const TOKEN_EXPIRES_AT_UNIX: i64 = 1_692_531_600; // 2026-08-23T01:00:00Z

// =============================================================================
// §2.1 SendMessage
// =============================================================================

/// `SendMessageRequest`(per aux-13 §2.1)
pub fn send_message_request() -> Value {
    json!({
        "conversation_id": CONV_ID_DM,
        "sender_id": USER_ID_AUTH,
        "idempotency_key": "33333333-3333-4333-8333-333333333333",
        "kind": "TEXT",                  // enum numeric 也可,这里用 enum name
        "content_json": "{\"text\":\"你好\"}",
        "reply_to": null,
    })
}

/// `Message` response(per aux-13 §2.1)
pub fn send_message_response() -> Value {
    json!({
        "id": MSG_ID_TEXT,
        "conversation_id": CONV_ID_DM,
        "sequence": 42,
        "sender_id": USER_ID_AUTH,
        "kind": "TEXT",
        "content_json": "{\"text\":\"你好\"}",
        "reply_to": null,
        "state": "sent",
        "created_at": "2026-08-23T00:00:00Z",
        "edited_at": null,
    })
}

// =============================================================================
// §2.2 ListMessages
// =============================================================================

/// `ListMessagesRequest`(per aux-13 §2.2)
pub fn list_messages_request() -> Value {
    json!({
        "conversation_id": CONV_ID_DM,
        "user_id": USER_ID_AUTH,
        "after_sequence": 42,
        "limit": 50,
    })
}

/// `ListMessagesResponse`(per aux-13 §2.2)
pub fn list_messages_response() -> Value {
    json!({
        "messages": [
            {
                "id": MSG_ID_TEXT,
                "conversation_id": CONV_ID_DM,
                "sequence": 43,
                "sender_id": USER_ID_AUTH,
                "kind": "TEXT",
                "content_json": "{\"text\":\"你好\"}",
                "reply_to": null,
                "state": "sent",
                "created_at": "2026-08-23T00:00:00Z",
                "edited_at": null,
            }
        ],
        "has_more": false,
        "latest_sequence": 43,
    })
}

// =============================================================================
// §2.3 ValidateAccessToken
// =============================================================================

/// `ValidateAccessTokenRequest`(per aux-13 §2.3)
pub fn validate_access_token_request() -> Value {
    json!({
        "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
    })
}

/// `ValidateAccessTokenResponse`(per aux-13 §2.3)
pub fn validate_access_token_response() -> Value {
    json!({
        "valid": true,
        "user_id": USER_ID_AUTH,
        "environment_id": ENV_ID_DEFAULT,
        "tenant_id": TENANT_ID_DEFAULT,
        "expires_at_unix": TOKEN_EXPIRES_AT_UNIX,
    })
}

/// `ValidateAccessTokenResponse` 失败版(per aux-13 §2.3)
pub fn validate_access_token_response_invalid() -> Value {
    json!({
        "valid": false,
        "user_id": "",
        "environment_id": "",
        "tenant_id": "",
        "expires_at_unix": 0,
    })
}

// =============================================================================
// §2.4 MarkRead
// =============================================================================

/// `MarkReadRequest`(per aux-13 §2.4)
pub fn mark_read_request() -> Value {
    json!({
        "conversation_id": CONV_ID_DM,
        "user_id": USER_ID_AUTH,
        "sequence": 42,
    })
}

/// `google.protobuf.Empty` 响应(per aux-13 §2.4)
pub fn empty_response() -> Value {
    json!({})
}

// =============================================================================
// 辅助:与 im-core 类型互转(若调用方需 ID newtype)
// =============================================================================

/// 把 `CONV_ID_DM` 转 `ConversationId`
pub fn conv_id() -> ConversationId {
    Uuid::parse_str(CONV_ID_DM).expect("valid uuid").into()
}

/// 把 `USER_ID_AUTH` 转 `UserId`
pub fn user_id() -> UserId {
    Uuid::parse_str(USER_ID_AUTH).expect("valid uuid").into()
}

/// 把 `ENV_ID_DEFAULT` 转 `EnvironmentId`
pub fn env_id() -> EnvironmentId {
    Uuid::parse_str(ENV_ID_DEFAULT).expect("valid uuid").into()
}

/// 把 `TENANT_ID_DEFAULT` 转 `TenantId`
pub fn tenant_id() -> TenantId {
    Uuid::parse_str(TENANT_ID_DEFAULT).expect("valid uuid").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_message_request_has_required_fields() {
        let v = send_message_request();
        assert_eq!(v["conversation_id"], CONV_ID_DM);
        assert_eq!(v["sender_id"], USER_ID_AUTH);
        // kind: TEXT 是 enum name(per core.proto)
        assert!(v["kind"].as_str().unwrap().contains("TEXT"));
    }

    #[test]
    fn send_message_response_has_sequence_42() {
        let v = send_message_response();
        assert_eq!(v["sequence"], 42);
        assert_eq!(v["state"], "sent");
    }

    #[test]
    fn list_messages_response_has_more_flag() {
        let v = list_messages_response();
        assert_eq!(v["has_more"], false);
        assert_eq!(v["latest_sequence"], 43);
        assert!(v["messages"].is_array());
    }

    #[test]
    fn validate_access_token_response_valid() {
        let v = validate_access_token_response();
        assert_eq!(v["valid"], true);
        assert_eq!(v["user_id"], USER_ID_AUTH);
    }

    #[test]
    fn mark_read_request_has_user_id() {
        let v = mark_read_request();
        assert_eq!(v["user_id"], USER_ID_AUTH);
        assert_eq!(v["sequence"], 42);
    }

    #[test]
    fn empty_response_is_empty_object() {
        let v = empty_response();
        assert!(v.is_object());
        assert_eq!(v.as_object().unwrap().len(), 0);
    }

    #[test]
    fn id_helpers_return_non_nil() {
        assert_ne!(conv_id().0, Uuid::nil());
        assert_ne!(user_id().0, Uuid::nil());
        assert_ne!(env_id().0, Uuid::nil());
        assert_ne!(tenant_id().0, Uuid::nil());
    }
}
