//! EventPublisher — 异步事件发布抽象
//!
//! 依据: ImplementationSpec §7.4.5 + DetailedDesign §9.1
//!
//! ## D-3 已实装 (2026-10-04)
//!
//! 此前 `NatsEventPublisher` 是 stub: `connect()` 不发起连接, `publish()` 把
//! 每条事件丢掉并返回 `Ok(())`。aux-04 §B.4 把「publish 事件供其他 pod 同步」
//! 写成**不变量**, 而这条不变量从未被满足 —— 且返回值 / 日志 / 指标三条渠道
//! 全都指向「正常」。现在改为真实的 NATS **JetStream** 发布
//! (`BasicDesign` 技术栈基线 + WBS D-3 + `ImplementationSpec §7.4.5`
//! "publish via NATS JetStream" + `deploy/k3s/dev/nats.yaml` 的 `--jetstream`)。
//!
//! ## 三个刻意的设计决定
//!
//! 1. **发布等 JetStream ack, 但有界超时。** `DetailedDesign §9.1` 要求
//!    「失败不阻塞 ack」, 而 `jetstream::Context::publish()` 返回的是要等服务端
//!    ack 的 future —— 不设上界的话, NATS 变慢会**顺着业务请求路径**传导成
//!    延迟尖峰。故用 `tokio::time::timeout` 给出确定上界, 超时按失败计。
//! 2. **Stream 由程序幂等创建, 不依赖运维预置。** `nats.yaml` 只传了
//!    `--jetstream --store_dir=/data`, **没有**任何 stream 配置; 安装包的
//!    `install.sh` 也不建 stream。若假定 stream 已存在, 则首次部署时每条事件
//!    都会拿到 `no responders` —— 又一次静默失效。故 `connect()` 里调
//!    `create_or_update_stream`（先 update, NotFound 时 create）。
//! 3. **`kind=stub` 与 `kind=nats` 是两种不同的类型。** 此前两者都调
//!    `NatsEventPublisher::connect`, 于是「我要 stub」这句话的实现是
//!    「去连 nats://stub:4222」。现在拆出 `StubEventPublisher`, stub 路径
//!    真的不连任何东西, 而 `kind=nats` 连不上会**启动失败** —— 配了 NATS 却
//!    静默退化成 stub, 正是本文件长期存在的那类缺陷。
//!
//! ## 未实装 (诚实声明)
//!
//! `DetailedDesign §9.1` 还要求「失败...写入 DLQ」。**DLQ 未实装。**
//! 当前失败路径是: 有界超时 / 服务端错误 -> 计数 + `warn!` + 返回
//! `Err(ServiceUnavailable)`, 调用方(`MessageService`)只记日志不阻塞 ack。
//! 即失败**可见、可测量**, 但**不可恢复** —— 真实重试依赖 V1+ outbox
//! (仓内 `message/service.rs` 的既有注释也这么写)。

use async_trait::async_trait;
use bytes::Bytes;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use im_common::AppError;

// ---------------------------------------------------------------------------
// 计数器 —— 三个语义不同的量, 混用一个会让「丢在哪」不可区分
// ---------------------------------------------------------------------------

/// 成功发布并拿到 JetStream ack 的事件数
static EVENTS_PUBLISHED: AtomicU64 = AtomicU64::new(0);
/// 尝试发布但失败(超时 / 服务端拒绝 / subject 非法)的事件数
static EVENTS_PUBLISH_FAILED: AtomicU64 = AtomicU64::new(0);
/// 被 [`StubEventPublisher`] 丢弃的事件数
///
/// 2026-10-03 新增。`publish()` 此前只发一条 `tracing::debug!` 并返回
/// `Ok(())` —— 而 `debug` 在默认日志级别下**不可见**, 于是「每条消息事件
/// 都被丢掉」这件事在整个运行期**没有任何外部表征**。
///
/// 2026-10-04 语义收窄: 这个计数器现在**只**统计显式选择的 stub。真实
/// publisher 的失败走 `EVENTS_PUBLISH_FAILED` —— 两者混同会让「配置了 stub」
/// 和「NATS 挂了」在指标上长得一样, 而它们的处置完全不同。
static EVENTS_DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn published_event_count() -> u64 {
    EVENTS_PUBLISHED.load(Ordering::Relaxed)
}

pub fn failed_event_count() -> u64 {
    EVENTS_PUBLISH_FAILED.load(Ordering::Relaxed)
}

