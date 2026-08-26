//! 领域事件 payload

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{ConversationId, MessageId, UserId};

/// `im.message.created` payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCreatedEvent {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    pub sender_id: Option<UserId>,
    pub sequence: i64,
    pub kind: String,
    pub ts: DateTime<Utc>,
}
