//! PgUserRepository + PgDeviceSessionRepository — PostgreSQL 实现
//!
//! 依据: aux-02 §F.4 users / §F.5 device_sessions
//!       IdentityRepository trait
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use im_common::ids::{DeviceSessionId, EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{ExternalIdentity, User, UserKind, UserRepository, UserState};
use super::token::{DeviceSession, DeviceSessionRepository};

// ============================================================================
// PgUserRepository
// ============================================================================

/// PostgreSQL 实现
#[derive(Clone)]
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, AppError> {
        let row: Option<UserRow> = sqlx::query_as(
            r#"
            SELECT id, environment_id, kind, external_identity, state, display_name, username, password_hash, created_at
            FROM users WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(UserRow::into_user))
    }

    async fn find_by_external_identity(
        &self,
        env: EnvironmentId,
        provider: &str,
        external_uid: &str,
    ) -> Result<Option<User>, AppError> {
        // external_identity JSONB 结构: { "provider": "...", "external_uid": "..." }
        // 用 jsonb path 查询命中
        let row: Option<UserRow> = sqlx::query_as(
            r#"
            SELECT id, environment_id, kind, external_identity, state, display_name, username, password_hash, created_at
            FROM users
            WHERE environment_id = $1
              AND external_identity IS NOT NULL
              AND external_identity->>'provider' = $2
              AND external_identity->>'external_uid' = $3
            "#,
        )
        .bind(env.0)
        .bind(provider)
        .bind(external_uid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(UserRow::into_user))
    }

    async fn find_by_username(
        &self,
        env: EnvironmentId,
        username: &str,
    ) -> Result<Option<User>, AppError> {
        // username 查询:仅匹配 password-login 用户(external_identity=NULL, username 非 NULL)
        // 注意:Guest 用户 username=NULL,会被 WHERE 过滤掉(符合预期)
        let row: Option<UserRow> = sqlx::query_as(
            r#"
            SELECT id, environment_id, kind, external_identity, state, display_name, username, password_hash, created_at
            FROM users
            WHERE environment_id = $1 AND username = $2
            "#,
        )
        .bind(env.0)
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(UserRow::into_user))
    }

    async fn create(
        &self,
        env: EnvironmentId,
        kind: UserKind,
        external: Option<ExternalIdentity>,
        display_name: Option<String>,
    ) -> Result<User, AppError> {
        // Guest 强制 external=NULL(aux-02 §F.4 + P1-2 自审)
        // User 强制 external 非 NULL
        let ext_json: Option<JsonValue> = match (kind, external) {
            (UserKind::Guest, _) => None, // Guest 强制 extid NULL
            (UserKind::User, Some(e)) => Some(serde_json::json!({
                "provider": e.provider,
                "external_uid": e.external_uid,
            })),
            (UserKind::User, None) => {
                return Err(AppError::Validation(
                    "kind='user' requires external_identity".into(),
                ))
            }
        };
        let kind_str = match kind {
            UserKind::User => "user",
            UserKind::Guest => "guest",
        };

        let row: UserRow = sqlx::query_as(
            r#"
            INSERT INTO users (environment_id, kind, external_identity, display_name)
            VALUES ($1, $2, $3, $4)
            RETURNING id, environment_id, kind, external_identity, state, display_name, username, password_hash, created_at
            "#,
        )
        .bind(env.0)
        .bind(kind_str)
        .bind(ext_json)
        .bind(display_name)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.into_user())
    }

    async fn register_with_password(
        &self,
        env: EnvironmentId,
        username: &str,
        password_hash: &str,
        display_name: Option<String>,
    ) -> Result<User, AppError> {
        // 密码登录模式:
        //   kind='user', external_identity=NULL(per aux-02 §F.4:User 必填 external_identity)
        //   但密码登录模式无 external identity(直接 username+password 入场)
        // 设计选择:沿用 kind='user',external_identity=NULL(C-3 WBS ULYS-144 设计)
        //   username/password 作为该模式的主身份凭证
        // dup username → PG UNIQUE(environment_id, username) violation → map_sqlx_error
        let row: UserRow = sqlx::query_as(
            r#"
            INSERT INTO users (environment_id, kind, external_identity, display_name, username, password_hash)
            VALUES ($1, 'user', NULL, $2, $3, $4)
            RETURNING id, environment_id, kind, external_identity, state, display_name, username, password_hash, created_at
            "#,
        )
        .bind(env.0)
        .bind(display_name)
        .bind(username)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.into_user())
    }

    async fn update_state(&self, id: UserId, state: UserState) -> Result<(), AppError> {
        let state_str = match state {
            UserState::Active => "active",
            UserState::Banned => "banned",
            UserState::Suspended => "suspended",
            UserState::Deleted => "deleted",
        };
        let n = sqlx::query("UPDATE users SET state = $1 WHERE id = $2")
            .bind(state_str)
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?
            .rows_affected();
        if n == 0 {
            return Err(AppError::NotFound(format!("user {}", id.0)));
        }
        Ok(())
    }

    async fn update_display_name(
        &self,
        id: UserId,
        display_name: Option<&str>,
    ) -> Result<User, AppError> {
        let row: Option<UserRow> = sqlx::query_as(
            r#"
            UPDATE users SET display_name = $1
            WHERE id = $2
            RETURNING id, environment_id, kind, external_identity, state, display_name, username, password_hash, created_at
            "#,
        )
        .bind(display_name)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        row.map(UserRow::into_user).ok_or_else(|| AppError::NotFound(format!("user {}", id.0)))
    }
}

