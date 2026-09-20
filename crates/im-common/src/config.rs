//! 配置加载 —— IM1.0 `AppConfig::load()` 唯一实现
//!
//! 依据:
//! - `docs/ImplementationSpec.md` §5 + §6(加载顺序 §6.1 + 校验规则 §6.2 + 热重载 §6.3 + Secret 映射 §6.4)
//! - `docs/templates/04-detailed-design/auxiliary/aux-12-config-spec.md` §A 27 env vars + §B 10 Secret Keys +
//!   §C 9 environments.settings + §D 11 校验规则 + §E 7 加载顺序 + §K Rust trait 草案
//! - `docs/BasicDesign.md` §14.3(9 项 environment.settings 字段)+ §14.4(环境差异)
//!
//! ## 加载顺序(7 步,per §E / §6.1)
//! 1. 读取 `IM_ENV` 决定加载 `config/{IM_ENV}.toml`(可选)
//! 2. 读取 `.env` 文件(dotenvy,仅 `IM_ENV=dev` 时)
//! 3. 读取环境变量(覆盖上面,优先级最高)
//! 4. 解析双密钥 JSON 数组(`IM_JWT_SIGNING_KEYS`)
//! 5. 解析 server_secrets JSON map(`IM_SERVER_SECRETS`)
//! 6. 校验必填项 + 范围 + 关联约束,失败 → `ConfigError::Multi` 聚合返回
//! 7. 构造 `AppConfig` 注入 Runtime
//!
//! ## 错误聚合
//! 校验阶段**不**快速失败,而是把所有违规收集进 `ConfigError::Multi(Vec<ConfigIssue>)`,
//! 让 `im-gateway` / `im-core` 启动日志一次性打印全部问题(避免运维改一项 → 启 → 再改一项循环)。
//!
//! ## 退出码
//! 启动失败统一走 `process::exit(78)`(`sysexists.h` 的 `EX_CONFIG`)。本模块提供 `load_or_exit()` 包装层;
//! 单纯 `load()` 返回 `Result`,便于单测与 `im-testkit` 复用。

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Environment
// ============================================================================

/// IM1.0 运行环境(由 `IM_ENV` 决定)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Staging,
    Prod,
}

impl Environment {
    /// 全部合法值(用于校验 `IM_ENV`)
    pub const ALL: &'static [&'static str] = &["dev", "staging", "prod"];

    /// 是否 dev(决定是否自动加载 `.env` 文件,per §E step 2)
    #[inline]
    pub fn is_dev(self) -> bool {
        matches!(self, Environment::Dev)
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Environment::Dev => f.write_str("dev"),
            Environment::Staging => f.write_str("staging"),
            Environment::Prod => f.write_str("prod"),
        }
    }
}

impl FromStr for Environment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dev" => Ok(Environment::Dev),
            "staging" => Ok(Environment::Staging),
            "prod" => Ok(Environment::Prod),
            other => Err(format!(
                "IM_ENV={other:?} 非法,合法值: dev / staging / prod (per aux-12 §A.1 / ImplementationSpec §6.2)"
            )),
        }
    }
}

// ============================================================================
// SigningKey — JWT 签名密钥
// ============================================================================

/// JWT 签名密钥(含 `kid`,轮换期双密钥 v1+v2 共存,per aux-12 §A.1 + §B.1)
///
/// `key` 持有 `Secret<[u8; 32]>` —— secrecy 0.8 仅对固定长度数组 [T; 1..=64] 提供
/// `CloneableSecret` / `DebugSecret`,所以 32-byte 数组是天然契合项;调用方通过
/// `ExposeSecret::expose_secret()` 拿到 `&[u8; 32]`。
///
/// 注:`crates/im-core/src/identity/token.rs` 暂用 `SecretString` 作为过渡实现,
/// V1 整合时统一迁移到 `im_common::config::SigningKey`(该 PR 注释已声明等待)。
#[derive(Debug, Clone)]
pub struct SigningKey {
    pub kid: String,
    pub key: Secret<[u8; 32]>,
}

impl SigningKey {
    /// 原始密钥引用
    #[inline]
    pub fn key_bytes(&self) -> &[u8; 32] {
        self.key.expose_secret()
    }
}

// ============================================================================
// 子配置(Ws / RateLimit / Message / Token / DbPool / Auth / Obs)
// ============================================================================

/// §A.2 第 11-13 项 — WS 协议三层参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsConfig {
    /// `IM_WS_PING_INTERVAL_SECONDS` 默认 30, ≥ 5
    pub ping_interval_seconds: u32,
    /// `IM_WS_HEARTBEAT_TIMEOUT_SECONDS` 默认 60, 必须 > `ping_interval_seconds`
    pub heartbeat_timeout_seconds: u32,
    /// `IM_WS_AUTH_TIMEOUT_MS` 默认 100, 范围 50-5000
    pub auth_timeout_ms: u32,
}

/// §A.2 第 14-17 项 — 限流阈值(全局四类)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` 默认 60, ≥ 1
    pub send_message_per_min: u32,
    /// `IM_RATE_LIMIT_GUEST_REGISTER_PER_HOUR` 默认 10, ≥ 1
    pub guest_register_per_hour: u32,
    /// `IM_RATE_LIMIT_TOKEN_EXCHANGE_PER_MIN` 默认 1000, ≥ 1
    pub token_exchange_per_min: u32,
    /// `IM_RATE_LIMIT_BUCKET_SIZE` 默认 100, ≥ 1
    pub bucket_size: u32,
}

/// §A.2 第 18-19 项 — 消息大小 + 撤回时间窗
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageConfig {
    /// `IM_MESSAGE_MAX_SIZE_BYTES` 默认 65536 (64KB), 范围 1024-1048576 (1KB-1MB)
    pub max_size_bytes: u32,
    /// `IM_MESSAGE_RECALL_WINDOW_SECONDS` 默认 120, ≥ 1
    pub recall_window_seconds: u32,
}

/// §A.2 第 20-21 项 — Access / Refresh Token TTL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// `IM_ACCESS_TOKEN_TTL_SECONDS` 默认 900 (15min), ≥ 60
    pub access_ttl_seconds: u32,
    /// `IM_REFRESH_TOKEN_TTL_SECONDS` 默认 2592000 (30d), ≥ 86400
    pub refresh_ttl_seconds: u32,
}

