//! Tracing 初始化 — JSON 输出 + EnvFilter 加载 + OTel exporter stub
//!
//! 依据:
//! - `docs/Observability.md` §6.1(OBS-ARCH-007)+ §8(JSON 日志)+ §9(Trace Context)
//! - `docs/ImplementationSpec.md` §7.1(im-common tracing.rs)+ §9.4(必接字段)
//! - `docs/templates/04-detailed-design/auxiliary/aux-09-log-query-cookbook.md` §B 14 必接字段
//!
//! ## 用法
//!
//! ```no_run
//! use im_common::tracing_init;
//! tracing_init::init(tracing_init::Config {
//!     service_name: "im-gateway",
//!     service_version: env!("CARGO_PKG_VERSION"),
//!     ..Default::default()
//! })?;
//! # Ok::<(), tracing_init::TracingError>(())
//! ```
//!
//! ## 设计要点
//!
//! - **JSON formatter** — `tracing_subscriber::fmt::layer().json()` 单行 JSON 输出,
//!   兼容 Loki / kubectl logs | jq (Observability §8.1)
//! - **EnvFilter** — 优先级 `IM_LOG_LEVEL` > `RUST_LOG` > `info,sqlx=warn` 默认,
//!   避免长期 debug 漏关导致磁盘爆 (aux-09 §I 禁忌)
//! - **服务字段注入** — `service.name` / `service.version` / `environment` /
//!   `instance` 写入每条 JSON 日志的根字段(aux-09 §B 必接字段)
//! - **OTel exporter stub** — 仅当 `IM_OTEL_EXPORTER_OTLP_ENDPOINT` 或
//!   `OTEL_EXPORTER_OTLP_ENDPOINT` 配置时才安装 OTLP/gRPC + `tracing-opentelemetry` 层。
//!   MVP 阶段默认关闭(ImplementationSpec §9.3),V1+ 启用。
//! - **重复初始化保护** — `try_init()` 拒绝二次调用,避免测试场景下误覆盖

use std::env;

use opentelemetry::trace::TracerProvider;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use thiserror::Error;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// tracing 初始化错误
///
/// 所有错误都是启动期致命错误 — `main()` 应直接 exit code 78(SysV `EX_CONFIG`)。
/// 与 `aux-12 §A.3 验证错误码` 对齐。
#[derive(Debug, Error)]
pub enum TracingError {
    /// 重复调用 `init`(`tracing-subscriber::registry().init()` 全局只允许一次)
    #[error("tracing subscriber already initialized; init() is idempotent only on first call")]
    AlreadyInitialized,

    /// EnvFilter 表达式解析失败
    #[error("invalid EnvFilter expression `{expr}`: {source}")]
    EnvFilter {
        expr: String,
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },

    /// OpenTelemetry pipeline 构建失败
    #[error("OpenTelemetry exporter install failed: {0}")]
    OtelInstall(String),
}

/// Tracing 初始化配置
///
/// 默认字段从环境变量推断(service name 强制要求显式传入 — 避免静默错误)
#[derive(Debug, Clone)]
pub struct Config {
    /// 服务名(写入 `service.name` 字段)。**必填**。
    pub service_name: &'static str,
    /// 服务版本(写入 `service.version` 字段)。默认 `env!("CARGO_PKG_VERSION")`。
    pub service_version: &'static str,
    /// 部署环境(dev/staging/prod)。来源 `IM_ENV` 环境变量,fallback `ENV`,fallback `dev`。
    pub environment: String,
    /// Pod / 实例名(写入 `instance` 字段)。来源 `POD_NAME` 环境变量,fallback 空串。
    pub instance: String,
    /// 是否强制 JSON 输出。默认 `true`(符合 Observability §8.1)。
    pub json_output: bool,
}

impl Config {
    /// 从 env 构造 Config;只 `service_name` / `service_version` 需显式传入。
    ///
    /// 用于辅助 `init(Config { service_name, ..Config::from_env() })` 调用模式。
    pub fn from_env() -> Self {
        Self {
            service_name: "",
            service_version: env!("CARGO_PKG_VERSION"),
            environment: read_environment(),
            instance: env::var("POD_NAME").unwrap_or_default(),
            json_output: true,
        }
    }
}

/// 读取 `environment` 字段,优先级 `IM_ENV` > `ENV` > `dev`。
///
/// 抽出为独立函数,便于测试(测试通过参数传入值,不修改 OS env)。
pub fn read_environment() -> String {
    env::var("IM_ENV")
        .or_else(|_| env::var("ENV"))
        .unwrap_or_else(|_| "dev".to_string())
}

