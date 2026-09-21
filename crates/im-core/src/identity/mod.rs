//! Identity 模块 — User / Guest / Token / DeviceSession
//!
//! 依据: ImplementationSpec §7.4.1

pub mod password;
pub mod pg;
pub mod repository;
pub mod service;
#[cfg(test)]
mod tests;
pub mod token;

pub use password::{
    hash_password, validate_password_strength, validate_username, verify_password, PasswordError,
};
pub use repository::{DeviceSessionRepository, UserRepository};
pub use service::IdentityService;
pub use token::{TokenService, AccessToken, RefreshToken, TokenClaims, TokenPair};
