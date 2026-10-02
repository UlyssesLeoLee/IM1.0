//! extension-runtime 占位 — V1+ 实装
//!
//! MVP 阶段: 仅暴露 Manifest schema,workspace 完整
//! V1 阶段: 进程级沙箱、Capability 校验、事件订阅、Command 分发
//!
//! 依据: SRS §26 EXT-FR-001..005 + ImplementationSpec §1.2

use serde::{Deserialize, Serialize};

/// Extension Manifest schema
///
/// V1+ 实现: 解析 → 校验 → 启动独立进程 → 订阅 NATS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,

    /// 声明的 Permission(scope)
    #[serde(default)]
    pub permissions: Vec<String>,

    /// 声明的 Capability
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// 事件订阅列表(如 ["im.message.created"])
    #[serde(default)]
    pub event_subscriptions: Vec<String>,

    /// 可接收的 Command 列表
    #[serde(default)]
    pub commands: Vec<String>,

    /// Webhook URL(可选)
    #[serde(default)]
    pub webhook_url: Option<String>,
}

pub fn placeholder() {}
