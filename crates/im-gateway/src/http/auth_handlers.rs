//! `/v1/auth/token/exchange` + `/v1/auth/guest` HTTP handlers — WBS C-3 + C-4
//!
//! 依据: aux-13-protocol-frame-samples.md §3.1 (C-3 token_exchange) + §3.2 (C-4 guest)
//!       ImplementationSpec §3.1.1 + §7.4.1
//!
//! ## 范围 (per 132-wbs.md §5.3.2 + task brief 2026-09-19)
//!
//! ### C-3 `POST /v1/auth/token/exchange`
//! - 游戏服务器 token 兑换
//! - **2026-10-03 安全修复**: 本 handler 此前**假定**"HMAC 签名已在中间件层验证"
//!   并把 `server_signature_verified` 硬编码为 `true`, 但该中间件从未实现
//!   (`http/auth.rs` 明说"留给 auth-lane worker")。结果是任何能访问本端点的人
//!   POST 任意 `external_uid` 即可换到该账号的 access token。
//!   现改为**真实验签** (per aux-13 §3.1):
//!   `X-IM-Server-Signature` = hex(HMAC-SHA256(server_secret, 原始 body)),
//!   外加 `X-IM-Timestamp` 新鲜度校验 (±300s), 由
//!   `IdentityService::verify_server_signature` 承担。
//! - nonce 防重放**尚未实现**(需 Valkey 共享存储, WBS D-4), 见 gap-ledger
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
//!
//! 不测真实 sqlx 调用 (留 PG 集成测试,WSL PG 18.6 未启 → FAIL 是已知)

