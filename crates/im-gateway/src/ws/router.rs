//! WS 路由注册 — 跟 /auth /conversations 同级
//!
//! 2026-09-19 lane-backend-core-2 (worker-C) 实装 + Mavis 父代理扩展
//!
//! 依据: 132-wbs.md §5.3.2 C-11 + aux-13 §2 (连接流程)

use actix_web::web;

/// 注册 /v1/ws 端点
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/ws")
            .route("", web::get().to(super::handler::ws_handler))
            .route("/", web::get().to(super::handler::ws_handler)),
    );
}