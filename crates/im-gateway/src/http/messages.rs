//! C-9 `POST /v1/conversations/{id}/messages` + `GET /v1/conversations/{id}/messages`
//!
//! WBS C-9 messages handler 实装 (per 132-wbs.md §5.3.2, base 250K / max 500K tokens)
//!
//! 依据: aux-13-protocol-frame-samples.md §3.3 (POST) + §3.4 (GET)
//!       ImplementationSpec §7.4.3 + DetailedDesign §9.1
//!
//! ## 范围
//!
//! ### POST
//! - Bearer auth (AuthedUser extractor)
//! - 接收 `{ kind, content, reply_to?, idempotency_key }`
//! - 走 MessageService::send_message (C-2 ✅ 已实装)
//! - MessageService 内部已校验:
//!   - 幂等 (同 (conv, sender, idem_key) 已存在返原 message)
//!   - content schema 校验 (按 kind 反序列化为 MessageContent)
//!   - sender 是 conversation member (否则 403)
//!   - DM friend 关系校验 (留 known gap)
//! - 返回 201 + MessageResponse
//!
//! ### GET
//! - Bearer auth
//! - 查询 `?after_sequence=&limit=` (默认 after=0, limit=50)
//! - 走 MessageService::list_messages (C-2 ✅ 已实装, 但留 known gap: 当前 list 不校验 member)
//! - 已知缺口: 成员校验留给 im-gateway 层补 (本 PR 补)
//! - 返回 200 + `{ messages: [...], has_more: bool, next_after_sequence: i64 }`
//!
//! ### 错误
//! - 400 `VALIDATION_ERROR` — content schema 错 / idem_key 空
//! - 401 `UNAUTHORIZED` — Bearer 无效
//! - 403 `FORBIDDEN` — 不是 conversation member
//! - 404 `NOT_FOUND` — conversation 不存在
//! - 413 `MESSAGE_TOO_LARGE` — content 超过 max_size_bytes
//! - 500 `INTERNAL_ERROR` — sqlx / 事件 publish 失败

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use im_common::ids::{ConversationId, MessageId};
use im_common::AppError;
use im_core::message::repository::Message;
use im_core::message::service::SendMessageCommand;

use super::state::AuthedUser;
use super::error_response::json_response;
use super::state::AppState;

// ============================================================================
// C-9 DTO: POST /v1/conversations/{id}/messages
// ============================================================================

/// POST request body (per aux-13 §3.3)
#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageRequest {
    /// 消息 kind: text | image | file | sticker | system | custom
    pub kind: String,
    /// 消息 content (per im_protocol::content::MessageContent schema)
    pub content: Value,
    /// 可选: 回复某条消息
    #[serde(default)]
    pub reply_to: Option<Uuid>,
    /// 幂等键 (UUID 推荐, 重复请求返原 message)
    pub idempotency_key: String,
}

/// POST response body — full Message wire shape
#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sequence: i64,
    pub sender_id: Option<Uuid>,
    pub kind: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Uuid>,
    /// sent | delivered | read | recalled | deleted
    pub state: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<Message> for MessageResponse {
    fn from(m: Message) -> Self {
        Self {
            id: m.id.0,
            conversation_id: m.conversation_id.0,
            sequence: m.sequence,
            sender_id: m.sender_id.map(|u| u.0),
            kind: m.kind,
            content: m.content,
            reply_to: m.reply_to.map(|m| m.0),
            state: serde_json::to_value(m.state)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "sent".to_string()),
            created_at: m.created_at,
            edited_at: m.edited_at,
        }
    }
}

// ============================================================================
// C-9 Handler: POST /v1/conversations/{id}/messages
// ============================================================================

