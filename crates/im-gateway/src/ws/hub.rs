//! WS 广播中枢 —— 按会话索引 + 载荷一次序列化共享
//!
//! 依据: aux-13 §1 + `DetailedDesign §5`
//!
//! ## 上一版是 fan-out-then-filter, 以及它为什么在 1 万连接下不可用
//!
//! 上一版用**一个** `tokio::sync::broadcast` 通道: `publish` 把帧投给**全部**
//! 订阅者, 每个连接收到后再按 `should_deliver` 过滤。这在功能上正确, 但:
//!
//! - **O(N) 投递**: `broadcast::send` 要往**每个**接收者的环形缓冲各写一格。
//!   1 万条连接发 1 条**两人私聊**, 代价是 1 万次缓冲写入 + 1 万次帧克隆,
//!   而真正该收到的是 2 个人。
//! - **`Undeliverable` 帧照样推给全部 N 个连接**, 收信侧才丢弃。
//!
//! 故本版把过滤从「收信侧」搬到「**发信侧**」。
//!
//! ## 本版设计
//!
//! `WsHub` 持有一个投递索引:
//!
//! ```text
//!   conversation_id ──▶ { conn_id, conn_id, ... }   (该会话的已鉴权成员连接)
//!   (全局)          ──▶ { conn_id, ... }            (全部已鉴权连接)
//!   conn_id         ──▶ mpsc::Sender<Outbound>      (该连接自己的出站通道)
//! ```
//!
//! `publish` 的成本因此是 **O(实际收件人)**, 而不是 O(全部连接):
//!
//! 1. `Audience::of(&frame)` 定投递范围 —— `Undeliverable` **立即返回 0**,
//!    一次索引都不碰(这正是上一版白推给 N 个连接的那批帧)。
//! 2. 一次哈希查表拿到收件人的 `mpsc::Sender`(在锁内克隆后立刻放锁)。
//! 3. `serde_json::to_string` **只做一次**, 结果放进 `Arc<str>`。
//! 4. 给每个收件人 `try_send(Arc::clone(&payload))` —— 只克隆一个胖指针。
//!
//! ## 为什么接收侧仍然保留 `should_deliver`(纵深防御)
//!
//! 索引是**主**过滤, 但它不是**唯一**过滤。接收侧再判一次 `should_deliver`
//! 意味着「索引说该收」与「会话说该收」必须**同时**成立, 任何一侧的 bug 都
//! 不直接变成数据泄漏。代价是每个**实际收件人**一次 `HashSet::contains`,
//! 不是 O(N)。
//!
//! ## 仍然存在的限制(诚实声明)
//!
//! - 成员关系在**鉴权时**快照一次, 索引与 `WsSession` 用的是同一份快照。
//!   会话期间被移出会话, 仍会收到该会话的广播, 直到该连接重连。修正需要
//!   基于事件的失效通知(留 G-1 presence 的成员变更事件)。**接收侧二次校验
//!   救不了这一条** —— 两侧读的是同一份快照, 不是互相独立的证据。
//! - 出站通道**满**时(慢客户端)按「丢本帧」处理, 与上一版 `Lagged` 的取舍
//!   一致: 实时消息宁可丢, 也不该让慢客户端把发送方卡住。缺口由
//!   `GET /v1/conversations/{id}/messages` 补齐(已实装)。
//! - 锁用 `std::sync::RwLock` 而非 `tokio::sync::RwLock`: `publish` 是**同步**
//!   函数, 且临界区里**没有**任何 `.await`(`try_send` 不阻塞)。见
//!   [`WsHub::publish`] 的说明。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;

use im_common::ids::ConversationId;
use im_protocol::content::MessageContent;
use im_protocol::ws_frames::{ServerFrame, WireMessage};

use super::session::{SessionState, WsSession};

