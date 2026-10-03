//! 真 PostgreSQL migration 验证 (2026-10-02 新增)
//!
//! 依据: `docs/Project-Status.md` §1.1.1 遗留工程债第 2 条 ——
//!       "6 份 SQL migration 未在真 PG 实例上跑过"。
//!       2026-10-02 已实测: 7/7 migration 在真 PG 18.6 上应用成功, 建成 14 张表,
//!       `crates/im-core/tests/pg_repos_integration.rs` 22 个真 PG 集成测试全绿。
//!       本文件把该验证固化成自动化测试, 补上 `migration_smoke.rs` 明确声明的
//!       "不覆盖: 真实 PG 执行" 缺口。
//!
//! 设计: **不引入新依赖**。`im-gateway` 的 dev-dependencies 已含
//!       `sqlx`(含 `migrate` feature), 直接用 `PgPool` 查询 `information_schema`
//!       / `pg_indexes` 断言真实 schema, 不需要 testcontainers。
//!       (2026-10-04: testcontainers 已作为**全仓无代码引用的死依赖**被删除,
//!        见 docs/gap-ledger.md §1.23 —— 本文件从未依赖过它, 不是这次移除
//!        的受害者。)
//!
//! 环境:
//!   - 设 `DATABASE_URL` 指向**已应用 migration** 的 PG → 跑真实断言
//!   - 未设 `DATABASE_URL` → 打印提示并**跳过**(不算失败), 保证无 PG 的
//!     开发机 / 沙箱跑 `cargo test --workspace` 仍然全绿。
//!   - **`IM_REQUIRE_PG=1` 时「跳过」改为硬失败**: 本机没 PG 时跳过是合理的,
//!     但 CI 明确挂了 PG service container, 此时跳过意味着 job 配错了 —— 而
//!     「跳过」在 test harness 里就是「通过」, 配置错误会以绿灯的形式混过去。
//!   - CI: `.github/workflows/ci.yml` 的 test-integration / test-unit 两个 job
//!     均设 `DATABASE_URL` + `IM_POSTGRES_URL` + `IM_REQUIRE_PG=1`; 建库用
//!     **仓内 `im-migrate`**(2026-10-03 起不再是 `sqlx migrate run` ——
//!     CI 必须跑我们真正要部署的那段代码, 见 §1.21)。
//!
//! 覆盖:
//!   1. 14 张表在真 PG 中确实存在
//!   2. 0007 给 `users` 加的 `username` / `password_hash` 两列确实存在
//!   3. 0007 的两条 partial 索引确实建成
//!   4. 0007 的 DB 层 CHECK 约束**真的生效**: 非法 username / 超长 password_hash
//!      被数据库拒绝 (验证 0007 设计决策 "DB 是真理之源" 而非仅应用层校验)
//!
//! 注意: 本测试只**读** schema + 在 users 表上做 INSERT 试探(每次用随机 UUID
//!       建独立 tenant/game/env 链, 不影响其它测试), 不改动 migration 产物。

use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::env;

/// 14 张表 (ImSpec §1.1 / aux-02 §F.1-F.14)。与 `migration_smoke.rs` 的
/// `EXPECTED_TABLES` 必须保持一致 —— 那边的静态检查保证 SQL 里有 CREATE TABLE,
/// 这边保证 SQL 真的在 PG 上建出了表。
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

/// 0007 新增的 partial 索引
const EXPECTED_0007_INDEXES: &[&str] = &["uniq_users_env_username", "idx_users_username"];

