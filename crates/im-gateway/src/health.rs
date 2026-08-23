//! 健康检查 + 监控端点
//!
//! 依据: ImplementationSpec §3.1.7

use actix_web::HttpResponse;
use serde_json::json;

/// Liveness — 进程存活
pub async fn healthz() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

/// Readiness — 进程 + 依赖全部就绪
/// MVP: 只检查进程;V1 加上 PG/Valkey/NATS 连接检查
pub async fn readyz() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ready"}))
}

/// Prometheus 指标(MVP 占位)
pub async fn metrics() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body("# MVP: prometheus exporter not yet enabled (set IM_PROMETHEUS_BIND to enable)\n")
}
