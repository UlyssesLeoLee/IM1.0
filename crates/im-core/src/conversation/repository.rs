//! Conversation Repository 接口
//!
//! 依据: ImplementationSpec §7.4.2

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversationKind {
    Dm,
    Group,
    Channel,
    System,
    Broadcast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub environment_id: EnvironmentId,
    pub kind: ConversationKind,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMember {
    pub conversation_id: ConversationId,
    pub user_id: UserId,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
    pub last_read_sequence: i64,
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn create(
        &self,
        env: EnvironmentId,
        kind: ConversationKind,
        metadata: Value,
    ) -> Result<Conversation, AppError>;

    async fn find_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Option<Conversation>, AppError>;

    async fn find_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, AppError>;

    async fn list_for_user(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError>;

    async fn add_member(
        &self,
        conv: ConversationId,
        user: UserId,
        role: MemberRole,
    ) -> Result<(), AppError>;

    async fn remove_member(&self, conv: ConversationId, user: UserId) -> Result<(), AppError>;

    async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, AppError>;

    async fn list_members(&self, conv: ConversationId) -> Result<Vec<ConversationMember>, AppError>;
}
