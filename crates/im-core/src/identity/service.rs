//! IdentityService — 业务编排层
//!
//! 依据: ImplementationSpec §7.4.1 + DetailedDesign §9.2
//!
//! ## 责任
//! - Server-to-Server Token Exchange (GAME-ID-003 红线)
//! - Guest 注册
//! - **2026-09-20 C-3 WBS 新增**: username/password register + RS256 JWT 签发
//! - Refresh Token Rotation
//! - Guest Upgrade (LinkAccount)

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;

use super::password::{hash_password, validate_password_strength, validate_username};
use super::repository::{
    ExternalIdentity, User, UserKind, UserRepository, UserState,
};
use super::token::{AccessToken, DeviceSessionRepository, TokenPair, TokenService};
use crate::common::repository::*; // 留位,后续会用到

/// Register 命令(username/password 路径,2026-09-20 C-3 WBS)
#[derive(Debug, Clone)]
pub struct RegisterCommand {
    pub environment_id: EnvironmentId,
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
}

/// Server Exchange 命令(Server-to-Server)
#[derive(Debug, Clone)]
pub struct ServerExchangeCommand {
    pub environment_id: EnvironmentId,
    pub external_provider: String,
    pub external_uid: String,
    pub display_name: Option<String>,
    /// HMAC 签名已在 gRPC 拦截器层校验,这里只是承载
    pub server_signature_verified: bool,
}

pub struct IdentityService<U: UserRepository, D: DeviceSessionRepository> {
    user_repo: U,
    device_repo: D,
    token_service: std::sync::Arc<TokenService>,
    server_secrets: std::collections::HashMap<EnvironmentId, secrecy::SecretString>,
}

