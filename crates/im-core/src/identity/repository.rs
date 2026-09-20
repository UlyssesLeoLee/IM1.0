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
///
/// 2026-09-20 C-3 WBS: 新增 `username` / `password_hash` 字段,用于 username/password
/// 注册登录路径(与原 extid 路径并存)。NULL 表示该 user 不走密码登录(纯 extid 或 Guest)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub environment_id: EnvironmentId,
    pub kind: UserKind,
    pub external_identity: Option<ExternalIdentity>,
    pub state: UserState,
    pub display_name: Option<String>,
    pub username: Option<String>,
    /// argon2id PHC-format 哈希字符串(含参数 + salt + hash);NULL 表示无密码登录路径
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
    /// 2026-09-20 C-3 WBS: 按 (env, username) 查找 user(用于 register 重复检查 / login)
    async fn find_by_username(
        &self,
        env: EnvironmentId,
        username: &str,
    ) -> Result<Option<User>, AppError>;
    async fn create(
        &self,
        env: EnvironmentId,
        kind: UserKind,
        external: Option<ExternalIdentity>,
        display_name: Option<String>,
    ) -> Result<User, AppError>;
    /// 2026-09-20 C-3 WBS: 注册时使用,创建带 username + password_hash 的 user
    async fn create_with_password(
        &self,
        env: EnvironmentId,
        username: &str,
        password_hash: &str,
        display_name: Option<String>,
    ) -> Result<User, AppError>;
    async fn update_state(&self, id: UserId, state: UserState) -> Result<(), AppError>;
    async fn update_display_name(
        &self,
        id: UserId,
        display_name: Option<&str>,
    ) -> Result<User, AppError>;
}

/// Device Session Repository trait (在 token.rs 中定义以避免循环)
pub use super::token::DeviceSessionRepository;
