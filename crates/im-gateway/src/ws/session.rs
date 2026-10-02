//! WsSession 占位骨架 — C-11 主实装预留接口
//!
//! 依据: aux-13 §1 客户端 8 类 / 服务端 11 类帧
//!
//! ## 范围 (per 132-wbs.md §5.3.2 C-11, 400K/800K tokens)
//! 本 PR (C-12 范围) 仅:
//! 1. 定义 WsSession 数据结构 + 鉴权与心跳调度接口骨架
//! 2. 提供 `try_handle_frame` 统一入口(分发到业务帧 + 心跳)
//!
//! 主实装(C-11)留待后续 worker:
//! - actix-ws 0.3 Text/Binary 收发循环
//! - im-core gRPC 客户端调用(`ValidateAccessToken` per aux-13 §2.3 + `SendMessage` per §2.1 等)
//! - 鉴权后 session 状态机:Unauthenticated → Authenticated → Closed
//! - ForceDisconnect 广播(`account_banned` / `token_revoked` 等)
//!
//! ## 派生约束 (per 8/26 JST 守门)
//! - 任何 wire 字段映射必须与 aux-13 §1 双向对齐(不可自创字段)
//! - 心跳参数 30s / 60s 与 aux-13 §1.1.8 + §1.3 一致
//! - 留"已知缺口"清单,交给父 Mavis 决定

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{ConversationId, DeviceSessionId, EnvironmentId, UserId};

use super::heartbeat::{HeartbeatState, PongFrame, SharedHeartbeat};

/// 鉴权状态机
///
/// 状态转换:
/// ```text
///   Created ─(收到 Auth 帧 鉴权通过)─→ Authenticated ─(close / 超时 / force)─→ Closed
///       └──(收到 Auth 帧 鉴权失败)───┘
///       └──(auth 100ms 超时)───────────────────→ Closed
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// 已创建,等待 client 首帧 Auth
    Created,
    /// 鉴权通过(claims 校验通过)
    Authenticated,
    /// 已关闭
    Closed,
}

/// WsSession — 单连接生命周期
///
/// MVP 仅做字段聚合 + 帧分发接口;收发循环在 C-11 主实装。
#[derive(Debug)]
pub struct WsSession {
    session_id: Uuid,
    state: SessionState,
    user_id: Option<UserId>,
    environment_id: Option<EnvironmentId>,
    device_session_id: Option<DeviceSessionId>,
    /// 本连接所属的会话集合(鉴权时快照一次)
    ///
    /// 2026-10-03 新增, 供 `ws::hub` 做广播过滤。**这不是优化, 是安全边界**:
    /// 不过滤的话用户 A 会收到用户 B 所在会话的消息。
    conversation_ids: std::collections::HashSet<ConversationId>,
    heartbeat: SharedHeartbeat,
}

impl WsSession {
    pub fn new(session_id: Uuid, heartbeat: SharedHeartbeat) -> Self {
        Self {
            session_id,
            state: SessionState::Created,
            user_id: None,
            environment_id: None,
            device_session_id: None,
            conversation_ids: std::collections::HashSet::new(),
            heartbeat,
        }
    }

