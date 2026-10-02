//! ReactionService — 添加 / 移除消息表情回应
//!
//! 依据: aux-13 §1.1.5(`react` 入站) + aux-02 §F.13(`message_reactions` 表)
//!       aux-05 §CRC card `react(user, emoji)`
//!
//! ## 为什么要单独一个 service(而不是塞进 `MessageService`)
//!
//! reaction 有自己的表(§F.13)与自己的仓储(`ReactionRepository`)。塞进
//! `MessageService` 会让它多一个构造参数, 而 `MessageService::new` 已有 4 个
//! 调用点(`main.rs` + 3 个测试) —— 为一个无关的子域改动既有构造签名, 会把
//! 无关的 diff 混进来。反应自己的 service 也让「谁能对谁的消息加表情」这条
//! 权限规则有一个明确的归属。
//!
//! ## 权限: **必须**是消息所属会话的成员
//!
//! 这是本 service 存在的**全部理由**。`ReactionRepository::add` 自身不做任何
//! 校验(它只管按 PK 幂等插入), 于是若直接暴露它, 任何持有任意 `message_id`
//! 的用户都能对**任何**会话里的任何消息加表情。校验链:
//! 1. 消息存在 → 否则 `MessageNotFound`
//! 2. `messages.conversation_id` 所属会话里, 该 user 是成员 → 否则 `Forbidden`
//! 3. emoji 非空

use std::sync::Arc;

use im_common::ids::{MessageId, UserId};
use im_common::AppError;

use super::repository::{Reaction, ReactionRepository};
use crate::conversation::repository::ConversationRepository;
use crate::message::repository::MessageRepository;

/// emoji 的合理上限 —— 挡住「把整篇文章塞进 emoji 列」这类输入
///
/// 表上没有 CHECK, 所以这道校验只在这里。取值宽松(足够放下最长的 ZWJ 序列
/// emoji + 若干修饰符), 不做 Unicode 规范化 —— 规范化会把不同序列合并成同一
/// 个 reaction, 那是产品决策, 不是实现能替的。
const MAX_EMOJI_BYTES: usize = 64;

pub struct ReactionService {
    reactions: Arc<dyn ReactionRepository>,
    messages: Arc<dyn MessageRepository>,
    conversations: Arc<dyn ConversationRepository>,
}

impl ReactionService {
    pub fn new(
        reactions: Arc<dyn ReactionRepository>,
        messages: Arc<dyn MessageRepository>,
        conversations: Arc<dyn ConversationRepository>,
    ) -> Self {
        Self {
            reactions,
            messages,
            conversations,
        }
    }

    /// 添加 reaction(幂等 —— 同一 `(msg, user, emoji)` 重复添加视为成功)
    ///
    /// 返回 `(Reaction, bool)`: 第二个值 `true` 表示**这次真的插入了新行**,
    /// `false` 表示「本来就有」(幂等重放)。调用方据此决定要不要广播 ——
    /// 对一条已存在的 reaction 再广播一次, 会让所有在线端看到重复的表情动画。
    pub async fn add_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<(Reaction, bool), AppError> {
        // 1. emoji 先判: 便宜的校验放最前, 避免为一个空字符串查两次库
        if emoji.trim().is_empty() {
            return Err(AppError::Validation("emoji must not be empty".into()));
        }
        if emoji.len() > MAX_EMOJI_BYTES {
            return Err(AppError::Validation(format!(
                "emoji too long: {} bytes (max {MAX_EMOJI_BYTES})",
                emoji.len()
            )));
        }

        // 2. 消息必须存在 —— 顺带拿到 conversation_id
        let msg = self
            .messages
            .find_by_id(message_id)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;

        // 3. **权限边界**: 必须是该消息所属会话的成员
        //
        // 顺序上这一步在插入之前 —— 若放在之后, 非成员的反应虽然最终会被
        // 成员检查拦下报错, 但**行已经写进去了**(除非显式回滚)。校验必须
        // 先于任何写。
        if !self
            .conversations
            .is_member(msg.conversation_id, user_id)
            .await?
        {
            return Err(AppError::Forbidden("not a conversation member".into()));
        }

        // 4. 插入。`add` 自身幂等(PK + ON CONFLICT DO NOTHING), 但它**不告诉
        //    调用方**是自己插的还是回查到的 —— 而广播需要这个区分, 故先查一次
        //    存在性再插。多一次查询换「不重复广播」, 对 reaction 这种低频操作
        //    是划算的。
        let already = self
            .reactions
            .list_for_message(message_id)
            .await?
            .iter()
            .any(|r| r.user_id == user_id && r.emoji == emoji);

        let reaction = self.reactions.add(message_id, user_id, emoji).await?;
        Ok((reaction, !already))
    }

    /// 移除 reaction(幂等 —— 不存在也视为成功, 返回 `false`)
    pub async fn remove_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<bool, AppError> {
        // 与 add 同理: 移除同样要校验成员, 否则任意用户能删别人的表情
        let msg = self
            .messages
            .find_by_id(message_id)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;
        if !self
            .conversations
            .is_member(msg.conversation_id, user_id)
            .await?
        {
            return Err(AppError::Forbidden("not a conversation member".into()));
        }
        self.reactions.remove(message_id, user_id, emoji).await
    }

    /// 列某条消息的全部 reaction
    ///
    /// 只读列表**也**要校验成员: 否则一个非成员能凭 `message_id` 枚举出
    /// 「谁对这条消息点了什么表情」—— 那是会话内的用户行为信息, 属于泄漏。
    pub async fn list_reactions(
        &self,
        message_id: MessageId,
        user_id: UserId,
    ) -> Result<Vec<Reaction>, AppError> {
        let msg = self
            .messages
            .find_by_id(message_id)
            .await?
            .ok_or(AppError::MessageNotFound(message_id.0))?;
        if !self
            .conversations
            .is_member(msg.conversation_id, user_id)
            .await?
        {
            return Err(AppError::Forbidden("not a conversation member".into()));
        }
        self.reactions.list_for_message(message_id).await
    }
}
