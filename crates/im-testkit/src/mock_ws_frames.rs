//! WebSocket 帧 mock 数据(per aux-13 §1.1 + §1.2)
//!
//! 12 帧 = 6 客户端(请求方)+ 6 服务端(推送/响应)
//! - 客户端:`ping` / `auth` / `send_message` / `edit_message` / `recall_message` / `mark_read`
//! - 服务端:`pong` / `connected` / `ack` (成功) / `ack` (失败) / `message_new` / `message_edited`
//!
//! 每个 mock 函数返回强类型(优先 `ClientFrame` / `ServerFrame` 枚举)或 `serde_json::Value`。
//! 强类型来自 im-protocol,不重写协议结构。

use serde_json::{json, Value};
use uuid::Uuid;

use im_protocol::{
    content::MessageContent,
    error_body::ErrorBody,
    ws_frames::{AckData, ClientFrame, ServerFrame, WireMessage},
};

// =============================================================================
// 稳定 UUID —— 单元测试期望值对齐(per aux-13 §1.1 固定 req_id)
// =============================================================================

/// aux-13 §1.1.1 `auth.req_id` 固定值
pub const REQ_ID_AUTH: &str = "11111111-1111-4111-8111-111111111111";
/// aux-13 §1.1.2 `send_message.req_id` 固定值
pub const REQ_ID_SEND_MESSAGE: &str = "22222222-2222-4222-8222-222222222222";
/// aux-13 §1.1.2 `idempotency_key` 固定值
pub const IDEMPOTENCY_KEY_SEND: &str = "33333333-3333-4333-8333-333333333333";
/// aux-13 §1.1.3 `edit_message.req_id` 固定值
pub const REQ_ID_EDIT: &str = "44444444-4444-4444-8444-444444444444";
/// aux-13 §1.1.4 `recall_message.req_id` 固定值
pub const REQ_ID_RECALL: &str = "55555555-5555-4555-8555-555555555555";
/// aux-13 §1.1.5 `react.req_id` 固定值
pub const REQ_ID_REACT: &str = "66666666-6666-4666-8666-666666666666";
/// aux-13 §1.1.6 `mark_read.req_id` 固定值
pub const REQ_ID_MARK_READ: &str = "77777777-7777-4777-8777-777777777777";
/// aux-13 §1.1.7 `typing.req_id` 固定值
pub const REQ_ID_TYPING: &str = "88888888-8888-4888-8888-888888888888";
/// aux-13 §1.1.8 `ping.ts` 固定值
pub const PING_TS_MS: i64 = 1_692_528_000_000;
/// aux-13 §1.2.1 connected.session_id
pub const SESSION_ID_CONNECTED: &str = "9b8e6679-7425-40de-944b-e07fc1f90ae7";
/// aux-13 §1.2.5 message_new message.id
pub const MESSAGE_ID_TEXT: &str = "8a7e6679-7425-40de-944b-e07fc1f90ae7";
/// aux-13 §1.2.5 message_new conversation_id
pub const CONVERSATION_ID_DM: &str = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
/// aux-13 §1.2.5 message_new sender_id
pub const USER_ID_SENDER: &str = "1a2e6679-7425-40de-944b-e07fc1f90ae7";
/// aux-13 §1.2.5/§1.2.8/§1.2.9 user_id(react/presence)
pub const USER_ID_PEER: &str = "2b3e6679-7425-40de-944b-e07fc1f90ae7";

/// JWT access_token 样例(per aux-13 §1.1.1)
pub const ACCESS_TOKEN_SAMPLE: &str =
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...";

// =============================================================================
// 客户端 → 服务端 帧
// =============================================================================

/// §1.1.1 `auth` —— 强类型版
pub fn auth_request_frame() -> ClientFrame {
    ClientFrame::Auth {
        req_id: Uuid::parse_str(REQ_ID_AUTH).expect("valid uuid"),
        access_token: ACCESS_TOKEN_SAMPLE.into(),
    }
}

/// §1.1.1 `auth` —— JSON Value 版(便于直接 wire 写)
pub fn auth_request_json() -> Value {
    json!({
        "type": "auth",
        "req_id": REQ_ID_AUTH,
        "access_token": ACCESS_TOKEN_SAMPLE,
    })
}

