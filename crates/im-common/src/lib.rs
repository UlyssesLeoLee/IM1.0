//! IM1.0 共享工具
//!
//! ## 模块
//! - [`error`] — 统一错误类型 `AppError` + `ErrorCode` 枚举(21 项,与 aux-03 §B 严格一致)
//! - [`ids`] — newtype IDs(UserId / ConversationId / MessageId / ...)
//! - [`time`] — Utc::now 包装
//!
//! ## 后续 PR 补
//! - `config` — AppConfig load (依赖 figment + dotenvy,本 PR 暂缺)
//! - `tracing_init` — tracing JSON 初始化

pub mod error;
pub mod ids;
pub mod time;

pub use error::{AppError, AppResult, ErrorCode};
