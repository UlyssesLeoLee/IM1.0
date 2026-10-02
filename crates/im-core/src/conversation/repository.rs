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

    async fn find_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError>;

    async fn find_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, AppError>;

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
    /// `PgConversationRepository::list_for_user` 目前的实现有两个对该用途致命的特点:
    /// 1. `cursor` 参数**被完全忽略**(形参名 `_cursor`, SQL 里没有 OFFSET) ——
    ///    所以「翻页取完」这条路根本不存在;
    /// 2. `ConversationService::list_user_conversations` 把 limit clamp 到
    ///    1..=50, 而 SQL 又 `LIMIT $2`。
    ///
    /// 于是复用它的后果是: **用户加入超过 50 个会话时, 超出部分静默丢失**。
    /// 对 UI 列表, 丢掉的只是「更早的会话还能再翻」; 对广播过滤, 丢掉的
    /// 是「这些会话的实时消息一条都收不到」—— 用户不会看到任何报错, 只会
    /// 以为对方没发言。故单列一个方法。
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

    async fn list_members(&self, conv: ConversationId)
        -> Result<Vec<ConversationMember>, AppError>;
}
