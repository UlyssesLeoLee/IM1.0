//! 领域事件 payload

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
