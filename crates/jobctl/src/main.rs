//! `jobctl` 二进制 —— 参数解析在 [`jobctl::parse_args`], 逻辑在 [`jobctl::dlq`]
//!
//! 本文件只做三件事: 初始化 tracing、按解析结果分派、把结果映射成退出码。
//! 刻意保持薄 —— 任何能被单测覆盖的判断都不该留在这里。

use std::process::ExitCode;

use im_core::event::publisher::{EventPublisher, NatsEventPublisher};
use jobctl::{dlq, Command, Invocation, USAGE};
use uuid::Uuid;

/// `IM_EVENT_PUBLISHER` 里我们唯一关心的那一小块
///
/// 刻意复用 `im_common::config::EventPublisherConfig` 这个**真实类型**而不是
/// 自己写一个 `{ nats_url: Option<String> }`: 那样一来, 配置的 JSON 形状若
/// 变了, 本文件会在**编译期**断, 而不是运行时安静地拿到 None。
#[derive(serde::Deserialize)]
struct NatsEnv {
    event_publisher: im_common::config::EventPublisherConfig,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // 该规则针对把 `args` 拼进**安全敏感操作**(如 exec 拼命令行)。此处是
    // 运维 CLI 读**自己的** argv 取 `--database-url`, 是 CLI 的本职, 不存在
    // 「不可信输入拼进危险调用」的结构; 下游 parse 只做「找到 flag 取下一个
    // 非空串」, 不做任何拼接执行。
    //
    // `nosemgrep` **只对紧邻的下一行生效** —— 2026-10-06 踩过: 第一版把它写在
    // 本段说明上方、中间隔了几行, 关联断了, semgrep 照样报出 finding。
    // im-migrate/src/main.rs 早就把这条写成了注释, 这里再记一次。
    // nosemgrep: rust.lang.security.args.args
    let raw: Vec<String> = std::env::args().skip(1).collect();

    let invocation = match jobctl::parse_args(&raw) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("jobctl: {e}");
            return ExitCode::from(2);
        }
    };

    match run(invocation).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(m)) => {
            eprintln!("jobctl: {m}");
            ExitCode::from(2)
        }
        Err(Failure::Operational(m)) => {
            eprintln!("jobctl: {m}");
            ExitCode::FAILURE
        }
    }
}

enum Failure {
    /// 退出码 2: 人敲错了, 重敲一次即可
    Usage(String),
    /// 退出码 1: 操作真的失败了, 需要人去查
    Operational(String),
}

async fn run(inv: Invocation) -> Result<(), Failure> {
    let Invocation {
        database_url,
        command,
    } = inv;

    if let Command::Help = command {
        println!("{USAGE}");
        return Ok(());
    }

    // 解析不出连接串是**用法**问题(没给必要的东西), 不是操作失败
    let url = match database_url {
        Some(u) => u,
        None => im_migrate::database_url().map_err(Failure::Usage)?,
    };

    let pool = im_migrate::connect(&url)
        .await
        .map_err(|e| Failure::Operational(format!("连不上数据库: {e}")))?;

    let result = match command {
        Command::Help => unreachable!("上面已处理"),
        Command::DlqList(opts) => list_cmd(&pool, &opts).await,
        Command::DlqReplay(id) => replay_cmd(&pool, id).await,
        Command::DlqDiscard(id) => discard_cmd(&pool, id).await,
    };

    pool.close().await;
    result
}

async fn list_cmd(pool: &sqlx::PgPool, opts: &jobctl::ListOptions) -> Result<(), Failure> {
    let rows = dlq::list(pool, opts).await.map_err(Failure::Operational)?;

    if rows.is_empty() {
        println!("(没有符合条件的死信记录)");
        return Ok(());
    }

    if opts.json {
        let out: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "dlq_id": r.dlq_id.to_string(),
                    "state": r.state(),
                    "original_task": r.original_task,
                    "dlq_destination": r.dlq_destination,
                    "error_code": r.error_code,
                    "error_message": r.error_message,
                    "error_http_status": r.error_http_response_code,
                    "context_attempt_count": r.context_attempt_count,
                    "failed_at": r.failed_at.to_rfc3339(),
                    "replay_attempts": r.replay_attempts,
                    "replayed_at": r.replayed_at.map(|t| t.to_rfc3339()),
                    "discarded_at": r.discarded_at.map(|t| t.to_rfc3339()),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(());
    }

    println!(
        "{:<36}  {:<9}  {:<28}  {:<5}  {:<4}  FAILED_AT",
        "DLQ_ID", "STATE", "ORIGINAL_TASK", "ATT", "HTTP"
    );
    for r in &rows {
        println!(
            "{:<36}  {:<9}  {:<28}  {:<5}  {:<4}  {}",
            r.dlq_id,
            r.state(),
            truncate(&r.original_task, 28),
            r.context_attempt_count,
            r.error_http_response_code,
            r.failed_at.to_rfc3339()
        );
    }
    println!();
    println!("{} 条。用 `--all` 可看已重放/已丢弃的。", rows.len());
    if rows.len() as i64 == opts.limit {
        println!(
            "(正好等于 --limit {} —— 可能还有更多, 加大 limit 或用 --since)",
            opts.limit
        );
    }
    Ok(())
}

