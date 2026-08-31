//! IM1.0 特定断言
//!
//! 3 类断言:
//! - [`assert_error_code`] —— ErrorCode 严格相等(配合 aux-03 §B 21 项)
//! - [`assert_ws_frame_shape`] —— WS frame 的 `type` 字段 vs 期望
//! - [`assert_json_schema`] —— message content 按 kind 对齐 aux-13 §4.1

use serde_json::Value;

use im_common::ErrorCode;
use im_protocol::{
    content::MessageContent, error_body::ErrorBody,
    ws_frames::{ClientFrame, ServerFrame},
};

// =============================================================================
// ErrorCode 断言
// =============================================================================

/// 断言 `actual == expected`,带可读失败信息
pub fn assert_error_code(actual: ErrorCode, expected: ErrorCode) {
    assert_eq!(
        actual, expected,
        "ErrorCode 不一致: actual={} ({}), expected={} ({})",
        actual.as_str(),
        actual.http_status(),
        expected.as_str(),
        expected.http_status()
    );
}

/// 断言 `actual` 字符串与 `ErrorCode` 解析后一致(供 JSON wire 验真)
pub fn assert_error_code_str(actual: &str, expected: ErrorCode) {
    let parsed: ErrorCode = actual.parse().unwrap_or(ErrorCode::InternalError);
    assert_eq!(
        parsed, expected,
        "wire 错误码字符串不一致: actual={}, parsed={}, expected={}",
        actual, parsed, expected
    );
}

// =============================================================================
// WS frame 断言
// =============================================================================

/// 断言一个 `ServerFrame` 变体匹配期望 kind 字符串
///
/// `expected_kind` ∈ {`"connected"`, `"ack"`, `"message_new"`, `"message_edited"`,
/// `"message_recalled"`, `"reaction_added"`, `"presence_update"`, `"typing"`,
/// `"pong"`, `"force_disconnect"`}
pub fn assert_ws_frame_shape(actual: &ServerFrame, expected_kind: &str) {
    let actual_kind = server_frame_kind(actual);
    assert_eq!(
        actual_kind, expected_kind,
        "WS server frame kind 不一致: actual={}, expected={}",
        actual_kind, expected_kind
    );
}

/// 断言一个 `ClientFrame` 变体匹配期望 kind 字符串
pub fn assert_client_frame_shape(actual: &ClientFrame, expected_kind: &str) {
    let actual_kind = client_frame_kind(actual);
    assert_eq!(
        actual_kind, expected_kind,
        "WS client frame kind 不一致: actual={}, expected={}",
        actual_kind, expected_kind
    );
}

/// 从 `ServerFrame` 提取 `type` 字符串(per im-protocol::ws_frames `serde(tag = "type")`)
fn server_frame_kind(frame: &ServerFrame) -> &'static str {
    match frame {
        ServerFrame::Connected { .. } => "connected",
        ServerFrame::Ack { .. } => "ack",
        ServerFrame::MessageEdited { .. } => "message_edited",
        ServerFrame::MessageRecalled { .. } => "message_recalled",
        ServerFrame::ReactionAdded { .. } => "reaction_added",
        ServerFrame::PresenceUpdate { .. } => "presence_update",
        ServerFrame::Typing { .. } => "typing",
        ServerFrame::Pong { .. } => "pong",
        ServerFrame::ForceDisconnect { .. } => "force_disconnect",
    }
}

fn client_frame_kind(frame: &ClientFrame) -> &'static str {
    match frame {
        ClientFrame::Auth { .. } => "auth",
        ClientFrame::SendMessage { .. } => "send_message",
        ClientFrame::EditMessage { .. } => "edit_message",
        ClientFrame::RecallMessage { .. } => "recall_message",
        ClientFrame::React { .. } => "react",
        ClientFrame::MarkRead { .. } => "mark_read",
        ClientFrame::Typing { .. } => "typing",
        ClientFrame::Ping { .. } => "ping",
    }
}

// =============================================================================
// JSON schema 断言(per aux-13 §4.1)
// =============================================================================

/// 6 个 message content kind(per aux-13 §4.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSchemaId {
    Text,
    Image,
    File,
    Sticker,
    System,
    Custom,
}

impl MessageSchemaId {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageSchemaId::Text => "text",
            MessageSchemaId::Image => "image",
            MessageSchemaId::File => "file",
            MessageSchemaId::Sticker => "sticker",
            MessageSchemaId::System => "system",
            MessageSchemaId::Custom => "custom",
        }
    }
}

