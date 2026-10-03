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

    // 跑之前先报总数, 跑之后报实际执行数 —— 两者不等(本次没新增)时, 日志
    // 里能直接看出「这次是空跑」。CI 里最常见的困惑是「Job 成功了但好像
    // 什么都没做」, 这行就是给那种困惑用的。
    let total = im_migrate::MIGRATOR.migrations.len();
    tracing::info!(total, "im-migrate: 开始应用迁移");

    match im_migrate::run_migrations(&pool).await {
        Ok(()) => {
            let applied: i64 =
                sqlx::query_scalar("SELECT count(*)::bigint FROM _sqlx_migrations WHERE success")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(-1);
            tracing::info!(total, applied, "im-migrate: 迁移完成");
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
