//! ConversationService — 业务编排
//!
//! 依据: ImplementationSpec §7.4.2 + DetailedDesign §9.3

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{
    Conversation, ConversationKind, ConversationRepository, MemberRole,
};

/// Guest 限制(ImplementationSpec §3.1.2 / §11.1):
/// `users.kind='guest'` 仅允许 kind=dm,且另一方非 banned
pub fn check_guest_can_create(user_kind_is_guest: bool, kind: ConversationKind) -> Result<(), AppError> {
    if user_kind_is_guest && !matches!(kind, ConversationKind::Dm) {
        return Err(AppError::Forbidden(
            "guest users can only create DM conversations".into(),
        ));
    }
    Ok(())
}

pub struct ConversationService {
    repo: Arc<dyn ConversationRepository>,
}

impl ConversationService {
    pub fn new(repo: Arc<dyn ConversationRepository>) -> Self {
        Self { repo }
    }

    /// 暴露底层 repo(用于跨 service 显式调用,如 C-10 list_members)
    /// MVP Day 3:留给 im-gateway 直接 list_members 用,C-9 之后考虑把 list_members
    /// 提升到 service 层(避免泄漏 repo)
    pub fn repo(&self) -> &Arc<dyn ConversationRepository> {
        &self.repo
    }

    pub async fn create_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Conversation, AppError> {
        if user_a == user_b {
            return Err(AppError::Validation("cannot create DM with self".into()));
        }

        // 幂等:已存在返回原会话
        if let Some(existing) = self.repo.find_dm(env, user_a, user_b).await? {
            return Ok(existing);
        }

        // user_a < user_b 规范化(dm_pairs CHECK 约束)
        let (a, b) = if user_a.0 < user_b.0 {
            (user_a, user_b)
        } else {
            (user_b, user_a)
        };

        let conv = self
            .repo
            .create(env, ConversationKind::Dm, Value::Object(Default::default()))
            .await?;

        self.repo.add_member(conv.id, a, MemberRole::Member).await?;
        self.repo.add_member(conv.id, b, MemberRole::Member).await?;
        Ok(conv)
    }

    pub async fn create_group(
        &self,
        env: EnvironmentId,
        creator: UserId,
        members: Vec<UserId>,
        metadata: Value,
    ) -> Result<Conversation, AppError> {
        if members.len() < 2 {
            return Err(AppError::Validation(
                "group needs at least 2 members".into(),
            ));
        }
        if members.len() > 500 {
            return Err(AppError::Validation("group too large (max 500)".into()));
        }

        let conv = self
            .repo
            .create(env, ConversationKind::Group, metadata)
            .await?;

        self.repo
            .add_member(conv.id, creator, MemberRole::Owner)
            .await?;
        for m in members {
            if m != creator {
                self.repo.add_member(conv.id, m, MemberRole::Member).await?;
            }
        }
        Ok(conv)
    }

    pub async fn list_user_conversations(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError> {
        let limit = limit.clamp(1, 200);
        self.repo.list_for_user(user, cursor, limit.clamp(1, 50)).await
    }

    pub async fn is_member(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<bool, AppError> {
        self.repo.is_member(conv, user).await
    }

    pub async fn get(&self, id: ConversationId) -> Result<Option<Conversation>, AppError> {
        self.repo.find_by_id(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::repository::ConversationKind;

    #[test]
    fn guest_cannot_create_group() {
        let r = check_guest_can_create(true, ConversationKind::Group);
        assert!(r.is_err());
    }

    #[test]
    fn guest_can_create_dm() {
        let r = check_guest_can_create(true, ConversationKind::Dm);
        assert!(r.is_ok());
    }

    #[test]
    fn user_can_create_any_kind() {
        for kind in [ConversationKind::Dm, ConversationKind::Group, ConversationKind::Channel] {
            assert!(check_guest_can_create(false, kind).is_ok());
        }
    }
}
