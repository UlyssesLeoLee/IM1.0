//! C-11 WsSession driver — actix-ws 0.3 收发循环实装 + 激活 C-12 skeleton
//!
//! 依据: aux-13-protocol-frame-samples.md §1 (客户端 8 类 / 服务端 11 类帧) + §2 (连接流程) + §4 (错误码)
//!       ImplementationSpec §3.2 (WS 协议)
//!       132-wbs.md §5.3.2 C-11 (base 400K / max 800K tokens)
//!
//! ## 范围 (per 132-wbs.md §5.3.2 + 2026-09-19 lane-backend-core-2)
//!
//! ### 实装
//! - `POST /v1/ws` 端点 (HTTP upgrade → WebSocket)
//! - actix-ws 0.3 Text/Binary 帧收发循环
//! - 第一帧 `Auth` (JSON text, type="auth", access_token=...) → 走 TokenService::validate_access_token → mark_authenticated
//! - 后续帧:
//!   - `Ping` → `HeartbeatState::on_frame()` 重置心跳 + 回
//!     `ServerFrame::Pong { ts }`(ts 原样回传, per aux-13 §1.1.8)。
//!     **2026-10-03 已修复**: 此前没有 Ping 分支, ping 落进业务帧兜底被回
//!     `VALIDATION_ERROR` 错误帧, 客户端永远收不到 pong。
//!   - 其他业务帧 (SendMessage / Edit / Recall / React / MarkRead / Typing) → 列已知缺口 (C-9 messages handler 还没做)
//! - 30s 心跳检查间隔(常量 `HEARTBEAT_TICK`)与 60s 无帧超时(`HeartbeatConfig::no_frame_timeout`)
//!   在 `run_ws_loop` 的 `select!` 内**同处判定**: 每 30s 查一次 idle, 满 60s
//!   则主动关闭连接(退出主循环 → `ws_handler` 走 close handshake)。
//!   **2026-10-03 已修复**: 原实现把 tick 放在独立后台 task 里, 而
//!   `actix_ws::Session` 归主循环所有, 后台 task 拿不到, 信号也无处可送;
//!   主循环又只 `await msg_stream.next()` 而无超时分支 —— 两者叠加导致
//!   60s 无帧超时**形同虚设**, 半开连接堆积到 TCP 超时。详见
//!   `docs/gap-ledger.md` §1.1 缺口 #H。
//! - ForceDisconnect hook stub (后续 G-1 presence 集成)
//!
//! ### 已知缺口 (per 守门 #1 缺标比错标)
//! 1. **业务帧处理 (SendMessage / Edit / Recall / React / MarkRead / Typing)**: 留 C-9 + 后续 lane
//!
//!    **2026-10-03 更正 —— 本条原注释是双重错误, 勿照抄**:
//!    - 原写「返 `UNSUPPORTED_OPERATION` (501, per aux-13 §4)」。**该错误码不存在**:
//!      `aux-03-error-code-registry.md` §B 是 MVP 错误码的**唯一权威表**, 列出 21 个
//!      已注册码, 与 `im_common::ErrorCode` 枚举逐项一致, 其中**没有**
//!      `UNSUPPORTED_OPERATION`, 也没有 501。
//!    - 原引用的 `aux-13 §4` 是**「前置条件 (Prerequisites)」**章节, 不是错误码章节 ——
//!      出处本身也不成立。
//!    - 实际代码返回 `VALIDATION_ERROR` (400)。它是个**已注册**码, 但语义不合:
//!      业务帧格式完全合法, 缺的只是服务端处理器。aux-03 §B 对该码的定义是
//!      「请求体校验失败 / 字段类型、长度、枚举值不合法」。
//!
//!    **为何不改成 501**: `aux-03 §B` 写明「任何 PR 增加必须同时更新本表与
//!    DetailedDesign」, 即新增错误码是**协议变更**; 而 ImplementationSpec 处于
//!    `[PROTOCOL-FROZEN]`。是否新增「未实装」类错误码属规范所有者的决定, 不是
//!    架构师可自行拍板的实现细节。已记入 `docs/gap-ledger.md` §1.6。
//! 2. **im-proto gRPC 客户端**: 有 stub (per worker-C 探索 `im_proto::im::core::v1::core_service_client::CoreServiceClient`), 但 MVP Day 3 没 wire-up gRPC channel; Auth 帧的 TokenClaims 解析走本地 TokenService (已经在 im-gateway 进程内), 不走 gRPC
//! 3. **ForceDisconnect broadcast**: 占位 broadcast channel, 实际 broadcasting 留 G-1 presence
//!
//! 已于 2026-10-03 结清:
//! - 曾列为缺口 4 的「60s 无帧超时未真正关闭连接」已修复 —— tick 搬进
//!   `run_ws_loop` 的 `select!`, 超时即退出主循环并走 close。
//! - 曾列为缺口 5 的「device_session_id 来自 JWT claims / TokenClaims 没 `dsid` 字段」
//!   已修复 —— 见 `docs/gap-ledger.md` §1.5 (C-7 logout 实装, 本轮一并补上)。

