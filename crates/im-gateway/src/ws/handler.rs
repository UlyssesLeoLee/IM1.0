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
//! 1. **业务帧处理** —— **2026-10-03 已实装 3/6**:
//!    - `SendMessage`: 走完整 `MessageService::send_message` 校验链
//!      (幂等 / content schema / conversation member / 大小上限),
//!      回 aux-13 §1.2.2 的 `ack`, 并经 `WsHub` 广播 `message_new`。
//!    - `EditMessage`: 走 `MessageService::edit_message`
//!      (仅原 sender / 撤回与删除态不可编辑 / 大小 / content schema)。
//!    - `RecallMessage`: 走 `MessageService::recall_message`
//!      (per aux-04 §B.4 转换表: 仅原 sender / `sent`·`delivered`·`read`
//!      三态可撤 / 终态拒绝 / 时间窗 `≤` 判定), 并广播 `message_recalled`
//!      —— 该帧带 `conversation_id`, 是**当前唯一可安全广播的变更类帧**。
//!
//!    仍**未实装 3 类**: `React` / `MarkRead` / `Typing` ——
//!    收到即回 `VALIDATION_ERROR`(带 req_id)。错误码语义不理想(帧格式合法,
//!    缺的是服务端处理器), 但**不新造错误码**, 理由见下。

//!
//!    **2026-10-03 更正 —— 以下说明本条原注释为何不可照抄**:
//!    - 原写「返 `UNSUPPORTED_OPERATION` (501, per aux-13 §4)」。**该错误码不存在**:
//!      `aux-03-error-code-registry.md` §B 是 MVP 错误码的**唯一权威表**, 列出 21 个
//!      已注册码, 与 `im_common::ErrorCode` 枚举逐项一致, 其中**没有**
//!      `UNSUPPORTED_OPERATION`, 也没有 501。
//!    - 原引用的 `aux-13 §4` 是**「前置条件 (Prerequisites)」**章节, 不是错误码章节 ——
//!      出处本身也不成立。
//!    - **为何不自行改成 501**: `aux-03 §B` 写明「任何 PR 增加必须同时更新本表与
//!      DetailedDesign」, 即新增错误码是**协议变更**; 而 ImplementationSpec 处于
//!      `[PROTOCOL-FROZEN]`。是否新增「未实装」类错误码属规范所有者的决定。已记入
//!      `docs/gap-ledger.md` §1.6。
//! 2. **im-proto gRPC 客户端**: 有 stub (per worker-C 探索 `im_proto::im::core::v1::core_service_client::CoreServiceClient`), 但 MVP Day 3 没 wire-up gRPC channel; Auth 帧的 TokenClaims 解析走本地 TokenService (已经在 im-gateway 进程内), 不走 gRPC
//!
//! 已于 2026-10-03 结清:
//! - 曾列为缺口 3 的「ForceDisconnect broadcast: 占位 broadcast channel」——
//!   该「占位物」**其实根本不存在**(文档说有、代码没有)。现已实装 `ws::hub::WsHub`:
//!   单个 `broadcast` 通道, `main.rs` 以 `web::Data` 注入为进程内单例。
//! - 曾列为缺口 4 的「`ServerFrame` 缺 `message_new` 变体」—— 已补
//!   `ServerFrame::MessageNew { message: WireMessage }`(aux-13 §1.2.5),
//!   并由 `handle_send_message` 在**新落库**后经 `WsHub::publish` 广播。
//!   投递范围由 `ws::hub::Audience` 判定, 其中「不带 conversation_id 的帧」
//!   (MessageEdited / ReactionAdded) 与「单连接响应」(Ack / Connected / Pong)
//!   判为 `Undeliverable` **一律不发**, 避免跨会话泄漏。
//! - 曾列为缺口 5 的「60s 无帧超时未真正关闭连接」已修复 —— tick 搬进
//!   `run_ws_loop` 的 `select!`, 超时即退出主循环并走 close。
//! - 曾列为缺口 6 的「device_session_id 来自 JWT claims / TokenClaims 没 `dsid` 字段」
//!   已修复 —— 见 `docs/gap-ledger.md` §1.5 (C-7 logout 实装)。
//! - 鉴权成功后回的是 `{"type":"auth_ok"}`, **该帧类型在 aux-13 中不存在**
//!   (§1.2.1 规定的是 `connected`)。已记入 `docs/gap-ledger.md` §2, 未擅改 ——
//!   改它会变动客户端可见的 wire 形状, 与 §1.7.3 的 `ack` 形状对齐性质不同。

use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::{CloseCode, CloseReason, Message};
use futures::StreamExt;
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio::time::interval;
use uuid::Uuid;

use im_common::ids::{ConversationId, DeviceSessionId, EnvironmentId, MessageId, UserId};
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

