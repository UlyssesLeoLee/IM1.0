//! Message Repository 接口
//!
//! 依据: ImplementationSpec §7.4.3 + DetailedDesign §9.1

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Postgres;
use uuid::Uuid;

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

    async fn find_by_id(
        &self,
        message_id: MessageId,
    ) -> Result<Option<Message>, AppError>;

    async fn update_state(
        &self,
        message_id: MessageId,
        new_state: MessageState,
    ) -> Result<(), AppError>;
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