/// 单连接出站通道容量
///
/// 上一版是**一个**容量 1024 的共享 `broadcast`, 1024 格由所有连接分摊;
/// 本版是**每连接**一条通道, 故容量必须按单连接算: 1 万连接 × 1024 格 ×
/// `Outbound` 约 40 字节 ≈ 400MB —— 不可接受。
///
/// 取 128: 1 万连接满载约 50MB(且 `tokio::mpsc` 的缓冲是**按块惰性分配**的,
/// 空连接不占内存), 而 128 帧的积压已远超客户端能及时消费的上限。
/// 超出即丢弃并计入 `dropped_broadcast_count()` —— 见「可观测性」。
const PER_CONN_CAPACITY: usize = 128;

/// 连接标识 —— 索引的主键
pub type ConnId = uuid::Uuid;

/// 一条待投递的出站消息
///
/// ## 为什么是「已序列化的字符串」而不是 `ServerFrame`
///
/// 上一版每个收件人各自 `serde_json::to_string`。一个 100 人的群发一条消息
/// 就要序列化 100 次**完全相同**的内容。改成在 `publish` 里序列化一次、
/// 用 `Arc<str>` 共享之后, N 个收件人只多付出一次胖指针克隆。
#[derive(Debug, Clone)]
pub struct Outbound {
    /// 投递范围 —— 供接收侧二次校验(纵深防御, 见模块文档)
    pub audience: Audience,
    /// 已序列化的 JSON 文本; 同一帧的所有收件人共享同一份
    pub payload: Arc<str>,
}

/// 投递索引 —— `publish` 的成本完全由它决定
#[derive(Default)]
struct Index {
    /// 每个连接的出站通道。**建连即建**, 与是否鉴权无关。
    sinks: HashMap<ConnId, mpsc::Sender<Outbound>>,
    /// 会话 id -> 该会话的已鉴权成员连接
    by_conversation: HashMap<ConversationId, HashSet<ConnId>>,
    /// 全部已鉴权连接(`Audience::All` 的目标)
    all_authed: HashSet<ConnId>,
}