/// §A.2 第 22 项 — sqlx 连接池上限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbPoolConfig {
    /// `IM_DB_POOL_MAX_CONNECTIONS` 默认 20, 范围 1-200
    pub max_connections: u32,
}

/// §A.2 第 23 项 — Token 校验中间件缓存(0 表示关闭)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// `IM_AUTH_MIDDLEWARE_CACHE_SECONDS` 默认 5, 范围 0-60
    pub middleware_cache_seconds: u32,
}

/// §A.3 第 25-27 项 — Prometheus + OTel + 服务名
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsConfig {
    /// `IM_PROMETHEUS_BIND` 空 = 关闭
    pub prometheus_bind: Option<String>,
    /// `IM_OTEL_EXPORTER_OTLP_ENDPOINT` 空 = 关闭(MVP 关闭)
    pub otel_otlp_endpoint: Option<String>,
    /// `IM_SERVICE_NAME` 默认 `CARGO_PKG_NAME`
    pub service_name: String,
}

// ============================================================================
// AppConfig
// ============================================================================

/// IM1.0 启动期配置 —— 27 项 env vars + 10 项 K3s Secret(经 env 注入后解析) = 46 项聚合
///
/// 字段一一对应 `aux-12-config-spec.md` §A + §B + §C 三表。
#[derive(Debug)]
pub struct AppConfig {
    // §A.1 必填(9 项)
    pub env: Environment,
    pub database_url: Secret<String>,
    pub valkey_url: Secret<String>,
    pub nats_url: Secret<String>,
    pub http_port: u16,
    pub grpc_port: u16,
    pub jwt_signing_keys: Vec<SigningKey>, // §A.1 第 7
       pub server_secrets: HashMap<Uuid, Secret<[u8; 32]>>, // §A.1 第 8
       pub refresh_token_pepper: Secret<[u8; 32]>, // §A.1 第 9

    // §A.2 可调(15 项)聚合到 6 个子 struct
    pub log_level: String,
    pub ws: WsConfig,
    pub rate_limit: RateLimitConfig,
    pub message: MessageConfig,
    pub token: TokenConfig,
    pub db_pool: DbPoolConfig,
    pub auth: AuthConfig,
    pub guest_default_name: String,                                  // §A.2 第 24

    // §A.3 可观测(3 项)
    pub obs: ObsConfig,
}

// ============================================================================
// 加载用 raw DTO —— 反序列化中间结构,所有字段 Optional 以便区分 "用户未设置" vs "默认值"
// ============================================================================

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    // env 是 figment-only 字段,IM_ENV 的实际加载在 load() 第一行直接走 std::env::var
    //(因为后续 toml / dotenvy 都需要 env 值来决定路径)
    #[allow(dead_code)]
    #[serde(default)]
    env: Option<String>,

    // §A.1 必填 9
    #[serde(default)]
    database_url: Option<String>,
    #[serde(default)]
    valkey_url: Option<String>,
    #[serde(default)]
    nats_url: Option<String>,
    #[serde(default)]
    http_port: Option<u32>,
    #[serde(default)]
    grpc_port: Option<u32>,
    // 注:IM_JWT_SIGNING_KEYS / IM_SERVER_SECRETS / IM_REFRESH_TOKEN_PEPPER 三个
    // JSON / hex 字段**不**走 figment (Env provider 会尝试按类型反序列化,
    // 而我们要的是 raw string 再二次解析)。它们在 load() 里直接 std::env::var 取。

    // §A.2 可调 15
    #[serde(default)]
    log_level: Option<String>,
    #[serde(default)]
    ws_ping_interval_seconds: Option<u32>,
    #[serde(default)]
    ws_heartbeat_timeout_seconds: Option<u32>,
    #[serde(default)]
    ws_auth_timeout_ms: Option<u32>,
    #[serde(default)]
    rate_limit_send_message_per_min: Option<u32>,
    #[serde(default)]
    rate_limit_guest_register_per_hour: Option<u32>,
    #[serde(default)]
    rate_limit_token_exchange_per_min: Option<u32>,
    #[serde(default)]
    rate_limit_bucket_size: Option<u32>,
    #[serde(default)]
    message_max_size_bytes: Option<u32>,
    #[serde(default)]
    message_recall_window_seconds: Option<u32>,
    #[serde(default)]
    access_token_ttl_seconds: Option<u32>,
    #[serde(default)]
    refresh_token_ttl_seconds: Option<u32>,
    #[serde(default)]
    db_pool_max_connections: Option<u32>,
    #[serde(default)]
    auth_middleware_cache_seconds: Option<u32>,
    #[serde(default)]
    guest_user_default_name: Option<String>,

    // §A.3 可观测 3
    #[serde(default)]
    prometheus_bind: Option<String>,
    #[serde(default)]
    otel_exporter_otlp_endpoint: Option<String>,
    #[serde(default)]
    service_name: Option<String>,
}

// ============================================================================
// ConfigError — 错误聚合
// ============================================================================

/// 单个配置错误(field + 描述)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    pub field: String,
    pub message: String,
}

impl fmt::Display for ConfigIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

