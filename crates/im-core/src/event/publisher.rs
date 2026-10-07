//! EventPublisher — 异步事件发布抽象
//!
//! 依据: ImplementationSpec §7.4.5 + DetailedDesign §9.1
//!
//! ## D-3 已实装 (2026-10-04)
//!
//! 此前 `NatsEventPublisher` 是 stub: `connect()` 不发起连接, `publish()` 把
//! 每条事件丢掉并返回 `Ok(())`。aux-04 §B.4 把「publish 事件供其他 pod 同步」
//! 写成**不变量**, 而这条不变量从未被满足 —— 且返回值 / 日志 / 指标三条渠道
//! 全都指向「正常」。现在改为真实的 NATS **JetStream** 发布
//! (`BasicDesign` 技术栈基线 + WBS D-3 + `ImplementationSpec §7.4.5`
//! "publish via NATS JetStream" + `deploy/k3s/dev/nats.yaml` 的 `--jetstream`)。
//!
//! ## 三个刻意的设计决定
//!
//! 1. **发布等 JetStream ack, 但有界超时。** `DetailedDesign §9.1` 要求
//!    「失败不阻塞 ack」, 而 `jetstream::Context::publish()` 返回的是要等服务端
//!    ack 的 future —— 不设上界的话, NATS 变慢会**顺着业务请求路径**传导成
//!    延迟尖峰。故用 `tokio::time::timeout` 给出确定上界, 超时按失败计。
//! 2. **Stream 由程序幂等创建, 不依赖运维预置。** `nats.yaml` 只传了
//!    `--jetstream --store_dir=/data`, **没有**任何 stream 配置; 安装包的
//!    `install.sh` 也不建 stream。若假定 stream 已存在, 则首次部署时每条事件
//!    都会拿到 `no responders` —— 又一次静默失效。故 `connect()` 里调
//!    `create_or_update_stream`（先 update, NotFound 时 create）。
//! 3. **`kind=stub` 与 `kind=nats` 是两种不同的类型。** 此前两者都调
//!    `NatsEventPublisher::connect`, 于是「我要 stub」这句话的实现是
//!    「去连 nats://stub:4222」。现在拆出 `StubEventPublisher`, stub 路径
//!    真的不连任何东西, 而 `kind=nats` 连不上会**启动失败** —— 配了 NATS 却
//!    静默退化成 stub, 正是本文件长期存在的那类缺陷。
//!
//! ## DLQ 已实装 (2026-10-05)
//!
//! 此前失败路径是: 有界超时 / 服务端错误 -> 计数 + `error!` + 返回
//! `Err(ServiceUnavailable)`, 调用方(`MessageService`)只记日志不阻塞 ack。
//! 即失败**可见、可测量**, 但**不可恢复** —— 事件永久丢失。
//!
//! 现按 `aux-08`(批处理重试 / 死信队列策略)实装:
//! - **§C.3**: NATS 事件推送失败重试 3 次, 退避 500ms / 1s / 2s
//! - **§D.1**: 重试耗尽仍失败 -> 写 DLQ
//! - **§D.2**: DLQ Record 结构逐字段照抄
//! - **§D.3**: 独立 DLQ stream (`IM_DLQ`, subject 过滤 `dlq.>`), 留存 7 天
//!
//! ### 关键设计: 重试**必须有总预算**, 因为它在业务请求路径上
//!
//! `publish()` 是被 `MessageService::send_message` **内联 await** 的
//! (`message/service.rs:232`, 在 `tx.commit()` 之后), 所以在这里同步重试会
//! **直接变成客户端的响应延迟**。而 `DetailedDesign §9.1` 要求「失败不阻塞
//! ack」。
//!
//! 照 §C.3 字面实现(3 次重试 x 每次 3s ack 超时 + 3.5s 退避)最坏要 **~15.5s**
//! —— 比实装前的 3s 差一个数量级, 大量客户端与中间代理会先超时, 于是「为了
//! 不丢事件」反而制造了「请求超时」这个更常见的问题。
//!
//! 故引入**墙钟总预算** [`RetryPolicy::total_budget`]:
//!
//! | 场景 | 行为 |
//! |---|---|
//! | NATS 正常 | 第 1 次即成功, ~1ms, **零变化** |
//! | 连接被拒(快速失败 ~5ms) | 退避 500ms/1s/2s 后重试, 总计 ~3.5s -> 恢复 |
//! | NATS 变慢(每次撞 3s 上界) | 预算耗尽即转 DLQ, 总计被硬钳在预算内 |
//!
//! 即**重试只在还付得起的时候发生**, 且最坏耗时 = 预算(当前 5s, 比实装前的
//! 3s 差 +2s)。这个 +2s 是拿「不丢事件」换的, 属显式取舍, 不是疏漏。
//!
//! ### 为什么编排逻辑是**泛型函数**而不是写在 impl 里
//!
//! 重试/退避/预算/DLQ 这一段是**纯编排**, 不含任何 IO 知识。它若写在
//! `NatsEventPublisher::publish` 里, 就只能靠真 NATS 才能测 —— 而「第一次
//! 就失败」「预算耗尽要转 DLQ」这类路径恰恰是 CI 里最难稳定复现的。
//! 抽成 [`orchestrate_publish`] 后, 用两个闭包注入 IO, 就能用
//! `tokio::time::pause()` 确定性测试退避与预算, **不碰 NATS**。
//!
//! ### 仍未实装 (诚实声明)
//!
//! `aux-08 §D.3` 的 MVP 范围是「**NATS DLQ subject** +
//! `audit_logs(action=dlq_record, detail=JSONB)`」。当前**只做了前者**。
//! 后者缺失有一个**真实的、可命名的**后果: 若 NATS **整体**不可用, 连
//! 写 DLQ 也会失败, 那条事件仍然丢失。这个情况由
//! `im_events_dlq_write_failed_total` 计数并 `error!`, 即**丢失是可见的**
//! —— 但它确实还是丢了。补 PG sink 见台账 §1.31。

use async_trait::async_trait;
use bytes::Bytes;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use im_common::AppError;

// 预算必须用 **tokio 的** Instant, 不能用 `std::time::Instant`。
//
// 2026-10-05 实测踩到: 用 std Instant 时, `budget_caps_total_attempts_when_
// each_attempt_is_slow` 在 `start_paused` 下观察到 **4 次**尝试(应为 2 次)
// —— 因为 `tokio::time::sleep` 推进的是 tokio 的虚拟时钟, 而 `std::time::
// Instant::now()` 几乎不动, 于是 `started.elapsed()` 恒为 ~0, 预算**永远
// 花不完**。
//
// 生产下两者都是真实时钟, 所以线上不炸 —— 但「用 A 时钟度量、用 B 时钟操作」
// 本来就是一处等着出事的隐患: 一旦有人在 paused-time 测试里断言预算, 或者
// 将来引入时钟抽象, 就会得到「预算形同虚设」且不报错的错误行为。度量与操作
// 必须同源。
use tokio::time::Instant;

// ---------------------------------------------------------------------------
// 计数器 —— 三个语义不同的量, 混用一个会让「丢在哪」不可区分
// ---------------------------------------------------------------------------

/// 成功发布并拿到 JetStream ack 的事件数
static EVENTS_PUBLISHED: AtomicU64 = AtomicU64::new(0);
/// 尝试发布但失败(超时 / 服务端拒绝 / subject 非法)的事件数
static EVENTS_PUBLISH_FAILED: AtomicU64 = AtomicU64::new(0);
/// 被 [`StubEventPublisher`] 丢弃的事件数
///
/// 2026-10-03 新增。`publish()` 此前只发一条 `tracing::debug!` 并返回
/// `Ok(())` —— 而 `debug` 在默认日志级别下**不可见**, 于是「每条消息事件
/// 都被丢掉」这件事在整个运行期**没有任何外部表征**。
///
/// 2026-10-04 语义收窄: 这个计数器现在**只**统计显式选择的 stub。真实
/// publisher 的失败走 `EVENTS_PUBLISH_FAILED` —— 两者混同会让「配置了 stub」
/// 和「NATS 挂了」在指标上长得一样, 而它们的处置完全不同。
static EVENTS_DROPPED: AtomicU64 = AtomicU64::new(0);

/// 已写入 DLQ 的事件数 (aux-08 §D.1 条件 1: 重试耗尽仍失败)
///
/// 与 `EVENTS_PUBLISH_FAILED` 是**两个不同的量**: 失败是「这一次没发出去」,
/// DLQ 是「已经落到可恢复的地方」。前者每次重试都 +1, 后者只 +1 一次。
static EVENTS_DLQ_TOTAL: AtomicU64 = AtomicU64::new(0);

/// **DLQ 写入本身也失败**的次数 —— 即事件**真的永久丢失**了
///
/// 这是本文件里最要紧的一个计数器, 因为它是唯一能说出「有多少事件彻底没了」
/// 的地方。没有它, NATS 整体不可用时「事件在丢」这件事**完全不可见**:
/// 失败计数在涨, 但没有任何东西说明这些失败连兜底都没兜住。
static EVENTS_DLQ_WRITE_FAILED: AtomicU64 = AtomicU64::new(0);

