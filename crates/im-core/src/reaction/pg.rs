//! PgReactionRepository — PostgreSQL 实现
//!
//! 依据: aux-02 §F.13 message_reactions 表
//!       ReactionRepository trait(同模块 repository.rs)
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use im_common::ids::{MessageId, UserId};
use im_common::AppError;

use super::repository::{Reaction, ReactionRepository};

/// PostgreSQL 实现
#[derive(Clone)]
pub struct PgReactionRepository {
    pool: PgPool,
}

impl PgReactionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 测试用:共享一个 pool 的引用构造
    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl ReactionRepository for PgReactionRepository {
    async fn add(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<Reaction, AppError> {
        // 幂等:ON CONFLICT DO NOTHING;若新插入,RETURNING 给出新行;
        // 若已存在(PK 命中),RETURNING 不会触发,我们回查一次拿现有行。
        let inserted: Option<(Uuid, Uuid, String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            INSERT INTO message_reactions (message_id, user_id, emoji)
            VALUES ($1, $2, $3)
            ON CONFLICT (message_id, user_id, emoji) DO NOTHING
            RETURNING message_id, user_id, emoji, created_at
            "#,
        )
        .bind(message_id.0)
        .bind(user_id.0)
        .bind(emoji)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        if let Some((mid, uid, emo, ts)) = inserted {
            Ok(Reaction {
                message_id: MessageId(mid),
                user_id: UserId(uid),
                emoji: emo,
                created_at: ts,
            })
        } else {
            // 已存在,回查
            self.find_one(message_id, user_id, emoji).await?.ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "reaction insert conflict but row not found (msg={}, user={}, emoji={})",
                    message_id.0, user_id.0, emoji
                ))
            })
        }
    }

    async fn remove(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<bool, AppError> {
        let n = sqlx::query(
            r#"
            DELETE FROM message_reactions
            WHERE message_id = $1 AND user_id = $2 AND emoji = $3
            "#,
        )
        .bind(message_id.0)
        .bind(user_id.0)
        .bind(emoji)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?
        .rows_affected();
        Ok(n > 0)
    }

    async fn list_for_message(&self, message_id: MessageId) -> Result<Vec<Reaction>, AppError> {
        let rows: Vec<(Uuid, Uuid, String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT message_id, user_id, emoji, created_at
            FROM message_reactions
            WHERE message_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(message_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|(mid, uid, emo, ts)| Reaction {
                message_id: MessageId(mid),
                user_id: UserId(uid),
                emoji: emo,
                created_at: ts,
            })
            .collect())
    }
}

impl PgReactionRepository {
    async fn find_one(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<Option<Reaction>, AppError> {
        let row: Option<(Uuid, Uuid, String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT message_id, user_id, emoji, created_at
            FROM message_reactions
            WHERE message_id = $1 AND user_id = $2 AND emoji = $3
            "#,
        )
        .bind(message_id.0)
        .bind(user_id.0)
        .bind(emoji)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        Ok(row.map(|(mid, uid, emo, ts)| Reaction {
            message_id: MessageId(mid),
            user_id: UserId(uid),
            emoji: emo,
            created_at: ts,
        }))
    }
}