/// 配置加载 / 校验错误,聚合所有违规
#[derive(Debug)]
pub enum ConfigError {
    /// 至少一条 `ConfigIssue`
    Multi(Vec<ConfigIssue>),
    /// 图层解析失败(TOML / .env / Env var 类型转换)
    Figment(Box<figment::Error>),
    /// dotenvy 读取 `.env` 失败(非致命,仅 dev)
    Dotenv(dotenvy::Error),
    /// `IM_JWT_SIGNING_KEYS` JSON 数组格式错误
    JsonParse(String),
    /// `IM_SERVER_SECRETS` JSON map 格式错误
    JsonMapParse(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Multi(issues) => {
                writeln!(f, "AppConfig 校验失败 ({} 项):", issues.len())?;
                for issue in issues {
                    writeln!(f, "  - {}", issue)?;
                }
                Ok(())
            }
            ConfigError::Figment(e) => write!(f, "figment 解析失败: {e}"),
            ConfigError::Dotenv(e) => write!(f, "dotenvy 读取 .env 失败: {e}"),
            ConfigError::JsonParse(s) => write!(f, "IM_JWT_SIGNING_KEYS JSON 数组解析失败: {s}"),
            ConfigError::JsonMapParse(s) => write!(f, "IM_SERVER_SECRETS JSON map 解析失败: {s}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<figment::Error> for ConfigError {
    fn from(e: figment::Error) -> Self {
        ConfigError::Figment(Box::new(e))
    }
}

impl From<dotenvy::Error> for ConfigError {
    fn from(e: dotenvy::Error) -> Self {
        ConfigError::Dotenv(e)
    }
}

// ============================================================================
// AppConfig::load 实现
// ============================================================================

impl AppConfig {
    /// 主入口 — 按 §E 7 步加载顺序 + 校验 + 构造
    ///
    /// `IM_ENV` 决定:
    /// - 加载 `config/{IM_ENV}.toml`(若存在;不强制)
    /// - 是否自动加载 `.env`(仅 `dev`)
    pub fn load() -> Result<Self, ConfigError> {
        // ----- step 1: IM_ENV 必须存在(必填,否则直接 fail-fast,无法继续)-----
        let env_raw = std::env::var("IM_ENV").map_err(|_| {
            ConfigError::Multi(vec![ConfigIssue {
                field: "IM_ENV".into(),
                message: "必填 (aux-12 §A.1 第 1 项),缺失".into(),
            }])
        })?;
        let env: Environment = env_raw.parse().map_err(|msg| {
            ConfigError::Multi(vec![ConfigIssue {
                field: "IM_ENV".into(),
                message: msg,
            }])
        })?;

        // ----- step 2: 加载 TOML(优先级最低)-----
        let mut figment = Figment::new();
        let toml_path = PathBuf::from(format!("config/{env}.toml"));
        if toml_path.exists() {
            figment = figment.merge(Toml::file(toml_path));
        }

        // ----- step 3: 仅 dev 加载 .env(dotenvy)-----
        if env.is_dev() {
            match dotenvy::dotenv() {
                Ok(path) => {
                    tracing::debug!(?path, "dotenvy 加载 .env 成功");
                }
                Err(e) if e.not_found() => {
                    // .env 不存在不算错(dev 环境常见)
                    tracing::debug!("dotenvy: .env 不存在,跳过");
                }
                Err(e) => return Err(ConfigError::Dotenv(e)),
            }
        }

        // ----- step 4: 环境变量(优先级最高,override 上面两层)-----
        // 用 IM_ 前缀 + 下划线转小写读取所有变量
        figment = figment.merge(Env::prefixed("IM_").split("__"));

        let raw: RawConfig = figment.extract()?;

        // ----- step 5: 解析 JSON 数组 / map / hex (单独处理,不走 figment 因其会按类型反序列化)-----
        let mut issues: Vec<ConfigIssue> = Vec::new();

        // IM_JWT_SIGNING_KEYS — JSON 数组 (直接读 raw env,figment 会尝试解析为 typed 值)
        let jwt_signing_keys = match std::env::var("IM_JWT_SIGNING_KEYS")
            .ok()
            .filter(|s| !s.is_empty())
        {
            None => {
                issues.push(ConfigIssue {
                    field: "IM_JWT_SIGNING_KEYS".into(),
                    message: "必填 (aux-12 §A.1 第 7 项),缺失或为空".into(),
                });
                Vec::new()
            }
            Some(s) => match parse_jwt_signing_keys(&s) {
                Ok(v) => v,
                Err(msg) => {
                    issues.push(ConfigIssue {
                        field: "IM_JWT_SIGNING_KEYS".into(),
                        message: msg,
                    });
                    Vec::new()
                }
            },
        };

        // IM_SERVER_SECRETS — JSON map,key=UUID,value=hex(64)
        let server_secrets = match std::env::var("IM_SERVER_SECRETS")
            .ok()
            .filter(|s| !s.is_empty())
        {
            None => {
                issues.push(ConfigIssue {
                    field: "IM_SERVER_SECRETS".into(),
                    message: "必填 (aux-12 §A.1 第 8 项),缺失或为空".into(),
                });
                HashMap::new()
            }
            Some(s) => match parse_server_secrets(&s) {
                Ok(m) => m,
                Err(msg) => {
                    issues.push(ConfigIssue {
                        field: "IM_SERVER_SECRETS".into(),
                        message: msg,
                    });
                    HashMap::new()
                }
            },
        };

        // IM_REFRESH_TOKEN_PEPPER — hex(64)
        let refresh_token_pepper = match std::env::var("IM_REFRESH_TOKEN_PEPPER")
            .ok()
            .filter(|s| !s.is_empty())
        {
            None => {
                issues.push(ConfigIssue {
                    field: "IM_REFRESH_TOKEN_PEPPER".into(),
                    message: "必填 (aux-12 §A.1 第 9 项),缺失或为空".into(),
                });
                None
            }
            Some(s) => match parse_hex_32(&s) {
                Ok(bytes) => Some(Secret::new(bytes)),
                Err(msg) => {
                    issues.push(ConfigIssue {
                        field: "IM_REFRESH_TOKEN_PEPPER".into(),
                        message: msg,
                    });
                    None
                }
            },
        };

        // ----- step 6: 必填 + 范围 + 关联校验(11 条 + 关联约束,全部聚合)-----
        // §A.1 第 2-6: 必填且 URL/范围
        let database_url = require_secret(&mut issues, "IM_DATABASE_URL", raw.database_url.as_deref(), |s| {
            // 启动 ping 校验由调用方在进程初始化后做(config::load() 内不连 DB,保持纯函数)
            if s.is_empty() {
                Err("不能为空字符串".into())
            } else {
                Ok(Secret::new(s.to_string()))
            }
        });

        let valkey_url = require_secret(&mut issues, "IM_VALKEY_URL", raw.valkey_url.as_deref(), |s| {
            if !s.starts_with("redis://") && !s.starts_with("rediss://") {
                Err(format!("必须以 redis:// 或 rediss:// 开头,实际: {s:?}"))
            } else {
                Ok(Secret::new(s.to_string()))
            }
        });

        let nats_url = require_secret(&mut issues, "IM_NATS_URL", raw.nats_url.as_deref(), |s| {
            if !s.starts_with("nats://") {
                Err(format!("必须以 nats:// 开头,实际: {s:?}"))
            } else {
                Ok(Secret::new(s.to_string()))
            }
        });

        let http_port = require_u16_in_range(
            &mut issues,
            "IM_HTTP_PORT",
            raw.http_port,
            Some(8080),
            1,
            65535,
        );

        let grpc_port = require_u16_in_range(
            &mut issues,
            "IM_GRPC_PORT",
            raw.grpc_port,
            Some(9000),
            1,
            65535,
        );

        // §A.2 — 可调 + 默认值
        // log_level 仅做 "看起来像 EnvFilter 表达式" 的轻校验 — 严格语义由 tracing_subscriber 启动时报告
        if let Some(lvl) = raw.log_level.as_deref() {
            if lvl.trim().is_empty() {
                issues.push(ConfigIssue {
                    field: "IM_LOG_LEVEL".into(),
                    message: "不能为空字符串".into(),
                });
            }
        }
        let log_level = raw
            .log_level
            .unwrap_or_else(|| "info,sqlx=warn".to_string());

        let ws_ping_interval_seconds = default_or_u32(&mut issues, "IM_WS_PING_INTERVAL_SECONDS",
            raw.ws_ping_interval_seconds, Some(30), |v| if v < 5 {
                Err(format!("必须 ≥ 5,实际 {v}"))
            } else {
                Ok(v)
            });

        let ws_heartbeat_timeout_seconds = default_or_u32(&mut issues, "IM_WS_HEARTBEAT_TIMEOUT_SECONDS",
            raw.ws_heartbeat_timeout_seconds, Some(60), |v| if v == 0 {
                Err("必须 > 0".into())
            } else {
                Ok(v)
            });

        let ws_auth_timeout_ms = default_or_u32(&mut issues, "IM_WS_AUTH_TIMEOUT_MS",
            raw.ws_auth_timeout_ms, Some(100), |v| if !(50..=5000).contains(&v) {
                Err(format!("必须在 50-5000 ms 之间,实际 {v}"))
            } else {
                Ok(v)
            });

        let rl_send = default_or_u32(&mut issues, "IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN",
            raw.rate_limit_send_message_per_min, Some(60), |v| if v < 1 {
                Err("必须 ≥ 1".into())
            } else {
                Ok(v)
            });

        let rl_guest = default_or_u32(&mut issues, "IM_RATE_LIMIT_GUEST_REGISTER_PER_HOUR",
            raw.rate_limit_guest_register_per_hour, Some(10), |v| if v < 1 {
                Err("必须 ≥ 1".into())
            } else {
                Ok(v)
            });

        let rl_token_exch = default_or_u32(&mut issues, "IM_RATE_LIMIT_TOKEN_EXCHANGE_PER_MIN",
            raw.rate_limit_token_exchange_per_min, Some(1000), |v| if v < 1 {
                Err("必须 ≥ 1".into())
            } else {
                Ok(v)
            });

        let rl_bucket = default_or_u32(&mut issues, "IM_RATE_LIMIT_BUCKET_SIZE",
            raw.rate_limit_bucket_size, Some(100), |v| if v < 1 {
                Err("必须 ≥ 1".into())
            } else {
                Ok(v)
            });

        let msg_max = default_or_u32(&mut issues, "IM_MESSAGE_MAX_SIZE_BYTES",
            raw.message_max_size_bytes, Some(65536), |v| {
                if !(1024..=1_048_576).contains(&v) {
                    Err(format!("必须在 1024-1048576 (1KB-1MB) 之间,实际 {v}"))
                } else {
                    Ok(v)
                }
            });

        let msg_recall = default_or_u32(&mut issues, "IM_MESSAGE_RECALL_WINDOW_SECONDS",
            raw.message_recall_window_seconds, Some(120), |v| if v < 1 {
                Err("必须 ≥ 1".into())
            } else {
                Ok(v)
            });

        let access_ttl = default_or_u32(&mut issues, "IM_ACCESS_TOKEN_TTL_SECONDS",
            raw.access_token_ttl_seconds, Some(900), |v| if v < 60 {
                Err("必须 ≥ 60 秒".into())
            } else {
                Ok(v)
            });

        let refresh_ttl = default_or_u32(&mut issues, "IM_REFRESH_TOKEN_TTL_SECONDS",
            raw.refresh_token_ttl_seconds, Some(2_592_000), |v| if v < 86_400 {
                Err("必须 ≥ 86400 秒 (1 天)".into())
            } else {
                Ok(v)
            });

        let db_pool_max = default_or_u32(&mut issues, "IM_DB_POOL_MAX_CONNECTIONS",
            raw.db_pool_max_connections, Some(20), |v| {
                if !(1..=200).contains(&v) {
                    Err(format!("必须在 1-200 之间,实际 {v}"))
                } else {
                    Ok(v)
                }
            });

        let auth_cache = default_or_u32(&mut issues, "IM_AUTH_MIDDLEWARE_CACHE_SECONDS",
            raw.auth_middleware_cache_seconds, Some(5), |v| if v > 60 {
                Err("必须在 0-60 之间".into())
            } else {
                Ok(v)
            });

        let guest_default_name = raw
            .guest_user_default_name
            .unwrap_or_else(|| "Guest".to_string());
        if guest_default_name.len() > 64 {
            issues.push(ConfigIssue {
                field: "IM_GUEST_USER_DEFAULT_NAME".into(),
                message: format!("长度 {}/64 超限", guest_default_name.len()),
            });
        }

        // §A.3 — 可观测
        let prometheus_bind = raw
            .prometheus_bind
            .filter(|s| !s.is_empty());
        let otel_otlp_endpoint = raw
            .otel_exporter_otlp_endpoint
            .filter(|s| !s.is_empty());
        let service_name = raw
            .service_name
            .unwrap_or_else(|| env!("CARGO_PKG_NAME").to_string());

        // §D 关联约束: heartbeat > ping_interval
        if let (Some(p), Some(h)) = (
            raw.ws_ping_interval_seconds,
            raw.ws_heartbeat_timeout_seconds,
        ) {
            if h <= p {
                issues.push(ConfigIssue {
                    field: "IM_WS_HEARTBEAT_TIMEOUT_SECONDS".into(),
                    message: format!(
                        "必须 > IM_WS_PING_INTERVAL_SECONDS (实际 heartbeat={h} ≤ ping={p})"
                    ),
                });
            }
        }

        // ----- step 7: 聚合 errors,若有则一次性返回 -----
        if !issues.is_empty() {
            return Err(ConfigError::Multi(issues));
        }

        // unwrap 安全:走到这里说明 issues 为空,所有 require_* 已就位
        Ok(AppConfig {
            env,
            database_url: database_url.expect("database_url required"),
            valkey_url: valkey_url.expect("valkey_url required"),
            nats_url: nats_url.expect("nats_url required"),
            http_port: http_port.expect("http_port required"),
            grpc_port: grpc_port.expect("grpc_port required"),
            jwt_signing_keys,
            server_secrets,
            refresh_token_pepper: refresh_token_pepper.expect("refresh_token_pepper required"),
            log_level,
            ws: WsConfig {
                ping_interval_seconds: ws_ping_interval_seconds.unwrap_or(30),
                heartbeat_timeout_seconds: ws_heartbeat_timeout_seconds.unwrap_or(60),
                auth_timeout_ms: ws_auth_timeout_ms.unwrap_or(100),
            },
            rate_limit: RateLimitConfig {
                send_message_per_min: rl_send.unwrap_or(60),
                guest_register_per_hour: rl_guest.unwrap_or(10),
                token_exchange_per_min: rl_token_exch.unwrap_or(1000),
                bucket_size: rl_bucket.unwrap_or(100),
            },
            message: MessageConfig {
                max_size_bytes: msg_max.unwrap_or(65536),
                recall_window_seconds: msg_recall.unwrap_or(120),
            },
            token: TokenConfig {
                access_ttl_seconds: access_ttl.unwrap_or(900),
                refresh_ttl_seconds: refresh_ttl.unwrap_or(2_592_000),
            },
            db_pool: DbPoolConfig {
                max_connections: db_pool_max.unwrap_or(20),
            },
            auth: AuthConfig {
                middleware_cache_seconds: auth_cache.unwrap_or(5),
            },
            guest_default_name,
            obs: ObsConfig {
                prometheus_bind,
                otel_otlp_endpoint,
                service_name,
            },
        })
    }

    /// 启动失败统一走 `process::exit(78)`(sysexists.h `EX_CONFIG`,per §D)
    ///
    /// `im-gateway` / `im-core` main() 第一行调用此函数,失败时日志已写,
    /// 进程退出码 78 让 K3s 重启 + CI 阻断。
    pub fn load_or_exit() -> Self {
        match Self::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!(error = %e, "AppConfig::load 失败,exit 78");
                eprintln!("FATAL: AppConfig::load 失败:\n{e}");
                std::process::exit(78);
            }
        }
    }
}

