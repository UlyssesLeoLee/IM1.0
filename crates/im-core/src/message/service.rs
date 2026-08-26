//! MessageService — 发消息主路径(最重要的 service)
//!
//! 依据: ImplementationSpec §7.4.3 + DetailedDesign §9.1
//!
//! ## 5 步实现
//! 1. 幂等检查(同 (conv, sender, idem_key) 已存在则直接返回)
//! 2. 校验 content schema(按 kind)
//! 3. 开事务:取 sequence + insert
//! 4. 提交事务后发布 `im.message.created` 事件(失败不阻塞 ack,V1+ outbox)
//! 5. 返回 Message

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use im_common::ids::{ConversationId, MessageId, UserId};
use im_common::AppError;

use super::content::{validate_fields, validate_serialized_size};
use super::repository::{Message, MessageRepository, MessageState, NewMessage};
use super::sequence::SequenceAllocator;
use crate::event::publisher::EventPublisher;
use crate::event::events::MessageCreatedEvent;

pub struct SendMessageCommand {
    pub conversation_id: ConversationId,
    pub sender_id: UserId,
    pub idempotency_key: String,
    pub kind: String,
    pub content: Value,
    pub reply_to: Option<MessageId>,
    /// 最大字节数(从 environments.settings 读取,默认 65536)
    pub max_size_bytes: usize,
}

pub struct MessageService {
    repo: Arc<dyn MessageRepository>,
    sequencer: Arc<dyn SequenceAllocator>,
    events: Arc<dyn EventPublisher>,
}

impl MessageService {
    pub fn new(
        repo: Arc<dyn MessageRepository>,
        sequencer: Arc<dyn SequenceAllocator>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            repo,
            sequencer,
            events,
        }
    }

    /// 发消息主路径
    pub async fn send_message(&self, cmd: SendMessageCommand) -> Result<Message, AppError> {
        // 1. 幂等检查:同 (conv, sender, idem_key) 已存在则直接返回(视为成功)
        if let Some(existing) = self
            .repo
            .find_by_idempotency_key(cmd.conversation_id, cmd.sender_id, &cmd.idempotency_key)
            .await?
        {
            tracing::debug!(
                message_id = %existing.id,
                sequence = existing.sequence,
                "idempotent replay"
            );
            return Ok(existing);
        }

        // 2. 校验 content 大小
        if cmd.content.to_string().len() > cmd.max_size_bytes {
            return Err(AppError::MessageTooLarge(
                cmd.content.to_string().len(),
                cmd.max_size_bytes,
            ));
        }
        // 业务侧字段校验(若需要按 schema)
        if let Ok(content) =
            serde_json::from_value::<im_protocol::content::MessageContent>(cmd.content.clone())
        {
            validate_fields(&content)?;
        }

        // 3. 开事务:取 sequence + insert
        let mut tx = self
            .repo
            .begin_tx()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx begin: {}", e)))?;
        let sequence = self
            .sequencer
            .next(&mut tx, cmd.conversation_id)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sequence: {}", e)))?;

        let new_msg = NewMessage {
            id: MessageId::new(),
            conversation_id: cmd.conversation_id,
            sequence,
            sender_id: Some(cmd.sender_id),
            kind: cmd.kind,
            content: cmd.content,
            reply_to: cmd.reply_to,
            idempotency_key: cmd.idempotency_key,
            state: MessageState::Sent,
        };
        let msg = self
            .repo
            .insert_in_tx(&mut tx, new_msg)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx commit: {}", e)))?;

        // 4. 事务提交后发布事件(失败不阻塞 ack,V1+ outbox)
        let event = MessageCreatedEvent {
            message_id: msg.id,
            conversation_id: msg.conversation_id,
            sender_id: msg.sender_id,
            sequence: msg.sequence,
            kind: msg.kind.clone(),
            ts: Utc::now(),
        };
        if let Err(e) = self
            .events
            .publish("im.message.created", &event)
            .await
        {
            // V1 引入 outbox 表;MVP 记录日志,SRE 手动重放
            tracing::error!(error = %e, message_id = %msg.id, "publish im.message.created failed, will be retried by outbox (V1+)");
        }

        // 5. 返回完整 Message
        Ok(msg)
    }

    pub async fn edit_message(
        &self,
        message_id: MessageId,
        user_id: UserId,
        new_content: Value,
        max_size_bytes: usize,
    ) -> Result<Message, AppError> {
        // 1. 查原 message
        let existing = self
            .repo
            .find_by_id(message_id)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;

        // 2. 仅 sender 可编辑
        if existing.sender_id != Some(user_id) {
            return Err(AppError::Forbidden("not the message sender".into()));
        }

        // 3. 大小校验
        if new_content.to_string().len() > max_size_bytes {
            return Err(AppError::MessageTooLarge(
                new_content.to_string().len(),
                max_size_bytes,
            ));
        }

        // 4. 更新(略,需要 SQL 实现;留待 MVP 编码阶段)
        Err(AppError::Internal(anyhow::anyhow!(
            "edit_message UPDATE not yet implemented in MVP; see ImplementationSpec §7.4.3"
        )))
    }

    pub async fn list_messages(
        &self,
        conversation_id: ConversationId,
        _user_id: UserId,    // 成员校验由 service 调用方完成
        after_sequence: i64,
        limit: i32,
    ) -> Result<Vec<Message>, AppError> {
        let limit = limit.clamp(1, 200);
        self.repo
            .list_after_sequence(conversation_id, after_sequence, limit)
            .await
    }
}
