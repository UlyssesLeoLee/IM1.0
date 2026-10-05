//! `dlq_records` 的读与处置 —— aux-08 §D.4 步骤 3/4
//!
//! 三条操作对应那张表的**三种结局**:
//!
//! | 结局 | 列 | 由谁写 | CLI
//! |---|---|---|---|
//! | 待处置 | `replayed_at IS NULL AND discarded_at IS NULL` | `PgDlqSink` | `dlq list`(默认只看这个)
//! | 已重放 | `replayed_at` | `dlq replay` | `dlq list --all`
//! | 已丢弃 | `discarded_at` | `dlq discard` | `dlq list --all`
//!
//! ## 列类型的教训(2026-10-06)
//!
//! `error_http_response_code` 在 0008 里是 **`SMALLINT`(INT2)**, 不是 INTEGER。
//! 同一天我在 `dlq_pg_sink_integration.rs` 里把它按 `i32` 解, 测试在真库上
//! panic 在 `sqlx-core/src/row.rs:74` —— `Row::get` 是泛型的, 泛型参数要等到
//! 运行时的 `T::compatible(&ty)` 才被检查。本文件所有整数列都显式标注了类型,
//! 改动时不要「顺手」改成别的宽度。

use std::future::Future;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::ListOptions;

/// `dlq list` 的一行
#[derive(Debug, Clone)]
pub struct DlqRow {
    pub dlq_id: Uuid,
    /// 事件语境下即 NATS subject(aux-08 §D.2 的 `original_task`)
    pub original_task: String,
    pub error_code: String,
    pub error_message: String,
    /// `SMALLINT` -> `i16`
    pub error_http_response_code: i16,
    pub context_attempt_count: i32,
    pub failed_at: DateTime<Utc>,
    pub dlq_destination: String,
    pub replay_attempts: i32,
    pub replayed_at: Option<DateTime<Utc>>,
    pub discarded_at: Option<DateTime<Utc>>,
}

impl DlqRow {
    /// 当前处于哪一种结局
    pub fn state(&self) -> &'static str {
        // 顺序有意义: `replayed_at` 优先于 `discarded_at`。理论上二者互斥
        // (两个 UPDATE 都带 `... IS NULL` 的守卫), 但若将来有人手工改库造成
        // 两者同时非空, 「已重放」是更接近事实的那个 —— 事件真的发出去了。
        if self.replayed_at.is_some() {
            "replayed"
        } else if self.discarded_at.is_some() {
            "discarded"
        } else {
            "pending"
        }
    }
}

/// `dlq list` 的查询
///
/// 列清单与 `row_to_dlq_row` 逐项对应 —— 改一处必须改另一处, 少改的那个要到
/// 真库上才炸(见本文件开头的 SMALLINT 教训)。
///
/// `LIMIT` 是**必需的**而不是锦上添花: `dlq_records` 按设计长留存, 死信量大时
/// 无上限的查询会淹没运维终端, 而人会习惯性 `| head` —— 于是没人看见后面的行,
/// 工具反而制造了「已处置」的错觉。
///
/// 待处置的判定(`replayed_at IS NULL AND discarded_at IS NULL`)与 0008 的
/// partial 索引 `idx_dlq_records_pending_replay_failed_at` 逐字一致, 默认路径
/// 能走上该索引。
const LIST_SQL: &str = "SELECT dlq_id, original_task, error_code, error_message, \
     error_http_response_code, context_attempt_count, failed_at, dlq_destination, \
     replay_attempts, replayed_at, discarded_at \
     FROM dlq_records \
     WHERE ($1::timestamptz IS NULL OR failed_at >= $1) \
       AND ($2::bool OR (replayed_at IS NULL AND discarded_at IS NULL)) \
     ORDER BY failed_at \
     LIMIT $3";

