//! Newtype IDs — 防止误用
//!
//! 依据: aux-01 §B Rust 命名 + aux-02 §F 各表 PK 字段

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[inline]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// 全零 UUID(用于"未指定"语义,实现侧可选使用)
            #[inline]
            pub fn nil() -> Self {
                Self(Uuid::nil())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<Uuid> for $name {
            #[inline]
            fn from(u: Uuid) -> Self {
                Self(u)
            }
        }

        impl From<$name> for Uuid {
            #[inline]
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }
    };
}

define_id!(UserId);
define_id!(EnvironmentId);
define_id!(TenantId);
define_id!(GameId);
define_id!(DeviceSessionId);
define_id!(ConversationId);
define_id!(MessageId);
define_id!(FriendRequestId);
define_id!(MediaId);
define_id!(AuditLogId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_new_is_unique() {
        let a = UserId::new();
        let b = UserId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn id_serde_roundtrip() {
        let id = ConversationId::new();
        let s = serde_json::to_string(&id).unwrap();
        let back: ConversationId = serde_json::from_str(&s).unwrap();
        assert_eq!(id, back);
    }
}
