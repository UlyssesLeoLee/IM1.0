//! Message 模块 — Message + Reaction + Sequence
//!
//! 依据: ImplementationSpec §7.4.3 + DetailedDesign §9.1

pub mod content;
pub mod repository;
pub mod sequence;
pub mod service;

pub use repository::{Message, MessageRepository, MessageState};
pub use sequence::SequenceAllocator;
pub use service::{MessageService, SendMessageCommand};