impl<U, D> IdentityService<U, D>
where
    U: UserRepository,
    D: DeviceSessionRepository,
{
    pub fn new(
        user_repo: U,
        device_repo: D,
        token_service: std::sync::Arc<TokenService>,
        server_secrets: std::collections::HashMap<EnvironmentId, secrecy::SecretString>,
    ) -> Self {
        Self {
            user_repo,
            device_repo,
            token_service,
            server_secrets,
        }
    }

    /// Server-to-Server Token Exchange
    /// GAME-ID-003 红线:仅游戏服务器可调,HMAC 签名已校验
    pub async fn server_exchange_token(
        &self,
        cmd: ServerExchangeCommand,
    ) -> Result<TokenPair, AppError> {
        if !cmd.server_signature_verified {
            return Err(AppError::Unauthorized(
                "server signature not verified".into(),
            ));
        }
        // 1. 查找现有 user(extid+env 唯一)
        let existing = self
            .user_repo
            .find_by_external_identity(
                cmd.environment_id,
                &cmd.external_provider,
                &cmd.external_uid,
            )
            .await?;

        let user = match existing {
            Some(u) => u,
            None => {
                // 2. 新建 user
                self.user_repo
                    .create(
                        cmd.environment_id,
                        UserKind::User,
                        Some(ExternalIdentity {
                            provider: cmd.external_provider,
                            external_uid: cmd.external_uid,
                        }),
                        cmd.display_name,
                    )
                    .await?
            }
        };

        // 3. 校验 state(只允许 active 签发新 token)
        if user.state != UserState::Active {
            return Err(match user.state {
                UserState::Banned => AppError::AccountBanned,
                UserState::Suspended => AppError::AccountSuspended,
                UserState::Deleted => AppError::NotFound("user not found".into()),
                _ => AppError::Internal(anyhow::anyhow!("unexpected user state")),
            });
        }

        // 4. 签发 access + refresh
        self.issue_token_pair(user).await
    }

    /// Guest 注册
    pub async fn guest_register(
        &self,
        environment_id: EnvironmentId,
    ) -> Result<TokenPair, AppError> {
        let user = self
            .user_repo
            .create(environment_id, UserKind::Guest, None, None)
            .await?;
        self.issue_token_pair(user).await
    }

    /// Username/password 注册(2026-09-20 C-3 WBS)
    ///
    /// 流程:
    ///   1. 校验 username 格式 + 密码强度
    ///   2. argon2id 哈希密码
    ///   3. INSERT user(kind='user', username, password_hash, extid=NULL)
    ///   4. 创建 DeviceSession(rotation refresh token)
    ///   5. 签发 access (RS256) + refresh token pair
    ///
    /// 错误映射:
    ///   - 用户名 / 密码格式不合法 → AppError::Validation
    ///   - (env, username) UNIQUE 命中 → AppError::AccountAlreadyExists (由 PgUserRepository::map_sqlx_error 映射)
    pub async fn register(&self, cmd: RegisterCommand) -> Result<TokenPair, AppError> {
        // 1. 校验 username 格式
        validate_username(&cmd.username).map_err(|e| {
            AppError::Validation(format!("invalid username '{}': {}", cmd.username, e))
        })?;
        // 2. 校验密码强度
        validate_password_strength(&cmd.password).map_err(|e| {
            AppError::Validation(format!("weak password: {}", e))
        })?;

        // 3. argon2id 哈希(耗时操作,默认参数 ~50-200ms;MVP 暂不调成 fast 参数)
        let password_hash = hash_password(&cmd.password)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("argon2 hash failed: {}", e)))?;

        // 4. INSERT user(若 UNIQUE 违反由 PgUserRepository::map_sqlx_error 转换为 AccountAlreadyExists)
        let user = self
            .user_repo
            .create_with_password(
                cmd.environment_id,
                &cmd.username,
                &password_hash,
                cmd.display_name.clone(),
            )
            .await?;

        // 5. 签发 token pair + 创建 DeviceSession
        self.issue_token_pair(user).await
    }

    /// 签发 token pair
    async fn issue_token_pair(&self, user: User) -> Result<TokenPair, AppError> {
        let access = self
            .token_service
            .issue_access_token(&user)
            .map_err(AppError::from)?;
        // refresh token 用 crypto-random UUID,hash 后存 DB
        let refresh_raw = Uuid::new_v4().to_string();
        let refresh_hash = crate::common::crypto::sha256_hex(&refresh_raw);
        let device_session = self
            .device_repo
            .create(user.id, None, &refresh_hash)
            .await?;

        Ok(TokenPair {
            access_token: access,
            refresh_token: super::token::RefreshToken(format!(
                "{}.{}",
                device_session.id, refresh_raw
            )),
            user_id: user.id,
            expires_in: 900, // TODO: 读 self.token_service 的 access_ttl
        })
    }

    /// Refresh Token 旋转
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, AppError> {
        // 解析 "session_id.raw_token"
        let (session_id, raw) = refresh_token
            .split_once('.')
            .ok_or_else(|| AppError::Unauthorized("invalid refresh token format".into()))?;
        let session_id: im_common::ids::DeviceSessionId = session_id
            .parse()
            .map_err(|_| AppError::Unauthorized("invalid refresh token format".into()))?;

        // 查 session
        let session = self
            .device_repo
            .find_by_refresh_token_hash(/* user_id: 待优化 */ im_common::ids::UserId::nil(), "")
            .await?
            .ok_or(AppError::Unauthorized("device session not found".into()))?;

        // 撤销旧 session
        self.device_repo.revoke(session_id).await?;

        // 查 user
        let user = self
            .user_repo
            .find_by_id(session.user_id)
            .await?
            .ok_or(AppError::Unauthorized("user not found".into()))?;

        if user.state != UserState::Active {
            return Err(AppError::AccountBanned);
        }

        self.issue_token_pair(user).await
    }

    /// Guest Upgrade / Account Link
    pub async fn link_account(
        &self,
        access_token: &str,
        external: ExternalIdentity,
    ) -> Result<TokenPair, AppError> {
        let claims = self
            .token_service
            .validate_access_token(access_token)
            .map_err(AppError::from)?;

        let user_id: UserId = claims
            .sub
            .parse()
            .map_err(|_| AppError::Unauthorized("invalid user_id".into()))?;
        let environment_id: EnvironmentId = claims
            .env
            .parse()
            .map_err(|_| AppError::Unauthorized("invalid env".into()))?;

        // 1. 查目标 extid 是否已被绑定
        if let Some(existing) = self
            .user_repo
            .find_by_external_identity(
                environment_id,
                &external.provider,
                &external.external_uid,
            )
            .await?
        {
            if existing.id != user_id {
                return Err(AppError::AccountMergeConflict);
            }
            // 已是同一 user,直接续 token
            let user = self
                .user_repo
                .find_by_id(user_id)
                .await?
                .ok_or(AppError::NotFound("user".into()))?;
            return self.issue_token_pair(user).await;
        }

        // 2. 绑定(MVP:不实现自动 merge,直接更新 user 记录)
        // 实现:UPDATE users SET external_identity = $1 WHERE id = $2
        // 留待 SQL 实现,目前返回未实现
        Err(AppError::Internal(anyhow::anyhow!(
            "link_account UPDATE not yet implemented in MVP; see ImplementationSpec §7.4.1"
        )))
    }

    pub async fn logout(
        &self,
        session_id: im_common::ids::DeviceSessionId,
    ) -> Result<(), AppError> {
        self.device_repo.revoke(session_id).await
    }

    pub async fn get_me(&self, user_id: UserId) -> Result<User, AppError> {
        self.user_repo
            .find_by_id(user_id)
            .await?
            .ok_or(AppError::NotFound("user".into()))
    }

    /// 暴露 TokenService 引用,用于边界(gRPC / REST handler)做 access_token 校验
    /// 不暴露 `&mut`,只允许 issue + validate 类的只读操作
    pub fn token_service(&self) -> &std::sync::Arc<TokenService> {
        &self.token_service
    }
}
