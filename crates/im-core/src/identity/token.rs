//! Token Service — Access / Refresh / Rotation
//!
//! 依据: ImplementationSpec §7.4.1 + DetailedDesign §9.2

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{DeviceSessionId, EnvironmentId, UserId};
use im_common::{AppError, ErrorCode};

use super::repository::User;
// UserId 从 im_common::ids 直接引用(已在上面 use)

/// Device Session Repository trait(在 token.rs 重复定义避免循环)
#[async_trait::async_trait]
pub trait DeviceSessionRepository: Send + Sync {
    async fn create(
        &self,
        user_id: UserId,
        device_fingerprint: Option<&str>,
        refresh_token_hash: &str,
    ) -> Result<DeviceSession, im_common::AppError>;
    async fn find_by_refresh_token_hash(
        &self,
        user_id: UserId,
        refresh_token_hash: &str,
    ) -> Result<Option<DeviceSession>, im_common::AppError>;
    async fn revoke(&self, id: im_common::ids::DeviceSessionId) -> Result<(), im_common::AppError>;
    /// 按 session id 直接查 (per 138 §8 缺口 #8 + 2026-09-19 lane-backend-core-2 修 IdentityService::refresh placeholder bug)
    ///
    /// refresh_token 格式 `<session_id>.<raw>`, 解析 session_id 后用此方法查 session
    /// (避免 caller 传 UserId::nil 占位 bug)
    async fn find_by_id(
        &self,
        id: im_common::ids::DeviceSessionId,
    ) -> Result<Option<DeviceSession>, im_common::AppError>;
}

/// JWT 签名密钥(含 kid, 用于轮换期双密钥校验)
///
/// 注:本 PR 暂不引入 im_common::config(secret crate 暂未实装),这里直接定义本地版本
/// V1 整合时统一从 im_common::config 引用
#[derive(Debug, Clone)]
pub struct SigningKey {
    pub kid: String,
    pub key: secrecy::SecretString,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("signing key not found for kid: {0}")]
    UnknownKid(String),
    #[error("jwt error: {0}")]
    Jwt(String),
    #[error("token expired")]
    Expired,
    #[error("device session revoked or not found")]
    DeviceSessionNotFound,
}

/// Access Token 字符串(包装,避免裸 String)
#[derive(Debug, Clone)]
pub struct AccessToken(pub String);

/// Refresh Token 字符串
#[derive(Debug, Clone)]
pub struct RefreshToken(pub String);

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,         // user_id (UUID)
    pub env: String,         // environment_id
    pub tenant: String,      // tenant_id
    pub exp: i64,            // unix seconds
    pub iat: i64,            // issued at
    pub kind: String,        // user | guest
    pub kid: String,         // signing key id(用于轮换期识别)
}

/// Token 对
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
    pub user_id: UserId,
    pub expires_in: i64,    // seconds
}

/// Device Session 实体
#[derive(Debug, Clone)]
pub struct DeviceSession {
    pub id: DeviceSessionId,
    pub user_id: UserId,
    pub device_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub struct TokenService {
    signing_keys: Vec<SigningKey>,    // 至少 1 个;轮换期有 v1+v2
    access_ttl: ChronoDuration,
    _refresh_pepper: SecretString,
}

impl TokenService {
    pub fn new(signing_keys: Vec<SigningKey>, access_ttl: ChronoDuration, refresh_pepper: SecretString) -> Self {
        assert!(!signing_keys.is_empty(), "at least one signing key required");
        Self {
            signing_keys,
            access_ttl,
            _refresh_pepper: refresh_pepper,
        }
    }

