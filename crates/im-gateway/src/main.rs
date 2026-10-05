//! im-gateway 入口
//!
//! 依据: ImplementationSpec §7.5 + DetailedDesign §2 + D-1 WBS
//!
//! ## 启动流程 (D-1 wire-up)
//! 1. `AppConfig::load()` — figment + dotenvy + 双密钥 JSON (per im-common::config)
//! 2. 连接 PgPool (per `IM__DATABASE__URL` / `postgres_url`)
//! 3. 构造 4 个 Service: ConversationService + MessageService + TokenService + IdentityService
//! 4. 注入 AppState 到 actix HttpServer (`web::Data`)
//! 5. bind http_port + run
//!
//! ## 环境变量覆盖
//!
//! 规则只有一条(per `im-common::config::AppConfig::load_from_paths` 的实现):
//! **变量名去掉 `IM_` 前缀后小写, 必须等于 `AppConfig` 的字段名**。该实现遍历
//! `std::env::vars()` 逐个 strip `IM_` 再小写, 不认识的 `IM_*` 变量会被
//! **静默忽略**。
//!
//! - `IM_HTTP_PORT` (u16, 默认 8080)
//! - `IM_POSTGRES_URL` (postgres://...) — 必填
//! - `IM_JWT_SIGNING_KEYS` (JSON 数组, 至少 1 项) — 必填
//! - `IM_REFRESH_PEPPER` (string) — 必填
//! - `IM_EVENT_PUBLISHER` (JSON 对象) — 必填
//!
//! ### `IM_EVENT_PUBLISHER` 是**一个** JSON 值, 不是两个扁平变量
//!
//! `AppConfig` 的字段是嵌套的 `event_publisher: EventPublisherConfig`,
//! 所以正确写法是:
//!
//! ```text
//! IM_EVENT_PUBLISHER={"kind":"stub","nats_url":""}
//! IM_EVENT_PUBLISHER={"kind":"nats","nats_url":"nats://nats:4222"}
//! ```
//!
//! **此前本文件的文档写的是 `IM_EVENT_PUBLISHER_KIND` /
//! `IM_EVENT_PUBLISHER_NATS_URL`, 那是错的**: 它们会被 strip 成
//! `event_publisher_kind` / `event_publisher_nats_url`, 不是任何字段名。
//! 照着配的结果是 `event_publisher` 仍然缺失(它没有 `#[serde(default)]`),
//! 服务启动失败。`deploy/k3s/dev/im-gateway.yaml` 与 `crates/jobctl` 用的
//! 一直是上面这个 JSON 写法, 即 k3s 清单是对的、只有本注释是错的。
//!
//! ### `config/default.toml` **不会被** `load()` 读到
//!
//! `AppConfig::load()` 调的是 `load_from_paths(None, None)`, 而 TOML provider
//! 只在 `config_dir` 为 `Some` 时才 merge(见 config.rs 的 `if let Some(dir)`)。
//! 故 `config/default.toml` / `config/local.toml` 在默认启动路径上**完全不参与**,
//! 生效的只有 `#[serde(default)]` 内置默认值与环境变量。**本文件下方的报错
//! 提示曾让 operator 去提供那个文件, 那是误导, 已更正。**
//!
//! ## 已知缺口 (per 138 §8 + D-1 MVP 范围)
//! - 46 项完整配置实装在 V1 阶段, 本 D-1 仅含 MVP 必需 9 项
//! - V1: 接 KMS 取 `server_secrets`, MVP 从 TOML 读 (gitignored)
//! - V1: 双密钥 JSON 格式正式化 (含 kid + active + key), MVP 已落

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, App, HttpServer};
use secrecy::{ExposeSecret, Secret};
use sqlx::postgres::PgPoolOptions;

use im_common::config::{AppConfig, EventPublisherKind, SigningKeyConfig};
use im_common::ids::EnvironmentId;
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::service::ConversationService;
use im_core::event::publisher::{DlqSink, NatsEventPublisher, PgDlqSink, StubEventPublisher};
use im_core::event::EventPublisher;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::service::IdentityService;
use im_core::identity::token::{SigningKey, TokenService};
use im_core::message::pg::{PgMessageRepository, PgSequenceAllocator};
use im_core::message::service::MessageService;