// ============================================================================
// 辅助:解析 + 校验
// ============================================================================

/// `IM_JWT_SIGNING_KEYS` = `[{"kid": "v1", "key": "<hex 64>"}, ...]`,至少 1 项
fn parse_jwt_signing_keys(s: &str) -> Result<Vec<SigningKey>, String> {
    #[derive(Deserialize)]
    struct RawKey {
        kid: String,
        key: String,
    }

    let arr: Vec<RawKey> = serde_json::from_str(s)
        .map_err(|e| format!("JSON 数组解析失败: {e}"))?;

    if arr.is_empty() {
        return Err("数组至少 1 项 (per §A.1 第 7)".into());
    }

    let mut out = Vec::with_capacity(arr.len());
    for (i, rk) in arr.into_iter().enumerate() {
        if rk.kid.is_empty() {
            return Err(format!("[{i}].kid 不能为空"));
        }
        let bytes = parse_hex_32(&rk.key).map_err(|e| format!("[{i}].key {e}"))?;
        out.push(SigningKey {
            kid: rk.kid,
            key: Secret::new(bytes),
        });
    }
    Ok(out)
}

/// `IM_SERVER_SECRETS` = `{"<env-uuid>": "<hex 64>", ...}`
fn parse_server_secrets(s: &str) -> Result<HashMap<Uuid, Secret<[u8; 32]>>, String> {
    #[derive(Deserialize)]
    struct RawMap(HashMap<String, String>);

    let raw: RawMap = serde_json::from_str(s)
        .map_err(|e| format!("JSON map 解析失败: {e}"))?;

    if raw.0.is_empty() {
        return Err("map 至少 1 项 (per §A.1 第 8)".into());
    }

    let mut out = HashMap::with_capacity(raw.0.len());
    for (k, v) in raw.0 {
        let uuid = Uuid::parse_str(&k)
            .map_err(|e| format!("key={k:?} 不是合法 UUID: {e}"))?;
        let bytes = parse_hex_32(&v).map_err(|e| format!("value (uuid={uuid}) {e}"))?;
        out.insert(uuid, Secret::new(bytes));
    }
    Ok(out)
}

