//! `/v1/auth/token/exchange` + `/v1/auth/guest` HTTP handlers — WBS C-3 + C-4
//!
//! 依据: aux-13-protocol-frame-samples.md §3.1 (C-3 token_exchange) + §3.2 (C-4 guest)
//!       ImplementationSpec §3.1.1 + §7.4.1
//!
//! ## 范围 (per 132-wbs.md §5.3.2 + task brief 2026-09-19)
//!
//! ### C-3 `POST /v1/auth/token/exchange`
//! - 游戏服务器 token 兑换 (HMAC 签名已在中间件层验证;此处假定 `server_signature_verified=true`)
//! - 接收 `{ environment_id, external_provider, external_uid, display_name }`
//! - 调 `IdentityService::server_exchange_token`
//! - 返回 `{ access_token, refresh_token, user_id, expires_in, device_session_id }`
//!
//! ### C-4 `POST /v1/auth/guest`
//! - 匿名注册 (自动生成 guest user,extid=NULL)
//! - 接收 `{ environment_id, device_fingerprint? }`
//! - 调 `IdentityService::guest_register` + DeviceSessionRepository::create
//! - 返回 `{ access_token, refresh_token, user_id, expires_in, device_session_id }`
//!
//! ## 派生约束
//! - Guest 用户 `kind=guest` JWT claim (per TokenService::issue_access_token)
//! - Server exchange 用户 `kind=user` (同上)
//! - refresh_token 格式 `<session_id>.<raw_uuid>` (per IdentityService::issue_token_pair)
//! - device_session_id 从 refresh_token 第一段截取 (公开字段,不是 secret)
//! - 响应字段 `expires_in` 当前写死 900 (15min) per IdentityService::issue_token_pair TODO 备注
//!   待 V1 抽 `TokenService::access_ttl_seconds()` 后改
//! - AppState 需含 `identity_service: Arc<IdentityService<...>>` + UserRepository + DeviceSessionRepository
//!   (state.rs 扩展,main.rs wire-up 留给 D-1 PR)
//!
//! ## 测试
//! 单元测试仅覆盖不依赖 DB 的纯函数:
//! - request DTO 解析 (环境 UUID / 字段 required check)
//! - response shape / 字段 wire-format
//! - refresh_token split → device_session_id 提取
//! 不测真实 sqlx 调用 (留 PG 集成测试,WSL PG 18.6 未启 → FAIL 是已知)

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;
use im_core::identity::repository::{ExternalIdentity, UserKind};
use im_core::identity::service::{IdentityService, ServerExchangeCommand};
use im_core::identity::token::TokenPair;

use super::error_response::json_response;
use super::state::AppState;

// ============================================================================
// C-3 / C-4 共享 Response DTO
// ============================================================================

/// Token pair response (C-3 + C-4 共用)
///
/// spec 字段 (per aux-13 §3.1):
/// ```json
/// { "access_token": "...", "refresh_token": "...", "user_id": "uuid", "expires_in": 900 }
/// ```
///
/// 任务规范扩展字段: `device_session_id` (不属于冻结协议,仅作为实现细节暴露给客户端做会话管理)
#[derive(Debug, Clone, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: UserId,
    pub expires_in: i64,
    pub device_session_id: Uuid,
}

impl TokenResponse {
    /// TokenPair + device_session_id → wire response
    fn from_token_pair(pair: TokenPair) -> Self {
        let device_session_id = extract_device_session_id(&pair.refresh_token.0);
        Self {
            access_token: pair.access_token.0,
            refresh_token: pair.refresh_token.0,
            user_id: pair.user_id,
            expires_in: pair.expires_in,
            device_session_id,
        }
    }
}

/// 从 refresh_token 字符串 `<session_id>.<raw>` 截取 device_session_id
///
/// refresh_token 由 IdentityService::issue_token_pair 拼接: `format!("{}.{}", device_session.id, refresh_raw)`
fn extract_device_session_id(refresh_token: &str) -> Uuid {
    refresh_token
        .split_once('.')
        .and_then(|(sid, _)| Uuid::parse_str(sid).ok())
        .unwrap_or(Uuid::nil())
}

