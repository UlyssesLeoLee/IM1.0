//! Conversation Repository 接口
//!
//! 依据: ImplementationSpec §7.4.2 + DetailedDesign §9.3
//!
//! 2026-09-20 升级 (C-8 WBS):
//! - 新增 `upsert_dm_pair` 用于创建 DM 时一次性建 dm_pairs 行(由 DB UNIQUE 约束保证唯一性)
//! - 新增 `count_owners` 用于 leave race 检测(不能删到没有 owner)
//! - 新增 `find_dm_in_tx` / `create_dm_in_tx` 让 Service 在单事务内完成"查 → 建 conv → 建 dm_pairs → 加成员"
//!   防止多并发 create_dm(a,b) 时短暂存在双 conv 或 dm_pairs 缺行

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

/// 规范化 dm_pairs 的 (user_a, user_b) 顺序 — CHECK(user_a < user_b) 要求 a<b
#[inline]
pub fn normalize_dm_pair(user_a: UserId, user_b: UserId) -> (UserId, UserId) {
    if user_a.0 < user_b.0 {
        (user_a, user_b)
    } else {
        (user_b, user_a)
    }
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    /// 创建空会话(不含成员,不含 dm_pairs)
    async fn create(
        &self,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: Value,
    ) -> Result<Conversation, AppError>;

    /// 在事务内创建空会话(给 Service 的 create_dm_in_tx 用)
    async fn create_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: Value,
    ) -> Result<Conversation, AppError>;

    /// 查 DM 会话(归一化参数,内部按 user_a<user_b 查 dm_pairs)
    async fn find_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError>;

    /// 在事务内查 DM(给 create_dm_in_tx 串行化并发竞争用)
    async fn find_dm_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError>;

    /// 幂等 upsert dm_pairs 行(ON CONFLICT DO NOTHING) — 创建 DM 时调用
    /// 返回 (是否新插入, 关联的 conversation_id)
    async fn upsert_dm_pair(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
        conversation_id: ConversationId,
    ) -> Result<DmPairUpsertResult, AppError>;

    /// 查 conversation;不存在返回 None
    async fn find_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, AppError>;

    /// 列出某 user 参与的所有会话
    async fn list_for_user(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError>;

    /// 在事务内 add_member
    async fn add_member_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        conv: ConversationId,
        user: UserId,
        role: MemberRole,
    ) -> Result<(), AppError>;

    /// 非事务 add_member — 给 Service 公开方法用(本身 ON CONFLICT 幂等)
    async fn add_member(
        &self,
        conv: ConversationId,
        user: UserId,
        role: MemberRole,
    ) -> Result<(), AppError>;

    /// 删除成员 — 返回删除的行数(0=本来就不在)
    async fn remove_member(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<u64, AppError>;

    /// 查是否成员
    async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, AppError>;

    /// 列出所有成员
    async fn list_members(
        &self,
        conv: ConversationId,
    ) -> Result<Vec<ConversationMember>, AppError>;

    /// 数 owner 数(给 leave race 检测:owner 不能直接 leave,必须先 transfer)
    async fn count_owners(&self, conv: ConversationId) -> Result<i64, AppError>;

    /// 查某 member 的 role(给 Service 决定是否能 leave/transfer)
    async fn get_member_role(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<Option<MemberRole>, AppError>;

    /// 暴露 Pool 给 Service 创建事务
    fn pool(&self) -> &PgPool;
}

use sqlx::PgPool;

/// dm_pairs upsert 返回值
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmPairUpsertResult {
    /// 是否本调用新建(否则表示已存在,可能由并发 create_dm 抢先)
    pub inserted: bool,
    pub conversation_id: ConversationId,
}
