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
// 守门 #1 缺口台账: F-4 (healthz/readyz) 依赖 F-2/F-3 (K3s 部署), 路由尚未接线。
// 保留不删; per docs/Project-Status.md 1.1.1 占位符保留约定。
#[allow(dead_code)]
pub async fn readyz() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ready"}))
}

/// Prometheus 指标(MVP 占位)
pub async fn metrics(hub: actix_web::web::Data<crate::ws::hub::WsHub>) -> HttpResponse {
    // 广播订阅数是本进程**唯一**能立刻回答的运营问题(WS 到底连上了几个),
    // 且它就挂在 `WsHub` 上, 不需要额外的指标框架即可暴露。
    //
    // 语义: **已订阅广播的连接数**。未鉴权的连接也算在内(它们在建连时就
    // subscribe 了), 所以这个值略大于「在线用户数」—— 排查时以
    // `ws::hub::Audience` 过滤后的实际投递为准。
    let subs = hub.subscriber_count();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(format!(
            "# MVP: prometheus exporter not yet enabled (set IM_PROMETHEUS_BIND to enable)\n\
             # HELP im_ws_broadcast_subscriptions 已订阅 WS 广播的连接数\n\
             # TYPE im_ws_broadcast_subscriptions gauge\n\
             im_ws_broadcast_subscriptions {subs}\n"
        ))
}