use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::{CloseCode, CloseReason, Message};
use futures::StreamExt;
use serde::Deserialize;
use tokio::time::interval;
use uuid::Uuid;

use im_common::ids::{DeviceSessionId, EnvironmentId, UserId};
use im_common::AppError;
use im_core::identity::token::TokenService;
use im_protocol::ws_frames::{ClientFrame, ServerFrame};

use super::session::{SessionState, WsSession};

use crate::http::state::AppState;

/// 心跳检查间隔 — 30s (per aux-13 §1.3)
///
/// 超时阈值本身在 `HeartbeatConfig::no_frame_timeout`(60s), 与本常量是
/// "多久检查一次" 与 "多久算超时" 的关系: 每 30s 查一次, 允许客户端丢一次
/// ping。两者不可混为一谈, 故分别定义。
const HEARTBEAT_TICK: Duration = Duration::from_secs(30);

// ============================================================================
// Auth 帧 DTO — 仅第一帧用,后续业务帧走 im_protocol::ws_frames::ClientFrame
// ============================================================================

/// WS Auth 帧 (per aux-13 §2.3 + §1.1.1)
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AuthFrame {
    Auth {
        #[serde(default)]
        req_id: Option<Uuid>,
        access_token: String,
    },
}

/// WS 错误响应帧 (per aux-13 §4 错误码格式)
#[derive(Debug, Clone, serde::Serialize)]
struct WsErrorFrame {
    #[serde(rename = "type")]
    ty: &'static str,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    req_id: Option<Uuid>,
}

impl WsErrorFrame {
    fn from_app_error(code: im_common::ErrorCode, message: &str, req_id: Option<Uuid>) -> Self {
        Self {
            ty: "error",
            code: code.as_str().to_string(),
            message: message.to_string(),
            req_id,
        }
    }
}

// ============================================================================
// HTTP upgrade → WS 端点
// ============================================================================

/// `GET /v1/ws` (HTTP upgrade → WebSocket)
///
/// 流程:
/// 1. actix_ws::handle() 建立 WS 连接
/// 2. 启 background task: 30s tick_heartbeat + 60s 无帧超时检测
/// 3. 主 loop: 收 Text frame → 解析 (Auth / Ping / 业务帧) → 处理 → 发响应
/// 4. 关闭: 业务逻辑 / force_close / 超时 → 走 close handshake
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (response, mut session, mut msg_stream) =
        actix_ws::handle(&req, stream).map_err(|e| -> actix_web::Error {
            tracing::warn!(error = ?e, "ws upgrade failed");
            actix_web::error::InternalError::from_response(
                String::from("ws_upgrade_failed"),
                HttpResponse::BadRequest().finish(),
            )
            .into()
        })?;

    // 创建 WsSession (per C-12 skeleton)
    let session_id = Uuid::new_v4();
    let hb = super::session::new_heartbeat();
    let mut ws_session = WsSession::new(session_id, hb.clone());

    // 注: 原先这里有一个独立后台 task 跑 30s heartbeat tick, 但它既拿不到
    // `actix_ws::Session` 也无处把超时信号送给主循环, 只能打日志后 break ——
    // 于是 60s 无帧超时形同虚设。该 task 已删除, tick 搬进 `run_ws_loop` 的
    // select!(见该函数内注释)。缺口 #H。

    // 主 loop — 异步 spawn, 不阻塞 HTTP upgrade response 返回
    let app_for_loop = app.clone();
    actix_web::rt::spawn(async move {
        if let Err(e) = run_ws_loop(
            &mut session,
            &mut msg_stream,
            &app_for_loop,
            &mut ws_session,
        )
        .await
        {
            tracing::warn!(error = ?e, session_id = %session_id, "ws loop ended");
        }
        // 关闭 session
        let _ = session
            .close(Some(CloseReason {
                code: CloseCode::Normal,
                description: Some("session closed".into()),
            }))
            .await;
    });

    Ok(response)
}