/// 未设 `DATABASE_URL` 时返回 `None`, 调用方据此跳过。
fn database_url() -> Option<String> {
    env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// 连不上 PG 时: 静默跳过, 还是直接失败
///
/// `IM_REQUIRE_PG=1` 时 panic 而不是跳过。
///
/// **为什么不能靠 grep CI 输出**: `cargo test` 默认丢弃**通过**测试的
/// stdout/stderr, 而「跳过」在 libtest 眼里就是「通过」—— 那句
/// `eprintln!("SKIP: ...")` 根本不会出现在正常输出里。实测全量
/// `cargo test --workspace` 日志里 skip 行数为 0, 而当时确有大批用例在跳过。
/// 只有 `--nocapture` 才看得见, 但那会让 CI 输出不可靠。故做在代码里。
///
/// 完整背景与教训见 `src/http/test_support.rs::skip_or_fail_pg` 与
/// `docs/gap-ledger.md` §1.15 / §1.22。
fn skip_or_fail_pg(context: &str) {
    if std::env::var("IM_REQUIRE_PG").as_deref() == Ok("1") {
        panic!(
            "IM_REQUIRE_PG=1 但连不上 PG({context})。这个环境**声称**要跑 PG 测试, \
             连不上只能是配置错了; 静默跳过会让「跑过」与「没跑」无法区分。"
        );
    }
    eprintln!("SKIP: {context}");
}

async fn pool() -> Option<sqlx::PgPool> {
    let url = database_url()?;
    match PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
    {
        Ok(p) => Some(p),
        Err(e) => {
            skip_or_fail_pg(&format!(
                "设了 DATABASE_URL 但连不上 ({e})。\
                 确认该实例已 `sqlx migrate run` 过。"
            ));
            None
        }
    }
}

/// 造一条独立的 tenant -> game -> environment 链, 返回 environment_id。
/// 用随机 UUID, 每次独立, 不与其它测试冲突。
///
/// `environments.name` 受白名单 CHECK 约束
/// (`name = ANY(ARRAY['production','staging','test'])`, 见 migration 0001),
/// 故这里只能取 `'test'` —— 这也正是 `pg_repos_integration.rs::make_env` 的用法。
async fn seed_env(p: &sqlx::PgPool) -> uuid::Uuid {
    sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name)
            VALUES (gen_random_uuid(), 'chk-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'chk-' || gen_random_uuid()::text FROM t
            RETURNING id
        )
        INSERT INTO environments (id, game_id, name)
        SELECT gen_random_uuid(), g.id, 'test' FROM g
        RETURNING id
        "#,
    )
    .fetch_one(p)
    .await
    .expect("seed tenant/game/environment 失败 —— schema 可能已损坏")
}

// ============================================================================
// 1. 14 张表确实存在
// ============================================================================

#[tokio::test]
async fn all_14_tables_exist_in_real_pg() {
    let Some(p) = pool().await else {
        eprintln!("SKIP: 无 DATABASE_URL, 跳过真 PG 表存在性验证");
        return;
    };

    let rows = sqlx::query(
        "SELECT tablename FROM pg_tables \
         WHERE schemaname = 'public' AND tablename = ANY($1)",
    )
    .bind(EXPECTED_TABLES)
    .fetch_all(&p)
    .await
    .expect("查询 pg_tables 失败");

    let present: Vec<String> = rows
        .iter()
        .map(|r| r.get::<String, _>("tablename"))
        .collect();

    let missing: Vec<&&str> = EXPECTED_TABLES
        .iter()
        .filter(|t| !present.iter().any(|p| p == *t))
        .collect();

    assert!(
        missing.is_empty(),
        "真 PG 中缺少这些表: {missing:?} (实际存在: {present:?})"
    );
    assert_eq!(
        present.len(),
        EXPECTED_TABLES.len(),
        "表数量不符: 期望 {} 实得 {}",
        EXPECTED_TABLES.len(),
        present.len()
    );
}

// ============================================================================
// 2. 0007 的两列存在
// ============================================================================

#[tokio::test]
async fn migration_0007_columns_exist_on_users() {
    let Some(p) = pool().await else {
        eprintln!("SKIP: 无 DATABASE_URL");
        return;
    };

    for col in ["username", "password_hash"] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_schema='public' AND table_name='users' AND column_name=$1)",
        )
        .bind(col)
        .fetch_one(&p)
        .await
        .expect("查 information_schema.columns 失败");
        assert!(exists, "users 表缺少 0007 新增的列 `{col}`");
    }
}

// ============================================================================
// 3. 0007 的两条 partial 索引建成
// ============================================================================

#[tokio::test]
async fn migration_0007_partial_indexes_exist() {
    let Some(p) = pool().await else {
        eprintln!("SKIP: 无 DATABASE_URL");
        return;
    };

    for idx in EXPECTED_0007_INDEXES {
        let row = sqlx::query(
            "SELECT indexdef FROM pg_indexes \
             WHERE schemaname='public' AND tablename='users' AND indexname=$1",
        )
        .bind(idx)
        .fetch_optional(&p)
        .await
        .expect("查 pg_indexes 失败")
        .unwrap_or_else(|| panic!("users 表缺少 0007 索引 `{idx}`"));

        let def: String = row.get("indexdef");
        // 0007 两个索引都是 partial (WHERE username IS NOT NULL)
        assert!(
            def.contains("WHERE"),
            "索引 `{idx}` 应是 partial index (WHERE username IS NOT NULL), 实际: {def}"
        );
        // uniq_users_env_username 必须是 UNIQUE
        if *idx == "uniq_users_env_username" {
            assert!(
                def.contains("UNIQUE"),
                "uniq_users_env_username 应是 UNIQUE 索引, 实际: {def}"
            );
        }
    }
}

