//! `jobctl` 的 DLQ 处置 —— 跑在**真 PostgreSQL** 上
//!
//! 依据 `migrations/0008_create_dlq_records.sql`。
//!
//! ## 为什么这些必须是真库集成测试
//!
//! 单测已经覆盖了参数解析、`payload_bytes` 的反变换、`state()` 判定 —— 那些
//! 都是纯函数。但下面这几件事**单测原理上答不了**:
//!
//! 1. 列名/列类型对不对。`error_http_response_code` 是 `SMALLINT`(INT2), 写成
//!    `i32` 编译照过、只在真库上 panic(2026-10-06 实测)。
//! 2. 「写进去的形状」与「读出来的形状」是不是同一个。`PgDlqSink` 在载荷不是
//!    JSON 时会把它存成 **JSON 字符串**; 若 `replay` 读出来直接再序列化, 重放
//!    出去的就是双重编码的坏载荷 —— 而它**看起来完全成功**。
//! 3. CAS 守卫(`WHERE ... replayed_at IS NULL`)在真库上是否真的拦得住。
//!
//! ## 关键手法: 用 `PgDlqSink` 写入, 而不是手写 INSERT
//!
//! 手写 INSERT 只能证明「我写的和我读的一致」。改用**真正的生产者**
//! (`PgDlqSink::store`)写入, 再用 jobctl 读出, 测的才是真正的
//! **生产者 → 消费者闭环**。若哪天 `DlqRecord` 改了存法而 jobctl 没跟上,
//! 这些用例会红; 手写 INSERT 则永远绿。
//!
//! ## 静默跳过纪律 (与 `IM_REQUIRE_PG` 同构)
//!
//! `cargo test` 默认丢弃通过用例的 stdout, 于是「跳过」与「通过」从输出上
//! 无法区分(本仓 2026-10-03 为此栽过)。故 `IM_REQUIRE_PG=1` 时连不上 PG 直接
//! **panic**。

use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use im_core::event::publisher::{DlqRecord, DlqSink, PgDlqSink};
use jobctl::{dlq, ListOptions};
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

fn database_url() -> String {
    env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "postgres://leo19@172.28.176.169:5544/postgres".to_string())
}

async fn pool() -> Option<PgPool> {
    match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url())
        .await
    {
        Ok(p) => Some(p),
        Err(e) => {
            if env::var("IM_REQUIRE_PG").as_deref() == Ok("1") {
                panic!("IM_REQUIRE_PG=1 但连不上 PG(jobctl 集成测试): {e}");
            }
            eprintln!("skip: jobctl 集成测试 —— 连不上 PG: {e}");
            None
        }
    }
}

async fn require_table(p: &PgPool) {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_schema='public' AND table_name='dlq_records')",
    )
    .fetch_one(p)
    .await
    .expect("查 information_schema 失败");
    assert!(
        exists,
        "public.dlq_records 不存在 —— 0008 没应用。先跑仓内 im-migrate。"
    );
}

/// 造一条死信, **经由真正的生产者** `PgDlqSink` 落库
async fn seed(p: &PgPool, topic: &str, payload: &[u8], failed_secs_ago: i64) -> DlqRecord {
    let now = Utc::now();
    let rec = DlqRecord::new(
        topic,
        payload,
        "publish: no JetStream ack",
        3,
        now - Duration::seconds(failed_secs_ago + 5),
        now - Duration::seconds(failed_secs_ago),
    );
    PgDlqSink::new(p.clone())
        .store(&rec)
        .await
        .expect("PgDlqSink 应写入成功(否则读侧测的就不是真实存法)");
    rec
}

async fn cleanup(p: &PgPool, ids: &[Uuid]) {
    for id in ids {
        let _ = sqlx::query("DELETE FROM dlq_records WHERE dlq_id = $1")
            .bind(id)
            .execute(p)
            .await;
    }
}

