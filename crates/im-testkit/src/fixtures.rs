//! 共享测试数据 builder
//!
//! 8 个 fixture builder:
//! - `Tenant` / `Game` / `Environment` —— 配置面(tenant / game / env 三层)
//! - `User` / `Device` / `Session` —— 身份面
//! - `Conversation` / `Message` —— 业务面
//!
//! 每个 fixture 返回强类型(im-core 公开 API)。builder pattern 允许链式 override 默认值。
//! 默认值来自 aux-02 数据字典 + 实际 im-core 字段对齐。

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use im_common::ids::{
    AuditLogId, ConversationId, DeviceSessionId, EnvironmentId, FriendRequestId, GameId,
    MediaId, MessageId, TenantId, UserId,
};

// =============================================================================
// 配置面
// =============================================================================

/// Tenant 字段(per aux-02 §A tenants 表)
#[derive(Debug, Clone)]
pub struct TenantFixture {
    pub id: TenantId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

impl Default for TenantFixture {
    fn default() -> Self {
        Self {
            id: TenantId::new(),
            name: "test-tenant".into(),
            created_at: Utc::now(),
        }
    }
}

impl TenantFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_id(mut self, id: TenantId) -> Self {
        self.id = id;
        self
    }

    /// 序列化为 im-core SQLx 期望的 JSON 形式(供 repository::create 传入)
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "name": self.name,
            "created_at": self.created_at,
        })
    }
}

/// Game 字段(per aux-02 §A games 表)
#[derive(Debug, Clone)]
pub struct GameFixture {
    pub id: GameId,
    pub tenant_id: TenantId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

impl Default for GameFixture {
    fn default() -> Self {
        Self {
            id: GameId::new(),
            tenant_id: TenantId::new(),
            name: "test-game".into(),
            created_at: Utc::now(),
        }
    }
}

impl GameFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_tenant(mut self, tenant: TenantId) -> Self {
        self.tenant_id = tenant;
        self
    }
}

/// Environment 字段(per aux-02 §A environments 表)
#[derive(Debug, Clone)]
pub struct EnvironmentFixture {
    pub id: EnvironmentId,
    pub game_id: GameId,
    pub tenant_id: TenantId,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl Default for EnvironmentFixture {
    fn default() -> Self {
        Self {
            id: EnvironmentId::new(),
            game_id: GameId::new(),
            tenant_id: TenantId::new(),
            name: "test-env".into(),
            enabled: true,
            created_at: Utc::now(),
        }
    }
}

impl EnvironmentFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn with_game(mut self, game: GameId) -> Self {
        self.game_id = game;
        self
    }
}

// =============================================================================
// 身份面
// =============================================================================

/// User 字段(per im_core::identity::repository::User 公开字段)
#[derive(Debug, Clone)]
pub struct UserFixture {
    pub id: UserId,
    pub environment_id: EnvironmentId,
    pub kind: UserKindFixture,
    pub display_name: Option<String>,
    pub state: UserStateFixture,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserKindFixture {
    User,
    Guest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStateFixture {
    Active,
    Banned,
    Suspended,
    Deleted,
}

impl Default for UserFixture {
    fn default() -> Self {
        Self {
            id: UserId::new(),
            environment_id: EnvironmentId::new(),
            kind: UserKindFixture::User,
            display_name: Some("test-user".into()),
            state: UserStateFixture::Active,
            created_at: Utc::now(),
        }
    }
}

impl UserFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn guest(mut self) -> Self {
        self.kind = UserKindFixture::Guest;
        self
    }

    pub fn banned(mut self) -> Self {
        self.state = UserStateFixture::Banned;
        self
    }

    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    pub fn with_environment(mut self, env: EnvironmentId) -> Self {
        self.environment_id = env;
        self
    }
}

/// Device 字段(per im-core identity::token::DeviceSession)
#[derive(Debug, Clone)]
pub struct DeviceFixture {
    pub id: DeviceSessionId,
    pub user_id: UserId,
    pub device_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Default for DeviceFixture {
    fn default() -> Self {
        Self {
            id: DeviceSessionId::new(),
            user_id: UserId::new(),
            device_fingerprint: Some("test-fingerprint".into()),
            created_at: Utc::now(),
            revoked_at: None,
        }
    }
}

impl DeviceFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn revoked(mut self) -> Self {
        self.revoked_at = Some(Utc::now());
        self
    }

