//! im-gateway WebSocket 模块
//!
//! 依据: aux-13 §1 + ImplementationSpec §3.2
//!
//! 本文件负责 WS 协议层:
//! - `heartbeat`: ping/pong 心跳循环(per C-12, 30s client ping / 60s server no-frame-timeout per aux-13 §1.3)
//! - `session`: WsSession 占位骨架(C-11 主实装的预留接口)
//! - `handler`: 后续 PR 接入 actix-ws 0.3

pub mod heartbeat;
pub mod session;