/// **PG 长留存层**写入失败的次数 (2026-10-06, aux-08 §D.3 第 2 层)
///
/// ## 为什么它与 `EVENTS_DLQ_WRITE_FAILED` 是两个量
///
/// 那个计数的语义是「**所有**层都没接住 -> 事件永久丢失」。而这一层可能失败
/// 时 NATS 层**已经接住了** —— 事件没丢, 只是丢了长留存的那份副本。
///
/// 两者的处置完全不同:
/// - `dlq_write_failed` 涨 = **有数据没了** -> P1
/// - 本计数涨 = **副本没留下** -> P2(7 天后那条死信就查不到了)
///
/// 合成一个数, 就无法区分「正在丢数据」与「备份没做上」—— 而这恰恰是
/// `aux-08 §D.5` 告警阈值要分开的两种情况。
static EVENTS_DLQ_PG_WRITE_FAILED: AtomicU64 = AtomicU64::new(0);

pub fn published_event_count() -> u64 {
    EVENTS_PUBLISHED.load(Ordering::Relaxed)
}

pub fn failed_event_count() -> u64 {
    EVENTS_PUBLISH_FAILED.load(Ordering::Relaxed)
}

pub fn dropped_event_count() -> u64 {
    EVENTS_DROPPED.load(Ordering::Relaxed)
}

/// 已进入 DLQ 的事件数
pub fn dlq_event_count() -> u64 {
    EVENTS_DLQ_TOTAL.load(Ordering::Relaxed)
}

/// DLQ 写入也失败、事件永久丢失的次数
pub fn dlq_write_failed_count() -> u64 {
    EVENTS_DLQ_WRITE_FAILED.load(Ordering::Relaxed)
}

/// PG 长留存层写入失败的次数(事件未必丢 —— 见该计数器的说明)
pub fn dlq_pg_write_failed_count() -> u64 {
    EVENTS_DLQ_PG_WRITE_FAILED.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 建立连接的上界。超过即视为 NATS 不可用。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 单次尝试等 JetStream ack 的上界。
///
/// `DetailedDesign §9.1` 要求发布失败不阻塞业务 ack, 而
/// `jetstream::Context::publish()` 返回的是要等服务端 ack 的 future ——
/// 不设上界的话, NATS 变慢会**顺着业务请求路径**传导成延迟尖峰。
///
/// 它由 [`RetryPolicy::ack_timeout`] 消费; 每次尝试实际取它与**剩余预算**的
/// 小者, 故「NATS 变慢」时不会白等满 3s。
const PUBLISH_ACK_TIMEOUT: Duration = Duration::from_secs(3);

/// 写 DLQ 自己的上界 —— 刻意**短**于 `PUBLISH_ACK_TIMEOUT`
///
/// 写 DLQ 发生在重试预算的末尾, 此时再等 3s 就等于把「不阻塞 ack」彻底
/// 破坏: 一次已经失败 5s 的请求会变成 8s。所以给 1s, 失败就**承认失败**
/// 并被 [`EVENTS_DLQ_WRITE_FAILED`] 计数 —— 宁可让「丢失」可见, 也不把
/// 请求挂住。
const DLQ_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// 写 **PG** DLQ 层自己的上界 —— 比 `DLQ_WRITE_TIMEOUT` **更短**
///
/// PG 写入比 NATS 慢的常见原因不是网络, 而是 `pool.acquire()` 在等一个空闲
/// 连接: 池被打满时它会等 `acquire_timeout`。给 1s 与给 NATS 层一样的上界,
/// 就等于在 NATS 已经慢的前提下再叠 1s。给 1s 的一半(500ms)是因为这一层是
/// **兜底**, 抢的是「NATS 挂掉时的那条路」—— 而那条路上, 用户请求已经等过
/// 完整的重试预算了。
const PG_DLQ_WRITE_TIMEOUT: Duration = Duration::from_millis(500);

/// 事件 stream 名
pub const EVENT_STREAM: &str = "IM_EVENTS";

/// 事件 subject 过滤器。覆盖 `BasicDesign §8` 的全部 topic
/// (`im.message.*` / `im.conversation.*` / `im.presence.*` / `im.identity.*`
/// / `im.auth.*`)。
pub const EVENT_SUBJECT_FILTER: &str = "im.>";

// ---------------------------------------------------------------------------
// DLQ (aux-08 §D.3)
// ---------------------------------------------------------------------------

/// DLQ stream 名。**与事件 stream 分开**是刻意的: 两者留存期与丢弃策略
/// 完全不同(事件长期留存, DLQ 7 天), 共用一个 stream 就只能取折中值, 而
/// 折中的那一边一定是错的。
pub const DLQ_STREAM: &str = "IM_DLQ";

/// DLQ subject 过滤器。对应 aux-08 §C.3 的 `dlq.event.<event_type>`。
pub const DLQ_SUBJECT_FILTER: &str = "dlq.>";

/// DLQ 留存期 —— aux-08 §D.3 写死「consumer 持久化 **7 天**」
pub const DLQ_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// DLQ 体积上界
///
/// aux-08 §D.3 **没给**这个数, 但没上界的队列就是 aux-08 §B 自己列的
/// 「资源错误: 磁盘满」—— 而一个专门在故障时被写入的队列, 恰恰是最容易在
/// 故障高峰把磁盘撑满的那个(故障期间 DLQ 写入速率最高)。
/// 超出后按 async-nats 的默认 `DiscardPolicy::Old` 丢最旧的,
/// 保留**最近的**死信 —— 对排障而言最近的比最旧的有用。
const DLQ_MAX_BYTES: i64 = 256 * 1024 * 1024;

/// 重试策略 (aux-08 §C.1 / §C.2 / §C.3)
///
/// ## 为什么 `total_budget` 是**本文件最重要的一个数字**
///
/// 它是「不丢事件」与「不阻塞 ack」这两个要求之间的**仲裁点**。见文件头
/// 「关键设计」。改这个值等于同时改这两条性质, 所以它被单独拎出来并在此
/// 解释, 而不是散成一个魔法数字。
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// 整个发布步骤(全部尝试 + 退避)的墙钟上界
    pub total_budget: Duration,
    /// 单次尝试等 JetStream ack 的上界
    pub ack_timeout: Duration,
    /// 各次重试前的退避。长度 = 最多重试次数(§C.3 的「3 次」)
    pub backoffs: [Duration; 3],
}

impl RetryPolicy {
    /// 事件发布用的策略
    ///
    /// `total_budget = 5s`: 比实装 DLQ 前的单次 3s 多 2s, 换来「NATS 整体
    /// 挂掉时事件仍然可恢复」。这个取舍写在文件头, 不是隐形的。
    pub const fn for_events() -> Self {
        Self {
            total_budget: Duration::from_secs(5),
            ack_timeout: PUBLISH_ACK_TIMEOUT,
            // aux-08 §C.3: 500ms / 1s / 2s
            backoffs: [
                Duration::from_millis(500),
                Duration::from_secs(1),
                Duration::from_secs(2),
            ],
        }
    }