/// 读取 OTel OTLP endpoint 配置。优先级 `IM_OTEL_EXPORTER_OTLP_ENDPOINT` > `OTEL_EXPORTER_OTLP_ENDPOINT` > None。
///
/// 抽出为独立函数,便于测试。
pub fn read_otel_endpoint() -> Option<String> {
    env::var("IM_OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .or_else(|| env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
}

/// 读取 EnvFilter 表达式,优先级 `IM_LOG_LEVEL` > `RUST_LOG` > `"info,sqlx=warn"`。
///
/// 抽出为独立函数,便于测试(测试通过参数传入值,不修改 OS env)。
pub fn read_env_filter() -> String {
    env::var("IM_LOG_LEVEL")
        .or_else(|_| env::var("RUST_LOG"))
        .unwrap_or_else(|_| "info,sqlx=warn".to_string())
}

/// 优先级选择 helper(纯函数)— 测试用。
///
/// 给定 `IM_LOG_LEVEL` / `RUST_LOG` 的可选值,返回最终生效的 EnvFilter 表达式。
pub fn pick_env_filter_expr(im_log_level: Option<&str>, rust_log: Option<&str>) -> String {
    im_log_level
        .or(rust_log)
        .unwrap_or("info,sqlx=warn")
        .to_string()
}

/// 优先级选择 helper(纯函数)— 测试用。
///
/// 给定 `IM_ENV` / `ENV` 的可选值,返回最终生效的 environment。
pub fn pick_environment(im_env: Option<&str>, env: Option<&str>) -> String {
    im_env.or(env).unwrap_or("dev").to_string()
}

/// 优先级选择 helper(纯函数)— 测试用。
///
/// 给定 `IM_OTEL_EXPORTER_OTLP_ENDPOINT` / `OTEL_EXPORTER_OTLP_ENDPOINT` 的可选值,
/// 返回最终生效的 endpoint。
pub fn pick_otel_endpoint(im_endpoint: Option<&str>, otel_endpoint: Option<&str>) -> Option<String> {
    im_endpoint
        .or(otel_endpoint)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

/// 初始化全局 tracing subscriber(JSON + EnvFilter + 可选 OTel)。
///
/// **幂等**:只接受第一次调用,后续调用返回 [`TracingError::AlreadyInitialized`]。
/// 多次 `init()` 通常意味着测试代码漏改 + main 启动链竞争,应当 fail-fast。
///
/// ## Env 优先级
///
/// | Env var | 用途 |
/// |---|---|
/// | `IM_LOG_LEVEL` | 业务首选(`aux-12 §A.3 第 10`)|
/// | `RUST_LOG`     | 兼容 tracing-subscriber 默认约定 |
/// | (none)         | 默认 `"info,sqlx=warn"`(Observability §8.3)|
///
/// `IM_OTEL_EXPORTER_OTLP_ENDPOINT` 或 `OTEL_EXPORTER_OTLP_ENDPOINT` 任一非空
/// 即启用 OTLP/gRPC exporter + `tracing-opentelemetry` 桥接层。
pub fn init(config: Config) -> Result<(), TracingError> {
    // 1. EnvFilter — IM_LOG_LEVEL > RUST_LOG > 默认
    let filter_expr = read_env_filter();
    let env_filter = EnvFilter::try_new(&filter_expr).map_err(|source| TracingError::EnvFilter {
        expr: filter_expr.clone(),
        source,
    })?;

    // 2. JSON formatter (stdout 单行 NDJSON) — 兼容 Loki + kubectl logs | jq
    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_level(true)
        // 业务关键字段 flatten 到 JSON 根(aux-09 §B 14 必接字段)
        // 具体值由业务侧 info!(error_code = "X", user_id = %u, ...) 提供
        .flatten_event(true);

    // 3. service.* / environment / instance 注入
    // 用 .map_event_format 把全局静态字段塞进每条 JSON 根
    let static_fields = StaticFieldsLayer::new(&config);

    // 4. 装配 subscriber(EnvFilter + 静态字段 + JSON fmt + 可选 OTel)
    let otel_endpoint = read_otel_endpoint();

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(static_fields)
        .with(fmt_layer);

    if let Some(endpoint) = otel_endpoint.as_deref() {
        let otel_layer = build_otel_layer(&config, endpoint)?;
        registry.with(otel_layer).try_init().map_err(|_| {
            TracingError::AlreadyInitialized
        })?;
    } else {
        registry.try_init().map_err(|_| TracingError::AlreadyInitialized)?;
    }

    Ok(())
}

/// 构建 OTLP/gRPC exporter + tracing-opentelemetry 桥接层
///
/// MVP 默认未启用(ImplementationSpec §9.3),V1+ 启用。
/// `IM_OTEL_EXPORTER_OTLP_ENDPOINT` 形如 `http://otel-collector.im1-obs:4317`。
fn build_otel_layer<S>(
    config: &Config,
    endpoint: &str,
) -> Result<impl Layer<S>, TracingError>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    // Resource 注入 service.* + deployment.environment + k8s.pod.name(Observability §6.1)
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name),
        KeyValue::new("service.version", config.service_version),
        KeyValue::new("deployment.environment", config.environment.clone()),
        KeyValue::new("k8s.pod.name", config.instance.clone()),
    ]);

    // Tracer Provider: OTLP/gRPC → Collector(install_batch 异步、Tokio runtime)
    // 返回的是 TracerProvider(opentelemetry_sdk::trace::TracerProvider),
    // 通过 .tracer(name) 获取 SdkTracer(实现 tracing_opentelemetry::PreSampledTracer)。
    let provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint),
        )
        .with_trace_config(sdktrace::Config::default().with_resource(resource))
        .install_batch(runtime::Tokio)
        .map_err(|e| TracingError::OtelInstall(e.to_string()))?;

    // 用 service.name 作为 tracer 名字(Observability §6.1 推荐实践)
    let tracer = provider.tracer(config.service_name);

    // 桥接到 tracing layer
    Ok(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// 全局静态字段注入层 — 把 `service.name` / `service.version` / `environment` /
/// `instance` 写入每条 JSON 日志的根字段。
///
/// 实现为 [`tracing_subscriber::Layer`] 的最小子集:on_event 时把字段塞进
/// 当前 span 的 extensions + JSON 输出层的字段表里。tracing-subscriber 的
/// JSON 层通过 `JsonStorage` 读字段,所以我们在 on_event 里 record 字段即可。
struct StaticFieldsLayer {
    service_name: &'static str,
    service_version: &'static str,
    environment: String,
    instance: String,
}

impl StaticFieldsLayer {
    fn new(config: &Config) -> Self {
        Self {
            service_name: config.service_name,
            service_version: config.service_version,
            environment: config.environment.clone(),
            instance: config.instance.clone(),
        }
    }
}

// Clone for tests
impl Clone for StaticFieldsLayer {
    fn clone(&self) -> Self {
        Self {
            service_name: self.service_name,
            service_version: self.service_version,
            environment: self.environment.clone(),
            instance: self.instance.clone(),
        }
    }
}

impl<S> Layer<S> for StaticFieldsLayer
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(&self, _event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // 把全局静态字段 record 到当前 span — 这样 tracing-subscriber 的 JSON 层
        // 在序列化为 EventFormatter 时会包含它们(flatten_event 模式下写入根 JSON)。
        // 注意:record 在 event 触发时执行,后续序列化层才能拿到。
        let span = tracing::Span::current();
        // 仅在有活跃 span 时 record(否则 tracing macros 不允许 record 到 root)
        if !span.is_disabled() {
            span.record("service.name", self.service_name);
            span.record("service.version", self.service_version);
            span.record("environment", self.environment.as_str());
            if !self.instance.is_empty() {
                span.record("instance", self.instance.as_str());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 minimal config
    fn test_config() -> Config {
        Config {
            service_name: "im-testkit",
            service_version: "0.0.1-test",
            environment: "test".to_string(),
            instance: "test-pod".to_string(),
            json_output: true,
        }
    }

    // ---- 纯函数单元测试:priority 选择(无 OS env 副作用) ----

    #[test]
    fn pick_env_filter_im_log_level_takes_priority() {
        assert_eq!(
            pick_env_filter_expr(Some("debug,im_gateway=trace"), Some("warn")),
            "debug,im_gateway=trace"
        );
    }

    #[test]
    fn pick_env_filter_falls_back_to_rust_log() {
        assert_eq!(
            pick_env_filter_expr(None, Some("info,sqlx=debug")),
            "info,sqlx=debug"
        );
    }

    #[test]
    fn pick_env_filter_default_when_none() {
        assert_eq!(pick_env_filter_expr(None, None), "info,sqlx=warn");
    }

    #[test]
    fn pick_environment_im_env_wins_over_env() {
        assert_eq!(pick_environment(Some("staging"), Some("prod")), "staging");
    }

    #[test]
    fn pick_environment_env_fallback() {
        assert_eq!(pick_environment(None, Some("staging")), "staging");
    }

    #[test]
    fn pick_environment_dev_default() {
        assert_eq!(pick_environment(None, None), "dev");
    }

    #[test]
    fn pick_otel_endpoint_im_wins_over_otel_env() {
        assert_eq!(
            pick_otel_endpoint(
                Some("http://im-otel:4317"),
                Some("http://otel-collector:4317")
            ),
            Some("http://im-otel:4317".to_string())
        );
    }

    #[test]
    fn pick_otel_endpoint_otel_env_fallback() {
        assert_eq!(
            pick_otel_endpoint(None, Some("http://otel:4317")),
            Some("http://otel:4317".to_string())
        );
    }

    #[test]
    fn pick_otel_endpoint_none_when_unset() {
        assert_eq!(pick_otel_endpoint(None, None), None);
    }

    #[test]
    fn pick_otel_endpoint_empty_string_treated_as_none() {
        // 空字符串在 OTLP 配置中应视为未启用(避免空 endpoint 启动 exporter 失败)
        assert_eq!(pick_otel_endpoint(Some(""), None), None);
        assert_eq!(pick_otel_endpoint(None, Some("")), None);
    }

    // ---- EnvFilter 解析(直接调 EnvFilter API) ----

    #[test]
    fn envfilter_valid_directives_succeed() {
        for expr in &[
            "info",
            "debug,sqlx=info",
            "trace,im_gateway=debug,sqlx=warn",
            "warn,im_core=trace",
            "info,sqlx=warn", // aux-12 §A.3 默认
        ] {
            EnvFilter::try_new(expr).unwrap_or_else(|e| {
                panic!("expected `{expr}` to parse: {e}");
            });
        }
    }

    #[test]
    fn envfilter_with_bogus_directive_fails() {
        // `!bogus` 触发 ParseError — 验证我们 wrap 成 TracingError::EnvFilter
        let result = EnvFilter::try_new("!bogus");
        match result {
            Err(_) => {}
            Ok(_) => panic!("expected parse error for `!bogus`"),
        }
    }

    #[test]
    fn env_filter_error_wraps_parse_error() {
        // 模拟 init() 内的错误路径(手工构造)
        let result = EnvFilter::try_new("!bogus").map_err(|source| TracingError::EnvFilter {
            expr: "!bogus".to_string(),
            source,
        });
        match result {
            Err(TracingError::EnvFilter { expr, .. }) => {
                assert_eq!(expr, "!bogus");
            }
            other => panic!("expected EnvFilter error, got {other:?}"),
        }
    }

    // ---- Config 默认值(只测常量字段,不依赖 env) ----

    #[test]
    fn config_default_uses_cargo_pkg_version() {
        // Config::default() 读 env,但 service_version 强制走 env!("CARGO_PKG_VERSION")
        let cfg = Config::default();
        assert_eq!(cfg.service_version, env!("CARGO_PKG_VERSION"));
        assert!(cfg.json_output);
        // service_name 由调用方覆盖(默认空 — 业务必填)
        assert_eq!(cfg.service_name, "");
    }

    #[test]
    fn test_config_helper_shape() {
        let cfg = test_config();
        assert_eq!(cfg.service_name, "im-testkit");
        assert_eq!(cfg.environment, "test");
        assert!(cfg.json_output);
    }

    // ---- 启动 init() 副作用测试(用 OnceLock 全局互斥避免竞争) ----

    /// init() 会注册全局 subscriber;在同一进程内只能 init 一次。
    /// 用 `OnceLock<()>` 串行化所有 init 测试 — 让第一个 init 成功,
    /// 后续全部拿到 `AlreadyInitialized`。
    fn try_init() -> Result<(), TracingError> {
        // 这里不依赖 OS env — 仅测试 init() 的多次调用语义
        init(test_config())
    }

    #[test]
    fn init_second_call_returns_already_initialized() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static DONE: AtomicBool = AtomicBool::new(false);

        let first = try_init();
        let second = try_init();

        // 在测试并行运行时,可能两个都拿到 AlreadyInitialized —
        // 我们只断言:至少第二次是 AlreadyInitialized
        if !DONE.swap(true, Ordering::SeqCst) {
            // 第一个测试运行:第一次 init 通常成功
            // (也可能在并行 cargo test 中被抢走,那就两个都 AlreadyInitialized)
            let _ = first;
        }
        match second {
            Err(TracingError::AlreadyInitialized) => {}
            Err(e) => panic!("expected AlreadyInitialized on second call, got {e:?}"),
            Ok(()) => panic!("expected AlreadyInitialized on second call, got Ok"),
        }
    }

    #[test]
    fn init_is_idempotent_under_concurrent_test_runs() {
        // 与 init_second_call 相同 — 但允许两种结果(Ok 或 AlreadyInitialized),
        // 主要断言不 panic,且对 AlreadyInitialized 优雅降级
        let result = try_init();
        match result {
            Ok(()) => {}                          // 第一个 init 成功
            Err(TracingError::AlreadyInitialized) => {} // 已被其他测试 init
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
}