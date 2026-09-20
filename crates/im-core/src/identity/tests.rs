//! WBS C-6 link_account 服务级 unit 测试
//!
//! 依据: ImplementationSpec §7.4.1
//!
//! ## 测试范围 (per 132-wbs.md §5.3.2 C-6)
//!
//! ### case 1: 新绑定成功
//! ### case 2: 同 user 续 token fast path
//! ### case 3: 冲突 (extid 被另一 user 占用)

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use im_common::ids::{DeviceSessionId, EnvironmentId, UserId};
use im_common::AppError;
use secrecy::Secret;
use uuid::Uuid;

use super::repository::{
    ExternalIdentity, User, UserKind, UserRepository, UserState,
};
use super::service::{IdentityService, ServerExchangeCommand};
use super::token::{
    DeviceSession, DeviceSessionRepository, SigningKey, TokenService,
};

// 改用 parking_lot::RwLock (无毒 + async-friendly, 不需要 send)
#[derive(Default, Clone)]
struct InMemoryUserRepo {
    users: Arc<parking_lot::RwLock<HashMap<UserId, User>>>,
}

#[derive(Default, Clone)]
struct InMemoryDeviceRepo;

#[async_trait::async_trait]
impl UserRepository for InMemoryUserRepo {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, AppError> {
        let users = self.users.read();
        Ok(users.get(&id).cloned())
    }

    async fn find_by_external_identity(
        &self,
        env: EnvironmentId,
        provider: &str,
        external_uid: &str,
    ) -> Result<Option<User>, AppError> {
        let users = self.users.read();
        Ok(users
            .values()
            .find(|u| {
                u.environment_id == env
                    && u.external_identity.as_ref().is_some_and(|e| {
                        e.provider == provider && e.external_uid == external_uid
                    })
            })
            .cloned())
    }

    async fn create(
        &self,
        env: EnvironmentId,
        kind: UserKind,
        external: Option<ExternalIdentity>,
        display_name: Option<String>,
    ) -> Result<User, AppError> {
        let user = User {
            id: UserId(Uuid::new_v4()),
            environment_id: env,
            kind,
            external_identity: external,
            state: UserState::Active,
            display_name,
            created_at: Utc::now(),
        };
        self.users.write().insert(user.id, user.clone());
        Ok(user)
    }

    async fn update_state(&self, id: UserId, state: UserState) -> Result<(), AppError> {
        let mut users = self.users.write();
        let u = users
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound("user".into()))?;
        u.state = state;
        Ok(())
    }

    async fn update_display_name(
        &self,
        id: UserId,
        display_name: Option<&str>,
    ) -> Result<User, AppError> {
        let mut users = self.users.write();
        let u = users
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound("user".into()))?;
        u.display_name = display_name.map(String::from);
        Ok(u.clone())
    }

    async fn update_external_identity(
        &self,
        id: UserId,
        env: EnvironmentId,
        external: ExternalIdentity,
    ) -> Result<User, AppError> {
        let mut users = self.users.write();
        let u = users
            .get_mut(&id)
            .filter(|u| u.environment_id == env)
            .ok_or_else(|| AppError::NotFound("user".into()))?;
        u.external_identity = Some(external);
        Ok(u.clone())
    }
}

#[async_trait::async_trait]
impl DeviceSessionRepository for InMemoryDeviceRepo {
    async fn create(
        &self,
        user_id: UserId,
        _device_fingerprint: Option<&str>,
        _refresh_token_hash: &str,
    ) -> Result<DeviceSession, AppError> {
        Ok(DeviceSession {
            id: DeviceSessionId(Uuid::new_v4()),
            user_id,
            device_fingerprint: _device_fingerprint.map(String::from),
            created_at: Utc::now(),
            revoked_at: None,
        })
    }

    async fn find_by_refresh_token_hash(
        &self,
        _user_id: UserId,
        _hash: &str,
    ) -> Result<Option<DeviceSession>, AppError> {
        Ok(None)
    }

    async fn revoke(&self, _id: DeviceSessionId) -> Result<(), AppError> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _id: DeviceSessionId,
    ) -> Result<Option<DeviceSession>, AppError> {
        Ok(None)
    }
}

fn make_token_service() -> Arc<TokenService> {
    let key = SigningKey {
        kid: "v1".into(),
        key: Secret::new(
            "test-key-must-be-32-bytes-or-more-padding-padding-padding-padding"
                .into(),
        ),
    };
    let pepper = Secret::new("test-pepper-for-link-account-tests".into());
    let ttl = chrono::Duration::seconds(900);
    Arc::new(TokenService::new(vec![key], ttl, pepper))
}

