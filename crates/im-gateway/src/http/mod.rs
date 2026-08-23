//! REST API 路由注册
//!
//! 依据: ImplementationSpec §3.1
//!
//! MVP Day 1: 仅注册路由指向 placeholder,真实 handler 在后续 PR 提交

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    // 鉴权(5)
    cfg.service(
        actix_web::web::scope("/auth")
            .route("/token/exchange", actix_web::web::post().to(crate::placeholder::token_exchange))
            .route("/guest", actix_web::web::post().to(crate::placeholder::guest))
            .route("/refresh", actix_web::web::post().to(crate::placeholder::refresh))
            .route("/link", actix_web::web::post().to(crate::placeholder::link))
            .route("/logout", actix_web::web::post().to(crate::placeholder::logout)),
    );
    // 会话(5)
    cfg.service(
        actix_web::web::scope("/conversations")
            .route("", actix_web::web::post().to(crate::placeholder::conv_create))
            .route("", actix_web::web::get().to(crate::placeholder::conv_list))
            .route("/{id}", actix_web::web::get().to(crate::placeholder::conv_get))
            .route("/{id}/messages", actix_web::web::get().to(crate::placeholder::msg_list))
            .route("/{id}/members", actix_web::web::get().to(crate::placeholder::members_list)),
    );
}
