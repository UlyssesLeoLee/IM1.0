//! `jobctl` —— 运维 CLI, 补上 DLQ 的**人工处置出口**
//!
//! 依据: `aux-08 §G 重跑步骤模板` + `aux-08 §K GAP-1`
//!
//! ## 为什么现在做这个
//!
//! `migrations/0008` 建了 `dlq_records` 长留存层, `PgDlqSink` 往里写死信。
//! 但**只写不读**: 运维看得见 `im_events_dlq_write_failed_total` 在涨, 却
//! 没有任何办法看到「到底哪些事件死了」或把它们重新发出去。GAP-1 原文写的是
//! 「`jobctl` CLI 工具 V1+ 实装, MVP 阶段手动跑 SQL/API —— 运维无可视化」。
//! 手动跑 SQL 的实际后果是: 那张表没有 partial 索引以外的读路径, 一条
//! `SELECT * FROM dlq_records` 在死信量大时会把运维自己的终端淹掉。
//!
//! ## 范围: 只做 `dlq` 子树
//!
//! `aux-08 §G` 的模板里还有 `status` / `reset` / `run` / `verify` 四条, 它们
//! 针对 JOB-001..004 四个**定时批处理**, 而 §A.1 明写「**MVP 阶段不实装**
//! JOB-001 ~ JOB-004; 本表为 V1+ 设计占位」。批处理本身都不存在, 给不存在的
//! 作业做 `run` 只会造出一个永远报「作业不存在」的命令。
//!
//! 故本工具只实装 `dlq list` / `dlq replay` / `dlq discard` 三条 —— 也就是
//! `dlq_records` 表的三种结局(待处置 / 已重放 / 已丢弃)。
//!
//! ## 退出码
//!
//! - `0` = 成功
//! - `1` = 操作失败(连不上库 / id 不存在 / 已处置过 / 发布失败)
//! - `2` = **用法**错误(子命令拼错 / 缺参数 / `--since` 格式不对)
//!
//! 把 2 单列出来, 是为了让脚本能区分「人敲错了」与「操作真的失败了」:
//! 前者重敲一次即可, 后者需要人去查。

use std::fmt;

use chrono::{DateTime, Utc};
use uuid::Uuid;

pub mod dlq;

/// `dlq list` 的默认条数
///
/// 规范(§G)的模板只给了 `--since`, 没有 `--limit`。但**没有上限的 list 在一张
/// 长留存表上是危险的**: `dlq_records` 按设计会一直攒(它带 `replayed_at` /
/// `discarded_at` 才需要清理), 死信上万时 `SELECT` 会把运维的终端冲垮, 而
/// 更糟的是人会习惯性 `| head` —— 于是后面的行根本没人看见, 工具反而制造了
/// 「已处置」的错觉。
pub const DEFAULT_LIMIT: i64 = 50;

/// `--limit` 的硬上界
///
/// 超过这个数几乎一定是敲错了(`--limit 500000` 少个 0)或想导出全量(那该走
/// `psql` 配 `\copy`)。在这里挡住, 好过让一条命令占满内存。
pub const MAX_LIMIT: i64 = 10_000;

/// 一次 CLI 调用的解析结果
#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    /// `--database-url` 显式覆盖; `None` 时回落到 `IM_POSTGRES_URL`
    pub database_url: Option<String>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Help,
    DlqList(ListOptions),
    DlqReplay(Uuid),
    DlqDiscard(Uuid),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListOptions {
    /// `failed_at >= since`; `None` = 不限
    pub since: Option<DateTime<Utc>>,
    pub limit: i64,
    /// 默认只看**待处置**的(`replayed_at IS NULL AND discarded_at IS NULL`)
    pub include_settled: bool,
    pub json: bool,
}

/// 用法错误 —— 与「操作失败」分开, 对应退出码 2
#[derive(Debug, Clone, PartialEq)]
pub struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

pub const USAGE: &str = "\
jobctl —— IM1.0 运维 CLI (aux-08 §G)

用法:
  jobctl dlq list [--since <RFC3339>] [--limit <n>] [--all] [--json]
  jobctl dlq replay  <dlq_id>      重放单条死信(重新发布到原 subject)
  jobctl dlq discard <dlq_id>      永久丢弃(写 discarded_at)
  jobctl --help