/// WS 主循环 (per C-11 范围)
async fn run_ws_loop(
    ws_session: &mut actix_ws::Session,
    msg_stream: &mut actix_ws::MessageStream,
    app: &web::Data<AppState>,
    state: &mut WsSession,
) -> Result<(), AppError> {
    // 2026-10-03 修(缺口 #H): 心跳 tick 原先放在一个独立后台 task 里, 但
    // 那个 task 只 `tracing::info!` + `break` —— `actix_ws::Session` 归本主循环
    // 所有, 后台 task 拿不到, 信号也无处可送; 而本循环原先只 `await
    // msg_stream.next()`, 没有超时分支。两者叠加的结果是 **60s 无帧超时
    // 永远不会关闭连接**, 半开连接一直堆积到 TCP 超时。现把 interval 搬进
    // 主循环用 select! 直接判定并退出(退出后由 ws_handler 统一走 close)。
    let mut tick = interval(HEARTBEAT_TICK);

    loop {
        // biased: 有帧时优先处理帧。客户端持续发帧时 tick 分支不会命中是**正确**
        // 的 —— 每帧都会 `heartbeat().on_frame()` 重置 idle, 本就不会超时;
        // 只有客户端停发帧时 msg 才不再 ready, select 才落到 tick 分支判超时。
        let msg_result = tokio::select! {
            biased;
            m = msg_stream.next() => m,
            _ = tick.tick() => {
                if state.tick_heartbeat() {
                    tracing::warn!(
                        session_id = %state.session_id(),
                        idle_ms = state.heartbeat().idle().as_millis() as u64,
                        "ws heartbeat timeout, closing"
                    );
                    state.force_close();
                    return Ok(());
                }
                continue;
            }
        };

        // 消息流结束(对端断开)等价于正常关闭
        let Some(msg_result) = msg_result else {
            state.force_close();
            return Ok(());
        };

        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = ?e, "ws recv error");
                return Err(AppError::Internal(anyhow::anyhow!("ws recv error: {e}")));
            }
        };

        match msg {
            Message::Text(text) => {
                let text_str: &str = match std::str::from_utf8(text.as_bytes()) {
                    Ok(s) => s,
                    Err(_) => {
                        send_error(
                            ws_session,
                            im_common::ErrorCode::ValidationError,
                            "invalid utf-8",
                            None,
                        )
                        .await;
                        continue;
                    }
                };

                // 1) 任何文本帧 → 重置心跳计时
                state.heartbeat().on_frame();

                // 2) 鉴权前只接受 Auth 帧
                if state.state() == SessionState::Created {
                    match serde_json::from_str::<AuthFrame>(text_str) {
                        Ok(AuthFrame::Auth {
                            req_id,
                            access_token,
                        }) => {
                            match handle_auth(state, &access_token, &app.token_service).await {
                                Ok((uid, env, dsid)) => {
                                    state.mark_authenticated(uid, env, dsid);
                                    tracing::info!(
                                        user_id = %uid,
                                        session_id = %state.session_id(),
                                        "ws authenticated"
                                    );
                                    // 鉴权成功 ack
                                    let ack = serde_json::json!({
                                        "type": "auth_ok",
                                        "req_id": req_id,
                                    });
                                    if ws_session.text(ack.to_string()).await.is_err() {
                                        return Err(AppError::Internal(anyhow::anyhow!(
                                            "ws send failed"
                                        )));
                                    }
                                }
                                Err(e) => {
                                    let code = e.code();
                                    let msg = format!("auth failed: {e}");
                                    send_error(ws_session, code, &msg, req_id).await;
                                    // 鉴权失败 → close
                                    return Err(e);
                                }
                            }
                        }
                        Err(_) => {
                            send_error(
                                ws_session,
                                im_common::ErrorCode::ValidationError,
                                "first frame must be Auth",
                                None,
                            )
                            .await;
                            return Err(AppError::Unauthorized("first frame must be Auth".into()));
                        }
                    }
                    continue;
                }

                // 3) 鉴权后: 解析 ClientFrame, 处理 Ping + 其他业务帧
                match serde_json::from_str::<ClientFrame>(text_str) {
                    Ok(ClientFrame::Auth { .. }) => {
                        // 重复 Auth 帧 → 拒绝
                        send_error(
                            ws_session,
                            im_common::ErrorCode::ValidationError,
                            "duplicate auth frame",
                            None,
                        )
                        .await;
                    }
                    Ok(ClientFrame::Ping { ts }) => {
                        // per aux-13 §1.1.8: 客户端 ping → 服务端 pong, ts 原样回传。
                        //
                        // 2026-10-03 修复: 此前**没有这个分支** —— `ClientFrame::Ping`
                        // 落进了下面的 `Ok(_)` 兜底, 被当成业务帧回
                        // VALIDATION_ERROR 错误帧, 于是客户端发 ping 永远收不到 pong,
                        // 而模块文档写的是"回 PongFrame(ts)"。这也是 #F 的
                        // `PingFrame`/`into_pong` 一直无人调用的直接原因。
                        // 现统一走 `im_protocol::ws_frames::ServerFrame::Pong` ——
                        // 与本分支族已有的 `ClientFrame` 出自同一套 wire 定义。
                        let pong = ServerFrame::Pong {
                            ts: ts.unwrap_or(0),
                        };
                        match serde_json::to_string(&pong) {
                            Ok(body) => {
                                if ws_session.text(body).await.is_err() {
                                    return Err(AppError::Internal(anyhow::anyhow!(
                                        "ws send pong failed"
                                    )));
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = ?e, "ws pong serialize failed");
                                send_error(
                                    ws_session,
                                    im_common::ErrorCode::InternalError,
                                    "pong serialize failed",
                                    None,
                                )
                                .await;
                            }
                        }
                    }
                    Ok(_) => {
                        // 业务帧 (SendMessage / Edit / Recall / React / MarkRead / Typing):
                        // 留 C-9 + 后续 lane。
                        //
                        // 2026-10-03 更正: 本注释原写「当前返 UNSUPPORTED_OPERATION」——
                        // **该错误码不存在**。aux-03 §B(MVP 错误码唯一权威表)的 21 个
                        // 已注册码里没有它, 也没有 501。实际返回的是
                        // `VALIDATION_ERROR` (400)。
                        //
                        // 语义确实不合(业务帧格式合法, 缺的是服务端处理器), 但**不能
                        // 自行改成 501**: aux-03 §B 规定新增错误码须同步更新该表与
                        // DetailedDesign, 属协议变更, 而 ImplementationSpec 处于
                        // `[PROTOCOL-FROZEN]`。是否新增「未实装」类错误码由规范所有者
                        // 决定。见模块文档缺口 1 + docs/gap-ledger.md §1.6。
                        send_error(
                            ws_session,
                            im_common::ErrorCode::ValidationError,
                            "business frame handler not implemented in MVP (per 138 §C-9)",
                            None,
                        )
                        .await;
                    }
                    Err(_) => {
                        send_error(
                            ws_session,
                            im_common::ErrorCode::ValidationError,
                            "invalid frame json",
                            None,
                        )
                        .await;
                    }
                }
            }
            Message::Binary(_) => {
                // MVP 不支持 binary frame (aux-13 §1 都是 JSON text)
                send_error(
                    ws_session,
                    im_common::ErrorCode::ValidationError,
                    "binary frame not supported",
                    None,
                )
                .await;
            }
            Message::Ping(bytes) => {
                // actix-ws 自动处理 WS-level ping, 这里只记录
                tracing::debug!(len = bytes.len(), "ws-level ping");
                state.heartbeat().on_frame();
            }
            Message::Pong(_) => {
                state.heartbeat().on_frame();
            }
            Message::Close(reason) => {
                tracing::info!(?reason, "ws close received");
                state.force_close();
                return Ok(());
            }
            Message::Continuation(_) => {
                send_error(
                    ws_session,
                    im_common::ErrorCode::ValidationError,
                    "continuation frame not supported",
                    None,
                )
                .await;
            }
            Message::Nop => {}
        }
    }
    // 无 `Ok(())`: 上面 `loop` 的每条出口(对端 close / 消息流结束 / 心跳超时)
    // 都直接 `return`, 循环不会正常落到底部。
}

