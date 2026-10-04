//! `im-migrate` —— 应用 `migrations/` 下全部未执行的迁移
//!
//! 用法:
//! ```text
//! IM_POSTGRES_URL=postgres://... im-migrate
//! im-migrate --database-url postgres://...
//! ```
//!
//! `--database-url` 优先于环境变量。两者都缺则非 0 退出。
//!
//! 用途是 `deploy/k3s/dev/migrate-job.yaml` 的 initContainer 与 CI 的
//! 「run migrations」步骤。退出码:
//! - 0 = 迁移已全部应用(或本次新应用了若干份)
//! - 1 = 失败(连接不上 / SQL 出错 / 未给连接串)
//!
//! **失败必须以非 0 退出**: Job 的 `restartPolicy: OnFailure` 依赖退出码判断,
//! 而网关的 initContainer 会等它成功才启动。用 0 退出一个没跑的迁移, 等于
//! 让网关在旧 schema 上启动。

use std::process::ExitCode;

/// 命令行 `--database-url <URL>`
///
/// 刻意支持它而不只读环境变量: CI 里显式传参能让「连的是哪个库」出现在
/// workflow 文件里, 读日志时不必去猜 job 级的 env 解析结果。
fn arg_database_url() -> Option<String> {
    // 该规则针对的是把 `args` 拼进**安全敏感操作**(如拼命令行走 exec)。
    // 此处是迁移二进制读**自己的** argv 取 `--database-url`, 是 CLI 的本职,
    // 不存在「不可信输入拼进危险调用」的结构。反过来讲: 真正的连接串来自
    // IM_POSTGRES_URL 环境变量, 这里的命令行参数只是显式覆盖, 且下游
    // parse_database_url 只做「找到 flag 后取下一个非空串」, 不做任何拼接执行。
    //
    // 注意: `nosemgrep` **只对紧邻的下一行生效**。第一版把它写在这段说明**上方**,
    // 中间隔了 4 行说明, 关联就断了 —— 扫描仍报出该 finding, 是靠隔离探针
    // (同目录三种写法 + 一个不抑制的对照组)才发现的, 不是什么玄学。
    // nosemgrep: rust.lang.security.args.args
    parse_database_url(std::env::args().skip(1))
}

/// `--database-url` 的解析逻辑(纯函数, 便于单测)
///
/// 抽出来而不是让测试去改 `std::env::args`: 进程参数是**全局**状态, 并行
/// 测试会互相污染; 更重要的是, 若测试里另写一份等价逻辑, 那测的就不是
/// 真正跑的那段代码了。
fn parse_database_url<I: Iterator<Item = String>>(args: I) -> Option<String> {
    let mut it = args;
    while let Some(a) = it.next() {
        if a == "--database-url" {
            return it.next().filter(|v| !v.trim().is_empty());
        }
        if let Some(v) = a.strip_prefix("--database-url=") {
            return Some(v.to_string()).filter(|s| !s.trim().is_empty());
        }
    }
    None
}

