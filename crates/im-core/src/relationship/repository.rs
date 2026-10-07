//! Relationship Repository 接口
//!
//! 依据: ImplementationSpec §7.4.4

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendRequestState {
    Pending,
    Accepted,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FriendshipState {
    Accepted,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequest {
    pub id: Uuid,
    pub environment_id: EnvironmentId,
    pub sender_id: UserId,
    pub recipient_id: UserId,
    pub state: FriendRequestState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 窄接口:只回答「`user` 是否已被 `target` 拉黑」。
///
/// ## 为什么要把它从 `FriendshipRepository` 拆成超 trait
///
/// `MessageService` 发 DM 前需要做 block 校验, 但它并不需要增删好友申请。
/// 让它依赖整个 `FriendshipRepository`(6 个方法) 等于让发消息这条主路径为了
/// 一个布尔查询背上整个好友域的接口面。拆成超 trait 后, 依赖面就是 1 个方法,
/// 而 `RelationshipService` 仍可按 `Arc<dyn FriendshipRepository>` 使用全集。
///
/// `FriendshipRepository: BlockChecker` 这层继承还带来一个约束: 任何实现了
/// 好友仓储的类型**自动**满足 block 查询能力, 不可能出现「能建关系但不能查拉黑」
/// 的半截实现。
#[async_trait]
pub trait BlockChecker: Send + Sync {
    /// `true` = `user` 已被 `target` 拉黑。
    ///
    /// 方向语义(极易写反, 2026-10-03 已在 `RelationshipService::send_request`
    /// 上栽过一次): 查的是 `friendships` 里 **`target` → `user`** 的 blocked 行,
    /// 即「谁拉黑了 user」, 而不是 `user` → `target`。
    async fn is_blocked(&self, user: UserId, target: UserId) -> Result<bool, AppError>;
}

#[async_trait]
pub trait FriendshipRepository: BlockChecker + Send + Sync {
    async fn create_request(
        &self,
        env: EnvironmentId,
        sender: UserId,
        recipient: UserId,
    ) -> Result<FriendRequest, AppError>;

    async fn find_request(&self, id: Uuid) -> Result<Option<FriendRequest>, AppError>;

    async fn respond_request(&self, id: Uuid, accept: bool) -> Result<(), AppError>;

    async fn block(&self, user: UserId, target: UserId) -> Result<(), AppError>;

    async fn list_friends(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<UserId>, AppError>;

    // `is_blocked` 由超 trait `BlockChecker` 提供, 此处不再重复声明。
}