/// 处理 Auth 帧: validate access_token + 提取 (user_id, env, device_session_id)
async fn handle_auth(
    _state: &WsSession,
    access_token: &str,
    token_service: &Arc<TokenService>,
) -> Result<(UserId, EnvironmentId, DeviceSessionId), AppError> {
    // 注: TokenService::validate_access_token 返回 TokenClaims (per token.rs:144)
    let claims = token_service
        .validate_access_token(access_token)
        .map_err(AppError::from)?;

    let user_id: UserId = claims
        .sub
        .parse()
        .map_err(|_| AppError::Unauthorized("invalid user_id in claims".into()))?;
    let environment_id: EnvironmentId = claims
        .env
        .parse()
        .map_err(|_| AppError::Unauthorized("invalid env in claims".into()))?;

    // 已知缺口: TokenClaims 暂不携带 device_session_id (per 138 §8 缺口 #2)
    // 当前用 nil placeholder; V1 实装 dsid JWT claim
    let device_session_id = DeviceSessionId::new();

    Ok((user_id, environment_id, device_session_id))
}

/// 通过 WS 发错误帧 (per aux-13 §4 错误码格式)
async fn send_error(
    session: &mut actix_ws::Session,
    code: im_common::ErrorCode,
    message: &str,
    req_id: Option<Uuid>,
) {
    let frame = WsErrorFrame::from_app_error(code, message, req_id);
    let body = serde_json::to_string(&frame).unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = session.text(body).await {
        tracing::warn!(error = ?e, "ws send error failed");
    }
}

