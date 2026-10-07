//! Conversation Repository 接口
//!
//! 依据: ImplementationSpec §7.4.2

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationKind {
    Dm,
    Group,
    Channel,
    System,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub environment_id: EnvironmentId,
    pub kind: ConversationKind,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMember {
    pub conversation_id: ConversationId,
    pub user_id: UserId,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
    pub last_read_sequence: i64,
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn create(
        &self,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: Value,
    ) -> Result<Conversation, AppError>;

    /// 把 DM 会话登记进 `dm_pairs`(per aux-02 §F.10)。
    ///
    /// 返回 `true` = 本次登记了新行; `false` = 该 `(env, user_a, user_b)`
    /// 已经有行了(并发下别人先到)。
    ///
    /// ## 为什么必须单列一个方法, 不能塞进 `create` 或 `add_member`
    ///
    /// `find_dm` 的幂等短路是 `INNER JOIN dm_pairs` —— **而 `dm_pairs` 在
    /// 2026-10-07 之前从不被任何生产代码写入**(全仓仅有的两处 INSERT 都在
    /// 测试里, 其中一处还写着「模拟 `create_dm`:插 dm_pairs 行」)。结果:
    /// 短路永远命中不了, **每次 `create_dm` 都新建一个会话 + 两个成员**。
    ///
    /// 这不是理论问题: `POST /v1/conversations/dm` 直接调它, 客户端任何一次
    /// 重试(超时 / 双击)都会多出一个重复 DM。
    ///
    /// 由 aux-06 §D.2 的 A-005 基准当场抓到(见 `docs/gap-ledger.md` §1.41)。
    async fn link_dm_pair(
        &self,
        env: EnvironmentId,
        conversation_id: ConversationId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<bool, AppError>;

    async fn find_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError>;

    async fn find_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, AppError>;

    /// 用户参与的会话列表, **按 `(created_at DESC, id DESC)` 排序的 keyset 分页**
    ///
    /// `cursor` 由 `crate::conversation::cursor::encode` 生成, 解析失败返
    /// `Validation`(400)。**实现方必须真的把它下推到 SQL** —— 忽略游标
    /// 会让接入方拿到无限重复的第 1 页, 而这正是 2026-10-08 修掉的 bug。
    async fn list_for_user(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError>;

    /// 列出用户所属的**全部**会话 id —— 不分页、不排序、**无上限**。
    ///
    /// ## 为什么不复用 `list_for_user`
    ///
    /// 两者是**不同用途**, 不是同一件事的两种写法:
    /// - `list_for_user` 服务于 UI 列表: 要完整 `Conversation` 行、要排序、要分页。
    /// - 本方法服务于 **WS 广播成员过滤**: 只需要一个 id 集合, 且**一个都不能少**。
    ///
    /// `PgConversationRepository::list_for_user` 对该用途仍有两个致命特点:
    /// 1. 有 `limit` 上限(1..=50) —— 超出部分静默丢失;
    /// 2. 游标一旦出错会整页失败。
    ///
    /// 于是复用它的后果是: **用户加入超过 50 个会话时, 超出部分静默丢失**。
    /// 对 UI 列表, 丢掉的只是「更早的会话还能再翻」; 对广播过滤, 丢掉的
    /// 是「这些会话的实时消息一条都收不到」—— 用户不会看到任何报错, 只会
    /// 以为对方没发言。故单列一个方法。
    ///
    /// 2026-10-08 更新: 原文把第 1 条写成「`cursor` 被完全忽略(形参名
    /// `_cursor`, SQL 里没有 OFFSET), 所以翻页取完这条路根本不存在」。
    /// 游标已实装, 但「有上限」这一条依然成立, 且它本身就是该方法存在的理由。
    async fn list_all_memberships_for_user(
        &self,
        user: UserId,
    ) -> Result<Vec<ConversationId>, AppError>;

    async fn add_member(
        &self,
        conv: ConversationId,
        user: UserId,
        role: MemberRole,
    ) -> Result<(), AppError>;

    async fn remove_member(&self, conv: ConversationId, user: UserId) -> Result<(), AppError>;

    async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, AppError>;

    /// 推进 `last_read_sequence`, 返回**是否真的推进了**
    ///
    /// ## 为什么不用 `GREATEST(last_read_sequence, $1)`
    ///
    /// `aux-07` §H.4 与 `aux-08` §幂等性 都建议写成
    /// `SET last_read_sequence = GREATEST(last_read_sequence, $1)`。那个写法能
    /// 保证单调, 但**达不到 `aux-04 §C.4` 自己写的目标** —— 那张表的
    /// 「`mark_read` 高频 | 写放大」一行要求「仅在 `new_sequence >
    /// last_read_sequence` 时 UPDATE, **减少 80% 写**」。
    ///
    /// 原因是 PG 的 UPDATE 即使把列写成**同样的值**, 仍会产生新的行版本(tuple
    /// version) —— 这正是 MVCC 的代价。所以 `GREATEST` 在「客户端重复上报同一个
    /// sequence」(极常见: 每次收到新消息都上报当前最大 seq, 而多条消息的 seq
    /// 可能重复上报)这种情形下**一次都没省下**。
    ///
    /// 把守卫放进 `WHERE` 才真正跳过写: 值没变时该行不匹配, 0 行受影响。
    /// 单调性由同一个 `WHERE` 保证, 不需要 `GREATEST`。
    ///
    /// `Ok(false)` 表示「已是该 sequence 或更靠后, 未推进」—— 这是**正常**
    /// (幂等重放), 不是错误。调用方若需区分「未推进」与「不是成员」, 须先查
    /// 成员关系(本方法无法区分: 两者都是 0 行)。
    async fn advance_last_read_sequence(
        &self,
        conv: ConversationId,
        user: UserId,
        sequence: i64,
    ) -> Result<bool, AppError>;

    /// 该会话当前**已分配**的最大 message sequence(即 `next_sequence - 1`)
    ///
    /// 从未分配过 sequence 的会话返回 `0`。
    ///
    /// ## 存在的理由: `mark_read` 的上界夹紧
    ///
    /// `advance_last_read_sequence` 是 `SET last_read_sequence = $1`, 客户端
    /// 传什么就写什么。传 `i64::MAX`(「全部标记已读」的一种自然写法)会把读
    /// 指针永久顶到极大值 —— 此后该会话任何 `sequence` 都不再推进, 未读数永久
    /// 失真, 且**没有任何报错**。这个值是**服务端的事实**, 客户端无从得知,
    /// 所以必须由服务端提供。
    ///
    /// 存在 `conversation_sequences` 里(与 `SequenceAllocator::next` 同一张表),
    /// 故这是一次极轻的单行查询。
    async fn max_allocated_sequence(&self, conv: ConversationId) -> Result<i64, AppError>;

    async fn list_members(&self, conv: ConversationId)
        -> Result<Vec<ConversationMember>, AppError>;
}