/// 解析 hex(64) → 32 bytes。空白容忍,大小写不敏感。
fn parse_hex_32(s: &str) -> Result<[u8; 32], String> {
    let trimmed = s.trim();
    if trimmed.len() != 64 {
        return Err(format!(
            "hex 长度必须 64 (32 bytes),实际 {} (per §A.1 第 9 / §A.1 第 7)",
            trimmed.len()
        ));
    }
    let bytes = hex::decode(trimmed).map_err(|e| format!("hex 解码失败: {e}"))?;
    bytes
        .try_into()
        .map_err(|v: Vec<u8>| format!("hex 解码后字节数 {} != 32", v.len()))
}

/// 必填字符串 + 自定义校验
fn require_secret<T, F>(
    issues: &mut Vec<ConfigIssue>,
    field: &str,
    raw: Option<&str>,
    f: F,
) -> Option<T>
where
    F: FnOnce(&str) -> Result<T, String>,
{
    match raw {
        None | Some("") => {
            issues.push(ConfigIssue {
                field: field.into(),
                message: "必填,缺失或为空".into(),
            });
            None
        }
        Some(s) => match f(s) {
            Ok(v) => Some(v),
            Err(msg) => {
                issues.push(ConfigIssue {
                    field: field.into(),
                    message: msg,
                });
                None
            }
        },
    }
}

/// 可选 u32 + 默认值 + 范围校验
fn default_or_u32<F>(
    issues: &mut Vec<ConfigIssue>,
    field: &str,
    raw: Option<u32>,
    default: Option<u32>,
    f: F,
) -> Option<u32>
where
    F: FnOnce(u32) -> Result<u32, String>,
{
    match raw {
        None => default,
        Some(raw) => match f(raw) {
            Ok(v) => Some(v),
            Err(msg) => {
                issues.push(ConfigIssue {
                    field: field.into(),
                    message: msg,
                });
                None
            }
        },
    }
}

