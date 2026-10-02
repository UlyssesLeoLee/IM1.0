//! 生产配置路径的端到端证明 (2026-10-03 新增)
//!
//! ## 这条测试存在的理由
//!
//! 2026-10-03 之前, `config.rs` 的测试**只覆盖「必填字段缺失 → Err」**。
//! 也就是说: 「四个必填环境变量都给了, 配置能不能真的加载出来」这件事
//! **从未被任何测试验证过**。而 `AppConfig::load()` 恰恰是 `main.rs` 调用的
//! 那个入口, 它传 `config_dir = None` —— **生产环境不读任何 TOML 文件**,
//! 配置的唯一来源就是环境变量。
//!
//! 实测结果: 加载**失败**, 报
//! `invalid type: found string "v1:somekey", expected a sequence for key
//! "default.jwt_signing_keys"`。即 `jwt_signing_keys: Vec<SigningKeyConfig>`
//! 这个**必填**字段结构上无法从扁平环境变量字符串喂进去 —— 于是
//! **im-gateway 在容器里必然启动失败**。这就是 `deploy/k3s/dev/` 清单
//! 从未端到端跑过的根因: 它不是「还没跑」, 是「跑必然红」。
//!
//! 修复见 `env_value_to_toml` 的文档: `[` / `{` 开头的值原样透传。
//! 本测试即该修复的护栏 —— 若哪天有人「顺手简化」掉透传分支, 这里立刻变红。

use im_common::config::AppConfig;

/// 设置一组环境变量并在结束后恢复原值。
///
/// 用 `Mutex` 串行化: 这些测试改的是**进程级**环境变量, 同进程内并行跑会
/// 互相污染(env 在多数平台上是进程全局的, 且测试线程共享它)。
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(pairs: &[(&str, &str)]) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved = pairs
            .iter()
            .map(|(k, _)| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in pairs {
            std::env::set_var(k, v);
        }
        Self { saved, _lock: lock }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in std::mem::take(&mut self.saved) {
            match v {
                Some(v) => std::env::set_var(&k, v),
                None => std::env::remove_var(&k),
            }
        }
    }
}

/// 容器里实际会给的四个必填变量(名字取自 `AppConfig` 的字段名, 非清单里的写法)
#[test]
fn env_only_config_loads_with_structured_fields_as_json() {
    let _g = EnvGuard::set(&[
        ("IM_HTTP_PORT", "8080"),
        ("IM_POSTGRES_URL", "postgres://im:im@postgres:5432/im"),
        (
            "IM_JWT_SIGNING_KEYS",
            r#"[{"kid":"v1","key":"k1","active":true},{"kid":"v2","key":"k2","active":false}]"#,
        ),
        ("IM_REFRESH_PEPPER", "pepper-value"),
        (
            "IM_EVENT_PUBLISHER",
            r#"{"kind":"nats","nats_url":"nats://nats:4222"}"#,
        ),
    ]);

    let cfg =
        AppConfig::load_from_paths(None, None).expect("仅环境变量(无 TOML)必须能加载出完整配置");

    assert_eq!(cfg.http_port, 8080);
    assert_eq!(cfg.postgres_url, "postgres://im:im@postgres:5432/im");
    assert_eq!(cfg.refresh_pepper_plain(), "pepper-value");

    // Vec 字段: 两个 key 都进来了, 且 active 标志被正确解析
    assert_eq!(cfg.jwt_signing_keys.len(), 2, "双密钥都要加载");
    assert_eq!(cfg.jwt_signing_keys[0].kid, "v1");
    assert!(cfg.jwt_signing_keys[0].active);
    assert!(!cfg.jwt_signing_keys[1].active, "active=false 必须被尊重");
    // 派生方法走的是 active 过滤, 顺带证明签名密钥选择可用
    assert_eq!(cfg.active_signing_keys().len(), 1);

    // 嵌套 struct 字段: 不需要 IM__ 嵌套命名法
    assert_eq!(
        cfg.event_publisher.nats_url.as_deref(),
        Some("nats://nats:4222")
    );
}

/// `server_secrets` 是 `HashMap<EnvironmentId, Secret<String>>` —— 同样只能靠
/// JSON 透传。HMAC 验签(`IdentityService::verify_server_signature`)依赖它,
/// 所以它在容器里配不上 = S2S token exchange 在容器里必然不可用。
#[test]
fn env_only_config_loads_server_secrets_map() {
    let env_uuid = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
    let _g = EnvGuard::set(&[
        ("IM_HTTP_PORT", "8080"),
        ("IM_POSTGRES_URL", "postgres://im:im@postgres:5432/im"),
        ("IM_JWT_SIGNING_KEYS", r#"[{"kid":"v1","key":"k1"}]"#),
        ("IM_REFRESH_PEPPER", "pepper-value"),
        (
            "IM_SERVER_SECRETS",
            &format!("{{\"{env_uuid}\":\"s2e-secret\"}}"),
        ),
        ("IM_EVENT_PUBLISHER", r#"{"kind":"stub"}"#),
    ]);

    let cfg = AppConfig::load_from_paths(None, None).expect("server_secrets 也要能从 env 配");
    let env_id: im_common::ids::EnvironmentId = env_uuid.parse().expect("uuid");
    assert_eq!(
        cfg.server_secret(&env_id),
        Some("s2e-secret"),
        "按 env_id 取 server secret 必须拿得到(HMAC 验签依赖)"
    );
}

/// 回归锁: 标量走原来的类型推断, **没有**因为加了 JSON 透传而改变。
#[test]
fn scalar_env_values_keep_their_inferred_types() {
    let _g = EnvGuard::set(&[
        ("IM_HTTP_PORT", "9999"),
        ("IM_POSTGRES_URL", "postgres://test:***@localhost/test"),
        ("IM_JWT_SIGNING_KEYS", r#"[{"kid":"v1","key":"k1"}]"#),
        ("IM_REFRESH_PEPPER", "test-pepper"),
        ("IM_REFRESH_TOKEN_TTL_SECONDS", "12345"),
        ("IM_MAX_MESSAGE_SIZE_BYTES", "2048"),
        ("IM_EVENT_PUBLISHER", r#"{"kind":"stub"}"#),
    ]);

    let cfg = AppConfig::load_from_paths(None, None).expect("标量路径未回归");
    assert_eq!(cfg.http_port, 9999, "整数字符串仍是整数, 不是字符串");
    assert_eq!(cfg.refresh_token_ttl_seconds, 12345);
    assert_eq!(cfg.max_message_size_bytes, 2048);
}

/// 写坏的 JSON 必须**报错**, 不能被静默接受成别的东西。
///
/// 「静默改写配置」比「启动失败」危险得多: 后者一眼可见, 前者会让服务用着
/// 一份**没人预期**的配置跑起来(比如签名密钥解析成了空数组 → 所有 token
/// 都验不过, 或者更糟: 解析成了某个默认值)。
#[test]
fn malformed_json_env_value_is_rejected_not_silently_accepted() {
    let _g = EnvGuard::set(&[
        ("IM_HTTP_PORT", "8080"),
        ("IM_POSTGRES_URL", "postgres://im:im@postgres:5432/im"),
        ("IM_JWT_SIGNING_KEYS", r#"[{"kid":"v1","key":}]"#), // 坏 JSON
        ("IM_REFRESH_PEPPER", "pepper-value"),
    ]);

    let r = AppConfig::load_from_paths(None, None);
    assert!(r.is_err(), "坏的 JSON 必须让配置加载失败, 不能被静默接受");
}