// ----------------------------------------------------------------------------
// UserRow + 映射
// ----------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    environment_id: Uuid,
    kind: String,
    external_identity: Option<JsonValue>,
    state: String,
    display_name: Option<String>,
    username: Option<String>,
    password_hash: Option<String>,
    created_at: DateTime<Utc>,
}

impl UserRow {
    fn into_user(self) -> User {
        let kind = match self.kind.as_str() {
            "guest" => UserKind::Guest,
            _ => UserKind::User,
        };
        let state = match self.state.as_str() {
            "banned" => UserState::Banned,
            "suspended" => UserState::Suspended,
            "deleted" => UserState::Deleted,
            _ => UserState::Active,
        };
        let external_identity = self.external_identity.and_then(|v| {
            let p = v.get("provider")?.as_str()?.to_string();
            let u = v.get("external_uid")?.as_str()?.to_string();
            Some(ExternalIdentity { provider: p, external_uid: u })
        });
        User {
            id: UserId(self.id),
            environment_id: EnvironmentId(self.environment_id),
            kind,
            external_identity,
            state,
            display_name: self.display_name,
            username: self.username,
            password_hash: self.password_hash,
            created_at: self.created_at,
        }
    }
}

// ============================================================================
// PgDeviceSessionRepository
// ============================================================================

#[derive(Clone)]
pub struct PgDeviceSessionRepository {
    pool: PgPool,
}

impl PgDeviceSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl DeviceSessionRepository for PgDeviceSessionRepository {
    async fn create(
        &self,
        user_id: UserId,
        device_fingerprint: Option<&str>,
        refresh_token_hash: &str,
    ) -> Result<DeviceSession, im_common::AppError> {
        let row: DeviceSessionRow = sqlx::query_as(
            r#"
            INSERT INTO device_sessions (user_id, device_fingerprint, refresh_token_hash)
            VALUES ($1, $2, $3)
            RETURNING id, user_id, device_fingerprint, created_at, revoked_at
            "#,
        )
        .bind(user_id.0)
        .bind(device_fingerprint)
        .bind(refresh_token_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.into_session())
    }

    async fn find_by_refresh_token_hash(
        &self,
        user_id: UserId,
        refresh_token_hash: &str,
    ) -> Result<Option<DeviceSession>, im_common::AppError> {
        // 注:实际 refresh token 校验需配合 user_id(防止 hash 跨用户撞库);
        // caller 必须传 user_id(从 JWT claims 拿),避免横向越权
        let row: Option<DeviceSessionRow> = sqlx::query_as(
            r#"
            SELECT id, user_id, device_fingerprint, created_at, revoked_at
            FROM device_sessions
            WHERE user_id = $1
              AND refresh_token_hash = $2
              AND revoked_at IS NULL
            "#,
        )
        .bind(user_id.0)
        .bind(refresh_token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.map(DeviceSessionRow::into_session))
    }

    async fn revoke(&self, id: DeviceSessionId) -> Result<(), im_common::AppError> {
        let n = sqlx::query(
            r#"
            UPDATE device_sessions SET revoked_at = now()
            WHERE id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .rows_affected();
        // revoke 已 revoked 的 session 视为幂等成功,不报错
        let _ = n;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct DeviceSessionRow {
    id: Uuid,
    user_id: Uuid,
    device_fingerprint: Option<String>,
    created_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

impl DeviceSessionRow {
    fn into_session(self) -> DeviceSession {
        DeviceSession {
            id: DeviceSessionId(self.id),
            user_id: UserId(self.user_id),
            device_fingerprint: self.device_fingerprint,
            created_at: self.created_at,
            revoked_at: self.revoked_at,
        }
    }
}

// ============================================================================
// sqlx -> AppError 映射
// ============================================================================

fn map_sqlx_error(e: sqlx::Error) -> AppError {
    match &e {
        // Unique violation → 业务层用 map_err 进一步映射
        // 这里保留 Internal,由 caller 决定语义
        sqlx::Error::RowNotFound => AppError::NotFound("row not found".into()),
        sqlx::Error::Database(db_err) => {
            // PG 23505 = unique_violation — 上层调用者(caller)负责映射到具体语义
            // 密码注册场景:register_with_password dup username → 上层转 AppError::Validation
            if db_err.code().as_deref() == Some("23505") {
                AppError::Internal(anyhow::anyhow!("unique violation: {}", db_err.message()))
            } else {
                AppError::Internal(anyhow::anyhow!("sqlx: {}", e))
            }
        }
        _ => AppError::Internal(anyhow::anyhow!("sqlx: {}", e)),
    }
}
