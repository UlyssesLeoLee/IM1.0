//! Argon2 密码哈希 / 校验
//!
//! MVP 范围: User 密码(目前仅 Game ID-Token 路径不需要密码,
//! 但留作 OAuth provider 兼容 / 未来 server-side 账户)
//!
//! 2026-09-20 C-3 WBS 新增: username / password 强度校验(register 时调用)

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("hash error: {0}")]
    Hash(String),
    #[error("verify error: {0}")]
    Verify(String),
    #[error("invalid hash format: {0}")]
    InvalidHash(String),
    /// 2026-09-20 C-3 WBS: 密码强度不足
    ///   详细原因放在 message 里(用于诊断 + 用户提示)
    #[error("weak password: {0}")]
    WeakPassword(String),
    /// 2026-09-20 C-3 WBS: username 格式不合法
    ///   长度 3-32,仅允许 [a-zA-Z0-9_-]
    #[error("invalid username: {0}")]
    InvalidUsername(String),
}

// ============================================================================
// 密码 / 用户名校验(2026-09-20 C-3 WBS)
// ============================================================================

/// 密码强度策略(MVP 简单版)
///   - 长度 8-128(防 bcrypt 72 字节截断;argon2 不受 72 字节限制但仍设上限防滥用)
///   - 至少 1 个字母 + 1 个数字
///
/// 注:未做字符类型组合(如必须大小写混合),避免过度限制用户体验。
/// 实装 OWASP ASVS L1 的最小集合;更高安全等级由后续 Phase (aux-09 日志策略)审计 + 限流补充。
pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = 128;

/// 用户名格式策略
///   - 长度 3-32,仅允许 [a-zA-Z0-9_-]
pub const MIN_USERNAME_LEN: usize = 3;
pub const MAX_USERNAME_LEN: usize = 32;

/// 校验密码强度。返回 `PasswordError::WeakPassword(msg)` 表示不通过。
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
        // 两个 flag 都 true 后可以提前 break
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

/// 校验 username 格式。
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
            "username must match [a-zA-Z0-9_-]+".into(),
        ));
    }
    Ok(())
}

// ============================================================================
// Argon2 哈希 / 校验
// ============================================================================

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

    // 2026-09-20 C-3 WBS 新增密码强度校验单测

    #[test]
    fn password_strength_accepts_strong() {
        // 含字母 + 数字,长度 ≥ 8
        assert!(validate_password_strength("hunter2pw").is_ok());
        assert!(validate_password_strength("a1b2c3d4e5").is_ok());
        assert!(validate_password_strength("00000000a").is_ok());
    }

    #[test]
    fn password_strength_rejects_short() {
        // < 8
        assert!(matches!(
            validate_password_strength("a1b2c3"),
            Err(PasswordError::WeakPassword(_))
        ));
        // 空
        assert!(matches!(
            validate_password_strength(""),
            Err(PasswordError::WeakPassword(_))
        ));
    }

    #[test]
    fn password_strength_rejects_no_letter() {
        // 只有数字
        assert!(matches!(
            validate_password_strength("12345678"),
            Err(PasswordError::WeakPassword(_))
        ));
    }

    #[test]
    fn password_strength_rejects_no_digit() {
        // 只有字母
        assert!(matches!(
            validate_password_strength("abcdefgh"),
            Err(PasswordError::WeakPassword(_))
        ));
    }

    #[test]
    fn password_strength_rejects_too_long() {
        let long = "a".repeat(MAX_PASSWORD_LEN + 1);
        assert!(matches!(
            validate_password_strength(&long),
            Err(PasswordError::WeakPassword(_))
        ));
    }

    #[test]
    fn username_valid() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("alice_2026").is_ok());
        assert!(validate_username("user-name").is_ok());
        assert!(validate_username("ABC123").is_ok());
    }

    #[test]
    fn username_rejects_too_short() {
        assert!(matches!(
            validate_username("ab"),
            Err(PasswordError::InvalidUsername(_))
        ));
    }

    #[test]
    fn username_rejects_too_long() {
        let long = "a".repeat(MAX_USERNAME_LEN + 1);
        assert!(matches!(
            validate_username(&long),
            Err(PasswordError::InvalidUsername(_))
        ));
    }

    #[test]
    fn username_rejects_special_chars() {
        // 不允许 . @ ! 等
        assert!(matches!(
            validate_username("alice@host"),
            Err(PasswordError::InvalidUsername(_))
        ));
        assert!(matches!(
            validate_username("alice.foo"),
            Err(PasswordError::InvalidUsername(_))
        ));
        assert!(matches!(
            validate_username("alice!"),
            Err(PasswordError::InvalidUsername(_))
        ));
    }
}