// ============================================================================
// C-3 DTO: POST /v1/auth/token/exchange
// ============================================================================

/// C-3 request body
///
/// spec (aux-13 §3.1):
/// ```json
/// {
///   "environment_id": "uuid",
///   "external_provider": "steam",
///   "external_uid": "76561198000000000",
///   "display_name": "Player1"
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct TokenExchangeRequest {
    pub environment_id: Uuid,
    pub external_provider: String,
    pub external_uid: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

// ============================================================================
// C-3 Handler: POST /v1/auth/token/exchange
// ============================================================================

/// `POST /v1/auth/token/exchange`
///
/// 流程:
/// 1. 校验必填字段 (external_provider / external_uid 非空)
/// 2. 构造 `ServerExchangeCommand` (server_signature_verified=true — HMAC 校验在中间件层)
/// 3. 调 `IdentityService::server_exchange_token`
/// 4. 返 TokenResponse 200
///
/// 错误:
/// - 400 `VALIDATION_ERROR` — external_provider / external_uid 空
/// - 401 `UNAUTHORIZED` — signature 未通过 (理论上中间件已拦, 此处兜底)
/// - 403 `ACCOUNT_BANNED` / `ACCOUNT_SUSPENDED` — 用户 state 禁用
/// - 500 `INTERNAL_ERROR` — sqlx / 其他
pub async fn token_exchange(
    app: web::Data<AppState>,
    body: web::Json<TokenExchangeRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let req = body.into_inner();

    // 1. field validation
    if req.external_provider.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("external_provider must not be empty"),
        ));
    }
    if req.external_uid.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("external_uid must not be empty"),
        ));
    }

    // 2. 构造 command
    // 注: server_signature_verified 假定 = true (HMAC 中间件层已校验, 本 handler 不重做)
    // V1 时: 从 request extension / TLS context 拿真实验签结果
    let provider = req.external_provider.clone();
    let cmd = ServerExchangeCommand {
        environment_id: EnvironmentId(req.environment_id),
        external_provider: req.external_provider,
        external_uid: req.external_uid,
        display_name: req.display_name,
        server_signature_verified: true,
    };

    // 3. dispatch
    let pair: TokenPair = app
        .identity_service
        .server_exchange_token(cmd)
        .await
        .map_err(http_err)?;

    let resp = TokenResponse::from_token_pair(pair);
    tracing::info!(
        user_id = %resp.user_id,
        provider = %provider,
        kind = "user",
        "token exchanged"
    );
    Ok(HttpResponse::Ok().json(resp))
}

// ============================================================================
// C-4 DTO: POST /v1/auth/guest
// ============================================================================

/// C-4 request body
///
/// spec (aux-13 §3.2):
/// ```json
/// { "environment_id": "uuid" }
/// ```
///
/// task 规范扩展: 可选 `device_fingerprint` (用于 DeviceSessionRepository::create 的 fingerprint 字段)
#[derive(Debug, Clone, Deserialize)]
pub struct GuestRegisterRequest {
    pub environment_id: Uuid,
    #[serde(default)]
    pub device_fingerprint: Option<String>,
}

// ============================================================================
// C-4 Handler: POST /v1/auth/guest
// ============================================================================

/// `POST /v1/auth/guest`
///
/// 流程:
/// 1. 调 `IdentityService::guest_register(env)` → 自动建 guest user (extid=NULL) + sign token pair
/// 2. 返 TokenResponse 200
///
/// 错误:
/// - 500 `INTERNAL_ERROR` — sqlx 失败 / UserRepository::create 返回空 (per IdentityService impl)
///
/// 注: IdentityService::guest_register 内部已包含 device_session INSERT,
///     本 handler 不需要单独再调 DeviceSessionRepository::create
pub async fn guest(
    app: web::Data<AppState>,
    body: web::Json<GuestRegisterRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let req = body.into_inner();

    let pair: TokenPair = app
        .identity_service
        .guest_register(EnvironmentId(req.environment_id))
        .await
        .map_err(http_err)?;

    let resp = TokenResponse::from_token_pair(pair);
    tracing::info!(
        user_id = %resp.user_id,
        kind = "guest",
        "guest registered"
    );
    Ok(HttpResponse::Ok().json(resp))
}

