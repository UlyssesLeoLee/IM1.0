//! 领域事件 payload

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

/// `im.message.recalled` payload
///
/// 2026-10-03 新增。aux-04 §B.4 不变量明写「转换必须 publish 事件
/// `im.message.{recalled,deleted}` 供其他 pod 同步」—— 本事件此前**不存在**,
/// 即撤回连事件都发不出。
///
/// 带 `conversation_id` 是为了订阅方能直接定位会话做失效/重取, 不必反查。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecalledEvent {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    /// 执行撤回的人 —— 恒为原 sender(per aux-04 §B.4 `actor = sender`),
    /// 但仍显式携带: 后续若放开 admin 代撤回, 订阅方无需改代码即可区分。
    pub actor_id: UserId,
    pub ts: DateTime<Utc>,
}
