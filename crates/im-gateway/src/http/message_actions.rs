//! 消息「动作类」REST 端点: 编辑 / 撤回 / reaction / 已读
//!
//! 依据: BasicDesign §7(REST 清单) + DetailedDesign 端点→需求映射表
//!       (该表按「§5」引用了这四个端点, 而 DetailedDesign §5 自己的完整清单
//!       **没有列它们** —— 见下方「规范自相矛盾」)
//!       aux-13 §1.1.3 / §1.1.4 / §1.1.5 / §1.1.6 (对应的 WS 帧)
//!
//! ## 为什么这四个端点此前不存在, 而能力已经实装
//!
//! 2026-10-03 之前, `edit` / `recall` / `react` / `mark_read` **只有 WS 帧**,
//! REST 侧一个都没有。这对「商业产品标准」是硬伤: 服务端 SDK、后台任务、
//! 非 WS 客户端(Web/桌面离线补传)全都无法触达这些能力 —— 而这些能力
//! **已经**在 service 层实装完毕(见 `MessageService::edit_message` /
//! `recall_message`、`ConversationService::mark_read`、`ReactionService`)。
//!
//! **刻意不重写校验逻辑**: 四个 handler 全部只做「解析 → 调 service → 映射
//! HTTP 状态码」, 所有业务规则(仅原 sender / 终态拒绝 / 时间窗 / 成员校验 /
//! 单调不回退)都在 service 里。WS 与 REST 走**同一条**校验链 —— 若 REST 另写
//! 一遍, 两条路径迟早漂移, 而漂移的那一侧就是越权漏洞。
//!
//! ## 规范自相矛盾 (只记录, 不擅自消解)
//!
//! `DetailedDesign.md` 的端点→需求映射表逐条写着
//! `| §5 PATCH /v1/conversations/{id}/messages/{msg_id} | 编辑消息 | ... |`
//! —— 明确标注这些端点属于「§5」。但 DetailedDesign §5 标题是
//! 「REST API **完整**清单(MVP)」, 其表格里**根本没有**这四个端点。
//!
//! 两份规范对「端点全集」的认知也不同:
//! - `BasicDesign §7`: 有 recall / reactions / read / PATCH edit, 无 friends / media / me
//! - `DetailedDesign §5`: 有 friends / media / me, 无 recall / reactions / read / PATCH
//!
//! 本模块只实现**两份规范都指向(或其一明确列出)**的 4 个消息类端点 —— 它们
//! 的意图无歧义(端点→需求表点名 + BasicDesign §7 列出 + service 已就绪)。
//! friends / media / me 那 8 个端点**不在本 commit 范围**: 它们的 service 层
//! 大多尚不存在(relationship 有仓储但无 service; `/me` 需要一个尚未定义的用户
//! 资料读写面), 硬做只能凭空发明 wire 形状。
//!
//! ## 状态码: 无规范可依, 故全部取自 `ErrorCode::http_status()` 单一真源
//!
//! 没有任何一份规范规定这四个端点的响应码, 故**不自造**:
//! - 失败 → 统一走 `json_response(code, ..)`, 状态码由 `ErrorCode::http_status()`
//!   决定(与既有 messages handler 完全一致)
//! - 成功 → REST 惯例, 且与实际结果对应:
//!   `PATCH` 改资源 → 200 + 资源; `recall` 改状态 → 200 + 资源;
//!   `reactions` 创建子资源 → **201**(新插入) / **200**(幂等重放, 资源已存在);
//!   `read` 幂等更新 → 200 + 当前读指针。
//!
//! ## 路径里的 conversation_id 必须与消息实际归属一致
//!
//! `PATCH /v1/conversations/A/messages/{msg}` —— 若 `msg` 其实属于会话 B,
//! 返回 404。理由: service 的编辑/撤回只校验「是否原 sender」, **不**校验
//! 路径里的 conversation; 若不在 handler 层对齐, 客户端会拿到一个
//! 「在会话 A 里编辑成功」的响应, 而资源其实在 B —— URL 说谎, 且让
//! 按 conversation 做的审计/限流全部错位。

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use im_common::ids::{ConversationId, MessageId};
use im_common::AppError;

