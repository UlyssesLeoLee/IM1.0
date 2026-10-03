//! 迁移执行库 —— 供二进制与测试共用
//!
//! ## 为什么需要它(而不是在 k8s 里跑 `sqlx migrate run`)
//!
//! `deploy/k3s/dev/migrate-job.yaml` 引用 `im1.0-im-migrate:latest` 执行
//! `sqlx migrate run`, 但**仓内没有任何东西能构建那个镜像** —— 没有 migrate
//! 二进制, 也没有 CI build job。要凑合就得依赖 `sqlx/sqlx-cli` 基础镜像,
//! 那又要求把 `migrations/` 挂进去, 镜像与代码的版本对应关系就断了。
//!
//! 用 `sqlx::migrate!` 把 SQL **编译期内嵌进二进制**: 镜像只有一个文件,
//! 且它携带的迁移与编译它的代码**必然同版本**。这不是优化, 是把
//! 「部署的 schema 版本」与「构建的代码版本」在物理上绑死 —— 否则会出现
//! 「镜像里的 SQL 比镜像里的代码旧」这种最难排查的状态。
//!
//! ## 环境变量名与 `AppConfig` 一致
//!
//! 用 `IM_POSTGRES_URL` 而**不是** sqlx CLI 惯用的 `DATABASE_URL`: 整个仓的
//! 配置读取都走 `IM_<UPPER_SNAKE>` 规则(见 `im_common::config`), 迁移工具
//! 没有理由成为例外。注意 `migrate-job.yaml` 此前写的是 `IM_DATABASE_URL`
//! —— 与 `AppConfig.postgres_url` 对不上, 已一并修正。

use std::time::Duration;

use sqlx::migrate::Migrator;

/// 编译期内嵌 `migrations/` 下的 7 份 SQL
///
/// 路径相对 `CARGO_MANIFEST_DIR`(即 `crates/im-migrate/`), 故要上溯两级。
/// 目录里多一份文件, 这个宏会在**编译期**把它算进校验和 —— 改了 SQL 而忘记
/// 重建镜像, 会在构建时炸, 而不是上线后行为诡异地不一致。
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// 应用全部未执行的迁移
///
/// 用 `run()` 而不是 `run_direct`: 后者跳过迁移, 直接执行 SQL, 不写
/// `_sqlx_migrations` 表 —— 于是无法判断「哪些已应用」, 重复跑会撞约束。
/// 带迁移历史表是「迁移可重放」的前提。
///
/// `ignore_missing` 显式设 false(它本来就是默认值, 但写出来是为了让
/// 「缺失即失败」这个决定在代码里可见): 已应用的迁移文件若从仓库里消失,
/// 必须报错而不是跳过 —— 那说明有人在删历史。
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

/// 从 `IM_POSTGRES_URL` 读连接串
pub fn database_url() -> Result<String, String> {
    std::env::var("IM_POSTGRES_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "IM_POSTGRES_URL 未设置(或为空)。\
             变量名与 im_common::config::AppConfig::postgres_url 一致; \
             注意不是 sqlx CLI 惯用的 DATABASE_URL。"
                .to_string()
        })
}

/// 建一个连接池
pub async fn connect(url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .connect(url)
        .await
}

