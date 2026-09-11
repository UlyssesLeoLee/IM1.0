//! PgConversationRepository — PostgreSQL 实现
//!
//! 依据: aux-02 §F.8 conversations / §F.9 conversation_sequences /
//!       §F.10 dm_pairs / §F.11 conversation_members
//!       ConversationRepository trait
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{
    Conversation, ConversationKind, ConversationMember, ConversationRepository, MemberRole,
};

#[derive(Clone)]
pub struct PgConversationRepository {
    pool: PgPool,
}

impl PgConversationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl ConversationRepository for PgConversationRepository {
    async fn create(
        &self,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: JsonValue,
    ) -> Result<Conversation, AppError> {
        if !matches!(metadata, JsonValue::Object(_)) {
            return Err(AppError::Validation("metadata must be JSON object".into()));
        }
        let kind_str = match kind {
            ConversationKind::Dm => "dm",
            ConversationKind::Group => "group",
            ConversationKind::Channel => "channel",
            ConversationKind::System => "system",
            ConversationKind::Broadcast => "broadcast",
        };
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx begin: {}", e)))?;

        let row: ConvRow = sqlx::query_as(
            r#"
            INSERT INTO conversations (environment_id, kind, metadata)
            VALUES ($1, $2, $3)
            RETURNING id, environment_id, kind, metadata, created_at
            "#,
        )
        .bind(env.0)
        .bind(kind_str)
        .bind(&metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        // 初始化 conversation_sequences
        sqlx::query(
            r#"
            INSERT INTO conversation_sequences (conversation_id, next_sequence)
            VALUES ($1, 1)
            "#,
        )
        .bind(row.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx commit: {}", e)))?;

        Ok(row.into_conversation())
    }

    async fn find_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError> {
        // 规范化 user_a < user_b(per dm_pairs CHECK 约束)
        let (a, b) = if user_a.0 < user_b.0 {
            (user_a.0, user_b.0)
        } else {
            (user_b.0, user_a.0)
        };
        let row: Option<ConvRow> = sqlx::query_as(
            r#"
            SELECT c.id, c.environment_id, c.kind, c.metadata, c.created_at
            FROM conversations c
            INNER JOIN dm_pairs d ON d.conversation_id = c.id
            WHERE d.environment_id = $1 AND d.user_a = $2 AND d.user_b = $3
            "#,
        )
        .bind(env.0)
        .bind(a)
        .bind(b)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(ConvRow::into_conversation))
    }

    async fn find_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, AppError> {
        let row: Option<ConvRow> = sqlx::query_as(
            r#"
            SELECT id, environment_id, kind, metadata, created_at
            FROM conversations WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(ConvRow::into_conversation))
    }

    async fn list_for_user(
        &self,
        user: UserId,
        _cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError> {
        let limit = limit.clamp(1, 200) as i64;
        let rows: Vec<ConvRow> = sqlx::query_as(
            r#"
            SELECT c.id, c.environment_id, c.kind, c.metadata, c.created_at
            FROM conversations c
            INNER JOIN conversation_members m ON m.conversation_id = c.id
            WHERE m.user_id = $1
            ORDER BY c.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(user.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(rows.into_iter().map(ConvRow::into_conversation).collect())
    }

    async fn add_member(
        &self,
        conv: ConversationId,
        user: UserId,
        role: MemberRole,
    ) -> Result<(), AppError> {
        let role_str = match role {
            MemberRole::Owner => "owner",
            MemberRole::Admin => "admin",
            MemberRole::Member => "member",
        };
        sqlx::query(
            r#"
            INSERT INTO conversation_members (conversation_id, user_id, role)
            VALUES ($1, $2, $3)
            ON CONFLICT (conversation_id, user_id) DO NOTHING
            "#,
        )
        .bind(conv.0)
        .bind(user.0)
        .bind(role_str)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(())
    }

    async fn remove_member(&self, conv: ConversationId, user: UserId) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM conversation_members
            WHERE conversation_id = $1 AND user_id = $2
            "#,
        )
        .bind(conv.0)
        .bind(user.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(())
    }

    async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, AppError> {
        let n: (i64,) = sqlx::query_as(
            r#"
            SELECT count(*)::bigint FROM conversation_members
            WHERE conversation_id = $1 AND user_id = $2
            "#,
        )
        .bind(conv.0)
        .bind(user.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(n.0 > 0)
    }

    async fn list_members(&self, conv: ConversationId) -> Result<Vec<ConversationMember>, AppError> {
        let rows: Vec<MemberRow> = sqlx::query_as(
            r#"
            SELECT conversation_id, user_id, role, joined_at, last_read_sequence
            FROM conversation_members
            WHERE conversation_id = $1
            ORDER BY joined_at ASC
            "#,
        )
        .bind(conv.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(rows.into_iter().map(MemberRow::into_member).collect())
    }
}

// ============================================================================
// Row 映射
// ============================================================================

#[derive(sqlx::FromRow)]
struct ConvRow {
    id: Uuid,
    environment_id: Uuid,
    kind: String,
    metadata: JsonValue,
    created_at: DateTime<Utc>,
}

impl ConvRow {
    fn into_conversation(self) -> Conversation {
        let kind = match self.kind.as_str() {
            "group" => ConversationKind::Group,
            "channel" => ConversationKind::Channel,
            "system" => ConversationKind::System,
            "broadcast" => ConversationKind::Broadcast,
            _ => ConversationKind::Dm,
        };
        Conversation {
            id: ConversationId(self.id),
            environment_id: EnvironmentId(self.environment_id),
            kind,
            metadata: self.metadata,
            created_at: self.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    conversation_id: Uuid,
    user_id: Uuid,
    role: String,
    joined_at: DateTime<Utc>,
    last_read_sequence: i64,
}

impl MemberRow {
    fn into_member(self) -> ConversationMember {
        let role = match self.role.as_str() {
            "owner" => MemberRole::Owner,
            "admin" => MemberRole::Admin,
            _ => MemberRole::Member,
        };
        ConversationMember {
            conversation_id: ConversationId(self.conversation_id),
            user_id: UserId(self.user_id),
            role,
            joined_at: self.joined_at,
            last_read_sequence: self.last_read_sequence,
        }
    }
}
