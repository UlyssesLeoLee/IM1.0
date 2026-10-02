//! SettingsService — 租户/环境级配置
//!
//! 依据: ImplementationSpec §6.3 + BasicDesign §14.3
//!
//! 9 项字段(详见 BasicDesign §14.3):
//! - friend_system_enabled
//! - rate_limit.send_message.per_min
//! - rate_limit.guest_register.per_hour
//! - message.recall_window_seconds
//! - message.retention_days.dm / group / channel
//! - voice.enabled
//! - audit.detailed
//!
//! ## 2026-10-03 变真
//!
//! 此前本文件是一个**看起来存在、实则什么都没做**的实现:
//! - `load_initial()` 注释写「实际: SELECT id, settings FROM environments」,
//!   函数体是 `Ok(())` —— 从不读库;
//! - `get(env)` 从一个**永远为空**的 `HashMap` 取值, 任何 env 都落到
//!   `unwrap_or_default()`, 即**恒定返回 `recall_window_seconds = 120`**;
//! - `invalidate()` 只从那个空 map 里删一个不存在的键, 从不重读。
//!
//! 与 `ws::handler` 那个「占位 broadcast channel」同族 —— 文档说有、代码没有。
//! 危险之处在于它**看起来是可配的**: 若把撤回时间窗接到 `get()` 上, 代码读起来
//! 是「从 env settings 读」(符合 aux-04 §B.4 不变量「不能写死」), 实际却恒为
//! 120 —— 这比明写死更糟, 因为它骗过了 review。
//!
//! ## 为什么 MVP **不缓存**
//!
//! 原设计有 `HashMap` 缓存 + Valkey pub/sub 失效广播。但失效通道属于
//! **WBS D-4 (Valkey)**, 尚未落地 —— 没有失效的缓存是**正确性隐患**: 运营改了
//! `recall_window_seconds`, 进程内仍按旧值判定, 且没有任何办法让它刷新。
//! 故 MVP 每次调用读一次(走 `environments.id` 主键, 一次索引查询; 撤回是低频
//! 操作, 无需优化)。真正的缓存随 D-4 落地时再加, 届时失效通道与缓存同时到位。

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use im_common::ids::EnvironmentId;
use im_common::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSettings {
    #[serde(default = "default_true")]
    pub friend_system_enabled: bool,

    #[serde(default)]
    pub rate_limit: RateLimitSettings,

    #[serde(default)]
    pub message: MessageSettings,

    #[serde(default)]
    pub voice: VoiceSettings,

    #[serde(default)]
    pub audit: AuditSettings,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitSettings {
    #[serde(default = "default_send_per_min")]
    pub send_message_per_min: u32,
    #[serde(default = "default_guest_register")]
    pub guest_register_per_hour: u32,
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            send_message_per_min: default_send_per_min(),
            guest_register_per_hour: default_guest_register(),
        }
    }
}

impl Default for EnvironmentSettings {
    fn default() -> Self {
        Self {
            friend_system_enabled: true,
            rate_limit: RateLimitSettings::default(),
            message: MessageSettings::default(),
            voice: VoiceSettings::default(),
            audit: AuditSettings::default(),
        }
    }
}

fn default_send_per_min() -> u32 {
    60
}
fn default_guest_register() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSettings {
    #[serde(default = "default_recall_window")]
    pub recall_window_seconds: u32,
    #[serde(default)]
    pub retention_days: RetentionSettings,
}

impl Default for MessageSettings {
    fn default() -> Self {
        Self {
            recall_window_seconds: 120,
            retention_days: RetentionSettings::default(),
        }
    }
}

fn default_recall_window() -> u32 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionSettings {
    pub dm: Option<u32>, // None = 永久
    pub group: Option<u32>,
    pub channel: Option<u32>,
}

impl Default for RetentionSettings {
    fn default() -> Self {
        Self {
            dm: None, // 永久
            group: None,
            channel: Some(365), // 频道 1 年
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceSettings {
    pub enabled: bool, // MVP 默认 false(由 Default 给 false)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditSettings {
    pub detailed: bool,
}

/// 环境级配置读取(per `environments.settings` JSONB 列)
///
/// 该列 `NOT NULL DEFAULT '{}'`, 且有 `CHECK (jsonb_typeof(settings) = 'object')`
/// 约束 —— 所以「行存在但 settings 为空对象」是合法状态, 必须由
/// `EnvironmentSettings` 的 serde `default` 逐字段补齐(每个字段都带
/// `#[serde(default = ...)]`), 而**不是**在代码里判空回退。
pub struct SettingsService {
    pool: PgPool,
}

impl SettingsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 读某环境的完整 settings
    ///
    /// 环境不存在时返回 `AppError::NotFound` —— 这与「环境存在但某个字段没配」
    /// 是**两件事**, 必须让调用方能区分: 前者应报 404, 后者由 serde default
    /// 正常兜底。
    ///
    /// **为什么用 `NotFound` 而不新增 `EnvironmentNotFound` 变体**: 新增
    /// `AppError` 变体必然新增一个 wire 错误码, 而 aux-03 §B 是 MVP 错误码的
    /// 唯一权威表、新增码属协议变更, ImplementationSpec 处于 `[PROTOCOL-FROZEN]`。
    /// 已注册的 `NOT_FOUND` 语义上完全覆盖(就是一个不存在的东西), 不必为此
    /// 冒破冻结的风险; 具体是哪个 id 放在 message 里, 服务端日志查得到。
    ///
    /// 反序列化失败**不**回退到 `default()`: 库里存着一份解析不了的 JSON,
    /// 静默当成默认值会让运营以为自己改的配置生效了(实际没生效), 且撤回
    /// 时间窗会悄悄变成 120s。故如实报错。
    pub async fn get(&self, env: EnvironmentId) -> Result<EnvironmentSettings, AppError> {
        let row: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT settings FROM environments WHERE id = $1")
                .bind(env.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlx settings: {e}")))?;

        let Some((raw,)) = row else {
            return Err(AppError::NotFound(format!("environment {}", env.0)));
        };
        serde_json::from_value(raw)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("environments.settings 解析失败: {e}")))
    }

    /// 撤回时间窗(秒)—— 便捷方法, 语义即 aux-04 §B.4 不变量里的
    /// `env.settings.message.recall_window_seconds`
    pub async fn recall_window_seconds(&self, env: EnvironmentId) -> Result<u32, AppError> {
        Ok(self.get(env).await?.message.recall_window_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = EnvironmentSettings::default();
        assert!(s.friend_system_enabled);
        assert_eq!(s.rate_limit.send_message_per_min, 60);
        assert_eq!(s.rate_limit.guest_register_per_hour, 10);
        assert_eq!(s.message.recall_window_seconds, 120);
        assert_eq!(s.message.retention_days.channel, Some(365));
        assert!(s.message.retention_days.dm.is_none());
        assert!(!s.voice.enabled);
        assert!(!s.audit.detailed);
    }

    #[test]
    fn json_roundtrip() {
        let s = EnvironmentSettings {
            friend_system_enabled: false,
            rate_limit: RateLimitSettings {
                send_message_per_min: 100,
                guest_register_per_hour: 5,
            },
            message: MessageSettings {
                recall_window_seconds: 300,
                retention_days: RetentionSettings {
                    dm: Some(0),
                    group: Some(30),
                    channel: Some(90),
                },
            },
            voice: VoiceSettings { enabled: true },
            audit: AuditSettings { detailed: true },
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: EnvironmentSettings = serde_json::from_str(&j).unwrap();
        assert_eq!(back.message.recall_window_seconds, 300);
        assert!(back.voice.enabled);
    }
}
