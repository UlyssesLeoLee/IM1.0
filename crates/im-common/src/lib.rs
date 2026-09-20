//! IM1.0 共享工具
//!
//! ## 模块
//! - [`error`] — 统一错误类型 `AppError` + `ErrorCode` 枚举(21 项,与 aux-03 §B 严格一致)
//! - [`ids`] — newtype IDs(UserId / ConversationId / MessageId / ...)
//! - [`time`] — Utc::now 包装
//! - [`tracing_init`] — Tracing JSON 初始化(Observability §6.1 + aux-09 §B)
//!
//! ## 模块
//! - [`config`] — AppConfig 加载 (D-1 WBS, figment + dotenvy + 双密钥 JSON)
//! - [`tracing_init`] — tracing JSON 初始化 (D-2 WBS, JSON + EnvFilter + OTel exporter stub)

pub mod config;
pub mod error;
pub mod ids;
pub mod time;
pub mod tracing_init;

pub use error::{AppError, AppResult, ErrorCode};