/// u16 + 1-65535 范围 + 可选默认(用于 port)
fn require_u16_in_range(
    issues: &mut Vec<ConfigIssue>,
    field: &str,
    raw: Option<u32>,
    default: Option<u16>,
    min: u32,
    max: u32,
) -> Option<u16> {
    let v = match raw {
        None => {
            // 端口若有 default 则不报错,缺失只是"用 default"
            if let Some(d) = default {
                return Some(d);
            }
            issues.push(ConfigIssue {
                field: field.into(),
                message: "必填且无默认值".into(),
            });
            return None;
        }
        Some(v) => v,
    };
    if v < min || v > max {
        issues.push(ConfigIssue {
            field: field.into(),
            message: format!("必须在 {min}-{max} 之间,实际 {v}"),
        });
        return None;
    }
    Some(v as u16)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 把测试相关 env 临时清空(避免被 CI shell 残留污染)
    fn clear_env() {
        for k in [
            "IM_ENV",
            "IM_DATABASE_URL",
            "IM_VALKEY_URL",
            "IM_NATS_URL",
            "IM_HTTP_PORT",
            "IM_GRPC_PORT",
            "IM_JWT_SIGNING_KEYS",
            "IM_SERVER_SECRETS",
            "IM_REFRESH_TOKEN_PEPPER",
            "IM_LOG_LEVEL",
            "IM_WS_PING_INTERVAL_SECONDS",
            "IM_WS_HEARTBEAT_TIMEOUT_SECONDS",
            "IM_WS_AUTH_TIMEOUT_MS",
            "IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN",
            "IM_RATE_LIMIT_GUEST_REGISTER_PER_HOUR",
            "IM_RATE_LIMIT_TOKEN_EXCHANGE_PER_MIN",
            "IM_RATE_LIMIT_BUCKET_SIZE",
            "IM_MESSAGE_MAX_SIZE_BYTES",
            "IM_MESSAGE_RECALL_WINDOW_SECONDS",
            "IM_ACCESS_TOKEN_TTL_SECONDS",
            "IM_REFRESH_TOKEN_TTL_SECONDS",
            "IM_DB_POOL_MAX_CONNECTIONS",
            "IM_AUTH_MIDDLEWARE_CACHE_SECONDS",
            "IM_GUEST_USER_DEFAULT_NAME",
            "IM_PROMETHEUS_BIND",
            "IM_OTEL_EXPORTER_OTLP_ENDPOINT",
            "IM_SERVICE_NAME",
        ] {
            std::env::remove_var(k);
        }
    }

    /// 注入 happy-path 默认值(全部 9 必填 + 全部 15 可调走默认)
    fn set_happy_env() {
        std::env::set_var("IM_ENV", "dev");
        std::env::set_var(
            "IM_DATABASE_URL",
            "postgres://user:pass@localhost:5432/im",
        );
        std::env::set_var("IM_VALKEY_URL", "redis://localhost:6379");
        std::env::set_var("IM_NATS_URL", "nats://localhost:4222");
        // http_port / grpc_port 走默认 8080 / 9000
        std::env::set_var(
            "IM_JWT_SIGNING_KEYS",
            r#"[{"kid":"v1","key":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}]"#,
        );
        let env_uuid = Uuid::new_v4();
        std::env::set_var(
            "IM_SERVER_SECRETS",
            format!(
                r#"{{"{}":"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"}}"#,
                env_uuid
            ),
        );
        std::env::set_var(
            "IM_REFRESH_TOKEN_PEPPER",
            "1111111111111111111111111111111111111111111111111111111111111111",
        );
    }

    // ---------- happy path ----------

    #[test]
    fn load_with_all_defaults_succeeds() {
        clear_env();
        set_happy_env();

        let cfg = AppConfig::load().expect("load should succeed");
        assert_eq!(cfg.env, Environment::Dev);
        assert_eq!(cfg.http_port, 8080);
        assert_eq!(cfg.grpc_port, 9000);
        assert_eq!(cfg.jwt_signing_keys.len(), 1);
        assert_eq!(cfg.jwt_signing_keys[0].kid, "v1");
        assert_eq!(cfg.jwt_signing_keys[0].key_bytes().len(), 32);
        assert_eq!(cfg.server_secrets.len(), 1);
        assert_eq!(cfg.refresh_token_pepper.expose_secret().len(), 32);
        assert_eq!(cfg.log_level, "info,sqlx=warn");
        assert_eq!(cfg.ws.ping_interval_seconds, 30);
        assert_eq!(cfg.ws.heartbeat_timeout_seconds, 60);
        assert_eq!(cfg.ws.auth_timeout_ms, 100);
        assert_eq!(cfg.rate_limit.send_message_per_min, 60);
        assert_eq!(cfg.rate_limit.guest_register_per_hour, 10);
        assert_eq!(cfg.rate_limit.token_exchange_per_min, 1000);
        assert_eq!(cfg.rate_limit.bucket_size, 100);
        assert_eq!(cfg.message.max_size_bytes, 65536);
        assert_eq!(cfg.message.recall_window_seconds, 120);
        assert_eq!(cfg.token.access_ttl_seconds, 900);
        assert_eq!(cfg.token.refresh_ttl_seconds, 2_592_000);
        assert_eq!(cfg.db_pool.max_connections, 20);
        assert_eq!(cfg.auth.middleware_cache_seconds, 5);
        assert_eq!(cfg.guest_default_name, "Guest");
        assert_eq!(cfg.obs.service_name, env!("CARGO_PKG_NAME"));
        assert!(cfg.obs.prometheus_bind.is_none());
        assert!(cfg.obs.otel_otlp_endpoint.is_none());
    }

    #[test]
    fn load_staging_env_parses() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_ENV", "staging");

        let cfg = AppConfig::load().expect("staging should parse");
        assert_eq!(cfg.env, Environment::Staging);
        assert!(!cfg.env.is_dev());
    }

    #[test]
    fn load_prod_env_parses() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_ENV", "prod");

        let cfg = AppConfig::load().expect("prod should parse");
        assert_eq!(cfg.env, Environment::Prod);
    }

    #[test]
    fn env_override_changes_defaults() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_HTTP_PORT", "9100");
        std::env::set_var("IM_WS_PING_INTERVAL_SECONDS", "10");
        std::env::set_var(
            "IM_MESSAGE_MAX_SIZE_BYTES",
            "262144", // 256KB
        );
        std::env::set_var("IM_PROMETHEUS_BIND", "0.0.0.0:9100");
        std::env::set_var("IM_SERVICE_NAME", "im-gateway");

        let cfg = AppConfig::load().expect("override load");
        assert_eq!(cfg.http_port, 9100);
        assert_eq!(cfg.ws.ping_interval_seconds, 10);
        assert_eq!(cfg.message.max_size_bytes, 262_144);
        assert_eq!(cfg.obs.prometheus_bind.as_deref(), Some("0.0.0.0:9100"));
        assert_eq!(cfg.obs.service_name, "im-gateway");
    }

    // ---------- 必填缺失(IM_ENV 单独 fail-fast,因为后续依赖其值)----------

    #[test]
    fn missing_im_env_is_reported() {
        clear_env();
        // 不设 IM_ENV
        let err = AppConfig::load().expect_err("should fail without IM_ENV");
        match err {
            ConfigError::Multi(issues) => {
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].field, "IM_ENV");
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn invalid_im_env_value_is_reported() {
        clear_env();
        std::env::set_var("IM_ENV", "production"); // 错拼
        let err = AppConfig::load().expect_err("should fail with invalid IM_ENV");
        match err {
            ConfigError::Multi(issues) => {
                assert!(issues.iter().any(|i| i.field == "IM_ENV"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    // ---------- §D 11 条校验规则 ----------

    #[test]
    fn missing_all_required_aggregates() {
        clear_env();
        std::env::set_var("IM_ENV", "dev");
        // 故意不设其余 8 必填
        let err = AppConfig::load().expect_err("should fail");
        match err {
            ConfigError::Multi(issues) => {
                // 至少 8 项必填违规:database_url / valkey_url / nats_url / jwt_signing_keys /
                // server_secrets / refresh_token_pepper  (http_port/grpc_port 走默认)
                let fields: Vec<&str> = issues.iter().map(|i| i.field.as_str()).collect();
                assert!(fields.contains(&"IM_DATABASE_URL"), "issues: {fields:?}");
                assert!(fields.contains(&"IM_VALKEY_URL"), "issues: {fields:?}");
                assert!(fields.contains(&"IM_NATS_URL"), "issues: {fields:?}");
                assert!(fields.contains(&"IM_JWT_SIGNING_KEYS"), "issues: {fields:?}");
                assert!(
                    fields.contains(&"IM_SERVER_SECRETS"),
                    "issues: {fields:?}"
                );
                assert!(
                    fields.contains(&"IM_REFRESH_TOKEN_PEPPER"),
                    "issues: {fields:?}"
                );
                assert!(issues.len() >= 6);
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn http_port_out_of_range_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_HTTP_PORT", "70000"); // > 65535
        let err = AppConfig::load().expect_err("port 70000 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(issues.iter().any(|i| i.field == "IM_HTTP_PORT"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn http_port_zero_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_HTTP_PORT", "0");
        let err = AppConfig::load().expect_err("port 0 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(issues.iter().any(|i| i.field == "IM_HTTP_PORT"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn grpc_port_out_of_range_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_GRPC_PORT", "99999");
        let err = AppConfig::load().expect_err("grpc port invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(issues.iter().any(|i| i.field == "IM_GRPC_PORT"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn valkey_url_wrong_scheme_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_VALKEY_URL", "http://localhost:6379");
        let err = AppConfig::load().expect_err("wrong valkey scheme");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_VALKEY_URL"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn nats_url_wrong_scheme_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_NATS_URL", "redis://localhost:6379");
        let err = AppConfig::load().expect_err("wrong nats scheme");
        match err {
            ConfigError::Multi(issues) => {
                assert!(issues.iter().any(|i| i.field == "IM_NATS_URL"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn message_max_size_too_small_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_MESSAGE_MAX_SIZE_BYTES", "100"); // < 1024
        let err = AppConfig::load().expect_err("too small max size");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_MESSAGE_MAX_SIZE_BYTES"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn message_max_size_too_large_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_MESSAGE_MAX_SIZE_BYTES", "2097152"); // > 1MB
        let err = AppConfig::load().expect_err("too large max size");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_MESSAGE_MAX_SIZE_BYTES"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn db_pool_max_connections_out_of_range_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_DB_POOL_MAX_CONNECTIONS", "0");
        let err = AppConfig::load().expect_err("pool=0 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_DB_POOL_MAX_CONNECTIONS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn db_pool_max_connections_above_200_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_DB_POOL_MAX_CONNECTIONS", "500");
        let err = AppConfig::load().expect_err("pool=500 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_DB_POOL_MAX_CONNECTIONS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn rate_limit_zero_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN", "0");
        let err = AppConfig::load().expect_err("rl=0 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn access_token_ttl_below_60_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_ACCESS_TOKEN_TTL_SECONDS", "30");
        let err = AppConfig::load().expect_err("ttl=30 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_ACCESS_TOKEN_TTL_SECONDS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn refresh_token_ttl_below_one_day_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_REFRESH_TOKEN_TTL_SECONDS", "3600");
        let err = AppConfig::load().expect_err("refresh ttl=3600 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_REFRESH_TOKEN_TTL_SECONDS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn heartbeat_must_exceed_ping() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_WS_PING_INTERVAL_SECONDS", "60");
        std::env::set_var("IM_WS_HEARTBEAT_TIMEOUT_SECONDS", "60"); // 等于,不违反
        let err = AppConfig::load().expect_err("heartbeat == ping invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues
                        .iter()
                        .any(|i| i.field == "IM_WS_HEARTBEAT_TIMEOUT_SECONDS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }

        clear_env();
        set_happy_env();
        std::env::set_var("IM_WS_PING_INTERVAL_SECONDS", "60");
        std::env::set_var("IM_WS_HEARTBEAT_TIMEOUT_SECONDS", "30"); // 小于
        let err = AppConfig::load().expect_err("heartbeat < ping invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(issues
                    .iter()
                    .any(|i| i.field == "IM_WS_HEARTBEAT_TIMEOUT_SECONDS"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn ws_ping_interval_below_5_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_WS_PING_INTERVAL_SECONDS", "4");
        let err = AppConfig::load().expect_err("ping<5 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_WS_PING_INTERVAL_SECONDS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn ws_auth_timeout_out_of_range_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_WS_AUTH_TIMEOUT_MS", "10"); // < 50
        let err = AppConfig::load().expect_err("auth timeout < 50 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_WS_AUTH_TIMEOUT_MS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn auth_middleware_cache_above_60_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_AUTH_MIDDLEWARE_CACHE_SECONDS", "120");
        let err = AppConfig::load().expect_err("cache>60 invalid");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues
                        .iter()
                        .any(|i| i.field == "IM_AUTH_MIDDLEWARE_CACHE_SECONDS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn guest_default_name_too_long_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var(
            "IM_GUEST_USER_DEFAULT_NAME",
            "a".repeat(65).as_str(),
        );
        let err = AppConfig::load().expect_err("guest name too long");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_GUEST_USER_DEFAULT_NAME"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    // ---------- §A.1 第 7-9 hex + JSON 解析 ----------

    #[test]
    fn jwt_signing_keys_must_be_valid_json_array() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_JWT_SIGNING_KEYS", "not json");
        let err = AppConfig::load().expect_err("jwt keys bad json");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_JWT_SIGNING_KEYS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn jwt_signing_keys_empty_array_is_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_JWT_SIGNING_KEYS", "[]");
        let err = AppConfig::load().expect_err("empty jwt keys");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_JWT_SIGNING_KEYS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn jwt_signing_keys_wrong_hex_length_is_rejected() {
        clear_env();
        set_happy_env();
        // 4 bytes hex, not 32
        std::env::set_var(
            "IM_JWT_SIGNING_KEYS",
            r#"[{"kid":"v1","key":"deadbeef"}]"#,
        );
        let err = AppConfig::load().expect_err("bad hex length");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_JWT_SIGNING_KEYS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn jwt_signing_keys_dual_key_rotation_supported() {
        clear_env();
        set_happy_env();
        std::env::set_var(
            "IM_JWT_SIGNING_KEYS",
            r#"[
                {"kid":"v1","key":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
                {"kid":"v2","key":"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"}
            ]"#,
        );
        let cfg = AppConfig::load().expect("dual keys ok");
        assert_eq!(cfg.jwt_signing_keys.len(), 2);
        assert_eq!(cfg.jwt_signing_keys[0].kid, "v1");
        assert_eq!(cfg.jwt_signing_keys[1].kid, "v2");
    }

    #[test]
    fn server_secrets_must_be_valid_json_map() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_SERVER_SECRETS", "[1,2,3]"); // 数组不是 map
        let err = AppConfig::load().expect_err("server secrets bad json");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_SERVER_SECRETS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn server_secrets_uuid_key_must_be_valid() {
        clear_env();
        set_happy_env();
        std::env::set_var(
            "IM_SERVER_SECRETS",
            r#"{"not-a-uuid":"fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"}"#,
        );
        let err = AppConfig::load().expect_err("bad uuid key");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_SERVER_SECRETS"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn refresh_token_pepper_must_be_hex_64() {
        clear_env();
        set_happy_env();
        std::env::set_var("IM_REFRESH_TOKEN_PEPPER", "tooshort");
        let err = AppConfig::load().expect_err("pepper too short");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_REFRESH_TOKEN_PEPPER"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    #[test]
    fn refresh_token_pepper_invalid_hex_chars_rejected() {
        clear_env();
        set_happy_env();
        std::env::set_var(
            "IM_REFRESH_TOKEN_PEPPER",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        );
        let err = AppConfig::load().expect_err("pepper invalid hex");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.iter().any(|i| i.field == "IM_REFRESH_TOKEN_PEPPER"),
                    "issues: {:?}",
                    issues
                );
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    // ---------- 错误聚合 -- 多条错误一次性返回 ----------

    #[test]
    fn multiple_validation_errors_are_aggregated() {
        clear_env();
        std::env::set_var("IM_ENV", "dev");
        std::env::set_var("IM_HTTP_PORT", "99999"); // 越界
        std::env::set_var("IM_MESSAGE_MAX_SIZE_BYTES", "100"); // 越界
        std::env::set_var("IM_DB_POOL_MAX_CONNECTIONS", "999"); // 越界
        // 缺 N 项必填
        let err = AppConfig::load().expect_err("should aggregate");
        match err {
            ConfigError::Multi(issues) => {
                assert!(
                    issues.len() >= 4,
                    "应该至少聚合 4 项,实际 {}: {:?}",
                    issues.len(),
                    issues
                );
                let fields: Vec<&str> = issues.iter().map(|i| i.field.as_str()).collect();
                assert!(fields.contains(&"IM_HTTP_PORT"));
                assert!(fields.contains(&"IM_MESSAGE_MAX_SIZE_BYTES"));
                assert!(fields.contains(&"IM_DB_POOL_MAX_CONNECTIONS"));
                assert!(fields.contains(&"IM_DATABASE_URL"));
            }
            other => panic!("expected Multi, got {other:?}"),
        }
    }

    // ---------- Display 实现 ----------

    #[test]
    fn config_error_display_lists_all_issues() {
        clear_env();
        std::env::set_var("IM_ENV", "dev");
        std::env::set_var("IM_HTTP_PORT", "99999");
        let err = AppConfig::load().expect_err("should fail");
        let s = format!("{err}");
        assert!(s.contains("AppConfig 校验失败"));
        assert!(s.contains("IM_HTTP_PORT"));
    }

    // ---------- Environment 派生 ----------

    #[test]
    fn environment_display_and_fromstr_roundtrip() {
        for env in [Environment::Dev, Environment::Staging, Environment::Prod] {
            let s = env.to_string();
            let parsed: Environment = s.parse().expect("roundtrip");
            assert_eq!(parsed, env);
        }
        assert!(Environment::Dev.is_dev());
        assert!(!Environment::Prod.is_dev());
        assert_eq!(Environment::ALL.len(), 3);
    }

    // ---------- 单元测试:parse_hex_32 ----------

    #[test]
    fn parse_hex_32_accepts_lowercase_and_uppercase() {
        let v = parse_hex_32(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("lower ok");
        assert_eq!(v.len(), 32);

        let v = parse_hex_32(
            "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
        )
        .expect("upper ok");
        assert_eq!(v.len(), 32);
    }

    #[test]
    fn parse_hex_32_rejects_wrong_length() {
        assert!(parse_hex_32("abcd").is_err());
        assert!(parse_hex_32(&"a".repeat(63)).is_err());
        assert!(parse_hex_32(&"a".repeat(65)).is_err());
    }

    #[test]
    fn parse_hex_32_rejects_invalid_chars() {
        assert!(parse_hex_32(&"z".repeat(64)).is_err());
    }
}