pub fn dropped_event_count() -> u64 {
    EVENTS_DROPPED.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 建立连接的上界。超过即视为 NATS 不可用。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 等 JetStream ack 的上界。`DetailedDesign §9.1` 要求发布失败不阻塞业务
/// ack, 所以这个等待必须**有界**; 见文件头「设计决定 1」。
const PUBLISH_ACK_TIMEOUT: Duration = Duration::from_secs(3);

/// 事件 stream 名
pub const EVENT_STREAM: &str = "IM_EVENTS";

/// 事件 subject 过滤器。覆盖 `BasicDesign §8` 的全部 topic
/// (`im.message.*` / `im.conversation.*` / `im.presence.*` / `im.identity.*`
/// / `im.auth.*`)。
pub const EVENT_SUBJECT_FILTER: &str = "im.>";

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// 发布事件; `payload` 是**已序列化的字节**
    ///
    /// ## 2026-10-03 对齐规范 —— 签名从 `&MessageCreatedEvent` 改为 `&[u8]`
    ///
    /// 此前本方法写死接收 `&MessageCreatedEvent`。这不只是风格问题, 它让
    /// **除 created 之外的任何事件都发不出去**: aux-04 §B.4 不变量要求
    /// 「转换必须 publish 事件 `im.message.{recalled,deleted}` 供其他 pod
    /// 同步」, 但 trait 的签名让这条不变量**在类型层面就无法满足** ——
    /// 加 `MessageRecalledEvent` 时编译器直接拒绝。
    ///
    /// `DetailedDesign.md` §publisher.rs 本来就写的是
    /// `async fn publish(&self, topic: &str, payload: &[u8])`。即**规范是对的,
    /// 实现偏离了规范**, 故此处是实现回归规范, 不是引入新设计。
    ///
    /// 不用 `&serde_json::Value` 是因为规范明确写的是字节; 序列化在调用方做,
    /// 各事件自己选格式。
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError>;
}

/// subject 合法性检查
///
/// NATS 规则: 不允许空白, 不允许空 token(前后点或连续点), 且**发布时不允许
/// 通配符**。提前判掉这些, 是为了让「调用方拼错了 topic」变成一条能指到
/// 调用点的错误, 而不是服务端一句含糊的 `invalid subject`。
///
/// 这属于**编程错误**而非运行时故障, 故返回 `Internal` 而非 `ServiceUnavailable`
/// —— 两者都在已注册的 21 项错误码内(500 / 503), 不需要新增错误码。
fn validate_subject(topic: &str) -> Result<(), AppError> {
    if topic.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject is empty"
        )));
    }
    if topic.chars().any(char::is_whitespace) {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject contains whitespace: {topic:?}"
        )));
    }
    if topic.contains('*') || topic.contains('>') {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject must not contain wildcards when publishing: {topic:?}"
        )));
    }
    if topic.split('.').any(str::is_empty) {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject has an empty token: {topic:?}"
        )));
    }
    Ok(())
}

/// 幂等确保事件 stream 存在
///
/// `create_or_update_stream` 内部先 `update_stream`, 只在 `NotFound` 时才
/// `create_stream`, 所以重复调用安全。**这一步是必需的**: 部署清单
/// (`deploy/k3s/dev/nats.yaml`) 没有预置 stream, 假定它存在等于假定事件
/// 永远拿 `no responders` —— 又一次静默失效。
async fn ensure_event_stream(js: &async_nats::jetstream::Context) -> Result<(), AppError> {
    let cfg = async_nats::jetstream::stream::Config {
        name: EVENT_STREAM.to_string(),
        subjects: vec![EVENT_SUBJECT_FILTER.to_string()],
        ..Default::default()
    };
    js.create_or_update_stream(cfg).await.map_err(|e| {
        AppError::ServiceUnavailable(format!(
            "ensure NATS JetStream stream {EVENT_STREAM} (subjects {EVENT_SUBJECT_FILTER}): {e}"
        ))
    })?;
    Ok(())
}

/// 真实的 NATS JetStream 发布器 (D-3)
pub struct NatsEventPublisher {
    client: async_nats::Client,
    js: async_nats::jetstream::Context,
}

