//! WS 广播中枢 (2026-10-03 新增)
//!
//! ## 此前根本不存在
//!
//! `ws/handler.rs` 的模块文档写「**占位** broadcast channel, 实际 broadcasting
//! 留 G-1 presence」—— 但代码里**没有任何 broadcast channel**。文档描述的
//! 占位物并不存在, 这是继 §1.8.2 `auth_ok` 之后又一例「文档说有、代码没有」。
//!
//! ## 为什么必须有
//!
//! `send_message` 实装后, 发送方能收到 `ack`, 但**其它客户端收不到任何东西**。
//! 一个只能对发起者自己说话的 WS 端点, 对 IM 协议是没有意义的。
//!
//! ## 设计: 单通道 + 连接侧过滤
//!
//! 用**一个** `tokio::sync::broadcast` 通道(而非 per-conversation 通道), 每个
//! 连接在鉴权成功后把自己所属的 `conversation_id` 集合记进 `WsSession`,
//! 收到广播时按集合过滤。
//!
//! **为什么不按会话分通道**: per-conversation 通道需要动态增减订阅, 而
//! `select!` 无法对「数量不定的一组 receiver」做分支(除非用 `FuturesUnordered`,
//! 复杂度显著上升)。单通道 + 内存过滤是同等正确且简单得多的做法。
//!
//! **过滤为什么是安全问题而不只是优化**: 若不过滤, 用户 A 会收到用户 B 所在
//! 会话的消息 —— 那是**跨会话数据泄漏**。所以这个过滤不是「少发一点」, 而是
//! 不做就是漏洞。
//!
//! ## 投递范围为什么是三态而不是「有/无 conversation_id」
//!
//! 最初 `frame_conversation` 返回 `Option<ConversationId>`, `None` 表示
//! 「与具体会话无关, 所有人都该收到」。这个二态签名是个**陷阱**: 它把两类
//! 语义完全不同的帧混成了同一个值 ——
//! - `PresenceUpdate` / `ForceDisconnect`: 确实该发给所有已鉴权连接;
//! - `MessageEdited` / `ReactionAdded`: 语义上属于某个会话, 但 **wire 形状
//!   不带 `conversation_id`**(aux-13 §1.2.6 / §1.2.8), 无从判断接收方是否成员。
//!
//! 二态下后者会被当成「发给所有人」→ 跨会话泄漏, 而且**不会有任何报错**。
//! 故改为三态 `Audience`, 第三态 `Undeliverable` 明确表示「不知道该发给谁,
//! 一律不发」。详见该枚举的定义。
//!
//! ## 已知限制
//!
//! - 成员关系在**鉴权时**快照一次。会话期间被移出会话的话, 仍会收到该会话的
//!   广播, 直到该连接重连。修正需要定期刷新或基于事件的失效通知(留 G-1 presence
//!   的成员变更事件)。
//! - `broadcast` 通道在**没有订阅者**时 `send` 返回 `Err`, 此时消息直接丢弃。
//!   这是正确的: 没人在线时本就不需要投递; 但**不重试**, 所以离线消息靠
//!   `GET /v1/conversations/{id}/messages` 拉取(已实装)。
//! - `MessageEdited` / `ReactionAdded` 暂**无法广播**(见上)。要让「编辑消息」
//!   实时同步到其它端, 需 aux-13 的 wire 形状带上 `conversation_id` —— 那是
//!   协议变更, 不由本文件拍板。

use std::collections::HashSet;

use tokio::sync::broadcast;

use im_common::ids::ConversationId;
use im_protocol::content::MessageContent;
use im_protocol::ws_frames::{ServerFrame, WireMessage};

use super::session::{SessionState, WsSession};

/// 广播通道容量
///
/// 满了就覆盖最旧的(`RecvError::Lagged`), 不阻塞发送方 —— 实时消息宁可丢旧的,
/// 也不该让慢客户端把发送方卡住。客户端可用
/// `GET /v1/conversations/{id}/messages` 补齐缺口。
const CHANNEL_CAPACITY: usize = 1024;

/// WS 广播中枢: 持有一个 `broadcast::Sender<ServerFrame>`
#[derive(Clone)]
pub struct WsHub {
    tx: broadcast::Sender<ServerFrame>,
}