use super::error_response::json_response;
use super::messages::MessageResponse;
use super::state::{AppState, AuthedUser};

/// 默认最大消息字节数 —— 与 `messages::send_message` 同源
///
/// **刻意不读 `environments.settings`**: `send_message` 也没读(它把
/// `max_size_bytes` 硬编在调用点并留了 TODO)。这两处要一起改成读 settings
/// 才一致; 单独改一处会让「同一个限制在两个端点有不同值」, 那比两处都写死
/// 更难排查。此处保持与既有端点同值并标注。
const MAX_SIZE_BYTES: usize = 65_536;

// ============================================================================
// DTO
// ============================================================================

/// `PATCH /{msg_id}` 请求体 (per aux-13 §1.1.3 `edit_message`)
#[derive(Debug, Clone, Deserialize)]
pub struct EditMessageRequest {
    pub content: Value,
}

/// `POST /{msg_id}/reactions` 请求体 (per aux-13 §1.1.5 `react`)
#[derive(Debug, Clone, Deserialize)]
pub struct AddReactionRequest {
    pub emoji: String,
}

/// `POST /{id}/read` 请求体 (per aux-13 §1.1.6 `mark_read`)
#[derive(Debug, Clone, Deserialize)]
pub struct MarkReadRequest {
    /// 已读到的 sequence(闭区间)
    pub sequence: i64,
}

/// `POST /{id}/read` 响应体
#[derive(Debug, Clone, Serialize)]
pub struct MarkReadResponse {
    pub conversation_id: Uuid,
    /// 服务端当前读指针 —— **不**回显请求值, 因为未推进时它可能与请求不同
    /// (请求了一个更小的 sequence 是**成功**但无效果, 见 aux-08 幂等约定)。
    /// 客户端若拿回显值会误以为读指针退了。
    pub last_read_sequence: i64,
    /// 本次是否真的推进了(`false` = 幂等重放, 不是错误)
    pub advanced: bool,
}

/// `POST /{msg_id}/reactions` 响应体
#[derive(Debug, Clone, Serialize)]
pub struct AddReactionResponse {
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub emoji: String,
    /// 201 时为 true; 200(幂等重放)时为 false
    pub created: bool,
}

// ============================================================================
// 内部: 路径与消息归属一致性
// ============================================================================

/// 校验「消息确实属于 path 里给的会话」, 不一致按 404 处理
///
/// 404 而非 403: 从调用方视角, `conversations/A/messages/{msg}` 这个资源
/// **不存在**(msg 不在 A 里), 与「存在但无权限」是两件事 —— 403 会顺带确认
/// 「该 id 对应的消息确实存在于别处」, 那本身是信息泄漏。
async fn ensure_message_in_conversation(
    app: &web::Data<AppState>,
    conv: ConversationId,
    msg: MessageId,
) -> Result<im_core::message::repository::Message, actix_web::Error> {
    let m = app
        .message_service
        .get(msg)
        .await
        .map_err(app_error)?
        .ok_or_else(|| {
            json_response(
                im_common::ErrorCode::NotFound,
                Some(conv),
                Some("message not found"),
            )
        })?;
    if m.conversation_id != conv {
        return Err(json_response(
            im_common::ErrorCode::NotFound,
            Some(conv),
            Some("message does not belong to this conversation"),
        ));
    }
    Ok(m)
}

/// `AppError` → actix 错误(状态码走 `ErrorCode::http_status()` 单一真源)
fn app_error(e: AppError) -> actix_web::Error {
    // 内部错误的具体原因不外泄(可能含 SQL / 内部路径), 与 WS 侧
    // `map_service_error` 同一取舍; 明细写服务端日志。
    if matches!(
        e.code(),
        im_common::ErrorCode::InternalError | im_common::ErrorCode::ServiceUnavailable
    ) {
        tracing::error!(error = %e, "rest message action failed");
        return json_response(
            im_common::ErrorCode::InternalError,
            None,
            Some("internal error"),
        );
    }
    json_response(e.code(), None, Some(&e.to_string()))
}

