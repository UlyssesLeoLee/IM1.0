//! REST API mock 数据(per aux-13 §3)
//!
//! 7 个 endpoint mock:
//! 1. `POST /v1/auth/token/exchange`(§3.1)
//! 2. `POST /v1/auth/guest`(§3.2)
//! 3. `POST /v1/conversations`(§3.3)
//! 4. `GET  /v1/conversations/{id}/messages`(§3.4)
//! 5. `POST /v1/conversations/{id}/messages`(§3.5)
//! 6. `POST /v1/media/presign`(§3.6)
//! 7. 通用错误响应(§3.7)
//!
//! 每个 endpoint 给出 Request body + Response body(成功/失败) JSON Value。

use serde_json::{json, Value};

// =============================================================================
// 稳定 ID / Token(对齐 aux-13 §3 + mock_grpc.rs)
// =============================================================================

const ENV_ID_DEFAULT: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
const USER_ID_DEFAULT: &str = "1a2e6679-7425-40de-944b-e07fc1f90ae7";
const PEER_USER_ID: &str = "2b3e6679-7425-40de-944b-e07fc1f90ae7";
const CONV_ID_DM: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
const MSG_ID_TEXT: &str = "8a7e6679-7425-40de-944b-e07fc1f90ae7";
const MEDIA_ID_SAMPLE: &str = "5c6e6679-7425-40de-944b-e07fc1f90ae7";
const REFRESH_TOKEN_SAMPLE: &str = "rt_9b8e6679-7425-40de-944b-e07fc1f90ae7";

/// §3.1 响应 expires_in(per aux-13 §3.1)
pub const ACCESS_TOKEN_TTL_SECONDS: u32 = 900;

// =============================================================================
// §3.1 POST /v1/auth/token/exchange
// =============================================================================

/// §3.1 请求体
pub fn login_request() -> Value {
    json!({
        "environment_id": ENV_ID_DEFAULT,
        "external_provider": "steam",
        "external_uid": "76561198000000000",
        "display_name": "Player1",
    })
}

/// §3.1 成功响应
pub fn login_response() -> Value {
    json!({
        "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
        "refresh_token": REFRESH_TOKEN_SAMPLE,
        "user_id": USER_ID_DEFAULT,
        "expires_in": ACCESS_TOKEN_TTL_SECONDS,
    })
}

/// §3.1 失败响应(401)
pub fn login_unauthorized_response() -> Value {
    json!({
        "code": "UNAUTHORIZED",
        "message": "auth.server_signature.invalid",
        "trace_id": "tr_01HXY...",
        "ts": 1_692_528_000_000_i64,
    })
}

// =============================================================================
// §3.2 POST /v1/auth/guest
// =============================================================================

/// §3.2 请求体
pub fn guest_register_request() -> Value {
    json!({
        "environment_id": ENV_ID_DEFAULT,
    })
}

/// §3.2 成功响应(同 §3.1 形状)
pub fn guest_register_response() -> Value {
    login_response()
}

// =============================================================================
// §3.3 POST /v1/conversations
// =============================================================================

/// §3.3 请求体
pub fn create_conversation_request() -> Value {
    json!({
        "kind": "dm",
        "member_user_ids": [PEER_USER_ID],
    })
}

/// §3.3 成功响应(201)
pub fn create_conversation_response() -> Value {
    json!({
        "id": CONV_ID_DM,
        "environment_id": ENV_ID_DEFAULT,
        "kind": "dm",
        "metadata": {},
        "created_at": "2026-08-23T00:00:00Z",
    })
}

/// §3.3 失败响应(409 ACCOUNT_MERGE_CONFLICT)
pub fn create_conversation_blocked_response() -> Value {
    json!({
        "code": "ACCOUNT_MERGE_CONFLICT",
        "message": "conversation.dm.peer_blocked",
        "trace_id": "tr_01HXY...",
        "ts": 1_692_528_000_000_i64,
    })
}

// =============================================================================
// §3.4 GET /v1/conversations/{id}/messages
// =============================================================================

/// §3.4 响应体
pub fn list_messages_response() -> Value {
    json!({
        "messages": [
            {
                "id": MSG_ID_TEXT,
                "conversation_id": CONV_ID_DM,
                "sequence": 43,
                "sender_id": PEER_USER_ID,
                "kind": "text",
                "content": { "text": "你好" },
                "reply_to": null,
                "state": "sent",
                "created_at": "2026-08-23T00:00:00Z",
                "edited_at": null,
                "reactions": []
            }
        ],
        "next_cursor": "44",
        "has_more": false,
    })
}

