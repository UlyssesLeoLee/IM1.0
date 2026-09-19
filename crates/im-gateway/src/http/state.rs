//! AppState — im-gateway 共享 HTTP handler 状态
//!
//! 依据: ImplementationSpec §7.5 + DetailedDesign §2.3
//!
//! ## 范围 (per 132-wbs.md §5.3 + Day 3)
//! MVP Day 3 注册:
//! - ConversationService(Arc<dyn ConversationRepository>)
//! - TokenService(对 Bearer access_token 验签)
//!
//! 后续 PR(D-1 / F-2)会扩展:
//! - MessageService / RelationshipService
//! - PgPool 直连(im-core gRPC 暂未实装,先用 sqlx 直连)
//! - 限流 / 监控指标

#![allow(dead_code)] // 部分字段留给后续 PR 接入

use std::sync::Arc;

use im_common::ids::UserId;
use im_core::conversation::service::ConversationService;
use im_core::identity::token::TokenService;

/// im-gateway 共享 handler 状态(用 `web::Data<AppState>` 注入)
#[derive(Clone)]
pub struct AppState {
    /// Conversation 服务(C-8 / C-10 用)
    pub conversation_service: Arc<ConversationService>,

    /// Token 验证服务(C-8 + C-10 Bearer 鉴权)
    pub token_service: Arc<TokenService>,
}

impl AppState {
    /// 构造最小可用 AppState
    pub fn new(
        conversation_service: Arc<ConversationService>,
        token_service: Arc<TokenService>,
    ) -> Self {
        Self {
            conversation_service,
            token_service,
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
