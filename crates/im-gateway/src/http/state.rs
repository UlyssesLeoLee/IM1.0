//! AppState — im-gateway 共享 HTTP handler 状态
//!
//! 依据: ImplementationSpec §7.5 + DetailedDesign §2.3
//!
//! ## 范围 (per 132-wbs.md §5.3 + Day 3 + Day 4 auth lane)
//! MVP Day 3 注册:
//! - ConversationService(Arc<dyn ConversationRepository>)
//! - TokenService(对 Bearer access_token 验签)
//!
//! 2026-09-19 Day 4 lane-backend-core-2 (worker-A C-3 + C-4) 扩展:
//! - IdentityService(PgUserRepository + PgDeviceSessionRepository + TokenService + server_secrets)
//! - UserRepository + DeviceSessionRepository(由 IdentityService 持有,handler 直接通过 IdentityService 调)
//!
//! 后续 PR(D-1 / F-2)会扩展:
//! - main.rs wire-up(PgPool + AppConfig::load → 构造 IdentityService → 注入 AppState)
//! - MessageService / RelationshipService
//! - 限流 / 监控指标

#![allow(dead_code)] // 部分字段留给后续 PR 接入

use std::sync::Arc;

use im_common::ids::UserId;
use im_core::conversation::service::ConversationService;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::service::IdentityService;
use im_core::identity::token::TokenService;
use im_core::message::service::MessageService;

/// im-gateway 共享 handler 状态(用 `web::Data<AppState>` 注入)
#[derive(Clone)]
pub struct AppState {
    /// Conversation 服务(C-8 / C-10 / C-9 list member check)
    pub conversation_service: Arc<ConversationService>,

    /// Message 服务(C-9 send + list)
    /// 注: MessageService 不是泛型 struct (per service.rs:49), 内部字段已 type-erased,
    ///     用具体类型不需要 generic 参数
    pub message_service: Arc<MessageService>,

    /// Token 验证服务(C-8 + C-10 Bearer 鉴权 + C-3/C-4 签发)
    pub token_service: Arc<TokenService>,

    /// Identity 服务(C-3 token_exchange + C-4 guest_register)
    ///
    /// 持有 PgUserRepository + PgDeviceSessionRepository + TokenService + server_secrets HashMap
    /// main.rs 的 wire-up 留给 D-1 PR (D-1 实装 AppConfig::load + 读 PG + 构造 IdentityService)
    pub identity_service: Arc<IdentityService<PgUserRepository, PgDeviceSessionRepository>>,
}

impl AppState {
    /// 构造最小可用 AppState (Day 4 + D-1 衔接)
    pub fn new(
        conversation_service: Arc<ConversationService>,
        message_service: Arc<MessageService>,
        token_service: Arc<TokenService>,
        identity_service: Arc<IdentityService<PgUserRepository, PgDeviceSessionRepository>>,
    ) -> Self {
        Self {
            conversation_service,
            message_service,
            token_service,
            identity_service,
        }
    }
}

/// 已鉴权请求上下文(Bearer token 解析后注入到 handler)
#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub user_id: UserId,
    pub environment_id: im_common::ids::EnvironmentId,
    pub tenant_id: String,
    pub kind: String, // "user" | "guest"
}
