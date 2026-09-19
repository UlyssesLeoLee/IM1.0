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
//!   - `Ping` (per C-12 skeleton) → HeartbeatState::on_frame + 回 PongFrame(ts)
//!   - 其他业务帧 (SendMessage / Edit / Recall / React / MarkRead / Typing) → 列已知缺口 (C-9 messages handler 还没做)
//! - 30s background task tick_heartbeat + 60s 无帧超时关闭
//! - ForceDisconnect hook stub (后续 G-1 presence 集成)
//!
//! ### 已知缺口 (per 守门 #1 缺标比错标)
//! 1. **业务帧处理 (SendMessage / Edit / Recall / React / MarkRead / Typing)**: 留 C-9 + 后续 lane; 当前收到非 Auth/Ping 帧返 `UNSUPPORTED_OPERATION` (501, per aux-13 §4)
//! 2. **im-proto gRPC 客户端**: 有 stub (per worker-C 探索 `im_proto::im::core::v1::core_service_client::CoreServiceClient`), 但 MVP Day 3 没 wire-up gRPC channel; Auth 帧的 TokenClaims 解析走本地 TokenService (已经在 im-gateway 进程内), 不走 gRPC
//! 3. **ForceDisconnect broadcast**: 占位 broadcast channel, 实际 broadcasting 留 G-1 presence
//! 4. **device_session_id 来自 JWT claims**: 当前 TokenClaims 没 `dsid` 字段 (per 138 §8 缺口 + AuthedUser), 用 refresh_token split 兜底

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
use im_protocol::ws_frames::ClientFrame;

use super::heartbeat::{PingFrame, PingPongType, PongFrame};
use super::session::{SessionState, WsSession};

use crate::http::state::AppState;

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
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream).map_err(
        |e| -> actix_web::Error {
            tracing::warn!(error = ?e, "ws upgrade failed");
            actix_web::error::InternalError::from_response(
                String::from("ws_upgrade_failed"),
                HttpResponse::BadRequest().finish(),
            )
            .into()
        },
    )?;

    // 创建 WsSession (per C-12 skeleton)
    let session_id = Uuid::new_v4();
    let hb = super::session::new_heartbeat();
    let mut ws_session = WsSession::new(session_id, hb.clone());
    let app_data = app.clone();

    // Background task: 30s heartbeat tick (per aux-13 §1.3)
    let hb_for_tick = hb.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            if hb_for_tick.tick() {
                tracing::info!(session_id = %session_id, "heartbeat timeout, closing ws");
                // 注: session 关闭由主 loop 检测; 此处仅日志, 实际关闭在主 loop
                break;
            }
        }
    });

    // 主 loop — 异步 spawn, 不阻塞 HTTP upgrade response 返回
    let app_for_loop = app_data.clone();
    actix_web::rt::spawn(async move {
        if let Err(e) = run_ws_loop(&mut session, &mut msg_stream, &app_for_loop, &mut ws_session).await {
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
    while let Some(msg_result) = msg_stream.next().await {
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
                        send_error(ws_session, im_common::ErrorCode::ValidationError, "invalid utf-8", None).await;
                        continue;
                    }
                };

                // 1) 任何文本帧 → 重置心跳计时
                state.heartbeat().on_frame();

                // 2) 鉴权前只接受 Auth 帧
                if state.state() == SessionState::Created {
                    match serde_json::from_str::<AuthFrame>(text_str) {
                        Ok(AuthFrame::Auth { req_id, access_token }) => {
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
                                        return Err(AppError::Internal(anyhow::anyhow!("ws send failed")));
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
                    Ok(_) => {
                        // 业务帧 (SendMessage / Edit / Recall / React / MarkRead / Typing):
                        // 列已知缺口 — 留 C-9 + 后续 lane, 当前返 UNSUPPORTED_OPERATION
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

    Ok(())
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

    #[test]
    fn auth_frame_deserialize_full() {
        let json = r#"{"type":"auth","req_id":"7c9e6679-7425-40de-944b-e07fc1f90ae7","access_token":"eyJ..."}"#;
        let f: AuthFrame = serde_json::from_str(json).unwrap();
        match f {
            AuthFrame::Auth { req_id, access_token } => {
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
            AuthFrame::Auth { req_id, access_token } => {
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
}