/// 断言 JSON value 满足对应 kind schema(必填字段存在)
pub fn assert_json_schema(actual: &Value, schema_id: MessageSchemaId) {
    let obj = actual
        .as_object()
        .unwrap_or_else(|| panic!("JSON 必须为 object: schema_id={}", schema_id.as_str()));

    match schema_id {
        MessageSchemaId::Text => {
            assert!(
                obj.contains_key("text"),
                "[text] 缺少必填字段 `text`: actual={:?}",
                actual
            );
        }
        MessageSchemaId::Image => {
            assert!(
                obj.contains_key("media_id"),
                "[image] 缺少必填字段 `media_id`: actual={:?}",
                actual
            );
        }
        MessageSchemaId::File => {
            assert!(
                obj.contains_key("media_id"),
                "[file] 缺少必填字段 `media_id`"
            );
            assert!(
                obj.contains_key("file_name"),
                "[file] 缺少必填字段 `file_name`"
            );
            assert!(
                obj.contains_key("size_bytes"),
                "[file] 缺少必填字段 `size_bytes`"
            );
        }
        MessageSchemaId::Sticker => {
            assert!(
                obj.contains_key("sticker_id"),
                "[sticker] 缺少必填字段 `sticker_id`"
            );
        }
        MessageSchemaId::System => {
            assert!(
                obj.contains_key("event"),
                "[system] 缺少必填字段 `event`"
            );
        }
        MessageSchemaId::Custom => {
            assert!(
                obj.contains_key("schema"),
                "[custom] 缺少必填字段 `schema`"
            );
            assert!(
                obj.contains_key("data"),
                "[custom] 缺少必填字段 `data`"
            );
        }
    }
}

/// 断言 `MessageContent` 强类型与 schema 一致
pub fn assert_message_content_schema(content: &MessageContent, expected: MessageSchemaId) {
    assert_eq!(
        content.kind_name(),
        expected.as_str(),
        "MessageContent kind 不一致"
    );
    // 复用 Value 路径做必填字段校验
    let v = serde_json::to_value(content).expect("MessageContent 可序列化");
    assert_json_schema(&v, expected);
}

// =============================================================================
// ErrorBody 断言
// =============================================================================

/// 断言 ErrorBody 的 `code` 字段匹配期望错误码
pub fn assert_error_body_code(body: &ErrorBody, expected: ErrorCode) {
    assert_eq!(
        body.code, expected.as_str(),
        "ErrorBody.code 不一致: actual={}, expected={}",
        body.code,
        expected.as_str()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_ws_frames;

    #[test]
    fn assert_error_code_passes_on_match() {
        assert_error_code(ErrorCode::Unauthorized, ErrorCode::Unauthorized);
    }

    #[test]
    fn assert_error_code_str_passes_on_match() {
        assert_error_code_str("RATE_LIMITED", ErrorCode::RateLimited);
    }

    #[test]
    fn assert_error_code_str_falls_back_to_internal() {
        // 未知字符串走 aux-03 兜底
        assert_error_code_str("NOT_A_REAL_CODE", ErrorCode::InternalError);
    }

    #[test]
    #[should_panic(expected = "ErrorCode 不一致")]
    fn assert_error_code_panics_on_mismatch() {
        assert_error_code(ErrorCode::Unauthorized, ErrorCode::NotFound);
    }

    #[test]
    fn assert_ws_frame_shape_passes_on_ping() {
        let frame = mock_ws_frames::ping_frame();
        assert_client_frame_shape(&frame, "ping");
    }

    #[test]
    fn assert_ws_frame_shape_passes_on_connected() {
        let frame = mock_ws_frames::connected_frame();
        assert_ws_frame_shape(&frame, "connected");
    }

    #[test]
    fn assert_ws_frame_shape_passes_on_pong() {
        let frame = mock_ws_frames::pong_frame();
        assert_ws_frame_shape(&frame, "pong");
    }

    #[test]
    fn assert_message_content_schema_text_passes() {
        let c = MessageContent::Text {
            text: "hi".into(),
        };
        assert_message_content_schema(&c, MessageSchemaId::Text);
    }

    #[test]
    fn assert_message_content_schema_image_passes() {
        let c = MessageContent::Image {
            media_id: uuid::Uuid::new_v4(),
            width: Some(800),
            height: Some(600),
            thumbnail_media_id: None,
        };
        assert_message_content_schema(&c, MessageSchemaId::Image);
    }

    #[test]
    fn assert_json_schema_text_passes_on_object_with_text_field() {
        let v = serde_json::json!({ "text": "hello" });
        assert_json_schema(&v, MessageSchemaId::Text);
    }

    #[test]
    #[should_panic(expected = "[file] 缺少必填字段")]
    fn assert_json_schema_file_panics_on_missing_field() {
        let v = serde_json::json!({ "media_id": "x" });
        assert_json_schema(&v, MessageSchemaId::File);
    }

    #[test]
    fn assert_error_body_code_passes() {
        let body = mock_ws_frames::validation_error_body();
        assert_error_body_code(&body, ErrorCode::ValidationError);
    }
}
