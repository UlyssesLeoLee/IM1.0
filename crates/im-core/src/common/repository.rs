//! Repository 公共 trait(留位,MVP 阶段以 trait 隔离 DB 访问)
//!
//! 所有具体 Repository 实现都依赖 `sqlx::PgPool`

// 当前 MVP 阶段:Repository 直接 trait + sqlx 实现
// 待 V1+ 引入 UnitOfWork / Aggregate 概念

pub use im_common::AppError;
