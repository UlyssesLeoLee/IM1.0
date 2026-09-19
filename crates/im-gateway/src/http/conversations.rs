//! `/v1/conversations` HTTP handlers — WBS C-8
//!
//! 依据: aux-13 §3.3 + ImplementationSpec §3.1.3
//!
//! ## 端点
//! - `POST /v1/conversations` — 创建 dm / group / channel 会话
//! - `GET /v1/conversations?cursor=&limit=` — 当前用户参与的会话列表
//!
//! ## 请求/响应 (per aux-13 §3.3)
//! ```json
//! POST body:
//! { "kind": "dm", "member_user_ids": ["..."], "metadata": {} }
//! POST 201:
//! { "id": "uuid", "environment_id": "uuid", "kind": "dm", "metadata": {}, "created_at": "..." }
//! ```
//!
//! ## 派生约束
//! - Guest 用户只能创建 dm(P1-5 per ImplementationSpec §11.1 + 132-wbs §5.3.2 备注)
//! - DM 走 Service.create_dm,幂等返回已有 conv
//! - Channel/Broadcast kind 留给 V1+,MVP 仅 dm / group
//! - Bearer token 鉴权失败 → 401 JSON 错误

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_core::conversation::repository::{Conversation, ConversationKind};
use im_core::conversation::service::ConversationService;

use super::error_response::json_response;
use super::state::{AppState, AuthedUser};

// ============================================================================
// Request / Response DTOs
// ============================================================================

/// POST /v1/conversations request body
///
/// per aux-13 §3.3:
/// ```json
/// { "kind": "dm", "member_user_ids": ["..."], "metadata": {} }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct CreateConversationRequest {
    /// dm | group | channel (MVP 限定 dm/group)
    pub kind: ConversationKindWire,
    /// DM 仅一个对端 user_id;Group 多个;Channel 暂留空
    #[serde(default)]
    pub member_user_ids: Vec<Uuid>,
    /// 仅 group 适用,如 {"title": "..."}
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// 仅 dm | group (MVP 范围);Channel/Broadcast 走 400 validation
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConversationKindWire {
    Dm,
    Group,
    // channel / broadcast 显式拒绝
}

impl ConversationKindWire {
    fn to_kind(self) -> ConversationKind {
        match self {
            ConversationKindWire::Dm => ConversationKind::Dm,
            ConversationKindWire::Group => ConversationKind::Group,
        }
    }
}

/// 会话 response
#[derive(Debug, Clone, Serialize)]
pub struct ConversationResponse {
    pub id: ConversationId,
    pub environment_id: EnvironmentId,
    pub kind: ConversationKind,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Conversation> for ConversationResponse {
    fn from(c: Conversation) -> Self {
        Self {
            id: c.id,
            environment_id: c.environment_id,
            kind: c.kind,
            metadata: c.metadata,
            created_at: c.created_at,
        }
    }
}

/// GET list response
#[derive(Debug, Clone, Serialize)]
pub struct ListConversationsResponse {
    pub conversations: Vec<ConversationResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /v1/conversations
///
/// 创建 dm 或 group 会话。guest 仅允许 dm(P1-5)。
pub async fn create(
    app: web::Data<AppState>,
    auth: AuthedUser,
    body: web::Json<CreateConversationRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let svc: &ConversationService = &app.conversation_service;

    // 1. guest 校验(P1-5 per ImplementationSpec §11.1)
    let kind = body.kind.to_kind();
    if auth.kind == "guest" && kind != ConversationKind::Dm {
        return Err(json_response(
            im_common::ErrorCode::Forbidden,
            None,
            Some("guest users can only create DM conversations"),
        ));
    }

    // 2. metadata 必须是 JSON object(per PgConversationRepository::create 的检查前置)
    if !matches!(body.metadata, serde_json::Value::Object(_))
        && !body.metadata.is_null()
    {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("metadata must be a JSON object"),
        ));
    }

