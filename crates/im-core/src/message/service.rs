//! MessageService — 发消息主路径(最重要的 service)
//!
//! 依据: ImplementationSpec §7.4.3 + DetailedDesign §9.1
//!
//! ## 5 步实现(C-2 WBS)
//! 1. 幂等检查(同 (conv, sender, idem_key) 已存在则直接返回,视为成功)
//! 2. 校验:content 大小 + 按 kind 的 schema 校验 + 会话成员 + DM friend 关系校验
//! 3. 开事务:取 sequence(行锁强单调) + insert
//! 4. 提交事务后发布 `im.message.created` 事件(失败不阻塞 ack; publisher
//!    按 aux-08 重试并把耗尽的事件写入 NATS DLQ)
//! 5. 返回 Message
//!
//! ## Friend 关系校验(per 132-wbs §5.3.1 C-2 验收)
//! DM 会话中,sender 不得是已被对端拉黑的人(2026-10-08 实装, 见 `check_block`)。
//! 「必须是 accepted 好友」这条更强的约束仍未实装 —— 现状只要求 sender 是会话
//! 成员且未被拉黑, 见 `docs/gap-ledger.md` §1.44。
//!
//! ## 2026-09-01 C-2 实装要点
//! - 移除 `if let Ok(content) = ...` 静默失败路径,改为 `MessageContent` 严格反序列化
//! - 校验放在事务前(避免无效 insert 浪费 sequence)
//! - friend 关系校验通过 `ConversationRepository::is_member` 做兜底(简单版,完整 friend
//!   关系校验在 im-gateway 层 + 后续 C-7 link_account 阶段补充)
//!
//! ## 2026-10-08 实装要点
//! - 步骤 2e 的拉黑校验由恒 `Ok(false)` 的空壳改为真查 `friendships`。
//!   在此之前 `POST /v1/friends/{id}/block` 写库成功但对消息完全不生效。
//! - `if let Ok(true) = ..` 这个 fail-open 形状一并消除: 拉黑状态查不出来时
//!   上抛错误, 不放行。

use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use im_common::ids::{ConversationId, MessageId, UserId};
use im_common::AppError;

use super::content::{validate_fields, validate_serialized_size};
use super::repository::{Message, MessageRepository, MessageState, NewMessage};
use super::sequence::SequenceAllocator;
use crate::conversation::repository::ConversationRepository;
use crate::event::events::{MessageCreatedEvent, MessageRecalledEvent};
use crate::event::publisher::EventPublisher;
use crate::relationship::repository::BlockChecker;

/// `GET /v1/conversations/{id}/messages` 单页上限
///
/// 显式常量: handler 算 `has_more` 时必须用**同一个**上限。
/// 2026-10-08 之前 handler 拿客户端原始 `limit` 去比返回条数, 而 service 把
/// limit 静默截到 200 —— 客户端传 `limit=1000` 时服务端返 200 条,
/// `200 == 1000` 为假 → `has_more=false`, 客户端**静默停止翻页**, 第 201 条
/// 之后的消息永久丢失, 响应里没有任何线索表明数据被截断。
pub const MAX_PAGE: i32 = 200;

#[derive(Debug, Clone)]
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