/// 把连接串的**库名**换成另一个, 其余(scheme/用户/口令/主机/端口)原样保留
///
/// 用于「在同一个 PG 实例上另开一个临时库」这类场景(测试隔离、影子库)。
/// 单独抽成纯函数并配单测, 原因是它极易写错且**错了不报编译错** ——
/// 手搓 URL 解析的典型失败是漏掉路径分隔符, 拼出
/// `postgres://u:p@host:5544dbname`, 而 sqlx 报的错是
/// `invalid port number`, 与「URL 里少了个斜杠」之间毫无提示关联。
pub fn with_database(base: &str, dbname: &str) -> String {
    // 先剥 scheme; 剩下的 `user:pass@host:port/path?query` 里, 第一个 `/`
    // 之前是 authority, 之后是路径。
    let scheme_end = match base.find("://") {
        Some(i) => i + 3,
        // 没有 scheme(例如 `host=... key=value` 形式的 DSN)时无路径可换。
        // 原样返回而不是猜 —— 猜错的连接串会连到**错误的库**上, 那比报错糟。
        None => return base.to_string(),
    };
    let after_scheme = &base[scheme_end..];
    match after_scheme.find('/') {
        // 保留到路径分隔符**之前**, 然后补一个新的 `/` + 新库名。
        Some(j) => format!("{}/{dbname}", &base[..scheme_end + j]),
        // 压根没有路径(如 `postgres://u:p@host:5544`)→ 直接追加。
        None => format!("{base}/{dbname}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 迁移器里确实编进了 7 份 SQL, 且版本号连续。
    ///
    /// 这条是纯逻辑断言(不连库), 守的是一件很具体的事: 有人往 `migrations/`
    /// 加了第 8 份、或者编号跳号时, 这里的数字会提醒你去更新文档里那些
    /// 「7/7 migration」的表述 —— 否则文档会开始说谎。
    #[test]
    fn migrator_embeds_all_seven_migrations() {
        assert_eq!(
            MIGRATOR.migrations.len(),
            7,
            "migrations/ 应有 7 份; 新增/删除时同步更新 \
             migration_smoke_pg.rs 的 EXPECTED_TABLES 注释与文档中的 '7/7'"
        );
        let versions: Vec<i64> = MIGRATOR.migrations.iter().map(|m| m.version).collect();
        assert_eq!(
            versions,
            vec![1, 2, 3, 4, 5, 6, 7],
            "版本号必须连续且从 1 开始, 否则 run() 会认为缺失的版本可跳过"
        );
    }

    /// 未设 `IM_POSTGRES_URL` 时必须给出**可操作的**错误信息。
    ///
    /// 断言的是错误文本里包含变量名: 迁移工具最常见的失败原因就是变量名写错
    /// 或用了 `DATABASE_URL`(sqlx CLI 惯例)。错误信息里没有名字, 排查就得
    /// 去翻文档。
    #[test]
    fn missing_database_url_error_names_the_variable() {
        let saved = std::env::var("IM_POSTGRES_URL").ok();
        std::env::remove_var("IM_POSTGRES_URL");
        let err = database_url().expect_err("未设变量必须报错");
        assert!(
            err.contains("IM_POSTGRES_URL"),
            "错误信息必须点名变量, 实际: {err}"
        );
        if let Some(v) = saved {
            std::env::set_var("IM_POSTGRES_URL", v);
        }
    }

    /// URL 换库名的几种形态。
    ///
    /// 这组断言的存在是因为该函数曾写成漏掉路径分隔符的版本, 拼出
    /// `postgres://im:im@localhost:5544probe`, 而 sqlx 报的错是
    /// `invalid port number` —— 错误信息与真正原因之间没有任何可追踪关联。
    /// 纯函数 + 单测是这类 bug 唯一便宜的护栏。
    /// `postgres://im:im@localhost:5544probe`, 而 sqlx 报的是
    /// `invalid port number` —— 错误信息与真正原因之间没有任何可追踪的关联。
    /// 纯函数 + 单测是这类 bug 唯一便宜的护栏。
    #[test]
    fn with_database_replaces_only_the_path() {
        // 带路径: 只换库名, 端口与凭据原样保留
        assert_eq!(
            with_database("postgres://im:im@localhost:5544/im_test", "probe"),
            "postgres://im:im@localhost:5544/probe"
        );
        // 没有路径: 追加
        assert_eq!(
            with_database("postgres://im:im@localhost:5544", "probe"),
            "postgres://im:im@localhost:5544/probe"
        );
        // 查询参数在库名之后, 本函数**丢弃**它 —— 因为原库名与参数一起被
        // 换掉了。断言这个行为, 免得将来有人以为参数被保留了。
        assert_eq!(
            with_database("postgres://u@h:5432/old?sslmode=disable", "new"),
            "postgres://u@h:5432/new"
        );
        // 无 scheme 的 DSN: 原样返回, 不猜
        assert_eq!(
            with_database("host=h user=u", "new"),
            "host=h user=u",
            "无法定位路径分隔符时应原样返回, 猜错会连到错误的库"
        );
    }
}
