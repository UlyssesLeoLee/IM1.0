//! Message Repository 接口
//!
//! 依据: ImplementationSpec §7.4.3 + DetailedDesign §9.1

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use im_common::ids::{ConversationId, MessageId, UserId};
use im_common::AppError;

/// 消息投递状态
///
/// 与 `users.state` 区分:本字段表示"消息本身在生命周期中的阶段"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageState {
    Sent,
    Delivered,
    Read,
    Recalled,
    Deleted,
}

impl MessageState {
    /// wire/DB 字符串形式(与 `messages.state` 的 CHECK 约束一致)
    ///
    /// 2026-10-03: 原先该转换是 pg.rs 里的私有函数, service 层拿不到, 导致
    /// 错误信息里只能拼字面量。转换逻辑属于枚举本身, 故放到这里。
    pub fn as_str(self) -> &'static str {
        match self {
            MessageState::Sent => "sent",
            MessageState::Delivered => "delivered",
            MessageState::Read => "read",
            MessageState::Recalled => "recalled",
            MessageState::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub sequence: i64,
    pub sender_id: Option<UserId>,
    pub kind: String,
    pub content: Value,
    pub reply_to: Option<MessageId>,
    pub state: MessageState,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// 开事务
    async fn begin_tx(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, AppError>;

    /// 在事务内 insert
    async fn insert_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        msg: NewMessage,
    ) -> Result<Message, AppError>;

    async fn find_by_idempotency_key(
        &self,
        conversation_id: ConversationId,
        sender_id: UserId,
        key: &str,
    ) -> Result<Option<Message>, AppError>;

    async fn list_after_sequence(
        &self,
        conversation_id: ConversationId,
        after: i64,
        limit: i32,
    ) -> Result<Vec<Message>, AppError>;

    async fn find_by_id(&self, message_id: MessageId) -> Result<Option<Message>, AppError>;

    async fn update_state(
        &self,
        message_id: MessageId,
        new_state: MessageState,
    ) -> Result<(), AppError>;

    /// 更新消息内容并打上 `edited_at` 时间戳, 返回更新后的 message
    ///
    /// 2026-10-03 新增: `edit_message` 此前做完 4 步校验后直接返回
    /// `AppError::Internal("not yet implemented")` —— 因为仓储层根本没有
    /// 改 content 的方法(只有 `update_state`)。schema 侧一直是齐的
    /// (`messages.content JSONB` + `messages.edited_at TIMESTAMPTZ`),
    /// 缺的只是这一条 UPDATE。
    ///
    /// 返回 `Ok(None)` 表示目标消息不存在(调用方转 `MessageNotFound`);
    /// 返回 `Ok(Some(m))` 携带**更新后**的行(edited_at 已填)。
    async fn update_content(
        &self,
        message_id: MessageId,
        new_content: &Value,
    ) -> Result<Option<Message>, AppError>;
}

#[derive(Debug, Clone)]
pub struct NewMessage {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub sequence: i64,
    pub sender_id: Option<UserId>,
    pub kind: String,
    pub content: Value,
    pub reply_to: Option<MessageId>,
    pub idempotency_key: String,
    pub state: MessageState,
}
