//! ConversationService 集成测试 — C-8 WBS ULYS-148
//!
//! 覆盖:
//! - DM 幂等 + dup dm 并发(50 个并发 create_dm → 同一 conv_id + 1 条 dm_pairs)
//! - DM 自创建拒绝
//! - Group/Channel 最小成员 + 上限 + Owner 角色
//! - DM 禁止 add_member
//! - Group last-owner 不能 leave(leave race 保护)
//! - add_member / remove_member 权限检查 + 幂等
//!
//! 用法:
//!   export DATABASE_URL=postgres://leo19@127.0.0.1:5544/postgres
//!   cargo test -p im-core --test conversation_service_test -- --test-threads=1

use serde_json::json;
use sqlx::PgPool;
use std::env;
use std::sync::Arc;
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::repository::ConversationKind;
use im_core::conversation::service::ConversationService;

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://leo19@172.28.176.169:5544/postgres".to_string()
    })
}

async fn pool() -> PgPool {
    let url = database_url();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .expect("failed to connect to PG 18.6")
}

async fn make_env() -> (EnvironmentId, UserId, UserId) {
    let p = pool().await;
    let p_ref = &p;
    let env_id: Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'svc-tenant-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'svc-game-' || gen_random_uuid()::text FROM t
            RETURNING id
        )
        INSERT INTO environments (id, game_id, name)
        SELECT gen_random_uuid(), g.id, 'test' FROM g
        RETURNING id
        "#,
    )
    .fetch_one(p_ref)
    .await
    .expect("make_env");

    let alice: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind, display_name) VALUES (gen_random_uuid(), $1, 'user', 'Alice') RETURNING id"#,
    )
    .bind(env_id)
    .fetch_one(p_ref)
    .await
    .expect("create Alice");

    let bob: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind, display_name) VALUES (gen_random_uuid(), $1, 'user', 'Bob') RETURNING id"#,
    )
    .bind(env_id)
    .fetch_one(p_ref)
    .await
    .expect("create Bob");

    (EnvironmentId(env_id), UserId(alice), UserId(bob))
}

// 包装:跑 async 测试的 helper(每个 test 内部建自己的 service + pool)
macro_rules! svc {
    () => {{
        let p = pool().await;
        let repo = Arc::new(PgConversationRepository::new(p));
        ConversationService::new(repo)
    }};
}

// ============================================================================
// DM
// ============================================================================

#[tokio::test]
async fn create_dm_basic() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();

    let conv = s.create_dm(env_id, alice, bob).await.expect("create_dm");
    assert_eq!(conv.kind, ConversationKind::Dm);

    // 两人都应是 member
    assert!(s.is_member(conv.id, alice).await.unwrap());
    assert!(s.is_member(conv.id, bob).await.unwrap());

    // 再次创建应返回同一 conv(幂等)
    let conv2 = s.create_dm(env_id, alice, bob).await.expect("create_dm idempotent");
    assert_eq!(conv.id, conv2.id);

    // 反向参数顺序也命中同一 dm
    let conv3 = s.create_dm(env_id, bob, alice).await.expect("create_dm reverse");
    assert_eq!(conv.id, conv3.id);

    // dm_pairs 行数 = 1
    let p = pool().await;
    let count: (i64,) = sqlx::query_as("SELECT count(*)::bigint FROM dm_pairs WHERE environment_id = $1")
        .bind(env_id.0)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn create_dm_self_rejected() {
    let (env_id, alice, _) = make_env().await;
    let s = svc!();

    let r = s.create_dm(env_id, alice, alice).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn create_dm_concurrent_dedup() {
    // 50 个并发 create_dm(a, b) → 全部返回同一 conv_id,dm_pairs 只有 1 行
    let (env_id, alice, bob) = make_env().await;

    let mut handles = Vec::new();
    for _ in 0..50 {
        handles.push(tokio::spawn(async move {
            let s = svc!();
            s.create_dm(env_id, alice, bob).await
        }));
    }
    let mut ids = std::collections::HashSet::new();
    for h in handles {
        let r = h.await.expect("join").expect("create_dm");
        ids.insert(r.id);
    }
    assert_eq!(ids.len(), 1, "all concurrent create_dm must return same conv_id");

    let p = pool().await;
    let count: (i64,) = sqlx::query_as("SELECT count(*)::bigint FROM dm_pairs WHERE environment_id = $1")
        .bind(env_id.0)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "dm_pairs must have exactly 1 row");

    // 不应该留下空的 conv(被 rollback 的)
    let conv_count: (i64,) = sqlx::query_as("SELECT count(*)::bigint FROM conversations WHERE environment_id = $1")
        .bind(env_id.0)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(conv_count.0, 1, "no orphan conversations");
}

// ============================================================================
// Group
// ============================================================================

