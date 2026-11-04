//! Integration tests: IdentityService::register (2026-09-20 C-3 WBS)
//!
//! 依据: SRS §11 IM-ID-001~004 + DetailedDesign §9.2 + docs/132-wbs.md §5.3 C-3
//!       ULYS-144 验收要求: 集成测试覆盖 happy path + dup username + weak password
//!
//! 环境: 需要 PG 18.6 实例(用 WSL Ubuntu 本地 init 集群,port 5544,trust auth)
//!       与 pg_repos_integration.rs 共享同一 DB schema;**必须先 apply migrations 0001~0007**
//! 用法:
//!   # 一次性迁移 (apply 0001..0007)
//!   sqlx migrate run --source migrations --database-url "$DATABASE_URL"
//!
//!   # 跑测试
//!   export DATABASE_URL=postgres://leo19@127.0.0.1:5544/postgres
//!   cargo test -p im-core --test identity_register_integration -- --test-threads=1
//!
//! --test-threads=1 是因为所有测试共享同一个 DB,需要串行执行避免主键冲突
//!
//! 关于 RS256 测试密钥:
//!   本测试在运行时自包含生成 RSA-2048 密钥对(避免把私钥入仓);
//!   每次测试启动生成新密钥,确保测试隔离。生产密钥由 im_common::config 注入。

use sqlx::PgPool;
use std::env;
use std::sync::Arc;

use chrono::Duration as ChronoDuration;
use jsonwebtoken::Algorithm;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::pkcs8::EncodePrivateKey;
use rsa::RsaPrivateKey;
use secrecy::SecretString;

use im_common::ids::EnvironmentId;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::repository::UserRepository;
use im_core::identity::service::{IdentityService, RegisterCommand};
use im_core::identity::token::{SigningKey, TokenService};

// ============================================================================
// 共享 Fixtures
// ============================================================================

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| {
        // 默认值:WSL Ubuntu 本地 PG 18.6 (per scripts/init-pg18-b1.sh)
        "postgres://leo19@172.28.176.169:5544/postgres".to_string()
    })
}

async fn pool() -> PgPool {
    let url = database_url();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .expect("failed to connect to PG 18.6 (run scripts/init-pg18-b1.sh first)")
}

/// 创建独立测试 fixture: 每个测试用 1 个 tenant + 1 个 game + 1 个 env
async fn make_env() -> EnvironmentId {
    let p = pool().await;
    let env_id: uuid::Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'test-tenant-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'test-game-' || gen_random_uuid()::text FROM t
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
    EnvironmentId(env_id)
}

/// 生成 RS256 密钥对(用于 IdentityService 测试)
/// 密钥 2048-bit,生成耗时 < 1s
fn generate_rs256_keypair() -> (String, String) {
    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("RSA keygen failed");
    let public_key = private_key.to_public_key();

    // PKCS#8 PEM 编码私钥(jsonwebtoken::EncodingKey::from_rsa_pem 接受)
    let private_pem = private_key
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .expect("encode private PEM")
        .to_string();

    // PKCS#1 PEM 编码公钥(jsonwebtoken::DecodingKey::from_rsa_pem 接受 PKCS#1)
    let public_pem = public_key
        .to_pkcs1_pem(rsa::pkcs8::LineEnding::LF)
        .expect("encode public PEM")
        .to_string();

    (private_pem, public_pem)
}

fn make_rs256_token_service() -> Arc<TokenService> {
    let (priv_pem, pub_pem) = generate_rs256_keypair();
    let key = SigningKey::rs256(
        "v1-rs256-test",
        SecretString::new(priv_pem),
        SecretString::new(pub_pem),
    );
    Arc::new(TokenService::new(
        vec![key],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ))
}

async fn make_identity_service() -> IdentityService<PgUserRepository, PgDeviceSessionRepository> {
    IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        make_rs256_token_service(),
        Default::default(), // server_secrets(register 不需要)
    )
}

// ============================================================================
// 集成测试 cases(ULYS-144 验收要求)
// ============================================================================