    pub fn with_user(mut self, user: UserId) -> Self {
        self.user_id = user;
        self
    }
}

/// Session 字段(简化版,实际由 TokenService 派生 AccessToken)
#[derive(Debug, Clone)]
pub struct SessionFixture {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: UserId,
    pub device_id: DeviceSessionId,
    pub environment_id: EnvironmentId,
    pub expires_at: DateTime<Utc>,
}

impl Default for SessionFixture {
    fn default() -> Self {
        Self {
            access_token: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...".into(),
            refresh_token: format!("rt_{}", Uuid::new_v4()),
            user_id: UserId::new(),
            device_id: DeviceSessionId::new(),
            environment_id: EnvironmentId::new(),
            expires_at: Utc::now() + chrono::Duration::seconds(900),
        }
    }
}

impl SessionFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_user(mut self, user: UserId) -> Self {
        self.user_id = user;
        self
    }
}

// =============================================================================
// 业务面
// =============================================================================

/// Conversation 字段(per im_core::conversation::repository::Conversation 公开字段)
#[derive(Debug, Clone)]
pub struct ConversationFixture {
    pub id: ConversationId,
    pub environment_id: EnvironmentId,
    pub kind: ConversationKindFixture,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationKindFixture {
    Dm,
    Group,
    Channel,
    System,
    Broadcast,
}

impl Default for ConversationFixture {
    fn default() -> Self {
        Self {
            id: ConversationId::new(),
            environment_id: EnvironmentId::new(),
            kind: ConversationKindFixture::Dm,
            metadata: json!({}),
            created_at: Utc::now(),
        }
    }
}

impl ConversationFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn group(mut self) -> Self {
        self.kind = ConversationKindFixture::Group;
        self
    }

    pub fn channel(mut self) -> Self {
        self.kind = ConversationKindFixture::Channel;
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_game_metadata(mut self, game_id: GameId) -> Self {
        self.metadata = json!({
            "game.type": "guild",
            "game.game_id": game_id.to_string(),
        });
        self
    }
}

/// Message 字段(per im_core::message::repository::Message 公开字段)
#[derive(Debug, Clone)]
pub struct MessageFixture {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub sequence: i64,
    pub sender_id: Option<UserId>,
    pub kind: String,
    pub content: Value,
    pub reply_to: Option<MessageId>,
    pub state: MessageStateFixture,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStateFixture {
    Sent,
    Delivered,
    Read,
    Recalled,
    Deleted,
}

impl Default for MessageFixture {
    fn default() -> Self {
        Self {
            id: MessageId::new(),
            conversation_id: ConversationId::new(),
            sequence: 1,
            sender_id: Some(UserId::new()),
            kind: "text".into(),
            content: json!({ "text": "hello" }),
            reply_to: None,
            state: MessageStateFixture::Sent,
            created_at: Utc::now(),
            edited_at: None,
        }
    }
}

impl MessageFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: json!({ "text": text.into() }),
            ..Self::default()
        }
    }

    pub fn image(media_id: MediaId) -> Self {
        Self {
            kind: "image".into(),
            content: json!({
                "media_id": media_id.to_string(),
                "width": 800,
                "height": 600,
            }),
            ..Self::default()
        }
    }

    pub fn with_sequence(mut self, seq: i64) -> Self {
        self.sequence = seq;
        self
    }

    pub fn with_conversation(mut self, conv: ConversationId) -> Self {
        self.conversation_id = conv;
        self
    }

    pub fn with_sender(mut self, user: UserId) -> Self {
        self.sender_id = Some(user);
        self
    }

    pub fn system_event(event: impl Into<String>) -> Self {
        Self {
            kind: "system".into(),
            content: json!({ "event": event.into() }),
            sender_id: None,
            ..Self::default()
        }
    }

    pub fn recalled(mut self) -> Self {
        self.state = MessageStateFixture::Recalled;
        self
    }
}

