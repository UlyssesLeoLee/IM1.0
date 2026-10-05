//! 迁移的**真实**端到端验证 —— 2026-10-03 新增
//!
//! ## 为什么这条测试与既有测试不同
//!
//! `crates/im-gateway/tests/migration_smoke_pg.rs` 是查 schema 的, 它要求
//! **`DATABASE_URL` 指向一个「已应用迁移」的库** —— 谁应用的、什么时候应用的,
//! 它一概不问。也就是说: 那条测试能证明「schema 是对的」, 但证明不了
//! 「**我们的代码能把它变成对的**」。
//!
//! 本文件补上后半段: 建一个**全新的空库**, 用 `im_migrate` 把 8 份迁移真的
//! 跑一遍, 然后断言 15 张表建成、0007 的列与索引都在、DB 层 CHECK 真的生效。
//!
//! 顺带说明一件之前没人提过的事: `migrate-job.yaml` 引用一个**无法构建**的
//! 镜像, 而 `cargo test` 里**没有任何代码会应用迁移** —— 也就是说, 当时那 7 份
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

/// 15 张表(与 `migration_smoke_pg.rs` 的 `EXPECTED_TABLES` 必须一致)
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
    // 0008(2026-10-06): aux-08 §D.3 的 DLQ 长留存层
    "dlq_records",
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

/// 在一个**全新临时库**上执行 `body`, 结束后**无条件**删库
///
/// 返回 `None` = 跳过(没配 `IM_POSTGRES_URL` 或连不上), 与既有 e2e 约定一致。
///
/// 契约上有一条硬要求: `body` **不许 panic**, 只能返回 `Err`。因为清理语句
/// 写在 `body` 之后, `body` 一旦 panic 就会跳过清理, 在 PG 上留下一个孤儿库
/// —— 反复跑测试会把实例的库越堆越多, 而没人会去查是谁留下的。把「断言」
/// 交给调用方在 `body` 返回之后做, 就同时拿到了「失败也清理」和「失败有
/// 清晰信息」。
///
/// 参数传**拥有的 `String`** 而非 `&str`: 场景体是 async block, 若拿到借用
/// 来的 `&str`, 编译器会要求 future 的生命周期短于调用点, 闭包形式写起来
/// 要么过不了 lifetime 检查, 要么得靠 HRTB 把签名撑大。传所有权就没这回事 ——
/// 顺带这也让每个场景能写成独立的 `async fn`(fn item 直接满足 `FnOnce`),
/// 比匿名闭包好读, 报错时函数名也能直接指到是哪个场景。
async fn in_scratch_db<T, F, Fut>(body: F) -> Option<T>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let (admin, name) = scratch_db().await?;
    let base = admin_url().await.expect("scratch_db 成功说明 url 存在");
    let out = body(url_for(&base, &name)).await;
    // 清理先于返回: 断言在调用方做, 断言失败时库已经被删掉了。
    let _ = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {name}"
    )))
    .execute(&admin)
    .await;
    Some(out)
}

#[tokio::test]
async fn migrations_apply_to_a_fresh_database() {
    let Some(result) = in_scratch_db(probe).await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    if let Err(e) = result {
        panic!("迁移未能应用到全新空库: {e}");
    }
}

async fn probe(url: String) -> Result<(), String> {
    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| format!("connect: {e}"))?;

    im_migrate::run_migrations(&pool)
        .await
        .map_err(|e| format!("run: {e}"))?;

    // 1) 15 张表都在
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

    // 2) 迁移历史表记录了 8 条且全部 success
    //
    // 2026-10-06: 0008(DLQ 的 PG 长留存层)加入后由 7 → 8。
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .map_err(|e| format!("count migrations: {e}"))?;
    assert_eq!(applied, 8, "8 份迁移应全部记入历史表");

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
    //
    // 2026-10-06: 这里**曾经**写死 7, 于是加 0008 时只改了上面 line 177 的那个
    // 8, 忘了这个副本 —— CI 红。而这条断言要表达的根本不是「历史里该有 7 条」,
    // 是「**重跑不改变条数**」。写死常量让意图与数字脱钩: 迁移一变就得同步两处,
    // 漏一处就红, 且红的原因(7) 指向不到真正的意图。
    //
    // 故改为引用第一次的值。迁移数量变化时这条**永不需要**再改。
    assert_eq!(
        applied2, applied,
        "重跑不得新增历史行: 第一次 {applied} 条, 第二次 {applied2} 条"
    );

    pool.close().await;
    Ok(())
}