/// C-3 happy path: register 成功 → 返回 TokenPair (access RS256 + refresh + device session)
#[tokio::test]
async fn register_happy_path_returns_token_pair() {
    let env_id = make_env().await;
    let svc = make_identity_service().await;

    let pair = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "alice".into(),
            password: "hunter2pw".into(),
            display_name: Some("Alice".into()),
        })
        .await
        .expect("register should succeed");

    // 1. 拿到 access + refresh + user_id
    assert!(!pair.access_token.0.is_empty(), "access_token should be non-empty");
    assert!(!pair.refresh_token.0.is_empty(), "refresh_token should be non-empty");
    assert_eq!(pair.expires_in, 900, "expires_in matches access_ttl");

    // 2. access token 是 RS256 算法签发
    let header = jsonwebtoken::decode_header(&pair.access_token.0).expect("decode header");
    assert_eq!(header.alg, Algorithm::RS256, "register must use RS256 (per C-3 验收)");
    assert_eq!(header.kid.as_deref(), Some("v1-rs256-test"));

    // 3. token claims 正确
    let claims = svc
        .token_service()
        .validate_access_token(&pair.access_token.0)
        .expect("validate_access_token");
    assert_eq!(claims.sub, pair.user_id.to_string());
    assert_eq!(claims.env, env_id.to_string());
    assert_eq!(claims.kind, "user");

    // 4. DeviceSession 已创建
    let p = pool().await;
    let row: Option<(uuid::Uuid, Option<chrono::DateTime<chrono::Utc>>, String)> =
        sqlx::query_as(
            "SELECT id, revoked_at, refresh_token_hash FROM device_sessions WHERE user_id = $1",
        )
        .bind(pair.user_id.0)
        .fetch_optional(&p)
        .await
        .expect("query device_sessions");
    let s = row.expect("DeviceSession row exists");
    assert!(s.1.is_none(), "session is active");
    assert!(!s.2.is_empty(), "refresh hash stored");

    // 5. user.password_hash 已写入(应看起来像 argon2 PHC 格式,以 $argon2 开头)
    let user_repo = PgUserRepository::new(p);
    let user = user_repo
        .find_by_id(pair.user_id)
        .await
        .expect("find_by_id")
        .expect("user exists");
    assert_eq!(user.username.as_deref(), Some("alice"));
    assert!(user.password_hash.is_some(), "password_hash stored");
    let phc = user.password_hash.as_deref().unwrap();
    assert!(
        phc.starts_with("$argon2"),
        "password_hash should be argon2 PHC format, got: {}",
        phc
    );
}

/// C-3 dup username: 同 env 下 register 两次同名 → 第二次失败 AccountAlreadyExists
#[tokio::test]
async fn register_duplicate_username_rejected() {
    let env_id = make_env().await;
    let svc = make_identity_service().await;

    // 第一次成功
    svc.register(RegisterCommand {
        environment_id: env_id,
        username: "bob".into(),
        password: "hunter2pw".into(),
        display_name: None,
    })
    .await
    .expect("first register should succeed");

    // 第二次同名 → AccountAlreadyExists
    let r = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "bob".into(),
            password: "different1pw".into(),
            display_name: None,
        })
        .await;
    assert!(
        matches!(r, Err(im_common::AppError::AccountAlreadyExists)),
        "duplicate username must return AccountAlreadyExists, got: {:?}",
        r
    );

    // 验证 DB 里只有一个 user(第二次未创建)
    let p = pool().await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE environment_id = $1 AND username = $2",
    )
    .bind(env_id.0)
    .bind("bob")
    .fetch_one(&p)
    .await
    .expect("count users");
    assert_eq!(count, 1, "only one user should exist after dup attempt");
}

/// C-3 weak password: 弱密码(< 8 字符 / 缺数字 / 缺字母) → Validation error,不写 DB
#[tokio::test]
async fn register_weak_password_rejected() {
    let env_id = make_env().await;
    let svc = make_identity_service().await;

    // 短 (< 8)
    let r1 = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "carol_short".into(),
            password: "a1b".into(),
            display_name: None,
        })
        .await;
    assert!(
        matches!(r1, Err(im_common::AppError::Validation(_))),
        "short password must return Validation, got: {:?}",
        r1
    );

    // 全数字
    let r2 = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "carol_digit".into(),
            password: "12345678".into(),
            display_name: None,
        })
        .await;
    assert!(
        matches!(r2, Err(im_common::AppError::Validation(_))),
        "digit-only password must return Validation, got: {:?}",
        r2
    );

    // 全字母
    let r3 = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "carol_letter".into(),
            password: "abcdefgh".into(),
            display_name: None,
        })
        .await;
    assert!(
        matches!(r3, Err(im_common::AppError::Validation(_))),
        "letter-only password must return Validation, got: {:?}",
        r3
    );

    // 验证 DB 里没有写任何 user
    let p = pool().await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE environment_id = $1 AND username LIKE 'carol_%'",
    )
    .bind(env_id.0)
    .fetch_one(&p)
    .await
    .expect("count users");
    assert_eq!(count, 0, "weak-password register attempts must not write any user");
}

/// C-3 边界补充: 非法 username(短 / 特殊字符)→ Validation error
#[tokio::test]
async fn register_invalid_username_rejected() {
    let env_id = make_env().await;
    let svc = make_identity_service().await;

    // 太短 (< 3)
    let r1 = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "ab".into(),
            password: "hunter2pw".into(),
            display_name: None,
        })
        .await;
    assert!(matches!(r1, Err(im_common::AppError::Validation(_))));

    // 含特殊字符 (.)
    let r2 = svc
        .register(RegisterCommand {
            environment_id: env_id,
            username: "dave.foo".into(),
            password: "hunter2pw".into(),
            display_name: None,
        })
        .await;
    assert!(
        matches!(r2, Err(im_common::AppError::Validation(_))),
        "username with '.' must return Validation, got: {:?}",
        r2
    );

    // 验证 DB 里没有写任何 user
    let p = pool().await;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE environment_id = $1 AND username IN ('ab', 'dave.foo')",
    )
    .bind(env_id.0)
    .fetch_one(&p)
    .await
    .expect("count users");
    assert_eq!(count, 0);
}