impl WsHub {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(CHANNEL_CAPACITY);
        Self { tx }
    }

    /// 订阅。**必须在连接建立时调用一次**, 否则该连接收不到任何广播。
    pub fn subscribe(&self) -> broadcast::Receiver<ServerFrame> {
        self.tx.subscribe()
    }

    /// 向所有订阅者广播一帧, 返回**本次到达的订阅者数量**
    ///
    /// 返回 `0` 表示当前无人在线 —— 那是**常态而非错误**(对方上线后靠
    /// `GET /v1/conversations/{id}/messages` 补), 但必须让调用方**看得见**,
    /// 所以返回计数而不是 `()` 或 `let _ =`。
    ///
    /// **为什么不返回 `Result`**: `broadcast::error::SendError<ServerFrame>` 会把
    /// 整个帧塞进 `Err` 变体(`ServerFrame` 最大的变体有 ~250 字节), 而调用方
    /// 唯一的动作是记一行日志、从不重试 —— 为一个不重试的错误付出每帧一次
    /// 所有权搬运 + 一个巨型错误类型不划算。帧本来就归调用方所有, 送出去
    /// 就该送出去。
    pub fn publish(&self, frame: ServerFrame) -> usize {
        self.tx.send(frame).unwrap_or(0)
    }

    /// 当前订阅者数量(诊断用)
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for WsHub {
    fn default() -> Self {
        Self::new()
    }
}