// ============================================================================
// 4. 0007 的 DB 层 CHECK 约束真的生效 (核心价值: 证明"DB 是真理之源")
// ============================================================================

/// 往 `users` 插一行的唯一入口。
///
/// 必须显式给 `kind`: 该列 NOT NULL 且受 `users_kind_check`
/// (`kind = ANY(ARRAY['user','guest'])`) 约束。若漏掉, 插入会先因 `kind`
/// 失败 —— 于是"非法 username 应被拒"这类**反向断言会假通过**
/// (被拒的原因根本不是 username 约束)。集中在此以杜绝该类假阳性。
async fn insert_user(
    p: &sqlx::PgPool,
    env_id: uuid::Uuid,
    username: Option<&str>,
    password_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO users (id, environment_id, kind, username, password_hash) \
         VALUES (gen_random_uuid(), $1, 'user', $2, $3)",
    )
    .bind(env_id)
    .bind(username)
    .bind(password_hash)
    .execute(p)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn migration_0007_db_check_rejects_invalid_username() {
    let Some(p) = pool().await else {
        eprintln!("SKIP: 无 DATABASE_URL");
        return;
    };
    let env_id = seed_env(&p).await;

    // 非法字符集 —— 0007 规定 [a-zA-Z0-9_-],故 'bad user!' 含空格与感叹号
    let r = insert_user(&p, env_id, Some("bad user!"), Some("x")).await;
    assert!(
        r.is_err(),
        "DB 层应拒绝含非法字符的 username, 实际插入成功 —— 0007 的 CHECK 约束未生效"
    );

    // 长度 < 3 —— 0007 规定 length BETWEEN 3 AND 64
    let r = insert_user(&p, env_id, Some("ab"), Some("x")).await;
    assert!(r.is_err(), "DB 层应拒绝长度 < 3 的 username");

    // 合法 username 必须放行 (防止"约束过严导致功能不可用"; 同时这条断言
    // 能兜住上面两条反向断言的假阳性 —— 若 INSERT 其实因别的原因恒失败,
    // 这里就会红)
    let ok = insert_user(&p, env_id, Some("valid_user-01"), Some("argon2id$abc")).await;
    assert!(ok.is_ok(), "合法 username 被拒, 约束过严: {:?}", ok.err());
}

#[tokio::test]
async fn migration_0007_db_check_rejects_oversized_password_hash() {
    let Some(p) = pool().await else {
        eprintln!("SKIP: 无 DATABASE_URL");
        return;
    };
    let env_id = seed_env(&p).await;

    // 0007: password_hash 长度 <= 256
    let long_hash = "x".repeat(257);
    let r = insert_user(&p, env_id, Some("pw_test_01"), Some(&long_hash)).await;
    assert!(r.is_err(), "DB 层应拒绝长度 > 256 的 password_hash");

    // 边界: 恰好 256 应放行 (证明约束是 <= 256 而非 < 256, 也排除"恒拒绝"假阳性)
    let ok_hash = "x".repeat(256);
    let ok = insert_user(&p, env_id, Some("pw_test_02"), Some(&ok_hash)).await;
    assert!(
        ok.is_ok(),
        "长度恰为 256 的 password_hash 应放行, 实际被拒: {:?}",
        ok.err()
    );
}

#[tokio::test]
async fn migration_0007_username_unique_within_environment() {
    let Some(p) = pool().await else {
        eprintln!("SKIP: 无 DATABASE_URL");
        return;
    };
    let env_id = seed_env(&p).await;

    // 同一 environment 内 username 必须唯一
    insert_user(&p, env_id, Some("dup_user_01"), Some("x"))
        .await
        .expect("首个 username 应插入成功");

    let r = insert_user(&p, env_id, Some("dup_user_01"), Some("y")).await;
    assert!(
        r.is_err(),
        "同一 environment 内重复 username 应被 partial unique 索引拒绝"
    );

    // NULL username 必须允许多条 (Guest 用户无 username, 0007 设计决策)
    for _ in 0..2 {
        insert_user(&p, env_id, None, None)
            .await
            .expect("NULL username 应可重复 (partial index WHERE username IS NOT NULL)");
    }

    // 不同 environment 内同名 username 应放行 (唯一性只约束 env 内)
    let env_id2 = seed_env(&p).await;
    insert_user(&p, env_id2, Some("dup_user_01"), Some("z"))
        .await
        .expect("不同 environment 下的同名 username 应放行");
}