    // 3. dispatch
    let conv = match kind {
        ConversationKind::Dm => {
            // P1-5 + aux-13 §3.3:DM 仅一个对端
            if body.member_user_ids.len() != 1 {
                return Err(json_response(
                    im_common::ErrorCode::ValidationError,
                    None,
                    Some("DM requires exactly one peer user_id in member_user_ids"),
                ));
            }
            let peer = UserId(body.member_user_ids[0]);
            if peer == auth.user_id {
                return Err(json_response(
                    im_common::ErrorCode::ValidationError,
                    None,
                    Some("cannot create DM with self"),
                ));
            }

            svc.create_dm(auth.environment_id, auth.user_id, peer)
                .await
                .map_err(http_err)?
        }
        ConversationKind::Group => {
            // 2-500 成员(per ConversationService::create_group 限制)
            let mut members: Vec<UserId> = body
                .member_user_ids
                .iter()
                .map(|u| UserId(*u))
                .collect();
            // creator 自动加入;若 client 已包含 creator,svc 去重
            if !members.contains(&auth.user_id) {
                members.push(auth.user_id);
            }

            let metadata = if body.metadata.is_null() {
                serde_json::json!({})
            } else {
                body.metadata.clone()
            };

            svc.create_group(auth.environment_id, auth.user_id, members, metadata)
                .await
                .map_err(http_err)?
        }
        // channel/broadcast/system:MVP 不支持,留 V1+
        _ => {
            return Err(json_response(
                im_common::ErrorCode::ValidationError,
                None,
                Some("MVP only supports dm/group conversation kinds"),
            ));
        }
    };

    let resp = ConversationResponse::from(conv);
    tracing::info!(
        conversation_id = %resp.id,
        user_id = %auth.user_id,
        kind = ?resp.kind,
        "conversation created"
    );
    Ok(HttpResponse::Created().json(resp))
}

/// GET /v1/conversations?cursor=&limit=
///
/// 当前用户参与的会话列表(per ImplementationSpec §3.1.3)
pub async fn list(
    app: web::Data<AppState>,
    auth: AuthedUser,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let svc: &ConversationService = &app.conversation_service;
    let limit = query.limit.unwrap_or(50);

    let convs = svc
        .list_user_conversations(auth.user_id, query.cursor.as_deref(), limit)
        .await
        .map_err(http_err)?;

    // has_more 推断:limit 返回 N 条 + cursor 仍能下推 → 客户端用 next_cursor
    let has_more = convs.len() as i32 >= limit.clamp(1, 50);
    let next_cursor = if has_more {
        // cursor = "created_at:last.id"(简化,client 透传即可)
        convs
            .last()
            .map(|c| format!("{}:{}", c.created_at.timestamp_millis(), c.id.0))
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ListConversationsResponse {
        conversations: convs.into_iter().map(ConversationResponse::from).collect(),
        next_cursor,
        has_more,
    }))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    /// 默认 50,clamp 1..=50(per ConversationService::list_user_conversations 内部限)
    #[serde(default)]
    pub limit: Option<i32>,
}

/// AppError → actix_web::Error 适配
fn http_err(e: im_common::AppError) -> actix_web::Error {
    use crate::error::error_to_response;
    let code = e.code();
    let resp = error_to_response(&e);
    tracing::warn!(error = ?e, "request failed");
    actix_web::error::InternalError::from_response(code.as_str().to_string(), resp).into()
}

// ============================================================================
// C-8 单元测试 — 不依赖 DB,只测 input validation + DTO 转换
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_wire_dm_to_kind() {
        assert_eq!(
            ConversationKindWire::Dm.to_kind(),
            ConversationKind::Dm
        );
    }

    #[test]
    fn kind_wire_group_to_kind() {
        assert_eq!(
            ConversationKindWire::Group.to_kind(),
            ConversationKind::Group
        );
    }

    #[test]
    fn conv_response_includes_id_and_kind() {
        let conv = Conversation {
            id: ConversationId::new(),
            environment_id: EnvironmentId::new(),
            kind: ConversationKind::Dm,
            metadata: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        };
        let conv_id = conv.id;
        let resp = ConversationResponse::from(conv);
        assert_eq!(resp.id, conv_id);
        assert_eq!(resp.kind, ConversationKind::Dm);
    }
}