impl NatsEventPublisher {
    /// 连接 NATS 并确保事件 stream 存在
    ///
    /// 失败即 `Err`: 调用方(`main.rs`)据此**拒绝启动**。配了 `kind=nats`
    /// 却静默退化成 stub, 是本文件长期存在的那类缺陷, 不再重复。
    pub async fn connect(url: &str) -> Result<Self, AppError> {
        let client = match tokio::time::timeout(CONNECT_TIMEOUT, async_nats::connect(url)).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                return Err(AppError::ServiceUnavailable(format!(
                    "connect to NATS at {url}: {e}"
                )))
            }
            Err(_) => {
                return Err(AppError::ServiceUnavailable(format!(
                    "connect to NATS at {url}: timed out after {CONNECT_TIMEOUT:?}"
                )))
            }
        };

        let js = async_nats::jetstream::new(client.clone());
        ensure_event_stream(&js).await?;

        tracing::info!(
            nats_url = url,
            stream = EVENT_STREAM,
            subjects = EVENT_SUBJECT_FILTER,
            "EventPublisher = NATS JetStream (D-3 实装)"
        );
        Ok(Self { client, js })
    }

    /// 底层 client —— 供集成测试订阅断言用
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }

    /// JetStream context —— 供集成测试直接查 stream 状态用
    ///
    /// 查 stream 的 `state.messages` 比「订阅者收到了」是**更强**的断言:
    /// 它证明事件被服务端**持久化**了, 而不只是被投递过一次。
    pub fn jetstream(&self) -> &async_nats::jetstream::Context {
        &self.js
    }
}

#[async_trait]
impl EventPublisher for NatsEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        validate_subject(topic)?;

        let js = self.js.clone();
        let subject = topic.to_string();
        let body = Bytes::copy_from_slice(payload);

        // 外层 Result = 发送本身失败; 内层 future = 等服务端 ack。
        let sent = match tokio::time::timeout(PUBLISH_ACK_TIMEOUT, js.publish(subject, body)).await
        {
            Err(_) => {
                let n = EVENTS_PUBLISH_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::error!(
                    topic,
                    bytes = payload.len(),
                    failed_total = n,
                    timeout = ?PUBLISH_ACK_TIMEOUT,
                    "event publish did not get a JetStream ack in time; no DLQ (V1+ outbox)"
                );
                return Err(AppError::ServiceUnavailable(format!(
                    "publish {topic}: no JetStream ack within {PUBLISH_ACK_TIMEOUT:?}"
                )));
            }
            Ok(Err(e)) => {
                let n = EVENTS_PUBLISH_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::error!(
                    topic,
                    bytes = payload.len(),
                    failed_total = n,
                    error = %e,
                    "event publish rejected by NATS; no DLQ (V1+ outbox)"
                );
                return Err(AppError::ServiceUnavailable(format!(
                    "publish {topic}: {e}"
                )));
            }
            Ok(Ok(ack)) => ack,
        };

        match sent.await {
            Ok(_ack) => {
                let n = EVENTS_PUBLISHED.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::debug!(
                    topic,
                    bytes = payload.len(),
                    published_total = n,
                    "event published"
                );
                Ok(())
            }
            Err(e) => {
                let n = EVENTS_PUBLISH_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::error!(
                    topic,
                    bytes = payload.len(),
                    failed_total = n,
                    error = %e,
                    "event publish ack was negative; no DLQ (V1+ outbox)"
                );
                Err(AppError::ServiceUnavailable(format!(
                    "publish {topic}: {e}"
                )))
            }
        }
    }
}

/// 显式选择的空实现 —— **只在 `event_publisher.kind=stub` 时使用**
///
/// 拆出这个类型是必要的: 此前 `kind=stub` 与 `kind=nats` 走的是同一个
/// `NatsEventPublisher::connect`, 于是「我要 stub」的实现是「去连
/// `nats://stub:4222`」。那种形状下, 打开真实 NATS 后 `kind=stub` 也会
/// 意外开始发事件, 而测试里的 `nats://stub:4222` 也会在连接失败后
/// 表现成「stub 坏了」。
///
/// 它**故意**返回 `Ok(())`: stub 的语义是「接受事件但不投递」, 调用方
/// 无需为部署形态改变错误处理。但它**必须**计数 + `warn!` —— 静默丢弃
/// 正是这个文件从 2026-08 起最大的问题。
#[derive(Debug, Default, Clone, Copy)]
pub struct StubEventPublisher;