fn opts(include_settled: bool) -> ListOptions {
    ListOptions {
        since: None,
        limit: 100,
        include_settled,
        json: false,
    }
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

/// 默认只看待处置 —— 这是「运维一眼看到要处理什么」的全部意义
#[tokio::test]
async fn list_defaults_to_pending_and_all_reveals_settled() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let a = seed(&p, "im.message.created", br#"{"a":1}"#, 60).await;
    let b = seed(&p, "im.message.recalled", br#"{"b":2}"#, 30).await;
    let c = seed(&p, "im.message.deleted", br#"{"c":3}"#, 10).await;
    let ids = [a.dlq_id, b.dlq_id, c.dlq_id];

    // 把 c 标成已重放
    sqlx::query("UPDATE dlq_records SET replayed_at = now() WHERE dlq_id = $1")
        .bind(c.dlq_id)
        .execute(&p)
        .await
        .unwrap();

    let pending = dlq::list(&p, &opts(false)).await.unwrap();
    let pending_ids: Vec<Uuid> = pending.iter().map(|r| r.dlq_id).collect();
    assert!(
        pending_ids.contains(&a.dlq_id) && pending_ids.contains(&b.dlq_id),
        "两条待处置的应出现在默认列表"
    );
    assert!(
        !pending_ids.contains(&c.dlq_id),
        "已重放的不该出现在默认列表 —— 否则运维会拿着它去重放"
    );
    assert!(
        pending.iter().all(|r| r.state() == "pending"),
        "默认列表里不该混进已处置的行"
    );

    let all = dlq::list(&p, &opts(true)).await.unwrap();
    let all_ids: Vec<Uuid> = all.iter().map(|r| r.dlq_id).collect();
    assert!(all_ids.contains(&c.dlq_id), "--all 应包含已重放的");

    // --since 真的在按时间筛。这里用 --all, 否则 10 秒前那条(c)已被标成
    // 已重放, 根本不会出现在默认列表里, 「它被筛进来了」就无从断言。
    let recent = ListOptions {
        since: Some(Utc::now() - Duration::seconds(20)),
        ..opts(true)
    };
    let recent_ids: Vec<Uuid> = dlq::list(&p, &recent)
        .await
        .unwrap()
        .iter()
        .map(|r| r.dlq_id)
        .collect();
    assert!(
        !recent_ids.contains(&a.dlq_id),
        "60 秒前的不该在 20 秒窗口里"
    );
    assert!(
        !recent_ids.contains(&b.dlq_id),
        "30 秒前的也不该在 20 秒窗口里"
    );
    assert!(
        recent_ids.contains(&c.dlq_id),
        "10 秒前的必须在 20 秒窗口里"
    );

    cleanup(&p, &ids).await;
}

/// SMALLINT 必须按 i16 解出来 —— 与 2026-10-06 那次 CI 红灯同一个属性
#[tokio::test]
async fn list_reads_the_smallint_status_column() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let a = seed(&p, "im.message.created", b"{}", 5).await;
    let ids = [a.dlq_id];

    let rows = dlq::list(&p, &opts(false)).await.unwrap();
    let row = rows.iter().find(|r| r.dlq_id == a.dlq_id).unwrap();
    assert_eq!(
        row.error_http_response_code, 503,
        "SMALLINT 列必须解出 503(写成 i32 会在真库上 panic)"
    );
    assert_eq!(row.error_code, "SERVICE_UNAVAILABLE");
    assert_eq!(row.original_task, "im.message.created");
    assert_eq!(row.dlq_destination, "dlq.event.im.message.created");
    assert_eq!(row.replay_attempts, 0);

    cleanup(&p, &ids).await;
}

// ---------------------------------------------------------------------------
// replay
// ---------------------------------------------------------------------------

/// 载荷是合法 JSON: 重放出去的字节必须**等价**于原载荷
#[tokio::test]
async fn replay_of_a_json_payload_republishes_equivalent_bytes() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let original = br#"{"message_id":"11111111-1111-1111-1111-111111111111","sequence":7}"#;
    let a = seed(&p, "im.message.created", original, 5).await;
    let ids = [a.dlq_id];

    let seen = Arc::new(Mutex::new(Vec::<(String, Vec<u8>)>::new()));
    let sink = seen.clone();
    let outcome = dlq::replay_with(&p, a.dlq_id, move |topic, bytes| {
        let sink = sink.clone();
        async move {
            sink.lock().unwrap().push((topic, bytes));
            Ok(())
        }
    })
    .await
    .unwrap();

    assert!(
        outcome.is_clean(),
        "干净的一次重放应是 Published: {outcome:?}"
    );

    // 拷出锁的作用域再断言: 既不把 guard 带过 await, 也让下面的断言读起来
    // 就是「publisher 到底收到了什么」。
    let cap = { seen.lock().unwrap().clone() };
    assert_eq!(cap.len(), 1, "publisher 恰好被调一次");
    assert_eq!(cap[0].0, "im.message.created", "必须发回原 subject");
    let back: serde_json::Value = serde_json::from_slice(&cap[0].1).unwrap();
    assert_eq!(back["sequence"], serde_json::json!(7), "载荷内容必须保真");
    assert_eq!(
        back["message_id"],
        serde_json::json!("11111111-1111-1111-1111-111111111111")
    );

    let rows = dlq::list(&p, &opts(true)).await.unwrap();
    let row = rows.iter().find(|r| r.dlq_id == a.dlq_id).unwrap();
    assert!(row.replayed_at.is_some(), "成功后必须标记为已重放");
    assert_eq!(row.replay_attempts, 1);
    assert!(row.discarded_at.is_none());

    cleanup(&p, &ids).await;
}

/// **本文件最该被守住的一条**: 解析失败时存下的 JSON 字符串, 重放时必须还原
/// 成**原始字节**, 而不是双重编码
///
/// 退化路径的样子:
/// ```text
/// 库里存的是   "\"hello\""            (JSON 字符串, 内容 hello)
/// 错的做法     "\"\\\"hello\\\"\""    (引号又套一层)
/// ```
/// 错的后果不是「重放失败」, 而是「重放成功地把一条坏数据又发了出去」——
/// 下游拿到 `\"hello\"` 解析成带引号的字符串, 业务上却看不出任何异常。
#[tokio::test]
async fn replay_of_an_unparseable_payload_sends_the_raw_bytes_not_double_encoded() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    // 非 JSON、非 UTF-8 —— `DlqRecord` 会把它存成 JSON 字符串
    let raw: &[u8] = b"\xff\xfe not utf8, not json at all";
    let a = seed(&p, "im.message.recalled", raw, 5).await;
    let ids = [a.dlq_id];

    let seen = Arc::new(Mutex::new(Vec::<(String, Vec<u8>)>::new()));
    let sink = seen.clone();
    dlq::replay_with(&p, a.dlq_id, move |topic, bytes| {
        let sink = sink.clone();
        async move {
            sink.lock().unwrap().push((topic, bytes));
            Ok(())
        }
    })
    .await
    .unwrap();

    let cap = { seen.lock().unwrap().clone() };
    assert_eq!(
        cap[0].1,
        raw.to_vec(),
        "重放出去的必须**逐字节**等于原始载荷。若失败, 检查 payload_bytes 是否 \
         退化成了 serde_json::to_vec(那个 JSON 字符串)"
    );
    // 反例守卫: 不能是「看起来一样的双重编码」
    let encoded = serde_json::to_vec(&serde_json::Value::String(
        String::from_utf8_lossy(raw).to_string(),
    ))
    .unwrap();
    assert_ne!(cap[0].1, encoded, "双重编码的形态必须被排除");
    drop(cap);

    cleanup(&p, &ids).await;
}

