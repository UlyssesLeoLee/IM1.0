//! REST API 路由注册
//!
//! 依据: ImplementationSpec §3.1 + aux-13 §3
//!
//! MVP Day 3: 注册真实 C-8 / C-10 handlers(conversation + members)
//! C-3..C-7(auth 系列) / C-9(messages) / C-11(WsSession 主实装) 留给后续 worker

pub mod auth;
pub mod conversations;
pub mod error_response;
pub mod members;
pub mod state;

use actix_web::web;

/// 注册 /v1 路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // 鉴权(5 个端点) — MVP Day 3 仍走 placeholder,C-3..C-7 由后续 worker 实装
    cfg.service(
        actix_web::web::scope("/auth")
            .route("/token/exchange", actix_web::web::post().to(crate::placeholder::token_exchange))
            .route("/guest", actix_web::web::post().to(crate::placeholder::guest))
            .route("/refresh", actix_web::web::post().to(crate::placeholder::refresh))
            .route("/link", actix_web::web::post().to(crate::placeholder::link))
            .route("/logout", actix_web::web::post().to(crate::placeholder::logout)),
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
}