/// 撤回时间窗判定 (per aux-04 §B.4 转换表: `now - created_at ≤ recall_window` 才允许)
///
/// ## 为什么抽成独立的纯函数
///
/// 边界语义(`≤` 允许 / 恰好等于上限**仍允许** / 超 1ms 即拒)是这条路径上
/// 最容易被改坏的一处 —— `>` 写成 `>=` 编译照过、绝大多数测试照过, 只有
/// 「恰好在边界上」那一条会变红。
///
/// 而「恰好在边界」**无法通过真实时钟观测**: 曾试图用
/// `UPDATE messages SET created_at = now() - interval '120 seconds'` 把消息
/// 回拨到恰好等于窗口上限, 结果该用例稳定失败 —— 因为 UPDATE 与 service
/// 调用之间已经过去了几毫秒, `elapsed` 实际是 120.00Xs 而非 120s。
/// 挂钟回拨只能测「明显在窗内」和「明显超窗」, 中间那一毫秒是测不到的。
///
/// 抽成纯函数后可以用**精确构造的入参**测边界本身, 比通过数据库回拨测**更强**:
/// `within_recall_window(t + 120s, t, 120s)` 恰好验证了 `≤` 这个字符, 而
/// 不依赖任何时序运气。
///
/// `created_at` 晚于 `now`(时钟漂移 / 跨机房写入)时按 `elapsed = 0` 处理,
/// 即**允许**撤回 —— 拿不到可靠时间差时, 拒绝一次合法撤回比误放更糟。
pub fn within_recall_window(
    now: chrono::DateTime<Utc>,
    created_at: chrono::DateTime<Utc>,
    window: chrono::Duration,
) -> bool {
    let elapsed_ms = now.signed_duration_since(created_at).num_milliseconds();
    elapsed_ms.max(0) <= window.num_milliseconds()
}

pub struct MessageService {
    repo: Arc<dyn MessageRepository>,
    sequencer: Arc<dyn SequenceAllocator>,
    events: Arc<dyn EventPublisher>,
    /// C-2 新增:用于 sender 是 conversation member 的快速校验
    conversation_repo: Arc<dyn ConversationRepository>,
    /// 2026-10-08 新增:DM 发送前的拉黑校验。
    ///
    /// ## 为什么以前不需要它
    ///
    /// `check_block` 曾经是个恒返 `Ok(false)` 的空壳, 理由写的是「避免注入
    /// RelationshipService 引起循环依赖, 完整实装在 C-9 + im-gateway 边界」。
    /// 于是从 2026-08 到 2026-10-07, `POST /v1/friends/{id}/block` 能成功写入
    /// `friendships(state='blocked')`, 但**没有任何发送路径会读它** —— 拉黑
    /// 对消息完全不生效。C-9 早已完成, 这个推迟项没人回来做。
    ///
    /// ## 为什么注入窄接口而不是 RelationshipService
    ///
    /// 依赖 `BlockChecker`(1 个方法)而不是整个好友仓储(6 个方法), 发消息这条
    /// 主路径不必为一次布尔查询背上整个好友域的接口面。注入 trait 而非具体
    /// 类型, 也让测试能直接构造「恒不拉黑 / 恒拉黑」的替身。
    block_checker: Arc<dyn BlockChecker>,
}

impl MessageService {
    pub fn new(
        repo: Arc<dyn MessageRepository>,
        sequencer: Arc<dyn SequenceAllocator>,
        events: Arc<dyn EventPublisher>,
        conversation_repo: Arc<dyn ConversationRepository>,
        block_checker: Arc<dyn BlockChecker>,
    ) -> Self {
        Self {
            repo,
            sequencer,
            events,
            conversation_repo,
            block_checker,
        }
    }

