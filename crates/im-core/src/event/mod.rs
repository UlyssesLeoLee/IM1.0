//! Event 模块 — 领域事件定义 + NATS publish
//!
//! 依据: ImplementationSpec §3.2.2 + BasicDesign §8
//!
//! 事件主题(2026-08-23 自审补全):
//! - im.message.created
//! - im.message.edited
//! - im.message.recalled
//! - im.message.reaction_added
//! - im.conversation.created
//! - im.conversation.member_joined / member_left
//! - im.presence.changed
//! - im.identity.state_changed
//! - im.auth.token_rotated

pub mod events;
pub mod publisher;

pub use events::MessageCreatedEvent;
pub use publisher::{EventPublisher, NatsEventPublisher};
