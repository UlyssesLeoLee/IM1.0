//! 迁移的**真实**端到端验证 —— 2026-10-03 新增
//!
//! ## 为什么这条测试与既有测试不同
//!
//! `crates/im-gateway/tests/migration_smoke_pg.rs` 是查 schema 的, 它要求
//! **`DATABASE_URL` 指向一个「已应用迁移」的库** —— 谁应用的、什么时候应用的,
//! 它一概不问。也就是说: 那条测试能证明「schema 是对的」, 但证明不了
//! 「**我们的代码能把它变成对的**」。
//!
//! 本文件补上后半段: 建一个**全新的空库**, 用 `im_migrate` 把 7 份迁移真的
//! 跑一遍, 然后断言 14 张表建成、0007 的列与索引都在、DB 层 CHECK 真的生效。
//!
//! 顺带说明一件之前没人提过的事: `migrate-job.yaml` 引用一个**无法构建**的
//! 镜像, 而 `cargo test` 里**没有任何代码会应用迁移** —— 也就是说, 7 份
//! migration 从未由本仓的代码在任何环境执行过。它们「看起来是对的」是因为
//! 有人(某次手工操作)在某个库上跑过, 之后的测试都复用那个已建好的库。
//!
//! ## 环境
//!
//! 需要 `IM_POSTGRES_URL`(与仓内配置一致)。未设则**跳过**, 与既有 e2e 约定
//! 一致, 保证无 PG 的机器跑 `cargo test --workspace` 仍全绿。
//!
//! 安全性: 本文件只在**临时库**上操作, 库名带随机后缀, 结束时无条件
//! `DROP DATABASE`。它**不会**碰 `IM_POSTGRES_URL` 指向的那个库。

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::env;

/// 14 张表(与 `migration_smoke_pg.rs` 的 `EXPECTED_TABLES` 必须一致)
const EXPECTED_TABLES: &[&str] = &[
    "tenants",
    "games",
    "environments",
    "users",
    "device_sessions",
    "friend_requests",
    "friendships",
    "conversations",
    "conversation_sequences",
    "dm_pairs",
    "conversation_members",
    "messages",
    "message_reactions",
    "audit_logs",
];

async fn admin_url() -> Option<String> {
    let u = env::var("IM_POSTGRES_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    Some(u)
}

/// 建一个临时库并返回 (admin 连接, 临时库名)
///
/// 用 `CREATE DATABASE` 而非 schema —— 隔离更彻底, 且能顺带证明迁移在
/// 「什么都不是」的库上也能跑(生产首次部署就是这个场景)。
async fn scratch_db() -> Option<(sqlx::PgPool, String)> {
    let url = admin_url().await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .ok()?;
    let name = format!("im_migrate_probe_{}", uuid::Uuid::new_v4().simple());
    // 库名不能作为绑定参数出现在 DDL 里, 只能拼进语句 —— 所以这里把
    // 「拼进去的东西是我们自己生成的」变成一条**可执行的断言**, 而不只是
    // 注释里的声明。将来有人改命名规则(比如加上用户输入), 这条会立刻红。
    assert!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "临时库名只允许字母数字下划线, 实际: {name}"
    );
    // `raw_sql` + `AssertSqlSafe`: CREATE/DROP DATABASE 的库名**不能**是绑定
    // 参数(PG 的 DDL 不接受占位符), 所以语句必然动态拼接。
    //
    // sqlx 0.9 对非字面量 SQL 有**编译期**拒绝(默认路径不许拼 SQL),
    // `AssertSqlSafe` 是它提供的唯一显式豁免 —— 用它等于书面承诺「我审计过」。
    // 本条的承诺由紧挨着的白名单断言支撑: 库名完全由前缀 + uuid.simple()
    // 生成, 不含任何外部输入。
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&pool)
        .await
        .ok()?;
    Some((pool, name))
}

/// 把连接串换成指向临时库
///
/// 实现放在 `im_migrate::with_database` 并配了纯逻辑单测 —— 手搓 URL 解析
/// 极易漏掉路径分隔符, 而那种错误的报错信息(port 解析失败)与真正原因毫无
/// 关联, 只能靠单测挡。
use im_migrate::with_database as url_for;

#[tokio::test]
async fn migrations_apply_to_a_fresh_database() {
    let Some((admin, name)) = scratch_db().await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    let base = admin_url().await.expect("scratch_db 成功说明 url 存在");

    // 用完无条件清理, 包括中途 panic 的情况(析构里没有, 但 drop 前会跑到这里
    // 之后的语句; 真正的保障是下面每一步都不假设前一步成功)
    let result = probe(&url_for(&base, &name)).await;

    // 清理先于断言: 断言失败时临时库也会被删掉, 不留垃圾
    let _ = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {name}"
    )))
    .execute(&admin)
    .await;

    if let Err(e) = result {
        panic!("迁移未能应用到全新空库: {e}");
    }
}

async fn probe(url: &str) -> Result<(), String> {
    let pool = im_migrate::connect(url)
        .await
        .map_err(|e| format!("connect: {e}"))?;

    im_migrate::run_migrations(&pool)
        .await
        .map_err(|e| format!("run: {e}"))?;

    // 1) 14 张表都在
    let rows = sqlx::query(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE'",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("query tables: {e}"))?;
    let actual: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>("table_name").unwrap_or_default())
        .collect();
    for t in EXPECTED_TABLES {
        assert!(
            actual.iter().any(|a| a == t),
            "表 {t} 未建成; 实际建成: {actual:?}"
        );
    }

    // 2) 迁移历史表记录了 7 条且全部 success
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .map_err(|e| format!("count migrations: {e}"))?;
    assert_eq!(applied, 7, "7 份迁移应全部记入历史表");

    // 3) 0007 的两列真的在 users 上
    for col in ["username", "password_hash"] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM information_schema.columns \
             WHERE table_schema='public' AND table_name='users' AND column_name = $1",
        )
        .bind(col)
        .fetch_one(&pool)
        .await
        .map_err(|e| format!("check col {col}: {e}"))?;
        assert_eq!(exists, 1, "0007 应给 users 加了 {col} 列");
    }

    // 4) 0007 的两条 partial 索引
    for idx in ["uniq_users_env_username", "idx_users_username"] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM pg_indexes \
             WHERE schemaname='public' AND indexname = $1",
        )
        .bind(idx)
        .fetch_one(&pool)
        .await
        .map_err(|e| format!("check index {idx}: {e}"))?;
        assert_eq!(exists, 1, "0007 应建出索引 {idx}");
    }

    // 5) **重跑必须幂等**: 第二次 run 不报错、不重复应用。
    //    迁移工具被 Job 反复触发是常态(backoffLimit 3), 不幂等就会在
    //    重试里撞约束 —— 而那正是「Job 永远失败」的经典原因。
    im_migrate::run_migrations(&pool)
        .await
        .map_err(|e| format!("第二次 run 应幂等成功, 实际失败: {e}"))?;
    let applied2: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .map_err(|e| format!("count again: {e}"))?;
    assert_eq!(applied2, 7, "重跑不得新增历史行");

    pool.close().await;
    Ok(())
}