// ============================================================================
// is_clean_target: 迁移前该拦的拦, 不该拦的别拦
// ============================================================================
//
// 起因是一次真实失败: 对本机 `im_test` 库跑 `im-migrate` 报
// `trigger "trg_environments_before_update" for relation "environments"
// already exists at line 765` —— 一句完全指不出原因的 PG 报错。
// 真实原因: 该库的 schema 是手工建的, 从未记进 `_sqlx_migrations`,
// 迁移器于是从 0001 重放, 撞上一堆已存在的对象。
//
// 迁移文件用 `IF NOT EXISTS` 兜住了建表和索引, 但 **CREATE TRIGGER 没有
// 这种写法**, 所以第一个触发器就炸。触发器成了「最先撞墙的那个」纯属
// 巧合 —— 报错指向它只是因为它排在最后, 而前面那些「已存在」都被静默吞了。
//
// 所以这些用例要同时钉两件事:
//   1. 拦得住(判 false)
//   2. **被拦下的那件事确实会发生**(不拦就会炸, 且报错指不到点上)
// 只断言第 1 条, 检查就成了无法证伪的仪式。

/// 在指定库上跑一段静态 DDL
///
/// `raw_sql` 在 sqlx 0.9 里对非字面量 SQL 有编译期拒绝, `AssertSqlSafe`
/// 是唯一显式豁免。本文件的 SQL 全部是字面量, 不含任何外部输入。
async fn ddl(pool: &sqlx::PgPool, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(pool)
        .await
        .map(|_| ())
}

/// 手工建出来的 `environments` + 触发器, 逐字照抄 0001 的结构
///
/// 刻意保持**能通过前 3 个 `IF NOT EXISTS`**(表 / 索引)的样子, 这样
/// `run_migrations` 才会一路走到 `CREATE TRIGGER` 才炸 —— 复现的才是那次
/// 真实的失败路径, 而不是随便一个更早的报错。
const HAND_BUILT_ENVIRONMENTS: &str = "CREATE TABLE environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
)";

const HAND_BUILT_FUNC: &str = "CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql";

const HAND_BUILT_TRIGGER: &str = "CREATE TRIGGER trg_environments_before_update
BEFORE UPDATE ON environments
FOR EACH ROW EXECUTE FUNCTION set_updated_at()";

/// sqlx 的迁移历史表(空表, 无任何行)
const EMPTY_HISTORY_TABLE: &str = "CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL
)";

async fn check_clean(url: String) -> Result<bool, String> {
    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let r = im_migrate::is_clean_target(&pool)
        .await
        .map_err(|e| format!("is_clean_target: {e}"));
    pool.close().await;
    r
}

/// 状态 1: 全新空库 → 可以跑
#[tokio::test]
async fn clean_target_accepts_a_completely_empty_database() {
    let Some(r) = in_scratch_db(check_clean).await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    assert_eq!(r, Ok(true), "全新空库没有任何东西可丢, 必须放行");
}

/// 状态 2: 被迁移系统管着的库(7 条历史)→ 可以跑
#[tokio::test]
async fn clean_target_accepts_a_migrated_database() {
    let Some(r) = in_scratch_db(scenario_migrated).await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    assert_eq!(r, Ok(true), "已有成功迁移历史的库必须放行(续跑是常态)");
}

async fn scenario_migrated(url: String) -> Result<bool, String> {
    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    im_migrate::run_migrations(&pool)
        .await
        .map_err(|e| format!("run: {e}"))?;
    let r = im_migrate::is_clean_target(&pool)
        .await
        .map_err(|e| format!("is_clean_target: {e}"));
    pool.close().await;
    r
}

/// 状态 3: 全新空库, 但**残留一张空的** `_sqlx_migrations` → 仍然可以跑
///
/// 这条是「上一次运行中途失败」留下的痕迹: sqlx 先建出历史表, 在迁移 1 上
/// 失败后回滚业务表, 却留下空的历史表。此时库里**没有任何业务表**, 跑迁移
/// 完全安全 —— 拦它只会逼人手工清一张空表, 属于假阳性。
///
/// 把它与状态 5 并排看才说得清: 决定放行与否的是**业务表在不在**,
/// 不是历史表在不在。
#[tokio::test]
async fn clean_target_accepts_a_fresh_database_with_a_leftover_empty_history_table() {
    let Some(r) = in_scratch_db(scenario_leftover_empty_history).await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    assert!(
        r.expect("场景内各步都返回 Ok"),
        "只有一张空历史表、没有业务表的库应当放行; 拦它会逼人手工清空表"
    );
}