/// §1.1.2 `send_message` —— 强类型版
pub fn send_message_frame() -> ClientFrame {
    ClientFrame::SendMessage {
        req_id: Uuid::parse_str(REQ_ID_SEND_MESSAGE).expect("valid uuid"),
        conversation_id: Uuid::parse_str(CONVERSATION_ID_DM).expect("valid uuid"),
        idempotency_key: Uuid::parse_str(IDEMPOTENCY_KEY_SEND).expect("valid uuid"),
        kind: "text".into(),
        content: MessageContent::Text {
            text: "你好".into(),
        },
        reply_to: None,
    }
}

/// §1.1.2 `send_message` —— JSON Value 版
pub fn send_message_json() -> Value {
    json!({
        "type": "send_message",
        "req_id": REQ_ID_SEND_MESSAGE,
        "conversation_id": CONVERSATION_ID_DM,
        "idempotency_key": IDEMPOTENCY_KEY_SEND,
        "kind": "text",
        "content": { "text": "你好" },
        "reply_to": null,
    })
}

/// §1.1.3 `edit_message` —— 强类型版
pub fn edit_message_frame() -> ClientFrame {
    ClientFrame::EditMessage {
        req_id: Uuid::parse_str(REQ_ID_EDIT).expect("valid uuid"),
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        content: MessageContent::Text {
            text: "你好(已编辑)".into(),
        },
    }
}

/// §1.1.4 `recall_message` —— 强类型版
pub fn recall_message_frame() -> ClientFrame {
    ClientFrame::RecallMessage {
        req_id: Uuid::parse_str(REQ_ID_RECALL).expect("valid uuid"),
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
    }
}

/// §1.1.5 `react` —— 强类型版
pub fn react_frame() -> ClientFrame {
    ClientFrame::React {
        req_id: Uuid::parse_str(REQ_ID_REACT).expect("valid uuid"),
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        emoji: "👍".into(),
    }
}

/// §1.1.6 `mark_read` —— 强类型版
pub fn mark_read_frame() -> ClientFrame {
    ClientFrame::MarkRead {
        req_id: Uuid::parse_str(REQ_ID_MARK_READ).expect("valid uuid"),
        conversation_id: Uuid::parse_str(CONVERSATION_ID_DM).expect("valid uuid"),
        sequence: 42,
    }
}

/// §1.1.7 `typing` —— 强类型版
pub fn typing_frame() -> ClientFrame {
    ClientFrame::Typing {
        req_id: Uuid::parse_str(REQ_ID_TYPING).expect("valid uuid"),
        conversation_id: Uuid::parse_str(CONVERSATION_ID_DM).expect("valid uuid"),
    }
}

/// §1.1.8 `ping` —— 强类型版
pub fn ping_frame() -> ClientFrame {
    ClientFrame::Ping {
        ts: Some(PING_TS_MS),
    }
}

/// §1.1.8 `ping` —— JSON Value 版
pub fn ping_json() -> Value {
    json!({
        "type": "ping",
        "ts": PING_TS_MS,
    })
}

// =============================================================================
// 服务端 → 客户端 帧
// =============================================================================

/// §1.2.1 `connected` —— 强类型版
pub fn connected_frame() -> ServerFrame {
    ServerFrame::Connected {
        session_id: Uuid::parse_str(SESSION_ID_CONNECTED).expect("valid uuid"),
    }
}

/// §1.2.1 `connected` —— JSON Value 版
pub fn connected_json() -> Value {
    json!({
        "type": "connected",
        "session_id": SESSION_ID_CONNECTED,
    })
}