选项:
  --database-url <URL>   覆盖 IM_POSTGRES_URL
  --since <RFC3339>      只看 failed_at >= 该时刻(如 2026-09-01T00:00:00Z)
  --limit <n>            最多返回 n 条(默认 50, 上界 10000)
  --all                  含已重放/已丢弃的; 默认只看**待处置**的
  --json                 以 JSON 输出(供脚本消费)

退出码: 0 成功 / 1 操作失败 / 2 用法错误";

/// 解析命令行参数(纯函数)
///
/// 抽成纯函数而不是让测试去改 `std::env::args`: 进程参数是**全局**状态, 并行
/// 测试会互相污染; 更要紧的是, 测试里另写一份等价逻辑, 测的就不是真正跑的
/// 那段代码了。
pub fn parse_args(args: &[String]) -> Result<Invocation, UsageError> {
    let (database_url, rest) = split_database_url(args);

    let first = match rest.first() {
        None => {
            return Ok(Invocation {
                database_url,
                command: Command::Help,
            })
        }
        Some(f) if *f == "--help" || *f == "-h" => {
            return Ok(Invocation {
                database_url,
                command: Command::Help,
            })
        }
        Some(f) => *f,
    };

    let tail = &rest[1..];
    let mut inv = match first {
        "dlq" => parse_dlq(tail)?,
        other => {
            return Err(UsageError(format!(
                "未知命令 {other:?}。本工具当前只实装了 `dlq` 子树 —— \
                 aux-08 §A.1 明写 JOB-001..004「MVP 阶段不实装」, \
                 故 status/reset/run/verify 没有对应的批处理可操作。\n\n{USAGE}"
            )))
        }
    };
    // `parse_dlq` 不该关心连接串, 故在最后统一回填 —— 这样它内部构造
    // `Invocation` 时不必为这个无关字段编造值。
    inv.database_url = database_url;
    Ok(inv)
}

fn parse_dlq(args: &[&str]) -> Result<Invocation, UsageError> {
    let sub = args
        .first()
        .copied()
        .ok_or_else(|| UsageError(format!("`dlq` 后面缺子命令。\n\n{USAGE}")))?;

    match sub {
        "list" => parse_dlq_list(&args[1..]).map(|command| Invocation {
            database_url: None,
            command,
        }),
        "replay" | "discard" => {
            let id = parse_uuid_arg(args.get(1).copied(), sub, args.get(2).copied())?;
            let command = if sub == "replay" {
                Command::DlqReplay(id)
            } else {
                Command::DlqDiscard(id)
            };
            Ok(Invocation {
                database_url: None,
                command,
            })
        }
        other => Err(UsageError(format!(
            "未知的 `dlq` 子命令 {other:?}。可用: list / replay / discard"
        ))),
    }
}

fn parse_dlq_list(args: &[&str]) -> Result<Command, UsageError> {
    let mut opts = ListOptions {
        since: None,
        limit: DEFAULT_LIMIT,
        include_settled: false,
        json: false,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--all" => opts.include_settled = true,
            "--json" => opts.json = true,
            "--since" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| UsageError("--since 后面缺时间戳".into()))?;
                opts.since = Some(parse_since(v)?);
                i += 1;
            }
            "--limit" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| UsageError("--limit 后面缺数字".into()))?;
                opts.limit = parse_limit(v)?;
                i += 1;
            }
            other => {
                return Err(UsageError(format!(
                    "`dlq list` 不认识参数 {other:?}。可用: --since / --limit / --all / --json"
                )))
            }
        }
        i += 1;
    }

    Ok(Command::DlqList(opts))
}

/// `--since` 只收 RFC3339(带时区)
///
/// 刻意**不**收 `2026-09-01` 这种裸日期: 死信时间线的排障里, 「9 月 1 号之后」
/// 到底按 UTC 还是按运维本地时区算, 差一天就足以让人漏掉一批死信。缺时区就
/// 报错, 好过猜一个。
pub fn parse_since(v: &str) -> Result<DateTime<Utc>, UsageError> {
    DateTime::parse_from_rfc3339(v.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            UsageError(format!(
                "--since 的 {v:?} 不是合法 RFC3339 时间戳: {e}\n\
                 例: 2026-09-01T00:00:00Z(必须带时区; 裸日期 2026-09-01 会被拒绝, \
                 因为死信时间线按错时区算会整批漏掉)"
            ))
        })
}

