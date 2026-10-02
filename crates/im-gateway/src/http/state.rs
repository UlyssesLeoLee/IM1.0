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

use std::sync::Arc;

use im_common::ids::UserId;
use im_core::conversation::service::ConversationService;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::service::IdentityService;
use im_core::identity::token::TokenService;
use im_core::message::service::MessageService;
use im_core::reaction::service::ReactionService;
use im_core::relationship::service::RelationshipService;
use im_core::settings::service::SettingsService;

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

    /// 环境级配置读取 (C-9 撤回时间窗等「不能写死」的参数来源)
    ///
    /// 2026-10-03 新增。此前 `im_core::settings::SettingsService` 存在但
    /// **从不读库**(`get()` 从空 HashMap 落 `default()`, 恒返回 120s) ——
    /// 见该文件模块文档。若当时把撤回时间窗接到它上面, 代码读起来是
    /// 「从 env settings 读」(合规), 实际却恒为写死值。现已改为真读
    /// `environments.settings` JSONB。
    pub settings_service: Arc<SettingsService>,

    /// Reaction 服务 (C-9 `react` 帧 —— 6 类业务帧的第 5 类)
    ///
    /// 权限边界在 `ReactionService` 内部: 必须是**消息所属会话的成员**。
    /// `ReactionRepository` 自身只做 PK 幂等插入, 不做任何校验, 所以它绝不能
    /// 被直接暴露给 handler。
    pub reaction_service: Arc<ReactionService>,

    /// Relationship 服务 (IM-REL-001 —— 好友申请/接受/拉黑/好友列表)
    ///
    /// 2026-10-03 新增, 为 `POST /v1/friends/requests` /
    /// `POST /v1/friends/requests/{id}/respond` /
    /// `POST /v1/friends/{id}/block` 三个端点服务。
    ///
    /// 此前该 service 已实装且无任何调用方 —— 又一个「能做的功能没有任何出口」
    /// (与 §1.14 记的消息动作同一形态)。
    pub relationship_service: Arc<RelationshipService>,
}

impl AppState {
    /// 构造最小可用 AppState (Day 4 + D-1 衔接)
    pub fn new(
        conversation_service: Arc<ConversationService>,
        message_service: Arc<MessageService>,
        token_service: Arc<TokenService>,
        identity_service: Arc<IdentityService<PgUserRepository, PgDeviceSessionRepository>>,
        settings_service: Arc<SettingsService>,
        reaction_service: Arc<ReactionService>,
        relationship_service: Arc<RelationshipService>,
    ) -> Self {
        Self {
            conversation_service,
            message_service,
            token_service,
            identity_service,
            settings_service,
            reaction_service,
            relationship_service,
        }
    }
}

/// 已鉴权请求上下文(Bearer token 解析后注入到 handler)
#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub user_id: UserId,
    pub environment_id: im_common::ids::EnvironmentId,
    // 守门 #1 缺口台账: 已聚合进鉴权上下文, 但现有 handler 尚未按租户过滤
    // (多租户隔离随 G-1/V1 落地)。保留字段不删;
    // per docs/Project-Status.md 1.1.1 占位符保留约定。
    #[allow(dead_code)]
    pub tenant_id: String,
    pub kind: String, // "user" | "guest"
    /// 该 access token 绑定的 device session (来自 JWT `dsid` claim)
    ///
    /// 2026-10-03 (C-7 logout 实装): `POST /v1/auth/logout` 靠它调
    /// `IdentityService::logout` → `device_repo.revoke` 真正吊销会话。
    /// `None` = token 是本字段加入前签发的旧 token, 或非本服务签发。
    pub device_session_id: Option<im_common::ids::DeviceSessionId>,
}