// =============================================================================
// §3.5 POST /v1/conversations/{id}/messages
// =============================================================================

/// §3.5 请求体
pub fn send_message_rest_request() -> Value {
    json!({
        "kind": "text",
        "content": { "text": "你好" },
        "reply_to": null,
    })
}

/// §3.5 成功响应(200,完整 message 结构)
pub fn send_message_rest_response() -> Value {
    json!({
        "id": MSG_ID_TEXT,
        "conversation_id": CONV_ID_DM,
        "sequence": 42,
        "sender_id": USER_ID_DEFAULT,
        "kind": "text",
        "content": { "text": "你好" },
        "reply_to": null,
        "state": "sent",
        "created_at": "2026-08-23T00:00:00Z",
        "edited_at": null,
        "reactions": []
    })
}

/// §3.5 幂等冲突响应(200,带 `idempotent_replay`)
pub fn send_message_rest_idempotent_replay() -> Value {
    let mut v = send_message_rest_response();
    v["idempotent_replay"] = json!(true);
    v
}

// =============================================================================
// §3.6 POST /v1/media/presign
// =============================================================================

/// §3.6 请求体
pub fn presign_media_request() -> Value {
    json!({
        "content_type": "image/png",
        "size_hint": 102_400,
    })
}

/// §3.6 成功响应
pub fn presign_media_response() -> Value {
    json!({
        "upload_url": "https://minio.example.com/im-media/...?X-Amz-Signature=...",
        "media_id": MEDIA_ID_SAMPLE,
        "expires_at": "2026-08-23T01:00:00Z",
    })
}

// =============================================================================
// §3.7 通用错误响应
// =============================================================================

/// §3.7 错误响应(VALIDATION_ERROR,带 details)
pub fn validation_error_response() -> Value {
    json!({
        "code": "VALIDATION_ERROR",
        "message": "request.validation.field_required",
        "trace_id": "tr_01HXY...",
        "ts": 1_692_528_000_000_i64,
        "details": [
            { "field": "content.text", "reason": "max_length" }
        ],
    })
}

/// §3.7 错误响应(NOT_FOUND,无 details)
pub fn not_found_error_response() -> Value {
    json!({
        "code": "NOT_FOUND",
        "message": "request.resource.not_found",
        "trace_id": "tr_01HXY...",
        "ts": 1_692_528_000_000_i64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_request_has_external_provider() {
        let v = login_request();
        assert_eq!(v["external_provider"], "steam");
        assert!(v["external_uid"].as_str().unwrap().starts_with("7656119"));
    }

    #[test]
    fn login_response_expires_in_900() {
        let v = login_response();
        assert_eq!(v["expires_in"], 900);
        assert_eq!(v["user_id"], USER_ID_DEFAULT);
    }

    #[test]
    fn login_unauthorized_code_string() {
        let v = login_unauthorized_response();
        assert_eq!(v["code"], "UNAUTHORIZED");
    }

    #[test]
    fn guest_register_request_minimal() {
        let v = guest_register_request();
        assert!(v.get("external_provider").is_none());
        assert!(v["environment_id"].is_string());
    }

    #[test]
    fn create_conversation_request_is_dm() {
        let v = create_conversation_request();
        assert_eq!(v["kind"], "dm");
        assert!(v["member_user_ids"].is_array());
    }

    #[test]
    fn create_conversation_blocked_code_409() {
        let v = create_conversation_blocked_response();
        assert_eq!(v["code"], "ACCOUNT_MERGE_CONFLICT");
    }

    #[test]
    fn list_messages_response_has_messages_array() {
        let v = list_messages_response();
        assert!(v["messages"].is_array());
        assert_eq!(v["has_more"], false);
    }

    #[test]
    fn send_message_rest_idempotent_replay_flag() {
        let v = send_message_rest_idempotent_replay();
        assert_eq!(v["idempotent_replay"], true);
    }

    #[test]
    fn presign_media_response_has_upload_url() {
        let v = presign_media_response();
        assert!(v["upload_url"].as_str().unwrap().starts_with("https://"));
        assert!(v["media_id"].is_string());
    }

    #[test]
    fn validation_error_response_has_field_details() {
        let v = validation_error_response();
        assert_eq!(v["code"], "VALIDATION_ERROR");
        assert_eq!(v["details"][0]["field"], "content.text");
    }

    #[test]
    fn not_found_error_no_details() {
        let v = not_found_error_response();
        assert_eq!(v["code"], "NOT_FOUND");
        assert!(v.get("details").is_none() || v["details"].is_null());
    }
}
