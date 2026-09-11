//! PgFriendshipRepository — PostgreSQL 实现
//!
//! 依据: aux-02 §F.6 friend_requests / §F.7 friendships
//!       FriendshipRepository trait
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{
    FriendRequest, FriendRequestState, FriendshipRepository,
};

#[derive(Clone)]
pub struct PgFriendshipRepository {
    pool: PgPool,
}

impl PgFriendshipRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl FriendshipRepository for PgFriendshipRepository {
    async fn create_request(
        &self,
        env: EnvironmentId,
        sender: UserId,
        recipient: UserId,
    ) -> Result<FriendRequest, AppError> {
        if sender == recipient {
            return Err(AppError::Validation("cannot send to self".into()));
        }
        // 幂等 + UNIQUE(env, sender, recipient) 触发;
        // 已存在且 state=pending → 返回该行;
        // 已存在且非 pending → FriendRequestExists(per 2026-08-23 P2-1 已知限制)
        let row: Option<FriendRequestRow> = sqlx::query_as(
            r#"
            INSERT INTO friend_requests (environment_id, sender_id, recipient_id, state)
            VALUES ($1, $2, $3, 'pending')
            ON CONFLICT (environment_id, sender_id, recipient_id) DO NOTHING
            RETURNING id, environment_id, sender_id, recipient_id, state, created_at, updated_at
            "#,
        )
        .bind(env.0)
        .bind(sender.0)
        .bind(recipient.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        if let Some(r) = row {
            return Ok(r.into_request());
        }
        // ON CONFLICT 触发 → 已存在;回查
        let existing: FriendRequestRow = sqlx::query_as(
            r#"
            SELECT id, environment_id, sender_id, recipient_id, state, created_at, updated_at
            FROM friend_requests
            WHERE environment_id = $1 AND sender_id = $2 AND recipient_id = $3
            "#,
        )
        .bind(env.0)
        .bind(sender.0)
        .bind(recipient.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!(
            "ON CONFLICT triggered but row not found (env={}, sender={}, recipient={})",
            env.0, sender.0, recipient.0
        )))?;

        // 已在 pending 视为幂等成功;非 pending 报错(per 2026-08-23 P2-1 限制)
        match existing.state.as_str() {
            "pending" => Ok(existing.into_request()),
            _ => Err(AppError::FriendRequestExists),
        }
    }

    async fn find_request(&self, id: Uuid) -> Result<Option<FriendRequest>, AppError> {
        let row: Option<FriendRequestRow> = sqlx::query_as(
            r#"
            SELECT id, environment_id, sender_id, recipient_id, state, created_at, updated_at
            FROM friend_requests WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(FriendRequestRow::into_request))
    }

    async fn respond_request(&self, id: Uuid, accept: bool) -> Result<(), AppError> {
        let new_state = if accept { "accepted" } else { "rejected" };
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx begin: {}", e)))?;

        // 1. 查 request + 锁
        let req: Option<(Uuid, Uuid, Uuid, String)> = sqlx::query_as(
            r#"
            SELECT id, sender_id, recipient_id, state
            FROM friend_requests WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        let (_, sender, recipient, state) = req.ok_or(AppError::FriendRequestNotFound(id))?;

        if state != "pending" {
            return Err(AppError::InvalidStateTransition {
                from: state,
                to: new_state.into(),
            });
        }

        // 2. 更新 request state
        sqlx::query(
            r#"
            UPDATE friend_requests SET state = $1
            WHERE id = $2
            "#,
        )
        .bind(new_state)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        // 3. 若 accept,建立 friendship(双向,accepted 状态)
        if accept {
            // 取 env_id(从 sender 推到 env)
            let env_id: Uuid = sqlx::query_scalar(
                r#"
                SELECT environment_id FROM users WHERE id = $1
                "#,
            )
            .bind(sender)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

            // 双向 friendships (accepted 状态);ON CONFLICT 幂等
            sqlx::query(
                r#"
                INSERT INTO friendships (environment_id, user_id, friend_id, state)
                VALUES ($1, $2, $3, 'accepted'), ($1, $3, $2, 'accepted')
                ON CONFLICT (environment_id, user_id, friend_id) DO NOTHING
                "#,
            )
            .bind(env_id)
            .bind(sender)
            .bind(recipient)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx commit: {}", e)))?;
        Ok(())
    }

    async fn block(&self, user: UserId, target: UserId) -> Result<(), AppError> {
        if user == target {
            return Err(AppError::Validation("cannot block self".into()));
        }
        // 拿到 env_id(user 必属某 env)
        let env_id: Uuid = sqlx::query_scalar(
            r#"
            SELECT environment_id FROM users WHERE id = $1
            "#,
        )
        .bind(user.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        // 单边 friendships(user_id=user, friend_id=target, state=blocked)
        // 也可建反向(target, user, accepted)保持视图一致;MVP 先单向
        sqlx::query(
            r#"
            INSERT INTO friendships (environment_id, user_id, friend_id, state)
            VALUES ($1, $2, $3, 'blocked')
            ON CONFLICT (environment_id, user_id, friend_id)
            DO UPDATE SET state = 'blocked'
            "#,
        )
        .bind(env_id)
        .bind(user.0)
        .bind(target.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(())
    }

    async fn list_friends(
        &self,
        user: UserId,
        _cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<UserId>, AppError> {
        let limit = limit.clamp(1, 200) as i64;
        // MVP:不实现 cursor,只按 created_at 倒序 + limit
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT friend_id FROM friendships
            WHERE user_id = $1 AND state = 'accepted'
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(user.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(rows.into_iter().map(|(u,)| UserId(u)).collect())
    }

    async fn is_blocked(&self, user: UserId, target: UserId) -> Result<bool, AppError> {
        // "user 被 target 屏蔽" = friendships(target, user, 'blocked') 存在
        let n: (i64,) = sqlx::query_as(
            r#"
            SELECT count(*)::bigint FROM friendships
            WHERE user_id = $1 AND friend_id = $2 AND state = 'blocked'
            "#,
        )
        .bind(target.0)
        .bind(user.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(n.0 > 0)
    }
}

// ============================================================================
// Row + 映射
// ============================================================================

#[derive(sqlx::FromRow)]
struct FriendRequestRow {
    id: Uuid,
    environment_id: Uuid,
    sender_id: Uuid,
    recipient_id: Uuid,
    state: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl FriendRequestRow {
    fn into_request(self) -> FriendRequest {
        let state = match self.state.as_str() {
            "accepted" => FriendRequestState::Accepted,
            "rejected" => FriendRequestState::Rejected,
            "expired" => FriendRequestState::Expired,
            _ => FriendRequestState::Pending,
        };
        FriendRequest {
            id: self.id,
            environment_id: EnvironmentId(self.environment_id),
            sender_id: UserId(self.sender_id),
            recipient_id: UserId(self.recipient_id),
            state,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

// 让编译通过:无