/// 截断过长字段, 让表格不被一条超长 payload 说明撑变形
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{keep}…")
}

async fn replay_cmd(pool: &sqlx::PgPool, id: Uuid) -> Result<(), Failure> {
    // 重放要真的把事件发出去, 所以必须连 NATS
    let nats_url = nats_url().map_err(Failure::Operational)?;

    // `None` = 不接 PG 长留存层。这不是省事, 而是**必须**: 若接上, 一次失败
    // 的重放会再写一条死信, 而那条死信又可以重放 —— 操作者重试几次就造出
    // 一条自己打自己、永不停歇的环。
    let publisher = NatsEventPublisher::connect(&nats_url, None)
        .await
        .map_err(|e| Failure::Operational(format!("连不上 NATS({nats_url}): {e}")))?;

    let outcome = dlq::replay_with(pool, id, |topic, bytes| {
        let publisher = &publisher;
        async move {
            publisher
                .publish(&topic, &bytes)
                .await
                .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(Failure::Operational)?;

    match outcome {
        dlq::ReplayOutcome::Published { attempts } => {
            println!("已重放 {id}(累计尝试 {attempts} 次)");
            Ok(())
        }
        dlq::ReplayOutcome::PublishedButNotMarked { attempts } => {
            // 不是失败(事件确实发出去了), 但**必须**让操作者看见: 极可能是
            // 有人同时处置了同一条, 于是这条事件被发了两次。
            tracing::warn!(
                dlq_id = %id,
                attempts,
                "事件已重新发布, 但该行没能标记为已重放 —— 并发下别人先处置了同一条。\
                 结果是一次**重复投递**; 请核对下游是否按 event_id 去重(aux-08 §E.2)。"
            );
            println!("已重放 {id}, 但未能标记(并发冲突) —— 请核对是否重复投递");
            Ok(())
        }
    }
}

async fn discard_cmd(pool: &sqlx::PgPool, id: Uuid) -> Result<(), Failure> {
    match dlq::discard(pool, id).await.map_err(Failure::Operational)? {
        dlq::DiscardOutcome::Discarded => {
            println!("已永久丢弃 {id}");
            Ok(())
        }
        dlq::DiscardOutcome::AlreadyDiscarded => {
            println!("{id} 本就已丢弃, 无变化");
            Ok(())
        }
        dlq::DiscardOutcome::AlreadyReplayed => Err(Failure::Operational(format!(
            "{id} 已经重放过 —— 事件确实发出去了, 不能再标丢弃。\
             否则同一条记录会既说「发了」又说「丢了」, 排障时两边都能引用它。"
        ))),
    }
}

/// 从 `IM_EVENT_PUBLISHER` 取 NATS 地址
///
/// 这是**唯一**的地址来源: `im-gateway` 走 `AppConfig.event_publisher.nats_url`
/// (同一份 `IM_EVENT_PUBLISHER` JSON)。刻意**不**引入 `IM_NATS_URL` 之类的新
/// 变量 —— 那个名字目前只是测试约定, 拿到生产环境里当配置读, 就会出现
/// 「网关连 A、jobctl 连 B」这种最难查的一类错。
fn nats_url() -> Result<String, String> {
    let raw = std::env::var("IM_EVENT_PUBLISHER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "未设 IM_EVENT_PUBLISHER。重放需要连 NATS, \
             而地址只有一个来源(与 im-gateway 同一份配置)。\n\
             例: IM_EVENT_PUBLISHER='{\"kind\":\"nats\",\"nats_url\":\"nats://localhost:4222\"}'"
                .to_string()
        })?;

    let env: NatsEnv =
        serde_json::from_str(&raw).map_err(|e| format!("IM_EVENT_PUBLISHER 不是合法 JSON: {e}"))?;

    env.event_publisher
        .nats_url
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            "IM_EVENT_PUBLISHER 里没有 nats_url(只有 kind=stub 时会这样)。\
             重放必须真的发到 NATS, 用 stub 跑等于把「已重放」写在纸上。"
                .to_string()
        })
}