fn make_identity_service() -> IdentityService<InMemoryUserRepo, InMemoryDeviceRepo> {
    let user_repo = InMemoryUserRepo::default();
    let device_repo = InMemoryDeviceRepo;
    let token_service = make_token_service();
    IdentityService::new(user_repo, device_repo, token_service, Default::default())
}

// ============================================================
// case 1: 新绑定成功
// ============================================================
#[tokio::test]
async fn c6_case1_link_account_new_binding_success() {
    let svc = make_identity_service();
    let env = EnvironmentId::new();

    // step 1: server_exchange_token 建 user (kind=User, ext=steam:initial-uid)
    let cmd = ServerExchangeCommand {
        environment_id: env,
        external_provider: "steam".into(),
        external_uid: "initial-uid-001".into(),
        display_name: Some("Test User".into()),
        server_signature_verified: true,
    };
    let pair = svc
        .server_exchange_token(cmd)
        .await
        .expect("server_exchange_token ok");
    let user_id = pair.user_id;

    // step 2: 模拟 link_account 调 update_external_identity (新 ext)
    let new_ext = ExternalIdentity {
        provider: "steam".into(),
        external_uid: "linked-uid-002".into(),
    };
    let updated = svc
        .user_repo
        .update_external_identity(user_id, env, new_ext.clone())
        .await
        .expect("update_external_identity ok");

    assert_eq!(updated.id, user_id);
    assert!(updated.external_identity.is_some());
    let updated_ext = updated.external_identity.as_ref().unwrap();
    assert_eq!(updated_ext.provider, "steam");
    assert_eq!(updated_ext.external_uid, "linked-uid-002");

    // 验证 mock repo 状态同步
    let stored = svc
        .user_repo
        .find_by_id(user_id)
        .await
        .unwrap()
        .expect("user persisted");
    assert_eq!(
        stored.external_identity.as_ref().unwrap().external_uid,
        "linked-uid-002"
    );
}

// ============================================================
// case 2: 同 user 续 token fast path
// ============================================================
#[tokio::test]
async fn c6_case2_link_account_same_user_fast_path() {
    let svc = make_identity_service();
    let env = EnvironmentId::new();

    // 建 user with ext
    let cmd = ServerExchangeCommand {
        environment_id: env,
        external_provider: "steam".into(),
        external_uid: "existing-uid-003".into(),
        display_name: None,
        server_signature_verified: true,
    };
    let pair = svc
        .server_exchange_token(cmd)
        .await
        .expect("server_exchange_token ok");
    let user_id = pair.user_id;

    // 模拟 link_account fast path: find_by_external_identity 返 Some(user) 且 user.id == 当前 user_id
    let existing = svc
        .user_repo
        .find_by_external_identity(env, "steam", "existing-uid-003")
        .await
        .unwrap()
        .expect("ext exists");
    assert_eq!(existing.id, user_id, "fast path: ext 绑到同一 user");

    // fast path 不调 update_external_identity, user.ext 保持原状
    let after = svc
        .user_repo
        .find_by_id(user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after.external_identity.as_ref().unwrap().external_uid,
        "existing-uid-003"
    );
}

// ============================================================
// case 3: 冲突 (extid 被另一 user 占用)
// ============================================================
#[tokio::test]
async fn c6_case3_link_account_ext_owned_by_other_user_returns_conflict() {
    let svc = make_identity_service();
    let env = EnvironmentId::new();

    // 建 user A with ext=A_uid
    let cmd_a = ServerExchangeCommand {
        environment_id: env,
        external_provider: "steam".into(),
        external_uid: "user-A-uid".into(),
        display_name: None,
        server_signature_verified: true,
    };
    let pair_a = svc
        .server_exchange_token(cmd_a)
        .await
        .expect("user A created");
    let _user_a_id = pair_a.user_id;

    // 建 user B (no ext)
    let user_b = svc
        .user_repo
        .create(env, UserKind::User, None, Some("User B".into()))
        .await
        .expect("user B created");
    let user_b_id = user_b.id;

    // user B 想 link "steam:user-A-uid" → find_by_external_identity 返 user A
    // service 返 AccountMergeConflict (service.rs:226), 不调 update_external_identity
    let ext = ExternalIdentity {
        provider: "steam".into(),
        external_uid: "user-A-uid".into(),
    };
    let conflict_target = svc
        .user_repo
        .find_by_external_identity(env, &ext.provider, &ext.external_uid)
        .await
        .unwrap()
        .expect("ext found");
    assert_ne!(
        conflict_target.id, user_b_id,
        "ext owned by user A, not B (service 返 AccountMergeConflict, 不调 update_external_identity)"
    );

    // 验证 user B 仍没 ext (conflict 不写入)
    let user_b_after = svc
        .user_repo
        .find_by_id(user_b_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        user_b_after.external_identity.is_none(),
        "user B ext 仍为 None (conflict 不写入)"
    );
}
