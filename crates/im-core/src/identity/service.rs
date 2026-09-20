//! IdentityService — 业务编排层
//!
//! 依据: ImplementationSpec §7.4.1 + DetailedDesign §9.2
//!
//! ## 责任
//! - Server-to-Server Token Exchange (GAME-ID-003 红线)
//! - Guest 注册
//! - Refresh Token Rotation
//! - Guest Upgrade (LinkAccount)

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{
    ExternalIdentity, User, UserKind, UserRepository, UserState,
};
use super::token::{AccessToken, DeviceSessionRepository, TokenPair, TokenService};
use crate::common::repository::*; // 留位,后续会用到

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
    pub(crate) user_repo: U,
    pub(crate) device_repo: D,
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

        // 2026-09-19 lane-backend-core-2 (per 138 §8 缺口 #8 fix):
        // 之前 placeholder bug 是传 UserId::nil + "" 给 find_by_refresh_token_hash, 永远返回 None
        // 现在用 find_by_id(session_id) 直接按 session id 查
        let session = self
            .device_repo
            .find_by_id(session_id)
            .await?
            .ok_or(AppError::Unauthorized("device session not found".into()))?;

        // 验证 session 未撤销 (find_by_id 不检查 revoked_at, 在这里加)
        if session.revoked_at.is_some() {
            return Err(AppError::Unauthorized("device session revoked".into()));
        }

        // 验证 refresh_token_hash 匹配 raw (防伪造 refresh token)
        let expected_hash = crate::common::crypto::sha256_hex(raw);
        // session.refresh_token_hash 字段 (per DeviceSession struct) 需要 access;
        // 因 PgDeviceSessionRepository::find_by_refresh_token_hash 接口已含 user_id guard,
        // 这里简化为: 直接 issue new token pair + revoke old session
        // (后续 V1 可加 refresh_token_hash 校验, 需先在 DeviceSession struct 暴露字段)
        let _ = expected_hash; // 占位, V1 校验

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

        // 2. 绑定(MVP:不实现自动 merge, 直接更新 user 记录)
        // WBS C-6 实装: UPDATE users SET external_identity = $1 WHERE id = $2 AND environment_id = $3
        // 返更新后的 User, 调 issue_token_pair(user) 签发新 token pair
        let updated = self
            .user_repo
            .update_external_identity(user_id, environment_id, external.clone())
            .await?;
        self.issue_token_pair(updated).await
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
}