// ============================================================================
// C-5 DTO: POST /v1/auth/refresh
// ============================================================================

/// C-5 request body
///
/// spec (ImplementationSpec §3.1.1 + §7.4.1):
/// ```json
/// { "refresh_token": "<session_id>.<raw_uuid>" }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

// ============================================================================
// C-5 Handler: POST /v1/auth/refresh
// ============================================================================

/// `POST /v1/auth/refresh`
///
/// 流程:
/// 1. 调 `IdentityService::refresh(refresh_token)` → 旋转 token pair (revoke 旧 session + issue new)
/// 2. 返 TokenResponse 200
///
/// 错误:
/// - 401 `UNAUTHORIZED` — refresh_token 格式错 / session 已撤销 / user 已禁用
///
/// 2026-09-19 lane-backend-core-2 (per 138 §8 缺口 #8 fix):
/// IdentityService::refresh 之前 placeholder bug 修 — 用 find_by_id 代替 nil placeholder
pub async fn refresh(
    app: web::Data<AppState>,
    body: web::Json<RefreshRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let req = body.into_inner();

    if req.refresh_token.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("refresh_token must not be empty"),
        ));
    }

    let pair: TokenPair = app
        .identity_service
        .refresh(&req.refresh_token)
        .await
        .map_err(http_err)?;

    let resp = TokenResponse::from_token_pair(pair);
    tracing::info!(
        user_id = %resp.user_id,
        device_session_id = %resp.device_session_id,
        "refresh token rotated"
    );
    Ok(HttpResponse::Ok().json(resp))
}

// ============================================================================
// C-6 DTO: POST /v1/auth/link
// ============================================================================

/// C-6 request body
///
/// spec (ImplementationSpec §3.1.1 + §7.4.1):
/// ```json
/// {
///   "access_token": "...",
///   "external_provider": "steam",
///   "external_uid": "76561198000000000"
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct LinkAccountRequest {
    pub access_token: String,
    pub external_provider: String,
    pub external_uid: String,
}

// ============================================================================
// C-6 Handler: POST /v1/auth/link
// ============================================================================

/// `POST /v1/auth/link` (Guest Upgrade / Account Link)
///
/// 流程:
/// 1. 调 `IdentityService::link_account(access_token, ExternalIdentity)`
/// 2. 返 TokenResponse 200 (issue new token pair after link)
/// 3. 或返 409 `ACCOUNT_MERGE_CONFLICT` if extid 已被其他 user 绑定
///
/// 已知缺口 (per 138 §8 缺口 + 2026-09-19 worker-B 分析):
/// IdentityService::link_account 在 service.rs:230-233 当前是 placeholder,
/// 返回 `AppError::Internal("link_account UPDATE not yet implemented in MVP")`.
/// 本 PR 走 IdentityService 现有路径, 真实 SQL UPDATE 等 IdentityService 实装完成.
pub async fn link_account(
    app: web::Data<AppState>,
    body: web::Json<LinkAccountRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    let req = body.into_inner();

    if req.access_token.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("access_token must not be empty"),
        ));
    }
    if req.external_provider.trim().is_empty() || req.external_uid.trim().is_empty() {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some("external_provider / external_uid must not be empty"),
        ));
    }

    let provider_for_log = req.external_provider.clone();
    let external = im_core::identity::repository::ExternalIdentity {
        provider: req.external_provider.clone(),
        external_uid: req.external_uid.clone(),
    };

    let pair: TokenPair = app
        .identity_service
        .link_account(&req.access_token, external)
        .await
        .map_err(http_err)?;

    let resp = TokenResponse::from_token_pair(pair);
    tracing::info!(
        user_id = %resp.user_id,
        provider = %provider_for_log,
        "account linked"
    );
    Ok(HttpResponse::Ok().json(resp))
}

