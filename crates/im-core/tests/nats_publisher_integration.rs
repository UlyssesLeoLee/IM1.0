//! D-3 集成测试 —— 真实 NATS JetStream
//!
//! 依据: WBS D-3「EventPublisher NATS JetStream 真实实现」
//!      + aux-04 §B.4「转换必须 publish 事件供其他 pod 同步」这条**不变量**
//!
//! ## 为什么必须是集成测试
//!
//! `publish()` 曾经的实现是「打一条 `debug!` 然后返回 `Ok(())`」。**任何**只
//! 断言「返回值是 Ok」的测试都会通过 —— 而事件根本没发出去。要证伪它, 唯一
//! 办法是让**真的 NATS server** 参与, 然后从**服务端那一侧**读回证据。
//!
//! 所以本文件的决定性断言不是「publish 没报错」, 而是:
//! **发完之后, JetStream stream 的消息计数确实增加了。**
//! 这比「订阅者收到了」更强 —— 它证明事件被**持久化**, 而不只是被投递过一次。
//!
//! ## 静默跳过纪律 (与 `IM_REQUIRE_PG` 同构)
//!
//! `cargo test` **默认丢弃通过测试的 stdout**, 而「跳过」在 libtest 眼里就是
//! 「通过」—— 于是「NATS 相关测试全过」与「一个都没跑」在输出上完全无法区分。
//! 本仓已经为 PG 栽过一次(2026-10-03, 6 个 WS e2e 全部静默跳过)。
//! 故: `IM_NATS_URL` 连不上时, `IM_REQUIRE_NATS=1` 下**直接 panic**。
//! CI 的 integration job 挂了 NATS service container, 所以那里"连不上"
//! 只可能是 job 配错了。

use im_core::event::publisher::{
    published_event_count, EventPublisher, NatsEventPublisher, PublisherReadiness, EVENT_STREAM,
};

/// 取 NATS 地址; 连不上时按 `IM_REQUIRE_NATS` 决定跳过还是 panic
fn nats_url_or_skip(context: &str) -> Option<String> {
    let url = match std::env::var("IM_NATS_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            if std::env::var("IM_REQUIRE_NATS").as_deref() == Ok("1") {
                panic!(
                    "IM_REQUIRE_NATS=1 但没有 IM_NATS_URL({context})。这个环境**声称**要跑 \
                     NATS 测试, 缺地址只能是 job 配错了。静默跳过会让「跑过」与「没跑」\
                     无法区分 —— 见 docs/gap-ledger.md §1.19 / §1.27。"
                );
            }
            eprintln!("skip: {context} —— 未设 IM_NATS_URL");
            return None;
        }
    };
    Some(url)
}

async fn connect_or_skip(context: &str) -> Option<NatsEventPublisher> {
    let url = nats_url_or_skip(context)?;
    match NatsEventPublisher::connect(&url).await {
        Ok(p) => Some(p),
        Err(e) => {
            if std::env::var("IM_REQUIRE_NATS").as_deref() == Ok("1") {
                panic!("IM_REQUIRE_NATS=1 但连不上 NATS({context}): {e}");
            }
            eprintln!("skip: {context} —— 连不上 NATS: {e}");
            None
        }
    }
}

async fn stream_message_count(p: &NatsEventPublisher) -> u64 {
    // 注意 `Stream::info()` 返回的是 Future(要再 await 一次), 不是字段。
    p.jetstream()
        .get_stream(EVENT_STREAM)
        .await
        .unwrap_or_else(|e| panic!("get_stream({EVENT_STREAM}) 失败: {e}"))
        .info()
        .await
        .unwrap_or_else(|e| panic!("stream info 失败: {e}"))
        .state
        .messages
}

/// 决定性用例: 事件真的进了 JetStream
///
/// 断言的是**服务端 stream 的消息数**, 不是返回值。这才能区分
/// 「真的发出去了」与「打了个 debug 日志然后返回 Ok」。
#[tokio::test]
async fn published_event_lands_in_jetstream() {
    let Some(p) = connect_or_skip("published_event_lands_in_jetstream").await else {
        return;
    };

    let before = stream_message_count(&p).await;
    let published_before = published_event_count();

    p.publish(
        "im.message.created",
        br#"{"message_id":"11111111-1111-1111-1111-111111111111"}"#,
    )
    .await
    .expect("publish 应成功");

    let after = stream_message_count(&p).await;
    assert!(
        after > before,
        "publish 之后 JetStream stream {EVENT_STREAM} 的消息数必须增加: before={before} after={after}"
    );
    assert_eq!(
        published_event_count(),
        published_before + 1,
        "发布成功计数必须 +1"
    );
}

/// 防回归: 真实 publisher **不得**碰 stub 的丢弃计数
///
/// 这是「D-3 不能被改回静默 no-op」的核心守卫。改坏的方式通常是把
/// `NatsEventPublisher::publish` 的函数体换回「计数 + warn + Ok(())」——
/// 那种改法下, 上面的用例会因为 stream 计数不涨而变红; 而这条用例保证
/// 三种计数不会被悄悄混同。
#[tokio::test]
async fn real_publisher_never_counts_as_dropped() {
    let Some(p) = connect_or_skip("real_publisher_never_counts_as_dropped").await else {
        return;
    };

    let dropped_before = im_core::event::publisher::dropped_event_count();
    p.publish("im.message.recalled", b"{\"probe\":1}")
        .await
        .expect("publish 应成功");
    assert_eq!(
        im_core::event::publisher::dropped_event_count(),
        dropped_before,
        "真实 publisher 绝不能增加 stub 的丢弃计数 —— 两者语义不同, 混同会让 \
         「NATS 挂了」和「配置成 stub」在面板上无法区分"
    );
}

