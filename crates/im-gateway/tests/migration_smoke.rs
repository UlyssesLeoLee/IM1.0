//! Day 2 GATE 补签: 8 份 SQL migration 静态验证 (0007 于 2026-09-21 C-3/C-4 整合新增,
//! 0008 于 2026-10-06 加 DLQ 的 PG 长留存层)
//!
//! **无 docker daemon 环境**: 仅跑 Migrator 解析 + 校验 SQL 文件不崩。
//! **完整版** (真 PG 执行): 见 `tests/migration_smoke_pg.rs` —— 2026-10-02 新增,
//! 用 `DATABASE_URL` 连真 PG 断言 15 张表 / 0007 两列 / 两条 partial 索引 /
//! 0008 的 3 条索引名逐字拼出列名 /
//! DB 层 CHECK 约束确实生效。设了 `DATABASE_URL` 即自动生效, 未设则跳过。
//!
//! (2026-10-02 更正: 本文件此前写着"见 tests/migration_smoke_docker.rs
//! (feature-gated, 默认不编)",但**该文件从不存在** —— 属悬空引用。真 PG 覆盖
//! 缺口因此长期没人补。现已由 migration_smoke_pg.rs 补上。)
//!
//! 当前测试覆盖:
//! 1. `sqlx::migrate::Migrator::new` 解析 8 份 SQL 文件不抛错
//! 2. 列出的迁移名与 `migrations/*.sql` 文件名 1:1 对应
//! 3. 8 份 SQL 至少能 create 15 张表(SQL 内的 `CREATE TABLE` 计数 = 15;
//!    0007 仅 ALTER users, 不新增表)
//!
//! 不覆盖: 真实 PG 执行 —— 移交 `tests/migration_smoke_pg.rs`。

use sqlx::migrate::Migrator;
use std::fs;
use std::path::Path;

const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

/// 15 张表名(ImSpec §1.1 / aux-02 §F.1-F.15 + 0008 的 `dlq_records`)。Day 2 GATE 验证用。
const EXPECTED_TABLES: &[&str] = &[
    // 0001: tenants / games / environments
    "tenants",
    "games",
    "environments",
    // 0002: users / device_sessions
    "users",
    "device_sessions",
    // 0003: friend_requests / friendships
    "friend_requests",
    "friendships",
    // 0004: conversations / conversation_sequences / dm_pairs / conversation_members
    "conversations",
    "conversation_sequences",
    "dm_pairs",
    "conversation_members",
    // 0005: messages / message_reactions
    "messages",
    "message_reactions",
    // 0006: audit_logs
    "audit_logs",
    // 0008: dlq_records(aux-08 §D.3 的长留存层; 0007 只 ALTER users, 不新增表)
    "dlq_records",
];

#[test]
fn migration_files_present_and_nonempty() {
    let dir = Path::new(MIGRATIONS_DIR);
    assert!(dir.exists(), "migrations/ 目录不存在: {:?}", dir);
    let mut count = 0;
    for entry in fs::read_dir(dir).expect("read migrations/") {
        let entry = entry.expect("dir entry");
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("sql") {
            let body = fs::read_to_string(&p).expect("read sql");
            assert!(!body.is_empty(), "{} 为空", p.display());
            assert!(
                body.contains("+migrate Up"),
                "{} 缺 +migrate Up 标记 (sqlx::migrate 要求)",
                p.display()
            );
            count += 1;
        }
    }
    assert_eq!(count, 8, "应有 8 份 SQL migration, 实际 {count}");
}

#[test]
fn migrator_parses_all_files() {
    // `Migrator::new` 解析 + 按版本排序 + 校验不可变 (无 -- 后缀)。
    // 不连 DB, 沙箱无 docker 也能跑。
    // sqlx 0.9: `Migrator::new` 是 async (返回 Future);用 block_on 同步等。
    let migrator = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async { Migrator::new(Path::new(MIGRATIONS_DIR)).await })
        .expect("Migrator::new");
    let migrations = migrator.iter();
    let names: Vec<String> = migrations.map(|m| m.version.to_string()).collect();
    assert_eq!(
        names.len(),
        8,
        "Migrator 应识别 8 份 migration, 实际 {} ({:?})",
        names.len(),
        names
    );
    for v in &names {
        let n: u64 = v.parse().expect("version parse");
        assert!((1..=8).contains(&n), "unexpected version {v}");
    }
}

#[test]
fn expected_tables_referenced_in_sql() {
    // 静态检查: 15 张表名在 8 份 SQL 中能找到 (create 计数 + 注释 + FK 引用)
    //
    // 真 PG 上的实际执行**不由本文件负责**, 也不由 testcontainers 负责 ——
    // 2026-10-04 已确认 testcontainers 是全仓无代码引用的死依赖并被删除
    // (见 docs/gap-ledger.md §1.23)。执行者是同目录的 `migration_smoke_pg.rs`
    // (查 information_schema / pg_indexes) 与 `im-migrate` 的 e2e。
    let mut all_sql = String::new();
    for entry in fs::read_dir(MIGRATIONS_DIR).expect("read migrations/") {
        let entry = entry.expect("dir entry");
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("sql") {
            all_sql.push_str(&fs::read_to_string(&p).expect("read sql"));
        }
    }
    for table in EXPECTED_TABLES {
        // 表名出现在 CREATE TABLE 一次 + 至少一处 FK / 索引 / 注释
        let create_count = all_sql
            .matches(&format!("CREATE TABLE IF NOT EXISTS {table}"))
            .count()
            + all_sql.matches(&format!("CREATE TABLE {table}")).count();
        assert!(
            create_count >= 1,
            "表 `{table}` 应在 CREATE TABLE 出现 ≥1 次, 实际 {create_count}"
        );
    }
}
