//! Utc::now() 包装 — 便于测试时 mock

use chrono::{DateTime, Utc};

/// 当前 UTC 时间
///
/// 测试时可使用 `#[cfg(test)] mod time { pub fn now() -> DateTime<Utc> { ... } }` 替换
#[inline]
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// unix milliseconds
#[inline]
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}