use crate::http::state::AppState;

mod error;
mod health;
mod http;
mod placeholder;
mod ws;

/// 默认 access token TTL (秒) — 15 分钟
// 守门 #1 缺口台账: 缺口 #A — http/auth_handlers.rs 响应 `expires_in` 当前写死 900,
// 待 V1 抽 `TokenService::access_ttl_seconds()` 后改为读本常量 (per auth_handlers.rs 模块 doc)。
// 保留常量而非删除,per docs/Project-Status.md §1.1.1 占位符保留约定。
#[allow(dead_code)]
const DEFAULT_ACCESS_TOKEN_TTL_SECONDS: i64 = 900;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 1. 加载配置
    //
    // 只加载「内置默认值 + 环境变量」: `AppConfig::load()` 走
    // `load_from_paths(None, None)`, config_dir 为 None, 故 `config/*.toml`
    // 在这条路径上**不参与**(见文件头「环境变量覆盖」一节)。
    let cfg = AppConfig::load().unwrap_or_else(|e| {
        // 注意 `{e}` 打出来是 `internal error` —— `AppError::Internal` 的
        // `#[error("internal error")]` 漏了 `{0}` 占位符, figment 的完整诊断
        // (哪个字段缺失/值哪里不对) 被 Display 丢掉了。
        //
        // **刻意不把那行诊断放出来**: figment 解析 IM_JWT_SIGNING_KEYS 失败时
        // 会回显输入值, 而输入值就是 JWT 签名密钥原文, 会直接打到 stderr。
        // 要修应改 `AppError::Internal` 的 Display 或另加一个只带字段名的
        // Config 变体, 而不是把 `{0}` 填回去。
        eprintln!("[im-gateway] config load failed: {e}");
        eprintln!("[im-gateway] the detailed reason is intentionally not printed here:");
        eprintln!("[im-gateway]   it may echo the value of IM_JWT_SIGNING_KEYS, i.e. the signing key.");
        eprintln!("[im-gateway] 4 required env vars (AppConfig fields have no serde default):");
        eprintln!("[im-gateway]   IM_POSTGRES_URL      postgres://user:pass@host:port/db");
        eprintln!("[im-gateway]   IM_JWT_SIGNING_KEYS   JSON array, e.g. [{{\"kid\":\"v1\",\"key\":\"...\",\"active\":true}}]");
        eprintln!("[im-gateway]   IM_REFRESH_PEPPER     any non-empty string");
        eprintln!("[im-gateway]   IM_EVENT_PUBLISHER    one JSON object, e.g. {{\"kind\":\"stub\",\"nats_url\":\"\"}}");
        eprintln!("[im-gateway] note: config/default.toml is NOT read by this code path.");
        eprintln!("[im-gateway] run scripts/preflight.ps1 (ships with the release package) to get");
        eprintln!("[im-gateway]   a per-variable verdict; it never prints any value.");
        std::process::exit(78);
    });

    // 2. tracing 初始化
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        http_port = cfg.http_port,
        event_publisher = ?cfg.event_publisher.kind,
        max_message_size_bytes = cfg.max_message_size_bytes,
        "im-gateway starting (D-1 wire-up: AppConfig + PgPool + 4 Service + AppState)"
    );

    // 3. PgPool (per cfg.postgres_url)
    let pg_pool = match PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&cfg.postgres_url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "PgPool connect failed");
            return Err(std::io::Error::other(e));
        }
    };

    // 4. Repository 实例化
    let conversation_repo: Arc<dyn im_core::conversation::repository::ConversationRepository> =
        Arc::new(PgConversationRepository::new(pg_pool.clone()));
    let message_repo: Arc<dyn im_core::message::repository::MessageRepository> =
        Arc::new(PgMessageRepository::new(pg_pool.clone()));
    let sequence_allocator: Arc<dyn im_core::message::sequence::SequenceAllocator> =
        Arc::new(PgSequenceAllocator::new(pg_pool.clone()));
    let user_repo = PgUserRepository::new(pg_pool.clone());
    let device_repo = PgDeviceSessionRepository::new(pg_pool.clone());

    // 5. EventPublisher
    //
    // 2026-10-04 D-3 实装: 两条分支现在是**两种不同的类型**。
    // 此前两者都调 `NatsEventPublisher::connect`, 而那个 connect 是 no-op,
    // 于是 `kind=nats` 也不连任何东西 —— 配了 NATS 与没配, 行为一模一样。
    let event_publisher: Arc<dyn EventPublisher> = match cfg.event_publisher.kind {
        EventPublisherKind::Stub => {
            tracing::warn!(
                "EventPublisher = Stub (IM_EVENT_PUBLISHER_KIND=stub): \
                 events will NOT be delivered, cross-pod sync is OFF. \
                 Set IM_EVENT_PUBLISHER_KIND=nats to enable it."
            );
            Arc::new(StubEventPublisher::new())
        }
        EventPublisherKind::Nats => {
            let url = cfg
                .event_publisher
                .nats_url
                .as_deref()
                .unwrap_or("nats://localhost:4222");
            // aux-08 §D.3 的 **PG 长留存层**(第 2 层)。
            //
            // 它的全部价值在于接住「NATS 整体不可用」—— 而那正是 NATS 层
            // 自己失效的场景。没有它, NATS 挂掉时死信**也写不进去**, 事件
            // 就真的永久没了(尽管 `IM_EVENTS_DLQ_WRITE_FAILED` 会诚实计数)。
            //
            // 生产下**总是**接上: `IM_POSTGRES_URL` 是必填项, 池在此之前
            // 已经建好(第 3 步)。故这里没有「PG 层可选」的分叉 —— 可选只
            // 存在于 `NatsEventPublisher` 那一侧, 供测试用。
            let pg_dlq: Arc<dyn DlqSink> = Arc::new(PgDlqSink::new(pg_pool.clone()));
            // 连不上就**启动失败**, 不静默退化成 stub。理由: 运维显式配了
            // nats, 却在 NATS 不可用时得到一个「看起来正常、事件全丢」的进程,
            // 正是本仓反复修掉的那类假绿灯。`kind=stub` 仍然是可用的显式选择。
            Arc::new(
                NatsEventPublisher::connect(url, Some(pg_dlq))
                    .await
                    .map_err(|e| std::io::Error::other(e.to_string()))?,
            )
        }
    };

    // 6. TokenService (SigningKeyConfig → SigningKey 转换)
    let signing_keys: Vec<SigningKey> = cfg
        .jwt_signing_keys
        .iter()
        .map(|k| SigningKey {
            kid: k.kid.clone(),
            key: Secret::new(k.key.clone()),
        })
        .collect();
    assert!(
        !signing_keys.is_empty(),
        "at least one signing key required (set IM__JWT__SIGNING__KEYS)"
    );

    let access_ttl = chrono::Duration::seconds(cfg.access_token_ttl_seconds);
    let refresh_pepper: Secret<String> = Secret::new(cfg.refresh_pepper.expose_secret().clone());
    let token_service = Arc::new(TokenService::new(signing_keys, access_ttl, refresh_pepper));

    // 7. server_secrets HashMap<EnvironmentId, Secret<String>> (从 cfg 转)
    let server_secrets: HashMap<EnvironmentId, Secret<String>> = cfg
        .server_secrets
        .iter()
        .map(|(k, v)| (*k, Secret::new(v.expose_secret().clone())))
        .collect();

    // 8. ConversationService + MessageService + IdentityService
    let conversation_service = Arc::new(ConversationService::new(conversation_repo.clone()));
    let message_service = Arc::new(MessageService::new(
        message_repo,
        sequence_allocator,
        event_publisher.clone(),
        conversation_repo,
    ));
    let identity_service = Arc::new(IdentityService::new(
        user_repo,
        device_repo,
        token_service.clone(),
        server_secrets,
    ));

    // 9. AppState
    let settings_service = Arc::new(im_core::settings::service::SettingsService::new(
        pg_pool.clone(),
    ));
    let reaction_repo: Arc<dyn im_core::reaction::repository::ReactionRepository> = Arc::new(
        im_core::reaction::pg::PgReactionRepository::new(pg_pool.clone()),
    );
    // message_repo 在第 4 步已被 move 进 MessageService, 这里需要一份给
    // ReactionService 做「消息 → conversation_id」的反查。取一份新的 Arc 指向
    // 同一个 pool, 代价可忽略, 且避免了把 MessageService 的构造顺序改成环状。
    let message_repo_for_reaction: Arc<dyn im_core::message::repository::MessageRepository> =
        Arc::new(PgMessageRepository::new(pg_pool.clone()));
    let conversation_repo_for_reaction: Arc<
        dyn im_core::conversation::repository::ConversationRepository,
    > = Arc::new(PgConversationRepository::new(pg_pool.clone()));
    let reaction_service = Arc::new(im_core::reaction::service::ReactionService::new(
        reaction_repo,
        message_repo_for_reaction,
        conversation_repo_for_reaction,
    ));

    let relationship_service = Arc::new(im_core::relationship::service::RelationshipService::new(
        Arc::new(im_core::relationship::pg::PgFriendshipRepository::new(
            pg_pool.clone(),
        )),
    ));

    let app_state = AppState::new(
        conversation_service,
        message_service,
        token_service,
        identity_service,
        settings_service,
        reaction_service,
        relationship_service,
    );

    // 9b. WS 广播中枢 —— 进程内单例。
    //
    // 必须是**所有 WS 连接共享的同一个** instance: 广播的意义就在于把一条
    // 消息送到「其它连接」, 每连接一个 hub 等于没有 hub(发给自己都不行)。
    // 用 `web::Data` 注入而非构造时 `new()`, 是为了让 actix 的每个 worker
    // 线程拿到的是同一份 —— `web::Data` 内部是 `Arc`, 克隆不复制。
    //
    // 若漏掉这一行, 编译仍会通过(extract 器是运行期解析的), 但每个
    // `/v1/ws` 请求都会拿到 500 —— 故在此显式注册。
    let ws_hub = web::Data::new(ws::hub::WsHub::new());

    // readiness 需要真实探测 PG, 故把 pool 也注入进去。
    //
    // 2026-10-03: `/readyz` 此前是 `main.rs` 里一个 `async fn readyz()`,
    // **无条件返回 200** —— 而 `DetailedDesign §5` 要求「PG/Valkey/NATS
    // 全部可达才 200」。后果: k8s 会把连不上数据库的实例判为 ready 并把
    // 流量打过去, 而那个实例的每个业务端点都会 500。
    let readiness_pool = web::Data::new(pg_pool.clone());

    // 2026-10-05: NATS 也纳入就绪判定 —— `DetailedDesign §5` 与
    // `ImplementationSpec §3.1.7` 都要求, 而 D-3 落地后 publisher 已经
    // 拿得到了(此前 `/readyz` 报的是恒定的 `not_checked`)。
    //
    // 单独注入而不是塞进 `AppState`: `AppState` 服务于 `/v1` 的业务
    // handler, 而探针只需要 publisher 一个字段; 让探针依赖整个 AppState
    // 会使任何构造 AppState 的测试都必须先凑齐 7 个 service。
    //
    // 漏掉这一行, 编译仍会通过(extract 器是运行期解析的), 但每个
    // `/readyz` 请求都会 500 —— 故在此显式注册, 与 `ws_hub` 同一理由。
    let readiness_publisher: web::Data<Arc<dyn EventPublisher>> =
        web::Data::new(event_publisher.clone());

    let http_port = cfg.http_port;
    tracing::info!(http_port, "im-gateway binding HTTP server");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .app_data(ws_hub.clone())
            .app_data(readiness_pool.clone())
            .app_data(readiness_publisher.clone())
            .service(web::scope("/v1").configure(http::configure))
            .route("/healthz", web::get().to(health::healthz))
            .route("/readyz", web::get().to(health::readyz))
            .route("/metrics", web::get().to(health::metrics))
    })
    .bind(("0.0.0.0", http_port))?
    .run()
    .await
}

// 避免未用警告 (SigningKeyConfig 在 cfg 加载时使用, 此处 suppress)
#[allow(dead_code)]
fn _ensure_signing_key_config_used(_: &SigningKeyConfig) {}
