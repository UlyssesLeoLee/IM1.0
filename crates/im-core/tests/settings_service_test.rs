//! SettingsService 集成测试 — 验证它**真的读库**
//!
//! ## 为什么需要这一整个文件
//!
//! 2026-10-03 之前, `SettingsService` 看起来存在、实则从不读库:
//! `get()` 从一个永远为空的 HashMap 落 `default()`, 任何 env 都返回
//! `recall_window_seconds = 120`。若把撤回时间窗接到它上面, 代码读起来
//! 符合 aux-04 §B.4「不能写死」的不变量, 实际却是写死值 —— 这种「看起来
//! 合规」的缺陷比明写死更难发现, 所以必须有测试把**「它确实读了库」**这件事
//! 钉死。
//!
//! 关键断言: 每个用例都创建一个**全新** environment 并写入**非默认**值。
//! 如果实现退化成返回 `default()`, 这些用例会全部读到 120 而失败。

use std::env;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use im_common::ids::EnvironmentId;
use im_core::settings::service::SettingsService;

fn database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://leo19@172.28.176.169:5544/postgres".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url())
        .await
        .expect("connect to PG")
}

/// 建 tenant→game→environment 三级, 返回新 environment id
///
/// `environments.name` 的 CHECK 只接受 `production` / `staging` / `test`, 且
/// `UNIQUE (game_id, name)` —— 故每次都建**新的** game, 否则第二次 INSERT 撞
/// 唯一约束。返回 `settings` 的**待写入值**由调用方给。
async fn make_env(p: &PgPool, settings_json: &str) -> EnvironmentId {
    let id: Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name)
            VALUES (gen_random_uuid(), 'set-tenant-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'set-game-' || gen_random_uuid()::text FROM t
            RETURNING id
        )
        INSERT INTO environments (id, game_id, name, settings)
        SELECT gen_random_uuid(), g.id, 'test', $1::jsonb FROM g
        RETURNING id
        "#,
    )
    .bind(settings_json)
    .fetch_one(p)
    .await
    .expect("make_env");
    EnvironmentId(id)
}

#[tokio::test]
async fn recall_window_is_read_from_the_database_not_hardcoded() {
    // 本条是整个文件的**核心**: 写入 777, 若实现返回 default() 会读到 120。
    let p = pool().await;
    let env = make_env(
        &p,
        r#"{"message": {"recall_window_seconds": 777, "retention_days": {}}}"#,
    )
    .await;

    let svc = SettingsService::new(p.clone());
    assert_eq!(
        svc.recall_window_seconds(env).await.expect("读 settings"),
        777,
        "撤回时间窗必须来自 environments.settings —— 若得到 120 说明实现退化成写死"
    );
}

#[tokio::test]
async fn two_environments_can_have_different_windows() {
    // per-env 覆盖是这条不变量的**全部意义**; 若实现忽略 env id 只读一个全局值,
    // 这条会失败。
    let p = pool().await;
    let strict = make_env(
        &p,
        r#"{"message": {"recall_window_seconds": 30, "retention_days": {}}}"#,
    )
    .await;
    let lax = make_env(
        &p,
        r#"{"message": {"recall_window_seconds": 86400, "retention_days": {}}}"#,
    )
    .await;

    let svc = SettingsService::new(p.clone());
    assert_eq!(svc.recall_window_seconds(strict).await.unwrap(), 30);
    assert_eq!(svc.recall_window_seconds(lax).await.unwrap(), 86400);
}

#[tokio::test]
async fn empty_settings_fall_back_to_serde_defaults() {
    // 该列 `NOT NULL DEFAULT '{}'` 且有 `jsonb_typeof = 'object'` CHECK,
    // 所以「行存在但为空对象」是**合法状态**, 必须靠 serde 逐字段 default 补齐。
    let p = pool().await;
    let env = make_env(&p, "{}").await;

    let svc = SettingsService::new(p.clone());
    let s = svc.get(env).await.expect("读 settings");
    assert_eq!(
        s.message.recall_window_seconds, 120,
        "未配置时应为 serde 默认 120"
    );
    assert!(s.friend_system_enabled);
    assert_eq!(s.rate_limit.send_message_per_min, 60);
}

#[tokio::test]
async fn partial_settings_only_override_given_keys() {
    // 只写 `recall_window_seconds` 时, **其余字段必须保持默认** ——
    // 尤其不能因为这一个字段存在就把整份 settings 丢掉。
    let p = pool().await;
    let env = make_env(&p, r#"{"message": {"recall_window_seconds": 15}}"#).await;

    let svc = SettingsService::new(p.clone());
    let s = svc.get(env).await.expect("读 settings");
    assert_eq!(s.message.recall_window_seconds, 15);
    assert_eq!(
        s.message.retention_days.channel,
        Some(365),
        "未写的子字段应保持默认"
    );
    assert!(!s.voice.enabled);
}

#[tokio::test]
async fn missing_environment_is_not_found_not_default() {
    // **关键区分**: 「环境不存在」与「环境存在但没配该字段」是两件事。
    // 前者必须报错(否则一个 typo 的 env id 会静默拿到 120, 表现为「撤回窗口
    // 莫名其妙变成 2 分钟」); 后者才由 serde default 兜底。
    let p = pool().await;
    let svc = SettingsService::new(p.clone());
    let r = svc.get(EnvironmentId::new()).await;
    assert!(
        matches!(r, Err(im_common::AppError::NotFound(_))),
        "不存在的环境应 NotFound, 实际: {r:?}"
    );
}
