//! Argon2 密码哈希 / 校验
//!
//! MVP 范围: User 密码(目前仅 Game ID-Token 路径不需要密码,
//! 但留作 OAuth provider 兼容 / 未来 server-side 账户)
//!
//! 2026-09-21 整合 (C-3 + C-4, 方案 C):
//! - C-3 贡献: validate_username + validate_password_strength + WeakPassword/InvalidUsername variant
//! - C-4 贡献: hash_password + verify_password + From<PasswordError> for AppError
//! - 整合: MAX_USERNAME_LEN 3-32 → 3-64 (兼容企业 SSO)

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

use im_common::AppError;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("hash error: {0}")]
    Hash(String),
    #[error("verify error: {0}")]
    Verify(String),
    #[error("invalid hash format: {0}")]
    InvalidHash(String),
    /// 2026-09-21 整合: C-3 密码强度不足
    #[error("weak password: {0}")]
    WeakPassword(String),
    /// 2026-09-21 整合: C-3 username 格式不合法
    ///   长度 3-64 (整合方案 C, 兼容企业 SSO), 仅允许 [a-zA-Z0-9_-]
    #[error("invalid username: {0}")]
    InvalidUsername(String),
}

// ============================================================================
// 密码 / 用户名校验 (C-3 WBS)
// ============================================================================

/// 密码强度策略 (MVP 简单版)
///   - 长度 8-128 (防 bcrypt 72 字节截断;argon2 不受 72 字节限制但仍设上限防滥用)
///   - 至少 1 个字母 + 1 个数字
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = 128;

/// 用户名格式策略 (整合方案 C: 长度上限 32 → 64)
///   - 长度 3-64, 仅允许 [a-zA-Z0-9_-]
pub const MIN_USERNAME_LEN: usize = 3;
pub const MAX_USERNAME_LEN: usize = 64;

/// 校验密码强度
pub fn validate_password_strength(password: &str) -> Result<(), PasswordError> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN {
        return Err(PasswordError::WeakPassword(format!(
            "password too short (len={}, min={})",
            len, MIN_PASSWORD_LEN
        )));
    }
    if len > MAX_PASSWORD_LEN {
        return Err(PasswordError::WeakPassword(format!(
            "password too long (len={}, max={})",
            len, MAX_PASSWORD_LEN
        )));
    }
    let mut has_letter = false;
    let mut has_digit = false;
    for ch in password.chars() {
        if ch.is_ascii_alphabetic() {
            has_letter = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        }
        if has_letter && has_digit {
            break;
        }
    }
    if !has_letter || !has_digit {
        return Err(PasswordError::WeakPassword(
            "password must contain at least one letter and one digit".into(),
        ));
    }
    Ok(())
}

/// 校验 username 格式
pub fn validate_username(username: &str) -> Result<(), PasswordError> {
    let len = username.chars().count();
    if len < MIN_USERNAME_LEN {
        return Err(PasswordError::InvalidUsername(format!(
            "username too short (len={}, min={})",
            len, MIN_USERNAME_LEN
        )));
    }
    if len > MAX_USERNAME_LEN {
        return Err(PasswordError::InvalidUsername(format!(
            "username too long (len={}, max={})",
            len, MAX_USERNAME_LEN
        )));
    }
    if !username
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(PasswordError::InvalidUsername(
            "username must contain only [a-zA-Z0-9_-]".into(),
        ));
    }
    Ok(())
}

// ============================================================================
// Argon2 hash / verify (C-4 WBS)
// ============================================================================

/// 密码校验失败统一归到 Unauthorized (不暴露 argon2 内部错误给客户端, 避免探测)
impl From<PasswordError> for AppError {
    fn from(_e: PasswordError) -> Self {
        AppError::Unauthorized("invalid username or password".into())
    }
}

/// 哈希密码(Argon2id,默认参数)
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Hash(e.to_string()))
}

/// 校验密码与哈希匹配
pub fn verify_password(password: &str, hash: &str) -> Result<bool, PasswordError> {
    let parsed = PasswordHash::new(hash).map_err(|e| PasswordError::InvalidHash(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &h).unwrap());
        assert!(!verify_password("hunter3", &h).unwrap());
    }

    #[test]
    fn hashes_are_unique() {
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert_ne!(a, b); // 不同的 salt
    }

    // ============================================================================
    // 2026-09-21 整合: validate_username + validate_password_strength 单测
    // ============================================================================

    #[test]
    fn validate_username_min_max_len() {
        assert!(validate_username("abc").is_ok());                  // 3 chars (min)
        assert!(validate_username("a").is_err());                   // 1 char (too short)
        assert!(validate_username("ab").is_err());                  // 2 chars (too short)
        assert!(validate_username(&"a".repeat(64)).is_ok());        // 64 chars (max, 整合方案 C)
        assert!(validate_username(&"a".repeat(65)).is_err());       // 65 chars (too long)
        assert!(validate_username(&"a".repeat(32)).is_ok());        // 32 chars (历史 C-3 上限, 现在 OK)
    }

    #[test]
    fn validate_username_charset() {
        assert!(validate_username("abc_DEF-123").is_ok());
        assert!(validate_username("with space").is_err());
        assert!(validate_username("with.dot").is_err());
        assert!(validate_username("with/slash").is_err());
        assert!(validate_username("with@symbol").is_err());
    }

    #[test]
    fn validate_password_strength_min_max() {
        assert!(validate_password_strength("abc1").is_err());         // too short
        assert!(validate_password_strength("abcdefg").is_err());      // no digit
        assert!(validate_password_strength("1234567").is_err());      // no letter
        assert!(validate_password_strength("abc12345").is_ok());
        assert!(validate_password_strength(&format!("{}1", "a".repeat(127))).is_ok());   // 128 chars with digit
        assert!(validate_password_strength(&"a".repeat(128)).is_err());                  // 128 chars no digit
        assert!(validate_password_strength(&format!("{}1", "a".repeat(128))).is_err()); // 129 chars (too long)
    }
}
