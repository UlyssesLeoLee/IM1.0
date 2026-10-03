//! im-gateway WebSocket 模块
//!
//! 依据: aux-13 §1 + ImplementationSpec §3.2
//!
//! 本文件负责 WS 协议层:
//! - `heartbeat`: ping/pong 心跳循环(per C-12, 30s client ping / 60s server no-frame-timeout per aux-13 §1.3)
//! - `session`: WsSession 占位骨架(C-11 主实装的预留接口)
//! - `handler`: actix-ws 0.3 收发循环实装(C-11 driver, 2026-09-19 lane-backend-core-2)
//! - `hub`: 广播中枢(2026-10-03 新增)—— 单 broadcast 通道 + 连接侧按会话成员
//!   关系过滤。**此前不存在**, handler 模块文档却称有「占位 broadcast channel」
//! - `router`: 路由注册 `/v1/ws`

pub mod handler;
pub mod heartbeat;
pub mod hub;
pub mod router;
pub mod session;

#[cfg(test)]
mod e2e;
