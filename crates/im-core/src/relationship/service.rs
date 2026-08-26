//! RelationshipService — 业务编排
//!
//! 依据: ImplementationSpec §7.4.4

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use std::sync::Arc;

use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{FriendRequestState, FriendshipRepository};

pub struct RelationshipService {
    repo: Arc<dyn FriendshipRepository>,
}

impl RelationshipService {
    pub fn new(repo: Arc<dyn FriendshipRepository>) -> Self {
        Self { repo }
    }

    pub async fn send_request(
        &self,
        env: EnvironmentId,
        sender: UserId,
        recipient: UserId,
    ) -> Result<(), AppError> {
        if sender == recipient {
            return Err(AppError::Validation(
                "cannot send friend request to self".into(),
            ));
        }
        // 检查是否已被对方屏蔽
        if self.repo.is_blocked(recipient, sender).await? {
            return Err(AppError::UserBlocked);
        }
        // 幂等:已存在同 (env, sender, recipient) 的 pending 请求 → 视为成功
        // 由 UNIQUE(env, sender, recipient) 触发,捕获错误转 FriendRequestExists
        self.repo
            .create_request(env, sender, recipient)
            .await
            .map(|_| ())
            .map_err(|e| match e {
                AppError::Internal(_) => AppError::FriendRequestExists,
                other => other,
            })
    }

    pub async fn respond_request(
        &self,
        request_id: Uuid,
        responder: UserId,
        accept: bool,
    ) -> Result<(), AppError> {
        let req = self
            .repo
            .find_request(request_id)
            .await?
            .ok_or(AppError::FriendRequestNotFound(request_id))?;

        // 仅 recipient 可响应
        if req.recipient_id != responder {
            return Err(AppError::Forbidden("not the recipient".into()));
        }
        // 仅 pending 可响应
        if req.state != FriendRequestState::Pending {
            return Err(AppError::InvalidStateTransition {
                from: format!("{:?}", req.state),
                to: if accept { "accepted".into() } else { "rejected".into() },
            });
        }

        self.repo.respond_request(request_id, accept).await
    }

    pub async fn block(&self, user: UserId, target: UserId) -> Result<(), AppError> {
        if user == target {
            return Err(AppError::Validation("cannot block self".into()));
        }
        self.repo.block(user, target).await
    }

    pub async fn list_friends(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<UserId>, AppError> {
        let limit = limit.clamp(1, 200);
        self.repo.list_friends(user, cursor, limit).await
    }
}
