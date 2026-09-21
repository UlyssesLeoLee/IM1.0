//! Identity Repository 接口
//!
//! 依据: ImplementationSpec §7.4.1 + DetailedDesign §9.2

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use im_common::ids::{DeviceSessionId, EnvironmentId, UserId};
use im_common::AppError;

use super::token::DeviceSession;

/// 用户类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserKind {
    User,
    Guest,
}

/// 用户状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserState {
    Active,
    Banned,
    Suspended,
    Deleted,
}

/// 外部身份(游戏服务器 Token Exchange 用)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalIdentity {
    pub provider: String,         // steam | xbox | psn | epic | custom_jwt | ...
    pub external_uid: String,
}

/// User 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub environment_id: EnvironmentId,
    pub kind: UserKind,
    pub external_identity: Option<ExternalIdentity>,
    pub state: UserState,
    pub display_name: Option<String>,
    /// 2026-09-21 C-3 + C-4 整合: username/password 登录凭证
    /// NULL 表示该 user 走 extid/OAuth 路径 (无密码登录)
    pub username: Option<String>,
    /// argon2id PHC-format 哈希字符串;NULL 表示无密码登录路径
    pub password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// User Repository trait
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, AppError>;
    async fn find_by_external_identity(
        &self,
        env: EnvironmentId,
        provider: &str,
        external_uid: &str,
    ) -> Result<Option<User>, AppError>;
    async fn create(
        &self,
        env: EnvironmentId,
        kind: UserKind,
        external: Option<ExternalIdentity>,
        display_name: Option<String>,
    ) -> Result<User, AppError>;
    async fn update_state(&self, id: UserId, state: UserState) -> Result<(), AppError>;
    async fn update_display_name(
        &self,
        id: UserId,
        display_name: Option<&str>,
    ) -> Result<User, AppError>;
    /// WBS C-6 link_account SQL 实装: 把 external_identity UPDATE 到 user 记录
    /// (MVP: 仅 external_identity, V1 加 external_linked_at 时间戳需新 migration)
    /// 返更新后的 User (供 service 调 issue_token_pair)
    ///
    /// 绑定外部身份 (Guest 升级 / Account Link):
    /// - 将 `kind` 从 `guest` 升级为 `user`,并写入 `external_identity`
    /// - 若同 `(environment_id, external_identity)` 已被其它 user 占用,返回
    ///   `AppError::Conflict` (映射 SQL `uniq_users_env_extid` 触发)
    /// - 若 user 已被 ban/suspend,返回 `AppError::NotFound` (不暴露存在性)
    /// - 不允许对 `kind='user'` 的记录二次绑定 (返回 `AppError::Validation`)
    async fn update_external_identity(
        &self,
        id: UserId,
        env: EnvironmentId,
        external: ExternalIdentity,
    ) -> Result<User, AppError>;

    // ============================================================================
    // 2026-09-21 整合 (C-3 + C-4): username/password 路径
    // ============================================================================

    /// C-4 WBS (ULYS-145): 按 (env, username) 查 user — 用于 authenticate
    /// 返回 None 表示用户不存在(由 caller 决定是否映射为 generic Unauthorized 防 enumeration)
    async fn find_by_username(
        &self,
        env: EnvironmentId,
        username: &str,
    ) -> Result<Option<User>, AppError>;

    /// C-3 WBS (ULYS-144): 创建带 username + password_hash 的 user
    /// dup username 由 PG UNIQUE 约束拦截 → map_sqlx_error → AppError::AccountAlreadyExists
    async fn create_with_password(
        &self,
        env: EnvironmentId,
        username: &str,
        password_hash: &str,
        display_name: Option<String>,
    ) -> Result<User, AppError>;
}

/// Device Session Repository trait (在 token.rs 中定义以避免循环)
pub use super::token::DeviceSessionRepository;