/// 一帧的投递范围 —— 广播过滤的**唯一**判定依据
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Audience {
    /// 属于某个会话: **只有该会话的已鉴权成员**应收到
    Conversation(ConversationId),

    /// 与会话无关: 所有**已鉴权**连接都应收到
    ///
    /// 仅限确实全局的帧。`PresenceUpdate` 归此类 —— 「谁在线」本质是全局事实;
    /// 若将来要做「只通知我关心的人」, 应改用 `Conversation` 收窄, 而不是靠
    /// 调用方自觉。
    All,

    /// **不可安全投递 —— 一律不发。**
    ///
    /// 用于两类帧:
    /// 1. 语义属于某会话、但 wire 形状不带 `conversation_id` 的帧
    ///    (`MessageEdited` / `ReactionAdded`) —— 无从判断接收方是否成员,
    ///    按「发给所有人」处理就是跨会话泄漏;
    /// 2. **单连接响应**(`Ack` / `Connected` / `Pong`) —— 它们承载的是某个
    ///    请求的 `req_id` 或某条连接自己的握手结果, 广播出去等于把别人的
    ///    请求回执塞给无关客户端。
    ///
    /// 携带 `&'static str` 是为了在丢弃时留下可 grep 的原因, 而不是静默吞掉。
    Undeliverable(&'static str),
}

impl Audience {
    /// 判定一帧的投递范围
    pub fn of(frame: &ServerFrame) -> Self {
        match frame {
            ServerFrame::MessageNew { message } => {
                Audience::Conversation(ConversationId(message.conversation_id))
            }
            ServerFrame::MessageRecalled {
                conversation_id, ..
            } => Audience::Conversation(ConversationId(*conversation_id)),
            ServerFrame::Typing {
                conversation_id, ..
            } => Audience::Conversation(ConversationId(*conversation_id)),

            ServerFrame::PresenceUpdate { .. } => Audience::All,
            ServerFrame::ForceDisconnect { .. } => Audience::All,

            ServerFrame::MessageEdited { .. } => {
                Audience::Undeliverable("aux-13 §1.2.6 MessageEdited 不带 conversation_id")
            }
            ServerFrame::ReactionAdded { .. } => {
                Audience::Undeliverable("aux-13 §1.2.8 ReactionAdded 不带 conversation_id")
            }
            ServerFrame::Ack { .. } => Audience::Undeliverable("ack 是单连接响应, 不得广播"),
            ServerFrame::Connected { .. } => Audience::Undeliverable("connected 是单连接响应"),
            ServerFrame::Pong { .. } => Audience::Undeliverable("pong 是单连接响应"),
        }
    }
}

/// 该帧是否应投递给这个连接 —— 广播过滤的**全部**逻辑所在
///
/// 抽成纯函数而不是内联在 `select!` 分支里, 是为了让这条安全规则能被
/// 单元测试直接覆盖(见本文件 tests)。
///
/// **注意 `All` 分支自己判了鉴权状态**: `is_member_of` 内含鉴权检查, 但
/// `All` 走不到那里 —— 若这里只写 `true`, 未鉴权的连接就能收到全局帧,
/// 那正是「连上就能听」的旁听入口。
pub fn should_deliver(audience: &Audience, state: &WsSession) -> bool {
    match audience {
        Audience::All => state.state() == SessionState::Authenticated,
        Audience::Conversation(conv) => state.is_member_of(*conv),
        Audience::Undeliverable(_) => false,
    }
}

/// 把 `Message` 实体转成下行 wire 形状 `WireMessage` (per aux-13 §1.2.5)
///
/// ## 为什么返回 `Result` 而不是给个默认值
///
/// `Message.content` 是 `serde_json::Value`(库里原样存), 而 `WireMessage.content`
/// 是强类型 `MessageContent`。反序列化失败意味着**库里这条数据的 content
/// 不符合 schema** —— 此时编造一个 `Text { text: "" }` 发出去, 客户端会看到
/// 一条空消息, 而真实内容被静默吞掉。故如实报错, 由调用方跳过本次广播。
///
/// 正常路径不会失败: 写入时 `MessageService::send_message` 已用同一个
/// `from_value::<MessageContent>` 校验过(见其 step 2c), 原样往返成立。
pub fn to_wire_message(m: &im_core::message::repository::Message) -> Result<WireMessage, String> {
    let content: MessageContent =
        serde_json::from_value(m.content.clone()).map_err(|e| format!("content schema: {e}"))?;
    Ok(WireMessage {
        id: m.id.0,
        conversation_id: m.conversation_id.0,
        sequence: m.sequence,
        sender_id: m.sender_id.map(|u| u.0),
        kind: m.kind.clone(),
        content,
        reply_to: m.reply_to.map(|r| r.0),
        state: m.state.as_str().to_string(),
        created_at: m.created_at,
        edited_at: m.edited_at,
        reactions: Vec::new(),
    })
}

/// 由会话列表构建成员集合(鉴权后快照一次)
pub fn membership_set<'a, I: IntoIterator<Item = &'a ConversationId>>(
    ids: I,
) -> HashSet<ConversationId> {
    ids.into_iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use im_common::ids::{DeviceSessionId, EnvironmentId, MessageId, UserId};
    use im_core::message::repository::{Message, MessageState};
    use uuid::Uuid;

    // ========================================================================
    // 投递范围判定 —— 这是安全边界, 不是优化
    // ========================================================================

    fn sample_message(conv: Uuid) -> Message {
        Message {
            id: MessageId(Uuid::new_v4()),
            conversation_id: ConversationId(conv),
            sequence: 7,
            sender_id: Some(UserId::new()),
            kind: "text".into(),
            content: serde_json::json!({ "kind": "text", "text": "hi" }),
            reply_to: None,
            state: MessageState::Sent,
            created_at: chrono::Utc::now(),
            edited_at: None,
        }
    }

    /// 构造一个已鉴权、属于 `convs` 的连接
    fn authed_session(convs: &[ConversationId]) -> WsSession {
        let mut s = WsSession::new(Uuid::new_v4(), super::super::session::new_heartbeat());
        s.mark_authenticated(UserId::new(), EnvironmentId::new(), DeviceSessionId::new());
        s.set_conversation_ids(membership_set(convs.iter()));
        s
    }

    fn created_session() -> WsSession {
        WsSession::new(Uuid::new_v4(), super::super::session::new_heartbeat())
    }

    #[test]
    fn message_new_targets_its_conversation() {
        let conv = ConversationId(Uuid::new_v4());
        let frame = ServerFrame::MessageNew {
            message: to_wire_message(&sample_message(conv.0)).unwrap(),
        };
        assert_eq!(Audience::of(&frame), Audience::Conversation(conv));
    }

    #[test]
    fn typing_and_recall_target_their_conversation() {
        let conv = ConversationId(Uuid::new_v4());
        let other = ConversationId(Uuid::new_v4());

        let typing = ServerFrame::Typing {
            conversation_id: conv.0,
            user_id: UserId::new().0,
        };
        assert_eq!(Audience::of(&typing), Audience::Conversation(conv));

        let recall = ServerFrame::MessageRecalled {
            message_id: Uuid::new_v4(),
            conversation_id: conv.0,
        };
        assert_eq!(Audience::of(&recall), Audience::Conversation(conv));

        // 成员连接收
        assert!(should_deliver(
            &Audience::of(&typing),
            &authed_session(&[conv, other])
        ));
        // 非成员连接**不**收 —— 这就是跨会话泄漏的闸门
        assert!(!should_deliver(
            &Audience::of(&typing),
            &authed_session(&[other])
        ));
    }

    #[test]
    fn frames_without_conversation_id_are_never_delivered() {
        // 回归: 二态 `Option` 版本把这类帧当 `None` → 发给所有人 → 跨会话泄漏。
        // 这三个测试是那次设计的守门。
        let edited = ServerFrame::MessageEdited {
            message_id: Uuid::new_v4(),
            content: MessageContent::Text { text: "x".into() },
            edited_at: chrono::Utc::now(),
        };
        let react = ServerFrame::ReactionAdded {
            message_id: Uuid::new_v4(),
            user_id: UserId::new().0,
            emoji: "👍".into(),
        };
        let ack = ServerFrame::Ack {
            req_id: Uuid::new_v4(),
            ok: true,
            data: None,
            error: None,
        };

        for frame in [edited, react, ack] {
            let a = Audience::of(&frame);
            assert!(
                matches!(a, Audience::Undeliverable(_)),
                "应判为不可投递, 实际 {a:?}"
            );
            // 即便连接是「全员」(拥有所有会话), 也**不得**收到
            assert!(
                !should_deliver(&a, &authed_session(&[ConversationId(Uuid::new_v4())])),
                "{a:?} 绝不能被投递"
            );
        }
    }

    #[test]
    fn global_frames_reach_every_authed_connection() {
        let a = Audience::of(&ServerFrame::ForceDisconnect {
            reason: "token_revoked".into(),
        });
        assert_eq!(a, Audience::All);
        assert!(should_deliver(&a, &authed_session(&[])));
    }

    #[test]
    fn unauthenticated_connection_receives_nothing() {
        // 旁听防护: 未鉴权连接即便「看起来该收」也不能收。
        let s = created_session();
        assert!(!should_deliver(&Audience::All, &s));
        assert!(!should_deliver(
            &Audience::Conversation(ConversationId(Uuid::new_v4())),
            &s
        ));
    }

    // ========================================================================
    // Message → WireMessage 转换
    // ========================================================================

    #[test]
    fn to_wire_message_roundtrips_content_and_state() {
        let conv = ConversationId(Uuid::new_v4());
        let m = sample_message(conv.0);
        let w = to_wire_message(&m).unwrap();
        assert_eq!(w.id, m.id.0);
        assert_eq!(w.conversation_id, conv.0);
        assert_eq!(w.sequence, 7);
        assert_eq!(w.kind, "text");
        assert_eq!(w.state, "sent");
        match &w.content {
            MessageContent::Text { text } => assert_eq!(text, "hi"),
            other => panic!("content 类型不符: {other:?}"),
        }
        // 上行 wire 形状: 帧 tag 来自 `ServerFrame` 枚举, `content` 的 tag 是
        // `"kind"`(来自 `MessageContent` 自身) —— 两层 tag 都要在, 且不能混。
        let s = serde_json::to_string(&ServerFrame::MessageNew { message: w }).unwrap();
        assert!(s.contains("\"type\":\"message_new\""), "帧 tag: {s}");
        assert!(
            s.contains("\"content\":{\"kind\":\"text\""),
            "content 必须自带 kind tag: {s}"
        );
    }

    #[test]
    fn to_wire_message_reports_corrupt_content_instead_of_fabricating() {
        // 库里 content 不合 schema 时必须报错, 不能编一条空 text 蒙混过关
        let mut m = sample_message(ConversationId(Uuid::new_v4()).0);
        m.content = serde_json::json!({ "kind": "text" }); // 缺 text 字段
        assert!(
            to_wire_message(&m).is_err(),
            "坏 content 应返回 Err 而非伪造"
        );
    }

    // ========================================================================
    // 真实 broadcast 通道行为
    // ========================================================================

    #[tokio::test]
    async fn published_frame_reaches_subscriber() {
        let hub = WsHub::new();
        let mut rx = hub.subscribe();
        assert_eq!(hub.subscriber_count(), 1);

        let conv = ConversationId(Uuid::new_v4());
        let frame = ServerFrame::MessageNew {
            message: to_wire_message(&sample_message(conv.0)).unwrap(),
        };
        assert_eq!(hub.publish(frame), 1, "1 个订阅者");

        let got = rx.recv().await.unwrap();
        assert_eq!(Audience::of(&got), Audience::Conversation(conv));
    }

    #[tokio::test]
    async fn publish_without_subscriber_reports_zero_not_panic() {
        // 无人在线是常态, 不是错误 —— 但必须**如实回报 0**, 不能吞掉。
        // `broadcast::send` 恰好只在 `receiver_count() == 0` 时返回 Err,
        // 故 unwrap_or(0) 的 0 精确对应「没人在线」, 不会与其它情况混淆。
        let hub = WsHub::new();
        let conv = ConversationId(Uuid::new_v4());
        let frame = ServerFrame::MessageNew {
            message: to_wire_message(&sample_message(conv.0)).unwrap(),
        };
        assert_eq!(hub.publish(frame), 0);
        assert_eq!(hub.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn slow_subscriber_gets_lagged_not_a_wrong_frame() {
        // 慢客户端: 覆盖最旧的。关键是它**收到 Lagged 而不是错帧** ——
        // 静默丢帧会让客户端以为「对方没发言」。
        let hub = WsHub::new();
        let mut rx = hub.subscribe();
        for _ in 0..(CHANNEL_CAPACITY + 10) {
            let conv = ConversationId(Uuid::new_v4());
            hub.publish(ServerFrame::MessageNew {
                message: to_wire_message(&sample_message(conv.0)).unwrap(),
            });
        }
        // 未 recv 过, 必然 Lagged
        match rx.recv().await {
            Err(broadcast::error::RecvError::Lagged(n)) => assert!(n > 0),
            other => panic!("应为 Lagged, 实际 {other:?}"),
        }
    }
}