#[tokio::test]
async fn create_group_min_size_rejected() {
    let (env_id, alice, _) = make_env().await;
    let s = svc!();
    let r = s.create_group(env_id, alice, vec![alice], json!({})).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn create_group_basic_and_role() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();

    let conv = s
        .create_group(env_id, alice, vec![alice, bob], json!({"topic": "test"}))
        .await
        .expect("create_group");
    assert_eq!(conv.kind, ConversationKind::Group);

    let p = pool().await;
    // creator 是 owner,bob 是 member
    let alice_role: (String,) = sqlx::query_as(
        "SELECT role FROM conversation_members WHERE conversation_id = $1 AND user_id = $2",
    )
    .bind(conv.id.0)
    .bind(alice.0)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(alice_role.0, "owner");
    let bob_role: (String,) = sqlx::query_as(
        "SELECT role FROM conversation_members WHERE conversation_id = $1 AND user_id = $2",
    )
    .bind(conv.id.0)
    .bind(bob.0)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(bob_role.0, "member");
}

#[tokio::test]
async fn create_group_too_large_rejected() {
    let (env_id, alice, _) = make_env().await;
    let s = svc!();

    let big: Vec<UserId> = (0..501).map(|_| UserId::new()).collect();
    let r = s.create_group(env_id, alice, big, json!({})).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

// ============================================================================
// Channel / System / Broadcast
// ============================================================================

#[tokio::test]
async fn create_channel_basic() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    let conv = s
        .create_channel(env_id, alice, vec![alice, bob], json!({"topic": "announce"}))
        .await
        .expect("create_channel");
    assert_eq!(conv.kind, ConversationKind::Channel);
    assert!(s.is_member(conv.id, alice).await.unwrap());
    assert!(s.is_member(conv.id, bob).await.unwrap());
}

#[tokio::test]
async fn create_system_no_members() {
    let (env_id, _alice, _bob) = make_env().await;
    let s = svc!();
    let conv = s
        .create_system(env_id, json!({"event": "x"}))
        .await
        .expect("create_system");
    assert_eq!(conv.kind, ConversationKind::System);
}

#[tokio::test]
async fn create_broadcast_no_members() {
    let (env_id, _alice, _bob) = make_env().await;
    let s = svc!();
    let conv = s
        .create_broadcast(env_id, json!({"event": "y"}))
        .await
        .expect("create_broadcast");
    assert_eq!(conv.kind, ConversationKind::Broadcast);
}

// ============================================================================
// add_member / remove_member / leave
// ============================================================================

#[tokio::test]
async fn dm_add_member_rejected() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    let conv = s.create_dm(env_id, alice, bob).await.unwrap();

    let r = s.add_member(conv.id, UserId::new()).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn group_add_member_idempotent() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    let conv = s
        .create_group(env_id, alice, vec![alice, bob], json!({}))
        .await
        .unwrap();

    // bob 已存在 → add_member 幂等成功
    s.add_member(conv.id, bob).await.expect("add bob again");
    // 加第 3 个新成员
    let carol = UserId::new();
    let p = pool().await;
    sqlx::query("INSERT INTO users (id, environment_id, kind) VALUES ($1, $2, 'user')")
        .bind(carol.0)
        .bind(env_id.0)
        .execute(&p)
        .await
        .unwrap();
    s.add_member(conv.id, carol).await.expect("add carol");
    assert!(s.is_member(conv.id, carol).await.unwrap());
}

#[tokio::test]
async fn group_remove_member_requires_owner() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    let conv = s
        .create_group(env_id, alice, vec![alice, bob], json!({}))
        .await
        .unwrap();

    // bob (member) 不能 kick alice
    let r = s.remove_member(conv.id, bob, alice).await;
    assert!(matches!(r, Err(im_common::AppError::Forbidden(_))));
}

#[tokio::test]
async fn group_remove_owner_rejected() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    // alice 是 owner,bob 是 member;alice 单独试图被 kick(actor=alice)→ 拒绝
    let conv = s
        .create_group(env_id, alice, vec![alice, bob], json!({}))
        .await
        .unwrap();

    // owner 不能 kick 自己(必须 transfer)
    let r = s.remove_member(conv.id, alice, alice).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn group_leave_race_last_owner_protected() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    // alice = owner, bob = member
    let conv = s
        .create_group(env_id, alice, vec![alice, bob], json!({}))
        .await
        .unwrap();

    // bob 先 leave(成员可以)
    s.leave(conv.id, bob).await.expect("bob leave");

    // 此时 alice 是唯一 owner → leave 应被拒
    let r = s.leave(conv.id, alice).await;
    assert!(
        matches!(r, Err(im_common::AppError::Validation(_))),
        "last owner must not be allowed to leave"
    );

    // alice 仍在成员列表
    assert!(s.is_member(conv.id, alice).await.unwrap());
}

#[tokio::test]
async fn group_leave_works_for_member_and_owner_with_transfer() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    let conv = s
        .create_group(env_id, alice, vec![alice, bob], json!({}))
        .await
        .unwrap();

    // bob (member) 直接 leave 成功
    let n = s.leave(conv.id, bob).await.expect("bob leave");
    assert_eq!(n, 1);
    assert!(!s.is_member(conv.id, bob).await.unwrap());

    // alice (唯一 owner) leave → 仍被拒
    let r = s.leave(conv.id, alice).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn dm_leave_succeeds() {
    let (env_id, alice, bob) = make_env().await;
    let s = svc!();
    let conv = s.create_dm(env_id, alice, bob).await.unwrap();

    // DM leave 不受 owner 检查约束
    let n = s.leave(conv.id, alice).await.expect("alice leave DM");
    assert_eq!(n, 1);
    assert!(!s.is_member(conv.id, alice).await.unwrap());
}