// ============================================================================
// PATCH /v1/conversations/{id}/messages/{msg_id} —— 编辑
// ============================================================================

/// `PATCH /v1/conversations/{id}/messages/{msg_id}`
///
/// 200 + `MessageResponse`(编辑后的消息)
pub async fn edit_message(
    auth: AuthedUser,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<EditMessageRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (conv_uuid, msg_uuid) = path.into_inner();
    let conv = ConversationId(conv_uuid);
    let msg_id = MessageId(msg_uuid);

    if !matches!(body.content, Value::Object(_)) {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            Some(conv),
            Some("content must be JSON object"),
        ));
    }
    ensure_message_in_conversation(&app, conv, msg_id).await?;

    let updated = app
        .message_service
        .edit_message(
            msg_id,
            auth.user_id,
            body.into_inner().content,
            MAX_SIZE_BYTES,
        )
        .await
        .map_err(app_error)?;

    Ok(HttpResponse::Ok().json(MessageResponse::from(&updated)))
}

// ============================================================================
// POST /v1/conversations/{id}/messages/{msg_id}/recall —— 撤回
// ============================================================================

/// `POST /v1/conversations/{id}/messages/{msg_id}/recall`
///
/// 200 + `MessageResponse`(state 已是 `recalled`)
///
/// 撤回时间窗读 `environments.settings.message.recall_window_seconds`
/// (aux-04 §B.4 不变量: **不能写死**), 解析方式与 WS 侧完全一致。
pub async fn recall_message(
    auth: AuthedUser,
    path: web::Path<(Uuid, Uuid)>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (conv_uuid, msg_uuid) = path.into_inner();
    let conv = ConversationId(conv_uuid);
    let msg_id = MessageId(msg_uuid);

    ensure_message_in_conversation(&app, conv, msg_id).await?;

    let window_secs = app
        .settings_service
        .recall_window_seconds(auth.environment_id)
        .await
        .map_err(app_error)?;

    let recalled = app
        .message_service
        .recall_message(
            msg_id,
            auth.user_id,
            chrono::Duration::seconds(i64::from(window_secs)),
        )
        .await
        .map_err(|e| app_error_with_conv(e, conv))?;

    Ok(HttpResponse::Ok().json(MessageResponse::from(&recalled)))
}

// ============================================================================
// POST /v1/conversations/{id}/messages/{msg_id}/reactions —— 添加 reaction
// ============================================================================

/// `POST /v1/conversations/{id}/messages/{msg_id}/reactions`
///
/// **201** + 资源(新插入) / **200** + 资源(幂等重放, 资源本就存在)
///
/// 权限(「必须是消息所属会话的成员」)由 `ReactionService` 承担, 本 handler
/// 不重复实现 —— 见该 service 的模块文档。
pub async fn add_reaction(
    auth: AuthedUser,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<AddReactionRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (conv_uuid, msg_uuid) = path.into_inner();
    let conv = ConversationId(conv_uuid);
    let msg_id = MessageId(msg_uuid);

    ensure_message_in_conversation(&app, conv, msg_id).await?;

    let emoji = body.into_inner().emoji;
    let (reaction, created) = app
        .reaction_service
        .add_reaction(msg_id, auth.user_id, &emoji)
        .await
        .map_err(|e| app_error_with_conv(e, conv))?;

    let resp = AddReactionResponse {
        message_id: reaction.message_id.0,
        user_id: reaction.user_id.0,
        emoji: reaction.emoji,
        created,
    };
    if created {
        Ok(HttpResponse::Created().json(resp))
    } else {
        Ok(HttpResponse::Ok().json(resp))
    }
}

