//! IdentityService::authenticate 集成测试 — 密码登录路径
//!
//! 依据: 132-wbs.md §5.3.1 C-4 验收
//!       SRS §11 IM-ID-001/002/003/004 + DetailedDesign §3 + C-4 WBS ULYS-145
//!
//! 覆盖:
//!   1. happy path:正确 username + password → TokenPair,access TTL = 900s/15min
//!   2. access token 可被 TokenService::validate_access_token 解码,claims.sub = user_id
//!   3. device session 在 DB 中存在,refresh_token_hash 非空
//!   4. 错密码 → AppError::Unauthorized(同 "invalid username or password" 文本)
//!   5. 用户不存在 → AppError::Unauthorized(同 4 文本,防 enumeration)
//!   6. banned user → AppError::AccountBanned
//!   7. suspended user → AppError::AccountSuspended
//!   8. deleted user → AppError::Unauthorized(同 4 文本,不暴露删除状态)
//!   9. 同 env 下多次 authenticate 颁发不同 device session(IM-ID-002 多设备并发)
//!  10. password 用户 vs external-identity 用户互不干扰(后者 username=NULL,authenticate 不命中)
//!
//! 用法:需要 PG 18.6 在 127.0.0.1:5544(或 DATABASE_URL)
//!   bash scripts/apply_migrations.sh   # 启动 PG + 应用所有迁移
//!   DATABASE_URL=postgres://leo19@127.0.0.1:5544/postgres \
//!     cargo test -p im-core --test identity_authenticate_integration -- --test-threads=1

use chrono::Duration as ChronoDuration;
use secrecy::SecretString;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use uuid::Uuid;

use im_common::ids::EnvironmentId;
use im_core::identity::password::hash_password;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::repository::{UserKind, UserRepository, UserState};
use im_core::identity::service::IdentityService;
use im_core::identity::token::{SigningKey, TokenService};

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://leo19@127.0.0.1:5544/postgres".into())
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(50)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .connect(&database_url())
        .await
        .expect("failed to connect to PG 18.6 (run scripts/apply_migrations.sh first)")
}

/// 建独立 env,返回 (env_id, alice_user_id, bob_user_id)
async fn make_env() -> (EnvironmentId, Uuid, Uuid) {
    let p = pool().await;
    let env_id: Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'c4-tenant-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'c4-game-' || gen_random_uuid()::text FROM t
            RETURNING id
        )
        INSERT INTO environments (id, game_id, name)
        SELECT gen_random_uuid(), g.id, 'test' FROM g
        RETURNING id
        "#,
    )
    .fetch_one(&p)
    .await
    .expect("make_env failed");

    let alice: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind, display_name)
           VALUES (gen_random_uuid(), $1, 'user', 'Alice') RETURNING id"#,
    )
    .bind(env_id)
    .fetch_one(&p)
    .await
    .expect("create Alice failed");

    let bob: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind, display_name)
           VALUES (gen_random_uuid(), $1, 'user', 'Bob') RETURNING id"#,
    )
    .bind(env_id)
    .fetch_one(&p)
    .await
    .expect("create Bob failed");

    (EnvironmentId(env_id), alice, bob)
}

/// 构建 IdentityService + TokenService(15min access TTL,单 key)
async fn build_service() -> (
    IdentityService<PgUserRepository, PgDeviceSessionRepository>,
    Arc<TokenService>,
) {
    let key = SigningKey {
        kid: "c4-v1".into(),
        key: SecretString::new(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        ),
    };
    let token_service = Arc::new(TokenService::new(
        vec![key],
        // 15 分钟 — SRS §11 IM-ID-003 + DetailedDesign §6.4 规定
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper-not-used-for-hs256".into()),
    ));
    let user_repo = PgUserRepository::new(pool().await); // pool 在调用方 await 后已就绪
    let device_repo = PgDeviceSessionRepository::new(pool().await);
    // IdentityService::new takes user_repo by value;move 进去
    let svc = IdentityService::new(user_repo, device_repo, token_service.clone(), Default::default());
    (svc, token_service)
}

/// 注册一个密码登录用户,返回 user_id(供测试用 — 模拟 C-3 的 register 后续实装)
async fn seed_password_user(env_id: EnvironmentId, username: &str, password: &str) -> Uuid {
    let user_repo = PgUserRepository::new(pool().await);
    let pwd_hash = hash_password(password).expect("hash_password");
    let user = user_repo
        .register_with_password(env_id, username, &pwd_hash, Some(username.into()))
        .await
        .expect("register_with_password failed");
    user.id.0
}

// =============================================================================
// Happy path
// =============================================================================

