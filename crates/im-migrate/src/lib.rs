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

/// 迁移前的状态检查: 这个库是「干净的」还是「建法不对」
///
/// 2026-10-03 新增。起因是一次真实失败: 对本机 `im_test` 库跑 `im-migrate`
/// 报 `trigger "trg_environments_before_update" for relation "environments"
/// already exists at line 765` —— 一句**完全指不出原因**的 PG 报错。
///
/// 真实原因: 该库的 schema 是**手工建的**(或用更早版本的 SQL 建的), 从未
/// 记录进 `_sqlx_migrations`。于是迁移器认为「什么都没应用过」, 从 0001
/// 开始重放, 撞上一堆已存在的对象。
///
/// 迁移文件用 `IF NOT EXISTS` 兜住了建表和索引, 但**触发器没有这种写法**
/// (PG 的 `CREATE TRIGGER` 没有 `IF NOT EXISTS`), 所以第一个触发器就炸。
/// 触发器之所以成了「最先报错的那个」纯属排在最后 —— 前面那些「已存在」
/// 全被 `IF NOT EXISTS` 静默吞了。
///
/// 2026-10-03 实测的完整报错:
///
/// ```text
/// while executing migration 1: error returned from database:
///   trigger "trg_environments_before_update" for relation "environments"
///   already exists at line 765
/// ```
///
/// 注意 sqlx 加的 `while executing migration 1:` 前缀: 它把人往「迁移 1
/// 写错了, 去改 SQL」上引, 而**迁移 1 完全正确**、是目标库建法不对。往错误
/// 方向指比不指更费时间 —— 那半句 PG 原文里没有任何词提到迁移历史。
/// 也就是说: 迁移对**全新库**是安全的(已由 e2e 验证), 对「手工建的库」必然
/// 失败, 而失败信息指向的是触发器, 没人会想到真正的原因是历史缺失。
///
/// (上述两句断言由 `tests/migrations_e2e.rs` 的
/// `clean_target_rejects_hand_built_schema_and_that_verdict_is_earned` 守着:
/// 它在真实 PG 上复现这条报错, 并断言 PG 原文里不出现 `_sqlx_migrations` /
/// `history` / `hand` 这类能反推真实原因的词。)
///
/// 返回值: `Ok(true)` = 可以安全开跑; `Ok(false)` = 检测到「有 schema 但无
/// 迁移历史」, 调用方应拒绝并给出人话指引。
///
/// ## 判据是**迁移历史的行数 + 业务表的有无**, 不是「历史表是否存在」
///
/// 第一版写成「`_sqlx_migrations` 表存在即视为受管理」, 结果**没触发** ——
/// 因为一次失败的运行会先建出这张表, 在迁移 1 上失败后回滚业务表, 却把空
/// 的历史表留在原地。于是「手工建的库 + 残留空历史表」被误判为受管理,
/// 放行之后就是上面那个指不到点上的触发器错误。故必须数行。
///
/// 两种「空历史表」必须区别对待, 这也是本函数最容易写错的地方:
///
/// | 状态 | 业务表 | 历史表行数 | 判定 | 理由 |
/// |---|---|---|---|---|
/// | 全新库 | 0 | 表不存在或 0 | **放行** | 没有任何东西可丢, 跑迁移是安全的 |
/// | 受管库 | ≥14 | 7 | **放行** | 续跑是 Job 重试的常态 |
/// | 手工建的库 | ≥1 | 表不存在或 0 | **拦下** | schema 来自别处, 迁移历史是空的 |
///
/// 一句话: **决定放行与否的是「业务表在不在」, 不是「历史表在不在」**。
/// 空历史表本身不是问题(它是失败运行的正常残骸), 有业务表却没历史才是。
///
/// 上面 5 种状态由 `tests/migrations_e2e.rs` 在真实 PG 上逐一验证, 其中
/// 「手工建的库」那条还额外断言了**不拦就真的会炸**, 以及报错里提不到
/// `migration` —— 否则这个检查只是一句无法证伪的自我安慰。
pub async fn is_clean_target(pool: &sqlx::PgPool) -> Result<bool, sqlx::Error> {
    // 迁移历史里**有几条**记录。表不存在时下面这条会报错, 故先探表。
    let history_table: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;

    let applied: i64 = if history_table == 0 {
        0
    } else {
        sqlx::query_scalar("SELECT count(*)::bigint FROM _sqlx_migrations")
            .fetch_one(pool)
            .await?
    };

    // 已有成功记录 → 这个库确实由迁移系统管着, 交给 run() 续跑即可。
    //
    // 这里**必须数行, 不能看表在不在**。变异验证过: 判据换成
    // `history_table > 0`(第一版)后, 「手工建的库 + 残留空历史表」会被
    // 误放行, 紧接着就是那段指不到点上的触发器报错 ——
    // 由 `clean_target_rejects_hand_built_schema_even_with_an_empty_history_table`
    // 精确捕获。
    if applied > 0 {
        return Ok(true);
    }

    // 没有记录, 但已有业务表 → schema 是从别处来的(或上次跑崩了留下的残局)。
    // 排除 `_sqlx_migrations` 自身, 否则「表存在但为空」会被误判成有业务表。
    let business: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' \
           AND table_name <> '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    Ok(business == 0)
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

    /// 迁移器里确实编进了 8 份 SQL, 且版本号连续。
    ///
    /// 这条是纯逻辑断言(不连库), 守的是一件很具体的事: 有人往 `migrations/`
    /// 加了第 9 份、或者编号跳号时, 这里的数字会提醒你去更新文档里那些
    /// 「8/8 migration」的表述 —— 否则文档会开始说谎。
    ///
    /// 2026-10-06: 0008(DLQ 的 PG 长留存层)加入后由 7 → 8。它**当场**变红,
    /// 与这条注释写的一字不差 —— 这正是它该有的样子。
    #[test]
    fn migrator_embeds_all_eight_migrations() {
        assert_eq!(
            MIGRATOR.migrations.len(),
            8,
            "migrations/ 应有 8 份; 新增/删除时同步更新 \
             migration_smoke_pg.rs 的 EXPECTED_TABLES 注释与文档中的 '8/8'"
        );
        let versions: Vec<i64> = MIGRATOR.migrations.iter().map(|m| m.version).collect();
        assert_eq!(
            versions,
            vec![1, 2, 3, 4, 5, 6, 7, 8],
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