/// WS 广播中枢
///
/// 克隆廉价(内部一个 `Arc`-like 的 `RwLock`), 可直接放进 `web::Data`。
pub struct WsHub {
    index: RwLock<Index>,
    /// 因出站通道写满而丢弃的帧数(慢客户端)
    dropped: AtomicU64,
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            index: RwLock::new(Index::default()),
            dropped: AtomicU64::new(0),
        }
    }

    /// 建连时调用: 分配该连接的出站通道
    ///
    /// **必须在进入 `select!` 循环之前调用** —— 否则建连到进入循环之间的
    /// 广播会静默漏掉(与上一版「订阅必须在循环前完成」同一个道理)。
    ///
    /// 通道在**鉴权前**就已存在, 但此时连接还不在任何投递索引里, 所以收不到
    /// 任何东西 —— 这正是「连上就能听」旁听防护的结构性保证, 不依赖任何
    /// 运行时检查。
    pub fn attach(&self, conn: ConnId) -> mpsc::Receiver<Outbound> {
        let (tx, rx) = mpsc::channel(PER_CONN_CAPACITY);
        self.write().sinks.insert(conn, tx);
        rx
    }

    /// 鉴权成功时调用: 把连接放进投递索引
    ///
    /// `convs` 是**鉴权时快照**的成员会话集合 —— 与 `WsSession` 里那份同源,
    /// 所以接收侧的二次校验与索引的判断不会互相矛盾。
    pub fn mark_authenticated(
        &self,
        conn: ConnId,
        convs: impl IntoIterator<Item = ConversationId>,
    ) {
        let mut idx = self.write();
        idx.all_authed.insert(conn);
        for c in convs {
            idx.by_conversation.entry(c).or_default().insert(conn);
        }
    }

    /// 连接关闭时调用: 从索引与通道表中移除
    ///
    /// 漏调的后果是 `Index` 随连接数**只增不减** —— 内存泄漏, 且投递时会向
    /// 已关闭的通道 `try_send` 拿到 `Closed`。后者是无害的(见 `publish`),
    /// 但前者不会自愈。
    pub fn detach(&self, conn: ConnId) {
        let mut idx = self.write();
        idx.sinks.remove(&conn);
        idx.all_authed.remove(&conn);
        for set in idx.by_conversation.values_mut() {
            set.remove(&conn);
        }
        // 顺手回收空集合, 否则「进过 1 万个会话的连接断开」会留下 1 万个空桶
        idx.by_conversation.retain(|_, set| !set.is_empty());
    }

    /// 向目标连接投递一帧, 返回**本次实际投递成功的连接数**
    ///
    /// 返回 `0` 表示无人在线 —— 那是**常态而非错误**(对方上线后靠
    /// `GET /v1/conversations/{id}/messages` 补), 但必须让调用方**看得见**,
    /// 所以返回计数而不是 `()` 或 `let _ =`。
    ///
    /// **为什么不返回 `Result`**: 投递失败只有两种(通道满 / 通道已关), 调用方
    /// 对两者的动作都是「记一行日志、从不重试」—— 为一个不重试的失败付出每帧
    /// 一次巨型错误类型不划算。丢弃由 `dropped_broadcast_count()` 单独暴露。
    ///
    /// ## 锁的用法
    ///
    /// 用 `std::sync::RwLock` 而非 `tokio::sync::RwLock`, 因为本函数是**同步**
    /// 的(三个调用点都在同步上下文里), 且**临界区内没有任何 `.await`**:
    /// `try_send` 不阻塞, 序列化也刻意放在放锁**之后**。故不存在
    /// 「持有锁 await」的死锁面。写锁只在 `attach` / `mark_authenticated` /
    /// `detach` 时短暂持有。
    pub fn publish(&self, frame: ServerFrame) -> usize {
        // 1) 定投递范围。Undeliverable 直接短路 —— 上一版这里会把帧推给全部 N 个
        //    连接再让收信侧丢弃。
        let audience = Audience::of(&frame);
        if let Audience::Undeliverable(why) = &audience {
            // 不是 warn 级: 这类帧**本来就不该**走到 publish(ack/pong 走单连接
            // 直发)。走到这里说明调用方判断错了, 故记 warn。
            tracing::warn!(%why, "frame is not broadcastable; dropped at publish time");
            return 0;
        }

        // 2) 取收件人的 Sender 列表 —— 一次哈希查表, 锁内只克隆不发送
        let senders: Vec<mpsc::Sender<Outbound>> = {
            let idx = self.read();
            match &audience {
                Audience::Conversation(conv) => match idx.by_conversation.get(conv) {
                    Some(set) => set
                        .iter()
                        .filter_map(|id| idx.sinks.get(id))
                        .filter(|tx| !tx.is_closed())
                        .cloned()
                        .collect(),
                    None => Vec::new(),
                },
                Audience::All => idx
                    .all_authed
                    .iter()
                    .filter_map(|id| idx.sinks.get(id))
                    .filter(|tx| !tx.is_closed())
                    .cloned()
                    .collect(),
                Audience::Undeliverable(_) => unreachable!("已在上面短路"),
            }
        };
        if senders.is_empty() {
            return 0;
        }

        // 3) 序列化**一次**, 放在锁外
        let payload: Arc<str> = match serde_json::to_string(&frame) {
            Ok(s) => Arc::from(s.into_boxed_str()),
            Err(e) => {
                // 序列化失败说明该帧的 wire 形状有问题(比如 content 不合 schema)。
                // 库里的 content 在写入时已用同一个 schema 校验过, 正常路径不会到这。
                tracing::error!(error = %e, "ws broadcast serialize failed; frame dropped");
                return 0;
            }
        };

        // 4) 逐个收件人投递, 只克隆胖指针
        let outbound = Outbound { audience, payload };
        let mut delivered = 0usize;
        let mut dropped = 0u64;
        for tx in &senders {
            match tx.try_send(outbound.clone()) {
                Ok(()) => delivered += 1,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // 慢客户端: 丢本帧。与上一版 `Lagged` 的取舍一致 ——
                    // 实时消息宁可丢, 也不该让一个慢客户端把发送方卡住。
                    dropped += 1;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    // 连接已死但还没 detach。不计为投递, 也不计为丢弃 ——
                    // 它会在该连接的 detach 里被清掉。
                }
            }
        }
        if dropped > 0 {
            self.dropped.fetch_add(dropped, Ordering::Relaxed);
            tracing::warn!(
                dropped,
                delivered,
                "ws outbox full; frames dropped for slow clients (clients resync via REST)"
            );
        }
        delivered
    }

    /// 已建立出站通道的连接数(诊断用)
    ///
    /// 语义: **含未鉴权的连接** —— 它们已建好通道但不在投递索引里。
    /// 对应上一版 `broadcast::receiver_count()`。
    pub fn subscriber_count(&self) -> usize {
        self.read().sinks.len()
    }

    /// 已鉴权的连接数(诊断用)
    ///
    /// 上一版**无法**区分「连上了」与「鉴权通过了」—— 两者都只是一个
    /// `broadcast` 订阅者。有了索引后这个数是 O(1) 的, 且能直接回答
    /// 「连接都建好了却一个都没鉴权」这类问题。
    pub fn authenticated_count(&self) -> usize {
        self.read().all_authed.len()
    }

    /// 投递索引里的会话数(诊断用)
    pub fn indexed_conversation_count(&self) -> usize {
        self.read().by_conversation.len()
    }

    /// 因慢客户端通道写满而丢弃的帧数累计
    ///
    /// **这个计数不能省**: 上一版靠 `RecvError::Lagged` 暴露慢客户端,
    /// 本版没有 broadcast 就没有 Lagged, 若不另设计数, 丢帧会**完全静默** ——
    /// 客户端只会以为「对方没发言」, 而服务端毫无线索。
    pub fn dropped_broadcast_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// 读锁。
    ///
    /// `poison` 的处理值得说明: 若某个持锁的线程 panic, 索引会被毒化。我们
    /// **选择继续用**它(`into_inner`)而不是让整个广播永久不可用 ——
    /// 索引里只有 HashMap/HashSet, 不存在「panic 写到一半」导致的不一致状态,
    /// 而让广播挂掉的后果(所有人收不到实时消息)远重于「状态可能不完整」。
    fn read(&self) -> std::sync::RwLockReadGuard<'_, Index> {
        self.index.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Index> {
        self.index.write().unwrap_or_else(|e| e.into_inner())
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
    /// 只剩**单连接响应**(`Ack` / `AuthOk` / `Connected` / `Pong`)落在这一态:
    /// 它们承载的是某个请求的 `req_id` 或某条连接自己的握手结果, 广播出去等于
    /// 把别人的请求回执塞给无关客户端。
    ///
    /// ## 「语义属于某会话但形状不带 `conversation_id`」已不再是本变体的成因
    ///
    /// 2026-10-07 前, `MessageEdited` / `ReactionAdded` 落在这里 —— 因为
    /// aux-13 §1.2.6/§1.2.8 的 wire 形状不带 `conversation_id`, 无从判断
    /// 接收方是否成员, 而「发给所有人」就是跨会话泄漏。后果是**编辑与表情
    /// 完全没有实时同步**。
    ///
    /// 现已给两帧补上**必填单态** `conversation_id`(用户拍板), 它们与
    /// `MessageNew` 走同一条 `Audience::Conversation` 定向投递路径。
    ///
    /// 那个二态 `Option` 版本踩过一次坑(`None` 退化成发给所有人 = 泄漏),
    /// 故现在用**单态**从类型上根除: 不存在「没有 conversation_id 的帧」这种
    /// 值, 于是也不存在「拿不到会话就发给所有人」这条分支。
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

            ServerFrame::MessageEdited {
                conversation_id, ..
            } => Audience::Conversation(ConversationId(*conversation_id)),

            ServerFrame::ReactionAdded {
                conversation_id, ..
            } => Audience::Conversation(ConversationId(*conversation_id)),

            ServerFrame::Ack { .. } => Audience::Undeliverable("ack 是单连接响应, 不得广播"),
            ServerFrame::AuthOk { .. } => Audience::Undeliverable("auth_ok 是单连接响应, 不得广播"),
            ServerFrame::Connected { .. } => Audience::Undeliverable("connected 是单连接响应"),
            ServerFrame::Pong { .. } => Audience::Undeliverable("pong 是单连接响应"),
        }
    }
}