    /// 鉴权完成回调(由 C-11 主实装在 ValidateAccessToken gRPC 成功后调用)
    pub fn mark_authenticated(
        &mut self,
        user_id: UserId,
        environment_id: EnvironmentId,
        device_session_id: DeviceSessionId,
    ) {
        self.state = SessionState::Authenticated;
        self.user_id = Some(user_id);
        self.environment_id = Some(environment_id);
        self.device_session_id = Some(device_session_id);
        self.heartbeat.on_frame();
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// 鉴权后的 user_id (供业务帧取 sender —— 身份只能来自会话状态, 不能来自帧内容)
    pub fn user_id(&self) -> Option<UserId> {
        self.user_id
    }

    /// 设置本连接所属会话集合(鉴权成功后快照一次)
    pub fn set_conversation_ids(&mut self, ids: std::collections::HashSet<ConversationId>) {
        self.conversation_ids = ids;
    }

    /// 该连接是否属于某会话 —— 广播过滤的安全边界
    ///
    /// 鉴权前(`state != Authenticated`)一律返回 false: 未鉴权的连接不该
    /// 收到任何广播, 否则会成为「连上就能听」的旁听入口。
    pub fn is_member_of(&self, conv: ConversationId) -> bool {
        self.state == SessionState::Authenticated && self.conversation_ids.contains(&conv)
    }

    /// 本连接所属会话数(诊断用)
    pub fn conversation_count(&self) -> usize {
        self.conversation_ids.len()
    }

    /// 鉴权后的 environment_id
    ///
    /// 2026-10-03 起**已被生产路径使用**: `ws::handler::handle_recall_message`
    /// 用它经 `SettingsService` 读 `environments.settings` 的撤回时间窗
    /// (aux-04 §B.4 不变量要求该值不能写死)。此前它挂着 `#[allow(dead_code)]`
    /// 且无任何调用方, 现压制已移除 —— 留着会让「没人用」这个事实看起来像
    /// 「有意保留」。
    pub fn environment_id(&self) -> Option<EnvironmentId> {
        self.environment_id
    }

    /// 心跳 tick — 返回 true 表示应主动 close
    // 守门 #1 缺口台账: 缺口 #H — 30s background task 调用点待 C-11 driver 实装。
    // 保留不删,per docs/Project-Status.md §1.1.1 占位符保留约定。
    #[allow(dead_code)]
    pub fn tick_heartbeat(&self) -> bool {
        self.heartbeat.tick()
    }

    /// 心跳共享状态(给 C-11 driver 访问 on_frame)
    pub fn heartbeat(&self) -> SharedHeartbeat {
        Arc::clone(&self.heartbeat)
    }

    /// 强制断开
    pub fn force_close(&mut self) {
        self.state = SessionState::Closed;
    }
}

/// WS 入站帧 — 心跳ping 的统一入口
///
/// C-11 主实装时,driver 会:
/// 1. 收到任意 frame → `session.heartbeat().on_frame()`
/// 2. 解析 ClientFrame:
///    - Auth → 调 im-core gRPC ValidateAccessToken,on success → mark_authenticated
///    - Ping → 立即回 PongFrame(ts)
///    - SendMessage / Edit / Recall / React / MarkRead / Typing → 调对应 im-core RPC,回 Ack
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// 守门 #1 缺口台账: 缺口 #I — ⚠️ **2026-10-03 状态变更**。
// 入站分派**已存在**于 `ws/handler.rs::run_ws_loop` 的主循环 `match`, 且已补齐
// 此前缺失的 `ClientFrame::Ping` 分支(ping 原先落进业务帧兜底, 收不到 pong)。
// 但该主循环走的是 `im_protocol::ws_frames::ClientFrame`, **不经过本枚举**
// —— 本枚举(含 `handle_ping()`)因此成为未接线的第二套入站抽象。
// 按 §1.1.1 占位符保留约定先留不删; 接线方向已与 #F 一致定为 im_protocol,
// 故**不再是待决项**, 清理时应与 `From<ClientFrame>` 及其单测一并处理。
#[allow(dead_code)]
pub enum WsInbound {
    /// 心跳 ping(per aux-13 §1.1.8:`{"type":"ping","ts":...}`)
    Ping {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ts: Option<i64>,
    },
}

impl WsInbound {
    /// 处理 ping,回 pong
    // 守门 #1 缺口台账: 缺口 #I — 待 C-11 driver 接线。保留不删。
    #[allow(dead_code)]
    pub fn handle_ping(&self) -> PongFrame {
        match self {
            WsInbound::Ping { ts } => PongFrame {
                ty: super::heartbeat::PingPongType::Pong,
                ts: ts.unwrap_or(0),
            },
        }
    }
}

/// 从 im_protocol::ws_frames::ClientFrame::Ping 转 WsInbound::Ping
impl From<im_protocol::ws_frames::ClientFrame> for WsInbound {
    fn from(_frame: im_protocol::ws_frames::ClientFrame) -> Self {
        // 当前 PR 只显式支持 Ping,其他帧留给 C-11 driver
        // 注意:ClientFrame::Ping { ts } 是 unit-like 字段结构
        // 此处用 default 行为是为了保留完整 client frame 的扩展性
        WsInbound::Ping { ts: None }
    }
}

/// 构造共享 Heartbeat 状态(C-11 driver 使用)
pub fn new_heartbeat() -> SharedHeartbeat {
    Arc::new(HeartbeatState::new(Default::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_transitions() {
        let hb = new_heartbeat();
        let mut s = WsSession::new(Uuid::new_v4(), hb);
        assert_eq!(s.state(), SessionState::Created);
        assert!(s.user_id().is_none());

        let uid = UserId::new();
        let env = EnvironmentId::new();
        let dev = DeviceSessionId::new();
        s.mark_authenticated(uid, env, dev);
        assert_eq!(s.state(), SessionState::Authenticated);
        assert_eq!(s.user_id(), Some(uid));
        assert_eq!(s.environment_id(), Some(env));

        s.force_close();
        assert_eq!(s.state(), SessionState::Closed);
    }

    #[test]
    fn ping_inbound_returns_pong_with_ts() {
        let inbound = WsInbound::Ping { ts: Some(12345) };
        let pong = inbound.handle_ping();
        assert_eq!(pong.ts, 12345);
    }

    #[test]
    fn ping_inbound_without_ts_returns_zero() {
        let inbound = WsInbound::Ping { ts: None };
        let pong = inbound.handle_ping();
        assert_eq!(pong.ts, 0);
    }

    #[test]
    fn ping_frame_serde_skips_missing_ts() {
        let inbound = WsInbound::Ping { ts: None };
        let s = serde_json::to_string(&inbound).unwrap();
        assert!(s.contains("\"type\":\"ping\""), "snake_case type: {s}");
        assert!(!s.contains("\"ts\""), "省略 ts 时不应序列化: {s}");
    }

    #[test]
    fn ping_frame_serde_with_ts() {
        let inbound = WsInbound::Ping { ts: Some(42) };
        let s = serde_json::to_string(&inbound).unwrap();
        let back: WsInbound = serde_json::from_str(&s).unwrap();
        match back {
            WsInbound::Ping { ts } => assert_eq!(ts, Some(42)),
        }
    }

    #[test]
    fn session_tick_returns_false_when_fresh() {
        let hb = new_heartbeat();
        let s = WsSession::new(Uuid::new_v4(), hb);
        assert!(!s.tick_heartbeat(), "新 session 不应立即超时");
    }

    #[test]
    fn heart_state_via_session_can_be_refreshed() {
        let hb = new_heartbeat();
        let session = WsSession::new(Uuid::new_v4(), hb.clone());
        let s_hb = session.heartbeat();
        s_hb.on_frame();
        // 立即 tick,不应超时
        assert!(!s_hb.tick());
    }
}
