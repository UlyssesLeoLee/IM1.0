//! Sequence 分配器 — 行锁原子自增
//!
//! 依据: ImplementationSpec §4.5 + DetailedDesign §5
//!
//! ## 性能
//! - 单会话连续写入时为单行串行(POC-02 验证)
//! - 实际实现走 `SELECT ... FOR UPDATE` + 同事务 UPDATE
//! - V1+ 评估 Snowflake / 分布式 ID(ADR 候选)

use async_trait::async_trait;

use im_common::ids::ConversationId;
use im_common::AppError;

#[async_trait]
pub trait SequenceAllocator: Send + Sync {
    /// 在事务内分配下一个 sequence
    async fn next(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        conversation_id: ConversationId,
    ) -> Result<i64, AppError>;
}