/// 该帧是否应投递给这个连接 —— **接收侧的纵深防御**
///
/// ## 它已经不是唯一的过滤器
///
/// 上一版这条规则是**全部**的广播过滤逻辑; 本版里投递索引已先做了过滤, 本函数
/// 退居二次校验: 索引说该收**且**会话说该收, 两者同时成立才写出去。
///
/// 保留它的理由是安全边界不该单点依赖 —— 索引若因任何 bug 多收了连接, 这一层
/// 仍能拦住。代价是每个**实际收件人**一次 `HashSet::contains`, 不是 O(N)。
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

    // ========================================================================
    // 投递范围判定 —— 这是安全边界, 不是优化
    // ========================================================================

    fn sample_message(conv: uuid::Uuid) -> Message {
        Message {
            id: MessageId(uuid::Uuid::new_v4()),
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

    fn new_message_frame(conv: ConversationId) -> ServerFrame {
        ServerFrame::MessageNew {
            message: to_wire_message(&sample_message(conv.0)).unwrap(),
        }
    }

    /// 构造一个已鉴权、属于 `convs` 的连接
    fn authed_session(convs: &[ConversationId]) -> WsSession {
        let mut s = WsSession::new(uuid::Uuid::new_v4(), super::super::session::new_heartbeat());
        s.mark_authenticated(UserId::new(), EnvironmentId::new(), DeviceSessionId::new());
        s.set_conversation_ids(membership_set(convs.iter()));
        s
    }

    fn created_session() -> WsSession {
        WsSession::new(uuid::Uuid::new_v4(), super::super::session::new_heartbeat())
    }

    #[test]
    fn message_new_targets_its_conversation() {
        let conv = ConversationId(uuid::Uuid::new_v4());
        let frame = new_message_frame(conv);
        assert_eq!(Audience::of(&frame), Audience::Conversation(conv));
    }

    #[test]
    fn typing_and_recall_target_their_conversation() {
        let conv = ConversationId(uuid::Uuid::new_v4());
        let other = ConversationId(uuid::Uuid::new_v4());

        let typing = ServerFrame::Typing {
            conversation_id: conv.0,
            user_id: UserId::new().0,
        };
        assert_eq!(Audience::of(&typing), Audience::Conversation(conv));

        let recall = ServerFrame::MessageRecalled {
            message_id: uuid::Uuid::new_v4(),
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
    fn session_scoped_frames_are_delivered_only_to_members() {
        // 2026-10-07: 本测试是**反转**后的版本。
        //
        // 此前叫 `frames_without_conversation_id_are_never_delivered`, 断言
        // `MessageEdited` / `ReactionAdded` 必须 `Undeliverable` —— 那是在给
        // 「两帧没有 conversation_id 所以不能发」这个**权宜之计**站岗。
        //
        // 用户拍板给两帧补了**必填单态** `conversation_id`, 它们现在必须
        // **定向投递给会话成员**, 且**非成员绝不收**(这才是跨会话泄漏的闸门)。
        //
        // 「没有 conversation_id 的帧」已不可能存在 —— 那是单态字段带来的
        // 类型级保证, 所以这里断言的是**成员/非成员**而不是「可不可投递」。
        let conv = ConversationId(uuid::Uuid::new_v4());
        let other = ConversationId(uuid::Uuid::new_v4());

        let edited = ServerFrame::MessageEdited {
            message_id: uuid::Uuid::new_v4(),
            conversation_id: conv.0,
            content: MessageContent::Text { text: "x".into() },
            edited_at: chrono::Utc::now(),
        };
        let react = ServerFrame::ReactionAdded {
            message_id: uuid::Uuid::new_v4(),
            conversation_id: conv.0,
            user_id: UserId::new().0,
            emoji: "👍".into(),
        };

        for frame in [edited, react] {
            let a = Audience::of(&frame);
            assert!(
                matches!(a, Audience::Conversation(c) if c == conv),
                "两帧现在必须定向投递到所属会话, 实际 {a:?}"
            );
            // 成员收得到
            assert!(
                should_deliver(&a, &authed_session(&[conv, other])),
                "会话成员应当收到"
            );
            // **非成员收不到** —— 这条才是防跨会话泄漏的闸门
            assert!(
                !should_deliver(&a, &authed_session(&[other])),
                "非会话成员绝不能收到 {a:?} —— 这就是跨会话泄漏"
            );
        }
    }

    #[test]
    fn single_connection_responses_are_never_broadcast() {
        // `AuthOk` 于 2026-10-07 加入本集合: 它是**鉴权回执**, 语义上只对
        // 那一条连接成立, 广播出去等于把别人的握手结果塞给无关客户端。
        //
        // 此前 `auth_ok` 是手工构造的裸 JSON, 根本不经过 `Audience::of` ——
        // 所以它此前**无法**被这条闸门覆盖。现在它有类型了, 才谈得上「判定
        // 它不该被广播」。
        for frame in [
            ServerFrame::Ack {
                req_id: uuid::Uuid::new_v4(),
                ok: true,
                data: None,
                error: None,
            },
            ServerFrame::AuthOk {
                req_id: Some(uuid::Uuid::new_v4()),
            },
            ServerFrame::Connected {
                session_id: uuid::Uuid::new_v4(),
            },
            ServerFrame::Pong { ts: 0 },
        ] {
            let a = Audience::of(&frame);
            assert!(
                matches!(a, Audience::Undeliverable(_)),
                "单连接响应应判为不可投递, 实际 {a:?}"
            );
            // 即便连接是「全员」(拥有所有会话), 也**不得**收到
            assert!(
                !should_deliver(&a, &authed_session(&[ConversationId(uuid::Uuid::new_v4())])),
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
            &Audience::Conversation(ConversationId(uuid::Uuid::new_v4())),
            &s
        ));
    }

    // ========================================================================
    // Message → WireMessage 转换
    // ========================================================================

    #[test]
    fn to_wire_message_roundtrips_content_and_state() {
        let conv = ConversationId(uuid::Uuid::new_v4());
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
        let mut m = sample_message(ConversationId(uuid::Uuid::new_v4()).0);
        m.content = serde_json::json!({ "kind": "text" }); // 缺 text 字段
        assert!(
            to_wire_message(&m).is_err(),
            "坏 content 应返回 Err 而非伪造"
        );
    }

    // ========================================================================
    // 投递索引 —— 证明成本是 O(收件人) 而不是 O(全部连接)
    // ========================================================================

    /// 建一条「已鉴权、属于 `convs`」的 hub 连接, 返回它的收件通道
    fn join(hub: &WsHub, convs: &[ConversationId]) -> (ConnId, mpsc::Receiver<Outbound>) {
        let conn = uuid::Uuid::new_v4();
        let rx = hub.attach(conn);
        hub.mark_authenticated(conn, convs.iter().copied());
        (conn, rx)
    }

    #[tokio::test]
    async fn publishes_only_to_members_of_the_target_conversation() {
        let hub = WsHub::new();
        let target = ConversationId(uuid::Uuid::new_v4());
        let other = ConversationId(uuid::Uuid::new_v4());

        let (_a, mut ra) = join(&hub, &[target]);
        let (_b, mut rb) = join(&hub, &[target]);
        let (_c, mut rc) = join(&hub, &[other]);

        let delivered = hub.publish(new_message_frame(target));
        assert_eq!(delivered, 2, "只有该会话的两名成员应收到");

        // 关键断言: 非成员的通道**是空的**, 不是「收到了但被过滤掉」。
        // 上一版这里会是 3 次缓冲写入 + 3 次过滤。
        assert!(a_payload(&mut ra).await.is_some());
        assert!(a_payload(&mut rb).await.is_some());
        assert!(
            rc.try_recv().is_err(),
            "非成员的出站通道里不该有任何东西 —— 这就是没有 fan-out 的直接证据"
        );
    }

    async fn a_payload(rx: &mut mpsc::Receiver<Outbound>) -> Option<String> {
        rx.try_recv().ok().map(|o| o.payload.to_string())
    }

    #[tokio::test]
    async fn all_recipients_share_one_serialized_copy() {
        // 序列化只做一次的**可观测**后果: 所有收件人拿到的是**同一个** Arc,
        // 即同一个堆分配。用 ptr 相等断言, 而不是"内容相等"(内容相等无法区分
        // 共享与各自拷贝)。
        let hub = WsHub::new();
        let conv = ConversationId(uuid::Uuid::new_v4());
        let (_a, mut ra) = join(&hub, &[conv]);
        let (_b, mut rb) = join(&hub, &[conv]);

        assert_eq!(hub.publish(new_message_frame(conv)), 2);
        let oa = ra.try_recv().unwrap();
        let ob = rb.try_recv().unwrap();
        assert!(
            std::ptr::eq(Arc::as_ptr(&oa.payload), Arc::as_ptr(&ob.payload)),
            "两个收件人应共享同一份序列化结果"
        );
        assert!(oa.payload.contains("\"type\":\"message_new\""));
    }

    #[tokio::test]
    async fn undeliverable_frame_touches_no_connection_at_all() {
        // 上一版: ack/message_edited 会推给**全部**订阅者再由收信侧丢弃。
        // 本版: publish 直接返回 0, 一个通道都不碰。
        let hub = WsHub::new();
        let conv = ConversationId(uuid::Uuid::new_v4());
        let (_a, mut ra) = join(&hub, &[conv]);

        let ack = ServerFrame::Ack {
            req_id: uuid::Uuid::new_v4(),
            ok: true,
            data: None,
            error: None,
        };
        assert_eq!(hub.publish(ack), 0);
        assert!(ra.try_recv().is_err(), "不可投递的帧不应进入任何通道");
    }

    #[tokio::test]
    async fn unauthenticated_connection_receives_nothing_even_with_a_sink() {
        // 旁听防护的结构性保证: attach 建了通道, 但没进索引 -> 收不到。
        let hub = WsHub::new();
        let conv = ConversationId(uuid::Uuid::new_v4());
        let conn = uuid::Uuid::new_v4();
        let mut rx = hub.attach(conn);
        // 故意不调 mark_authenticated

        assert_eq!(hub.subscriber_count(), 1, "通道已建");
        assert_eq!(hub.authenticated_count(), 0, "但未鉴权");
        assert_eq!(hub.publish(new_message_frame(conv)), 0);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn detach_removes_connection_from_index_and_channels() {
        let hub = WsHub::new();
        let conv = ConversationId(uuid::Uuid::new_v4());
        let (a, _ra) = join(&hub, &[conv]);
        let (b, _rb) = join(&hub, &[conv]);
        assert_eq!(hub.subscriber_count(), 2);
        assert_eq!(hub.authenticated_count(), 2);
        assert_eq!(hub.indexed_conversation_count(), 1);

        hub.detach(a);
        assert_eq!(hub.subscriber_count(), 1, "通道表应少一条");
        assert_eq!(hub.authenticated_count(), 1, "全局集合应少一条");
        assert_eq!(hub.publish(new_message_frame(conv)), 1, "只剩 b 该收");

        hub.detach(b);
        // 最后一个成员断开后, 空会话桶应被回收 —— 否则索引只增不减, 内存泄漏
        assert_eq!(hub.indexed_conversation_count(), 0);
        assert_eq!(hub.publish(new_message_frame(conv)), 0);
    }

    #[tokio::test]
    async fn slow_client_is_counted_not_silently_dropped() {
        // 上一版靠 `RecvError::Lagged` 暴露慢客户端。本版没有 broadcast, 若不
        // 另设计数, 丢帧会完全静默 —— 客户端只会以为"对方没发言"。
        let hub = WsHub::new();
        let conv = ConversationId(uuid::Uuid::new_v4());
        let (conn, _rx) = join(&hub, &[conv]); // 故意不消费, 制造积压

        let mut last = 0usize;
        for _ in 0..(PER_CONN_CAPACITY + 10) {
            last = hub.publish(new_message_frame(conv));
        }
        assert_eq!(last, 0, "通道已满, 后续全部丢弃");
        assert_eq!(
            hub.dropped_broadcast_count(),
            (PER_CONN_CAPACITY + 10 - PER_CONN_CAPACITY) as u64
        );
        hub.detach(conn);
    }

    #[tokio::test]
    async fn global_frames_reach_all_authed_but_not_unauthed() {
        let hub = WsHub::new();
        let (_a, mut ra) = join(&hub, &[]);
        let ghost = uuid::Uuid::new_v4();
        let mut rg = hub.attach(ghost); // 未鉴权

        let frame = ServerFrame::ForceDisconnect {
            reason: "token_revoked".into(),
        };
        assert_eq!(hub.publish(frame), 1);
        assert!(ra.try_recv().is_ok());
        assert!(rg.try_recv().is_err(), "未鉴权连接不得收到全局帧");
    }

    #[tokio::test]
    async fn publish_without_subscriber_reports_zero_not_panic() {
        // 无人在线是常态, 不是错误 —— 但必须**如实回报 0**, 不能吞掉。
        let hub = WsHub::new();
        let conv = ConversationId(uuid::Uuid::new_v4());
        assert_eq!(hub.publish(new_message_frame(conv)), 0);
        assert_eq!(hub.subscriber_count(), 0);
    }

    #[tokio::test]
    async fn large_fleet_does_not_fan_out_to_non_members() {
        // 本次改造的核心断言: 1000 条连接分布在 500 个会话上, 往其中一个会话
        // 发 1 条消息, **只有**那 2 个成员收到, 其余 998 个通道**全空**。
        //
        // 上一版这里会写 1000 格缓冲; 本版只写 2 格。这是 O(N) -> O(收件人) 的
        // 可观测证据 —— 不是靠计时, 而是靠"非收件人的通道里确实什么都没有"。
        const CONVS: usize = 500;
        const PER_CONV: usize = 2;

        let hub = WsHub::new();
        let convs: Vec<ConversationId> = (0..CONVS)
            .map(|_| ConversationId(uuid::Uuid::new_v4()))
            .collect();
        let mut receivers: Vec<mpsc::Receiver<Outbound>> = Vec::with_capacity(CONVS * PER_CONV);
        for c in &convs {
            for _ in 0..PER_CONV {
                let (_id, rx) = join(&hub, std::slice::from_ref(c));
                receivers.push(rx);
            }
        }
        assert_eq!(hub.subscriber_count(), CONVS * PER_CONV);

        let target = convs[0];
        assert_eq!(hub.publish(new_message_frame(target)), PER_CONV);

        for (i, rx) in receivers.iter_mut().enumerate() {
            let got = rx.try_recv().is_ok();
            assert_eq!(
                got,
                i < PER_CONV,
                "只有前 {PER_CONV} 个是该会话成员, 第 {i} 个的有无应为 {got}"
            );
        }
    }
}