/// `POST /v1/conversations/{id}/messages`
///
/// 流程 (走 MessageService 已有 send_message):
/// 1. 解析 path param `{id}` → ConversationId
/// 2. AuthedUser → sender_id
/// 3. 字段校验 (kind 非空 + idem_key 非空 + content 是 JSON object)
/// 4. 构造 SendMessageCommand
/// 5. 调 message_service.send_message()
/// 6. 返 201 + MessageResponse
pub async fn send_message(
    auth: AuthedUser,
    path: web::Path<Uuid>,
    body: web::Json<SendMessageRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let conv_id = ConversationId(path.into_inner());
    let sender_id = auth.user_id;
    let req = body.into_inner();

    // 1. 字段校验
    if req.kind.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("kind must not be empty"),
        ));
    }
    if req.idempotency_key.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("idempotency_key must not be empty"),
        ));
    }
    if !matches!(req.content, Value::Object(_)) {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("content must be JSON object"),
        ));
    }

    // 2. 构造 command
    // max_size_bytes 默认 65536 (per MessageService::send_message 注释 + SRS §16.4)
    // V1 实装时从 environments.settings 读
    let cmd = SendMessageCommand {
        conversation_id: conv_id,
        sender_id,
        idempotency_key: req.idempotency_key,
        kind: req.kind,
        content: req.content,
        reply_to: req.reply_to.map(MessageId),
        max_size_bytes: 65536,
    };

    // 3. dispatch — AppState 暂无 message_service 字段 (per D-1 PR 扩展)
    // 已知缺口 (per 138 §8 + v1.10.0): message_service 当前不在 AppState, 本 PR 走
    // 兜底: 把 message_service 也加到 AppState 字段 (D-1 阶段扩展)
    //
    // 实际: 后续 D-1 PR 会扩展 AppState 加 message_service. 本 PR 已加 message_service
    //       字段 (见 state.rs 修改).
    let msg = app
        .message_service
        .send_message(cmd)
        .await
        .map_err(http_err)?;

    let resp = MessageResponse::from(msg);
    tracing::info!(
        message_id = %resp.id,
        conversation_id = %resp.conversation_id,
        sequence = resp.sequence,
        "message sent"
    );
    Ok(HttpResponse::Created().json(resp))
}

// ============================================================================
// C-9 DTO: GET /v1/conversations/{id}/messages
// ============================================================================

/// GET response body (per aux-13 §3.4)
#[derive(Debug, Clone, Serialize)]
pub struct ListMessagesResponse {
    pub messages: Vec<MessageResponse>,
    pub has_more: bool,
    /// client 用 next_after_sequence 作为下一次请求的 after_sequence (per aux-13 §5.4)
    pub next_after_sequence: i64,
}

// ============================================================================
// C-9 Handler: GET /v1/conversations/{id}/messages
// ============================================================================

