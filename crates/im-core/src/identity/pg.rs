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
            SELECT id, environment_id, kind, external_identity, state, display_name,
                   username, password_hash, created_at
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
            SELECT id, environment_id, kind, external_identity, state, display_name,
                   username, password_hash, created_at
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
        // 2026-09-20 C-3 WBS: 按 (env, username) 查找 user, 用于 register 重复检查 / login
        let row: Option<UserRow> = sqlx::query_as(
            r#"
            SELECT id, environment_id, kind, external_identity, state, display_name,
                   username, password_hash, created_at
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
            RETURNING id, environment_id, kind, external_identity, state, display_name,
                      username, password_hash, created_at
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

    async fn create_with_password(
        &self,
        env: EnvironmentId,
        username: &str,
        password_hash: &str,
        display_name: Option<String>,
    ) -> Result<User, AppError> {
        // 2026-09-20 C-3 WBS: 注册路径,创建 username-based user(kind='user', extid=NULL)
        //   备注:虽然 extid=NULL, 但因为 username 是 NOT NULL 且 (env, username) 有 UNIQUE 索引,
        //   重复 register 会被 PG unique violation 拦截,映射为 AppError::AccountAlreadyExists
        let row: UserRow = sqlx::query_as(
            r#"
            INSERT INTO users (environment_id, kind, external_identity, display_name,
                               username, password_hash)
            VALUES ($1, 'user', NULL, $2, $3, $4)
            RETURNING id, environment_id, kind, external_identity, state, display_name,
                      username, password_hash, created_at
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
            RETURNING id, environment_id, kind, external_identity, state, display_name,
                      username, password_hash, created_at
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
        sqlx::Error::RowNotFound => AppError::NotFound("row not found".into()),
        // PG unique_violation (SQLSTATE 23505) → AccountAlreadyExists
        // 用于 register 时 (env, username) 重复 / device_sessions 同 user_id 下 active refresh hash 重复
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            AppError::AccountAlreadyExists
        }
        _ => AppError::Internal(anyhow::anyhow!("sqlx: {}", e)),
    }
}
