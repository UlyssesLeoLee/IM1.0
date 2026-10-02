//! EventPublisher — 异步事件发布抽象
//!
//! 依据: ImplementationSpec §7.4.5 + DetailedDesign §9.1

use async_trait::async_trait;

use im_common::AppError;

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
/// MVP: 直接 publish,JetStream 持久化作为 V1+ 增强
pub struct NatsEventPublisher {
    _client: Option<async_nats::Client>,
}

impl NatsEventPublisher {
    pub async fn connect(_url: &str) -> Result<Self, AppError> {
        // 留待 MVP 编码阶段实装
        // let client = async_nats::connect(url).await
        //     .map_err(|e| AppError::ServiceUnavailable(format!("nats connect: {}", e)))?;
        // Ok(Self { _client: Some(client) })
        Ok(Self { _client: None })
    }
}

#[async_trait]
impl EventPublisher for NatsEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        // 留待 MVP 编码阶段实装
        tracing::debug!(
            topic = topic,
            bytes = payload.len(),
            "NatsEventPublisher.publish (stub)"
        );
        Ok(())
    }
}
