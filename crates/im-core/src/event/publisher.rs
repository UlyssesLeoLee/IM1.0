//! EventPublisher — 异步事件发布抽象
//!
//! 依据: ImplementationSpec §7.4.5 + DetailedDesign §9.1

use async_trait::async_trait;

use im_common::AppError;
use std::sync::atomic::{AtomicU64, Ordering};

/// 进程级计数: 被 stub 丢弃的事件数
///
/// 2026-10-03 新增。此前 `publish()` 只发一条 `tracing::debug!` 并返回
/// `Ok(())` —— 而 `debug` 在默认日志级别下**不可见**, 于是「每条消息事件
/// 都被丢掉」这件事在整个运行期**没有任何外部表征**: 日志没有、返回值没有、
/// 指标没有。读代码的人会以为事件发出去了。
///
/// 静默丢弃比报错更糟, 因为它把一个架构性缺口(aux-04 §B.4 要求「publish
/// 事件供其他 pod 同步」, 而 D-3 未实装)伪装成了一个正常运行的系统。
/// 故改成: 计数 + `warn!` + `/metrics` 暴露。**不改变启动语义** —— 是否让
/// 网关硬依赖 NATS 属于部署决策, 不由本文件拍板。
static EVENTS_DROPPED: AtomicU64 = AtomicU64::new(0);

/// 读取被丢弃的事件数(供 `/metrics` 暴露)
pub fn dropped_event_count() -> u64 {
    EVENTS_DROPPED.load(Ordering::Relaxed)
}

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

/// NATS JetStream 实现
///
/// **当前是 stub**: `connect()` 不发起连接, `publish()` 丢弃全部事件。
/// 见 `dropped_event_count()` 与 D-3 缺口。
///
/// 为什么仍保留 `Option<Client>` 而不是删掉整个 struct: 接线时的形状已经
/// 定好(签名符合 `DetailedDesign §publisher.rs`), D-3 落地时只需把
/// `connect` 的注释代码取消 —— 但**在真正实现并测试之前, 本文件不声称
/// 事件已发出**。
pub struct NatsEventPublisher {
    _client: Option<async_nats::Client>,
}

impl NatsEventPublisher {
    /// **未连接任何东西** —— 保留 URL 形参是为了接线时不改调用点。
    pub async fn connect(url: &str) -> Result<Self, AppError> {
        tracing::warn!(
            nats_url = url,
            "D-3: NatsEventPublisher is a STUB — every publish() will be DROPPED. \
             Cross-pod event sync is NOT working. See docs/gap-ledger.md (D-3)."
        );
        Ok(Self { _client: None })
    }
}

#[async_trait]
impl EventPublisher for NatsEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        let n = EVENTS_DROPPED.fetch_add(1, Ordering::Relaxed) + 1;
        // warn 而非 debug: 默认日志级别下 debug 不可见, 那样这个缺口依然
        // 是静默的。warn 让「事件在丢」变成运维能看见的事。
        tracing::warn!(
            topic,
            bytes = payload.len(),
            dropped_total = n,
            "D-3 STUB: event DROPPED (no NATS client). Cross-pod sync not implemented."
        );
        Ok(())
    }
}