/// 列出死信
pub async fn list(pool: &PgPool, opts: &ListOptions) -> Result<Vec<DlqRow>, String> {
    let rows = sqlx::query(LIST_SQL)
        .bind(opts.since)
        .bind(opts.include_settled)
        .bind(opts.limit)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("查询 dlq_records 失败: {e}"))?;

    rows.iter().map(row_to_dlq_row).collect()
}

fn row_to_dlq_row(row: &sqlx::postgres::PgRow) -> Result<DlqRow, String> {
    // 逐列用 `try_get` 并带列名: 上一条的类型错配之所以难查, 就是因为
    // `row.get::<i32,_>` 的失败信息里只有泛型参数, 不知道是哪一列。
    let col = |e: sqlx::Error| format!("解析 dlq_records 行失败: {e}");

    Ok(DlqRow {
        dlq_id: row.try_get("dlq_id").map_err(col)?,
        original_task: row.try_get("original_task").map_err(col)?,
        error_code: row.try_get("error_code").map_err(col)?,
        error_message: row.try_get("error_message").map_err(col)?,
        error_http_response_code: row.try_get("error_http_response_code").map_err(col)?,
        context_attempt_count: row.try_get("context_attempt_count").map_err(col)?,
        failed_at: row.try_get("failed_at").map_err(col)?,
        dlq_destination: row.try_get("dlq_destination").map_err(col)?,
        replay_attempts: row.try_get("replay_attempts").map_err(col)?,
        replayed_at: row.try_get("replayed_at").map_err(col)?,
        discarded_at: row.try_get("discarded_at").map_err(col)?,
    })
}

/// 把库里的 `original_payload` 还原成**当初要发出去的字节**
///
/// ## 这是本文件最容易写错的一处
///
/// `PgDlqSink` 写入 `original_payload`(JSONB)时用的是**解析结果**:
///
/// - 载荷是合法 JSON -> 存成对象/数组
/// - 载荷不是 JSON   -> `DlqRecord` 退化成把**原始字节**塞进一个 JSON 字符串
///
/// 所以重放时不能无脑 `serde_json::to_vec`:
/// ```text
/// 库里存的是   "\"hello\""            (JSON 字符串, 内容是 hello)
/// 直接 to_vec  "\"\\\"hello\\\"\""    (双重编码!)
/// 发出去的载荷里凭空多了一层引号
/// ```
///
/// 消费者按 JSON 解析时会得到字符串 `"hello"`(含引号)而不是 `hello`, 于是
/// **重放看起来成功了, 实际上把一条坏数据重新发了出去** —— 这比重放失败更糟。
///
/// 字节层: 非 UTF-8 的损坏**发生在写入时**, 这里救不回来
///
/// `DlqRecord::new`(im-core/src/event/publisher.rs:374)用的是
/// `String::from_utf8_lossy(payload)`, 它把非法字节**替换成 U+FFFD**
/// (`EF BF BD`)。也就是说 `0xFF 0xFE` 落库时已变成 `EF BF BD EF BF BD`
/// —— 信息在**写**的那一步就没了, 本函数无论怎么写都还原不回去。
///
/// 这是 2026-10-06 在真库上实测到的
/// (`replay_of_a_non_utf8_payload_is_lossy_at_write_time`)。要真正无损, 得改
/// `original_payload` 的存储形状(如 `{"__b64__": "..."}`), 而 aux-08 §D.2
/// 冻结了它的 JSON 形状 —— 那是规范所有者的裁决, 不是这里能顺手改的。
/// 已记入 `docs/gap-ledger.md` §1.35。
///
/// 故本函数的契约是: **对 UTF-8 载荷无损**, 对非 UTF-8 载荷返回替换字符后的
/// 结果。调用方不该假装它能还原任意字节。
pub fn payload_bytes(stored: &Value) -> Vec<u8> {
    match stored {
        // 解析失败时存的那条: 里面就是(可能被 lossy 处理过的)原始字节
        Value::String(raw) => raw.as_bytes().to_vec(),
        other => serde_json::to_vec(other).unwrap_or_default(),
    }
}

