//! REST API 路由注册
//!
//! 依据: ImplementationSpec §3.1 + aux-13 §3
//!
//! ## 注册范围 (per 132-wbs.md §5.3 + 2026-09-19 lane-backend-core 实装)
//! MVP Day 3: C-8 / C-10 / C-12 wired
//! MVP Day 4 (lane-backend-core-2): C-3..C-7 + C-11 wired
//! MVP Day 5 (lane-backend-core-3): C-9 wired
//! 留给后续: E-1..E-4 测试补齐, F-2 k3s namespace

pub mod auth;
pub mod auth_handlers;
pub mod conversations;
pub mod error_response;
pub mod members;
pub mod message_actions;
pub mod messages;
pub mod state;

use actix_web::web;

/// 注册 /v1 路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // 鉴权(5 个端点) — MVP Day 4 实装 C-3..C-7
    //   C-3 POST /auth/token/exchange → auth_handlers::token_exchange
    //   C-4 POST /auth/guest         → auth_handlers::guest
    //   C-5 POST /auth/refresh       → auth_handlers::refresh (per 138 §8 缺口 #8 fix 用 find_by_id)
    //   C-6 POST /auth/link          → auth_handlers::link_account
    //     (2026-10-03 更正: 本行原注释「待 UPDATE 实现, 当前返 InternalError」
    //      已过时 —— IdentityService::link_account 早已实装 guest→user 升级)
    //   C-7 POST /auth/logout        → auth_handlers::logout (2026-10-03 已实装:
    //     dsid claim 加入 TokenClaims, 此前本端点无条件报错, access token 无法吊销)
    cfg.service(
        actix_web::web::scope("/auth")
            .route(
                "/token/exchange",
                actix_web::web::post().to(auth_handlers::token_exchange),
            )
            .route("/guest", actix_web::web::post().to(auth_handlers::guest))
            .route(
                "/refresh",
                actix_web::web::post().to(auth_handlers::refresh),
            )
            .route(
                "/link",
                actix_web::web::post().to(auth_handlers::link_account),
            )
            .route("/logout", actix_web::web::post().to(auth_handlers::logout)),
    );

    // 会话(9) — MVP Day 3+5 实装 C-8 + C-9 + C-10
    //   C-9 messages POST/GET    → lane-backend-core-3 实装
    //   C-8 conversations POST   → 已有实装(创建 dm/group/channel)
    //   C-8 conversations GET    → 已有实装(列出当前用户所有会话)
    //   C-10 conversations/{id}/members GET → 已有实装
    //   2026-10-03: GET conversations/{id} 不再走 placeholder —— 它此前被标
    //     「out-of-scope」, 但 `ConversationService::get` 早已实装, 挂着
    //     placeholder 只会让一个**能做的端点**看起来做不了。
    //
    //   2026-10-03 新增 4 个消息动作端点(per BasicDesign §7 + DetailedDesign
    //   端点→需求映射表): edit / recall / reactions / read。此前这四项
    //   **只有 WS 帧**, REST 侧完全没有 —— 服务端 SDK / 后台任务 / 非 WS
    //   客户端无法触达。四个 handler 全部只做「解析 → service → 状态码」,
    //   业务规则与 WS 路径共用同一条 service 校验链, 不重写。
    cfg.service(
        actix_web::web::scope("/conversations")
            .route("", actix_web::web::post().to(conversations::create))
            .route("", actix_web::web::get().to(conversations::list))
            .route("/{id}", actix_web::web::get().to(conversations::get))
            .route(
                "/{id}/messages",
                actix_web::web::post().to(messages::send_message),
            )
            .route(
                "/{id}/messages",
                actix_web::web::get().to(messages::list_messages),
            )
            .route(
                "/{id}/messages/{msg_id}",
                actix_web::web::patch().to(message_actions::edit_message),
            )
            .route(
                "/{id}/messages/{msg_id}/recall",
                actix_web::web::post().to(message_actions::recall_message),
            )
            .route(
                "/{id}/messages/{msg_id}/reactions",
                actix_web::web::post().to(message_actions::add_reaction),
            )
            .route(
                "/{id}/read",
                actix_web::web::post().to(message_actions::mark_read),
            )
            .route("/{id}/members", actix_web::web::get().to(members::list)),
    );

    // WebSocket (C-11 driver) — MVP Day 4 实装
    cfg.service(actix_web::web::scope("/ws").configure(crate::ws::router::configure));
}
