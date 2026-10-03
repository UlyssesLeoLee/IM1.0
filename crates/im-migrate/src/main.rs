//! `im-migrate` —— 应用 `migrations/` 下全部未执行的迁移
//!
//! 用法: `IM_POSTGRES_URL=postgres://... im-migrate`
//!
//! 用途是 `deploy/k3s/dev/migrate-job.yaml` 的 initContainer。退出码:
//! - 0 = 迁移已全部应用(或本次新应用了若干份)
//! - 1 = 失败(连接不上 / SQL 出错 / 未设变量)
//!
//! **失败必须以非 0 退出**: Job 的 `restartPolicy: OnFailure` 依赖退出码判断,
//! 而网关的 initContainer 会等它成功才启动。用 0 退出一个没跑的迁移, 等于
//! 让网关在旧 schema 上启动。

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let url = match im_migrate::database_url() {
        Ok(u) => u,
        Err(e) => {
            // 不打印连接串本身 —— 它含口令。只说取不到。
            tracing::error!(error = %e, "im-migrate: 配置错误");
            return ExitCode::FAILURE;
        }
    };

    let pool = match im_migrate::connect(&url).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "im-migrate: 连不上数据库");
            return ExitCode::FAILURE;
        }
    };

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