/// 发布**失败**时: 记一次尝试, 但**绝不**标记为已重放
///
/// 这是本工具最要紧的不变量。若失败时也标了 `replayed_at`, 该行会从
/// `dlq list`(默认只看待处置)的视野里永久消失 —— 静默丢数据, 正是这层
/// 存在的理由所要防的那件事。
#[tokio::test]
async fn a_failed_publish_leaves_the_row_pending_but_counts_the_attempt() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let a = seed(&p, "im.message.created", b"{}", 5).await;
    let ids = [a.dlq_id];

    let err = dlq::replay_with(&p, a.dlq_id, |_topic, _bytes| async {
        Err("simulated NATS outage".to_string())
    })
    .await
    .expect_err("发布失败必须返回 Err");
    assert!(
        err.contains("simulated NATS outage"),
        "错误信息要带出真实原因"
    );

    let rows = dlq::list(&p, &opts(false)).await.unwrap();
    let row = rows
        .iter()
        .find(|r| r.dlq_id == a.dlq_id)
        .expect("发布失败后该行必须仍在待处置列表里");
    assert!(
        row.replayed_at.is_none(),
        "发布失败**不得**标记为已重放 —— 否则它从默认列表消失, 等于静默丢数据"
    );
    assert_eq!(
        row.replay_attempts, 1,
        "失败也是一次尝试, 必须记账, 否则运维以为没人碰过"
    );

    cleanup(&p, &ids).await;
}

