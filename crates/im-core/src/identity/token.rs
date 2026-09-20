//! Token Service — Access / Refresh / Rotation
//!
//! 依据: ImplementationSpec §7.4.1 + DetailedDesign §9.2
//!
//! 2026-09-20 C-3 WBS 新增: 支持 RS256 算法(用于 username/password 注册登录路径),
//! 同时保留原 HS256(用于游戏服务器 Token Exchange IM-ID-003)。
//! 实际算法由 `TokenService::signing_algorithm` 在构造时决定;注册路径注入 RS256 配置,
//! 游戏服务器路径注入 HS256 配置。
//! 注意:同一 TokenPair 的 access + refresh 共享同一算法的 signing_key(单算法服务)。

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
}

/// JWT 签名密钥(含 kid, 用于轮换期双密钥校验)
///
/// 2026-09-20 C-3 WBS 新增 `algorithm` 字段 + RSA 公钥:
/// - HS256: key=SecretString(对称)
/// - RS256: key=SecretString(PKCS8 PEM 编码私钥), public_key=PEM 编码 RSA 公钥
/// 注:本 PR 暂不引入 im_common::config(secret crate 暂未实装),这里直接定义本地版本
/// V1 整合时统一从 im_common::config 引用
#[derive(Debug, Clone)]
pub struct SigningKey {
    pub kid: String,
    pub key: secrecy::SecretString,
    /// RS256 用:PKCS8 PEM 编码的 RSA 公钥(用于校验);HS256 时为 None
    pub public_key: Option<secrecy::SecretString>,
    /// 算法:HS256(对称, 默认) / RS256(非对称, 用于 username/password 路径)
    pub algorithm: Algorithm,
}

impl SigningKey {
    /// 构造 HS256 对称密钥(用于游戏服务器 Token Exchange)
    pub fn hs256(kid: impl Into<String>, key: SecretString) -> Self {
        Self { kid: kid.into(), key, public_key: None, algorithm: Algorithm::HS256 }
    }

    /// 构造 RS256 非对称密钥对(用于 username/password 注册/登录)
    /// `key` = PKCS8 PEM 编码私钥(签名);`public_key` = PEM 公钥(校验)
    pub fn rs256(kid: impl Into<String>, key: SecretString, public_key: SecretString) -> Self {
        Self {
            kid: kid.into(),
            key,
            public_key: Some(public_key),
            algorithm: Algorithm::RS256,
        }
    }
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
        // 2026-09-20 C-3 WBS 校验:所有 signing_key 算法必须一致
        // (HS256 TokenService 不能验证 RS256 token,反之亦然;避免混用导致的隐患)
        let first_algo = signing_keys[0].algorithm;
        for k in &signing_keys[1..] {
            assert_eq!(
                k.algorithm, first_algo,
                "all signing_keys must use the same algorithm (got {:?} and {:?})",
                first_algo, k.algorithm
            );
        }
        // RS256 密钥必须提供 public_key(用于校验)
        if first_algo == Algorithm::RS256 {
            for k in &signing_keys {
                assert!(
                    k.public_key.is_some(),
                    "RS256 signing_key '{}' missing public_key",
                    k.kid
                );
            }
        }
        Self {
            signing_keys,
            access_ttl,
            _refresh_pepper: refresh_pepper,
        }
    }

    /// 当前服务所用的 JWT 算法(HS256 / RS256)
    #[inline]
    pub fn signing_algorithm(&self) -> Algorithm {
        self.signing_keys[0].algorithm
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

        let mut header = Header::new(key.algorithm);
        header.kid = Some(key.kid.clone());

        let encoding_key = encoding_key_for(key)?;
        let token = encode(&header, &claims, &encoding_key)
            .map_err(|e| TokenError::Jwt(e.to_string()))?;

        Ok(AccessToken(token))
    }

    /// 校验 Access Token(双密钥:轮换期 v1+v2 同时可验)
    pub fn validate_access_token(&self, token: &str) -> Result<TokenClaims, TokenError> {
        // 1. 先用未签发 kid 解 header
        let header = jsonwebtoken::decode_header(token).map_err(|e| TokenError::Jwt(e.to_string()))?;
        let kid = header.kid.ok_or_else(|| TokenError::Jwt("missing kid".into()))?;
        let header_algo = header.alg;

        // 2. 找匹配的 signing key
        let key = self
            .signing_keys
            .iter()
            .find(|k| k.kid == kid)
            .ok_or_else(|| TokenError::UnknownKid(kid.clone()))?;

        // 3. 算法必须与 TokenService 配置一致
        if header_algo != key.algorithm {
            return Err(TokenError::Jwt(format!(
                "algorithm mismatch: header={:?}, configured={:?}",
                header_algo, key.algorithm
            )));
        }

        // 4. 校验 + 解码
        let mut validation = Validation::new(key.algorithm);
        validation.validate_exp = true;
        let decoding_key = decoding_key_for(key)?;
        let data = decode::<TokenClaims>(token, &decoding_key, &validation)
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

/// 2026-09-20 C-3 WBS: 把 SigningKey 映射到 jsonwebtoken::EncodingKey
fn encoding_key_for(key: &SigningKey) -> Result<EncodingKey, TokenError> {
    match key.algorithm {
        Algorithm::HS256 => Ok(EncodingKey::from_secret(key.key.expose_secret().as_bytes())),
        Algorithm::RS256 => EncodingKey::from_rsa_pem(key.key.expose_secret().as_bytes())
            .map_err(|e| TokenError::Jwt(format!("invalid RSA PEM private key: {}", e))),
        other => Err(TokenError::Jwt(format!("unsupported algorithm: {:?}", other))),
    }
}

/// 2026-09-20 C-3 WBS: 把 SigningKey 映射到 jsonwebtoken::DecodingKey
///   HS256: 用 key;RS256: 用 public_key
fn decoding_key_for(key: &SigningKey) -> Result<DecodingKey, TokenError> {
    match key.algorithm {
        Algorithm::HS256 => Ok(DecodingKey::from_secret(key.key.expose_secret().as_bytes())),
        Algorithm::RS256 => {
            let pem = key
                .public_key
                .as_ref()
                .ok_or_else(|| TokenError::Jwt("RS256 missing public_key".into()))?;
            DecodingKey::from_rsa_pem(pem.expose_secret().as_bytes())
                .map_err(|e| TokenError::Jwt(format!("invalid RSA PEM public key: {}", e)))
        }
        other => Err(TokenError::Jwt(format!("unsupported algorithm: {:?}", other))),
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
            username: Some("test_user".into()),
            password_hash: None,
            created_at: Utc::now(),
        }
    }

    fn key(kid: &str) -> SigningKey {
        SigningKey::hs256(
            kid,
            SecretString::new("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
        )
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

