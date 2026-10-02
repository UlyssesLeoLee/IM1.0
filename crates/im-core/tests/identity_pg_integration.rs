//! Identity 认证路径的真 PostgreSQL 集成测试 —— **补 `server_exchange_token` /
//! `guest_register` 的端到端覆盖与几条安全隔离路径**
//!
//! ## 与既有测试的分工(重要, 别重复造)
//!
//! `tests/pg_repos_integration.rs` **已经**在真库上覆盖了 identity 仓储的基础面:
//! `user_create_guest_and_find_by_id` / `user_create_with_extid_and_find_by_extid` /
//! `user_create_without_extid_for_user_kind_rejected` / `user_update_state_banned` /
//! `device_session_create_find_revoke` / `device_session_create_find_revoke_dup_removed`
//! 以及 `IdentityService::link_account` 的 5 个用例。
//!
//! 本文件补的是**既有覆盖没有的部分**:
//! 1. `server_exchange_token` / `guest_register` 的**端到端**路径 —— 既有只测了
//!    `link_account`, 这两个是 C-3 / C-4 的主入口, 此前没有真库覆盖
//! 2. 几条**安全隔离**断言(见下), 既有测试没有覆盖到
//!
//! 个别用例与既有测试有部分重叠(如 device_session 的 create/find/revoke 基本面),
//! 这是有意的冗余: 它把安全断言和基本 CRUD 放在同一个用例里, 便于失败时定位。
//!
//! ## 运行
//!
//! 需要 `DATABASE_URL` 指向已应用 migration 的 PG 18.6; 未设置时全部用例
//! **跳过**(提前 return, 不是失败), 以免在无 DB 环境制造假红。
//!
//! ```powershell
//! $env:DATABASE_URL = "postgres://im:im@localhost:5544/im_test"
//! cargo test -p im-core --test identity_pg_integration -j 1
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use im_common::ids::{EnvironmentId, UserId};
use im_common::AppError;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::repository::UserRepository;
use im_core::identity::repository::{UserKind, UserState};
use im_core::identity::service::{IdentityService, ServerExchangeCommand};
use im_core::identity::token::{DeviceSessionRepository, SigningKey, TokenService};
use secrecy::SecretString;
use sqlx::PgPool;

// ============================================================================
// 环境准备
// ============================================================================

fn database_url() -> Option<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

async fn pool() -> Option<PgPool> {
    let url = database_url()?;
    match sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
    {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("[skip] 连不上 PG ({e}), 相关用例跳过");
            None
        }
    }
}

/// 建独立 fixture: 1 tenant + 1 game + 1 environment。
///
/// `users.environment_id` 有外键指向 `environments`(users_environment_id_fkey),
/// 所以不能只 `EnvironmentId::new()` —— 那会撞 FK 约束。必须先把
/// tenant -> game -> environment 三级建出来, 这与 `pg_repos_integration.rs`
/// 的 `make_env()` 同构。
async fn fresh_env(p: &PgPool) -> EnvironmentId {
    let env_id: uuid::Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name)
            VALUES (gen_random_uuid(), 'idtest-tenant-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'idtest-game-' || gen_random_uuid()::text FROM t
            RETURNING id
        )
        INSERT INTO environments (id, game_id, name)
        SELECT gen_random_uuid(), g.id, 'test' FROM g
        RETURNING id
        "#,
    )
    .fetch_one(p)
    .await
    .expect("建 tenant/game/environment fixture 失败");
    EnvironmentId(env_id)
}

fn ext(provider: &str, uid: &str) -> im_core::identity::repository::ExternalIdentity {
    im_core::identity::repository::ExternalIdentity {
        provider: provider.into(),
        external_uid: uid.into(),
    }
}

fn test_token_service() -> Arc<TokenService> {
    Arc::new(TokenService::new(
        vec![SigningKey {
            kid: "v1".into(),
            key: SecretString::new(
                "test-key-must-be-32-bytes-or-more-padding-padding-padding".into(),
            ),
        }],
        chrono::Duration::seconds(900),
        SecretString::new("test-pepper-for-identity-pg-integration".into()),
    ))
}

// ============================================================================
// PgUserRepository
// ============================================================================