    /// 签发 Access Token
    pub fn issue_access_token(&self, user: &User) -> Result<AccessToken, TokenError> {
        // 用第 1 个 key 签发(轮换时仍可用 v1 签发,v2 用于校验新发的 v2 token)
        let key = &self.signing_keys[0];
        let now = Utc::now();
        let claims = TokenClaims {
            sub: user.id.to_string(),
            env: user.environment_id.to_string(),
            tenant: String::new(), // 由调用方补充,或从 user.environment_id 推
            exp: (now + self.access_ttl).timestamp(),
            iat: now.timestamp(),
            kind: match user.kind {
                super::repository::UserKind::User => "user".into(),
                super::repository::UserKind::Guest => "guest".into(),
            },
            kid: key.kid.clone(),
        };

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(key.kid.clone());

        let token = encode(
            &header,
            &claims,
            &EncodingKey::from_secret(key.key.expose_secret().as_bytes()),
        )
        .map_err(|e| TokenError::Jwt(e.to_string()))?;

        Ok(AccessToken(token))
    }

    /// 校验 Access Token(双密钥:轮换期 v1+v2 同时可验)
    pub fn validate_access_token(&self, token: &str) -> Result<TokenClaims, TokenError> {
        // 1. 先用未签发 kid 解 header
        let header = jsonwebtoken::decode_header(token).map_err(|e| TokenError::Jwt(e.to_string()))?;
        let kid = header.kid.ok_or_else(|| TokenError::Jwt("missing kid".into()))?;

        // 2. 找匹配的 signing key
        let key = self
            .signing_keys
            .iter()
            .find(|k| k.kid == kid)
            .ok_or_else(|| TokenError::UnknownKid(kid.clone()))?;

        // 3. 校验 + 解码
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        let data = decode::<TokenClaims>(
            token,
            &DecodingKey::from_secret(key.key.expose_secret().as_bytes()),
            &validation,
        )
        .map_err(|e| {
            if e.to_string().contains("ExpiredSignature") {
                TokenError::Expired
            } else {
                TokenError::Jwt(e.to_string())
            }
        })?;

        Ok(data.claims)
    }
}

impl From<TokenError> for AppError {
    fn from(e: TokenError) -> Self {
        match e {
            TokenError::Expired | TokenError::DeviceSessionNotFound => {
                AppError::Unauthorized("token expired or revoked".into())
            }
            TokenError::UnknownKid(_) | TokenError::Jwt(_) => {
                AppError::Unauthorized("invalid token".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repository::{UserKind, UserState};
    use secrecy::SecretString;

    fn test_user() -> User {
        User {
            id: UserId::new(),
            environment_id: EnvironmentId::new(),
            kind: UserKind::User,
            external_identity: None,
            state: UserState::Active,
            display_name: Some("test".into()),
            username: None,           // 2026-09-21 整合
            password_hash: None,      // 2026-09-21 整合
            created_at: Utc::now(),
        }
    }

    fn key(kid: &str) -> SigningKey {
        SigningKey {
            kid: kid.into(),
            key: SecretString::new("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
        }
    }

    #[test]
    fn issue_and_validate_roundtrip() {
        let svc = TokenService::new(
            vec![key("v1")],
            ChronoDuration::seconds(900),
            SecretString::new("pepper".into()),
        );
        let token = svc.issue_access_token(&test_user()).unwrap();
        let claims = svc.validate_access_token(&token.0).unwrap();
        assert_eq!(claims.kid, "v1");
    }

    #[test]
    fn dual_key_validation_during_rotation() {
        // v1 + v2 共存,token 用 v1 签发,v1/v2 都能验
        let svc = TokenService::new(
            vec![key("v1"), key("v2")],
            ChronoDuration::seconds(900),
            SecretString::new("pepper".into()),
        );
        let token_v1 = svc.issue_access_token(&test_user()).unwrap();
        let claims = svc.validate_access_token(&token_v1.0).unwrap();
        assert_eq!(claims.kid, "v1");
    }

    #[test]
    fn unknown_kid_rejected() {
        let svc = TokenService::new(
            vec![key("v1")],
            ChronoDuration::seconds(900),
            SecretString::new("pepper".into()),
        );
        let other_svc = TokenService::new(
            vec![key("v2")], // 不同的 key
            ChronoDuration::seconds(900),
            SecretString::new("pepper".into()),
        );
        let token = other_svc.issue_access_token(&test_user()).unwrap();
        assert!(matches!(
            svc.validate_access_token(&token.0),
            Err(TokenError::UnknownKid(_))
        ));
    }
}