// =============================================================================
// 辅助:测试用稳定 ID 工厂(避免每次 ::new() 重新生成)
// =============================================================================

/// 稳定 UUID 工厂(便于测试间对齐)
pub fn stable_tenant_id() -> TenantId {
    Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_game_id() -> GameId {
    Uuid::parse_str("8a7e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_environment_id() -> EnvironmentId {
    Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_user_id() -> UserId {
    Uuid::parse_str("1a2e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_conversation_id() -> ConversationId {
    Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_message_id() -> MessageId {
    Uuid::parse_str("8a7e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_audit_log_id() -> AuditLogId {
    Uuid::parse_str("9c8e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

pub fn stable_friend_request_id() -> FriendRequestId {
    Uuid::parse_str("ad8e6679-7425-40de-944b-e07fc1f90ae7").unwrap().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_builder_returns_valid_tenant() {
        let t = TenantFixture::new().with_name("acme");
        assert_eq!(t.name, "acme");
        assert_ne!(t.id, TenantId::nil());
    }

    #[test]
    fn test_game_builder_links_tenant() {
        let tenant = TenantFixture::new();
        let g = GameFixture::new().with_tenant(tenant.id);
        assert_eq!(g.tenant_id, tenant.id);
    }

    #[test]
    fn test_environment_builder_can_be_disabled() {
        let env = EnvironmentFixture::new().disabled();
        assert!(!env.enabled);
    }

    #[test]
    fn test_user_builder_default_is_active() {
        let u = UserFixture::new();
        assert_eq!(u.state, UserStateFixture::Active);
        assert_eq!(u.kind, UserKindFixture::User);
    }

    #[test]
    fn test_user_builder_guest_sets_kind() {
        let u = UserFixture::new().guest();
        assert_eq!(u.kind, UserKindFixture::Guest);
    }

    #[test]
    fn test_user_builder_banned_sets_state() {
        let u = UserFixture::new().banned();
        assert_eq!(u.state, UserStateFixture::Banned);
    }

    #[test]
    fn test_device_builder_revoked() {
        let d = DeviceFixture::new().revoked();
        assert!(d.revoked_at.is_some());
    }

    #[test]
    fn test_session_builder_for_user() {
        let u = UserFixture::new();
        let s = SessionFixture::new().for_user(u.id);
        assert_eq!(s.user_id, u.id);
    }

    #[test]
    fn test_conversation_builder_group() {
        let c = ConversationFixture::new().group();
        assert_eq!(c.kind, ConversationKindFixture::Group);
    }

    #[test]
    fn test_conversation_builder_game_metadata() {
        let game_id = stable_game_id();
        let c = ConversationFixture::new().with_game_metadata(game_id);
        assert_eq!(c.metadata["game.game_id"], game_id.to_string());
    }

    #[test]
    fn test_message_builder_text() {
        let m = MessageFixture::text("hi");
        assert_eq!(m.content["text"], "hi");
        assert_eq!(m.kind, "text");
    }

    #[test]
    fn test_message_builder_image() {
        let media = MediaId::new();
        let m = MessageFixture::image(media);
        assert_eq!(m.kind, "image");
        assert_eq!(m.content["media_id"], media.to_string());
    }

    #[test]
    fn test_message_builder_system_event() {
        let m = MessageFixture::system_event("join");
        assert_eq!(m.kind, "system");
        assert_eq!(m.content["event"], "join");
        assert!(m.sender_id.is_none());
    }

    #[test]
    fn test_message_builder_recalled() {
        let m = MessageFixture::text("oops").recalled();
        assert_eq!(m.state, MessageStateFixture::Recalled);
    }

    #[test]
    fn stable_ids_are_deterministic() {
        assert_eq!(
            stable_user_id().to_string(),
            "1a2e6679-7425-40de-944b-e07fc1f90ae7"
        );
        assert_eq!(
            stable_conversation_id().to_string(),
            "7c9e6679-7425-40de-944b-e07fc1f90ae7"
        );
    }
}
