//! Bearer token 鉴权中间件 + extractor
//!
//! 依据: aux-13 §3.1 (Authorization: Bearer <token>) + TokenService 校验链
//!
//! ## 范围 (per 132-wbs.md §5.3.2 C-3..C-7 auth 后续实装,本 PR 仅)
//! 本模块只做:
//! 1. `Bearer <token>` 提取 + TokenService.validate_access_token
//! 2. 解析 claims 为 AuthedUser,失败 → AppError::Unauthorized
//! 3. 缺失/无效 → 401 + JSON 错误响应
//!
//! C-3 (token_exchange + HMAC 签名校验) 留给 auth-lane worker。

use actix_web::dev::Payload;
use actix_web::http::header;
use actix_web::{FromRequest, HttpRequest};
use std::future::Future;
use std::pin::Pin;

use im_core::identity::token::TokenService;

use super::error_response::json_response;
use super::state::{AppState, AuthedUser};

// 从 state.rs 拿到 AuthedUser 后,这里仅作为 extractor 的实装文件
// 业务 handler(conversations/members)直接 `use super::state::AuthedUser`

/// `AuthedUser` actix extractor
///
/// 用法:
/// ```ignore
/// async fn handler(auth: AuthedUser, ...) -> HttpResponse { ... }
/// ```
impl FromRequest for AuthedUser {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        // 1. 提 Bearer
        let bearer = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer ").map(|s| s.to_string()));

        // 2. 从 app data 拿 token service
        let token_service = req.app_data::<actix_web::web::Data<AppState>>().map(|d| d.token_service.clone());

        Box::pin(async move {
            let token = match bearer {
                Some(t) if !t.is_empty() => t,
                _ => {
                    return Err(json_response(
                        im_common::ErrorCode::Unauthorized,
                        None,
                        Some("missing or invalid Authorization header"),
                    ));
                }
            };

            let svc = match token_service {
                Some(s) => s,
                None => {
                    return Err(json_response(
                        im_common::ErrorCode::ServiceUnavailable,
                        None,
                        Some("token service not configured"),
                    ));
                }
            };

            // 3. 校验
            let claims = match svc.validate_access_token(&token) {
                Ok(c) => c,
                Err(e) => {
                    return Err(json_response(
                        im_common::ErrorCode::Unauthorized,
                        None,
                        Some(format!("token invalid: {}", e).as_str()),
                    ));
                }
            };

            // 4. 解析 claims → AuthedUser
            let user_id: im_common::ids::UserId = claims
                .sub
                .parse()
                .map_err(|_| json_response(im_common::ErrorCode::Unauthorized, None, Some("invalid sub")))?;
            let environment_id: im_common::ids::EnvironmentId = claims
                .env
                .parse()
                .map_err(|_| json_response(im_common::ErrorCode::Unauthorized, None, Some("invalid env")))?;

            Ok(AuthedUser {
                user_id,
                environment_id,
                tenant_id: claims.tenant,
                kind: claims.kind,
            })
        })
    }
}

/// 辅助:复用 TokenService.validate(在非 extractor 场景,例如错误 handler 内部)
#[allow(dead_code)]
pub async fn validate_bearer(
    svc: &TokenService,
    token: &str,
) -> Result<AuthedUser, im_common::AppError> {
    let claims = svc
        .validate_access_token(token)
        .map_err(|_| im_common::AppError::Unauthorized("invalid token".into()))?;
    let user_id: im_common::ids::UserId = claims
        .sub
        .parse()
        .map_err(|_| im_common::AppError::Unauthorized("invalid sub".into()))?;
    let environment_id: im_common::ids::EnvironmentId = claims
        .env
        .parse()
        .map_err(|_| im_common::AppError::Unauthorized("invalid env".into()))?;
    Ok(AuthedUser {
        user_id,
        environment_id,
        tenant_id: claims.tenant,
        kind: claims.kind,
    })
}