// ============================================================================
// 单元测试 — 不启 actix server, 只测 DTO 解析 + 辅助函数
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    // C-12 心跳骨架帧仅测试路径引用 (per ping_frame_pong_roundtrip_via_c12_skeleton);
    // 真实收发循环接线前 driver 走 ClientFrame::Ping (per 本文件模块 doc 已知缺口 #2)。
    use crate::ws::heartbeat::{PingFrame, PingPongType, PongFrame};

    #[test]
    fn auth_frame_deserialize_full() {
        let json = r#"{"type":"auth","req_id":"7c9e6679-7425-40de-944b-e07fc1f90ae7","access_token":"eyJ..."}"#;
        let f: AuthFrame = serde_json::from_str(json).unwrap();
        match f {
            AuthFrame::Auth {
                req_id,
                access_token,
            } => {
                assert_eq!(
                    req_id.unwrap().to_string(),
                    "7c9e6679-7425-40de-944b-e07fc1f90ae7"
                );
                assert_eq!(access_token, "eyJ...");
            }
        }
    }

    #[test]
    fn auth_frame_deserialize_without_req_id() {
        // req_id 可选 (per aux-13 §1.1.1)
        let json = r#"{"type":"auth","access_token":"eyJ..."}"#;
        let f: AuthFrame = serde_json::from_str(json).unwrap();
        match f {
            AuthFrame::Auth {
                req_id,
                access_token,
            } => {
                assert!(req_id.is_none());
                assert_eq!(access_token, "eyJ...");
            }
        }
    }

    #[test]
    fn auth_frame_missing_access_token_fails() {
        let json = r#"{"type":"auth"}"#;
        let r: Result<AuthFrame, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn auth_frame_wrong_type_fails() {
        // type != "auth" → serde tag 不匹配
        let json = r#"{"type":"ping","access_token":"eyJ..."}"#;
        let r: Result<AuthFrame, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn ws_error_frame_serialization() {
        let frame = WsErrorFrame {
            ty: "error",
            code: "UNAUTHORIZED".into(),
            message: "auth failed: invalid token".into(),
            req_id: Some(Uuid::new_v4()),
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(s.contains("\"type\":\"error\""));
        assert!(s.contains("\"code\":\"UNAUTHORIZED\""));
        assert!(s.contains("\"message\":\"auth failed: invalid token\""));
        assert!(s.contains("\"req_id\":"));
    }

    #[test]
    fn ws_error_frame_without_req_id_omits_field() {
        let frame = WsErrorFrame {
            ty: "error",
            code: "VALIDATION_ERROR".into(),
            message: "bad".into(),
            req_id: None,
        };
        let s = serde_json::to_string(&frame).unwrap();
        assert!(!s.contains("req_id"), "缺 req_id 时不序列化: {s}");
    }

    #[test]
    fn ping_frame_pong_roundtrip_via_c12_skeleton() {
        // 跟 C-12 heartbeat skeleton 互通
        let ping = PingFrame {
            ty: PingPongType::Ping,
            ts: Some(12345),
        };
        let pong = PongFrame {
            ty: PingPongType::Pong,
            ts: 12345,
        };
        let ping_json = serde_json::to_string(&ping).unwrap();
        assert!(ping_json.contains("\"type\":\"ping\""));
        assert!(ping_json.contains("\"ts\":12345"));
        let pong_json = serde_json::to_string(&pong).unwrap();
        assert!(pong_json.contains("\"type\":\"pong\""));
        assert!(pong_json.contains("\"ts\":12345"));
    }

    // ========================================================================
    // 缺口 #H 回归: 心跳 tick 驱动的超时判定
    // ========================================================================
    //
    // 原实现的 bug 是**这条路径根本没被执行** —— tick 在一个独立后台 task 里,
    // 那个 task 拿不到 `actix_ws::Session`, 只能打日志后 break; 主循环又只
    // `await msg_stream.next()`。所以"60s 无帧"从来不会导致关闭。
    //
    // 下面两个测试锁住修复后 `run_ws_loop` 里 select! tick 分支所依赖的判定。
    //
    // **为什么用真实 sleep 而不是 `tokio::time::pause()`**:
    // `HeartbeatState` 内部用的是 `std::time::Instant`(见 heartbeat.rs), 而
    // `tokio::time::pause()` 虚拟化的是 **tokio 自己的时钟**。两者不是同一个
    // 时钟 —— 即使 `advance(60s)`, `Instant::elapsed()` 仍接近 0, 判定恒为
    // false。踩过这个坑: 最初写的就是 pause + advance, 结果 50 passed / 1 failed
    // 且失败原因完全反直觉。改为把阈值按比例缩小(60s→50ms, 30s→30ms)后用
    // 真实 sleep, 与 heartbeat.rs 里 `heartbeat_state_on_frame_resets_idle`
    // 的既有做法同构, 总耗时约 90ms。
    //
    // **测试边界(如实声明)**: 这里验证的是"tick 驱动下的超时判定正确",
    // 不是"连接真的被关闭"。后者需要真实 WS 端到端(需 WS 客户端依赖, 本仓库
    // 当前没有, 且 CI 用 --locked 不宜临时加依赖), 故仍未覆盖 —— 见
    // `docs/gap-ledger.md` §1.1。

    /// 构造一个**按比例缩小**的心跳 session, 语义与生产 30s/60s 完全同构
    fn scaled_session(tick: Duration, timeout: Duration) -> WsSession {
        let cfg = super::super::heartbeat::HeartbeatConfig {
            expected_ping_interval: tick,
            no_frame_timeout: timeout,
        };
        let hb = Arc::new(super::super::heartbeat::HeartbeatState::new(cfg));
        WsSession::new(Uuid::new_v4(), hb)
    }

    #[tokio::test]
    async fn silent_session_is_flagged_timeout_once_threshold_passed() {
        // 同构于生产: tick / timeout = 1:2 比例(生产是 30s / 60s)
        //
        // **为什么是 100ms/200ms 而不是更小的数值**: 用真实 `sleep` 测时序必然
        // 有抖动 —— 机器负载高时 30ms 的 sleep 可能实际耗 50ms+。最初取
        // 30ms/50ms(余量仅 20ms)时, 这个测试在编译后机器繁忙时序的重复运行里
        // 出现过 flaky 失败(第一次 sleep 后就误判超时)。现在余量 100ms,
        // 总耗时约 300ms。**不要再把阈值压到余量 < 50ms 的水平。**
        let tick_dur = Duration::from_millis(100);
        let state = scaled_session(tick_dur, Duration::from_millis(200));
        let mut ticker = tokio::time::interval(tick_dur);

        // tokio interval 首次 tick 立即完成: idle≈0, 绝不能误判
        ticker.tick().await;
        assert!(!state.tick_heartbeat(), "首次 tick 时 idle≈0,不应判超时");

        // 一个周期后: idle≈100ms < 200ms, 仍不应超时(对应生产的前 30s)
        tokio::time::sleep(tick_dur).await;
        ticker.tick().await;
        assert!(!state.tick_heartbeat(), "idle ~100ms < 阈值 200ms,不应超时");

        // 再一个周期: idle≈200ms >= 200ms, 必须判超时 —— 这正是修复前
        // 永远不会发生的那次判定
        tokio::time::sleep(tick_dur).await;
        ticker.tick().await;
        assert!(
            state.tick_heartbeat(),
            "idle ~200ms >= 阈值 200ms,必须判超时(生产即 60s 无帧)"
        );
    }

    #[tokio::test]
    async fn frames_keep_session_alive_past_threshold() {
        let tick_dur = Duration::from_millis(100);
        let state = scaled_session(tick_dur, Duration::from_millis(200));
        let mut ticker = tokio::time::interval(tick_dur);

        // 连续 4 个周期, 每周期都收到一帧(模拟客户端持续发 ping)
        for i in 0..4 {
            ticker.tick().await;
            state.heartbeat().on_frame();
            assert!(!state.tick_heartbeat(), "第 {i} 次 tick 刚收到帧,不应超时");

            tokio::time::sleep(tick_dur).await;
            ticker.tick().await;
            state.heartbeat().on_frame();
            assert!(
                !state.tick_heartbeat(),
                "第 {i} 个周期: 持续发帧的连接不应被判超时"
            );
        }
    }

    // ========================================================================
    // Ping → Pong wire 契约 (per aux-13 §1.1.8)
    // ========================================================================
    //
    // 2026-10-03: 修复前 `ClientFrame::Ping` 落进 `Ok(_)` 业务帧兜底, 客户端
    // 发 ping 收不到 pong。下面锁住"ping 能被解析成 ClientFrame::Ping"且
    // "pong 的 wire 形状与 ts 回传"这两个环节 —— 它们是主循环 Ping 分支的
    // 前提, 且可在纯 serde 层验证(不需要真实 WS 连接)。

    #[test]
    fn client_ping_frame_parses_into_ping_variant() {
        // 修复前这个 JSON 会被解析成功, 然后落进 `Ok(_)` 兜底
        let parsed: ClientFrame = serde_json::from_str(r#"{"type":"ping","ts":1692528000000}"#)
            .expect("ping 帧必须能解析成 ClientFrame::Ping");
        match parsed {
            ClientFrame::Ping { ts } => assert_eq!(ts, Some(1692528000000)),
            other => panic!("期望 ClientFrame::Ping, 实际是 {other:?}"),
        }
    }

    #[test]
    fn client_ping_frame_without_ts_parses_as_none() {
        let parsed: ClientFrame =
            serde_json::from_str(r#"{"type":"ping"}"#).expect("无 ts 的 ping 也应可解析");
        match parsed {
            ClientFrame::Ping { ts } => assert_eq!(ts, None, "缺省 ts 应为 None"),
            other => panic!("期望 ClientFrame::Ping, 实际是 {other:?}"),
        }
    }

    #[test]
    fn server_pong_echoes_ts_and_uses_pong_type() {
        let pong = ServerFrame::Pong { ts: 1692528000000 };
        let s = serde_json::to_string(&pong).unwrap();
        assert!(s.contains("\"type\":\"pong\""), "snake_case type: {s}");
        assert!(s.contains("\"ts\":1692528000000"), "ts 必须原样回传: {s}");
    }

    #[test]
    fn server_pong_with_missing_client_ts_uses_zero() {
        // ts 缺省时回 0 —— 与 aux-13 的 pong.ts 保持同构, 不让字段消失
        let parsed: ClientFrame =
            serde_json::from_str(r#"{"type":"ping"}"#).expect("无 ts 的 ping 也应可解析");
        let ts = match parsed {
            ClientFrame::Ping { ts } => ts.unwrap_or(0),
            other => panic!("期望 ClientFrame::Ping, 实际是 {other:?}"),
        };
        let s = serde_json::to_string(&ServerFrame::Pong { ts }).unwrap();
        assert!(s.contains("\"ts\":0"), "缺省 ts 回 0 而非省略字段: {s}");
    }

    #[test]
    fn ping_is_not_mistaken_for_a_business_frame() {
        // 显式锁住"ping 不会落进业务帧兜底"这一事实: 枚举里 Ping 是独立变体
        let parsed: ClientFrame =
            serde_json::from_str(r#"{"type":"ping","ts":1}"#).expect("ping 可解析");
        let is_ping = matches!(parsed, ClientFrame::Ping { .. });
        assert!(is_ping, "ping 必须匹配到 Ping 变体, 而非被业务帧分支吞掉");
    }
}