/// 构造 aux-13 §1.2.4 形状的失败响应帧
///
/// **纯函数, 无 IO** —— `send_error`(线上) 与测试断言**共用这一条路径**。
/// 此前本文件有一个 `WsErrorFrame` struct 专供 `send_error` 使用, 另有一个
/// 测试直接断言它; 若保留两份构造逻辑, 测试就会验证一个线上并不产生的形状 ——
/// 那正是本轮反复在堵的那类错(验证对象与被测对象不同源)。现只保留这一个。
///
/// 2026-10-03: 此前发的是**另造**的顶层 `{"type":"error", code, message, req_id}`
/// 帧, 与规范形状不一致。根因是 `ServerFrame::Ack` 只有 `ok: bool` 却挂不了
/// 错误载荷。现已给 `ServerFrame::Ack` 补 `error` 字段(见 gap-ledger §1.7.3)。
///
/// `req_id = None`(非法 JSON / binary 帧 / continuation 帧等协议级错误, 解析不出
/// 请求)统一用 `Uuid::nil()` 占位, 表示「无可关联的请求」。
fn error_ack_frame(code: im_common::ErrorCode, message: &str, req_id: Option<Uuid>) -> ServerFrame {
    ServerFrame::Ack {
        req_id: req_id.unwrap_or_else(Uuid::nil),
        ok: false,
        data: None,
        error: Some(im_protocol::error_body::ErrorBody::new(
            code.as_str(),
            message,
            "",
        )),
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
    hub: web::Data<super::hub::WsHub>,
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
    let hub_for_loop = hub.clone();
    actix_web::rt::spawn(async move {
        if let Err(e) = run_ws_loop(
            &mut session,
            &mut msg_stream,
            &app_for_loop,
            &hub_for_loop,
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
    hub: &super::hub::WsHub,
    state: &mut WsSession,
) -> Result<(), AppError> {
    // 2026-10-03 修(缺口 #H): 心跳 tick 原先放在一个独立后台 task 里, 但
    // 那个 task 只 `tracing::info!` + `break` —— `actix_ws::Session` 归本主循环
    // 所有, 后台 task 拿不到, 信号也无处可送; 而本循环原先只 `await
    // msg_stream.next()`, 没有超时分支。两者叠加的结果是 **60s 无帧超时
    // 永远不会关闭连接**, 半开连接一直堆积到 TCP 超时。现把 interval 搬进
    // 主循环用 select! 直接判定并退出(退出后由 ws_handler 统一走 close)。
    let mut tick = interval(HEARTBEAT_TICK);

    // 广播订阅**必须在进入循环前**完成: broadcast 通道只向「订阅之后」的
    // 接收者投递, 若在循环内首次收到帧时才订阅, 连接建立到进入循环之间的
    // 广播会被静默漏掉。
    let mut hub_rx = hub.subscribe();

    loop {
        // biased: 有帧时优先处理帧。客户端持续发帧时 tick 分支不会命中是**正确**
        // 的 —— 每帧都会 `heartbeat().on_frame()` 重置 idle, 本就不会超时;
        // 只有客户端停发帧时 msg 才不再 ready, select 才落到 tick 分支判超时。
        let msg_result = tokio::select! {
            biased;
            m = msg_stream.next() => m,
            // tick 排在广播**之前**是刻意的: biased 按书写顺序轮询, 若广播在前,
            // 通道里一旦有积压帧就会每次都命中广播分支, tick 永远轮不到 ——
            // 广播洪水能把心跳超时判定饿死, 正是本函数上一轮修掉的那类漏洞。
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
            b = hub_rx.recv() => {
                match b {
                    Ok(frame) => {
                        deliver_broadcast(ws_session, state, frame).await?;
                    }
                    // Lagged: 慢客户端漏掉了最旧的若干帧。这里**只记不补** ——
                    // 补齐要按 conversation 逐个拉 REST, 属于另一个量级的逻辑。
                    // 但必须留痕: 静默跳过会让客户端以为「对方没发言」。
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            session_id = %state.session_id(),
                            skipped,
                            "ws broadcast lagged; client must resync via GET /v1/conversations/{{id}}/messages"
                        );
                    }
                    // Closed: hub 已被销毁, 不会再有任何广播。这条连接已失去意义,
                    // 继续留着只会变成一个永远静默的连接。
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::warn!(
                            session_id = %state.session_id(),
                            "ws hub closed, terminating connection"
                        );
                        state.force_close();
                        return Ok(());
                    }
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
                                    // 广播成员集合在**回 auth_ok 之前**加载, 且失败
                                    // 即拒绝连接。
                                    //
                                    // 反过来做(先回 ack 再加载、失败只记日志)的后果是:
                                    // 连接鉴权通过、能收发自己的消息, 但**收不到任何
                                    // 别人的消息**, 且**没有任何报错** —— 用户只会
                                    // 以为对方没发言。这种故障比连不上难查得多。
                                    //
                                    // 用 `list_membership_ids` 而非
                                    // `list_user_conversations`: 后者 limit 被 clamp
                                    // 到 50 且 cursor 被忽略, 加入超过 50 个会话的
                                    // 用户会**静默漏收**老会话的广播。
                                    let conv_ids = match app
                                        .conversation_service
                                        .list_membership_ids(uid)
                                        .await
                                    {
                                        Ok(ids) => ids,
                                        Err(e) => {
                                            let code = e.code();
                                            let msg = format!("membership load failed: {e}");
                                            send_error(ws_session, code, &msg, req_id).await;
                                            return Err(e);
                                        }
                                    };

                                    state.mark_authenticated(uid, env, dsid);
                                    state.set_conversation_ids(super::hub::membership_set(
                                        conv_ids.iter(),
                                    ));
                                    tracing::info!(
                                        user_id = %uid,
                                        session_id = %state.session_id(),
                                        conversations = state.conversation_count(),
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
                    // C-9 已实装: send_message 走完整 MessageService 校验链
                    Ok(frame @ ClientFrame::SendMessage { .. }) => {
                        // sender 来自**已鉴权的会话状态**, 不取自帧内容 ——
                        // 客户端没有资格声明自己是谁。
                        match state.user_id() {
                            Some(uid) => {
                                handle_send_message(ws_session, app, hub, uid, frame).await
                            }
                            None => {
                                send_error(
                                    ws_session,
                                    im_common::ErrorCode::Unauthorized,
                                    "not authenticated",
                                    None,
                                )
                                .await
                            }
                        }
                    }
                    // C-9 已实装: edit_message
                    Ok(frame @ ClientFrame::EditMessage { .. }) => match state.user_id() {
                        Some(uid) => handle_edit_message(ws_session, app, uid, frame).await,
                        None => {
                            send_error(
                                ws_session,
                                im_common::ErrorCode::Unauthorized,
                                "not authenticated",
                                None,
                            )
                            .await
                        }
                    },
                    // C-9 已实装: recall_message (per aux-04 §B.4)
                    Ok(frame @ ClientFrame::RecallMessage { .. }) => match state.user_id() {
                        Some(uid) => {
                            handle_recall_message(ws_session, app, hub, state, uid, frame).await
                        }
                        None => {
                            send_error(
                                ws_session,
                                im_common::ErrorCode::Unauthorized,
                                "not authenticated",
                                None,
                            )
                            .await
                        }
                    },
                    // 其余 3 类业务帧仍逐一显式列出, 不用 `Ok(_)` 兜底。两个理由:
                    //
                    // (1) req_id 必须回传。aux-13 §1.2.4 规定 error 帧带 req_id,
                    //     客户端据此把失败响应关联回自己的请求。此前这里传 `None`,
                    //     客户端拿到一个「无主」的错误帧 —— 同一连接上并发多个
                    //     请求时无法知道是哪一个失败了。
                    //
                    // (2) 显式 or-pattern 让本 match 变**穷尽**: 未来给
                    //     `ClientFrame` 加变体会在此处编译报错, 而不会被 `Ok(_)`
                    //     静默吞掉。ping 漏洞(gap-ledger §1.2)就是这么藏的 ——
                    //     `ClientFrame::Ping` 解析成功, 却落进了 `Ok(_)`。
                    Ok(
                        ClientFrame::React { req_id, .. }
                        | ClientFrame::MarkRead { req_id, .. }
                        | ClientFrame::Typing { req_id, .. },
                    ) => {
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
                            "frame handler not implemented in MVP (per 138 §C-9)",
                            Some(req_id),
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

    // 2026-10-03 修复(此前是个**看起来像真的**假值):
    // 此前这里写 `let device_session_id = DeviceSessionId::new();` —— 凭空生成
    // 一个随机 UUID, 而 `device_sessions` 表里**没有对应行**。该值随后经
    // `mark_authenticated` 存进 `WsSession.device_session_id`, 于是 WS 会话
    // 持有一个永远匹配不上任何真实会话的 id: 将来任何「按 device session 强制
    // 下线」的逻辑都会指向一个不存在的对象, 且**不会报错**。
    //
    // 注释写的是「当前用 nil placeholder」, 但代码生成的是随机 UUID —— 注释
    // 反而**掩盖了**危险: `Uuid::nil()` 一眼看出是假的, 随机 UUID 看着正常。
    //
    // 现读 `dsid` claim(见 gap-ledger §1.5, C-7 logout 实装时加入 TokenClaims)。
    // 缺失时**拒绝**而非编造: WS 是长连接, 一个无法被吊销的鉴权通道正是
    // §1.5 关掉的那个安全缺口。缺 dsid 的 token 是 dsid 字段加入前签发的,
    // 有效期 ≤15min, 让客户端重新登录即可。
    let device_session_id: DeviceSessionId = claims
        .dsid
        .as_deref()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            AppError::Unauthorized(
                "access token carries no device session (dsid); re-authenticate".into(),
            )
        })?;

    Ok((user_id, environment_id, device_session_id))
}

/// C-9 业务帧: `send_message` (per aux-13 §1.1.2)
///
/// 逻辑与 REST `http::messages::send_message` **刻意保持一致**(同一条
/// `MessageService::send_message` 校验链: 幂等 / content schema / conversation
/// member / 大小上限), 差别只在响应形状: REST 返 201 + body, WS 返
/// `ServerFrame::Ack { ok: true, data }`。
///
/// 2026-10-03 实装 —— 此前本帧落进 `Ok(_)` 兜底, 回一个「未实装」错误。
async fn handle_send_message(
    ws_session: &mut actix_ws::Session,
    app: &web::Data<AppState>,
    hub: &super::hub::WsHub,
    sender_id: UserId,
    frame: ClientFrame,
) {
    let ClientFrame::SendMessage {
        req_id,
        conversation_id,
        idempotency_key,
        kind,
        content,
        reply_to,
    } = frame
    else {
        unreachable!("调用方保证传入 SendMessage 变体");
    };

    // 幂等预判: 命中则回 `idempotent_replay: true`, 不重复发事件(per
    // aux-13 §1.2.3)。见 MessageService::find_by_idempotency_key 的偏差说明。
    let key = idempotency_key.to_string();
    match app
        .message_service
        .find_by_idempotency_key(ConversationId(conversation_id), sender_id, &key)
        .await
    {
        Ok(Some(existing)) => {
            // **刻意不广播**: 命中幂等说明这条消息在更早的请求里已落库, 那时已经
            // 广播过。客户端用同一个 idempotency_key 重试(网络超时后很常见)若
            // 再广播一次, 所有其它在线端都会收到**同一条消息的第二个副本** ——
            // 而它们根本没发起过这个请求, 无从去重。
            send_ack(
                ws_session,
                req_id,
                Some(existing.id.0),
                Some(existing.sequence),
                true,
            )
            .await;
            return;
        }
        Ok(None) => {}
        Err(e) => {
            send_error(
                ws_session,
                im_common::ErrorCode::InternalError,
                &format!("idempotency lookup failed: {e}"),
                Some(req_id),
            )
            .await;
            return;
        }
    }

    let cmd = im_core::message::service::SendMessageCommand {
        conversation_id: ConversationId(conversation_id),
        sender_id,
        idempotency_key: key,
        kind,
        // `ClientFrame.content` 是 `MessageContent`(强类型), 而
        // `SendMessageCommand.content` 是 `serde_json::Value` —— 序列化成
        // JSON 交给 service 侧按 kind 反序列化回强类型(该 schema 校验在
        // service 内, 见其 step 2)。
        content: serde_json::to_value(&content).unwrap_or(serde_json::Value::Null),
        reply_to: reply_to.map(MessageId),
        // 默认 65536 (per REST messages.rs 同值 + SRS §16.4); V1 从
        // environments.settings 读
        max_size_bytes: 65_536,
    };

    match app.message_service.send_message(cmd).await {
        Ok(msg) => {
            // 先 ack 再广播: 广播是同步的(非阻塞通道), 但把发送方的确认路径排在
            // 最前, 广播侧的耗时不会推迟「我发出去了」这个反馈。
            send_ack(
                ws_session,
                req_id,
                Some(msg.id.0),
                Some(msg.sequence),
                false,
            )
            .await;
            publish_new_message(hub, &msg);
        }
        Err(e) => {
            // service 侧的校验/权限错误按其 AppError 映射到已注册错误码,
            // 不再一律回「未实装」。
            let (code, detail) = map_service_error(&e);
            send_error(
                ws_session,
                code,
                &format!("{}: {detail}", "send_message failed"),
                Some(req_id),
            )
            .await;
        }
    }
}

/// C-9 业务帧: `edit_message` (per aux-13 §1.1.3)
///
/// 2026-10-03 实装。此前本帧落进 `Ok(_)` 兜底回「未实装」, 而 service 层的
/// `MessageService::edit_message` 更是做完 4 步校验后**无条件**返
/// `InternalError`(见 gap-ledger §1.9)—— 两端都没通。
///
/// sender 同样取自已鉴权的会话状态; service 内部会再校验「仅原 sender 可编辑」
/// 与「已撤回/已删除不可编辑」。
///
/// **不广播, 且这是 wire 形状的硬限制而非疏漏**: 对应的 `ServerFrame::MessageEdited`
/// (aux-13 §1.2.6) 只带 `message_id` + `content` + `edited_at`, **不带
/// `conversation_id`**。广播中枢因此无法判断接收方是不是该会话成员 ——
/// 发给所有人就是跨会话泄漏, 不发则编辑无法实时同步。补 `conversation_id`
/// 属协议变更, 不在本文件拍板范围(见 `ws::hub::Audience::Undeliverable`)。
async fn handle_edit_message(
    ws_session: &mut actix_ws::Session,
    app: &web::Data<AppState>,
    sender_id: UserId,
    frame: ClientFrame,
) {
    let ClientFrame::EditMessage {
        req_id,
        message_id,
        content,
    } = frame
    else {
        unreachable!("调用方保证传入 EditMessage 变体");
    };

    let new_content = match serde_json::to_value(&content) {
        Ok(v) => v,
        Err(e) => {
            send_error(
                ws_session,
                im_common::ErrorCode::ValidationError,
                &format!("content serialize failed: {e}"),
                Some(req_id),
            )
            .await;
            return;
        }
    };

    match app
        .message_service
        .edit_message(MessageId(message_id), sender_id, new_content, 65_536)
        .await
    {
        Ok(msg) => {
            send_ack(
                ws_session,
                req_id,
                Some(msg.id.0),
                Some(msg.sequence),
                false,
            )
            .await;
        }
        Err(e) => {
            let (code, detail) = map_service_error(&e);
            send_error(
                ws_session,
                code,
                &format!("edit_message failed: {detail}"),
                Some(req_id),
            )
            .await;
        }
    }
}

/// 把一条新落库的消息广播出去 (per aux-13 §1.2.5 `message_new`)
///
/// **任何失败都只记日志、不影响发送方**: 消息已落库、发送方已收到 ack, 此时
/// 广播失败影响的是「别人什么时候看到」, 而不是「消息有没有发出去」。让它
/// 影响发送方的 ack 是错的 —— 离线接收方本来就靠 REST 拉取。
fn publish_new_message(hub: &super::hub::WsHub, msg: &im_core::message::repository::Message) {
    let frame = match super::hub::to_wire_message(msg) {
        Ok(m) => ServerFrame::MessageNew { message: m },
        Err(e) => {
            // 库里 content 不合 schema。**不伪造**一条空消息糊弄过去 ——
            // 那会让客户端看到一条空消息而真实内容被吞掉。
            tracing::error!(
                error = %e,
                message_id = %msg.id.0,
                "message_new broadcast skipped: stored content does not match MessageContent schema"
            );
            return;
        }
    };
    let delivered = hub.publish(frame);
    if delivered == 0 {
        // 无人在线(常态): 消息已落库, 对方上线后靠 REST 补。
        tracing::debug!(
            message_id = %msg.id.0,
            "message_new broadcast: no subscribers (recipients resync via REST)"
        );
    } else {
        tracing::debug!(
            delivered,
            message_id = %msg.id.0,
            "message_new broadcast"
        );
    }
}

/// 广播帧的接收侧投递 —— 过滤规则的**唯一**执行点
///
/// 过滤判定本身在 `ws::hub::should_deliver`(纯函数, 有单测); 本函数只负责
/// 「判完之后把帧写出去」。**不在这里写过滤逻辑** —— 两处各写一遍过滤, 早晚会
/// 漂移, 而漂移的那一侧就是数据泄漏。
async fn deliver_broadcast(
    ws_session: &mut actix_ws::Session,
    state: &WsSession,
    frame: ServerFrame,
) -> Result<(), AppError> {
    let audience = super::hub::Audience::of(&frame);
    if !super::hub::should_deliver(&audience, state) {
        // 只在「不可投递」时留痕: 过滤掉绝大多数帧是正常现象(非成员), 逐帧 warn
        // 会把日志淹掉。而 Undeliverable 是**帧本身的性质问题**, 值得每次记录。
        if let super::hub::Audience::Undeliverable(why) = &audience {
            tracing::warn!(%why, "broadcast frame is not deliverable; dropped");
        }
        return Ok(());
    }
    let body = serde_json::to_string(&frame)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ws broadcast serialize: {e}")))?;
    ws_session
        .text(body)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ws broadcast send: {e}")))?;
    Ok(())
}

/// C-9 业务帧: `recall_message` (per aux-13 §1.1.5 + aux-04 §B.4)
///
/// 2026-10-03 实装。状态机与时间窗全部由 `MessageService::recall_message` 判定,
/// 本函数只负责: 解析环境 → 读真实时间窗 → 调用 → ack → 广播。
///
/// **为什么不把 120 写在这里**: aux-04 §B.4 不变量明写「撤回时间窗由
/// `environments.settings.message.recall_window_seconds` 控制, **不能写死**」。
/// 值一律经 `SettingsService` 读 `environments.settings` JSONB 取到; 该列
/// `DEFAULT '{}'`, 缺失字段由 serde default 补 120 —— 那是**配置默认**, 与
/// 代码写死是两回事。
///
/// **广播**: `MessageRecalled` 带 `conversation_id`(aux-13 §1.2.7), 因此是
/// **当前唯一可安全广播的变更类帧** —— 广播中枢能判断接收方是否成员。
/// (`MessageEdited` / `ReactionAdded` 不带, 判为 `Undeliverable`, 见 ws::hub。)
async fn handle_recall_message(
    ws_session: &mut actix_ws::Session,
    app: &web::Data<AppState>,
    hub: &super::hub::WsHub,
    state: &WsSession,
    sender_id: UserId,
    frame: ClientFrame,
) {
    let ClientFrame::RecallMessage { req_id, message_id } = frame else {
        unreachable!("调用方保证传入 RecallMessage 变体");
    };

    // 时间窗需要 environment_id —— 它来自 token claims, 记在会话状态里,
    // 不从帧内容取(客户端没有资格声明自己属于哪个环境)。
    let Some(env) = state.environment_id() else {
        send_error(
            ws_session,
            im_common::ErrorCode::Unauthorized,
            "session carries no environment",
            Some(req_id),
        )
        .await;
        return;
    };

    let window_secs = match app.settings_service.recall_window_seconds(env).await {
        Ok(v) => v,
        Err(e) => {
            let (code, detail) = map_service_error(&e);
            send_error(
                ws_session,
                code,
                &format!("recall_window lookup failed: {detail}"),
                Some(req_id),
            )
            .await;
            return;
        }
    };

    match app
        .message_service
        .recall_message(
            MessageId(message_id),
            sender_id,
            chrono::Duration::seconds(i64::from(window_secs)),
        )
        .await
    {
        Ok(msg) => {
            send_ack(
                ws_session,
                req_id,
                Some(msg.id.0),
                Some(msg.sequence),
                false,
            )
            .await;
            let delivered = hub.publish(ServerFrame::MessageRecalled {
                message_id: msg.id.0,
                conversation_id: msg.conversation_id.0,
            });
            tracing::debug!(
                delivered,
                message_id = %msg.id.0,
                window_secs,
                "message_recalled"
            );
        }
        Err(e) => {
            let (code, detail) = map_service_error(&e);
            send_error(
                ws_session,
                code,
                &format!("recall_message failed: {detail}"),
                Some(req_id),
            )
            .await;
        }
    }
}

/// 发成功 ack (per aux-13 §1.2.2)
async fn send_ack(
    session: &mut actix_ws::Session,
    req_id: Uuid,
    message_id: Option<Uuid>,
    sequence: Option<i64>,
    idempotent_replay: bool,
) {
    let frame = ServerFrame::Ack {
        req_id,
        ok: true,
        data: Some(im_protocol::ws_frames::AckData {
            message_id,
            sequence,
            idempotent_replay,
        }),
        error: None,
    };
    if let Err(e) = session
        .text(serde_json::to_string(&frame).unwrap_or_default())
        .await
    {
        tracing::warn!(error = ?e, "ws send ack failed");
    }
}

/// `AppError` → (已注册错误码, 可安全回给客户端的说明)
///
/// **不自己维护映射** —— 直接用 `AppError::code()`(im-common 里的穷尽 match,
/// 单一真源)。若在此另写一份映射, 两处会随变体新增而漂移。
///
/// 唯一自行决定的是**哪些错误的消息可以外泄**:
/// - `InternalError` / `ServiceUnavailable`: 只回通用文案, 具体原因(可能含
///   SQL / 连接串 / 内部路径)写服务端日志。aux-03 §B 对这两码的客户端建议是
///   「重试」而非「展示细节」。
/// - 其余业务码: 消息可直接回传。aux-03 §B 明确 `VALIDATION_ERROR` 的响应
///   应含 `details: [{field, reason}]` 供客户端展示具体字段错误。
fn map_service_error(e: &AppError) -> (im_common::ErrorCode, String) {
    use im_common::ErrorCode as EC;
    let code = e.code();
    match code {
        EC::InternalError | EC::ServiceUnavailable => {
            tracing::error!(error = %e, "send_message internal failure");
            (code, "internal error".into())
        }
        _ => (code, e.to_string()),
    }
}

/// 通过 WS 发错误帧 (per aux-13 §1.2.4)
async fn send_error(
    session: &mut actix_ws::Session,
    code: im_common::ErrorCode,
    message: &str,
    req_id: Option<Uuid>,
) {
    let body = serde_json::to_string(&error_ack_frame(code, message, req_id))
        .unwrap_or_else(|_| "{}".into());
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
        // 走 `error_ack_frame` —— 与 `send_error` 线上路径同一个构造函数,
        // 断言的是真实 wire 形状(aux-13 §1.2.4)。
        let req_id = Uuid::new_v4();
        let s = serde_json::to_string(&error_ack_frame(
            im_common::ErrorCode::Unauthorized,
            "auth failed: invalid token",
            Some(req_id),
        ))
        .unwrap();
        assert!(
            s.contains("\"type\":\"ack\""),
            "必须是 ack 帧, 非另造的 error 帧: {s}"
        );
        assert!(s.contains("\"ok\":false"), "失败响应 ok 必须为 false: {s}");
        assert!(
            s.contains("\"code\":\"UNAUTHORIZED\""),
            "code 应嵌在 error 里: {s}"
        );
        assert!(s.contains("\"message\":\"auth failed: invalid token\""));
        assert!(
            s.contains(&format!("\"req_id\":\"{req_id}\"")),
            "必须回传 req_id: {s}"
        );
    }

    #[test]
    fn ws_error_frame_without_req_id_uses_nil_sentinel() {
        // 协议级错误(非法 JSON / binary 帧等)解析不出 req_id, 统一用
        // `Uuid::nil()` 表示「无可关联的请求」—— 而不是省略字段。
        let s = serde_json::to_string(&error_ack_frame(
            im_common::ErrorCode::ValidationError,
            "bad",
            None,
        ))
        .unwrap();
        assert!(s.contains("\"req_id\""), "req_id 字段恒在: {s}");
        assert!(
            s.contains("00000000-0000-0000-0000-000000000000"),
            "无 req_id 时应为 nil UUID: {s}"
        );
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

    // ========================================================================
    // WS Auth 帧的 device_session_id (2026-10-03 修复"凭空造 id")
    // ========================================================================

    fn ts() -> Arc<TokenService> {
        Arc::new(im_core::identity::token::TokenService::new(
            vec![im_core::identity::token::SigningKey {
                kid: "v1".into(),
                key: secrecy::SecretString::new(
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                ),
            }],
            chrono::Duration::seconds(900),
            secrecy::SecretString::new("pepper".into()),
        ))
    }

    fn ws_user() -> im_core::identity::repository::User {
        im_core::identity::repository::User {
            id: UserId::new(),
            environment_id: EnvironmentId::new(),
            kind: im_core::identity::repository::UserKind::User,
            external_identity: None,
            state: im_core::identity::repository::UserState::Active,
            display_name: Some("ws".into()),
            username: None,
            password_hash: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn ws_auth_returns_the_dsid_carried_by_the_token() {
        // 关键: 返回的必须是 **token 里那个** dsid。此前实现返回
        // `DeviceSessionId::new()` —— 随机 UUID, 库里没有对应行, 且不报错。
        let svc = ts();
        let user = ws_user();
        let real_dsid = DeviceSessionId::new();
        let token = svc
            .issue_access_token_for_session(&user, Some(real_dsid))
            .unwrap();

        let (_uid, _env, got) = handle_auth(
            &scaled_session(Duration::from_secs(30), Duration::from_secs(60)),
            &token.0,
            &svc,
        )
        .await
        .expect("带 dsid 的 token 应通过 WS 鉴权");

        assert_eq!(
            got, real_dsid,
            "handle_auth 必须返回 token 携带的 dsid, 不能凭空造一个"
        );
    }

    #[tokio::test]
    async fn ws_auth_rejects_token_without_dsid() {
        // 缺 dsid 时**拒绝**, 不编造 —— WS 是长连接, 无法吊销的鉴权通道
        // 正是 gap-ledger §1.5 关掉的那个安全缺口。
        let svc = ts();
        let token = svc.issue_access_token(&ws_user()).unwrap();

        let r = handle_auth(
            &scaled_session(Duration::from_secs(30), Duration::from_secs(60)),
            &token.0,
            &svc,
        )
        .await;
        assert!(
            r.is_err(),
            "无 dsid 的 token 不该被放行, 更不该拿到一个编造的 session id"
        );
    }

    #[test]
    fn map_service_error_does_not_leak_internal_details() {
        // 内部错误的原因可能含 SQL / 连接串 / 内部路径, 绝不能原样回给客户端。
        let secret = "postgres://im:hunter2@10.0.0.5:5432/im SELECT * FROM users";
        let e = AppError::Internal(anyhow::anyhow!(secret));
        let (code, msg) = map_service_error(&e);
        assert_eq!(code, im_common::ErrorCode::InternalError);
        assert!(
            !msg.contains("hunter2") && !msg.contains("10.0.0.5"),
            "内部错误细节泄漏到客户端: {msg}"
        );

        // 服务不可用同理
        let e = AppError::ServiceUnavailable("redis://:pw@cache:6379".into());
        let (code, msg) = map_service_error(&e);
        assert_eq!(code, im_common::ErrorCode::ServiceUnavailable);
        assert!(!msg.contains("pw"), "依赖地址/凭据泄漏: {msg}");
    }

    #[test]
    fn map_service_error_passes_through_business_errors() {
        // 业务错误的说明应原样回传 —— aux-03 §B 要求 VALIDATION_ERROR 的
        // 响应含字段级原因供客户端展示。
        let (code, msg) = map_service_error(&AppError::Validation("kind must not be empty".into()));
        assert_eq!(code, im_common::ErrorCode::ValidationError);
        assert!(
            msg.contains("kind must not be empty"),
            "业务原因应回传: {msg}"
        );

        let (code, _) = map_service_error(&AppError::Forbidden("not a member".into()));
        assert_eq!(code, im_common::ErrorCode::Forbidden);
    }

    #[test]
    fn map_service_error_covers_every_registered_app_error_variant() {
        // 穷尽性回归: 逐一构造各变体, 断言映射出的码都**已注册**
        // (即 `map_service_error` 用的 `AppError::code()` 不会自造新码)。
        let variants = vec![
            AppError::Unauthorized("a".into()),
            AppError::Forbidden("b".into()),
            AppError::NotFound("c".into()),
            AppError::RateLimited(1),
            AppError::Validation("d".into()),
            AppError::IdempotencyConflict(Uuid::new_v4()),
            AppError::InvalidStateTransition {
                from: "a".into(),
                to: "b".into(),
            },
            AppError::RecallWindowExpired(chrono::Utc::now()),
            AppError::AccountBanned,
            AppError::AccountSuspended,
            AppError::AccountMergeConflict,
            AppError::FriendRequestExists,
            AppError::FriendRequestNotFound(Uuid::new_v4()),
            AppError::UserBlocked,
            AppError::ConversationNotFound(Uuid::new_v4()),
            AppError::MessageNotFound(Uuid::new_v4()),
            AppError::MessageTooLarge(10, 5),
            AppError::InvalidIdempotencyKey("e".into()),
            AppError::EnvironmentDisabled(Uuid::new_v4()),
            AppError::Internal(anyhow::anyhow!("f")),
            AppError::ServiceUnavailable("g".into()),
        ];
        for v in variants {
            let (code, _) = map_service_error(&v);
            // `as_str()` 必须落在 aux-03 §B 的注册集合内(由 check-error-codes.ps1 兜底)
            let wire = code.as_str();
            assert!(
                wire.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "{v:?} 映射出非错误码形状的串: {wire}"
            );
        }
    }

    #[test]
    fn business_frame_error_frame_carries_req_id() {
        // aux-13 §1.2.4: 失败响应带 req_id, 客户端据此关联回自己的请求。
        // 此前业务帧分支传 `None`, 客户端拿到「无主」错误帧。
        let req_id = Uuid::new_v4();
        let s = serde_json::to_string(&error_ack_frame(
            im_common::ErrorCode::ValidationError,
            "frame handler not implemented in MVP (per 138 §C-9)",
            Some(req_id),
        ))
        .unwrap();
        assert!(
            s.contains(&format!("\"req_id\":\"{req_id}\"")),
            "错误帧必须回传 req_id, 实际: {s}"
        );
    }

    #[test]
    fn every_client_frame_variant_is_routed_to_an_explicit_arm() {
        // **这条测试断言的边界要说清**: 它验证「8 个客户端帧变体全部可解析、
        // 且每个都能被归入某个显式分类」, 分类逻辑写在测试里(与 handler 的
        // match 同形但**不是同一份代码**)。
        //
        // 「handler 真的没有 `Ok(_)` 兜底」这一点**不是测试保证的**, 而是
        // **编译期保证** —— handler 里的 match 是穷尽的(8 个变体各有分支),
        // 未来新增变体会在那里编译报错。测试若照抄一份 match, 反而会在
        // handler 改动时静默漂移, 给人虚假的安全感。
        //
        // 那为什么还要这条? 因为它能锁住另一件事: **每个变体的 wire 形状
        // 都真的可解析**。若某个变体因为 serde tag 写错而永远匹配不上,
        // handler 里那条分支就是死代码 —— 而这正是 ping 漏洞的形态。
        let samples: Vec<(&str, serde_json::Value)> = vec![
            (
                // 注意: 首帧走 handler 自己的 `AuthFrame`(req_id 可选), 后续帧走
                // `ClientFrame::Auth`(req_id 必填) —— 两个类型形状不同, 样例按
                // 后者给全。
                "auth",
                serde_json::json!({"type":"auth","req_id":"11111111-1111-4111-8111-111111111111","access_token":"t"}),
            ),
            (
                "send_message",
                serde_json::json!({"type":"send_message","req_id":"22222222-2222-4222-8222-222222222222","conversation_id":"22222222-2222-4222-8222-222222222222","idempotency_key":"22222222-2222-4222-8222-222222222222","kind":"text","content":{"kind":"text","text":"hi"}}),
            ),
            (
                "edit_message",
                serde_json::json!({"type":"edit_message","req_id":"22222222-2222-4222-8222-222222222222","message_id":"22222222-2222-4222-8222-222222222222","content":{"kind":"text","text":"hi"}}),
            ),
            (
                "recall_message",
                serde_json::json!({"type":"recall_message","req_id":"22222222-2222-4222-8222-222222222222","message_id":"22222222-2222-4222-8222-222222222222"}),
            ),
            (
                "react",
                serde_json::json!({"type":"react","req_id":"22222222-2222-4222-8222-222222222222","message_id":"22222222-2222-4222-8222-222222222222","emoji":"👍"}),
            ),
            (
                "mark_read",
                serde_json::json!({"type":"mark_read","req_id":"22222222-2222-4222-8222-222222222222","conversation_id":"22222222-2222-4222-8222-222222222222","sequence":1}),
            ),
            (
                "typing",
                serde_json::json!({"type":"typing","req_id":"22222222-2222-4222-8222-222222222222","conversation_id":"22222222-2222-4222-8222-222222222222"}),
            ),
            ("ping", serde_json::json!({"type":"ping","ts":1})),
        ];
        assert_eq!(samples.len(), 8, "ClientFrame 应有 8 个变体");

        for (name, v) in samples {
            let parsed: ClientFrame =
                serde_json::from_value(v).unwrap_or_else(|e| panic!("{name} 应可解析: {e}"));
            // 关键: Ping / Auth 走各自专属分支, 其余 6 个业务帧走显式 or-pattern
            // (都绑定 req_id), 即不存在「不知道落到哪」的变体。
            let classified = match parsed {
                ClientFrame::Auth { .. } => "auth",
                ClientFrame::Ping { .. } => "ping",
                ClientFrame::SendMessage { .. }
                | ClientFrame::EditMessage { .. }
                | ClientFrame::RecallMessage { .. }
                | ClientFrame::React { .. }
                | ClientFrame::MarkRead { .. }
                | ClientFrame::Typing { .. } => "business",
            };
            assert!(
                matches!(classified, "auth" | "ping" | "business"),
                "{name} 未被任何显式分支覆盖"
            );
        }
    }
}
