//! im-core: 业务逻辑主进程
//!
//! 依据: ImplementationSpec §7.4 + DetailedDesign §9
//!
//! ## 内部模块
//! - `identity` — User / Guest / Token / DeviceSession
//! - `relationship` — Friend / Block
//! - `conversation` — Conversation + Membership + DM
//! - `message` — Message + Reaction + Sequence
//! - `event` — 领域事件发布到 NATS
//! - `settings` — environments.settings 加载 + 缓存 + 失效
//!
//! ## 跨模块规则
//! 模块间禁止直接访问对方的数据库表(每个模块通过自己的 Repository 接口访问)
//! 跨模块调用通过 Service 方法显式声明

pub mod common;
pub mod conversation;
pub mod event;
pub mod identity;
pub mod message;
pub mod relationship;
pub mod settings;

// 重新导出常用类型
pub use im_common::{AppError, AppResult, ErrorCode};