use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;
use im_core::identity::service::ServerExchangeCommand;
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
/// 1. 读取**原始 body 字节**并解析 DTO (签名基于原始字节, 不能先反序列化)
/// 2. 校验 `X-IM-Server-Signature` + `X-IM-Timestamp` (per aux-13 §3.1)
/// 3. 构造 `ServerExchangeCommand` (`server_signature_verified` = 第 2 步的真实结果)
/// 4. 调 `IdentityService::server_exchange_token`
/// 5. 返 TokenResponse 200
///
/// 错误:
/// - 400 `VALIDATION_ERROR` — external_provider / external_uid 空 / body 非合法 JSON
/// - 401 `UNAUTHORIZED` — 签名缺失/不匹配/时间戳不新鲜/该环境未配 server secret
/// - 403 `ACCOUNT_BANNED` / `ACCOUNT_SUSPENDED` — 用户 state 禁用
/// - 500 `INTERNAL_ERROR` — sqlx / 其他
pub async fn token_exchange(
    app: web::Data<AppState>,
    body: web::Bytes,
    http_req: HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    // 2026-10-03 安全修复(可利用漏洞): 此前本 handler 用 `web::Json<..>` 直接
    // 反序列化, 并把 `server_signature_verified` **硬编码为 true**, 注释假定
    // "HMAC 在中间件层已校验" —— 但该中间件从未实现(auth.rs 明说留给后续
    // worker)。后果: 任何能访问本端点的人 POST 任意 external_uid 即可换到
    // 该账号的 access token。现改为真实验签。
    //
    // 签名基于**原始字节**, 所以不能用 `web::Json`(它会消费并丢弃原始 body)。
    let req: TokenExchangeRequest = serde_json::from_slice(&body).map_err(|e| {
        json_response(
            im_common::ErrorCode::ValidationError,
            None,
            Some(&format!("invalid JSON body: {e}")),
        )
    })?;

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

    // 2. 真实验签 (per aux-13 §3.1)
    let sig = header_str(&http_req, "X-IM-Server-Signature");
    let ts = header_str(&http_req, "X-IM-Timestamp").and_then(|s| s.trim().parse::<i64>().ok());
    let now = chrono::Utc::now().timestamp();
    app.identity_service
        .verify_server_signature(
            EnvironmentId(req.environment_id),
            &body,
            sig.as_deref(),
            ts,
            now,
        )
        .map_err(|e| {
            json_response(
                im_common::ErrorCode::Unauthorized,
                None,
                Some(&e.to_string()),
            )
        })?;

    // 3. 构造 command —— 走到这里说明验签已通过
    let provider = req.external_provider.clone();
    let cmd = ServerExchangeCommand {
        environment_id: EnvironmentId(req.environment_id),
        external_provider: req.external_provider,
        external_uid: req.external_uid,
        display_name: req.display_name,
        // 走到这里说明 `verify_server_signature` 已返回 Ok; 这个 true 是上一步
        // 验签成功的**结果**, 不再是"假定中间件已验过"的硬编码。
        server_signature_verified: true,
    };

    // 4. dispatch
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

/// 读一个 header 并转为 owned String; 缺失或非 UTF-8 时返回 None
fn header_str(req: &HttpRequest, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
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
    // 守门 #1 缺口台账: 缺口 #C — task 规范要求的可选 fingerprint,
    // 待 IdentityService::guest_register 支持 fingerprint 入参后写入 (per 本 struct doc)。
    // wire 契约字段,先保留不删,per docs/Project-Status.md §1.1.1 占位符保留约定。
    #[allow(dead_code)]
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
    actix_web::error::InternalError::from_response(e.code().as_str().to_string(), resp).into()
}

// ============================================================================
// 单元测试 — 不依赖 DB / HTTP server,仅测 DTO + refresh_token 解析
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    // 非测试路径用全限定名 `im_core::identity::repository::ExternalIdentity` 构造,
    // 短名仅本测试模块需要 (per external_identity_construction)。
    use im_core::identity::repository::ExternalIdentity;

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
        assert_eq!(extract_device_session_id("notuuid.rawuuid"), Uuid::nil());
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
        assert_eq!(
            req.environment_id.to_string(),
            "7c9e6679-7425-40de-944b-e07fc1f90ae7"
        );
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

    // ========================================================================
    // C-3 端到端: handler 层真的调用了验签吗?
    // ========================================================================
    //
    // 2026-10-03 修的漏洞本质是 **handler 压根没调用验签**, 只测 service 层的
    // `verify_server_signature` 证明不了这一点 —— handler 完全可以再次忘记调用。
    // 所以这里走 actix test harness 发**真实 HTTP 请求**, 锁住端点行为。
    //
    // 因为 `AppState.identity_service` 的类型写死为
    // `IdentityService<PgUserRepository, PgDeviceSessionRepository>`, 这组用例
    // **必须连真 PG**; 未设 `DATABASE_URL` 时全部跳过。

    fn test_db_url() -> Option<String> {
        std::env::var("DATABASE_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    /// 构造完整 AppState(PG pool + 测试用 server secret)
    async fn test_app_state(
        p: &sqlx::PgPool,
        env: EnvironmentId,
        secret: &str,
    ) -> web::Data<AppState> {
        use im_core::conversation::pg::PgConversationRepository;
        use im_core::conversation::service::ConversationService;
        use im_core::event::publisher::NatsEventPublisher;
        use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
        use im_core::identity::service::IdentityService;
        use im_core::identity::token::{SigningKey, TokenService};
        use im_core::message::pg::{PgMessageRepository, PgSequenceAllocator};
        use im_core::message::service::MessageService;

        let conv_repo = Arc::new(PgConversationRepository::new(p.clone()));
        let conversation_service = Arc::new(ConversationService::new(conv_repo.clone()));
        let message_service = Arc::new(MessageService::new(
            Arc::new(PgMessageRepository::new(p.clone())),
            Arc::new(PgSequenceAllocator::new(p.clone())),
            Arc::new(
                NatsEventPublisher::connect("")
                    .await
                    .expect("stub publisher"),
            ),
            conv_repo,
        ));
        let token_service = Arc::new(TokenService::new(
            vec![SigningKey {
                kid: "v1".into(),
                key: secrecy::SecretString::new(
                    "test-key-must-be-32-bytes-or-more-padding-padding".into(),
                ),
            }],
            chrono::Duration::seconds(900),
            secrecy::SecretString::new("test-pepper".into()),
        ));
        let mut secrets = std::collections::HashMap::new();
        secrets.insert(env, secrecy::SecretString::new(secret.to_string()));
        let identity_service = Arc::new(IdentityService::new(
            PgUserRepository::new(p.clone()),
            PgDeviceSessionRepository::new(p.clone()),
            token_service.clone(),
            secrets,
        ));

        web::Data::new(AppState::new(
            conversation_service,
            message_service,
            token_service,
            identity_service,
        ))
    }

    /// 建 tenant/game/environment 三级 fixture(与 im-core 集成测试同构)
    async fn make_env(p: &sqlx::PgPool) -> EnvironmentId {
        let env_id: Uuid = sqlx::query_scalar(
            r#"
            WITH t AS (
                INSERT INTO tenants (id, name)
                VALUES (gen_random_uuid(), 'auth-e2e-tenant-' || gen_random_uuid()::text)
                RETURNING id
            ), g AS (
                INSERT INTO games (id, tenant_id, name)
                SELECT gen_random_uuid(), t.id, 'auth-e2e-game-' || gen_random_uuid()::text FROM t
                RETURNING id
            )
            INSERT INTO environments (id, game_id, name)
            SELECT gen_random_uuid(), g.id, 'test' FROM g
            RETURNING id
            "#,
        )
        .fetch_one(p)
        .await
        .expect("建 fixture 失败");
        EnvironmentId(env_id)
    }

    async fn e2e_pool() -> Option<sqlx::PgPool> {
        let url = test_db_url()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&url)
            .await
            .ok()
    }

    const E2E_SECRET: &str = "e2e-server-secret-for-token-exchange";

    fn sign_body(secret: &str, body: &[u8]) -> String {
        im_core::common::crypto::hmac_sha256_hex(secret.as_bytes(), body)
    }

    /// 无签名 / 错签名 / 过期时间戳 → 401, 且**库里不得产生 user**
    #[actix_web::test]
    async fn token_exchange_without_valid_signature_returns_401_and_creates_nothing() {
        let Some(p) = e2e_pool().await else { return };
        let env = make_env(&p).await;
        let state = test_app_state(&p, env, E2E_SECRET).await;

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state)
                .route("/v1/auth/token/exchange", web::post().to(token_exchange)),
        )
        .await;

        // uid 每次运行唯一: 否则本测试**不可重入** —— 第二次跑时库里已有上一轮
        // 留下的 user, `count == 1` 断言会假失败(变异测试中实测到 left: 2)。
        let uid = format!("e2e-attacker-{}", Uuid::new_v4());
        let body = format!(
            r#"{{"environment_id":"{}","external_provider":"steam","external_uid":"{uid}"}}"#,
            env.0
        );
        let now = chrono::Utc::now().timestamp();

        // (a) 完全无签名头
        let req = actix_web::test::TestRequest::post()
            .uri("/v1/auth/token/exchange")
            .set_json(serde_json::from_str::<serde_json::Value>(&body).unwrap())
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::UNAUTHORIZED,
            "无签名必须 401"
        );

        // (b) 签名错误
        let req = actix_web::test::TestRequest::post()
            .uri("/v1/auth/token/exchange")
            .insert_header((
                "X-IM-Server-Signature",
                sign_body("wrong-secret", body.as_bytes()),
            ))
            .insert_header(("X-IM-Timestamp", now.to_string()))
            .set_json(serde_json::from_str::<serde_json::Value>(&body).unwrap())
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        // (c) 时间戳过期(签名正确也没用)
        let stale = now - 100_000;
        let req = actix_web::test::TestRequest::post()
            .uri("/v1/auth/token/exchange")
            .insert_header((
                "X-IM-Server-Signature",
                sign_body(E2E_SECRET, body.as_bytes()),
            ))
            .insert_header(("X-IM-Timestamp", stale.to_string()))
            .set_json(serde_json::from_str::<serde_json::Value>(&body).unwrap())
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        // 关键断言: 三次被拒之后, 库里绝不能出现该 external_uid 的 user
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM users WHERE external_identity->>'external_uid' = $1",
        )
        .bind(&uid)
        .fetch_one(&p)
        .await
        .expect("count users");
        assert_eq!(count, 0, "验签失败的请求不得创建任何 user");
    }

    /// 正确签名 → 200 且真的换出 token pair
    #[actix_web::test]
    async fn token_exchange_with_valid_signature_returns_200() {
        let Some(p) = e2e_pool().await else { return };
        let env = make_env(&p).await;
        let state = test_app_state(&p, env, E2E_SECRET).await;

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(state)
                .route("/v1/auth/token/exchange", web::post().to(token_exchange)),
        )
        .await;

        // uid 每次运行唯一(同 attacker 用例: 固定 uid 会让本测试不可重入)
        let uid = format!("e2e-valid-{}", Uuid::new_v4());
        let body = format!(
            r#"{{"environment_id":"{}","external_provider":"steam","external_uid":"{uid}","display_name":"E2E"}}"#,
            env.0
        );
        let now = chrono::Utc::now().timestamp();
        let sig = sign_body(E2E_SECRET, body.as_bytes());

        let req = actix_web::test::TestRequest::post()
            .uri("/v1/auth/token/exchange")
            .insert_header(("Content-Type", "application/json"))
            .insert_header(("X-IM-Server-Signature", sig))
            .insert_header(("X-IM-Timestamp", now.to_string()))
            .set_payload(body.clone())
            .to_request();

        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::OK,
            "正确签名应换出 token"
        );

        let json: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(
            !json["access_token"].as_str().unwrap_or("").is_empty(),
            "响应应含非空 access_token"
        );
        assert!(!json["refresh_token"].as_str().unwrap_or("").is_empty());

        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM users WHERE external_identity->>'external_uid' = $1",
        )
        .bind(&uid)
        .fetch_one(&p)
        .await
        .expect("count users");
        assert_eq!(count, 1, "验签通过应真的落库一个 user");
    }
}