#[tokio::test(flavor = "current_thread")]
async fn authenticate_happy_path_issues_15min_token_pair() {
    let (env_id, _, _) = make_env().await;
    let username = "alice_login";
    let password = "CorrectHorseBatteryStaple!1";
    let user_id = seed_password_user(env_id, username, password).await;

    let (svc, _) = build_service().await;
    let pair = svc
        .authenticate(env_id, username, password)
        .await
        .expect("authenticate should succeed for correct password");

    // 1. TokenPair.expires_in = 900(15min)
    assert_eq!(pair.expires_in, 900, "Access TTL must be 15min (900s)");
    assert_eq!(pair.user_id.0, user_id);

    // 2. access_token 非空 + 可解码
    assert!(!pair.access_token.0.is_empty());
    let (_, token_svc) = build_service().await;
    let claims = token_svc
        .validate_access_token(&pair.access_token.0)
        .expect("validate_access_token must succeed for freshly-issued token");
    assert_eq!(claims.sub, user_id.to_string());
    assert_eq!(claims.env, env_id.to_string());
    assert_eq!(claims.kind, "user");

    // 3. claims.exp - claims.iat == 900
    let ttl = claims.exp - claims.iat;
    assert_eq!(ttl, 900, "JWT exp - iat must equal access_ttl (900s)");

    // 4. refresh_token 格式: "{session_id}.{raw_uuid}"
    let refresh = &pair.refresh_token.0;
    let (sid_str, _raw) = refresh
        .split_once('.')
        .expect("refresh_token must have session_id.raw format");
    let sid: Uuid = sid_str
        .parse()
        .expect("session_id must be a valid UUID");
    assert_eq!(sid.to_string(), sid_str);

    // 5. device_sessions 表存在该 row 且未 revoke
    let p = pool().await;
    let revoked_opt: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar(
        "SELECT revoked_at FROM device_sessions WHERE id = $1",
    )
    .bind(sid)
    .fetch_optional(&p)
    .await
    .expect("device_session lookup");
    let revoked_at = revoked_opt.expect("device_session row must exist");
    assert!(
        revoked_at.is_none(),
        "fresh session must not have revoked_at set, got: {:?}",
        revoked_at
    );
}

// =============================================================================
// 错误路径
// =============================================================================

