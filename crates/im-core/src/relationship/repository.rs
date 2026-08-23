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

#[async_trait]
pub trait FriendshipRepository: Send + Sync {
    async fn create_request(
        &self,
        env: EnvironmentId,
        sender: UserId,
        recipient: UserId,
    ) -> Result<FriendRequest, AppError>;

    async fn find_request(&self, id: Uuid) -> Result<Option<FriendRequest>, AppError>;

    async fn respond_request(
        &self,
        id: Uuid,
        accept: bool,
    ) -> Result<(), AppError>;

    async fn block(&self, user: UserId, target: UserId) -> Result<(), AppError>;

    async fn list_friends(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<UserId>, AppError>;

    async fn is_blocked(&self, user: UserId, target: UserId) -> Result<bool, AppError>;
}
