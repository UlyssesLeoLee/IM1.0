//! 集成测试: `PgDlqSink` —— aux-08 §D.3 的 **PG 长留存层**
//!
//! 跑在真 PostgreSQL 上。依据 `migrations/0008_create_dlq_records.sql`。
//!
//! ## 为什么必须是集成测试
//!
//! `combine_dlq_results` 的判定逻辑在单测里就够(它是纯函数)。但**这一层到底
//! 有没有真的落库**不是纯函数能回答的: SQL 拼错列名、JSONB 绑定类型不对、
//! CHECK 约束撞上, 全都会编译通过、单测全绿, 只在真库上炸。
//!
//! 决定性断言: **写完之后从库那一侧 SELECT 回来**, 逐字段对拍。断言的是
//! 「数据在那里」, 不是「insert 没报错」。
//!
//! ## 静默跳过纪律 (与 `IM_REQUIRE_PG` / `IM_REQUIRE_NATS` 同构)
//!
//! `cargo test` 默认丢弃通过测试的 stdout, 而「跳过」在 libtest 眼里就是
//! 「通过」—— 于是「PG 层测试全过」与「一个都没跑」无法区分。本仓已经为 PG
//! 栽过一次(2026-10-03, 6 个 WS e2e 静默跳过)。故 `IM_REQUIRE_PG=1` 时连不上
//! 直接 **panic**。

use chrono::Utc;
use im_core::event::publisher::{DlqRecord, DlqSink, PgDlqSink};
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
                panic!("IM_REQUIRE_PG=1 但连不上 PG(dlq_records 集成测试): {e}");
            }
            eprintln!("skip: dlq_records 集成测试 —— 连不上 PG: {e}");
            None
        }
    }
}

fn sample_record(task: &str) -> DlqRecord {
    let now = Utc::now();
    DlqRecord::new(
        task,
        br#"{"message_id":"11111111-1111-1111-1111-111111111111","sequence":7}"#,
        "publish im.message.created: no JetStream ack within 3s",
        4,
        now - std::time::Duration::from_secs(5),
        now,
    )
}

/// 表不存在时**直接失败**, 而不是静默跳过
///
/// 这一条很关键: 若 0008 没被应用, `store()` 会报 `relation "dlq_records"
/// does not exist`, 而跳过逻辑可能把它当成「环境不对」吞掉 —— 于是这一整层
/// 「测过了」的错觉就成立了。
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
        "public.dlq_records 不存在 —— 0008 migration 没应用。\
         先跑仓内 im-migrate, 再跑本测试"
    );
}

#[tokio::test]
async fn pg_sink_persists_the_record_and_it_can_be_read_back() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let sink = PgDlqSink::new(p.clone());
    let rec = sample_record("im.message.created");
    let dlq_id = rec.dlq_id;

    sink.store(&rec)
        .await
        .expect("store 应成功 —— 0008 的列名/类型与 PgDlqSink 的 INSERT 必须对得上");

    let row = sqlx::query(
        "SELECT original_task, original_payload, error_code, error_http_response_code, \
                context_attempt_count, dlq_destination, replay_attempts, replayed_at \
         FROM dlq_records WHERE dlq_id = $1",
    )
    .bind(dlq_id)
    .fetch_one(&p)
    .await
    .expect("刚写的行读不回来 —— 这一层等于没落库");

    use sqlx::Row;
    assert_eq!(row.get::<String, _>("original_task"), "im.message.created");
    assert_eq!(row.get::<i32, _>("error_http_response_code"), 503);
    assert_eq!(row.get::<i32, _>("context_attempt_count"), 4);
    assert_eq!(
        row.get::<String, _>("dlq_destination"),
        "dlq.event.im.message.created"
    );
    // 重放簿记的初始状态: 未重放, 0 次
    assert_eq!(row.get::<i32, _>("replay_attempts"), 0);
    assert!(
        row.get::<Option<chrono::DateTime<Utc>>, _>("replayed_at")
            .is_none(),
        "新写入的死信必须处于「未重放」状态"
    );

    // 载荷是 JSONB: 从库那侧读回来必须还是对象, 而不是被转成了字符串
    let payload: serde_json::Value = row.get("original_payload");
    assert!(
        payload.is_object(),
        "original_payload 应存成 JSONB 对象, 实际: {payload}"
    );
    assert_eq!(payload["sequence"], serde_json::json!(7));

    cleanup(&p, dlq_id).await;
}

/// 非 JSON 载荷必须**原样保住**, 而不是被丢弃或报错
///
/// `DlqRecord` 在解析失败时把载荷退化成 JSON **字符串**。若 PG 层不接受这种
/// 值, 那么「解析失败」这条路径就会在**兜底层**再失败一次 —— 而兜底层的失败
/// 才真的是永久丢失。
#[tokio::test]
async fn non_json_payload_survives_the_pg_layer_as_a_string() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let now = Utc::now();
    let rec = DlqRecord::new(
        "im.message.recalled",
        b"\xff\xfe not utf8, not json at all",
        "boom",
        1,
        now,
        now,
    );
    let dlq_id = rec.dlq_id;

    PgDlqSink::new(p.clone())
        .store(&rec)
        .await
        .expect("非 JSON 载荷也必须写进去 —— 它是最后一道兜底");

    use sqlx::Row;
    let row = sqlx::query("SELECT original_payload FROM dlq_records WHERE dlq_id = $1")
        .bind(dlq_id)
        .fetch_one(&p)
        .await
        .expect("读不回来");
    let payload: serde_json::Value = row.get("original_payload");
    assert!(
        payload.is_string(),
        "解析失败时载荷应退化成 JSON 字符串, 实际: {payload}"
    );

    cleanup(&p, dlq_id).await;
}

/// CHECK 约束必须**真的生效**: 非法 http 状态码被库拒
///
/// 与 `migration_smoke_pg.rs` 里 0007 的做法同构 —— 证明「DB 是真理之源」
/// 而不只是应用层校验。若这条不成立, 将来有人绕过 `DlqRecord` 直接写表时,
/// 就能塞进 `http_status = 0` 这样的垃圾值。
#[tokio::test]
async fn db_rejects_an_out_of_range_http_status() {
    let Some(p) = pool().await else { return };
    require_table(&p).await;

    let r = sqlx::query(
        "INSERT INTO dlq_records (dlq_id, original_task, original_payload, \
           error_code, error_message, error_http_response_code, context_attempt_count, \
           context_first_attempt_at, context_last_attempt_at, failed_at, \
           dlq_destination) \
         VALUES ($1,'im.x','{}'::jsonb,'E','m',0,1,now(),now(),now(),'dlq.event.im.x')",
    )
    .bind(Uuid::new_v4())
    .execute(&p)
    .await;

    assert!(
        r.is_err(),
        "error_http_response_code = 0 必须被 CHECK 约束拒绝 —— 0008 的 \
         chk_dlq_records_error_http_response_code 若没生效, 应用层漏校验就能写进垃圾"
    );
}

async fn cleanup(p: &PgPool, dlq_id: Uuid) {
    let _ = sqlx::query("DELETE FROM dlq_records WHERE dlq_id = $1")
        .bind(dlq_id)
        .execute(p)
        .await;
}
