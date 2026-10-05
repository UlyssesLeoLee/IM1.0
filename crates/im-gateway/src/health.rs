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
use std::sync::Arc;
use std::time::Duration;

use im_core::event::publisher::EventPublisher;

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
/// | **NATS** | ✅ 查 | `DetailedDesign §5` 与 `ImplementationSpec §3.1.7` 都明确要求。2026-10-05 起**真的能查了**: `AppState` 之外的 `readiness_publisher` 让本端点拿到 `EventPublisher`, 由 `PublisherReadiness::readiness()` 读 `async_nats` 客户端的本地连接状态。此前这里写的是「拿不到 publisher 实例」—— 那在 D-3 落地后已经不成立, 留着就是一句假话 |
/// | Valkey | ❌ 不查 | **D-4 未落地**, 配置里没有对应字段, 代码里没有客户端。`DetailedDesign §5` 要求「全部可达才 200」, 但 Valkey 尚不存在 —— 若强行查, readyz 永远返 503, 整个部署起不来。宁可少查并显式声明, 也不要把 Pod 永久判死 |
///
/// ## 为什么 NATS 探针**不发网络请求**
///
/// `readiness()` 读的是 `async_nats::Client` 内部的 `watch` channel, 是**本地
/// 读**。换成「发一条消息等回包」会有两个问题: 给 NATS 增加探针流量; NATS
/// 假死(TCP 连着但不回包)时把探针一起拖到超时, 于是**依赖坏了反而让 readyz
/// 超时**, 而超时与「判定为不健康」在 k8s 眼里是两件事。
///
/// 本地读还天然满足探针的硬约束: `im-gateway.yaml` 的 readinessProbe 没写
/// `timeoutSeconds`(默认 **1s**), 任何一次带 IO 的检查都有被放大成 CrashLoop
/// 的风险 —— 这正是 `PG_PING_TIMEOUT` 要显式压到 500ms 的同一个理由。
///
/// ## 什么时候 NATS 不阻断 readiness
///
/// `IM_EVENT_PUBLISHER_KIND=stub` 时事件是**配置决定**不投递, 不是故障。此时
/// 报 `ok` 是撒谎(集成方会以为 NATS 正常), 报 503 是滥罚(每个开发部署都会
/// 永远不健康)。故报 `ok_stub_events_not_delivered` —— 既不撒谎也不滥罚,
/// 且读响应的人一眼能看出「事件没在发」。
pub async fn readyz(
    pool: web::Data<sqlx::PgPool>,
    publisher: web::Data<Arc<dyn EventPublisher>>,
) -> HttpResponse {
    // 上界由 `PG_PING_TIMEOUT` 强制, 不依赖 sqlx 自身的连接超时 ——
    // 后者可能因为池里已有坏连接而拖得比 1s 更久。
    let pg = tokio::time::timeout(
        PG_PING_TIMEOUT,
        sqlx::query("SELECT 1").execute(pool.get_ref()),
    )
    .await;

    let (pg_ok, detail) = match pg {
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

    // NATS —— 本地读连接状态, 无 IO, 无超时。
    let pub_state = publisher.readiness();
    if !pub_state.is_ready() {
        tracing::warn!(
            state = pub_state.as_wire_str(),
            "readiness: event publisher not connected; 事件正在进 DLQ"
        );
    }

    let mut checks = serde_json::Map::new();
    checks.insert("postgres".to_string(), detail);
    checks.insert("nats".to_string(), json!(pub_state.as_wire_str()));
    // 显式列出「没查」的依赖, 避免读响应的人以为「没提到就是查过了且通过」
    checks.insert("valkey".to_string(), json!("not_implemented_D4"));

    if pg_ok && pub_state.is_ready() {
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

    // 领域事件发布的五个计数 (D-3 实装于 2026-10-04, DLQ 增量于 2026-10-05)。
    //
    // 2026-10-03 只有一个 `dropped`, 用来让「事件在静默消失」这件事可见。
    // 2026-10-04 D-3 落地后必须**拆成三个**, 因为三种状态的处置完全不同:
    //
    //   published  正常增长                    —— 无需处理
    //   failed     NATS 超时 / 服务端拒绝        —— 事件没出去, 需查 NATS;
    //                                             **含每次重试**, 故大于 DLQ 数
    //   dropped    `kind=stub`, 显式选择不投递   —— 配置问题, 不是故障
    //
    // 2026-10-05 补 DLQ 后再加两个 —— 它们回答「事件去哪了」:
    //
    //   dlq                 重试耗尽, 已落到可恢复的地方(待人工重放)
    //   dlq_write_failed    连写 DLQ 都失败 —— 事件**真的永久没了**
    //
    // 把 failed 与 dropped 混成一个数, 会让「NATS 挂了」和「本来就配了 stub」
    // 在面板上长得一样, 于是真正的故障反而看不见了 —— 这正是原设计要消灭的
    // 那种「一切看起来都正常」。
    let published = im_core::event::publisher::published_event_count();
    let failed = im_core::event::publisher::failed_event_count();
    let dropped = im_core::event::publisher::dropped_event_count();

    // 2026-10-05 (DLQ 实装) 新增的两个计数。
    //
    // `dlq` 与 `write_failed` 绝不能合并成一个: 前者是「事件已经落到可恢复的
    // 地方」(可接受), 后者是「连兜底都没兜住, 事件**真的没了**」(不可接受)。
    // 合成一个数, 面板上就分不出「有积压待处理」和「正在丢数据」—— 而这两者
    // 的处置完全不同。
    let dlq = im_core::event::publisher::dlq_event_count();
    let dlq_write_failed = im_core::event::publisher::dlq_write_failed_count();

    // 2026-10-05(WS 投递索引改造)新增的三个 WS 指标。
    //
    // 为什么不沿用上一版的 `Lagged`: 上一版是单个 `broadcast` 通道, 慢客户端
    // 会撞上 `RecvError::Lagged`, 那是天然的可观测点。改成 per-connection 出站
    // 通道后没有 broadcast 就没有 Lagged, 若不另设计数, **丢帧会完全静默** ——
    // 客户端只会以为「对方没发言」, 服务端毫无线索。所以 `dropped` 不是锦上添花,
    // 是把丢掉的那份可观测性补回来。
    //
    // 另两个是为了让「投递索引」本身可观察:
    // - `im_ws_authenticated_connections` 能回答「连接都建好了却一个都没鉴权」
    //   这类问题(上一版区分不了: 建连与鉴权都只是一个 broadcast 订阅者)
    // - `im_ws_indexed_conversations` 是索引规模, 它的异常增长/归零都能看出来
    let ws_authed = hub.authenticated_count();
    let ws_indexed_convs = hub.indexed_conversation_count();
    let ws_dropped = hub.dropped_broadcast_count();

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(format!(
            "# MVP: prometheus exporter not yet enabled (set IM_PROMETHEUS_BIND to enable)\n\
             # HELP im_ws_broadcast_subscriptions 已建出站通道的 WS 连接数(含尚未鉴权的)\n\
             # TYPE im_ws_broadcast_subscriptions gauge\n\
             im_ws_broadcast_subscriptions {subs}\n\
             # HELP im_ws_authenticated_connections 已进入投递索引的 WS 连接数\n\
             # TYPE im_ws_authenticated_connections gauge\n\
             im_ws_authenticated_connections {ws_authed}\n\
             # HELP im_ws_indexed_conversations 投递索引中登记的会话数\n\
             # TYPE im_ws_indexed_conversations gauge\n\
             im_ws_indexed_conversations {ws_indexed_convs}\n\
             # HELP im_ws_broadcast_dropped_total 因收件端出站通道写满而丢弃的帧数(慢客户端; 客户端需经 REST 补齐)\n\
             # TYPE im_ws_broadcast_dropped_total counter\n\
             im_ws_broadcast_dropped_total {ws_dropped}\n\
             # HELP im_events_published_total 成功发布并拿到 JetStream ack 的领域事件数\n\
             # TYPE im_events_published_total counter\n\
             im_events_published_total {published}\n\
             # HELP im_events_publish_failed_total 发布尝试失败数(含每次重试, 故大于 DLQ 数); 耗尽后转 aux-08 DLQ 而非直接丢弃\n\
             # TYPE im_events_publish_failed_total counter\n\
             im_events_publish_failed_total {failed}\n\
             # HELP im_events_dropped_total 因 IM_EVENT_PUBLISHER_KIND=stub 被丢弃的领域事件数(配置问题, 非故障)\n\
             # TYPE im_events_dropped_total counter\n\
             im_events_dropped_total {dropped}\n\
             # HELP im_events_dlq_total 重试耗尽后写入 NATS DLQ 的事件数(aux-08; 可恢复, 待人工重放)\n\
             # TYPE im_events_dlq_total counter\n\
             im_events_dlq_total {dlq}\n\
             # HELP im_events_dlq_write_failed_total 连写 DLQ 都失败、事件**永久丢失**的次数; NATS 整体不可用时增长\n\
             # TYPE im_events_dlq_write_failed_total counter\n\
             im_events_dlq_write_failed_total {dlq_write_failed}\n"
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use im_core::event::publisher::StubEventPublisher;

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

    /// 把 readyz 挂到真实路由上跑 —— 顺带验证 `web::Data` 的两项接线,
    /// 而不只是「函数本身返回了什么」。直接调 handler 会漏掉「忘记注册
    /// app_data」这类错误(那种错误在编译期完全看不出来, 只在运行期变成 500)。
    ///
    /// `publisher` 显式传参而不是写死 stub: 这样同一个 helper 既能覆盖
    /// 「stub 部署」也能覆盖「NATS 断线」, 而后者才是这次改动的重点。
    async fn readyz_via_route_with(
        publisher: Arc<dyn EventPublisher>,
    ) -> (actix_web::http::StatusCode, ReadyBody) {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(dead_pool()))
                .app_data(web::Data::new(publisher))
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

    /// 默认走 stub —— 与本地/开发部署的真实形态一致。
    async fn readyz_via_route() -> (actix_web::http::StatusCode, ReadyBody) {
        readyz_via_route_with(Arc::new(StubEventPublisher::new())).await
    }

    /// 一个**永远不 ready**的发布器, 用于验证 503 那条分支。
    ///
    /// 不去真连一个坏 NATS: 那要么等连接超时(让每个用例都变慢), 要么依赖
    /// 一个「连不上」的 DNS/端口(在别的机器上可能反而连上了)。把这个状态
    /// 做成**显式的实现**, 是为了让「NATS 断线 -> 503」这条性质被**确定性**
    /// 地测到, 而不是寄希望于环境里恰好有个坏服务。
    struct UnreachablePublisher;

    #[async_trait::async_trait]
    impl EventPublisher for UnreachablePublisher {
        async fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), im_common::AppError> {
            unreachable!("readiness 用例不应触发 publish")
        }
        fn readiness(&self) -> im_core::event::publisher::PublisherReadiness {
            im_core::event::publisher::PublisherReadiness::Disconnected
        }
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
    /// 误读成「查过了且通过」—— 而实际上 Valkey 根本不存在。
    #[actix_web::test]
    async fn readyz_states_which_dependencies_were_not_checked() {
        let (_, body) = readyz_via_route().await;
        // Valkey 仍未查(D-4 未落地)。NATS **已查**, 由下一批用例覆盖 ——
        // 它曾经在这里被断言成 `not_checked`, 而那个断言本身就是过期的。
        assert_eq!(body.checks["valkey"], "not_implemented_D4");
    }

    /// NATS 断线必须让 readyz 返 503。
    ///
    /// 这是本次改动的**全部意义**: 此前这个端点报 `not_checked` 且恒返
    /// 「NATS 没问题」, 于是 NATS 挂掉时 k8s 继续往这个实例送流量, 而每条
    /// 领域事件都在重试后进 DLQ —— 集群看起来健康, 事件在悄悄积压。
    #[actix_web::test]
    async fn readyz_is_503_when_the_event_publisher_is_not_ready() {
        let (status, body) = readyz_via_route_with(Arc::new(UnreachablePublisher)).await;
        assert_eq!(
            status,
            actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
            "NATS 断线时必须 503 —— 继续接流量只会让事件继续往 DLQ 里积压"
        );
        assert_eq!(body.status, "not_ready");
        assert_eq!(body.checks["nats"], "disconnected");
    }

    /// stub 部署**不**该被判成不健康, 但也**不能**报成 `nats: ok`。
    ///
    /// 两个方向的错都有人会踩: 报 `ok` 让集成方以为事件在发(其实全被丢弃),
    /// 报 503 让每个本地开发部署永远起不来。
    #[actix_web::test]
    async fn readyz_does_not_fail_the_pod_for_a_deliberate_stub_publisher() {
        // `_status` 是**刻意**不用的: PG 在这个用例里是 `dead_pool`, 所以
        // 响应必然是 503, 而那个 503 是 PG 造成的 —— 断言它等于把两件事混在
        // 一起, 分不清是「stub 放行了」还是「PG 挡住了」。
        //
        // 要验「stub 本身不阻断 readiness」, 需要一个 **PG 可达**的池, 那属于
        // 集成层(要真 PG), 不在本文件。`PublisherReadiness::is_ready()` 的
        // 单测锁住了判定本身; 这里锁住的是「响应里那串字」。
        let (_status, body) = readyz_via_route().await;
        assert_eq!(body.checks["nats"], "ok_stub_events_not_delivered");
        assert_ne!(
            body.checks["nats"], "ok",
            "stub 报 `ok` 等于对集成方撒谎: 事件其实一条都没发出去"
        );
    }

    /// 两个原因要能**分别**从响应里读出来。
    ///
    /// 只有一个 `status: not_ready` 时, 运维无法区分「数据库挂了」与
    /// 「NATS 挂了」—— 而两者的处置完全不同(前者查 PG, 后者查 NATS)。
    #[actix_web::test]
    async fn readyz_reports_pg_and_publisher_failures_independently() {
        let (status, body) = readyz_via_route_with(Arc::new(UnreachablePublisher)).await;
        assert_eq!(status, actix_web::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body.checks["postgres"], "unreachable",
            "PG 不可达时必须如实报 postgres, 不能被 NATS 的失败掩盖"
        );
        assert_eq!(body.checks["nats"], "disconnected");
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

    /// 事件发布的三个计数必须**分别**出现在 `/metrics` 里。
    ///
    /// 先用 stub publish 一次把 dropped 推上去, 再断言文本里出现了它。
    /// 锁住两件事: ① `publish()` 确实在计数(没被悄悄改回 no-op);
    /// ② 计数真的被暴露了(加了计数器却忘了挂到指标上, 是同一种静默)。
    ///
    /// 同时锁住第三件事: published / failed / dropped **是三个不同的指标名**。
    /// D-3 实装前只有一个 `dropped`; 若有人图省事把它们合回一个, 断言会红 ——
    /// 因为「NATS 挂了」与「配置成 stub」在面板上必须是两种样子。
    #[actix_web::test]
    async fn metrics_expose_the_dropped_event_counter() {
        use im_core::event::publisher::{EventPublisher, StubEventPublisher};

        let before = im_core::event::publisher::dropped_event_count();
        let pubr = StubEventPublisher::new();
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
        // 只断言「> before」而不是「== before+1」。
        //
        // 这个计数器是**进程级全局**(`publisher.rs` 里的 static AtomicU64), 而
        // libtest 在同一进程内并行跑用例 —— 本二进制里那 26 个 e2e 都经
        // `test_support::e2e_pool` 持有 `StubEventPublisher`, 发消息时也会
        // 递增它。所以精确相等是一个**必然 flaky** 的断言(本机全绿、CI 上偶发
        // 红的典型形态)。`value > before` 仍然锁住了要锁的性质: 计数真的动过,
        // 且不是一个占位常量。
        let line = text
            .lines()
            .find(|l| l.starts_with("im_events_dropped_total "))
            .unwrap_or_else(|| panic!("/metrics 里没有 im_events_dropped_total 的数据行: {text}"));
        let value: u64 = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_else(|| panic!("无法解析指标值: {line}"))
            .parse()
            .unwrap_or_else(|e| panic!("指标值不是整数({line}): {e}"));
        assert!(
            value > before,
            "指标值必须至少增长到 {}(实测 {value}) —— 说明计数真的被接到了 /metrics, \
             而非占位: {text}",
            before + 1
        );
        // D-3 实装后另外两个指标必须**各自独立**存在。合并成一个会让
        // 「NATS 不可用」与「显式配了 stub」在面板上无法区分。
        for name in [
            "im_events_published_total",
            "im_events_publish_failed_total",
        ] {
            assert!(
                text.contains(name),
                "/metrics 必须同时暴露 {name}, 否则真实发布故障会与 stub 混为一谈: {text}"
            );
        }

        // 2026-10-05 DLQ: 两个计数必须都在, 且**语义相反地重要**。
        //
        // `write_failed` 是本文件里唯一能说出「有多少事件彻底没了」的指标。
        // 它若没被挂到 /metrics, NATS 整体不可用时的数据丢失就完全不可见 ——
        // 而 `publish_failed` 在涨, 看上去像「在重试, 等会就好」。
        //
        // 断言用**前后差**而不是绝对值 0: libtest 在本进程内并行跑用例, 任何
        // 「某个别的测试让 NATS 抖了一下」都会让绝对值断言假红。差值断言测的
        // 是同一个性质(本用例这次 stub publish 没有碰 DLQ), 且是确定的。
        let dlq_before = im_core::event::publisher::dlq_event_count();
        let dlq_failed_before = im_core::event::publisher::dlq_write_failed_count();
        StubEventPublisher::new()
            .publish("im.probe.dlq_wiring", b"{}")
            .await
            .expect("stub 返回 Ok");
        assert_eq!(
            im_core::event::publisher::dlq_event_count(),
            dlq_before,
            "stub 是**配置选择**不是故障, 绝不该进 DLQ —— 否则运维会去排障一个 \
             根本不存在的问题"
        );
        assert_eq!(
            im_core::event::publisher::dlq_write_failed_count(),
            dlq_failed_before,
            "stub 同理不该动「永久丢失」计数"
        );

        for name in ["im_events_dlq_total", "im_events_dlq_write_failed_total"] {
            assert!(
                text.contains(name),
                "/metrics 必须暴露 {name}: 事件发布失败若没进 DLQ, 没有任何外部表征。\
                 加了计数器却忘挂 /metrics, 与没加计数器是同一种静默: {text}"
            );
            let line = text
                .lines()
                .find(|l| l.starts_with(&format!("{name} ")))
                .unwrap_or_else(|| panic!("/metrics 里没有 {name} 的数据行: {text}"));
            let value: u64 = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_else(|| panic!("无法解析 {name} 的值: {line}"))
                .parse()
                .unwrap_or_else(|e| panic!("{name} 的值不是整数({line}): {e}"));
            assert!(
                value >= dlq_before.min(dlq_failed_before),
                "{name} 的值 {value} 看起来不像进程级累计值 —— 它必须真接到了 \
                 /metrics 而不是硬编码常量: {line}"
            );
        }
    }
}