/// `connect()` 的 stream 创建必须**幂等**
///
/// 部署清单(`deploy/k3s/dev/nats.yaml`)**没有**预置 stream, 所以第一次
/// connect 会创建它; 而每个 pod 启动时都会 connect, 所以后续必须走 update
/// 分支而不报错。如果这里挂了, 第二个 pod 会起不来 —— 而单 pod 的测试
/// 永远发现不了。
#[tokio::test]
async fn connect_is_idempotent_across_restarts() {
    let Some(_) = connect_or_skip("connect_is_idempotent (first)").await else {
        return;
    };

    let url = std::env::var("IM_NATS_URL").expect("第一个 connect 已确认有地址");
    // 第二次连接: stream 已存在, create_or_update_stream 必须走 update 分支
    match NatsEventPublisher::connect(&url).await {
        Ok(_) => {}
        Err(e) => {
            panic!("stream 已存在时第二次 connect 必须成功(create_or_update_stream 幂等): {e}")
        }
    }
}

/// 反向用例: 连不上必须**报错**, 而不是静默退化成 stub
///
/// 这条不需要 NATS, 因此在**任何**环境都会跑。上一版实现的
/// `connect()` 无论传什么 URL 都返回 `Ok`, 于是「配了 `kind=nats` 但 NATS
/// 没起来」会得到一个看起来正常、事件全丢的进程。
#[tokio::test]
async fn connect_to_dead_endpoint_fails_instead_of_silently_stubbing() {
    // 127.0.0.1:1 —— 保留端口, 本机几乎不可能有服务在监听
    let started = std::time::Instant::now();
    let r = NatsEventPublisher::connect("nats://127.0.0.1:1").await;
    let elapsed = started.elapsed();

    assert!(
        r.is_err(),
        "连不上必须返回 Err; 返回 Ok 等于静默退化 —— 那正是 D-3 之前的行为"
    );
    // 有界: 不能挂死。connect 上界是 5s, 留出余量。
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "connect 失败必须有界, 实际耗时 {elapsed:?} —— 无限等待会把网关启动卡住"
    );
}

/// 真连上 NATS 时, 就绪判定必须是 `Ready`
///
/// 这条是 `/readyz` 纳入 NATS 之后**唯一**能把「读本地连接状态」和「NATS 真的
/// 接受了连接」对上的地方。`publisher.rs` 的单测只能验映射函数, 验不了
/// `connection_state()` 在真实连接后确实变成 `Connected` —— 而那正是
/// 「`/readyz` 会一直报 503」这种故障的唯一可能来源。
///
/// 守卫方向: 若有人把 `NatsEventPublisher::readiness` 写成恒返回 `Disconnected`,
/// 网关会**永远**不 ready, 而这类故障在所有其他测试里都看不出来。
#[tokio::test]
async fn connected_publisher_reports_ready() {
    let Some(p) = connect_or_skip("connected_publisher_reports_ready").await else {
        return;
    };

    assert_eq!(
        p.readiness(),
        PublisherReadiness::Ready,
        "真连上 NATS 的 publisher 必须报 Ready —— 否则 /readyz 会恒 503, \
         k8s 把所有实例都摘出 Service endpoints, 表现为「服务完全起不来」"
    );
    assert!(
        p.readiness().is_ready(),
        "同一个结论也必须能从 is_ready() 得到 —— 探针用的是它"
    );
}

/// 就绪判定**不发网络请求** —— 它必须瞬时返回
///
/// `im-gateway.yaml` 的 readinessProbe 没写 `timeoutSeconds`(默认 **1s**)。
/// 若这个检查带 IO, NATS 一慢就把探针拖到超时, 而探针超时与「判定为不健康」
/// 在 k8s 眼里是两件事(前者会累计 failureThreshold, 后者只是摘流量)。
///
/// 故断言它是**本地读**: 连上以后连续 128 次判定必须远快于 1s。
#[tokio::test]
async fn readiness_check_is_local_and_not_io_bound() {
    let Some(p) = connect_or_skip("readiness_check_is_local_and_not_io_bound").await else {
        return;
    };

    let started = std::time::Instant::now();
    for _ in 0..128 {
        let _ = p.readiness();
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "128 次就绪判定耗时 {elapsed:?} —— 这不该是 IO。若它变慢, 说明有人把 \
         网络请求塞进了探针路径"
    );
}

// 非法 subject 的校验在 `publisher.rs` 的 unit test 里覆盖
// (`subject_rejects_wildcards` / `subject_rejects_empty_token` /
// `subject_rejects_whitespace` / `illegal_subject_maps_to_internal_not_service_unavailable`)。
//
// 之所以放在那里而不是这里: subject 校验发生在**任何网络动作之前**, 所以它
// 不需要 NATS; 把它放进集成测试文件会让人误以为它需要 NATS, 从而在没有
// NATS 的环境下被静默跳过 —— 而这正是本文件要消灭的那类假绿。