/// 已重放 / 已丢弃的不许再重放
#[tokio::test]
async fn replay_refuses_rows_that_are_already_settled() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let a = seed(&p, "im.message.created", b"{}", 60).await;
    let b = seed(&p, "im.message.deleted", b"{}", 50).await;
    let ids = [a.dlq_id, b.dlq_id];

    sqlx::query("UPDATE dlq_records SET replayed_at = now() WHERE dlq_id = $1")
        .bind(a.dlq_id)
        .execute(&p)
        .await
        .unwrap();
    sqlx::query("UPDATE dlq_records SET discarded_at = now() WHERE dlq_id = $1")
        .bind(b.dlq_id)
        .execute(&p)
        .await
        .unwrap();

    let r1 = dlq::replay_with(&p, a.dlq_id, |_t, _b| async { Ok(()) }).await;
    let e1 = r1.unwrap_err();
    assert!(e1.contains("已重放"), "已重放的应被拒, 实际: {e1}");

    let r2 = dlq::replay_with(&p, b.dlq_id, |_t, _b| async { Ok(()) }).await;
    let e2 = r2.unwrap_err();
    assert!(e2.contains("丢弃"), "已丢弃的应被拒, 实际: {e2}");

    cleanup(&p, &ids).await;
}

/// 不存在的 id 必须报错, 不能安静地「成功」
#[tokio::test]
async fn replay_of_an_unknown_id_fails_loudly() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let r = dlq::replay_with(&p, Uuid::new_v4(), |_t, _b| async { Ok(()) }).await;
    assert!(r.is_err(), "不存在的 id 必须失败 —— 静默成功等于谎报已重放");
}

// ---------------------------------------------------------------------------
// discard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discard_marks_the_row_and_is_idempotent() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let a = seed(&p, "im.message.created", b"{}", 5).await;
    let ids = [a.dlq_id];

    assert_eq!(
        dlq::discard(&p, a.dlq_id).await.unwrap(),
        dlq::DiscardOutcome::Discarded
    );
    // 再来一次: 不该报错, 也不该改任何东西(运维脚本会重复调用)
    assert_eq!(
        dlq::discard(&p, a.dlq_id).await.unwrap(),
        dlq::DiscardOutcome::AlreadyDiscarded
    );

    let rows = dlq::list(&p, &opts(true)).await.unwrap();
    let row = rows.iter().find(|r| r.dlq_id == a.dlq_id).unwrap();
    assert!(row.discarded_at.is_some());
    assert!(row.replayed_at.is_none());

    cleanup(&p, &ids).await;
}

/// 已重放过的不许再丢弃
///
/// 否则同一条记录会既说「事件发出去了」又说「丢了」, 排障时两边都能引用它。
#[tokio::test]
async fn discard_refuses_an_already_replayed_row() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let a = seed(&p, "im.message.created", b"{}", 5).await;
    let ids = [a.dlq_id];

    sqlx::query("UPDATE dlq_records SET replayed_at = now() WHERE dlq_id = $1")
        .bind(a.dlq_id)
        .execute(&p)
        .await
        .unwrap();

    assert_eq!(
        dlq::discard(&p, a.dlq_id).await.unwrap(),
        dlq::DiscardOutcome::AlreadyReplayed
    );

    let rows = dlq::list(&p, &opts(true)).await.unwrap();
    let row = rows.iter().find(|r| r.dlq_id == a.dlq_id).unwrap();
    assert!(row.discarded_at.is_none(), "被拒的丢弃不得留下痕迹");

    cleanup(&p, &ids).await;
}
