//! IM1.0 共享工具
//!
//! ## 模块
//! - [`error`] — 统一错误类型 `AppError` + `ErrorCode` 枚举(21 项,与 aux-03 §B 严格一致)
//! - [`ids`] — newtype IDs(UserId / ConversationId / MessageId / ...)
//! - [`time`] — Utc::now 包装
//! - [`config`] — `AppConfig::load()` 实现(aux-12 §A 27 env vars + §B 10 Secret 解析 + §D 11 校验规则)
//!
//! ## 后续 PR 补
//! - `tracing_init` — tracing JSON 初始化

pub mod config;
pub mod error;
pub mod ids;
pub mod time;

pub use config::{AppConfig, ConfigError, ConfigIssue, Environment, RateLimitConfig};
pub use error::{AppError, AppResult, ErrorCode};