/// `--limit` 必须是 1..=MAX_LIMIT
///
/// 0 与负数单独给出错误, 而不是「无限制」: 运维敲 `--limit 0` 的真实意图
/// 几乎总是想看全部, 而那正是要防的事。
pub fn parse_limit(v: &str) -> Result<i64, UsageError> {
    let n: i64 = v
        .trim()
        .parse()
        .map_err(|e| UsageError(format!("--limit 的 {v:?} 不是整数: {e}")))?;
    if n < 1 {
        return Err(UsageError(format!(
            "--limit 必须是正整数, 收到 {n}。\
             「不限制」不是可选项 —— 这张表会一直攒, 不限条数的 list 会淹掉终端。"
        )));
    }
    if n > MAX_LIMIT {
        return Err(UsageError(format!(
            "--limit 上界是 {MAX_LIMIT}, 收到 {n}(多半是少敲了一个 0)。\
             要导出全量请直接用 psql 配 \\copy。"
        )));
    }
    Ok(n)
}

fn parse_uuid_arg(first: Option<&str>, sub: &str, extra: Option<&str>) -> Result<Uuid, UsageError> {
    let raw = first.ok_or_else(|| {
        UsageError(format!(
            "`dlq {sub}` 后面缺 dlq_id。用 `jobctl dlq list` 查 id。"
        ))
    })?;
    if extra.is_some() {
        return Err(UsageError(format!(
            "`dlq {sub}` 只接受一个 dlq_id, 多余的参数被忽略会让敲错的人以为生效了"
        )));
    }
    Uuid::parse_str(raw).map_err(|e| {
        UsageError(format!(
            "dlq_id {raw:?} 不是合法 UUID: {e}\n\
             (id 形如 550e8400-e29b-41d4-a716-446655440000)"
        ))
    })
}

