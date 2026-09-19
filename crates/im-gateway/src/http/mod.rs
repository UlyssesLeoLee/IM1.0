//! REST API 路由注册
//!
//! 依据: ImplementationSpec §3.1 + aux-13 §3
//!
//! ## 注册范围 (per 132-wbs.md §5.3 + 2026-09-19 lane-backend-core-2 实装)
//! MVP Day 3: C-8 / C-10 / C-12 wired
//! MVP Day 4 (lane-backend-core-2, Mavis 父代理亲自推): C-3..C-7 + C-11 wired
//! 留给后续: C-9 messages handler (lane-messages)

pub mod auth;
pub mod auth_handlers;
pub mod conversations;
pub mod error_response;
pub mod members;
pub mod state;

use actix_web::web;

/// 注册 /v1 路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // 鉴权(5 个端点) — MVP Day 4 实装 C-3..C-7
    //   C-3 POST /auth/token/exchange → auth_handlers::token_exchange
    //   C-4 POST /auth/guest         → auth_handlers::guest
    //   C-5 POST /auth/refresh       → auth_handlers::refresh (per 138 §8 缺口 #8 fix 用 find_by_id)
    //   C-6 POST /auth/link          → auth_handlers::link_account (IdentityService::link_account 待 UPDATE 实现, 当前返 InternalError)
    //   C-7 POST /auth/logout        → auth_handlers::logout (per 138 §8 缺口 #2 device_session_id JWT claim 未实装, 兜底 401)
    cfg.service(
        actix_web::web::scope("/auth")
            .route("/token/exchange", actix_web::web::post().to(auth_handlers::token_exchange))
            .route("/guest", actix_web::web::post().to(auth_handlers::guest))
            .route("/refresh", actix_web::web::post().to(auth_handlers::refresh))
            .route("/link", actix_web::web::post().to(auth_handlers::link_account))
            .route("/logout", actix_web::web::post().to(auth_handlers::logout)),
    );

    // 会话(5) — MVP Day 3 实装 C-8 + C-10
    //   C-9 messages POST/GET    → 后续 worker 实装 (留给 lane-messages)
    //   C-8 conversations POST   → 本 PR 实装(创建 dm/group/channel)
    //   C-8 conversations GET    → 本 PR 实装(列出当前用户所有会话)
    //   C-10 conversations/{id}/members GET → 本 PR 实装
    cfg.service(
        actix_web::web::scope("/conversations")
            .route("", actix_web::web::post().to(conversations::create))
            .route("", actix_web::web::get().to(conversations::list))
            .route("/{id}", actix_web::web::get().to(crate::placeholder::conv_get))
            .route("/{id}/messages", actix_web::web::get().to(crate::placeholder::msg_list))
            .route(
                "/{id}/members",
                actix_web::web::get().to(members::list),
            ),
    );

    // WebSocket (C-11 driver) — MVP Day 4 实装
    cfg.service(
        actix_web::web::scope("/ws").configure(crate::ws::router::configure),
    );
}