/// `replay` 的结果
#[derive(Debug, Clone, PartialEq)]
pub enum ReplayOutcome {
    /// 已重新发布, 并已标记为重放
    Published { attempts: i32 },
    /// 已重新发布, 但**没能**标记 —— 并发下别人先处置了同一条。
    /// 事件确实发出去了(可能重复), 故不算失败, 但必须让操作者看见。
    PublishedButNotMarked { attempts: i32 },
}

impl ReplayOutcome {
    pub fn is_clean(&self) -> bool {
        matches!(self, ReplayOutcome::Published { .. })
    }
}

/// 重放一条死信
///
/// `publish` 以闭包注入而不是直接连 NATS: 这样「读 → 发 → 标记」这条编排逻辑
/// 能在**不依赖 NATS** 的情况下被完整测到(真 PG + 假 publisher)。IO 只留在
/// `main.rs` 里。
///
/// `topic` 取 `String` 而非 `&str`: 返回类型 `Fut` 不随入参生命周期变化, 被
/// 借用的 `&str` 因此进不了 async 块(会撞 E0521 之类的借用检查)。传所有权
/// 是这里唯一能同时满足「闭包注入」与「async」的形状。
///
/// ## 为什么是「先发布、后标记」而不是反过来
///
/// 两种顺序各有各的坏处, 取决于**哪种失败更不可逆**:
///
/// - 先标记后发布: 发布失败 → 这一行被标成「已重放」但其实没发出去, 而
///   `dlq list` 默认只看待处置, 于是它从运维的视野里**永久消失**。这是静默
///   丢数据 —— 正是这一整层要防的那件事。
/// - 先发布后标记: 标记失败 → 事件被发出去两次。
///
/// 选后者, 因为 aux-08 §E.2 的幂等矩阵规定「NATS 事件推送按 event_id 去重
/// (消费者维护 seen_set)」, 重复投递有归处; 而「标了已重放却没发」没有。
/// 代价(重复发布)被如实报成 [`ReplayOutcome::PublishedButNotMarked`], 不藏。
pub async fn replay_with<F, Fut>(
    pool: &PgPool,
    id: Uuid,
    publish: F,
) -> Result<ReplayOutcome, String>
where
    F: FnOnce(String, Vec<u8>) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let row = sqlx::query(
        "SELECT original_task, original_payload, replayed_at, discarded_at, replay_attempts \
         FROM dlq_records WHERE dlq_id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("查询 dlq_records/{id} 失败: {e}"))?
    .ok_or_else(|| format!("没有 id 为 {id} 的死信记录(可能已被清理)"))?;

    use sqlx::Row;
    let col = |e: sqlx::Error| format!("解析 dlq_records/{id} 失败: {e}");
    let topic: String = row.try_get("original_task").map_err(col)?;
    let stored: Value = row.try_get("original_payload").map_err(col)?;
    let replayed_at: Option<DateTime<Utc>> = row.try_get("replayed_at").map_err(col)?;
    let discarded_at: Option<DateTime<Utc>> = row.try_get("discarded_at").map_err(col)?;
    let attempts: i32 = row.try_get("replay_attempts").map_err(col)?;

    if replayed_at.is_some() {
        return Err(format!("{id} 已于先前重放过, 不再重复(要再发请人工新建)"));
    }
    if discarded_at.is_some() {
        return Err(format!("{id} 已被永久丢弃, 不能重放"));
    }

    let bytes = payload_bytes(&stored);

    match publish(topic.clone(), bytes).await {
        Ok(()) => {
            // CAS: 守卫条件让并发的第二个操作者更新不到行, 从而 rows_affected
            // 恰好能回答「是我标的, 还是别人先标了」。
            let n = sqlx::query(
                "UPDATE dlq_records \
                 SET replayed_at = now(), replay_attempts = replay_attempts + 1 \
                 WHERE dlq_id = $1 AND replayed_at IS NULL AND discarded_at IS NULL",
            )
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("标记 {id} 已重放失败: {e}"))?
            .rows_affected();

            if n == 1 {
                Ok(ReplayOutcome::Published {
                    attempts: attempts + 1,
                })
            } else {
                Ok(ReplayOutcome::PublishedButNotMarked {
                    attempts: attempts + 1,
                })
            }
        }
        Err(e) => {
            // 发布失败: 次数照样 +1 —— 它是**尝试**的账本, 不是成功的账本。
            // 运维看到 attempts=3 就知道这条重试过三回, 而不是以为没人碰过。
            let _ = sqlx::query(
                "UPDATE dlq_records SET replay_attempts = replay_attempts + 1 \
                 WHERE dlq_id = $1",
            )
            .bind(id)
            .execute(pool)
            .await;
            Err(format!("重放 {id} 失败(已记一次尝试): {e}"))
        }
    }
}