    /// 测试用: 极短退避 + 极小预算
    ///
    /// 存在的意义是让**时间**不成为测试的变量。即便如此, 断言仍以
    /// 「尝试次数」与「DLQ 是否被写」为主, 不以实际耗时为主 —— 见模块内
    /// `budget_caps_total_attempts` 的注释。
    pub const fn for_tests() -> Self {
        Self {
            total_budget: Duration::from_secs(60),
            ack_timeout: Duration::from_secs(30),
            backoffs: [
                Duration::from_millis(1),
                Duration::from_millis(2),
                Duration::from_millis(4),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// DLQ Record (aux-08 §D.2, 逐字段照抄)
// ---------------------------------------------------------------------------

/// aux-08 §D.2 的 `error` 子结构
#[derive(Debug, Clone, serde::Serialize)]
pub struct DlqError {
    /// 错误码。与 aux-03 §B 的 21 项一致。
    pub code: String,
    pub message: String,
    /// 恒为 `null` —— 详见 [`DlqRecord`] 的说明
    pub stack: Option<String>,
    pub http_status: u16,
}

/// aux-08 §D.2 的 `context` 子结构
#[derive(Debug, Clone, serde::Serialize)]
pub struct DlqContext {
    /// 恒为 `null` —— 发布器在事务提交后调用, 拿不到请求上下文
    pub trace_id: Option<String>,
    pub user_id: Option<String>,
    pub env_id: Option<String>,
    /// 总共试了几次(含最终失败的那次)
    pub attempt_count: u32,
    pub first_attempt_at: chrono::DateTime<Utc>,
    pub last_attempt_at: chrono::DateTime<Utc>,
}

/// aux-08 §D.2 的 DLQ Record
///
/// ## 三个字段恒为 `null`/`None`, 这不是偷懒
///
/// - [`DlqError::stack`]: 规范写「stack trace, **脱敏后**」。本仓**没有**
///   做脱敏, 而把未脱敏的 stack 写进一个**会被持久化 7 天、且运维要读**的
///   队列是净风险。宁可为空, 也不写一份没脱敏的。
/// - [`DlqContext::trace_id`] / [`DlqContext::user_id`] / [`DlqContext::env_id`]:
///   `publish()` 是被 `MessageService` 在 `tx.commit()` **之后**调用的, 那里
///   已经没有请求上下文了。**编造**一个值会让排障时被错误信息带偏, 比空着更糟。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DlqRecord {
    pub dlq_id: Uuid,
    /// 事件 subject(aux-08 §D.2 的 `original_task` 在批处理语境下是作业名;
    /// 事件语境下对应的就是 topic)
    pub original_task: String,
    /// 原事件载荷
    ///
    /// 规范样例是对象。但载荷是**任意字节**, 不保证是合法 JSON —— 解析失败时
    /// 退化为一个 JSON **字符串**。
    ///
    /// ## 这个退化是**有损**的, 不是无损回退
    ///
    /// 非 UTF-8 字节在这里就被 `String::from_utf8_lossy` 替换成 U+FFFD
    /// (`EF BF BD`): `0xFF 0xFE` 落库时已经变成 `EF BF BD EF BF BD`, 信息在
    /// **写**的这一步就没了 —— 下游再怎么写都还原不回来。
    ///
    /// 所以本字段对 **UTF-8 载荷无损**, 对**非 UTF-8 载荷不保证可还原**。
    /// (早先此处写的是「原始字节不丢」, 那句话是错的: 没有任何地方保留原始字节。)
    ///
    /// 完整分析与真库实测见 `crates/jobctl/src/dlq.rs:135-149`
    /// (`replay_of_a_non_utf8_payload_is_lossy_at_write_time`), 已记入
    /// `docs/gap-ledger.md` §1.35。要真正无损得改存储形状
    /// (如 `{"__b64__": "..."}`), 而 aux-08 §D.2 冻结了该形状 ——
    /// 那是规范所有者的裁决, 不是这里能顺手改的。
    pub original_payload: serde_json::Value,
    pub error: DlqError,
    pub context: DlqContext,
    pub failed_at: chrono::DateTime<Utc>,
    /// aux-08 §D.2 的形态是 `NATS_subject_dlq.media.presign`
    pub dlq_destination: String,
}

impl DlqRecord {
    /// aux-08 §C.3: `dlq.event.<event_type>`
    pub fn destination_for(topic: &str) -> String {
        format!("dlq.event.{topic}")
    }

    pub fn new(
        topic: &str,
        payload: &[u8],
        error_message: &str,
        attempt_count: u32,
        first_attempt_at: chrono::DateTime<Utc>,
        last_attempt_at: chrono::DateTime<Utc>,
    ) -> Self {
        Self {
            dlq_id: Uuid::new_v4(),
            original_task: topic.to_string(),
            original_payload: serde_json::from_slice::<serde_json::Value>(payload).unwrap_or_else(
                |_| serde_json::Value::String(String::from_utf8_lossy(payload).into_owned()),
            ),
            error: DlqError {
                code: im_common::ErrorCode::ServiceUnavailable
                    .as_str()
                    .to_string(),
                message: error_message.to_string(),
                stack: None,
                http_status: 503,
            },
            context: DlqContext {
                trace_id: None,
                user_id: None,
                env_id: None,
                attempt_count,
                first_attempt_at,
                last_attempt_at,
            },
            failed_at: Utc::now(),
            dlq_destination: Self::destination_for(topic),
        }
    }
}

// ---------------------------------------------------------------------------
// 就绪判定 —— 供 `/readyz` 用(ImplementationSpec §3.1.7 + DetailedDesign §5:
// 「PG/Valkey/NATS 全部可达才 200」)
// ---------------------------------------------------------------------------

/// 事件发布器的就绪结论
///
/// ## 为什么不是 `Result`
///
/// `Result` 只有「成功 / 失败」两种形状, 而这里有**四种**需要如实上报的状态。
/// 若把「配置成 stub」也报成成功, `/readyz` 就会对集成方显示 `nats: ok`,
/// 而实际上**每条领域事件都被丢弃** —— 这正是本仓反复修掉的那类「绿灯在
/// 撒谎」(见 `health.rs` 的 `/metrics` 文案事故)。故用具名枚举, 让每一种
/// 状态都能原样出现在响应里, 由读响应的人自己判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublisherReadiness {
    /// 事件可正常投递
    Ready,
    /// 显式配置为 stub (`IM_EVENT_PUBLISHER_KIND=stub`): 事件**有意**被丢弃,
    /// 其余功能完全正常, 故**不**阻断 readiness
    ///
    /// 若把它判成 not-ready, 每一个本地/开发部署都会被永久判死 —— 那不是
    /// 诚实, 是把「配置选择」当成「故障」。
    StubByConfiguration,
    /// NATS 连接不存在或已关闭
    Disconnected,
    /// NATS 正在(重)连接
    ///
    /// 与 `Disconnected` 一样不能接流量, 但**处置不同**: 前者多半是服务端
    /// 重启 / 网络抖动, 客户端正在自愈, 很快会自己恢复。
    Connecting,
}

impl PublisherReadiness {
    /// 是否可以接流量
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready | Self::StubByConfiguration)
    }

    /// `/readyz` 响应里 `checks.nats` 的取值
    ///
    /// 与 `docs/api/openapi.json` 的 `ReadyResponse.checks.nats` enum
    /// **一一对应**。改这里必须同步改那里 —— 两侧串在 OpenAPI 契约门禁上。
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Ready => "ok",
            Self::StubByConfiguration => "ok_stub_events_not_delivered",
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
        }
    }
}

/// 把 NATS 连接状态映射成就绪结论
///
/// 抽成**纯函数**是为了能脱离 NATS 测试: `State::Disconnected` 与
/// `State::Pending` 在 CI 里稳定构造不出来 —— 前者需要一个真断开的连接,
/// 后者只在首次连接/重连的那个窗口里出现。
pub fn readiness_from_state(state: &async_nats::connection::State) -> PublisherReadiness {
    use async_nats::connection::State;
    match state {
        State::Connected => PublisherReadiness::Ready,
        State::Disconnected => PublisherReadiness::Disconnected,
        State::Pending => PublisherReadiness::Connecting,
    }
}

#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// 发布事件; `payload` 是**已序列化的字节**
    ///
    /// ## 2026-10-03 对齐规范 —— 签名从 `&MessageCreatedEvent` 改为 `&[u8]`
    ///
    /// 此前本方法写死接收 `&MessageCreatedEvent`。这不只是风格问题, 它让
    /// **除 created 之外的任何事件都发不出去**: aux-04 §B.4 不变量要求
    /// 「转换必须 publish 事件 `im.message.{recalled,deleted}` 供其他 pod
    /// 同步」, 但 trait 的签名让这条不变量**在类型层面就无法满足** ——
    /// 加 `MessageRecalledEvent` 时编译器直接拒绝。
    ///
    /// `DetailedDesign.md` §publisher.rs 本来就写的是
    /// `async fn publish(&self, topic: &str, payload: &[u8])`。即**规范是对的,
    /// 实现偏离了规范**, 故此处是实现回归规范, 不是引入新设计。
    ///
    /// 不用 `&serde_json::Value` 是因为规范明确写的是字节; 序列化在调用方做,
    /// 各事件自己选格式。
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError>;

    /// 当前能否投递事件 —— 供 `/readyz` 使用
    ///
    /// 2026-10-05 新增。`DetailedDesign §5` 与 `ImplementationSpec §3.1.7`
    /// 都要求就绪判定覆盖 NATS, 但此前这个端点**拿不到 publisher 实例**,
    /// 于是只能报一个恒定的 `not_checked`。
    ///
    /// **刻意不给默认实现**: 漏实现应当是**编译错误**, 而不是悄悄返回某个
    /// 看起来正常的值 —— 那等于把「没人检查过」包装成「检查过且通过」。
    fn readiness(&self) -> PublisherReadiness;
}

/// subject 合法性检查
///
/// NATS 规则: 不允许空白, 不允许空 token(前后点或连续点), 且**发布时不允许
/// 通配符**。提前判掉这些, 是为了让「调用方拼错了 topic」变成一条能指到
/// 调用点的错误, 而不是服务端一句含糊的 `invalid subject`。
///
/// 这属于**编程错误**而非运行时故障, 故返回 `Internal` 而非 `ServiceUnavailable`
/// —— 两者都在已注册的 21 项错误码内(500 / 503), 不需要新增错误码。
fn validate_subject(topic: &str) -> Result<(), AppError> {
    if topic.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject is empty"
        )));
    }
    if topic.chars().any(char::is_whitespace) {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject contains whitespace: {topic:?}"
        )));
    }
    if topic.contains('*') || topic.contains('>') {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject must not contain wildcards when publishing: {topic:?}"
        )));
    }
    if topic.split('.').any(str::is_empty) {
        return Err(AppError::Internal(anyhow::anyhow!(
            "event subject has an empty token: {topic:?}"
        )));
    }
    Ok(())
}