    /// 发消息主路径
    pub async fn send_message(&self, cmd: SendMessageCommand) -> Result<Message, AppError> {
        // === 第 1 步:幂等检查 ===
        // 同 (conv, sender, idem_key) 已存在 → 直接返回原 Message(重试视为成功)
        if let Some(existing) = self
            .repo
            .find_by_idempotency_key(cmd.conversation_id, cmd.sender_id, &cmd.idempotency_key)
            .await?
        {
            tracing::debug!(
                message_id = %existing.id,
                sequence = existing.sequence,
                "idempotent replay, returning existing message"
            );
            return Ok(existing);
        }

        // === 第 2 步:校验 ===
        // 2a. content 必须是非空 JSON object
        if !matches!(cmd.content, Value::Object(_)) {
            return Err(AppError::Validation("content must be JSON object".into()));
        }

        // 2b. content 大小
        let serialized = serde_json::to_string(&cmd.content).unwrap_or_default();
        if serialized.len() > cmd.max_size_bytes {
            return Err(AppError::MessageTooLarge(
                serialized.len(),
                cmd.max_size_bytes,
            ));
        }

        // 2c. content schema(按 kind 反序列化为 MessageContent 严格校验)
        let content: im_protocol::content::MessageContent =
            serde_json::from_value(cmd.content.clone())
                .map_err(|e| AppError::Validation(format!("content schema: {}", e)))?;
        validate_fields(&content)?;
        validate_serialized_size(&content, cmd.max_size_bytes)?;

        // 2d. sender 必须是会话成员(防止越权发消息)
        let is_member = self
            .conversation_repo
            .is_member(cmd.conversation_id, cmd.sender_id)
            .await?;
        if !is_member {
            return Err(AppError::Forbidden(format!(
                "user {} is not a member of conversation {}",
                cmd.sender_id.0, cmd.conversation_id.0
            )));
        }

        // 2e. DM 拉黑校验(C-2 验收点)
        //
        // 仅在 conversations.kind='dm' 时生效;group/channel/broadcast 跳过。
        //
        // 2026-10-08:此前这一段的调用是 `if let Ok(true) = self.check_block(..)`,
        // 而 `check_block` 恒返 `Ok(false)` —— 两重失效叠在一起: 恒 false 的
        // 空壳, 加上 `if let Ok(..)` 把查询失败当成「没被拉黑」而放行(fail-open)。
        // 现在改成错误上抛: 数据库查不了拉黑状态时宁可发不出去, 也不放行。
        if let Some(conv) = self
            .conversation_repo
            .find_by_id(cmd.conversation_id)
            .await?
        {
            if matches!(
                conv.kind,
                crate::conversation::repository::ConversationKind::Dm
            ) {
                // 拿 conversation 全部 members,找"对端"
                let members = self
                    .conversation_repo
                    .list_members(cmd.conversation_id)
                    .await?;
                let other = members
                    .iter()
                    .find(|m| m.user_id != cmd.sender_id)
                    .map(|m| m.user_id);
                // DM 必有 2 个成员,找不到说明数据异常。
                //
                // 这里**不能**静默跳过: 数据异常意味着这条消息根本无从判断
                // 是否该被拉黑拦下。与其放行(默认允许), 不如报出来让运维
                // 看见 —— 否则「拉黑失效」会再次变成一条查不到根因的静默行为。
                let Some(other_id) = other else {
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "dm conversation {} has no member other than sender {}",
                        cmd.conversation_id.0,
                        cmd.sender_id.0
                    )));
                };
                if self.check_block(other_id, cmd.sender_id).await? {
                    return Err(AppError::UserBlocked);
                }
            }
        }

        // === 第 3 步:开事务,取 sequence + insert ===
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
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx insert: {}", e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx commit: {}", e)))?;

        // === 第 4 步:提交后发事件(失败不阻塞 ack; publisher 已写 DLQ) ===
        let event = MessageCreatedEvent {
            message_id: msg.id,
            conversation_id: msg.conversation_id,
            sender_id: msg.sender_id,
            sequence: msg.sequence,
            kind: msg.kind.clone(),
            ts: Utc::now(),
        };
        if let Err(e) = self.publish_event("im.message.created", &event).await {
            // 消息本身已落库并会返回给客户端, 所以这里只记日志。
            // 措辞必须指向**当下真实存在的**恢复路径(publisher 写的 DLQ), 而不是
            // 一个「V1+ 会有 outbox」的将来式 —— 故障期间照着日志去查的人会
            // 找不到任何东西。
            tracing::error!(
                error = %e,
                message_id = %msg.id,
                "publish im.message.created failed; see im_events_dlq_total / \
                 im_events_dlq_write_failed_total (DLQ pending replay)"
            );
        }

        // === 第 5 步:返回完整 Message ===
        Ok(msg)
    }

    /// DM 拉黑校验:`sender` 是否已被 `other` 拉黑。
    ///
    /// ## 实装(2026-10-08)
    ///
    /// 此前这里是 `let _ = (other, sender); Ok(false)` —— 签名在, 注释在,
    /// 错误码 `AppError::UserBlocked` 也在, 唯独判断这件事没做, 所以
    /// 「拉黑后不能发消息」这条产品语义从未生效。本函数此前 2 个月的行为
    /// 与「任何人都没被拉黑」完全无法区分。
    ///
    /// ## 参数方向
    ///
    /// `is_blocked(user, target)` 的语义是「user 被 target 拉黑」(查
    /// `friendships` 的 `target → user` 方向)。所以问「sender 被 other 拉黑吗」
    /// 要传 `(sender, other)`, 与本函数入参顺序 `(other, sender)` **相反**。
    /// 2026-10-03 已在 `RelationshipService::send_request` 上把方向写反过一次,
    /// 这里显式写明以免第三次。
    async fn check_block(&self, other: UserId, sender: UserId) -> Result<bool, AppError> {
        self.block_checker.is_blocked(sender, other).await
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

        // 2.1 状态机: 已撤回 / 已删除的消息不可编辑
        //
        // 2026-10-03 新增。此前没有这道守卫 —— 若只看 sender, 已撤回的消息
        // 仍可被编辑出内容, 与「撤回」的语义直接冲突(撤回后应只剩墓碑)。
        // 用已注册错误码 `INVALID_STATE_TRANSITION`(per aux-03 §B, 409),
        // 不新造码。
        match existing.state {
            MessageState::Recalled | MessageState::Deleted => {
                return Err(AppError::InvalidStateTransition {
                    from: existing.state.as_str().to_string(),
                    to: "edited".into(),
                });
            }
            _ => {}
        }

        // 3. 大小校验
        let serialized = serde_json::to_string(&new_content).unwrap_or_default();
        if serialized.len() > max_size_bytes {
            return Err(AppError::MessageTooLarge(serialized.len(), max_size_bytes));
        }

        // 4. content schema
        let content: im_protocol::content::MessageContent =
            serde_json::from_value(new_content.clone())
                .map_err(|e| AppError::Validation(format!("content schema: {}", e)))?;
        validate_fields(&content)?;
        validate_serialized_size(&content, max_size_bytes)?;

        // 5. 落库
        //
        // 2026-10-03 实装。此前本函数做完上面 4 步校验后**无条件**返回
        // `AppError::Internal("not yet implemented")` —— 也就是每次调用都得到
        // 一个 500 级错误, 看起来像服务端故障而不是「功能没做」。仓储层当时
        // 根本没有改 content 的方法(只有 `update_state`), 现已补
        // `MessageRepository::update_content`。
        let updated = self
            .repo
            .update_content(message_id, &new_content)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;
        Ok(updated)
    }

    /// 撤回消息 (per aux-04 §B.4 转换表 line 241)
    ///
    /// | from | event | to | guard | 失败码 |
    /// |---|---|---|---|---|
    /// | `sent` / `delivered` / `read` | `recall_message` | `recalled` | actor = sender 且 now - created_at ≤ recall_window | `RECALL_WINDOW_EXPIRED` / `FORBIDDEN` |
    /// | `recalled` | 任何 | 拒绝(终态) | — | `INVALID_STATE_TRANSITION` |
    /// | `deleted` | 任何 | 拒绝(终态) | — | `INVALID_STATE_TRANSITION` |
    ///
    /// **关于 `read → recalled`**: aux-04 的**转换图** line 225 在这条边上写了
    /// 「V1+ 评估是否允许」(GAP-3), 而**转换表** line 241 明确把 `read` 列入
    /// 允许迁移的来源集。此处以转换表为准(它才是规范性的那张表), 且 GAP-3
    /// 的原文问题其实是**展示**层面 —— 「已读撤回是否还显示"已读"标识?」,
    /// 不是转移本身是否允许。若 PM 认为应禁止, 改本函数的状态守卫即可, 落库
    /// 与事件逻辑不受影响。
    ///
    /// ## 为什么 `recall_window` 由调用方传入而不自己读
    ///
    /// aux-04 不变量要求窗口来自 `env.settings.message.recall_window_seconds`
    /// 且「不能写死」, 但**读 settings 需要 `EnvironmentId`** —— service 从
    /// `messages` 行拿不到它(要经 conversation), 而 MessageService 的构造参数里
    /// 没有 settings 依赖。故与 `edit_message(max_size_bytes)` 同一约定: 参数传入,
    /// 由 gateway 用 `SettingsService` 解析后传进来。这样 service 可脱离 DB 单测,
    /// 且「值从哪来」只有一处(见 `im-gateway::ws::handle_recall_message`)。
    pub async fn recall_message(
        &self,
        message_id: MessageId,
        user_id: UserId,
        recall_window: chrono::Duration,
    ) -> Result<Message, AppError> {
        // 1. 查原 message
        let existing = self
            .repo
            .find_by_id(message_id)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;

        // 2. 仅 sender 可撤回 (per aux-04 §B.4 `actor = sender`)
        if existing.sender_id != Some(user_id) {
            return Err(AppError::Forbidden("not the message sender".into()));
        }

        // 3. 状态机守卫: recalled / deleted 是**终态**(per 转换表 line 243-244)
        match existing.state {
            MessageState::Recalled | MessageState::Deleted => {
                return Err(AppError::InvalidStateTransition {
                    from: existing.state.as_str().to_string(),
                    to: "recalled".into(),
                });
            }
            // sent / delivered / read 三态均允许(转换表 line 241)
            MessageState::Sent | MessageState::Delivered | MessageState::Read => {}
        }

        // 4. 时间窗: now - created_at ≤ recall_window
        if !within_recall_window(Utc::now(), existing.created_at, recall_window) {
            return Err(AppError::RecallWindowExpired(existing.created_at));
        }

        // 5. 落库 + 发事件 (per aux-04 不变量「转换必须 publish im.message.recalled」)
        self.repo
            .update_state(message_id, MessageState::Recalled)
            .await?;
        // 重读以返回**更新后**的行 —— `update_state` 只返回 (), 直接把
        // `existing` 改一下 state 返回会掩盖「库里到底写成没有」, 而那正是
        // 本轮反复在堵的那类「做完校验就返回假值」。
        let updated = self
            .repo
            .find_by_id(message_id)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;

        let event = MessageRecalledEvent {
            message_id: updated.id,
            conversation_id: updated.conversation_id,
            actor_id: user_id,
            ts: Utc::now(),
        };
        if let Err(e) = self.publish_event("im.message.recalled", &event).await {
            // 与 send_message 同一约定: 事件失败不阻塞调用方(状态已落库)。
            // publisher 内部已按 aux-08 重试并写 DLQ, 所以这条错误说的是
            // 「DLQ 里待重放」而不是「等某个将来的 outbox」—— 两者的处置不同。
            tracing::error!(
                error = %e,
                message_id = %updated.id,
                "publish im.message.recalled failed; see im_events_dlq_total / \
                 im_events_dlq_write_failed_total (DLQ pending replay)"
            );
        }
        Ok(updated)
    }

    /// 序列化 + 发布一个领域事件
    ///
    /// ## 为什么序列化失败也要**继续发**
    ///
    /// `EventPublisher::publish` 收的是字节, 序列化在调用方做(per DetailedDesign)。
    /// 本项目的事件 payload 全是 String / i64 / DateTime / Option 的平铺结构,
    /// 序列化在实践中不会失败。真失败时发空 payload 是**错的**, 但此时
    /// 「漏发」与「发空」都已是坏状态 —— 故选择「记录 + 照常发」, 让
    /// 订阅方至少能感知到「有这个 topic 的活动」, 而错误留在服务端日志里。
    /// 静默 return 会让事件**完全消失**, 那是更坏的失败模式。
    async fn publish_event<T: serde::Serialize>(
        &self,
        topic: &str,
        event: &T,
    ) -> Result<(), AppError> {
        let payload = serde_json::to_vec(event).unwrap_or_else(|e| {
            tracing::error!(error = %e, topic, "event serialize failed; publishing empty payload");
            Vec::new()
        });
        self.events.publish(topic, &payload).await
    }

    /// 按 id 查消息 (2026-10-03 新增)
    ///
    /// 存在的理由: REST 的消息动作端点(`edit` / `recall` / `reactions`)必须先
    /// 确认「这条消息确实属于 path 里给的会话」, 否则 URL 会说谎(客户端以为在
    /// 会话 A 编辑, 资源其实在 B), 且按 conversation 做的审计与限流全部错位。
    ///
    /// **不提供 `repo()` 逃生口**: 那样等于把仓储层暴露给 gateway, 业务规则会
    /// 开始散落到 handler 层 —— 那正是本轮反复在堵的「两条路径漂移」。
    /// 需要哪条查询就在这里显式加一个方法。
    pub async fn get(&self, message_id: MessageId) -> Result<Option<Message>, AppError> {
        self.repo.find_by_id(message_id).await
    }

    /// 按幂等键预查既有消息 (2026-10-03 新增, 供 WS ack 判定 `idempotent_replay`)
    ///
    /// `send_message` 内部遇到重放会返回既有 message, 但**不告诉调用方这是重放**。
    /// REST 路径不关心(REST 的 `IDEMPOTENCY_CONFLICT` 语义是 HTTP 200 返回原
    /// message_id), 而 WS 路径必须回填 `AckData.idempotent_replay`(per
    /// aux-13 §1.2.2 / §1.2.3), 否则客户端无法区分「新消息」与「重放」。
    ///
    /// 注意: 这只是**预判**, 不替代 `send_message` 内部的检查 —— 并发下仍可能
    /// 两边都查不到, 由 `send_message` 内部 + 唯一索引兜底。此时本方法返回
    /// `None` 而实际发生了重放, `idempotent_replay` 会报 false。这是已知的
    /// 保守偏差(把重放报成新消息, 好过把新消息报成重放), 未实装更精确的方案。
    pub async fn find_by_idempotency_key(
        &self,
        conversation_id: ConversationId,
        sender_id: UserId,
        idempotency_key: &str,
    ) -> Result<Option<Message>, AppError> {
        self.repo
            .find_by_idempotency_key(conversation_id, sender_id, idempotency_key)
            .await
    }

    pub async fn list_messages(
        &self,
        conversation_id: ConversationId,
        _user_id: UserId, // 成员校验由 service 调用方完成
        after_sequence: i64,
        limit: i32,
    ) -> Result<Vec<Message>, AppError> {
        let limit = limit.clamp(1, MAX_PAGE);
        self.repo
            .list_after_sequence(conversation_id, after_sequence, limit)
            .await
    }
}

#[cfg(test)]
mod tests {
    //! MessageService 单元测试 — 用 mockall 或直 trait
    //!
    //! MVP 留位:完整 mock 在 im-testkit crate 实现,本模块单元测试在 C-9
    //! (依赖 conversation_repo / message_repo / sequence / events 4 个 Arc<dyn Trait>
    //! 难以 in-process 全部 mock,改在 tests/message_service_test.rs 集成测试覆盖)

    use super::*;

    #[test]
    fn send_message_command_field_set() {
        let cmd = SendMessageCommand {
            conversation_id: ConversationId::new(),
            sender_id: UserId::new(),
            idempotency_key: "idem-1".into(),
            kind: "text".into(),
            content: serde_json::json!({"text": "hi"}),
            reply_to: None,
            max_size_bytes: 65536,
        };
        assert_eq!(cmd.idempotency_key, "idem-1");
        assert_eq!(cmd.kind, "text");
        assert_eq!(cmd.max_size_bytes, 65536);
    }
}
