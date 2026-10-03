//! 健康检查 + 监控端点
//!
//! 依据: ImplementationSpec §3.1.7 + `DetailedDesign §5`
//! (`GET /readyz | 就绪检查(PG/Valkey/NATS 全部可达才 200)`)
//!
//! ## liveness 与 readiness 必须是两件事
//!
//! - `/healthz` = **进程活着吗**。答「活着」, 于是 k8s 不重启它。
//! - `/readyz`  = **能接流量吗**。答「不能」, 于是 k8s 把它从 Service 的
//!   endpoints 里摘掉, 流量转给别人。
//!
//! 若两者合并成一个(比如让 liveness 也查依赖), 数据库一抖, 全部副本同时被
//! 重启 —— 把「一个依赖不可用」放大成「整个服务不可用且重启中」。
//! 故本文件里 `healthz` **绝不**碰任何外部依赖。

use actix_web::web;
use actix_web::HttpResponse;
use serde_json::json;
use std::time::Duration;

/// readiness 探测 PG 的**硬上界**
///
/// 必须显著小于 k8s 探针的 `timeoutSeconds`(清单里没写, 默认 **1s**)。
/// 超时的后果不是「这次探测失败」, 而是探针超时 → liveness 也可能连环失败
/// → Pod 被反复重启 —— 一个本该只影响流量的依赖故障, 被放大成 CrashLoop。
///
/// 1s 上界里留出余量取 500ms: PG 在同机/同网络下 `SELECT 1` 是亚毫秒级,
/// 500ms 已经足够区分「健康」与「连不上/网络分区」。
const PG_PING_TIMEOUT: Duration = Duration::from_millis(500);

/// Liveness — 进程存活
///
/// **刻意不检查任何依赖**。理由见模块文档: 依赖抖动不应触发重启。
pub async fn healthz() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

/// Readiness — 能否接流量
///
/// ## 查什么, 不查什么(每一条都有理由, 不是随手取舍)
///
/// | 依赖 | 查不查 | 理由 |
/// |---|---|---|
/// | **PostgreSQL** | ✅ 查 | 唯一有**真实失败模式**的依赖: 网关每个业务端点都要它。查不到就等于「接了流量也全部 500」, 这正是 readiness 该拦下的情况 |
/// | NATS | ❌ 不查 | `NatsEventPublisher` 目前是 stub(`_client: None`, 见 `im-core/src/event/publisher.rs`), **没有任何连接可查**。查一个永远「可达」的空壳只会让响应体多一个恒为 true 的字段, 给人虚假的安全感 |
/// | Valkey | ❌ 不查 | **D-4 未落地**, 配置里没有对应字段, 代码里没有客户端。`DetailedDesign §5` 要求「全部可达才 200」, 但 Valkey 尚不存在 —— 若强行查, readyz 永远返 503, 整个部署起不来。宁可少查并显式声明, 也不要把 Pod 永久判死 |
///
/// NATS / Valkey 两项待 D-3 / D-4 落地后接上, 届时本函数的 `checks` 地图
/// 自然扩展, 契约不变。
pub async fn readyz(pool: web::Data<sqlx::PgPool>) -> HttpResponse {
    // 上界由 `PG_PING_TIMEOUT` 强制, 不依赖 sqlx 自身的连接超时 ——
    // 后者可能因为池里已有坏连接而拖得比 1s 更久。
    let pg = tokio::time::timeout(
        PG_PING_TIMEOUT,
        sqlx::query("SELECT 1").execute(pool.get_ref()),
    )
    .await;

    let (ok, detail) = match pg {
        Ok(Ok(_)) => (true, json!("ok")),
        Ok(Err(e)) => {
            // 记日志而不是把错误原文返回给调用方: 探针响应会进 k8s 事件与
            // 监控, 连接串之类的内容不该出现在那里。细节看服务端日志。
            tracing::warn!(error = %e, "readiness: postgres ping failed");
            (false, json!("unreachable"))
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = PG_PING_TIMEOUT.as_millis(),
                "readiness: postgres ping timed out"
            );
            (false, json!("timeout"))
        }
    };

    let mut checks = serde_json::Map::new();
    checks.insert("postgres".to_string(), detail);
    // 显式列出「没查」的依赖, 避免读响应的人以为「没提到就是查过了且通过」
    checks.insert("nats".to_string(), json!("not_checked_stub_publisher"));
    checks.insert("valkey".to_string(), json!("not_implemented_D4"));

    if ok {
        HttpResponse::Ok().json(json!({"status": "ready", "checks": checks}))
    } else {
        // 503: k8s 见到非 2xx 即把该 Pod 摘出 Service endpoints
        HttpResponse::ServiceUnavailable().json(json!({"status": "not_ready", "checks": checks}))
    }
}

