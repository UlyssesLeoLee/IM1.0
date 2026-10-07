//! 会话列表 keyset 游标的编解码 —— 单一事实源
//!
//! ## 为什么要有这个文件
//!
//! 2026-10-08 之前, 游标格式知识是**分裂**的:
//! - `im-gateway/src/http/conversations.rs` 知道格式(`created_at_millis:id`),
//!   并且把这个字符串发给客户端;
//! - `PgConversationRepository::list_for_user` 把形参命名成 `_cursor`,
//!   SQL 里既没有 `OFFSET` 也没有游标谓词 —— **收到即丢弃**。
//!
//! 两边各自都「正常」, 契约测试也全绿(它只检查路由存在与否), 于是
//! `GET /v1/conversations` 从上线起就没有真正翻过页: 每带一次 cursor
//! 就拿回一模一样的头 N 条, 客户端按规范写的循环会**无限重复同一页**,
//! 且第 N+1 个之后的会话永远拿不到。
//!
//! 把编解码收进本文件, 格式就只有一个定义处: 要么两边都跟着它变,
//! 要么编译不过。这不是重构洁癖, 是这个 bug 的直接成因。
//!
//! ## 格式
//!
//! `"{created_at 的 epoch 微秒数}:{conversation_id}"`
//!
//! - **微秒而不是毫秒**: PG 的 `timestamptz` 精度就是微秒。用毫秒会把
//!   同一毫秒内的两条会话折叠成同一个键, keyset 比较要么漏行要么重复。
//   往返必须无损, 否则分页会静默丢会话。
//! - **`(时间, id)` 复合键**: `created_at` 不是唯一键(批量建会话会撞同一
//!   时刻)。只按时间比较会漏掉「与上一页最后一条同刻」的记录, 这正是
//!   keyset 分页必须带 tiebreaker 的原因。
//! - **不透明**: spec 承诺 client 只需原样回传, 不承诺可解析。所以加
//!   版本前缀之类的演进空间是安全的。

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use im_common::AppError;

/// 把一条会话编码成游标
pub fn encode(created_at: DateTime<Utc>, id: Uuid) -> String {
    format!("{}:{}", created_at.timestamp_micros(), id)
}

/// 解析游标
///
/// 解析失败**不静默降级成「从头开始」** —— 静默会让客户端拿到第 1 页、
/// 以为自己已��尾, 于是既不报错也永远拿不到后续页, 正是本 bug 当初
/// 表现出来的那个样子。宁可 400。
pub fn decode(raw: &str) -> Result<(DateTime<Utc>, Uuid), AppError> {
    let bad = || {
        AppError::Validation(format!(
            "invalid cursor: expected \"<epoch_micros>:<uuid>\", got {raw:?}"
        ))
    };
    let (micros, id) = raw.split_once(':').ok_or_else(bad)?;
    let micros: i64 = micros.parse().map_err(|_| bad())?;
    let id: Uuid = id.parse().map_err(|_| bad())?;
    let ts = Utc.timestamp_micros(micros).single().ok_or_else(bad)?;
    Ok((ts, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_microseconds() {
        // 微秒级时间戳往返无损 —— 这是分页不漏行/不重复的前提。
        let ts = Utc
            .timestamp_micros(1_789_000_000_123_456)
            .single()
            .unwrap();
        let id = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let c = encode(ts, id);
        let (ts2, id2) = decode(&c).expect("decode");
        assert_eq!(ts, ts2, "时间戳往返必须无损, 否则 keyset 分页会漏行");
        assert_eq!(id, id2);
    }

    #[test]
    fn two_conversations_in_same_microsecond_are_distinguishable() {
        // 同一微秒的两条会话必须有**不同**游标, 否则后一条会被前一条的
        // 谓词吃掉(第 1 页末尾 == 第 2 页开头, 客户端永远少一条)。
        let ts = Utc
            .timestamp_micros(1_789_000_000_123_456)
            .single()
            .unwrap();
        let a = encode(ts, Uuid::from_u128(1));
        let b = encode(ts, Uuid::from_u128(2));
        assert_ne!(a, b);
        assert_eq!(decode(&a).unwrap().1, Uuid::from_u128(1));
        assert_eq!(decode(&b).unwrap().1, Uuid::from_u128(2));
    }

    #[test]
    fn millisecond_format_is_not_silently_accepted() {
        // 旧格式(毫秒)必须报明确错误, 不能被当成有效游标。
        let err = decode("1789000000123:01234567-89ab-cdef-0123-4567-89abcdef")
            .expect_err("毫秒格式必须被拒");
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn garbage_cursor_is_rejected_not_ignored() {
        for bad in ["", "abc", "123", "1:2", ":", "1:"] {
            let r = decode(bad);
            assert!(r.is_err(), "cursor {bad:?} 必须报错而不是从头开始");
        }
    }

    #[test]
    fn out_of_range_microseconds_is_rejected() {
        // i64::MAX 微秒远超 chrono 可表示范围, 必须在 chrono 侧被拒,
        // 而不是 panic 或回绕成一个合法但错误的时间。
        assert!(decode(&format!("{}:{}", i64::MAX, Uuid::nil())).is_err());
    }
}