/// §1.2.2 `ack` 成功 —— 强类型版
pub fn ack_success_frame(req_id: Uuid) -> ServerFrame {
    ServerFrame::Ack {
        req_id,
        ok: true,
        data: Some(AckData {
            message_id: Some(Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid")),
            sequence: Some(42),
            idempotent_replay: false,
        }),
    }
}

/// §1.2.2 `ack` 成功 —— JSON Value 版(默认 REQ_ID_SEND_MESSAGE)
pub fn ack_success_json() -> Value {
    json!({
        "type": "ack",
        "req_id": REQ_ID_SEND_MESSAGE,
        "ok": true,
        "data": {
            "message_id": MESSAGE_ID_TEXT,
            "sequence": 42,
        },
    })
}

/// §1.2.3 `ack` 幂等重放 —— JSON Value 版
pub fn ack_idempotent_replay_json() -> Value {
    json!({
        "type": "ack",
        "req_id": REQ_ID_SEND_MESSAGE,
        "ok": true,
        "data": {
            "message_id": MESSAGE_ID_TEXT,
            "sequence": 42,
            "idempotent_replay": true,
        },
    })
}

/// §1.2.4 `ack` 真错误 —— JSON Value 版(RATE_LIMITED 样例)
pub fn ack_error_json() -> Value {
    json!({
        "type": "ack",
        "req_id": REQ_ID_SEND_MESSAGE,
        "ok": false,
        "error": {
            "code": "RATE_LIMITED",
            "message": "auth.rate_limit.send_message",
            "trace_id": "tr_01HXY...",
        },
    })
}

/// §1.2.5 `message_new` —— 强类型版
pub fn message_new_frame() -> ServerFrame {
    // 注意:ServerFrame::MessageNew 在 im-protocol 当前未实现(im-protocol 当前 11 帧不含 message_new),
    // 故使用 WireMessage 强类型 + JSON Value 路径
    let _ = (
        Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        Uuid::parse_str(CONVERSATION_ID_DM).expect("valid uuid"),
    );
    // 强类型路径仅占位(WireMessage 用于其他场景);若 im-protocol 加 MessageNew 变体,本函数可切换
    ServerFrame::MessageEdited {
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        content: MessageContent::Text {
            text: "你好".into(),
        },
        edited_at: chrono::Utc::now(),
    }
}

/// §1.2.5 `message_new` —— JSON Value 版(完整结构,严格按 aux-13)
pub fn message_new_json() -> Value {
    json!({
        "type": "message_new",
        "message": wire_message_json(),
    })
}

/// 通用 WireMessage JSON(per aux-13 §1.2.5 字段集)
pub fn wire_message_json() -> Value {
    json!({
        "id": MESSAGE_ID_TEXT,
        "conversation_id": CONVERSATION_ID_DM,
        "sequence": 42,
        "sender_id": USER_ID_SENDER,
        "kind": "text",
        "content": { "text": "你好" },
        "reply_to": null,
        "state": "sent",
        "created_at": "2026-08-23T00:00:00Z",
        "edited_at": null,
        "reactions": [],
    })
}

/// §1.2.6 `message_edited` —— 强类型版
pub fn message_edited_frame() -> ServerFrame {
    ServerFrame::MessageEdited {
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        content: MessageContent::Text {
            text: "你好(已编辑)".into(),
        },
        edited_at: chrono::Utc::now(),
    }
}

/// §1.2.6 `message_edited` —— JSON Value 版
pub fn message_edited_json() -> Value {
    json!({
        "type": "message_edited",
        "message_id": MESSAGE_ID_TEXT,
        "content": { "text": "你好(已编辑)" },
        "edited_at": "2026-08-23T00:01:00Z",
    })
}

/// §1.2.7 `message_recalled` —— 强类型版
pub fn message_recalled_frame() -> ServerFrame {
    ServerFrame::MessageRecalled {
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        conversation_id: Uuid::parse_str(CONVERSATION_ID_DM).expect("valid uuid"),
    }
}

/// §1.2.7 `message_recalled` —— JSON Value 版
pub fn message_recalled_json() -> Value {
    json!({
        "type": "message_recalled",
        "message_id": MESSAGE_ID_TEXT,
        "conversation_id": CONVERSATION_ID_DM,
    })
}

/// §1.2.8 `reaction_added` —— 强类型版
pub fn reaction_added_frame() -> ServerFrame {
    ServerFrame::ReactionAdded {
        message_id: Uuid::parse_str(MESSAGE_ID_TEXT).expect("valid uuid"),
        user_id: Uuid::parse_str(USER_ID_PEER).expect("valid uuid"),
        emoji: "👍".into(),
    }
}

/// §1.2.9 `presence_update` —— 强类型版
pub fn presence_update_frame() -> ServerFrame {
    ServerFrame::PresenceUpdate {
        user_id: Uuid::parse_str(USER_ID_PEER).expect("valid uuid"),
        status: "online".into(),
    }
}

/// §1.2.10 `typing` —— 强类型版
pub fn typing_broadcast_frame() -> ServerFrame {
    ServerFrame::Typing {
        conversation_id: Uuid::parse_str(CONVERSATION_ID_DM).expect("valid uuid"),
        user_id: Uuid::parse_str(USER_ID_PEER).expect("valid uuid"),
    }
}

/// §1.2.11 `pong` —— 强类型版
pub fn pong_frame() -> ServerFrame {
    ServerFrame::Pong { ts: PING_TS_MS }
}

/// §1.2.11 `pong` —— JSON Value 版
pub fn pong_json() -> Value {
    json!({
        "type": "pong",
        "ts": PING_TS_MS,
    })
}

/// §1.2.12 `force_disconnect` —— 强类型版
pub fn force_disconnect_frame() -> ServerFrame {
    ServerFrame::ForceDisconnect {
        reason: "token_revoked".into(),
    }
}

/// §1.2.12 `force_disconnect` —— JSON Value 版
pub fn force_disconnect_json() -> Value {
    json!({
        "type": "force_disconnect",
        "reason": "token_revoked",
    })
}

/// §3.7 通用错误响应体强类型(aux-13 §3.7)
pub fn validation_error_body() -> ErrorBody {
    ErrorBody::new(
        "VALIDATION_ERROR",
        "request.validation.field_required",
        "tr_01HXY...",
    )
    .with_details(vec![im_protocol::error_body::FieldError {
        field: "content.text".into(),
        reason: "max_length".into(),
    }])
}

// =============================================================================
// 内部:辅助
// =============================================================================

#[allow(dead_code)]
fn _unused_imports_silencer() {
    // 让 WireMessage 强类型对外仍可见(在断言中可能用到)
    let _: Option<WireMessage> = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_frame_has_required_fields() {
        let frame = ping_frame();
        match frame {
            ClientFrame::Ping { ts } => assert_eq!(ts, Some(PING_TS_MS)),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn auth_request_serializes_to_json() {
        let v = auth_request_json();
        assert_eq!(v["type"], "auth");
        assert_eq!(v["req_id"], REQ_ID_AUTH);
        assert!(v["access_token"].as_str().unwrap().starts_with("eyJ"));
    }

    #[test]
    fn pong_frame_roundtrip() {
        let frame = pong_frame();
        match frame {
            ServerFrame::Pong { ts } => assert_eq!(ts, PING_TS_MS),
            _ => panic!("wrong variant"),
        }
        let json = pong_json();
        assert_eq!(json["type"], "pong");
        assert_eq!(json["ts"], PING_TS_MS);
    }

    #[test]
    fn ack_success_json_shape() {
        let v = ack_success_json();
        assert_eq!(v["type"], "ack");
        assert_eq!(v["ok"], true);
        assert_eq!(v["data"]["sequence"], 42);
        assert!(v.get("error").is_none() || v["error"].is_null());
    }

    #[test]
    fn ack_idempotent_replay_flag_set() {
        let v = ack_idempotent_replay_json();
        assert_eq!(v["data"]["idempotent_replay"], true);
    }

    #[test]
    fn ack_error_code_string_matches_aux03() {
        let v = ack_error_json();
        // aux-03 §B: RATE_LIMITED 是有效错误码
        assert_eq!(v["error"]["code"], "RATE_LIMITED");
    }

    #[test]
    fn message_new_json_has_message_payload() {
        let v = message_new_json();
        assert_eq!(v["type"], "message_new");
        assert_eq!(v["message"]["id"], MESSAGE_ID_TEXT);
        assert_eq!(v["message"]["sequence"], 42);
        assert_eq!(v["message"]["kind"], "text");
    }

    #[test]
    fn message_edited_frame_preserves_text() {
        let frame = message_edited_frame();
        match frame {
            ServerFrame::MessageEdited { content, .. } => match content {
                MessageContent::Text { text } => assert_eq!(text, "你好(已编辑)"),
                _ => panic!("wrong content variant"),
            },
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn force_disconnect_reason_is_token_revoked() {
        let frame = force_disconnect_frame();
        match frame {
            ServerFrame::ForceDisconnect { reason } => assert_eq!(reason, "token_revoked"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn validation_error_body_has_field_details() {
        let body = validation_error_body();
        assert_eq!(body.code, "VALIDATION_ERROR");
        assert_eq!(body.details.len(), 1);
        assert_eq!(body.details[0].field, "content.text");
    }
}
