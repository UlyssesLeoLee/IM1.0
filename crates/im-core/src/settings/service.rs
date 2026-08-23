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

use std::collections::HashMap;

use im_common::ids::EnvironmentId;
use im_common::AppError;
use serde::{Deserialize, Serialize};

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
    pub dm: Option<u32>,      // None = 永久
    pub group: Option<u32>,
    pub channel: Option<u32>,
}

impl Default for RetentionSettings {
    fn default() -> Self {
        Self {
            dm: None,           // 永久
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

pub struct SettingsService {
    cache: HashMap<EnvironmentId, EnvironmentSettings>,
}

impl SettingsService {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// 启动时全量加载(实际从 DB + 写 Valkey)
    pub async fn load_initial(&mut self) -> Result<(), AppError> {
        // MVP: 仅初始化空 cache
        // 实际:SELECT id, settings FROM environments
        Ok(())
    }

    /// 获取 env 配置
    pub fn get(&self, env: EnvironmentId) -> EnvironmentSettings {
        self.cache
            .get(&env)
            .cloned()
            .unwrap_or_default()
    }

    /// 收到 Valkey pub/sub 失效广播后重载
    pub async fn invalidate(&mut self, env: EnvironmentId) -> Result<(), AppError> {
        self.cache.remove(&env);
        // 实际:重新从 DB SELECT
        Ok(())
    }
}

impl Default for SettingsService {
    fn default() -> Self {
        Self::new()
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
