//! `GET /v1/conversations/{id}/members` — WBS C-10
//!
//! 依据: ImplementationSpec §3.1.3 + aux-02 §F.11 (conversation_members)
//!
//! ## 行为
//! - 鉴权(Bearer):401 UNAUTHORIZED
//! - 调用方必须是 conversation member,否则 403 FORBIDDEN
//! - 返回 members + role + joined_at + last_read_sequence
//!
//! ## 响应
//! ```json
//! {
//!   "conversation_id": "uuid",
//!   "members": [
//!     { "user_id": "uuid", "role": "owner", "joined_at": "...", "last_read_sequence": 0 }
//!   ]
//! }
//! ```

use actix_web::{web, HttpResponse};
use serde::Serialize;
use uuid::Uuid;

use im_common::ids::ConversationId;
use im_common::AppError;
use im_core::conversation::repository::{ConversationMember, MemberRole};

use super::error_response::json_response;
use super::state::{AppState, AuthedUser};

// ============================================================================
// Response DTOs
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MemberDto {
    pub user_id: Uuid,
    pub role: MemberRole,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub last_read_sequence: i64,
}

impl From<ConversationMember> for MemberDto {
    fn from(m: ConversationMember) -> Self {
        Self {
            user_id: m.user_id.0,
            role: m.role,
            joined_at: m.joined_at,
            last_read_sequence: m.last_read_sequence,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListMembersResponse {
    pub conversation_id: Uuid,
    pub members: Vec<MemberDto>,
}

// ============================================================================
// Handler
// ============================================================================

pub async fn list(
    app: web::Data<AppState>,
    auth: AuthedUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, actix_web::Error> {
    let conv_id = ConversationId(path.into_inner());

    // 1. 调用方必须是 conversation 成员(防止越权枚举)
    let is_member = app
        .conversation_service
        .is_member(conv_id, auth.user_id)
        .await
        .map_err(http_err)?;
    if !is_member {
        return Err(json_response(
            im_common::ErrorCode::Forbidden,
            Some(conv_id),
            Some("not a member of this conversation"),
        ));
    }

    // 2. 列成员(per ConversationRepository::list_members)
    let members = app
        .conversation_service
        .repo()
        .list_members(conv_id)
        .await
        .map_err(http_err)?;

    tracing::info!(
        conversation_id = %conv_id,
        caller_user_id = %auth.user_id,
        member_count = members.len(),
        "members listed"
    );

    Ok(HttpResponse::Ok().json(ListMembersResponse {
        conversation_id: conv_id.0,
        members: members.into_iter().map(MemberDto::from).collect(),
    }))
}

fn http_err(e: AppError) -> actix_web::Error {
    use crate::error::error_to_response;
    let resp = error_to_response(&e);
    tracing::warn!(error = ?e, "list_members failed");
    actix_web::error::InternalError::from_response(
        e.code().as_str().to_string(),
        resp,
    )
    .into()
}

// ============================================================================
// C-10 单元测试 — DTO 转换
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use im_common::ids::ConversationId;

    #[test]
    fn member_dto_from_entity() {
        let m = ConversationMember {
            conversation_id: ConversationId::new(),
            user_id: im_common::ids::UserId::new(),
            role: MemberRole::Owner,
            joined_at: chrono::Utc::now(),
            last_read_sequence: 42,
        };
        let dto = MemberDto::from(m.clone());
        assert_eq!(dto.user_id, m.user_id.0);
        assert_eq!(dto.role, MemberRole::Owner);
        assert_eq!(dto.last_read_sequence, 42);
    }

    #[test]
    fn list_members_response_round() {
        let resp = ListMembersResponse {
            conversation_id: Uuid::new_v4(),
            members: vec![],
        };
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"conversation_id\""));
        assert!(s.contains("\"members\""));
    }

    #[test]
    fn member_role_serde_strings() {
        // wire: owner | admin | member (per aux-02 §F.11)
        for (role, expected) in [
            (MemberRole::Owner, "\"owner\""),
            (MemberRole::Admin, "\"admin\""),
            (MemberRole::Member, "\"member\""),
        ] {
            let s = serde_json::to_string(&role).unwrap();
            assert_eq!(s, expected);
        }
    }
}