/// 摘出 `--database-url` **并把它从参数表里移除**(可出现在任意位置)
///
/// 刻意支持显式传参而不只读环境变量: CI 与 k8s Job 里显式传参能让「连的是哪个
/// 库」出现在清单文件里, 读日志时不必去猜环境变量的解析结果。
///
/// ## 为什么必须**移除**而不能只是「读出来」
///
/// 第一版只读不移, 于是 `jobctl dlq list --database-url X` 会在
/// `parse_dlq_list` 里被当成「不认识的参数」而报错 —— 连接串明明是合法的,
/// 工具却说用法错误。是 `database_url_is_extracted_from_any_position` 抓到的:
/// 它断言的是「任意位置都能用」, 而实现只支持「第一个位置」。
///
/// 两个错误凑在一起会最迷惑人: 连接串被正确读到了(看起来像成功), 却仍然退出 2。
/// 凡是被某个解析器**消费掉**的参数, 都必须从后续解析器的输入里拿掉。
fn split_database_url(args: &[String]) -> (Option<String>, Vec<&str>) {
    let mut url = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.iter().map(String::as_str).peekable();

    while let Some(a) = it.next() {
        if a == "--database-url" {
            // 取下一个作值; 若没有下一个(flag 落在末尾), url 保持 None
            if let Some(v) = it.next() {
                if !v.trim().is_empty() {
                    url = Some(v.to_string());
                }
            }
            continue;
        }
        if let Some(v) = a.strip_prefix("--database-url=") {
            if !v.trim().is_empty() {
                url = Some(v.to_string());
            }
            continue;
        }
        rest.push(a);
    }
    (url, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_three_dlq_subcommands() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(
            parse_args(&args(&["dlq", "list"])).unwrap().command,
            Command::DlqList(ListOptions {
                since: None,
                limit: DEFAULT_LIMIT,
                include_settled: false,
                json: false,
            })
        );
        assert_eq!(
            parse_args(&args(&["dlq", "replay", id])).unwrap().command,
            Command::DlqReplay(Uuid::parse_str(id).unwrap())
        );
        assert_eq!(
            parse_args(&args(&["dlq", "discard", id])).unwrap().command,
            Command::DlqDiscard(Uuid::parse_str(id).unwrap())
        );
    }

    /// 默认只看待处置 —— 这是**列表存在的主要理由**
    ///
    /// 若默认变成「全都要」, 运维每次都要先 `| grep pending`, 而漏筛一次就会
    /// 拿着已丢弃的死信去重放。
    #[test]
    fn list_defaults_to_pending_only() {
        let Command::DlqList(o) = parse_args(&args(&["dlq", "list"])).unwrap().command else {
            panic!("应当是 DlqList");
        };
        assert!(!o.include_settled, "默认必须只看待处置");
        assert_eq!(o.limit, DEFAULT_LIMIT, "默认必须有限条数");
    }

    #[test]
    fn list_accepts_all_flags_in_any_order() {
        let Command::DlqList(o) = parse_args(&args(&[
            "dlq",
            "list",
            "--json",
            "--all",
            "--limit",
            "5",
            "--since",
            "2026-09-01T00:00:00Z",
        ]))
        .unwrap()
        .command
        else {
            panic!("应当是 DlqList");
        };
        assert!(o.json && o.include_settled);
        assert_eq!(o.limit, 5);
        assert_eq!(o.since.unwrap().to_rfc3339(), "2026-09-01T00:00:00+00:00");
    }

    /// 裸日期必须被拒: 时区猜错会整批漏掉死信
    #[test]
    fn since_rejects_naive_dates_and_keeps_offsets() {
        assert!(parse_since("2026-09-01").is_err());
        assert!(parse_since("not-a-time").is_err());
        // 带偏移的应当被规范化成 UTC
        let o = parse_since("2026-09-01T09:00:00+09:00").unwrap();
        assert_eq!(o.to_rfc3339(), "2026-09-01T00:00:00+00:00");
    }

    #[test]
    fn limit_rejects_zero_negative_and_overflow() {
        assert!(parse_limit("0").is_err());
        assert!(parse_limit("-1").is_err());
        assert!(parse_limit("abc").is_err());
        assert!(parse_limit(&(MAX_LIMIT + 1).to_string()).is_err());
        assert_eq!(parse_limit("1").unwrap(), 1);
        assert_eq!(parse_limit(&MAX_LIMIT.to_string()).unwrap(), MAX_LIMIT);
    }

    #[test]
    fn database_url_is_extracted_from_any_position() {
        let u = "postgres://a@b/c".to_string();
        for form in [
            vec!["--database-url", "postgres://a@b/c", "dlq", "list"],
            vec!["dlq", "list", "--database-url", "postgres://a@b/c"],
            vec!["--database-url=postgres://a@b/c", "dlq", "list"],
        ] {
            assert_eq!(
                parse_args(&args(&form)).unwrap().database_url,
                Some(u.clone()),
                "形式 {form:?} 应能解析出连接串"
            );
        }
        // 空值当作「没给」, 而不是拿空串去连库
        assert_eq!(
            parse_args(&args(&["--database-url", "  ", "dlq", "list"]))
                .unwrap()
                .database_url,
            None
        );
    }

    /// 敲错时必须**报错**, 不能退化成「少做一点事还成功」
    ///
    /// 2026-10-06 实测过的同类形态: 解析结果为 0 却当成「没有违规」放行。
    /// 这里守着同一个原则 —— 解析不出来就红。
    #[test]
    fn usage_errors_are_rejected_not_silently_ignored() {
        // 规范 §G 里的批处理子命令: 本工具**刻意不实装**, 必须明确报错
        for absent in ["status", "run", "reset", "verify"] {
            let e = parse_args(&args(&["jobctl-absent", absent])).unwrap_err();
            assert!(e.to_string().contains("只实装了"), "应说明只实装了 dlq");
        }
        assert!(parse_args(&args(&["dlq", "nope"])).is_err());
        assert!(parse_args(&args(&["dlq", "replay"])).is_err());
        assert!(parse_args(&args(&["dlq", "replay", "not-a-uuid"])).is_err());
        assert!(parse_args(&args(&["dlq", "list", "--since"])).is_err());
        assert!(parse_args(&args(&["dlq", "list", "--limit"])).is_err());
        assert!(parse_args(&args(&["dlq", "list", "--typo"])).is_err());
        // 多余参数不能被静默忽略
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert!(parse_args(&args(&["dlq", "discard", id, "extra"])).is_err());
    }

    /// 对照组: 上面那些必须报错的形式, 正确形式必须通过
    ///
    /// 没有这条, 「凡输入皆报错」的解析器也能让上一条全绿 —— 那样的解析器
    /// 毫无用处。判别力来自「错误集」与「通过集」互不相交。
    #[test]
    fn valid_forms_still_parse() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert!(parse_args(&args(&["dlq", "list"])).is_ok());
        assert!(parse_args(&args(&["dlq", "replay", id])).is_ok());
        assert!(parse_args(&args(&["dlq", "discard", id])).is_ok());
        assert!(parse_args(&args(&["dlq", "list", "--limit", "3"])).is_ok());
        assert!(parse_args(&args(&[])).is_ok(), "无参数应给 help 而不是报错");
    }
}