/// Prometheus 指标(MVP: 手工暴露, 未接 prometheus exporter)
pub async fn metrics(hub: actix_web::web::Data<crate::ws::hub::WsHub>) -> HttpResponse {
    // 广播订阅数是本进程**唯一**能立刻回答的运营问题(WS 到底连上了几个),
    // 且它就挂在 `WsHub` 上, 不需要额外的指标框架即可暴露。
    //
    // 语义: **已订阅广播的连接数**。未鉴权的连接也算在内(它们在建连时就
    // subscribe 了), 所以这个值略大于「在线用户数」—— 排查时以
    // `ws::hub::Audience` 过滤后的实际投递为准。
    let subs = hub.subscriber_count();

    // 被丢弃的领域事件数 (D-3)。
    //
    // 2026-10-03 新增。这个指标的意义不是「监控一个正常运行的指标」, 而是
    // **让一个架构性缺口可见**: `NatsEventPublisher` 仍是 stub, 每条
    // `im.message.{created,recalled,deleted}` 都被丢弃。若这个数持续增长,
    // 就说明「跨 pod 事件同步」根本没在工作 —— 而从日志或返回值上完全看不出来
    // (stub 返回 `Ok(())`, 日志级别是默认不可见的 `debug`)。
    //
    // 面板上把 `rate(im_events_dropped_total[5m])` 画出来, 应当恒为 0;
    // 一旦 D-3 接线, 它会归零, 且之后任何非 0 都意味着真丢事件了。
    let dropped = im_core::event::publisher::dropped_event_count();

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(format!(
            "# MVP: prometheus exporter not yet enabled (set IM_PROMETHEUS_BIND to enable)\n\
             # HELP im_ws_broadcast_subscriptions 已订阅 WS 广播的连接数\n\
             # TYPE im_ws_broadcast_subscriptions gauge\n\
             im_ws_broadcast_subscriptions {subs}\n\
             # HELP im_events_dropped_total 因 D-3 stub 被丢弃的领域事件数(实现 NATS 后应恒为 0)\n\
             # TYPE im_events_dropped_total counter\n\
             im_events_dropped_total {dropped}\n"
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 响应体的最小契约 —— 只在测试里用, 故随测试走(不放在模块顶层,
    /// 否则非测试构建会因「从未构造」而触发 dead_code)。
    #[derive(serde::Deserialize)]
    struct ReadyBody {
        status: String,
        #[serde(default)]
        checks: serde_json::Map<String, serde_json::Value>,
    }

    /// 构造一个**连接永远建不起来**的池。
    ///
    /// 用 `connect_lazy` 而非 `connect`: 后者会当场失败并返回
    /// `Err(PoolTimedOut)`, 根本走不到 handler。这也更贴近生产 —— 池在
    /// 启动时建好(那时 PG 可达, 否则进程根本起不来), readiness 要捕捉的
    /// 是**之后**的不可用: PG 重启、网络分区、连接被中间件掐断。
    fn dead_pool() -> sqlx::PgPool {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_millis(200))
            .connect_lazy("postgres://nobody:nobody@127.0.0.1:1/none")
            .expect("connect_lazy 不会发起连接")
    }

    /// 把 readyz 挂到真实路由上跑 —— 顺带验证 `web::Data<PgPool>` 的接线,
    /// 而不只是「函数本身返回了什么」。直接调 handler 会漏掉「忘记注册
    /// app_data」这类错误(那种错误在编译期完全看不出来)。
    async fn readyz_via_route() -> (actix_web::http::StatusCode, ReadyBody) {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(dead_pool()))
                .route("/readyz", web::get().to(readyz)),
        )
        .await;

        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::get()
                .uri("/readyz")
                .to_request(),
        )
        .await;
        let status = resp.status();
        let bytes = actix_web::test::read_body(resp).await;
        (
            status,
            serde_json::from_slice(&bytes).expect("readyz 响应必须是合法 JSON"),
        )
    }

    /// 端口 1 上不会有 PG。连接会被拒或超时, 两种都算「不可达」。
    #[actix_web::test]
    async fn readyz_is_503_when_postgres_is_unreachable() {
        let (status, body) = readyz_via_route().await;
        assert_eq!(
            status,
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            "PG 不可达时必须 503 —— 返 200 等于把流量送给一个每个端点都会 500 的实例"
        );
        assert_eq!(body.status, "not_ready");
        assert_eq!(body.checks["postgres"], "unreachable");
    }

    /// 「没查的依赖」必须**显式**出现在响应里。
    ///
    /// 少了它, 读 `/readyz` 的人(运维或未来的健康检查脚本)会把「没提到」
    /// 误读成「查过了且通过」—— 而实际上 NATS 是 stub、Valkey 根本不存在。
    #[actix_web::test]
    async fn readyz_states_which_dependencies_were_not_checked() {
        let (_, body) = readyz_via_route().await;
        assert_eq!(body.checks["nats"], "not_checked_stub_publisher");
        assert_eq!(body.checks["valkey"], "not_implemented_D4");
    }

    /// 探针超时上界必须显著小于 k8s 默认 `timeoutSeconds: 1`。
    ///
    /// 这条是纯逻辑断言, 故意不连库: 它锁住的是一个**数值**。若有人把
    /// 500ms 调成 5s, 线上表现是探针持续超时 → Pod 反复重启, 而本地
    /// 任何测试都不会发现(因为没有 k8s 在跑)。
    #[test]
    fn pg_ping_timeout_is_well_under_the_default_probe_timeout() {
        assert!(
            PG_PING_TIMEOUT < Duration::from_secs(1),
            "PG 探测上界({:?})必须 < k8s 探针默认 timeoutSeconds(1s), \
             否则依赖抖动会变成 CrashLoop",
            PG_PING_TIMEOUT
        );
    }

    /// D-3 缺口必须**在 `/metrics` 里看得见**, 而不是只躺在日志里。
    ///
    /// 先 publish 一次把计数推上去, 再断言 `/metrics` 文本里出现了它。
    /// 锁住两件事: ① `publish()` 确实在计数(没被悄悄改回 no-op);
    /// ② 计数真的被暴露了(加了计数器却忘了挂到指标上, 是同一种静默)。
    #[actix_web::test]
    async fn metrics_expose_the_dropped_event_counter() {
        use im_core::event::publisher::{EventPublisher, NatsEventPublisher};

        let before = im_core::event::publisher::dropped_event_count();
        let pubr = NatsEventPublisher::connect("nats://stub:4222")
            .await
            .expect("stub publisher 不会失败");
        pubr.publish("im.message.created", b"{\"probe\":1}")
            .await
            .expect("stub 返回 Ok");
        assert_eq!(
            im_core::event::publisher::dropped_event_count(),
            before + 1,
            "stub publish 必须让丢弃计数 +1"
        );

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(crate::ws::hub::WsHub::new()))
                .route("/metrics", web::get().to(metrics)),
        )
        .await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::get()
                .uri("/metrics")
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let bytes = actix_web::test::read_body(resp).await;
        let text = String::from_utf8(bytes.to_vec()).expect("utf8");

        assert!(
            text.contains("im_events_dropped_total"),
            "/metrics 必须暴露丢弃计数, 否则 D-3 缺口继续隐身: {text}"
        );
        assert!(
            text.contains(&format!("im_events_dropped_total {}", before + 1)),
            "指标值必须是真实计数({}), 而非占位: {text}",
            before + 1
        );
    }
}