/// 幂等确保事件 stream 存在
///
/// `create_or_update_stream` 内部先 `update_stream`, 只在 `NotFound` 时才
/// `create_stream`, 所以重复调用安全。**这一步是必需的**: 部署清单
/// (`deploy/k3s/dev/nats.yaml`) 没有预置 stream, 假定它存在等于假定事件
/// 永远拿 `no responders` —— 又一次静默失效。
async fn ensure_event_stream(js: &async_nats::jetstream::Context) -> Result<(), AppError> {
    let cfg = async_nats::jetstream::stream::Config {
        name: EVENT_STREAM.to_string(),
        subjects: vec![EVENT_SUBJECT_FILTER.to_string()],
        ..Default::default()
    };
    js.create_or_update_stream(cfg).await.map_err(|e| {
        AppError::ServiceUnavailable(format!(
            "ensure NATS JetStream stream {EVENT_STREAM} (subjects {EVENT_SUBJECT_FILTER}): {e}"
        ))
    })?;
    Ok(())
}

/// 幂等确保 DLQ stream 存在 (aux-08 §D.3)
///
/// 与事件 stream **分开建**, 理由见 [`DLQ_STREAM`]。
async fn ensure_dlq_stream(js: &async_nats::jetstream::Context) -> Result<(), AppError> {
    let cfg = async_nats::jetstream::stream::Config {
        name: DLQ_STREAM.to_string(),
        subjects: vec![DLQ_SUBJECT_FILTER.to_string()],
        max_age: DLQ_MAX_AGE,
        max_bytes: DLQ_MAX_BYTES,
        ..Default::default()
    };
    js.create_or_update_stream(cfg).await.map_err(|e| {
        AppError::ServiceUnavailable(format!(
            "ensure NATS JetStream DLQ stream {DLQ_STREAM} (subjects {DLQ_SUBJECT_FILTER}): {e}"
        ))
    })?;
    Ok(())
}

/// 发布编排: 重试 -> 退避 -> 预算 -> DLQ。**不含任何 IO 知识**
///
/// IO 由两个闭包注入, 所以整段编排逻辑可以脱离 NATS 被确定性测试。
///
/// ## 契约
///
/// - `attempt(deadline)`: 做**一次**发布尝试, 须在 `deadline` 内自行超时。
///   参数是「本次尝试最多可用的墙钟时间」, 调用方据此给 `ack_timeout` 与剩余
///   预算取小 —— 这正是「变慢时不浪费 3s 等一次注定超时的 ack」的机制。
/// - `dead_letter(record)`: 把死信写出去。
///
/// 返回 `Ok(())` 当且仅当某次尝试成功。否则一定已经尝试过写 DLQ —— **包括
/// DLQ 写失败的情况**(那会额外计数并 `error!`)。本函数**不会**在未写 DLQ 的
/// 情况下返回失败, 因为那正是实装前「静默丢失」的那个缺口。
pub(crate) async fn orchestrate_publish<A, AF, D, DF>(
    topic: &str,
    payload: &[u8],
    policy: &RetryPolicy,
    mut attempt: A,
    mut dead_letter: D,
) -> Result<(), AppError>
where
    A: FnMut(Duration) -> AF,
    AF: Future<Output = Result<(), String>>,
    D: FnMut(DlqRecord) -> DF,
    DF: Future<Output = Result<(), String>>,
{
    let started = Instant::now();
    let first_attempt_at = Utc::now();
    let mut attempt_no: u32 = 0;
    let mut last_err = String::from("publish did not run (budget exhausted before first attempt)");

    loop {
        let remaining = policy.total_budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            tracing::warn!(
                topic,
                budget = ?policy.total_budget,
                attempts = attempt_no,
                "event publish budget exhausted; going to DLQ"
            );
            break;
        }
        attempt_no += 1;
        match attempt(remaining.min(policy.ack_timeout)).await {
            Ok(()) => {
                let n = EVENTS_PUBLISHED.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::debug!(
                    topic,
                    bytes = payload.len(),
                    attempt = attempt_no,
                    published_total = n,
                    "event published"
                );
                return Ok(());
            }
            Err(e) => {
                let n = EVENTS_PUBLISH_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    topic,
                    attempt = attempt_no,
                    failed_total = n,
                    budget_left_ms = policy
                        .total_budget
                        .saturating_sub(started.elapsed())
                        .as_millis() as u64,
                    error = %e,
                    "event publish attempt failed"
                );
                last_err = e;
            }
        }

        // 还有退避档、且付得起 -> 退避后重试; 否则直接进 DLQ
        let Some(&backoff) = policy.backoffs.get((attempt_no - 1) as usize) else {
            break;
        };
        let remaining = policy.total_budget.saturating_sub(started.elapsed());
        if remaining <= backoff {
            tracing::debug!(
                topic,
                backoff_ms = backoff.as_millis() as u64,
                budget_left_ms = remaining.as_millis() as u64,
                "not enough budget left for another backoff; going to DLQ"
            );
            break;
        }
        tokio::time::sleep(backoff).await;
    }

    // ---- 重试耗尽 -> aux-08 §D.1 条件 1: 写 DLQ ----
    let record = DlqRecord::new(
        topic,
        payload,
        &last_err,
        attempt_no,
        first_attempt_at,
        Utc::now(),
    );
    match dead_letter(record).await {
        Ok(()) => {
            let n = EVENTS_DLQ_TOTAL.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::error!(
                topic,
                attempts = attempt_no,
                dlq_total = n,
                dlq_destination = DlqRecord::destination_for(topic),
                "event publish failed after retries; event written to DLQ (recoverable)"
            );
            Err(AppError::ServiceUnavailable(format!(
                "publish {topic}: {last_err} (written to DLQ)"
            )))
        }
        Err(e) => {
            let n = EVENTS_DLQ_WRITE_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::error!(
                topic,
                attempts = attempt_no,
                dlq_write_failed_total = n,
                error = %e,
                "event publish failed AND the DLQ write failed; THIS EVENT IS PERMANENTLY LOST"
            );
            Err(AppError::ServiceUnavailable(format!(
                "publish {topic}: {last_err} (DLQ write also failed: {e})"
            )))
        }
    }
}

/// 合成两层 DLQ 的结果: **任一层接住就算可恢复** (aux-08 §D.3)
///
/// ## 为什么这条规则值得单独抽成一个纯函数
///
/// 它是本次增量的**全部意义**所在: PG 层的价值恰恰在于接住「NATS 整体不可用」。
/// 若判定仍写成「NATS 写成功才算可恢复」, 那么在 PG 层唯一重要的那个场景里,
/// PG 会被**自己拖累成「失败」** —— 一个只在 NATS 挂掉时才触发的 bug, 平时
/// 永远看不见。
///
/// 而这几种组合在集成环境里**没法稳定复现**: 要让 NATS 层在 PG 层正常的场景下
/// 失败, 只能去杀 NATS。故抽成纯函数, 逐格断言。
///
/// ## 真值表(逐格对应 `dlq_is_recoverable_when_either_layer_accepts`)
///
/// | nats | pg | 判定 |
/// |---|---|---|
/// | Ok | Ok / None | 可恢复 |
/// | Err | Ok | 可恢复 —— **PG 层存在的全部意义** |
/// | Ok | Err | 可恢复 —— 丢的是长留存副本, 事件仍在 NATS |
/// | Err | None | 永久丢失 —— 没配 PG 层时**没有**东西接住它 |
/// | Err | Err | 永久丢失 |
///
/// ## 为什么 `None` 不能算成「PG 成功」
///
/// `None` 是「该进程**没配**这一层」, 不是「这一层成功」。把它当成功的话,
/// 「没配 PG 且 NATS 挂了」会被判成可恢复 —— 而实际上没有任何东西接住那条
/// 事件, 它就是丢了。`absent_pg_layer_does_not_rescue_a_failed_publish` 专门
/// 锁这一格。
pub fn combine_dlq_results(
    nats: Result<(), String>,
    pg: Option<Result<(), String>>,
) -> Result<(), String> {
    match (nats, pg) {
        (Ok(()), _) => Ok(()),
        (Err(_), Some(Ok(()))) => Ok(()),
        (Err(nats_e), None) => Err(nats_e),
        (Err(nats_e), Some(Err(pg_e))) => Err(format!("nats: {nats_e}; postgres: {pg_e}")),
    }
}