#[tokio::test]
async fn pg_user_create_then_find_by_id_roundtrips_all_fields() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());
    let env = fresh_env(&p).await;

    let created = repo
        .create(
            env,
            UserKind::User,
            Some(ext("steam", "76561198000000001")),
            Some("Player One".into()),
        )
        .await
        .expect("create 真实 INSERT users");

    let fetched = repo
        .find_by_id(created.id)
        .await
        .expect("find_by_id 查询")
        .expect("刚建的 user 必须能查到");

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.environment_id, env, "environment_id 必须原样往返");
    assert_eq!(fetched.kind, UserKind::User);
    assert_eq!(fetched.display_name.as_deref(), Some("Player One"));
    let e = fetched
        .external_identity
        .as_ref()
        .expect("external_identity 应持久化并读回");
    assert_eq!(e.provider, "steam");
    assert_eq!(e.external_uid, "76561198000000001");
    assert_eq!(
        fetched.state,
        UserState::Active,
        "新建 user 的默认状态必须是 active"
    );
}

#[tokio::test]
async fn pg_user_find_by_external_identity_scopes_to_environment() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());
    let env = fresh_env(&p).await;

    let created = repo
        .create(env, UserKind::User, Some(ext("epic", "epic-uid-1")), None)
        .await
        .expect("create");

    let found = repo
        .find_by_external_identity(env, "epic", "epic-uid-1")
        .await
        .expect("find_by_external_identity 查询")
        .expect("按 (env, provider, uid) 必须查到");
    assert_eq!(found.id, created.id);

    // 换 environment 必须查不到 —— 证明查询真的带上了 env 条件,
    // 而不是只按 provider+uid 命中(那会跨环境越权)
    let cross = repo
        .find_by_external_identity(fresh_env(&p).await, "epic", "epic-uid-1")
        .await
        .expect("跨环境查询不应报错");
    assert!(
        cross.is_none(),
        "同一 external uid 在另一个 environment 下不得被查到"
    );
}

#[tokio::test]
async fn pg_user_duplicate_external_identity_is_rejected() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());
    let env = fresh_env(&p).await;

    repo.create(env, UserKind::User, Some(ext("steam", "dup-uid-1")), None)
        .await
        .expect("第一次 create 应成功");

    let second = repo
        .create(env, UserKind::User, Some(ext("steam", "dup-uid-1")), None)
        .await;

    assert!(
        second.is_err(),
        "重复 (env, provider, uid) 必须被 DB 唯一约束拒绝, \
         否则会出现两个 user 抢同一外部身份"
    );
    // 确认库里确实只有一条
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE environment_id = $1 AND external_identity->>'external_uid' = 'dup-uid-1'",
    )
    .bind(env.0)
    .fetch_one(&p)
    .await
    .expect("count users");
    assert_eq!(count, 1, "唯一约束应保证库里只有 1 行");
}

#[tokio::test]
async fn pg_user_create_guest_forces_null_external_identity() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());
    let env = fresh_env(&p).await;

    // 即便传了 external, guest 也必须强制 NULL(aux-02 §F.4 + P1-2 红线)
    let created = repo
        .create(
            env,
            UserKind::Guest,
            Some(ext("steam", "should-be-ignored")),
            None,
        )
        .await
        .expect("guest create");

    assert_eq!(created.kind, UserKind::Guest);
    assert!(
        created.external_identity.is_none(),
        "guest 不得绑定外部身份, 即使调用方传了"
    );
    let (kind, ext_identity): (String, Option<serde_json::Value>) =
        sqlx::query_as("SELECT kind::text, external_identity FROM users WHERE id = $1")
            .bind(created.id.0)
            .fetch_one(&p)
            .await
            .expect("重查 users");
    assert_eq!(kind, "guest");
    assert!(
        ext_identity.is_none(),
        "落库的 external_identity 必须为 NULL"
    );
}

#[tokio::test]
async fn pg_user_create_user_without_external_is_rejected() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());

    // kind=user 但无外部身份 → 必须是 Validation 错误, 不能建出半残记录
    let res = repo
        .create(
            fresh_env(&p).await,
            UserKind::User,
            None,
            Some("orphan".into()),
        )
        .await;
    assert!(
        matches!(res, Err(AppError::Validation(_))),
        "kind=user 无 external_identity 应返 Validation, 实际 {res:?}"
    );
}

#[tokio::test]
async fn pg_user_update_state_and_display_name_persist() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());
    let env = fresh_env(&p).await;

    let created = repo
        .create(
            env,
            UserKind::User,
            Some(ext("google", "g-1")),
            Some("Old".into()),
        )
        .await
        .expect("create");

    repo.update_state(created.id, UserState::Banned)
        .await
        .expect("update_state");
    let updated = repo
        .update_display_name(created.id, Some("New Name"))
        .await
        .expect("update_display_name");

    assert_eq!(updated.state, UserState::Banned, "封禁状态必须落库");
    assert_eq!(updated.display_name.as_deref(), Some("New Name"));

    let reread = repo
        .find_by_id(created.id)
        .await
        .expect("重查")
        .expect("user 仍应存在");
    assert_eq!(reread.state, UserState::Banned);
    assert_eq!(reread.display_name.as_deref(), Some("New Name"));
}

