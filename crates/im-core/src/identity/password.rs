//! Argon2 密码哈希 / 校验
//!
//! MVP 范围: User 密码(目前仅 Game ID-Token 路径不需要密码,
//! 但留作 OAuth provider 兼容 / 未来 server-side 账户)
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
}

/// 密码校验失败统一归到 Unauthorized(不暴露 argon2 内部错误给客户端,避免探测)
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
}