/// DLQ 的一个落地后端
///
/// ## 为什么要抽象成 trait
///
/// `aux-08 §D.3` 要求**两层**留存: NATS JetStream(7 天 / 256MB)与
/// PostgreSQL(长留存)。而 PG 那一层的**全部价值**在于接住「NATS 整体不可用」
/// —— 那正是 NATS 层失效的场景。故两层必须**并列**而非二选一, 且「可恢复」的
/// 判定必须改成「**任一**层接住」。
///
/// 抽象成 trait 的收益是**可测**: `orchestrate_publish` 的 DLQ 分支此前用
/// 闭包注入, 现在两个后端的行为可以在无 PG、无 NATS 的环境下分别验。
#[async_trait]
pub trait DlqSink: Send + Sync {
    /// 后端标识, 出现在启动日志与错误串里
    fn name(&self) -> &'static str;

    /// 把一条死信落地
    ///
    /// 返回 `Err(String)` 表示**这一层没接住**。调用方据此决定:
    /// 全部后端都失败才算「事件永久丢失」。
    async fn store(&self, record: &DlqRecord) -> Result<(), String>;
}

/// aux-08 §D.3 的 PostgreSQL 长留存层 (`migrations/0008_create_dlq_records.sql`)
///
/// ## 为什么**不**写 `audit_logs`
///
/// `aux-08 §D.3` 的 MVP 原文是 `audit_logs(action=dlq_record, detail=JSONB)`,
/// 但那张表(`migrations/0006`)有 `tenant_id UUID NOT NULL` 与
/// `target_type` 的 6 值 CHECK(不含「事件」), 而事件发布路径**拿不到租户**。
/// 详见 0008 migration 顶部。2026-10-06 架构拍板走专表。
pub struct PgDlqSink {
    pool: sqlx::PgPool,
}

impl PgDlqSink {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DlqSink for PgDlqSink {
    fn name(&self) -> &'static str {
        "postgres:dlq_records"
    }

    async fn store(&self, record: &DlqRecord) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO dlq_records (\
               dlq_id, original_task, original_payload,\
               error_code, error_message, error_stack, error_http_response_code,\
               context_trace_id, context_user_id, context_env_id,\
               context_attempt_count, context_first_attempt_at,\
               context_last_attempt_at, failed_at, dlq_destination\
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(record.dlq_id)
        .bind(&record.original_task)
        // `original_payload` 是 JSONB。`DlqRecord` 构造时已保证它是合法的
        // `serde_json::Value`(解析失败会退化成 JSON 字符串), 故可直接绑定。
        .bind(&record.original_payload)
        .bind(&record.error.code)
        .bind(&record.error.message)
        .bind(&record.error.stack)
        .bind(record.error.http_status as i16)
        .bind(&record.context.trace_id)
        .bind(&record.context.user_id)
        .bind(&record.context.env_id)
        .bind(record.context.attempt_count as i32)
        .bind(record.context.first_attempt_at)
        .bind(record.context.last_attempt_at)
        .bind(record.failed_at)
        .bind(&record.dlq_destination)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|e| format!("insert into dlq_records: {e}"))
    }
}

/// 真实的 NATS JetStream 发布器 (D-3)
pub struct NatsEventPublisher {
    client: async_nats::Client,
    js: async_nats::jetstream::Context,
    /// aux-08 §D.3 的 PG 长留存层
    ///
    /// `None` = 只靠 NATS 层(测试 / 显式不配)。生产下 `main.rs` **总是**
    /// 传 `Some` —— 它有 `IM_POSTGRES_URL`, 而那是必填项。取 `Option` 而非
    /// 必填, 是为了让「本进程没接住 PG 死信」这件事能被 `Some/None` 直接
    /// 表达, 并在启动日志里报出来, 而不是悄悄退化成单层。
    pg_dlq: Option<Arc<dyn DlqSink>>,
}

impl NatsEventPublisher {
    /// 连接 NATS 并确保事件 stream 存在
    ///
    /// 失败即 `Err`: 调用方(`main.rs`)据此**拒绝启动**。配了 `kind=nats`
    /// 却静默退化成 stub, 是本文件长期存在的那类缺陷, 不再重复。
    ///
    /// `pg_dlq` 是 `aux-08 §D.3` 的 PG 长留存层。传 `None` 表示只靠 NATS 层
    /// —— 生产下**不该**发生, 而它一旦发生就会打在启动日志里。
    pub async fn connect(url: &str, pg_dlq: Option<Arc<dyn DlqSink>>) -> Result<Self, AppError> {
        let client = match tokio::time::timeout(CONNECT_TIMEOUT, async_nats::connect(url)).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                return Err(AppError::ServiceUnavailable(format!(
                    "connect to NATS at {url}: {e}"
                )))
            }
            Err(_) => {
                return Err(AppError::ServiceUnavailable(format!(
                    "connect to NATS at {url}: timed out after {CONNECT_TIMEOUT:?}"
                )))
            }
        };

        let js = async_nats::jetstream::new(client.clone());
        ensure_event_stream(&js).await?;
        // DLQ stream 同为程序幂等创建 —— 与事件 stream 同理, 假定运维预置
        // 等于假定 DLQ 永远写不进去, 于是失败的事件又会变回「静默丢失」。
        ensure_dlq_stream(&js).await?;

        tracing::info!(
            nats_url = url,
            stream = EVENT_STREAM,
            subjects = EVENT_SUBJECT_FILTER,
            dlq_stream = DLQ_STREAM,
            dlq_max_age = ?DLQ_MAX_AGE,
            pg_dlq_layer = pg_dlq.as_ref().map(|s| s.name()).unwrap_or("DISABLED"),
            "EventPublisher = NATS JetStream (D-3 实装, DLQ 已实装)"
        );
        Ok(Self { client, js, pg_dlq })
    }

    /// 底层 client —— 供集成测试订阅断言用
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }

    /// JetStream context —— 供集成测试直接查 stream 状态用
    ///
    /// 查 stream 的 `state.messages` 比「订阅者收到了」是**更强**的断言:
    /// 它证明事件被服务端**持久化**了, 而不只是被投递过一次。
    pub fn jetstream(&self) -> &async_nats::jetstream::Context {
        &self.js
    }
}

#[async_trait]
impl EventPublisher for NatsEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        // 编程错误(非法 subject)不属于瞬时故障, 不进重试也不进 DLQ ——
        // 重试一个拼错的 topic 只会把 5s 预算烧光, 而结果必然还是错。
        validate_subject(topic)?;

        let js_attempt = self.js.clone();
        let subject = topic.to_string();
        let body = Bytes::copy_from_slice(payload);
        let policy = RetryPolicy::for_events();

        // 一次尝试: 自己负责在给定的墙钟上界内超时(outer = 发送本身,
        // inner = 等服务端 ack)。失败一律转成 String 交回编排层。
        //
        // 注意 `js_attempt` / `js_dlq` 是**各自独立**的 clone: 两个 `move`
        // 闭包都会把捕获的变量按值拿走, 共用一个 `js` 会在第二个闭包处借用
        // 检查失败。
        let mut do_attempt = move |deadline: Duration| {
            let js = js_attempt.clone();
            let subject = subject.clone();
            let body = body.clone();
            async move {
                let sent =
                    match tokio::time::timeout(deadline, js.publish(subject.clone(), body)).await {
                        Err(_) => {
                            return Err(format!("no JetStream ack within {deadline:?}"));
                        }
                        Ok(Err(e)) => return Err(e.to_string()),
                        Ok(Ok(ack)) => ack,
                    };
                sent.await.map(|_ack| ()).map_err(|e| e.to_string())
            }
        };

        // 写 DLQ: **两层并列** —— NATS JetStream(7 天) + PG `dlq_records`(长留存)。
        //
        // 判定规则: **任一层接住就算可恢复**。这正是 PG 层的存在意义 ——
        // 它专门接住「NATS 整体不可用」这个 NATS 层自己失效的场景。
        // 若仍按「NATS 写成功才算可恢复」判定, PG 层在它唯一重要的场景里
        // 反而会被自己拖累成「失败」。
        let js_dlq = self.js.clone();
        let pg_dlq = self.pg_dlq.clone();
        let mut do_dlq = move |rec: DlqRecord| {
            let js = js_dlq.clone();
            let pg = pg_dlq.clone();
            async move {
                let subject = rec.dlq_destination.clone();
                let body = Bytes::from(
                    serde_json::to_vec(&rec).map_err(|e| format!("serialize DlqRecord: {e}"))?,
                );
                // DLQ 写入**不能再等一个完整 ack_timeout**: 此刻已经在预算
                // 末尾, 再等 3s 就等于把「不阻塞 ack」彻底破坏。给一个短上界,
                // 失败就承认失败(并被计数), 而不是把请求挂住。
                let nats_res = match tokio::time::timeout(
                    DLQ_WRITE_TIMEOUT,
                    js.publish(subject, body),
                )
                .await
                {
                    Err(_) => Err(format!("DLQ write timed out after {DLQ_WRITE_TIMEOUT:?}")),
                    Ok(Err(e)) => Err(e.to_string()),
                    Ok(Ok(ack)) => ack.await.map(|_a| ()).map_err(|e| e.to_string()),
                };

                let pg_res = match &pg {
                    None => Ok(()), // 该进程没配 PG 层, 不是失败
                    Some(sink) => {
                        // PG 写入给一个**更短**的上界: 它此刻在预算末尾, 而
                        // `acquire()` 可能要等池里空闲连接。宁可承认失败, 也不
                        // 把用户的请求挂住 —— PG 失败会被单独计数, 不是静默。
                        match tokio::time::timeout(PG_DLQ_WRITE_TIMEOUT, sink.store(&rec)).await {
                            Err(_) => {
                                let n =
                                    EVENTS_DLQ_PG_WRITE_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
                                tracing::error!(
                                    dlq_id = %rec.dlq_id,
                                    sink = sink.name(),
                                    pg_dlq_write_failed_total = n,
                                    timeout_ms = PG_DLQ_WRITE_TIMEOUT.as_millis(),
                                    "PG DLQ write timed out"
                                );
                                Err(format!(
                                    "PG DLQ write timed out after {PG_DLQ_WRITE_TIMEOUT:?}"
                                ))
                            }
                            Ok(Err(e)) => {
                                let n =
                                    EVENTS_DLQ_PG_WRITE_FAILED.fetch_add(1, Ordering::Relaxed) + 1;
                                tracing::error!(
                                    dlq_id = %rec.dlq_id,
                                    sink = sink.name(),
                                    pg_dlq_write_failed_total = n,
                                    error = %e,
                                    "PG DLQ write failed (NATS 层可能仍接住了本条)"
                                );
                                Err(e)
                            }
                            Ok(Ok(())) => Ok(()),
                        }
                    }
                };

                // 合成: 至少一层成功 = 可恢复
                // `pg` 为 `None` 的分支在上面已折成 `Ok(())` —— 与
                // `combine_dlq_results(nats, None)` 等价(两者都直接返回 nats)。
                combine_dlq_results(nats_res, Some(pg_res))
            }
        };

        orchestrate_publish(topic, payload, &policy, &mut do_attempt, &mut do_dlq).await
    }

    /// 读 `async_nats` 客户端内部的连接状态
    ///
    /// 这是**本地读**(`watch` channel), 不发网络请求, 所以 readiness 探针
    /// 不会因为 NATS 慢而超时 —— 这正是它适合放进探针的原因。相比「发一条
    /// 请求等回包」, 它不会给 NATS 增加探针流量, 也不会在 NATS 假死(TCP
    /// 连着但不回包)时把探针一起拖住。
    ///
    /// 它同样不会**误报健康**: 连接一断, `connection_state()` 立刻变
    /// `Disconnected`, 与服务端是否还在接受请求无关。
    fn readiness(&self) -> PublisherReadiness {
        readiness_from_state(&self.client.connection_state())
    }
}