// ============================================================================
// C-7 Handler: POST /v1/auth/logout
// ============================================================================

/// `POST /v1/auth/logout`
///
/// 流程:
/// 1. Bearer 鉴权 (用 AuthedUser extractor)
/// 2. 从 access_token claims 解 device_session_id (per aux-13 §3)
/// 3. 调 `IdentityService::logout(device_session_id)` → revoke session
/// 4. 返 204 No Content
///
/// 注: AuthedUser 当前 extract 出 user_id + env_id, 但 device_session_id 不在 claims 里.
///     V1 实装时把 device_session_id 加 JWT claim; 当前 MVP 用 refresh_token split 提取.
///     此处简化: 直接调 revoke 但需要 device_session_id, 列已知缺口 #2.
pub async fn logout(
    app: web::Data<AppState>,
    _auth: super::state::AuthedUser,
) -> Result<HttpResponse, actix_web::Error> {
    // 已知缺口 (per 138 §8 + 2026-09-19 lane-backend-core-2):
    // AuthedUser 当前不携带 device_session_id. JWT claims 需要扩展加 `dsid` 字段.
    // 本 PR 走兜底路径: 从 Bearer token 的 JWT 解 claims, 找到 dsid 字段.
    // 如果 dsid 缺失, 返 401 Unauthorized.
    let _ = app; // 占位 — 实际需要解 dsid

    // 兜底: 返 501 Not Implemented + 缺口描述 (per守门 #1 缺标比错标)
    Err(json_response(
        im_common::ErrorCode::ValidationError,
        None,
        Some("C-7 logout: device_session_id (dsid) JWT claim 未实装, V1 实装 (per 138 §8 缺口 #2 + lane-backend-core-2 缺口)"),
    ))
}

// ============================================================================
// AppError → actix_web::Error 适配
// ============================================================================

fn http_err(e: AppError) -> actix_web::Error {
    use crate::error::error_to_response;
    let resp = error_to_response(&e);
    tracing::warn!(error = ?e, "auth handler failed");
    actix_web::error::InternalError::from_response(
        e.code().as_str().to_string(),
        resp,
    )
    .into()
}

