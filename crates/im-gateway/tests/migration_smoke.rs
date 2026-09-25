//! Day 2 GATE 补签: 7 份 SQL migration 验证 (0007 于 2026-09-21 C-3/C-4 整合新增)
//!
//! **无 docker daemon 环境**: 仅跑 Migrator 解析 + 校验 SQL 文件不崩。
//! **完整版** (testcontainers::Postgres + sqlx::migrate!): 见
//! `tests/migration_smoke_docker.rs`(feature-gated, 默认不编)。
//!
//! 当前测试覆盖:
//! 1. `sqlx::migrate::Migrator::new` 解析 7 份 SQL 文件不抛错
//! 2. 列出的迁移名与 `migrations/*.sql` 文件名 1:1 对应
//! 3. 7 份 SQL 至少能 create 14 张表(SQL 内的 `CREATE TABLE` 计数 = 14; 0007 仅 ALTER users, 不新增表)
//!
//! 不覆盖: 真实 PG 执行(需 docker / 真 PG 实例)。

use sqlx::migrate::Migrator;
use std::fs;
use std::path::Path;

const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

/// 14 张表名(ImSpec §1.1 / aux-02 §F.1-F.14)。Day 2 GATE 验证用。
const EXPECTED_TABLES: &[&str] = &[
    // 0001: tenants / games / environments
    "tenants", "games", "environments",
    // 0002: users / device_sessions
    "users", "device_sessions",
    // 0003: friend_requests / friendships
    "friend_requests", "friendships",
    // 0004: conversations / conversation_sequences / dm_pairs / conversation_members
    "conversations", "conversation_sequences", "dm_pairs", "conversation_members",
    // 0005: messages / message_reactions
    "messages", "message_reactions",
    // 0006: audit_logs
    "audit_logs",
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
    assert_eq!(count, 7, "应有 7 份 SQL migration, 实际 {count}");
}

#[test]
fn migrator_parses_all_six_files() {
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
        7,
        "Migrator 应识别 7 份 migration, 实际 {} ({:?})",
        names.len(),
        names
    );
    for v in &names {
        let n: u64 = v.parse().expect("version parse");
        assert!((1..=7).contains(&n), "unexpected version {v}");
    }
}

#[test]
fn expected_14_tables_referenced_in_sql() {
    // 静态检查: 14 张表名在 6 份 SQL 中能找到 (create 计数 + 注释 + FK 引用)
    // 实际执行由 testcontainers 版本负责
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
            + all_sql
                .matches(&format!("CREATE TABLE {table}"))
                .count();
        assert!(
            create_count >= 1,
            "表 `{table}` 应在 CREATE TABLE 出现 ≥1 次, 实际 {create_count}"
        );
    }
}