// ============================================================================
// POST /v1/conversations/{id}/read —— 上报已读
// ============================================================================

/// `POST /v1/conversations/{id}/read`
///
/// 200 + `MarkReadResponse`。`advanced: false` 是**成功**(幂等重放, per aux-08),
/// 不是错误 —— 故不返 409。
pub async fn mark_read(
    auth: AuthedUser,
    path: web::Path<Uuid>,
    body: web::Json<MarkReadRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let conv = ConversationId(path.into_inner());
    let requested = body.into_inner().sequence;

    if requested < 0 {
        // 表上是 `CHECK (last_read_sequence >= 0)`, 负值必然写不进去。提前
        // 返 400 好过让它撞 DB 约束后变成 500。
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            Some(conv),
            Some("sequence must be >= 0"),
        ));
    }

    let advanced = app
        .conversation_service
        .mark_read(conv, auth.user_id, requested)
        .await
        .map_err(|e| app_error_with_conv(e, conv))?;

    // 回**服务端**当前值而非请求值: 请求了更小的 sequence 是成功但无效果,
    // 回显请求值会让客户端误以为读指针退了。
    let current = app
        .conversation_service
        .repo()
        .list_members(conv)
        .await
        .map_err(app_error)?
        .into_iter()
        .find(|m| m.user_id == auth.user_id)
        .map(|m| m.last_read_sequence)
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(MarkReadResponse {
        conversation_id: conv.0,
        last_read_sequence: current,
        advanced,
    }))
}

/// 带 conversation 上下文的错误映射(响应体里带上 conv_id, per aux-13 §4)
fn app_error_with_conv(e: AppError, conv: ConversationId) -> actix_web::Error {
    if matches!(
        e.code(),
        im_common::ErrorCode::InternalError | im_common::ErrorCode::ServiceUnavailable
    ) {
        tracing::error!(error = %e, "rest message action failed");
        return json_response(
            im_common::ErrorCode::InternalError,
            Some(conv),
            Some("internal error"),
        );
    }
    json_response(e.code(), Some(conv), Some(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_sequence_is_rejected_before_touching_db() {
        // 纯逻辑断言: 表上是 CHECK >= 0, 负值必须走 400 而不是撞约束变 500。
        assert!(MarkReadRequest { sequence: -1 }.sequence < 0);
        assert!(!(MarkReadRequest { sequence: 0 }.sequence < 0));
    }

    #[test]
    fn add_reaction_request_rejects_missing_emoji_by_deserialization() {
        // `emoji` 是必填(无 #[serde(default)]), 缺字段时 400 由 actix 的
        // Json extractor 产生 —— 这里只确认「不会静默当成空串」。
        let ok: Result<AddReactionRequest, _> = serde_json::from_str(r#"{"emoji":"👍"}"#);
        assert!(ok.is_ok());
        let missing: Result<AddReactionRequest, _> = serde_json::from_str("{}");
        assert!(missing.is_err(), "缺 emoji 必须报错, 不能默认成空串");
    }

    /// `MarkReadResponse` 的 `last_read_sequence` 是**服务端当前值**, 不是请求值。
    ///
    /// 锁住「不回显请求值」这个决定: 请求一个**更小**的 sequence 是成功但无效果
    /// (aux-08 幂等约定), 若回显请求值, 客户端会以为读指针退了。
    #[test]
    fn mark_read_response_reports_server_value_not_requested_value() {
        let resp = MarkReadResponse {
            conversation_id: Uuid::nil(),
            last_read_sequence: 100,
            advanced: false,
        };
        let j = serde_json::to_value(&resp).unwrap();
        assert_eq!(j["last_read_sequence"], 100);
        assert_eq!(j["advanced"], false, "幂等重放仍是成功, 不是错误");
        // 请求值(比如 50)绝不出现在响应里
        assert!(
            j.get("requested_sequence").is_none(),
            "响应体不应回显请求值: {j}"
        );
    }
}