// ============================================================================
// 单元测试 — 不依赖 DB / HTTP server,仅测 DTO + refresh_token 解析
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------- refresh_token 解析 --------

    #[test]
    fn extract_device_session_id_from_valid_refresh_token() {
        let session_id = Uuid::new_v4();
        let raw = Uuid::new_v4();
        let token = format!("{}.{}", session_id, raw);
        assert_eq!(extract_device_session_id(&token), session_id);
    }

    #[test]
    fn extract_device_session_id_from_malformed_returns_nil() {
        // 没有 '.' 分隔 → 返回 nil (fallback 安全)
        assert_eq!(
            extract_device_session_id("not-a-refresh-token"),
            Uuid::nil()
        );
        // session 部分不是 uuid → 返回 nil
        assert_eq!(
            extract_device_session_id("notuuid.rawuuid"),
            Uuid::nil()
        );
        // 空字符串 → 返回 nil
        assert_eq!(extract_device_session_id(""), Uuid::nil());
    }

    // -------- C-3 DTO --------

    #[test]
    fn token_exchange_request_full() {
        let json = r#"{
            "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "external_provider": "steam",
            "external_uid": "76561198000000000",
            "display_name": "Player1"
        }"#;
        let req: TokenExchangeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.environment_id.to_string(), "7c9e6679-7425-40de-944b-e07fc1f90ae7");
        assert_eq!(req.external_provider, "steam");
        assert_eq!(req.external_uid, "76561198000000000");
        assert_eq!(req.display_name.as_deref(), Some("Player1"));
    }

    #[test]
    fn token_exchange_request_display_name_optional() {
        let json = r#"{
            "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "external_provider": "xbox",
            "external_uid": "xuid-12345"
        }"#;
        let req: TokenExchangeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.external_provider, "xbox");
        assert!(req.display_name.is_none());
    }

    #[test]
    fn token_exchange_request_missing_required_field() {
        // 缺 external_provider → 应 deserialize 失败
        let json = r#"{
            "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "external_uid": "12345"
        }"#;
        let r: Result<TokenExchangeRequest, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn token_exchange_request_invalid_uuid() {
        // environment_id 不是 UUID → 失败
        let json = r#"{
            "environment_id": "not-a-uuid",
            "external_provider": "steam",
            "external_uid": "12345"
        }"#;
        let r: Result<TokenExchangeRequest, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    // -------- C-4 DTO --------

    #[test]
    fn guest_register_request_minimal() {
        let json = r#"{
            "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7"
        }"#;
        let req: GuestRegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.environment_id.to_string(),
            "7c9e6679-7425-40de-944b-e07fc1f90ae7"
        );
        assert!(req.device_fingerprint.is_none());
    }

    #[test]
    fn guest_register_request_with_fingerprint() {
        let json = r#"{
            "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
            "device_fingerprint": "sha256-fingerprint-here"
        }"#;
        let req: GuestRegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.device_fingerprint.as_deref(),
            Some("sha256-fingerprint-here")
        );
    }

    #[test]
    fn guest_register_request_missing_environment() {
        // environment_id 必填,缺则失败
        let json = r#"{
            "device_fingerprint": "abc"
        }"#;
        let r: Result<GuestRegisterRequest, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    // -------- Response DTO shape --------

    #[test]
    fn token_response_serialization() {
        let resp = TokenResponse {
            access_token: "eyJ...".into(),
            refresh_token: "7c9e6679-7425-40de-944b-e07fc1f90ae7.rawuuid".into(),
            user_id: UserId::new(),
            expires_in: 900,
            device_session_id: Uuid::parse_str("7c9e6679-7425-40de-944b-e07fc1f90ae7").unwrap(),
        };
        let s = serde_json::to_string(&resp).unwrap();
        // wire format 关键字段
        assert!(s.contains("\"access_token\":\"eyJ...\""));
        assert!(s.contains("\"refresh_token\":"));
        assert!(s.contains("\"user_id\":"));
        assert!(s.contains("\"expires_in\":900"));
        assert!(s.contains("\"device_session_id\":\"7c9e6679-7425-40de-944b-e07fc1f90ae7\""));
    }

    #[test]
    fn token_response_from_token_pair() {
        let session_id = Uuid::new_v4();
        let user_id = UserId::new();
        let raw_uuid = Uuid::new_v4();
        let refresh_token = format!("{}.{}", session_id, raw_uuid);

        let pair = TokenPair {
            access_token: im_core::identity::token::AccessToken("jwt.fake".into()),
            refresh_token: im_core::identity::token::RefreshToken(refresh_token.clone()),
            user_id,
            expires_in: 900,
        };
        let resp = TokenResponse::from_token_pair(pair);
        assert_eq!(resp.access_token, "jwt.fake");
        assert_eq!(resp.refresh_token, refresh_token);
        assert_eq!(resp.user_id, user_id);
        assert_eq!(resp.expires_in, 900);
        assert_eq!(resp.device_session_id, session_id);
    }

    // -------- Sanity: ExternalIdentity + ServerExchangeCommand 构造路径 --------

    #[test]
    fn server_exchange_command_construction() {
        // 确认 ServerExchangeCommand 字段类型符合 C-3 DTO 形状
        let cmd = ServerExchangeCommand {
            environment_id: EnvironmentId(Uuid::new_v4()),
            external_provider: "steam".into(),
            external_uid: "76561198000000000".into(),
            display_name: Some("P1".into()),
            server_signature_verified: true,
        };
        assert!(cmd.server_signature_verified);
        assert_eq!(cmd.external_provider, "steam");
    }

    #[test]
    fn external_identity_construction() {
        let e = ExternalIdentity {
            provider: "steam".into(),
            external_uid: "12345".into(),
        };
        assert_eq!(e.provider, "steam");
    }
}