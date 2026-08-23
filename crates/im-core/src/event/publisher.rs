//! EventPublisher — 异步事件发布抽象
//!
//! 依据: ImplementationSpec §7.4.5 + DetailedDesign §9.1

use async_trait::async_trait;

use im_common::AppError;

use super::events::MessageCreatedEvent;

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// 发布事件(具体序列化由实现决定,MVP 用 JSON)
    async fn publish(&self, topic: &str, payload: &MessageCreatedEvent) -> Result<(), AppError>;
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
    async fn publish(
        &self,
        topic: &str,
        _payload: &MessageCreatedEvent,
    ) -> Result<(), AppError> {
        // 留待 MVP 编码阶段实装
        tracing::debug!(topic = topic, "NatsEventPublisher.publish (stub)");
        Ok(())
    }
}