/// `discard` 的结果
#[derive(Debug, Clone, PartialEq)]
pub enum DiscardOutcome {
    Discarded,
    /// 已重放过的不许丢弃: 事件已经发出去了, 再标丢弃会造出「既发了又当没发」
    /// 的一条记录, 排障时两边都能引用它。
    AlreadyReplayed,
    AlreadyDiscarded,
}

/// 永久丢弃一条死信
///
/// ## 审计留痕落在哪(以及为什么不在 `audit_logs`)
///
/// aux-08 §D.4 步骤 4 要求「重跑 / 跳过 / 永久丢弃**必须留 audit 记录**」。
/// 本实现把痕迹留在 `dlq_records.discarded_at` 这一行上, **没有**往
/// `audit_logs` 写 —— 原因是写不进去:
///
/// `migrations/0006:18` 给 `audit_logs.target_type` 加了 6 值 CHECK
/// (`user` / `conversation` / `environment` / `extension` / `secret` / `system`),
/// **不含** `dlq` 或 `event`。要往里写就得往枚举里加第 7 个值, 那是 schema 变更,
/// 得由 DB owner 拍板(这与当初 `dlq_records` 选择独立成表是同一个原因)。
///
/// 行本身就是记录: 它带着 `original_task` / `error_code` / `discarded_at`,
/// 足以回答「哪条死信、为什么、被谁在什么时候丢的」中的前三个。
///
/// **已知缺口**: 「被谁」答不了 —— `dlq_records` 没有 `discarded_by` 列, 而
/// 0008 已经进了迁移历史, 按 aux-01 §H「永远追加, 不改历史」不能就地加列。
/// 同理 aux-08 §K GAP-12 要求的「二次确认 + 双人审批」也没有实现。
/// 两项都记在 `docs/gap-ledger.md`。
pub async fn discard(pool: &PgPool, id: Uuid) -> Result<DiscardOutcome, String> {
    let row = sqlx::query("SELECT replayed_at, discarded_at FROM dlq_records WHERE dlq_id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("查询 dlq_records/{id} 失败: {e}"))?
        .ok_or_else(|| format!("没有 id 为 {id} 的死信记录(可能已被清理)"))?;

    use sqlx::Row;
    let col = |e: sqlx::Error| format!("解析 dlq_records/{id} 失败: {e}");
    let replayed_at: Option<DateTime<Utc>> = row.try_get("replayed_at").map_err(col)?;
    let discarded_at: Option<DateTime<Utc>> = row.try_get("discarded_at").map_err(col)?;

    if replayed_at.is_some() {
        return Ok(DiscardOutcome::AlreadyReplayed);
    }
    if discarded_at.is_some() {
        return Ok(DiscardOutcome::AlreadyDiscarded);
    }

    let n = sqlx::query(
        "UPDATE dlq_records SET discarded_at = now() \
         WHERE dlq_id = $1 AND discarded_at IS NULL AND replayed_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| format!("标记 {id} 已丢弃失败: {e}"))?
    .rows_affected();

    if n == 1 {
        Ok(DiscardOutcome::Discarded)
    } else {
        // 与本函数开头的读之间发生了并发写
        Ok(DiscardOutcome::AlreadyDiscarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 载荷还原的不对称 —— 本文件最该被守住的一行
    ///
    /// 判别力来自**两个方向**同时成立: 合法 JSON 要还原成字节, 解析失败留下
    /// 的字符串要还原成**原始字节**而不是双重编码的 JSON 字符串。只测前者
    /// 的话, 一个恒等于 `serde_json::to_vec` 的实现也能全绿。
    #[test]
    fn payload_bytes_reverses_both_storage_shapes() {
        // 1) 合法 JSON -> 存成对象 -> 还原成等价字节
        let obj = json!({"message_id": "abc", "sequence": 7});
        assert_eq!(payload_bytes(&obj), serde_json::to_vec(&obj).unwrap());
        // 键序不该影响「数据在不在」, 故只断言能解回同一个值
        let back: Value = serde_json::from_slice(&payload_bytes(&obj)).unwrap();
        assert_eq!(back["sequence"], json!(7));

        // 2) 解析失败 -> 存成 JSON 字符串 -> **还原成原始字节**, 不带引号
        let stored = Value::String(r#"{"broken":"#.to_string());
        assert_eq!(payload_bytes(&stored), br#"{"broken":"#.to_vec());
        // 反例守卫: 若实现退化成 to_vec, 这里的字节会多一层引号
        assert_ne!(
            payload_bytes(&stored),
            serde_json::to_vec(&stored).unwrap(),
            "直接 to_vec 会双重编码, 那正是本函数要防的 bug"
        );

        // 3) 数组与标量
        assert_eq!(payload_bytes(&json!([1, 2])), b"[1,2]");
        assert_eq!(payload_bytes(&json!(null)), b"null");
    }

    /// 状态判定: 两者都空才是待处置
    #[test]
    fn state_reflects_which_settlement_column_is_set() {
        let now = Utc::now();
        let base = DlqRow {
            dlq_id: Uuid::nil(),
            original_task: "im.message.created".into(),
            error_code: "SERVICE_UNAVAILABLE".into(),
            error_message: "x".into(),
            error_http_response_code: 503,
            context_attempt_count: 3,
            failed_at: now,
            dlq_destination: "dlq.event.im.message.created".into(),
            replay_attempts: 0,
            replayed_at: None,
            discarded_at: None,
        };
        assert_eq!(base.state(), "pending");

        let replayed = DlqRow {
            replayed_at: Some(now),
            ..base.clone()
        };
        assert_eq!(replayed.state(), "replayed");

        let discarded = DlqRow {
            discarded_at: Some(now),
            ..base.clone()
        };
        assert_eq!(discarded.state(), "discarded");

        // 两者同时非空(只可能来自手工改库): 取更接近事实的「已重放」
        let both = DlqRow {
            replayed_at: Some(now),
            discarded_at: Some(now),
            ..base
        };
        assert_eq!(both.state(), "replayed");
    }

    #[test]
    fn only_a_clean_publish_counts_as_clean() {
        assert!(ReplayOutcome::Published { attempts: 1 }.is_clean());
        assert!(!ReplayOutcome::PublishedButNotMarked { attempts: 1 }.is_clean());
    }

    /// SQL 里出现的列必须与 0008 的列名逐字一致
    ///
    /// 拼错列名不会编译失败, 只在真库上炸 —— 与 2026-10-06 那次 SMALLINT
    /// 错配同一类: 静态看起来没问题, 只有真 PG 才说话。
    #[test]
    fn list_sql_mentions_only_real_columns() {
        for col in [
            "original_task",
            "error_code",
            "error_message",
            "error_http_response_code",
            "context_attempt_count",
            "failed_at",
            "dlq_destination",
            "replay_attempts",
            "replayed_at",
            "discarded_at",
        ] {
            assert!(LIST_SQL.contains(col), "LIST_SQL 缺列 {col}");
        }
        // aux-01 §I: 禁 status 词段, 故这一列不能叫 error_http_status
        assert!(!LIST_SQL.contains("error_http_status"));
    }
}
