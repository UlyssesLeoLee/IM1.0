//! Reaction 模块 — Message 表情反应
//!
//! 依据: ImplementationSpec §7.4.3 (messages 子特性 reactions)
//!       aux-02 §F.13 message_reactions 表
//!
//! ## 6 个 PgRepository 中的一员(C-1 WBS)

pub mod pg;
pub mod repository;

pub use repository::{Reaction, ReactionRepository};
