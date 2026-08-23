//! WebSocket 客户端/服务端帧
//!
//! 依据: aux-13 §1.1 (客户端 8 类) / §1.2 (服务端 11 类)
//! 帧格式: JSON over WebSocket Text Frame

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::content::MessageContent;

/// 客户端 → 服务端
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// 鉴权(WS 握手后第一帧)
    Auth {
        req_id: Uuid,
        access_token: String,
    },
    /// 发消息
    SendMessage {
        req_id: Uuid,
        conversation_id: Uuid,
        idempotency_key: Uuid,
        kind: String,                  // text | image | file | sticker | system | custom
        content: MessageContent,
        #[serde(default)]
        reply_to: Option<Uuid>,
    },
    /// 编辑消息
    EditMessage {
        req_id: Uuid,
        message_id: Uuid,
        content: MessageContent,
    },
    /// 撤回消息
    RecallMessage {
        req_id: Uuid,
        message_id: Uuid,
    },
    /// 添加 reaction
    React {
        req_id: Uuid,
        message_id: Uuid,
        emoji: String,
    },
    /// 上报已读
    MarkRead {
        req_id: Uuid,
        conversation_id: Uuid,
        sequence: i64,
    },
    /// 输入中指示
    Typing {
        req_id: Uuid,
        conversation_id: Uuid,
    },
    /// 心跳
    Ping {
        #[serde(default)]
        ts: Option<i64>,
    },
}

/// 服务端 → 客户端
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    /// WS 握手成功
    Connected {
        session_id: Uuid,
    },
    /// 请求响应(成功)
    Ack {
        req_id: Uuid,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<AckData>,
    },
    /// 消息被编辑
    MessageEdited {
        message_id: Uuid,
        content: MessageContent,
        edited_at: chrono::DateTime<chrono::Utc>,
    },
    /// 消息被撤回
    MessageRecalled {
        message_id: Uuid,
        conversation_id: Uuid,
    },
    /// 新 reaction
    ReactionAdded {
        message_id: Uuid,
        user_id: Uuid,
        emoji: String,
    },
    /// 在线状态变化
    PresenceUpdate {
        user_id: Uuid,
        status: String,                  // online | offline | away | busy | invisible
    },
    /// 输入中广播
    Typing {
        conversation_id: Uuid,
        user_id: Uuid,
    },
    /// 心跳响应
    Pong {
        ts: i64,
    },
    /// 强制下线
    ForceDisconnect {
        reason: String,                  // token_revoked | account_banned | account_deleted | admin_kick
    },
}

/// 成功 ack 的 data 字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    /// 幂等重放(同 idem key 已存在,返回原 message)
    #[serde(default, skip_serializing_if = "is_false")]
    pub idempotent_replay: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// 下行 Message JSON Schema(对应 aux-13 §4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sequence: i64,
    pub sender_id: Option<Uuid>,
    pub kind: String,
    pub content: MessageContent,
    #[serde(default)]
    pub reply_to: Option<Uuid>,
    pub state: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub emoji: String,
    pub user_ids: Vec<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::MessageContent;

    #[test]
    fn client_frame_auth_roundtrip() {
        let frame = ClientFrame::Auth {
            req_id: Uuid::new_v4(),
            access_token: "eyJ...".into(),
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"type\":\"auth\""));
        let back: ClientFrame = serde_json::from_str(&s).unwrap();
        match back {
            ClientFrame::Auth { access_token, .. } => assert_eq!(access_token, "eyJ..."),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn server_frame_ack_idempotent_replay() {
        let frame = ServerFrame::Ack {
            req_id: Uuid::new_v4(),
            ok: true,
            data: Some(AckData {
                message_id: Some(Uuid::new_v4()),
                sequence: Some(42),
                idempotent_replay: true,
            }),
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"idempotent_replay\":true"));
    }

    #[test]
    fn client_frame_send_message_roundtrip() {
        let frame = ClientFrame::SendMessage {
            req_id: Uuid::new_v4(),
            conversation_id: Uuid::new_v4(),
            idempotency_key: Uuid::new_v4(),
            kind: "text".into(),
            content: MessageContent::Text {
                text: "hi".into(),
            },
            reply_to: None,
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"type\":\"send_message\""));
        assert!(s.contains("\"text\":\"hi\""));
    }
}
