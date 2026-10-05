//! WebSocket 客户端/服务端帧
//!
//! 依据: aux-13 §1.1 (客户端 **8** 类) / §1.2 (服务端 **10** 类)
//! 帧格式: JSON over WebSocket Text Frame
//!
//! ## 「11 类」是错的, 正确数字是 10 (2026-10-05 核对 aux-13 标题后修正)
//!
//! 本行此前写「服务端 11 类」。逐个数 aux-13 §1.2 的小节标题:
//! §1.2.1 `connected` / §1.2.2 `ack` / §1.2.3 `ack` / §1.2.4 `ack` /
//! §1.2.5 `message_new` / §1.2.6 `message_edited` / §1.2.7 `message_recalled` /
//! §1.2.8 `reaction_added` / §1.2.9 `presence_update` / §1.2.10 `typing` /
//! §1.2.11 `pong` / §1.2.12 `force_disconnect` —— **12 个小节**, 其中 `ack`
//! 占了 3 个(成功 / 幂等冲突 / 真错误), 去重后是 **10 个不同帧类型**,
//! 与下方 `ServerFrame` 的 10 个变体 **1:1 完全对应**。
//!
//! **别把「11」理解成「缺 1 类」**: aux-13 并**没有**定义已读回执下行帧。
//! 该需求来自另一份文档(aux-04 §B.4 转换表要求 mark_read 后 fanout), 属规范级
//! 遗漏, 不是本枚举少实现了一个变体。见 `docs/gap-ledger.md` §1.12。
//!
//! 客户端侧 8 类与 aux-13 §1.1.1–§1.1.8 逐条对齐, 无增减。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::content::MessageContent;

/// 客户端 → 服务端
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// 鉴权(WS 握手后第一帧)
    Auth { req_id: Uuid, access_token: String },
    /// 发消息
    SendMessage {
        req_id: Uuid,
        conversation_id: Uuid,
        idempotency_key: Uuid,
        kind: String, // text | image | file | sticker | system | custom
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
    RecallMessage { req_id: Uuid, message_id: Uuid },
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
    Typing { req_id: Uuid, conversation_id: Uuid },
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
    Connected { session_id: Uuid },
    /// 请求响应(成功)
    Ack {
        req_id: Uuid,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<AckData>,
        /// 失败时的错误载荷(per aux-13 §1.2.4)
        ///
        /// 2026-10-03 新增。此前本变体只有 `ok: bool` 却**挂不了错误内容** ——
        /// `ok=false` 表达不出 aux-13 §1.2.4 规定的形状, 导致 im-gateway 只好
        /// 另造一个顶层 `{"type":"error",...}` 帧(`WsErrorFrame`), 与规范不一致。
        /// 证据见 docs/gap-ledger.md §1.7.3。
        ///
        /// 语义: `ok=true` 时为 `None`; `ok=false` 时应为 `Some`。
        /// 本字段不强制该不变量(serde 无法表达), 由构造方保证。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<crate::error_body::ErrorBody>,
    },
    /// 新消息广播 (per aux-13 §1.2.5)
    ///
    /// 2026-10-03 新增。此前本枚举**没有**这个变体, 而 `ws_frames` 模块文档
    /// 声称「aux-13 §1.2 服务端 11 类」—— 缺的就是它。后果: 即便
    /// `send_message` 实装成功, 其它客户端**收不到任何广播**, WS 端点等于
    /// 只能对发起方自己说话。`im-testkit/src/mock_ws_frames.rs:238` 当时已
    /// 记录了这个缺口(「ServerFrame::MessageNew 在 im-protocol 当前未实装」),
    /// 靠 JSON Value 绕道。
    MessageNew { message: WireMessage },
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
        status: String, // online | offline | away | busy | invisible
    },
    /// 输入中广播
    Typing {
        conversation_id: Uuid,
        user_id: Uuid,
    },
    /// 心跳响应
    Pong { ts: i64 },
    /// 强制下线
    ForceDisconnect {
        reason: String, // token_revoked | account_banned | account_deleted | admin_kick
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
            // aux-03 §B: IDEMPOTENCY_CONFLICT 在 WS 路径以**成功**形式返回
            // (ok=true + idempotent_replay=true), 故 error 恒为 None。
            error: None,
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
            content: MessageContent::Text { text: "hi".into() },
            reply_to: None,
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"type\":\"send_message\""));
        assert!(s.contains("\"text\":\"hi\""));
    }

    // -------- ack 失败形状 (2026-10-03, 见 gap-ledger §1.7.3) --------

    #[test]
    fn server_frame_ack_failure_carries_error_payload() {
        // aux-13 §1.2.4 规定的失败形状:
        //   {"type":"ack","req_id":...,"ok":false,"error":{code,message,trace_id}}
        //
        // 此前 `ServerFrame::Ack` 只有 `ok: bool`, 表达不了这个形状 —— 这正是
        // im-gateway 另造顶层 `{"type":"error",...}` 帧的原因。
        let req_id = Uuid::new_v4();
        let frame = ServerFrame::Ack {
            req_id,
            ok: false,
            data: None,
            error: Some(crate::error_body::ErrorBody::new(
                "RATE_LIMITED",
                "auth.rate_limit.send_message",
                "tr_01HXY",
            )),
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"type\":\"ack\""), "失败响应仍是 ack 帧: {s}");
        assert!(s.contains("\"ok\":false"));
        assert!(s.contains(&format!("\"req_id\":\"{req_id}\"")));
        assert!(s.contains("\"code\":\"RATE_LIMITED\""));
        assert!(s.contains("\"message\":\"auth.rate_limit.send_message\""));
        assert!(s.contains("\"trace_id\":\"tr_01HXY\""));
    }

    #[test]
    fn server_frame_ack_success_omits_error_field() {
        // 成功帧不该带 error 字段(与 data / error 互斥)
        let s = serde_json::to_string(&ServerFrame::Ack {
            req_id: Uuid::new_v4(),
            ok: true,
            data: Some(AckData {
                message_id: Some(Uuid::new_v4()),
                sequence: Some(1),
                idempotent_replay: false,
            }),
            error: None,
        })
        .unwrap();
        assert!(s.contains("\"ok\":true"));
        assert!(!s.contains("\"error\""), "成功帧不应含 error: {s}");
    }

    #[test]
    fn legacy_ack_without_error_field_still_deserializes() {
        // `#[serde(default)]` 保证本字段加入前签发的成功 ack 仍能解析
        let json = r#"{"type":"ack","req_id":"22222222-2222-4222-8222-222222222222","ok":true,"data":{"message_id":"22222222-2222-4222-8222-222222222222","sequence":42}}"#;
        let f: ServerFrame = serde_json::from_str(json).expect("旧 ack 帧应仍可解析");
        match f {
            ServerFrame::Ack { ok, error, .. } => {
                assert!(ok);
                assert!(error.is_none());
            }
            _ => panic!("应解析为 Ack"),
        }
    }
}
