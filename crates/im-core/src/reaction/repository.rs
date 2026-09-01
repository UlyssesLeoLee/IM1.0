//! Reaction Repository 接口
//!
//! 依据: aux-02 §F.13 message_reactions 表
//!       (message_id, user_id, emoji) 联合主键
//!
//! 2026-09-01 新增(C-1 WBS):im-core 6 个 PgRepository 实装需求
//! 之前 message 模块的注释提到 Reaction 但没单独建模块。

#![allow(dead_code)] // 2026-09-01 Day 8 C-1:新模块,clippy 占位 OK,V1 实装/调优时移除
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use im_common::ids::{MessageId, UserId};
use im_common::AppError;

/// 一条 reaction 实体
#[derive(Debug, Clone)]
pub struct Reaction {
    pub message_id: MessageId,
    pub user_id: UserId,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
}

/// Reaction Repository trait
///
/// 设计要点:
/// - 幂等:同 (msg, user, emoji) 重复 add → 视为成功,不报错
/// - list:返回某条 message 的所有 reaction(emoji 聚合可选,留给 service 层)
/// - 唯一性:PK 联合主键保证(数据库层)
#[async_trait]
pub trait ReactionRepository: Send + Sync {
    /// 增(幂等)
    async fn add(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<Reaction, AppError>;

    /// 删(返回是否真的删了;不存在不报错)
    async fn remove(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
    ) -> Result<bool, AppError>;

    /// 列出某 message 的所有 reaction
    async fn list_for_message(&self, message_id: MessageId) -> Result<Vec<Reaction>, AppError>;
}