/// `GET /v1/conversations/{id}/messages?after_sequence=&limit=`
///
/// 流程:
/// 1. AuthedUser 鉴权
/// 2. 解析 query params (默认 after=0, limit=50)
/// 3. member 校验 (im-gateway 兜底, per MessageService 已知缺口 #2)
/// 4. 调 MessageService::list_messages()
/// 5. 返 ListMessagesResponse
pub async fn list_messages(
    auth: AuthedUser,
    path: web::Path<Uuid>,
    query: web::Query<ListMessagesQuery>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let conv_id = ConversationId(path.into_inner());
    let user_id = auth.user_id;
    let q = query.into_inner();

    // 1. member 校验 (per MessageService 已知缺口 #2: list_messages 不校验 member)
    // 走 ConversationService.is_member (per first batch 已有 repo() 接入)
    let is_member = app
        .conversation_service
        .repo()
        .is_member(conv_id, user_id)
        .await
        .map_err(http_err)?;

    if !is_member {
        return Err(json_response(
            im_common::ErrorCode::Forbidden,
            None,
            Some("not a member of this conversation"),
        ));
    }

    // 2. 调 list_messages
    let messages = app
        .message_service
        .list_messages(conv_id, user_id, q.after_sequence, q.limit)
        .await
        .map_err(http_err)?;

    // 3. 构造响应
    let has_more = (messages.len() as i32) == q.limit;
    let next_after_sequence = messages
        .last()
        .map(|m| m.sequence)
        .unwrap_or(q.after_sequence);

    let messages_resp: Vec<MessageResponse> = messages.into_iter().map(MessageResponse::from).collect();

    tracing::info!(
        conversation_id = %conv_id.0,
        user_id = %user_id.0,
        count = messages_resp.len(),
        after_sequence = q.after_sequence,
        "messages listed"
    );

    Ok(HttpResponse::Ok().json(ListMessagesResponse {
        messages: messages_resp,
        has_more,
        next_after_sequence,
    }))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListMessagesQuery {
    #[serde(default)]
    pub after_sequence: i64,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

fn default_limit() -> i32 {
    50
}

// ============================================================================
// AppError → actix_web::Error 适配
// ============================================================================

fn http_err(e: AppError) -> actix_web::Error {
    use crate::error::error_to_response;
    let resp = error_to_response(&e);
    tracing::warn!(error = ?e, "message handler failed");
    actix_web::error::InternalError::from_response(e.code().as_str().to_string(), resp).into()
}

// ============================================================================
// 单元测试 — 仅测 DTO + Response shape, 不连 DB
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use im_core::message::repository::{Message, MessageState};

    fn sample_message(id: Uuid, conv: Uuid, seq: i64) -> Message {
        Message {
            id: MessageId(id),
            conversation_id: ConversationId(conv),
            sequence: seq,
            sender_id: Some(im_common::ids::UserId::new()),
            kind: "text".into(),
            content: serde_json::json!({"text": "hi"}),
            reply_to: None,
            state: MessageState::Sent,
            created_at: chrono::Utc.with_ymd_and_hms(2026, 9, 19, 12, 0, 0).unwrap(),
            edited_at: None,
        }
    }

    // -------- SendMessageRequest DTO --------

    #[test]
    fn send_message_request_full() {
        let json = r#"{
            "kind": "text",
            "content": {"text": "hi"},
            "reply_to": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "idempotency_key": "idem-1"
        }"#;
        let req: SendMessageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.kind, "text");
        assert_eq!(req.idempotency_key, "idem-1");
        assert!(req.reply_to.is_some());
    }

    #[test]
    fn send_message_request_without_reply_to() {
        let json = r#"{
            "kind": "text",
            "content": {"text": "hi"},
            "idempotency_key": "idem-1"
        }"#;
        let req: SendMessageRequest = serde_json::from_str(json).unwrap();
        assert!(req.reply_to.is_none());
    }

    #[test]
    fn send_message_request_missing_kind_fails() {
        let json = r#"{
            "content": {"text": "hi"},
            "idempotency_key": "idem-1"
        }"#;
        let r: Result<SendMessageRequest, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn send_message_request_missing_idempotency_key_fails() {
        let json = r#"{
            "kind": "text",
            "content": {"text": "hi"}
        }"#;
        let r: Result<SendMessageRequest, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    // -------- ListMessagesQuery DTO --------

    #[test]
    fn list_messages_query_defaults() {
        let q: ListMessagesQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(q.after_sequence, 0);
        assert_eq!(q.limit, 50);
    }

    #[test]
    fn list_messages_query_explicit() {
        let json = r#"{"after_sequence": 100, "limit": 20}"#;
        let q: ListMessagesQuery = serde_json::from_str(json).unwrap();
        assert_eq!(q.after_sequence, 100);
        assert_eq!(q.limit, 20);
    }

    // -------- MessageResponse shape --------

    #[test]
    fn message_response_from_message() {
        let conv = Uuid::new_v4();
        let id = Uuid::new_v4();
        let m = sample_message(id, conv, 42);
        let resp = MessageResponse::from(m);
        assert_eq!(resp.id, id);
        assert_eq!(resp.conversation_id, conv);
        assert_eq!(resp.sequence, 42);
        assert_eq!(resp.kind, "text");
        assert_eq!(resp.state, "sent");
        assert!(resp.edited_at.is_none());
    }

    #[test]
    fn message_response_serialization_includes_all_fields() {
        let conv = Uuid::new_v4();
        let id = Uuid::new_v4();
        let m = sample_message(id, conv, 1);
        let resp = MessageResponse::from(m);
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"id\":"));
        assert!(s.contains("\"conversation_id\":"));
        assert!(s.contains("\"sequence\":1"));
        assert!(s.contains("\"sender_id\":"));
        assert!(s.contains("\"kind\":\"text\""));
        assert!(s.contains("\"content\":"));
        assert!(s.contains("\"state\":\"sent\""));
        assert!(s.contains("\"created_at\":"));
        // edited_at 缺省时不序列化 (skip_serializing_if)
        assert!(!s.contains("\"edited_at\""));
    }

    // -------- ListMessagesResponse shape --------

    #[test]
    fn list_messages_response_serialization() {
        let resp = ListMessagesResponse {
            messages: vec![],
            has_more: false,
            next_after_sequence: 0,
        };
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"messages\":[]"));
        assert!(s.contains("\"has_more\":false"));
        assert!(s.contains("\"next_after_sequence\":0"));
    }
}