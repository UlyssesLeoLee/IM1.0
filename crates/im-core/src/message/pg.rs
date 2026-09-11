//! PgMessageRepository + PgSequenceAllocator — PostgreSQL 实现
//!
//! 依据: aux-02 §F.12 messages + §F.4 conversation_sequences(已写在 conversation/pg.rs,但 sequence 分配放这里)
//!       ImplementationSpec §4.5(sequence 强单调行锁)
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use im_common::ids::{ConversationId, MessageId, UserId};
use im_common::AppError;

use super::repository::{Message, MessageRepository, MessageState, NewMessage};
use super::sequence::SequenceAllocator;

// ============================================================================
// PgMessageRepository
// ============================================================================

#[derive(Clone)]
pub struct PgMessageRepository {
    pool: PgPool,
}

impl PgMessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl MessageRepository for PgMessageRepository {
    async fn begin_tx(&self) -> Result<Transaction<'_, Postgres>, AppError> {
        self.pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx begin: {}", e)))
    }

    async fn insert_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        msg: NewMessage,
    ) -> Result<Message, AppError> {
        let state_str = message_state_to_str(msg.state);
        // 强校验 content 是 JSON object
        if !matches!(msg.content, JsonValue::Object(_)) {
            return Err(AppError::Validation("content must be JSON object".into()));
        }
        let row: MessageRow = sqlx::query_as(
            r#"
            INSERT INTO messages (
                id, conversation_id, sequence, sender_id, kind, content,
                reply_to, idempotency_key, state
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, conversation_id, sequence, sender_id, kind, content,
                      reply_to, state, created_at, edited_at
            "#,
        )
        .bind(msg.id.0)
        .bind(msg.conversation_id.0)
        .bind(msg.sequence)
        .bind(msg.sender_id.map(|u| u.0))
        .bind(&msg.kind)
        .bind(&msg.content)
        .bind(msg.reply_to.map(|m| m.0))
        .bind(&msg.idempotency_key)
        .bind(state_str)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx insert_in_tx: {}", e)))?;
        Ok(row.into_message())
    }

    async fn find_by_idempotency_key(
        &self,
        conversation_id: ConversationId,
        sender_id: UserId,
        key: &str,
    ) -> Result<Option<Message>, AppError> {
        // 系统消息 sender=NULL 用 NULLS NOT DISTINCT 也能命中;
        // 用 `UserId::nil()` (Uuid::nil() = 00000000-...) 作为 sentinel,
        // 表示"任意 sender(NULL 或实际 UUID)都算同 idem"
        // SQL:`(sender_id = $2) OR ($2 是 nil 且 sender_id IS NULL)` — 简化用:
        // `sender_id IS NOT DISTINCT FROM $2 OR $2 = '00000000-0000-0000-0000-000000000000'`
        let nil_uuid = Uuid::nil();
        let row: Option<MessageRow> = sqlx::query_as(
            r#"
            SELECT id, conversation_id, sequence, sender_id, kind, content,
                   reply_to, state, created_at, edited_at
            FROM messages
            WHERE conversation_id = $1
              AND idempotency_key = $3
              AND (
                  sender_id IS NOT DISTINCT FROM $2
                  OR $2 = $4
              )
            "#,
        )
        .bind(conversation_id.0)
        .bind(sender_id.0)
        .bind(key)
        .bind(nil_uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(MessageRow::into_message))
    }

    async fn list_after_sequence(
        &self,
        conversation_id: ConversationId,
        after: i64,
        limit: i32,
    ) -> Result<Vec<Message>, AppError> {
        let limit = limit.clamp(1, 200);
        let rows: Vec<MessageRow> = sqlx::query_as(
            r#"
            SELECT id, conversation_id, sequence, sender_id, kind, content,
                   reply_to, state, created_at, edited_at
            FROM messages
            WHERE conversation_id = $1 AND sequence > $2
            ORDER BY sequence ASC
            LIMIT $3
            "#,
        )
        .bind(conversation_id.0)
        .bind(after)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(rows.into_iter().map(MessageRow::into_message).collect())
    }

    async fn find_by_id(&self, message_id: MessageId) -> Result<Option<Message>, AppError> {
        let row: Option<MessageRow> = sqlx::query_as(
            r#"
            SELECT id, conversation_id, sequence, sender_id, kind, content,
                   reply_to, state, created_at, edited_at
            FROM messages WHERE id = $1
            "#,
        )
        .bind(message_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?;
        Ok(row.map(MessageRow::into_message))
    }

    async fn update_state(&self, message_id: MessageId, new_state: MessageState) -> Result<(), AppError> {
        let state_str = message_state_to_str(new_state);
        let n = sqlx::query(
            r#"
            UPDATE messages SET state = $1
            WHERE id = $2
            "#,
        )
        .bind(state_str)
        .bind(message_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx: {}", e)))?
        .rows_affected();
        if n == 0 {
            return Err(AppError::MessageNotFound(message_id.0));
        }
        Ok(())
    }
}

// ============================================================================
// PgSequenceAllocator
// ============================================================================

#[derive(Clone)]
pub struct PgSequenceAllocator {
    /// 留位:V1+ 可能需要直查 pool(如全局 sequence 缓存),MVP 仅在 tx 内分配
    #[allow(dead_code)]
    pool: PgPool,
}

impl PgSequenceAllocator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_pool(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl SequenceAllocator for PgSequenceAllocator {
    async fn next(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        conversation_id: ConversationId,
    ) -> Result<i64, AppError> {
        // 行锁(SELECT ... FOR UPDATE)+ UPDATE next_sequence = next_sequence + 1
        // RETURNING old value;新 sequence = old
        let row: (i64,) = sqlx::query_as(
            r#"
            UPDATE conversation_sequences
            SET next_sequence = next_sequence + 1
            WHERE conversation_id = $1
            RETURNING next_sequence - 1
            "#,
        )
        .bind(conversation_id.0)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx sequence: {}", e)))?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "conversation_sequences row not found for conv {}",
                conversation_id.0
            ))
        })?;
        Ok(row.0)
    }
}

// ============================================================================
// Row 映射 + helpers
// ============================================================================

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    conversation_id: Uuid,
    sequence: i64,
    sender_id: Option<Uuid>,
    kind: String,
    content: JsonValue,
    reply_to: Option<Uuid>,
    state: String,
    created_at: DateTime<Utc>,
    edited_at: Option<DateTime<Utc>>,
}

impl MessageRow {
    fn into_message(self) -> Message {
        Message {
            id: MessageId(self.id),
            conversation_id: ConversationId(self.conversation_id),
            sequence: self.sequence,
            sender_id: self.sender_id.map(UserId),
            kind: self.kind,
            content: self.content,
            reply_to: self.reply_to.map(MessageId),
            state: message_state_from_str(&self.state),
            created_at: self.created_at,
            edited_at: self.edited_at,
        }
    }
}

fn message_state_to_str(s: MessageState) -> &'static str {
    match s {
        MessageState::Sent => "sent",
        MessageState::Delivered => "delivered",
        MessageState::Read => "read",
        MessageState::Recalled => "recalled",
        MessageState::Deleted => "deleted",
    }
}

fn message_state_from_str(s: &str) -> MessageState {
    match s {
        "delivered" => MessageState::Delivered,
        "read" => MessageState::Read,
        "recalled" => MessageState::Recalled,
        "deleted" => MessageState::Deleted,
        _ => MessageState::Sent,
    }
}