#[tokio::test(flavor = "current_thread")]
async fn authenticate_wrong_password_rejected_with_enumeration_safe_error() {
    let (env_id, _, _) = make_env().await;
    let username = "bob_login";
    seed_password_user(env_id, username, "right_password").await;

    let (svc, _) = build_service().await;
    let err = svc
        .authenticate(env_id, username, "wrong_password")
        .await
        .expect_err("authenticate must reject wrong password");
    assert!(
        matches!(err, im_common::AppError::Unauthorized(_)),
        "wrong password → Unauthorized, got: {:?}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn authenticate_unknown_username_returns_same_error_as_wrong_password() {
    let (env_id, _, _) = make_env().await;
    let (svc, _) = build_service().await;

    // 不存在的 username
    let err1 = svc
        .authenticate(env_id, "no_such_user", "any_password")
        .await
        .expect_err("unknown username must fail");
    let msg1 = match &err1 {
        im_common::AppError::Unauthorized(m) => m.clone(),
        _ => panic!("unknown username → Unauthorized, got: {:?}", err1),
    };

    // 存在但密码错
    let username = "carol_login";
    seed_password_user(env_id, username, "right").await;
    let err2 = svc
        .authenticate(env_id, username, "wrong")
        .await
        .expect_err("wrong password must fail");
    let msg2 = match &err2 {
        im_common::AppError::Unauthorized(m) => m.clone(),
        _ => panic!("wrong password → Unauthorized, got: {:?}", err2),
    };

    // 关键:两条错误信息**完全相同** —— 防 username enumeration
    assert_eq!(
        msg1, msg2,
        "username-enumeration defense: error message must be identical for unknown user vs wrong password"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn authenticate_banned_user_rejected_with_account_banned() {
    let (env_id, _, _) = make_env().await;
    let username = "dave_login";
    let user_id = seed_password_user(env_id, username, "pw").await;

    // 设为 banned
    let p = pool().await;
    sqlx::query("UPDATE users SET state = 'banned' WHERE id = $1")
        .bind(user_id)
        .execute(&p)
        .await
        .expect("ban failed");

    let (svc, _) = build_service().await;
    let err = svc
        .authenticate(env_id, username, "pw")
        .await
        .expect_err("banned user must fail");
    assert!(
        matches!(err, im_common::AppError::AccountBanned),
        "banned → AccountBanned, got: {:?}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn authenticate_suspended_user_rejected_with_account_suspended() {
    let (env_id, _, _) = make_env().await;
    let username = "erin_login";
    let user_id = seed_password_user(env_id, username, "pw").await;

    let p = pool().await;
    sqlx::query("UPDATE users SET state = 'suspended' WHERE id = $1")
        .bind(user_id)
        .execute(&p)
        .await
        .expect("suspend failed");

    let (svc, _) = build_service().await;
    let err = svc
        .authenticate(env_id, username, "pw")
        .await
        .expect_err("suspended user must fail");
    assert!(
        matches!(err, im_common::AppError::AccountSuspended),
        "suspended → AccountSuspended, got: {:?}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn authenticate_deleted_user_returns_generic_unauthorized() {
    let (env_id, _, _) = make_env().await;
    let username = "frank_login";
    let user_id = seed_password_user(env_id, username, "pw").await;

    let p = pool().await;
    sqlx::query("UPDATE users SET state = 'deleted' WHERE id = $1")
        .bind(user_id)
        .execute(&p)
        .await
        .expect("delete failed");

    let (svc, _) = build_service().await;
    let err = svc
        .authenticate(env_id, username, "pw")
        .await
        .expect_err("deleted user must fail");
    // 不暴露 "deleted" 状态给客户端(防 enumeration + 防破坏 audit 隐式语义)
    assert!(
        matches!(err, im_common::AppError::Unauthorized(_)),
        "deleted → Unauthorized (generic), got: {:?}",
        err
    );
}

// =============================================================================
// SRS IM-ID-002 多设备并发
// =============================================================================

#[tokio::test(flavor = "current_thread")]
async fn authenticate_twice_creates_two_distinct_device_sessions() {
    let (env_id, _, _) = make_env().await;
    let username = "grace_login";
    seed_password_user(env_id, username, "pw").await;

    let (svc, _) = build_service().await;
    let pair1 = svc.authenticate(env_id, username, "pw").await.unwrap();
    let pair2 = svc.authenticate(env_id, username, "pw").await.unwrap();

    // access token 必然不同(JWT 携带 iat 不同,且 kid 虽同但 iat 区分)
    assert_ne!(pair1.access_token.0, pair2.access_token.0);
    // refresh token 不同(session_id 不同)
    assert_ne!(pair1.refresh_token.0, pair2.refresh_token.0);

    // 两个 device session 都在 DB 中
    let p = pool().await;
    let n: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM device_sessions ds
           JOIN users u ON ds.user_id = u.id
           WHERE u.username = $1 AND u.environment_id = $2"#,
    )
    .bind(username)
    .bind(env_id.0)
    .fetch_one(&p)
    .await
    .expect("count sessions");
    assert_eq!(n, 2, "two logins → two active device sessions");
}

// =============================================================================
// External-identity 用户不在 authenticate 命中范围
// =============================================================================

#[tokio::test(flavor = "current_thread")]
async fn authenticate_does_not_hit_external_identity_user_without_username() {
    let (env_id, _, _) = make_env().await;
    let p = pool().await;

    // 建一个 kind='user' + external_identity 必填的用户(username=NULL)
    let _ext_user_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind, external_identity, display_name)
           VALUES (gen_random_uuid(), $1, 'user', $2::jsonb, 'SteamPlayer') RETURNING id"#,
    )
    .bind(env_id.0)
    .bind(r#"{"provider":"steam","external_uid":"76561198000000099"}"#)
    .fetch_one(&p)
    .await
    .expect("create ext user");

    let (svc, _) = build_service().await;
    // 用不存在的 username 试 → generic Unauthorized(不会撞到 ext user)
    let err = svc
        .authenticate(env_id, "SteamPlayer", "any")
        .await
        .expect_err("must reject");
    assert!(matches!(err, im_common::AppError::Unauthorized(_)));
}

// =============================================================================
// TokenService.access_ttl getter sanity
// =============================================================================

#[test]
fn token_service_access_ttl_reports_900s() {
    let key = SigningKey {
        kid: "v1".into(),
        key: SecretString::new("k".repeat(64).into()),
    };
    let svc = TokenService::new(
        vec![key],
        ChronoDuration::seconds(900),
        SecretString::new("pepper".into()),
    );
    assert_eq!(svc.access_ttl_seconds(), 900);
    assert_eq!(svc.access_ttl(), ChronoDuration::seconds(900));
}

// 静默 UserKind/UserState 引用,防止 dead_code lint
#[allow(dead_code)]
fn _mark_used() {
    let _ = UserKind::User;
    let _ = UserState::Active;
}