async fn scenario_leftover_empty_history(url: String) -> Result<bool, String> {
    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    ddl(&pool, EMPTY_HISTORY_TABLE)
        .await
        .map_err(|e| format!("建空历史表: {e}"))?;
    let r = im_migrate::is_clean_target(&pool)
        .await
        .map_err(|e| format!("is_clean_target: {e}"));
    pool.close().await;
    r
}

/// 状态 4: 手工建的 schema, 无迁移历史 → 拦下, 且**证明不拦就会炸**
#[tokio::test]
async fn clean_target_rejects_hand_built_schema_and_that_verdict_is_earned() {
    let Some(r) = in_scratch_db(scenario_hand_built).await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    let (verdict, boom) = r.expect("场景内各步都返回 Ok");

    assert_eq!(verdict, Ok(false), "手工建的 schema 没有迁移历史, 必须拦下");
    // 对照组: 报错指向触发器, 而触发器只是**最后**撞墙的那个; 前面 3 个
    // `IF NOT EXISTS` 全都静默放行了。
    //
    // 更糟的是 sqlx 加的前缀 `while executing migration 1:` —— 它让人以为
    // 「迁移 1 写错了, 去改 SQL」。而这个库恰恰是**迁移 1 完全正确**、
    // 目标库建法不对。往错误方向指比不指更费时间。
    //
    // 故断言 PG 那半句里**没有任何**能让人反推出真实原因的词。
    let pg_part = boom
        .split_once("error returned from database:")
        .map(|(_, rest)| rest)
        .unwrap_or(boom.as_str());
    assert!(
        pg_part.contains("trg_environments_before_update"),
        "预期复现 `CREATE TRIGGER` 无 IF NOT EXISTS 导致的失败, 实际: {boom}"
    );
    for hint in ["_sqlx_migrations", "history", "hand", "已存在业务表"] {
        assert!(
            !pg_part.to_lowercase().contains(&hint.to_lowercase()),
            "PG 报错里出现了 {hint:?}, 它比预期更有指向性, \
             「完全指不出真正原因」的说法需要修正。实际: {boom}"
        );
    }
}

async fn scenario_hand_built(url: String) -> Result<(Result<bool, String>, String), String> {
    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    for stmt in [HAND_BUILT_ENVIRONMENTS, HAND_BUILT_FUNC, HAND_BUILT_TRIGGER] {
        ddl(&pool, stmt)
            .await
            .map_err(|e| format!("手工建表: {e}"))?;
    }
    let verdict = im_migrate::is_clean_target(&pool)
        .await
        .map_err(|e| format!("is_clean_target: {e}"));
    // 故意**跳过**检查直接跑, 看它到底会怎么炸 —— 这才是「拦得值不值」
    // 的唯一证据。
    let boom = im_migrate::run_migrations(&pool)
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| "居然成功了, 未复现那次失败".to_string());
    pool.close().await;
    Ok((verdict, boom))
}

/// 状态 5: 手工建的 schema **+ 残留空历史表** → 拦下
///
/// 这条是「故必须数行」那句话的全部意义。判据若写成「历史表存在即视为
/// 受管理」(第一版), 这里会**放行** —— 而放行之后就是状态 4 里那个
/// 指不到点上的触发器错误。
#[tokio::test]
async fn clean_target_rejects_hand_built_schema_even_with_an_empty_history_table() {
    let Some(r) = in_scratch_db(scenario_hand_built_with_empty_history).await else {
        eprintln!("skip: 未设 IM_POSTGRES_URL 或连不上");
        return;
    };
    assert!(
        !r.expect("场景内各步都返回 Ok"),
        "业务表在 + 历史表空 => 仍须拦下; 只看「历史表是否存在」会误放行"
    );
}

async fn scenario_hand_built_with_empty_history(url: String) -> Result<bool, String> {
    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    for stmt in [
        HAND_BUILT_ENVIRONMENTS,
        EMPTY_HISTORY_TABLE,
        HAND_BUILT_FUNC,
        HAND_BUILT_TRIGGER,
    ] {
        ddl(&pool, stmt)
            .await
            .map_err(|e| format!("手工建表: {e}"))?;
    }
    let r = im_migrate::is_clean_target(&pool)
        .await
        .map_err(|e| format!("is_clean_target: {e}"));
    pool.close().await;
    r
}
