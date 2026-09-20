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
//! - `IM_HTTP_PORT` (u16, 默认 8080)
//! - `IM_POSTGRES_URL` (postgres://...) — 必填
//! - `IM_JWT_SIGNING_KEYS` (JSON 数组, 至少 1 项) — 必填
//! - `IM_REFRESH_PEPPER` (string) — 必填
//! - `IM_EVENT_PUBLISHER_KIND` (stub|nats, 默认 stub)
//! - `IM_EVENT_PUBLISHER_NATS_URL` (optional, 仅 kind=nats)
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

use im_common::config::{
    AppConfig, EventPublisherKind, SigningKeyConfig,
};
use im_common::ids::EnvironmentId;
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::service::ConversationService;
use im_core::event::publisher::NatsEventPublisher;
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
const DEFAULT_ACCESS_TOKEN_TTL_SECONDS: i64 = 900;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 1. 加载配置 (figment: default.toml < local.toml < env vars, per im-common::config)
    let cfg = AppConfig::load().unwrap_or_else(|e| {
        eprintln!("[im-gateway] config load failed: {e}");
        eprintln!("[im-gateway] hint: set IM_POSTGRES_URL + IM_JWT_SIGNING_KEYS + IM_REFRESH_PEPPER");
        eprintln!("[im-gateway]        or provide config/default.toml (see config/local.toml.example)");
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
            return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
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
    let event_publisher: Arc<dyn EventPublisher> = match cfg.event_publisher.kind {
        EventPublisherKind::Stub => {
            tracing::info!("EventPublisher = Stub (MVP, no NATS connection)");
            // NatsEventPublisher::connect 本身就是 no-op stub (per event/publisher.rs)
            Arc::new(
                NatsEventPublisher::connect("nats://stub:4222")
                    .await
                    .expect("NatsEventPublisher stub never fails"),
            )
        }
        EventPublisherKind::Nats => {
            let url = cfg.event_publisher.nats_url.as_deref().unwrap_or("nats://localhost:4222");
            tracing::info!(nats_url = url, "EventPublisher = Nats (MVP-stub: no real connect)");
            Arc::new(
                NatsEventPublisher::connect(url)
                    .await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?,
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
        event_publisher,
        conversation_repo,
    ));
    let identity_service = Arc::new(IdentityService::new(
        user_repo,
        device_repo,
        token_service.clone(),
        server_secrets,
    ));

    // 9. AppState
    let app_state = AppState::new(
        conversation_service,
        message_service,
        token_service,
        identity_service,
    );

    let http_port = cfg.http_port;
    tracing::info!(http_port, "im-gateway binding HTTP server");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .service(web::scope("/v1").configure(http::configure))
            .route("/healthz", web::get().to(health::healthz))
            .route("/readyz", web::get().to(readyz))
            .route("/metrics", web::get().to(health::metrics))
    })
    .bind(("0.0.0.0", http_port))?
    .run()
    .await
}

/// /readyz — D-1 wire-up 后, 只有 AppState 构造成功才 ready
async fn readyz() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(serde_json::json!({"status": "ready"}))
}

// 避免未用警告 (SigningKeyConfig 在 cfg 加载时使用, 此处 suppress)
#[allow(dead_code)]
fn _ensure_signing_key_config_used(_: &SigningKeyConfig) {}