#[tokio::test]
async fn pg_user_missing_id_returns_none_not_error() {
    let Some(p) = pool().await else { return };
    let repo = PgUserRepository::new(p.clone());
    let got = repo
        .find_by_id(UserId::new())
        .await
        .expect("查不存在的 id 不该是 Err");
    assert!(
        got.is_none(),
        "不存在的 user 应返回 Ok(None), 以便上层区分 404 与 500"
    );
}

// ============================================================================
// PgDeviceSessionRepository
// ============================================================================

#[tokio::test]
async fn pg_device_session_create_then_find_by_id_roundtrips() {
    let Some(p) = pool().await else { return };
    let user_repo = PgUserRepository::new(p.clone());
    let dev_repo = PgDeviceSessionRepository::new(p.clone());
    let env = fresh_env(&p).await;

    let user = user_repo
        .create(env, UserKind::User, Some(ext("steam", "dev-owner")), None)
        .await
        .expect("create user");

    let session = dev_repo
        .create(user.id, Some("device-fp-001"), "hash-abc")
        .await
        .expect("create 真实 INSERT device_sessions");

    let loaded = dev_repo
        .find_by_id(session.id)
        .await
        .expect("find_by_id 查询")
        .expect("刚建的 device_session 必须能查到");

    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.user_id, user.id, "device_session 必须正确关联 user");
    assert_eq!(loaded.device_fingerprint.as_deref(), Some("device-fp-001"));
    assert!(loaded.revoked_at.is_none(), "新建 session 不应是已撤销状态");
}

#[tokio::test]
async fn pg_device_session_find_by_refresh_hash_requires_matching_user() {
    let Some(p) = pool().await else { return };
    let user_repo = PgUserRepository::new(p.clone());
    let dev_repo = PgDeviceSessionRepository::new(p.clone());
    let env = fresh_env(&p).await;

    let owner = user_repo
        .create(env, UserKind::User, Some(ext("steam", "hash-owner")), None)
        .await
        .expect("create owner");
    let other = user_repo
        .create(env, UserKind::User, Some(ext("steam", "hash-other")), None)
        .await
        .expect("create other");

    dev_repo
        .create(owner.id, None, "secret-hash-1")
        .await
        .expect("create session");

    // 持有者按 hash 查得回
    let mine = dev_repo
        .find_by_refresh_token_hash(owner.id, "secret-hash-1")
        .await
        .expect("owner 查询")
        .expect("owner 应查得到自己的 session");
    assert_eq!(mine.user_id, owner.id);

    // 换另一个 user 用同一 hash 查 —— 必须查不到(否则可拿别人 token 换 session)
    let stolen = dev_repo
        .find_by_refresh_token_hash(other.id, "secret-hash-1")
        .await
        .expect("other 查询");
    assert!(
        stolen.is_none(),
        "refresh hash 查 session 必须同时匹配 user_id, 防止跨用户取用"
    );
}

#[tokio::test]
async fn pg_device_session_revoke_marks_revoked_at() {
    let Some(p) = pool().await else { return };
    let user_repo = PgUserRepository::new(p.clone());
    let dev_repo = PgDeviceSessionRepository::new(p.clone());
    let env = fresh_env(&p).await;

    let user = user_repo
        .create(
            env,
            UserKind::User,
            Some(ext("steam", "revoke-owner")),
            None,
        )
        .await
        .expect("create user");
    let session = dev_repo
        .create(user.id, Some("fp-to-revoke"), "hash-revoke")
        .await
        .expect("create device_session");

    dev_repo.revoke(session.id).await.expect("revoke");

    let after = dev_repo
        .find_by_id(session.id)
        .await
        .expect("revoke 后查询不应报错")
        .expect("记录仍应可查(便于审计)");
    assert!(
        after.revoked_at.is_some(),
        "撤销后 revoked_at 必须非空, 否则被撤销的 refresh token 仍可换新 token"
    );
}

// ============================================================================
// IdentityService 端到端(真 PG)
// ============================================================================

fn make_identity_service(
    p: PgPool,
) -> IdentityService<PgUserRepository, PgDeviceSessionRepository> {
    IdentityService::new(
        PgUserRepository::new(p.clone()),
        PgDeviceSessionRepository::new(p),
        test_token_service(),
        HashMap::new(),
    )
}