impl StubEventPublisher {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EventPublisher for StubEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        let n = EVENTS_DROPPED.fetch_add(1, Ordering::Relaxed) + 1;
        // warn 而非 debug: 默认日志级别下 debug 不可见, 那样这个缺口依然
        // 是静默的。warn 让「事件在丢」变成运维能看见的事。
        tracing::warn!(
            topic,
            bytes = payload.len(),
            dropped_total = n,
            "event DROPPED: event_publisher.kind=stub (no NATS). Cross-pod sync is OFF by configuration."
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- subject 校验: 这些不需要 NATS 就能测, 且是最容易写错的边界 ----

    #[test]
    fn subject_rejects_empty() {
        assert!(validate_subject("").is_err());
    }

    #[test]
    fn subject_rejects_whitespace() {
        assert!(validate_subject("im.message.created ").is_err());
        assert!(validate_subject("im message created").is_err());
        assert!(validate_subject("im.message\tcreated").is_err());
    }

    #[test]
    fn subject_rejects_wildcards() {
        // `>` 与 `*` 在发布侧非法(它们只对订阅合法)
        assert!(validate_subject("im.>").is_err());
        assert!(validate_subject("im.message.*").is_err());
    }

    #[test]
    fn subject_rejects_empty_token() {
        assert!(validate_subject("im..created").is_err());
        assert!(validate_subject(".im.created").is_err());
        assert!(validate_subject("im.created.").is_err());
    }

    #[test]
    fn subject_accepts_the_real_topics() {
        // BasicDesign §8 的全部 topic 形状
        for t in [
            "im.message.created",
            "im.message.edited",
            "im.message.recalled",
            "im.message.reaction_added",
            "im.conversation.created",
            "im.conversation.member_joined",
            "im.conversation.member_left",
            "im.presence.changed",
            "im.identity.state_changed",
            "im.auth.token_rotated",
        ] {
            assert!(validate_subject(t).is_ok(), "{t} should be accepted");
        }
    }

    // ---- 错误码归属: 不得引入 21 项注册表之外的新码 ----

    #[test]
    fn illegal_subject_maps_to_internal_not_service_unavailable() {
        let e = validate_subject("im.>").unwrap_err();
        assert_eq!(e.code(), im_common::ErrorCode::InternalError);
    }

    // ---- 计数相关用例: 必须串行 ----
    //
    // 三个计数是**进程级全局** `static AtomicU64`(生产上 `/metrics` 就要这个
    // 进程级视图), 而 libtest 在**同一进程内并行跑测试**。若两个用例都断言
    // 「delta == N」而不加锁, 彼此的 publish 会插进对方的 before/assert 之间,
    // 把 `before + 2` 变成 `before + 3`。
    //
    // 实测踩过: run 37261532690 在 GitHub runner 上
    // `stub_counts_every_dropped_event` 失败于 publisher.rs:388, 而本机
    // 424 passed 全绿 —— 纯时序差异。故凡断言全局计数 delta 的用例都必须
    // 先拿这把锁。
    //
    // 用 `tokio::sync::Mutex` 而非 `std::sync::Mutex`: 锁要**跨 `.await`**
    // 持有(`publish` 是 async), 而 `clippy::await_holding_lock` 会把
    // 「std Mutex 的 guard 跨 await」判为 correctness 问题 —— 在多线程
    // runtime 上那确实可能死锁, 且 `-D warnings` 下直接编译失败。
    static COUNTER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    // ---- stub: 计数必须真的累加(防"改回静默 no-op") ----

    #[tokio::test]
    async fn stub_counts_every_dropped_event() {
        let _guard = COUNTER_LOCK.lock().await;
        let before = dropped_event_count();
        let p = StubEventPublisher::new();
        p.publish("im.message.created", b"{\"a\":1}").await.unwrap();
        p.publish("im.message.recalled", b"{}").await.unwrap();
        assert_eq!(
            dropped_event_count(),
            before + 2,
            "stub 必须逐条计数 —— 这正是它存在的理由"
        );
    }

    #[tokio::test]
    async fn stub_does_not_touch_the_real_publisher_counters() {
        let _guard = COUNTER_LOCK.lock().await;
        // 三种语义必须分开, 否则「配了 stub」与「NATS 挂了」在指标上无法区分
        let pub_before = published_event_count();
        let fail_before = failed_event_count();
        let p = StubEventPublisher::new();
        p.publish("im.message.created", b"{}").await.unwrap();
        assert_eq!(published_event_count(), pub_before);
        assert_eq!(failed_event_count(), fail_before);
    }
}
