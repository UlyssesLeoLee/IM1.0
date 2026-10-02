//! RelationshipService — 业务编排
//!
//! 依据: ImplementationSpec §7.4.4

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
        // 检查是否**被对方**屏蔽
        //
        // 2026-10-03 修正:原先写的是 `is_blocked(recipient, sender)`。而
        // `FriendshipRepository::is_blocked(user, target)` 的语义是
        // 「**user 被 target 屏蔽**」(SQL: `user_id = target AND
        // friend_id = user`, 屏蔽者存在 user_id 列)。所以原写法问的其实是
        // 「recipient 被 sender 屏蔽」—— **方向正好反了**:
        //   - 我拉黑的人照样能给我发好友申请(应该被拒的没被拒)
        //   - 我拉黑过的人我反而发不出申请(不该被拒的被拒)
        //
        // 要问的是「sender 被 recipient 屏蔽」, 故传 `(sender, recipient)`。
        // 端到端用例 `blocked_user_cannot_send_friend_request` 锁住这条。
        if self.repo.is_blocked(sender, recipient).await? {
            return Err(AppError::UserBlocked);
        }
        // 重复申请的错误码由仓储层按 aux-11 §4 区分:
        // 已有 pending → FriendRequestExists;已有终态 → InvalidStateTransition。
        // 两者都是 409, 但**码不同**, 客户端据此决定能否重试。
        self.repo
            .create_request(env, sender, recipient)
            .await
            .map(|_| ())
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
                to: if accept {
                    "accepted".into()
                } else {
                    "rejected".into()
                },
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