/// 显式选择的空实现 —— **只在 `event_publisher.kind=stub` 时使用**
///
/// 拆出这个类型是必要的: 此前 `kind=stub` 与 `kind=nats` 走的是同一个
/// `NatsEventPublisher::connect`, 于是「我要 stub」的实现是「去连
/// `nats://stub:4222`」。那种形状下, 打开真实 NATS 后 `kind=stub` 也会
/// 意外开始发事件, 而测试里的 `nats://stub:4222` 也会在连接失败后
/// 表现成「stub 坏了」。
///
/// 它**故意**返回 `Ok(())`: stub 的语义是「接受事件但不投递」, 调用方
/// 无需为部署形态改变错误处理。但它**必须**计数 + `warn!` —— 静默丢弃
/// 正是这个文件从 2026-08 起最大的问题。
#[derive(Debug, Default, Clone, Copy)]
pub struct StubEventPublisher;

impl StubEventPublisher {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EventPublisher for StubEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        let n = EVENTS_DROPPED.fetch_add(1, Ordering::Relaxed) + 1;
        // warn 而非 debug: 默认日志级别下 debug 不可见, 那样这个缺口依然
        // 是静默的。warn 让「事件在丢」变成运维能看见的事。
        tracing::warn!(
            topic,
            bytes = payload.len(),
            dropped_total = n,
            "event DROPPED: event_publisher.kind=stub (no NATS). Cross-pod sync is OFF by configuration."
        );
        Ok(())
    }

    fn readiness(&self) -> PublisherReadiness {
        PublisherReadiness::StubByConfiguration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- subject 校验: 这些不需要 NATS 就能测, 且是最容易写错的边界 ----

    #[test]
    fn subject_rejects_empty() {
        assert!(validate_subject("").is_err());
    }

    #[test]
    fn subject_rejects_whitespace() {
        assert!(validate_subject("im.message.created ").is_err());
        assert!(validate_subject("im message created").is_err());
        assert!(validate_subject("im.message\tcreated").is_err());
    }

    #[test]
    fn subject_rejects_wildcards() {
        // `>` 与 `*` 在发布侧非法(它们只对订阅合法)
        assert!(validate_subject("im.>").is_err());
        assert!(validate_subject("im.message.*").is_err());
    }

    #[test]
    fn subject_rejects_empty_token() {
        assert!(validate_subject("im..created").is_err());
        assert!(validate_subject(".im.created").is_err());
        assert!(validate_subject("im.created.").is_err());
    }

    #[test]
    fn subject_accepts_the_real_topics() {
        // BasicDesign §8 的全部 topic 形状
        for t in [
            "im.message.created",
            "im.message.edited",
            "im.message.recalled",
            "im.message.reaction_added",
            "im.conversation.created",
            "im.conversation.member_joined",
            "im.conversation.member_left",
            "im.presence.changed",
            "im.identity.state_changed",
            "im.auth.token_rotated",
        ] {
            assert!(validate_subject(t).is_ok(), "{t} should be accepted");
        }
    }

    // ---- 错误码归属: 不得引入 21 项注册表之外的新码 ----

    #[test]
    fn illegal_subject_maps_to_internal_not_service_unavailable() {
        let e = validate_subject("im.>").unwrap_err();
        assert_eq!(e.code(), im_common::ErrorCode::InternalError);
    }

    // ---- 计数相关用例: 必须串行 ----
    //
    // 三个计数是**进程级全局** `static AtomicU64`(生产上 `/metrics` 就要这个
    // 进程级视图), 而 libtest 在**同一进程内并行跑测试**。若两个用例都断言
    // 「delta == N」而不加锁, 彼此的 publish 会插进对方的 before/assert 之间,
    // 把 `before + 2` 变成 `before + 3`。
    //
    // 实测踩过: run 37261532690 在 GitHub runner 上
    // `stub_counts_every_dropped_event` 失败于 publisher.rs:388, 而本机
    // 424 passed 全绿 —— 纯时序差异。故凡断言全局计数 delta 的用例都必须
    // 先拿这把锁。
    //
    // 用 `tokio::sync::Mutex` 而非 `std::sync::Mutex`: 锁要**跨 `.await`**
    // 持有(`publish` 是 async), 而 `clippy::await_holding_lock` 会把
    // 「std Mutex 的 guard 跨 await」判为 correctness 问题 —— 在多线程
    // runtime 上那确实可能死锁, 且 `-D warnings` 下直接编译失败。
    static COUNTER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    // ---- stub: 计数必须真的累加(防"改回静默 no-op") ----

    #[tokio::test]
    async fn stub_counts_every_dropped_event() {
        let _guard = COUNTER_LOCK.lock().await;
        let before = dropped_event_count();
        let p = StubEventPublisher::new();
        p.publish("im.message.created", b"{\"a\":1}").await.unwrap();
        p.publish("im.message.recalled", b"{}").await.unwrap();
        assert_eq!(
            dropped_event_count(),
            before + 2,
            "stub 必须逐条计数 —— 这正是它存在的理由"
        );
    }

    // ---- 就绪判定: 三个 NATS 状态 + stub 的诚实性 ----

    use async_nats::connection::State;

    #[test]
    fn readiness_maps_every_nats_connection_state() {
        // 三个变体逐个断言。若这里漏了任何一个, `/readyz` 就会对「正在重连」
        // 或「已断开」报健康 —— 而这两种状态下事件正在进 DLQ。
        assert_eq!(
            readiness_from_state(&State::Connected),
            PublisherReadiness::Ready
        );
        assert_eq!(
            readiness_from_state(&State::Disconnected),
            PublisherReadiness::Disconnected
        );
        assert_eq!(
            readiness_from_state(&State::Pending),
            PublisherReadiness::Connecting
        );
    }

    /// 对照组 + 反例守卫: 映射**不能**退化成常量。
    ///
    /// 一个恒返回 `Ready` 的映射会让 `/readyz` 永远 200 —— 与本文件交付前
    /// 「NATS 拿不到就报 `not_checked`」相比更糟: 那至少还诚实地承认没查。
    /// 这里断言三个输入产出**三个不同的值**, 且只有 `Ready` 是 ready 的。
    #[test]
    fn readiness_mapping_is_not_constant_and_gates_on_the_right_side() {
        let states = [State::Connected, State::Disconnected, State::Pending];
        let mapped: Vec<PublisherReadiness> = states.iter().map(readiness_from_state).collect();

        assert_ne!(
            mapped[0], mapped[1],
            "Connected 与 Disconnected 必须能区分, 否则探针在 NATS 挂掉时仍报健康"
        );
        assert_ne!(
            mapped[1], mapped[2],
            "Disconnected 与 Pending 必须能区分 —— 处置不同(前者要查, 后者在自愈)"
        );
        assert_eq!(
            mapped.iter().filter(|r| r.is_ready()).count(),
            1,
            "只有 Connected 算 ready; 断线/重连都必须阻断流量"
        );
    }

    /// stub **不是** ready 意义上的「正常投递」, 但也**不该**阻断流量。
    #[test]
    fn stub_is_ready_but_must_not_report_itself_as_healthy_nats() {
        let r = StubEventPublisher::new().readiness();
        assert!(
            r.is_ready(),
            "stub 是显式配置, 判成 not-ready 会让每个开发部署永久 503"
        );
        assert_eq!(
            r.as_wire_str(),
            "ok_stub_events_not_delivered",
            "stub 绝不能报 `ok` —— 那等于告诉集成方 NATS 正常而事件其实全被丢弃"
        );
        assert_ne!(r, PublisherReadiness::Ready);
    }

    /// 四个取值都要满足「同一个值映射出同一个字符串」——
    /// `as_wire_str` 会被探针反复调用, 不一致会让相邻两次探测给出不同结论。
    #[test]
    fn wire_strings_are_stable_and_distinct_where_they_must_be() {
        let all = [
            PublisherReadiness::Ready,
            PublisherReadiness::StubByConfiguration,
            PublisherReadiness::Disconnected,
            PublisherReadiness::Connecting,
        ];
        for r in all {
            assert_eq!(r.as_wire_str(), r.as_wire_str(), "{r:?} 的取值必须稳定");
            assert!(
                !r.as_wire_str().is_empty(),
                "{r:?} 的取值不能是空串 —— 空串在 JSON 里读起来像「没报」。全部取值: {all:?}"
            );
        }
        let mut names: Vec<&str> = all.iter().map(|r| r.as_wire_str()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "四个取值必须互不相同: {names:?}");
    }

    // ---- 两层 DLQ 的合成规则 (aux-08 §D.3) ----

    /// 四种组合逐个断言, 因为**每一种的后果都不同**
    ///
    /// 第 2 行是本次增量的**全部意义**: NATS 挂掉而 PG 接住了 —— 事件**没丢**。
    /// 若这一行判成失败, 那么 PG 层会在它唯一重要的场景里被自己拖累, 而这种
    /// 缺陷**平时永远看不出来**(要 NATS 挂掉才触发)。
    #[test]
    fn dlq_is_recoverable_when_either_layer_accepts() {
        let ok: Result<(), String> = Ok(());
        let nats_bad: Result<(), String> = Err("nats down".into());
        let pg_bad: Result<(), String> = Err("pg down".into());

        // 1. 两层都成功
        assert!(combine_dlq_results(ok.clone(), Some(ok.clone())).is_ok());
        // 2. NATS 失败、PG 成功 -> **可恢复**(PG 层存在的意义)
        assert!(
            combine_dlq_results(nats_bad.clone(), Some(ok.clone())).is_ok(),
            "NATS 挂掉但 PG 接住了 -> 事件没丢, 必须判可恢复"
        );
        // 3. NATS 成功、PG 失败 -> 可恢复(长留存副本没了, 但事件在 NATS 里)
        assert!(
            combine_dlq_results(ok.clone(), Some(pg_bad.clone())).is_ok(),
            "NATS 接住了就还没丢 —— 丢了的是长留存副本, 那是另一件事"
        );
        // 4. 两层都失败 -> 永久丢失
        assert!(
            combine_dlq_results(nats_bad.clone(), Some(pg_bad.clone())).is_err(),
            "两层都没接住才是永久丢失"
        );
    }

    /// 未配 PG 层时, 判定退化成「只看 NATS」, 且 `None` 本身**不是失败**
    ///
    /// 两个方向都要锁:
    /// - NATS 成功 + `None` -> 可恢复(没配不是「失败」)
    /// - NATS 失败 + `None` -> **仍然丢失**。`None` 是一层**不存在**的层,
    ///   不是一层**成功**的层; 若把它算成成功, 「没配 PG 且 NATS 挂了」会被
    ///   误判成可恢复, 而那条事件其实没人接。
    ///
    /// 第一版实现正是踩了第二格: `None | Some(Ok(()))` 被合到一支, 于是
    /// `Some(Ok(()))` 跟着 `None` 一起被判成「PG 无事发生」。本用例当时是红的。
    #[test]
    fn absent_pg_layer_does_not_rescue_a_failed_publish() {
        let ok: Result<(), String> = Ok(());
        let nats_bad: Result<(), String> = Err("nats down".into());

        assert!(combine_dlq_results(ok.clone(), None).is_ok());
        assert!(
            combine_dlq_results(nats_bad, None).is_err(),
            "没配 PG 层时, NATS 失败就是真的没接住 —— 没有第二层兜底"
        );
    }

    /// 反例守卫: 两层都失败时, 错误串必须**同时**含两边的原因
    ///
    /// 只报一边, 排障的人就会去查那个健康的组件 —— 而真正的故障在另一边。
    #[test]
    fn both_layers_failing_reports_both_reasons() {
        let err = combine_dlq_results(
            Err("nats: timed out after 1s".into()),
            Some(Err("pg: pool exhausted".into())),
        )
        .expect_err("两层都失败必须是 Err");
        let e = err.to_string();
        assert!(
            e.contains("nats") && e.contains("pg"),
            "错误串必须同时报出两层的原因, 实际: {e}"
        );
    }

    /// 对照组: 合成函数**不能**退化成「忽略 PG 层」或「恒返回成功」
    ///
    /// 这两条退化方向都很难靠肉眼发现: 前者让第 2 行断言失效, 后者让第 4 行
    /// 失效。故把「输入的 PG 结果确实改变了输出」显式钉住。
    #[test]
    fn dlq_combination_actually_reacts_to_the_pg_result() {
        let nats_bad: Result<(), String> = Err("nats down".into());
        let pg_ok: Result<(), String> = Ok(());
        let pg_bad: Result<(), String> = Err("pg down".into());

        let with_ok = combine_dlq_results(nats_bad.clone(), Some(pg_ok));
        let with_bad = combine_dlq_results(nats_bad, Some(pg_bad));

        assert!(
            with_ok.is_ok() && with_bad.is_err(),
            "同样的 NATS 失败, PG 成功与 PG 失败必须给出不同结论 —— \
             否则说明 PG 层的结果根本没参与判定"
        );
    }

    #[tokio::test]
    async fn stub_does_not_touch_the_real_publisher_counters() {
        let _guard = COUNTER_LOCK.lock().await;
        // 三种语义必须分开, 否则「配了 stub」与「NATS 挂了」在指标上无法区分
        let pub_before = published_event_count();
        let fail_before = failed_event_count();
        let dlq_before = dlq_event_count();
        let p = StubEventPublisher::new();
        p.publish("im.message.created", b"{}").await.unwrap();
        assert_eq!(published_event_count(), pub_before);
        assert_eq!(failed_event_count(), fail_before);
        // stub 是「配置选择」不是「故障」, 故绝不该进 DLQ —— 否则运维会去
        // 排障一个根本不存在的问题
        assert_eq!(dlq_event_count(), dlq_before);
    }

    // -----------------------------------------------------------------------
    // DLQ 编排 (aux-08)
    //
    // 全部用 `tokio::time::pause()` + 极短退避: **时间不是测试的变量**。
    // 若用真实 sleep, 这几个用例会拖慢整个 suite 且在 CI 上变 flaky; 而
    // flaky 的测试会被习惯性忽略, 那比没有测试更糟。
    // -----------------------------------------------------------------------

    /// 全局尝试记录。`std::sync::Mutex` 而非 `tokio::sync::Mutex`:
    /// 它只在**同步**上下文里被 push, 不跨 await 持有, 故不会有
    /// `clippy::await_holding_lock` 问题。
    ///
    /// 所有触碰它的用例都**先拿 `COUNTER_LOCK`**, 故它们彼此串行 ——
    /// 否则 libtest 的并行执行会让「clear 之后数长度」读到别人的数据。
    static ATTEMPTS: std::sync::Mutex<Vec<Duration>> = std::sync::Mutex::new(Vec::new());

    /// 取 [`ATTEMPTS`] 的 guard, **对 poison 免疫**
    ///
    /// 2026-10-05 实测踩到: `assert_eq!(attempts().len(), 4)` 在
    /// 断言 panic 时, 那把 `Mutex` 的 guard **仍在存活**(临时值到语句结束才
    /// drop), 于是锁被 poison。此后每一个 `lock().unwrap()` 都炸
    /// `PoisonError` —— 一个断言失败被伪装成**一串**与它无关的失败,
    /// 真正的原因反而被埋掉了。
    ///
    /// 这里的豁免是安全的: `Vec<Duration>` 里没有任何需要跨 panic 保持一致的
    /// 不变量, 每个用例进来都先 `clear()`。
    fn attempts() -> std::sync::MutexGuard<'static, Vec<Duration>> {
        ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_on_first_attempt_and_never_touches_dlq() {
        let _guard = COUNTER_LOCK.lock().await;
        attempts().clear();
        let dlq_before = dlq_event_count();
        let mut dlq_writes: Vec<DlqRecord> = Vec::new();

        let res = orchestrate_publish(
            "im.message.created",
            b"{}",
            &RetryPolicy::for_tests(),
            |deadline| async move {
                attempts().push(deadline);
                Ok::<(), String>(())
            },
            |rec| {
                dlq_writes.push(rec);
                async { Ok::<(), String>(()) }
            },
        )
        .await;

        assert!(res.is_ok(), "第一次成功就不该返回失败: {res:?}");
        assert_eq!(attempts().len(), 1, "只该尝试一次");
        assert!(dlq_writes.is_empty(), "成功时绝不该写 DLQ");
        assert_eq!(dlq_event_count(), dlq_before, "成功时 DLQ 计数不该动");
    }

    #[tokio::test(start_paused = true)]
    async fn retries_until_success_without_ever_writing_dlq() {
        let _guard = COUNTER_LOCK.lock().await;
        attempts().clear();
        let dlq_before = dlq_event_count();
        let mut dlq_writes = 0usize;

        // 第 3 次才成功 —— 证明重试真的发生了, 且成功后不写 DLQ
        let res = orchestrate_publish(
            "im.message.created",
            b"{}",
            &RetryPolicy::for_tests(),
            |deadline| {
                let n = {
                    let mut g = attempts();
                    g.push(deadline);
                    g.len()
                };
                async move {
                    if n < 3 {
                        Err("transient".to_string())
                    } else {
                        Ok(())
                    }
                }
            },
            |_rec| {
                dlq_writes += 1;
                async { Ok::<(), String>(()) }
            },
        )
        .await;

        assert!(res.is_ok(), "第 3 次成功就该返回 Ok: {res:?}");
        assert_eq!(attempts().len(), 3, "应恰好尝试 3 次");
        assert_eq!(dlq_writes, 0, "成功了就绝不该写 DLQ");
        assert_eq!(dlq_event_count(), dlq_before, "成功时 DLQ 计数不该动");
    }

    #[tokio::test(start_paused = true)]
    async fn exhausts_retries_then_writes_exactly_one_dlq_record() {
        let _guard = COUNTER_LOCK.lock().await;
        attempts().clear();
        let dlq_before = dlq_event_count();
        let failed_before = dlq_write_failed_count();
        let mut records: Vec<DlqRecord> = Vec::new();

        let res = orchestrate_publish(
            "im.message.recalled",
            br#"{"message_id":"11111111-1111-4111-8111-111111111111"}"#,
            &RetryPolicy::for_tests(),
            |deadline| {
                attempts().push(deadline);
                async { Err::<(), String>("always down".to_string()) }
            },
            |rec| {
                records.push(rec);
                async { Ok::<(), String>(()) }
            },
        )
        .await;

        assert!(res.is_err(), "全失败必须返回 Err");
        // aux-08 §C.3「3 次」= 1 次首发 + 3 次重试上限由 backoffs 长度决定;
        // for_tests 的 3 档退避 -> 最多 4 次尝试
        assert_eq!(attempts().len(), 4, "应为首发 + 3 次重试(aux-08 §C.3)");
        assert_eq!(records.len(), 1, "必须**恰好**写 1 条 DLQ 记录");
        assert_eq!(dlq_event_count(), dlq_before + 1, "DLQ 计数应 +1");
        assert_eq!(
            dlq_write_failed_count(),
            failed_before,
            "DLQ 写成功时不该动 write_failed 计数"
        );

        // 逐字段核对 DLQ Record (aux-08 §D.2)
        let r = &records[0];
        assert_eq!(r.original_task, "im.message.recalled");
        assert_eq!(r.dlq_destination, "dlq.event.im.message.recalled");
        assert_eq!(r.context.attempt_count, 4, "attempt_count 应含最终失败那次");
        assert_eq!(r.error.http_status, 503);
        assert_eq!(r.error.code, "SERVICE_UNAVAILABLE");
        assert!(
            r.original_payload.get("message_id").is_some(),
            "合法 JSON 载荷应被解析成对象保留: {}",
            r.original_payload
        );
        assert!(
            r.error.stack.is_none(),
            "stack 恒为 None(见 DlqRecord 文档)"
        );
        assert!(r.context.trace_id.is_none());
    }

    /// 非 JSON 载荷**不能丢** —— 退化成 JSON 字符串也要保留原始字节
    #[tokio::test(start_paused = true)]
    async fn non_json_payload_is_preserved_as_a_string() {
        let _guard = COUNTER_LOCK.lock().await;
        let mut records: Vec<DlqRecord> = Vec::new();
        let _ = orchestrate_publish(
            "im.message.created",
            b"not json at all <<<",
            &RetryPolicy::for_tests(),
            |_d| async { Err::<(), String>("down".into()) },
            |rec| {
                records.push(rec);
                async { Ok::<(), String>(()) }
            },
        )
        .await;
        assert_eq!(
            records[0].original_payload,
            serde_json::Value::String("not json at all <<<".into()),
            "非 JSON 载荷必须原样保留, 不能变成 null 或空对象"
        );
    }

    /// DLQ **写失败**时: 事件真的永久丢失, 必须被单独计数
    ///
    /// 这是本文件最要紧的一条断言 —— 没有它, NATS 整体不可用时的数据丢失
    /// 完全不可见(失败计数在涨, 但没人知道连兜底都没兜住)。
    #[tokio::test(start_paused = true)]
    async fn dlq_write_failure_is_counted_and_never_reported_as_recoverable() {
        let _guard = COUNTER_LOCK.lock().await;
        let dlq_before = dlq_event_count();
        let failed_before = dlq_write_failed_count();

        let res = orchestrate_publish(
            "im.message.created",
            b"{}",
            &RetryPolicy::for_tests(),
            |_d| async { Err::<(), String>("down".into()) },
            |_rec| async { Err::<(), String>("dlq is down too".into()) },
        )
        .await;

        let err = res.expect_err("全失败必须 Err");
        let msg = err.to_string();
        assert!(
            msg.contains("DLQ write also failed"),
            "错误信息必须明说 DLQ 也失败了, 不能让调用方以为可恢复: {msg}"
        );
        assert_eq!(
            dlq_event_count(),
            dlq_before,
            "DLQ 没写成, 就不能计入「已进 DLQ」"
        );
        assert_eq!(
            dlq_write_failed_count(),
            failed_before + 1,
            "永久丢失必须被计数 —— 否则它完全不可见"
        );
    }

    /// 预算**钳住**总尝试次数: 这是「不阻塞 ack」那条性质的可测形式
    ///
    /// 断言用「尝试次数」而不是「实际耗时」: 耗时断言在 CI 上是 flaky 的
    /// 经典来源, 而次数断言测的是同一件事(付不起就不重试), 且是确定的。
    #[tokio::test(start_paused = true)]
    async fn budget_caps_total_attempts_when_each_attempt_is_slow() {
        let _guard = COUNTER_LOCK.lock().await;
        attempts().clear();

        // 每次尝试都吃满 3s ack 上界, 而总预算只有 4s
        let policy = RetryPolicy {
            total_budget: Duration::from_secs(4),
            ack_timeout: Duration::from_secs(3),
            backoffs: [
                Duration::from_millis(500),
                Duration::from_secs(1),
                Duration::from_secs(2),
            ],
        };

        let res = orchestrate_publish(
            "im.message.created",
            b"{}",
            &policy,
            |deadline| async move {
                attempts().push(deadline);
                tokio::time::sleep(deadline).await; // 吃满
                Err::<(), String>("timeout".into())
            },
            |_rec| async { Ok::<(), String>(()) },
        )
        .await;

        assert!(res.is_err());
        let n = attempts().len();
        assert!(
            n < 4,
            "4s 预算装不下 4 次 x 3s 的尝试, 应当更早转 DLQ, 实际尝试 {n} 次"
        );
        assert!(n >= 1, "至少要尝试过一次: {n}");
    }

    /// 预算**足够**时, 退避档用满 —— 证明上面那条不是因为「退避数组被截断」
    #[tokio::test(start_paused = true)]
    async fn generous_budget_uses_every_backoff_slot() {
        let _guard = COUNTER_LOCK.lock().await;
        attempts().clear();

        let _ = orchestrate_publish(
            "im.message.created",
            b"{}",
            &RetryPolicy::for_tests(), // 60s 预算, 极短退避
            |deadline| async move {
                attempts().push(deadline);
                Err::<(), String>("fast failure".into())
            },
            |_rec| async { Ok::<(), String>(()) },
        )
        .await;

        assert_eq!(attempts().len(), 4, "预算充足时应走满 1 次首发 + 3 档退避");
    }
}
