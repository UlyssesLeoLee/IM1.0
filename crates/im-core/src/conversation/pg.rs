//! PgConversationRepository — PostgreSQL 实现
//!
//! 依据: aux-02 §F.8 conversations / §F.9 conversation_sequences /
//!       §F.10 dm_pairs / §F.11 conversation_members
//!       ConversationRepository trait
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装
//! 2026-09-20 升级(C-8 WBS ULYS-148):
//! - 新增 create_in_tx / find_dm_in_tx / upsert_dm_pair / add_member_in_tx 等事务版本
//! - 让 Service 可在单事务里完成"建 conv + 建 dm_pairs + 加双方成员"以保证 dup-dm 唯一性

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{
    normalize_dm_pair, Conversation, ConversationKind, ConversationMember,
    ConversationRepository, DmPairUpsertResult, MemberRole,
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

    fn kind_to_str(kind: ConversationKind) -> &'static str {
        match kind {
            ConversationKind::Dm => "dm",
            ConversationKind::Group => "group",
            ConversationKind::Channel => "channel",
            ConversationKind::System => "system",
            ConversationKind::Broadcast => "broadcast",
        }
    }

    fn role_to_str(role: MemberRole) -> &'static str {
        match role {
            MemberRole::Owner => "owner",
            MemberRole::Admin => "admin",
            MemberRole::Member => "member",
        }
    }
}

#[async_trait]
impl ConversationRepository for PgConversationRepository {
    fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn create(
        &self,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: JsonValue,
    ) -> Result<Conversation, AppError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx begin: {}", e)))?;
        let conv = self.create_in_tx(&mut tx, env, kind, metadata).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx commit: {}", e)))?;
        Ok(conv)
    }

    async fn create_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: JsonValue,
    ) -> Result<Conversation, AppError> {
        if !matches!(metadata, JsonValue::Object(_)) {
            return Err(AppError::Validation("metadata must be JSON object".into()));
        }
        let kind_str = Self::kind_to_str(kind);

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
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        // 初始化 conversation_sequences(同事务内)
        sqlx::query(
            r#"
            INSERT INTO conversation_sequences (conversation_id, next_sequence)
            VALUES ($1, 1)
            "#,
        )
        .bind(row.id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;

        Ok(row.into_conversation())
    }

    async fn find_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx begin: {}", e)))?;
        let r = self.find_dm_in_tx(&mut tx, env, user_a, user_b).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx commit: {}", e)))?;
        Ok(r)
    }

    async fn find_dm_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError> {
        let (a, b) = normalize_dm_pair(user_a, user_b);
        let row: Option<ConvRow> = sqlx::query_as(
            r#"
            SELECT c.id, c.environment_id, c.kind, c.metadata, c.created_at
            FROM conversations c
            INNER JOIN dm_pairs d ON d.conversation_id = c.id
            WHERE d.environment_id = $1 AND d.user_a = $2 AND d.user_b = $3
            "#,
        )
        .bind(env.0)
        .bind(a.0)
        .bind(b.0)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(ConvRow::into_conversation))
    }

    async fn upsert_dm_pair(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
        conversation_id: ConversationId,
    ) -> Result<DmPairUpsertResult, AppError> {
        let (a, b) = normalize_dm_pair(user_a, user_b);
        // ON CONFLICT (environment_id, user_a, user_b) DO NOTHING + RETURNING
        // 命中冲突时 RETURNING 不返回行 → inserted=false
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            INSERT INTO dm_pairs (environment_id, user_a, user_b, conversation_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (environment_id, user_a, user_b) DO NOTHING
            RETURNING conversation_id
            "#,
        )
        .bind(env.0)
        .bind(a.0)
        .bind(b.0)
        .bind(conversation_id.0)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx upsert_dm_pair: {}", e)))?;

        if let Some((cid,)) = row {
            Ok(DmPairUpsertResult {
                inserted: true,
                conversation_id: ConversationId(cid),
            })
        } else {
            // 已存在 → 回查 conversation_id(可能与传进来的不同)
            let existing: (Uuid,) = sqlx::query_as(
                r#"
                SELECT conversation_id FROM dm_pairs
                WHERE environment_id = $1 AND user_a = $2 AND user_b = $3
                "#,
            )
            .bind(env.0)
            .bind(a.0)
            .bind(b.0)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
            Ok(DmPairUpsertResult {
                inserted: false,
                conversation_id: ConversationId(existing.0),
            })
        }
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
        let role_str = Self::role_to_str(role);
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

    async fn add_member_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        conv: ConversationId,
        user: UserId,
        role: MemberRole,
    ) -> Result<(), AppError> {
        let role_str = Self::role_to_str(role);
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
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(())
    }

    async fn remove_member(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<u64, AppError> {
        let res = sqlx::query(
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
        Ok(res.rows_affected())
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

    async fn list_members(
        &self,
        conv: ConversationId,
    ) -> Result<Vec<ConversationMember>, AppError> {
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

    async fn count_owners(&self, conv: ConversationId) -> Result<i64, AppError> {
        let n: (i64,) = sqlx::query_as(
            r#"
            SELECT count(*)::bigint FROM conversation_members
            WHERE conversation_id = $1 AND role = 'owner'
            "#,
        )
        .bind(conv.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(n.0)
    }

    async fn get_member_role(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<Option<MemberRole>, AppError> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT role FROM conversation_members
            WHERE conversation_id = $1 AND user_id = $2
            "#,
        )
        .bind(conv.0)
        .bind(user.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(|(r,)| match r.as_str() {
            "owner" => MemberRole::Owner,
            "admin" => MemberRole::Admin,
            _ => MemberRole::Member,
        }))
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
