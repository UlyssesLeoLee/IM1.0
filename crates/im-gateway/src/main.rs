//! im-gateway 入口
//!
//! 依据: ImplementationSpec §7.5 + DetailedDesign §2
//!
//! ## 启动流程(MVP Day 1 简化版)
//! 1. 启动 actix HTTP server (8080) + 占位路由
//! 2. 真实业务路由(handler + gRPC client)在后续 PR 提交
//!
//! 本文件暂以"可启动 + 可路由"为最低目标,完整配置加载见后续 PR

use actix_web::{web, App, HttpServer};

mod error;
mod health;
mod http;
mod placeholder;
mod ws;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // MVP Day 1: 硬编码端口 8080,真实 config 加载见后续 PR (D-1 WBS)
    let http_port: u16 = std::env::var("IM_HTTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        http_port,
        "im-gateway starting (MVP Day 3: C-8/C-10/C-12 wired, AppState wiring deferred to D-1)"
    );

    // MVP Day 3 note: AppState(包含 ConversationService + TokenService + PgPool)
    // 留给 D-1 (config 加载) 实装。本 PR(C-8/C-10/C-12)实装:
    // - HTTP handlers(conversations + members)+ Bearer auth extractor
    // - WS heartbeat module + WsSession 骨架(C-12 接口预留)
    // D-1 PR 完成后,这里会替换 placeholder 为 AppState 注入,
    // 并把 conv_create / conv_list / members_list 从 placeholder 切到真实 handler
    // (目前已在 http::configure 注册真实 handler,要求 D-1 先 wire up state)。
    let _ = (http_port, ws::heartbeat::HeartbeatConfig::default());

    HttpServer::new(move || {
        App::new()
            .service(web::scope("/v1").configure(http::configure))
            .route("/healthz", web::get().to(health::healthz))
            .route("/readyz", web::get().to(health::readyz))
            .route("/metrics", web::get().to(health::metrics))
    })
    .bind(("0.0.0.0", http_port))?
    .run()
    .await
}