#[tokio::test]
async fn identity_server_exchange_persists_user_and_returns_token_pair() {
    let Some(p) = pool().await else { return };
    let svc = make_identity_service(p.clone());
    let env = fresh_env(&p).await;

    let pair = svc
        .server_exchange_token(ServerExchangeCommand {
            environment_id: env,
            external_provider: "steam".into(),
            external_uid: "76561198000009999".into(),
            display_name: Some("E2E Player".into()),
            server_signature_verified: true,
        })
        .await
        .expect("server_exchange_token 在真库上应成功");

    assert!(!pair.access_token.0.is_empty(), "access_token 不应为空");
    assert!(!pair.refresh_token.0.is_empty(), "refresh_token 不应为空");
    assert_eq!(
        pair.refresh_token.0.matches('.').count(),
        1,
        "refresh_token 应为 <session_id>.<raw> 两段, 实际 {}",
        pair.refresh_token.0
    );

    // 用独立 repo 实例验证真的落库
    let verify = PgUserRepository::new(p.clone());
    let found = verify
        .find_by_external_identity(env, "steam", "76561198000009999")
        .await
        .expect("重查")
        .expect("user 必须已持久化");
    assert_eq!(found.display_name.as_deref(), Some("E2E Player"));
    assert_eq!(
        found.kind,
        UserKind::User,
        "server exchange 的 kind 必须是 user"
    );
}

#[tokio::test]
async fn identity_server_exchange_is_idempotent_per_external_identity() {
    let Some(p) = pool().await else { return };
    let svc = make_identity_service(p.clone());
    let env = fresh_env(&p).await;

    svc.server_exchange_token(ServerExchangeCommand {
        environment_id: env,
        external_provider: "steam".into(),
        external_uid: "idem-uid".into(),
        display_name: Some("First".into()),
        server_signature_verified: true,
    })
    .await
    .expect("首次 exchange");

    // 同一外部身份再次登录: 必须复用同一 user(否则每次登录都新建账号)
    svc.server_exchange_token(ServerExchangeCommand {
        environment_id: env,
        external_provider: "steam".into(),
        external_uid: "idem-uid".into(),
        display_name: Some("Second".into()),
        server_signature_verified: true,
    })
    .await
    .expect("二次 exchange");

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE environment_id = $1 AND external_identity->>'external_uid' = 'idem-uid'",
    )
    .bind(env.0)
    .fetch_one(&p)
    .await
    .expect("count users");
    assert_eq!(count, 1, "同一外部身份重复登录不得创建第二个 user");
}

#[tokio::test]
async fn identity_server_exchange_rejects_unverified_signature() {
    let Some(p) = pool().await else { return };
    let svc = make_identity_service(p.clone());
    let env = fresh_env(&p).await;

    let res = svc
        .server_exchange_token(ServerExchangeCommand {
            environment_id: env,
            external_provider: "steam".into(),
            external_uid: "unsigned-attempt".into(),
            display_name: None,
            // 关键安全路径: HMAC 未验证的请求绝不能换到 token
            server_signature_verified: false,
        })
        .await;

    assert!(
        matches!(res, Err(AppError::Unauthorized(_))),
        "未验证签名必须返 Unauthorized, 实际 {res:?}"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE external_identity->>'external_uid' = 'unsigned-attempt'",
    )
    .fetch_one(&p)
    .await
    .expect("count");
    assert_eq!(count, 0, "被拒的 exchange 不得写入任何 user");
}

#[tokio::test]
async fn identity_guest_register_creates_guest_user_in_pg() {
    let Some(p) = pool().await else { return };
    let svc = make_identity_service(p.clone());
    let env = fresh_env(&p).await;

    let pair = svc
        .guest_register(env)
        .await
        .expect("guest_register 在真库上应成功");
    assert!(!pair.access_token.0.is_empty(), "access_token 不应为空");
    assert!(!pair.refresh_token.0.is_empty(), "refresh_token 不应为空");

    // guest 的 kind 必须是 guest 且 external_uid 为 NULL
    let (kind, ext_identity): (String, Option<serde_json::Value>) = sqlx::query_as(
        "SELECT kind::text, external_identity FROM users WHERE environment_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(env.0)
    .fetch_one(&p)
    .await
    .expect("查最近创建的 user");
    assert_eq!(kind, "guest", "匿名注册的用户 kind 必须是 guest");
    assert!(ext_identity.is_none(), "guest 不应绑定外部身份");
}