/// 已成功应用的迁移份数
///
/// ## 为什么是两次查询, 而不是一条带 CASE 的
///
/// 第一版写成:
/// ```text
/// SELECT CASE WHEN to_regclass('public._sqlx_migrations') IS NULL THEN 0
///            ELSE (SELECT count(*) FROM _sqlx_migrations WHERE success) END
/// ```
/// 看着能兼顾「新库还没这张表」与「老库数行数」, 实际**在全新库上报错**,
/// 被 `unwrap_or(-1)` 吞成 -1。实测: 新库上 `applied_before=-1`,
/// 于是 `applied_now = 7 - (-1) = 8` —— 日志里出现「本次应用了 8 份」这种
/// 明显荒谬的数。
///
/// 原因: **PG 在解析/计划阶段就解析子查询里的表名**, relation 不存在会直接
/// 报错, `CASE` 的短路求值根本没机会生效。表存在与否的判断必须在**独立于该表**
/// 的查询里做。
///
/// -1 与 0 必须区分: 0 是「确实没应用过」, -1 是「查不到, 未知」。把未知说成 0,
/// 恰好会重演本函数要修的那个毛病(数字看着正常, 其实没说真话)。
async fn applied_migration_count(pool: &sqlx::PgPool) -> i64 {
    // 第一步: 这张表在吗? (`to_regclass` 在表不存在时返回 NULL 而不是报错)
    let exists: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations')::text")
            .fetch_one(pool)
            .await
            .ok()
            .flatten();

    match exists {
        None => 0,
        Some(_) => {
            sqlx::query_scalar("SELECT count(*)::bigint FROM _sqlx_migrations WHERE success")
                .fetch_one(pool)
                .await
                .unwrap_or(-1)
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let url = match arg_database_url() {
        Some(u) => u,
        None => match im_migrate::database_url() {
            Ok(u) => u,
            Err(e) => {
                // 不打印连接串本身 —— 它含口令。只说取不到。
                tracing::error!(error = %e, "im-migrate: 配置错误");
                return ExitCode::FAILURE;
            }
        },
    };

    let pool = match im_migrate::connect(&url).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "im-migrate: 连不上数据库");
            return ExitCode::FAILURE;
        }
    };

    // 迁移前的状态检查: 「有 schema 但没有迁移历史」是一个**必须停下来问人**
    // 的状态 —— 它继续跑下去只会报一句指向触发器/索引的 PG 错误, 没人能从中
    // 推出真正原因。见 is_clean_target 的文档。
    match im_migrate::is_clean_target(&pool).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::error!(
                "im-migrate: 目标库**已有业务表但没有迁移历史**。\n\
                 这说明它的 schema 不是本仓的 migrations/ 建的(多半是手工建的, \
                 或用更早版本的 SQL 建的), 迁移历史因此是空的。\n\
                 若直接跑, 会从 0001 重放, 撞上一堆已存在的对象 —— 而报错会长这样:\n  \
                   while executing migration 1: error returned from database:\n    \
                   trigger \"trg_environments_before_update\" for relation \
                 \"environments\" already exists at line 765\n\
                 那句 `while executing migration 1` 极具误导性: 它看着像迁移 1 \
                 写错了, 其实 0001 一行没错, 是这个库的建法不对。\n\
                 (建表/索引有 IF NOT EXISTS 兜着, 只有 CREATE TRIGGER 没有, \
                 所以第一个炸的偏偏是触发器 —— 碰巧而已, 换个库可能先炸索引。)\n\
                 处理方式(二选一):\n  \
                   A) 这个库要废弃 -> 换一个新库(最省事, 也是迁移系统的预期用法)\n  \
                   B) 这个库要保留 -> 先备份, 再手工把已应用的迁移登记进 \
                 _sqlx_migrations(版本号要与文件名一致), 之后本工具才能正确续跑"
            );
            return ExitCode::FAILURE;
        }
        Err(e) => {
            tracing::error!(error = %e, "im-migrate: 迁移前检查失败");
            return ExitCode::FAILURE;
        }
    }

    // 跑之前先记一次「已应用数」, 跑完再记一次, **差值**才是本次真正执行的份数。
    //
    // 2026-10-04 修: 此前这里只查了跑**之后**的
    // `SELECT count(*) FROM _sqlx_migrations WHERE success`, 并把它叫作
    // 「实际执行数」, 注释还写着「两者不等(本次没新增)时, 日志里能直接看出
    // 这次是空跑」。但那条 SQL 数的是**表里全部成功行**, 与本次运行无关 ——
    // 跑完永远是 7, 于是 `total` 与 `applied` **永远相等**, 它声称要解决的
    // 「Job 成功了但好像什么都没做」根本没被解决。
    // 实测证据: 对同一个已迁移完的库跑第二次, 旧日志仍打 `total=7 applied=7`,
    // 而 `_sqlx_migrations` 行数没变(本次实际应用 0 份)。
    //
    // `to_regclass` 分支: 新库上 `_sqlx_migrations` 这个表**还不存在**
    // (它由 sqlx 的 migrator 自己建), 直接查会因 relation 不存在而报错。
    // 那种情况下「已应用数」的正确答案就是 0, 故在 SQL 里判掉。
    let total = im_migrate::MIGRATOR.migrations.len();
    let applied_before = applied_migration_count(&pool).await;
    tracing::info!(
        total,
        applied_before,
        "im-migrate: 开始应用迁移(applied_before = 跑之前已应用的份数)"
    );

    match im_migrate::run_migrations(&pool).await {
        Ok(()) => {
            let applied_after = applied_migration_count(&pool).await;
            let applied_now = applied_after - applied_before;
            tracing::info!(
                total,
                applied_before,
                applied_now,
                applied_total = applied_after,
                "im-migrate: 迁移完成(applied_now = 本次实际应用的份数; \
                 0 = 空跑, 库已是最新)"
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            tracing::error!(error = %e, "im-migrate: 迁移失败");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{arg_database_url, parse_database_url};

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn database_url_flag_forms() {
        assert_eq!(
            parse_database_url(args(&["--database-url", "postgres://a@b/c"]).into_iter()),
            Some("postgres://a@b/c".into())
        );
        assert_eq!(
            parse_database_url(args(&["--database-url=postgres://a@b/c"]).into_iter()),
            Some("postgres://a@b/c".into())
        );
        // 空值必须当「没给」, 而不是拿空串去连库(那会连到默认 socket)
        assert_eq!(
            parse_database_url(args(&["--database-url", "  "]).into_iter()),
            None
        );
        assert_eq!(
            parse_database_url(args(&["--database-url="]).into_iter()),
            None
        );
        // 缺参数值时不得把下一个 flag 当成 URL
        assert_eq!(
            parse_database_url(args(&["--database-url"]).into_iter()),
            None
        );
        // 无关参数不应干扰
        assert_eq!(parse_database_url(args(&["--verbose"]).into_iter()), None);
    }

    /// 无参数时 `arg_database_url()` 返回 None(而不是 panic)。
    ///
    /// 本用例在 cargo test 下运行时进程参数是 `["<exe>", ...test args]`,
    /// 正常不会含 `--database-url`; 它守的是「未知参数不会导致崩溃」。
    #[test]
    fn no_flag_returns_none() {
        assert_eq!(arg_database_url(), None);
    }
}
