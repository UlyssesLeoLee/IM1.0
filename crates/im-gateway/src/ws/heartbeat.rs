//! WS 心跳模块 — C-12 WBS
//!
//! 依据: aux-13 §1.1.8 (client ping) + §1.2.11 (server pong) + §1.3 协议规则总结
//!
//! ## 设计
//! - 客户端每 30s 发一次 `{"type": "ping", "ts": <ms>}`
//! - 服务端收到 ping 立即回 `{"type": "pong", "ts": <ms>}`,ts 与 ping 帧一致
//! - 服务端维护 last_activity_at;60s 内无任何帧视为死连接,主动 close
//!   (`IM_WS_HEARTBEAT_TIMEOUT_SECONDS=60`,见 aux-13 §1.1.8 注释 + DetailedDesign §10)
//!
//! ## 与 C-11 集成点
//! 本模块只做心跳调度,不主动维护 WS 收发循环。
//! `WsSession`(C-11 主实装)应在 auth 完成且收到 client frame 时调用
//! [`HeartbeatState::on_frame`],在 tick 时调用 [`HeartbeatState::tick`] 检查超时。
//!
//! 本 PR 仅预留接口 + 单元测试覆盖;真正的 actix-ws 0.3 收发循环在 C-11 接入。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 心跳配置(MVP 默认值与 aux-13 §1.1.8 注释一致)
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// 客户端 ping 间隔(期望,非强制) — 30s
    pub expected_ping_interval: Duration,
    /// 服务端无帧超时 — 60s(2 × 30s,允许一次 ping 丢)
    pub no_frame_timeout: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        // aux-13 §1.1.8:客户端每 30s 发一次 ping
        // aux-13 §1.1.8 注释:`IM_WS_HEARTBEAT_TIMEOUT_SECONDS=60`
        Self {
            expected_ping_interval: Duration::from_secs(30),
            no_frame_timeout: Duration::from_secs(60),
        }
    }
}

/// 心跳状态 — 单 session 实例
///
/// `WsSession` 持有一个,通过 `Arc` 共享给驱动循环。
/// 现在先做单线程版本;后续 actix-ws 适配时可能改为 `Arc<tokio::sync::Mutex>`。
#[derive(Debug)]
pub struct HeartbeatState {
    cfg: HeartbeatConfig,
    last_frame_at: Mutex<std::time::Instant>,
}

impl HeartbeatState {
    pub fn new(cfg: HeartbeatConfig) -> Self {
        Self {
            cfg,
            last_frame_at: Mutex::new(std::time::Instant::now()),
        }
    }

    /// 收到任意 client 帧(ping/auth/业务帧)时调用,刷新活跃时间
    pub fn on_frame(&self) {
        if let Ok(mut g) = self.last_frame_at.lock() {
            *g = std::time::Instant::now();
        }
    }

    /// 当前自上次任意帧以来的空闲时长
    pub fn idle(&self) -> Duration {
        self.last_frame_at
            .lock()
            .map(|g| g.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// 距离超时的剩余时间(已超时则返回 0)
    pub fn remaining(&self) -> Duration {
        let idle = self.idle();
        self.cfg
            .no_frame_timeout
            .checked_sub(idle)
            .unwrap_or(Duration::ZERO)
    }

    /// tick — 心跳循环每次唤醒时调用,返回 true 表示应主动断开连接
    pub fn tick(&self) -> bool {
        self.idle() >= self.cfg.no_frame_timeout
    }

    pub fn config(&self) -> &HeartbeatConfig {
        &self.cfg
    }
}

/// 客户端 ping 帧(MVP 选 JSON 文本帧,per aux-13 §1.1.8)
///
/// 注:im-protocol 已有 `ClientFrame::Ping { ts }` 枚举,本 struct 仅作独立心跳路径
/// 的 wire 兼容镜像(避免强耦合 im-protocol 的全部 8 类帧)。后续 C-11 主实装时,
/// 若决定统一走 im_protocol::ws_frames,直接复用即可。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingFrame {
    #[serde(rename = "type")]
    pub ty: PingPongType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
}

/// pong 帧 wire 镜像(同上)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PongFrame {
    #[serde(rename = "type")]
    pub ty: PingPongType,
    pub ts: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PingPongType {
    Ping,
    Pong,
}

impl PingFrame {
    /// 构造标准 pong 响应(回 ts)
    pub fn into_pong(&self) -> PongFrame {
        PongFrame {
            ty: PingPongType::Pong,
            ts: self.ts.unwrap_or(0),
        }
    }
}

/// 共享 HeartbeatState 入口 — 便于 WsSession 持有 Arc
pub type SharedHeartbeat = Arc<HeartbeatState>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_default_config_matches_aux13() {
        let cfg = HeartbeatConfig::default();
        assert_eq!(cfg.expected_ping_interval, Duration::from_secs(30));
        // aux-13 §1.1.8 注释:IM_WS_HEARTBEAT_TIMEOUT_SECONDS=60
        assert_eq!(cfg.no_frame_timeout, Duration::from_secs(60));
    }

    #[test]
    fn heartbeat_state_tick_before_timeout_returns_false() {
        let cfg = HeartbeatConfig::default();
        let hs = HeartbeatState::new(cfg);
        assert!(!hs.tick(), "刚初始化的 state,空闲时间 0s,不应超时");
    }

    #[test]
    fn heartbeat_state_on_frame_resets_idle() {
        let cfg = HeartbeatConfig {
            expected_ping_interval: Duration::from_millis(10),
            no_frame_timeout: Duration::from_millis(50),
        };
        let hs = HeartbeatState::new(cfg);
        // 模拟空闲 100ms
        std::thread::sleep(Duration::from_millis(100));
        assert!(hs.tick(), "100ms > 50ms timeout,应超时");

        // on_frame 后立即不超时
        hs.on_frame();
        assert!(!hs.tick(), "on_frame 后立即 tick,不应超时");
    }

    #[test]
    fn ping_frame_into_pong_preserves_ts() {
        let ping = PingFrame {
            ty: PingPongType::Ping,
            ts: Some(1692528000000),
        };
        let pong = ping.into_pong();
        assert_eq!(pong.ty, PingPongType::Pong);
        assert_eq!(pong.ts, 1692528000000);
    }

    #[test]
    fn ping_frame_serde_roundtrip() {
        let ping = PingFrame {
            ty: PingPongType::Ping,
            ts: Some(1692528000000),
        };
        let s = serde_json::to_string(&ping).unwrap();
        assert!(s.contains("\"type\":\"ping\""), "snake_case type: {s}");
        let back: PingFrame = serde_json::from_str(&s).unwrap();
        assert_eq!(back, ping);
    }

    #[test]
    fn pong_frame_serde_roundtrip() {
        let pong = PongFrame {
            ty: PingPongType::Pong,
            ts: 1692528000000,
        };
        let s = serde_json::to_string(&pong).unwrap();
        assert!(s.contains("\"type\":\"pong\""), "snake_case type: {s}");
        let back: PongFrame = serde_json::from_str(&s).unwrap();
        assert_eq!(back, pong);
    }
}
