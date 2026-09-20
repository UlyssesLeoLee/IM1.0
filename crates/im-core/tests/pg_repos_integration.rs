//! 集成测试:6 个 PgRepository + 1 PgSequenceAllocator,跑在真 PG 18.6 上
//!
//! 依据: WBS C-1(132-wbs.md §5.3.1)验收要求
//!
//! 环境:需要 PG 18.6 实例(用 WSL Ubuntu 本地 init 集群,port 5544,trust auth)
//! 用法:
//!   export DATABASE_URL=postgres://leo19@127.0.0.1:5544/postgres
//!   cargo test -p im-core --test pg_repos_integration -- --test-threads=1
//!
//! --test-threads=1 是因为所有测试共享同一个 DB,需要串行执行避免主键冲突

use serde_json::json;
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

use im_common::ids::{EnvironmentId, MessageId, UserId};
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::repository::{ConversationKind, ConversationRepository, MemberRole};
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::repository::{ExternalIdentity, User, UserKind, UserRepository, UserState};
use im_core::identity::token::DeviceSessionRepository;
use im_core::identity::service::IdentityService;
use im_core::identity::token::TokenService;
use im_core::message::pg::{PgMessageRepository, PgSequenceAllocator};
use im_core::message::repository::{MessageRepository, MessageState, NewMessage};
use im_core::message::sequence::SequenceAllocator;
use im_core::reaction::pg::PgReactionRepository;
use im_core::reaction::repository::ReactionRepository;
use im_core::relationship::pg::PgFriendshipRepository;
use im_core::relationship::repository::{FriendRequestState, FriendshipRepository};

// ============================================================================
// 共享测试 Fixtures
// ============================================================================

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| {
        // 默认值:WSL Ubuntu 本地 PG 18.6 (per scripts/init-pg18-b1.sh)
        "postgres://leo19@172.28.176.169:5544/postgres".to_string()
    })
}

/// 每个 test 自己拿新 pool(MVP 测试成本可接受,避开 OnceCell + 多次 tokio runtime 的连接持有问题)
async fn pool() -> PgPool {
    let url = database_url();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .expect("failed to connect to PG 18.6 (run scripts/init-pg18-b1.sh first)")
}

/// 创建独立测试 fixture:每个测试用 1 个 tenant + 1 个 game + 1 个 env
async fn make_env() -> (EnvironmentId, UserId, UserId) {
    let p = pool().await;
    let p_ref = &p;
    let env_id: Uuid = sqlx::query_scalar(
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
    .fetch_one(p_ref)
    .await
    .expect("make_env failed");

    let user_a: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users (id, environment_id, kind, display_name)
        VALUES (gen_random_uuid(), $1, 'user', 'Alice')
        RETURNING id
        "#,
    )
    .bind(env_id)
    .fetch_one(p_ref)
    .await
    .expect("create Alice failed");

    let user_b: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO users (id, environment_id, kind, display_name)
        VALUES (gen_random_uuid(), $1, 'user', 'Bob')
        RETURNING id
        "#,
    )
    .bind(env_id)
    .fetch_one(p_ref)
    .await
    .expect("create Bob failed");

    (EnvironmentId(env_id), UserId(user_a), UserId(user_b))
}

// ============================================================================
// PgUserRepository 测试
// ============================================================================

#[tokio::test]
async fn user_create_guest_and_find_by_id() {
    let (env_id, _, _) = make_env().await;
    let repo = PgUserRepository::new(pool().await);

    let guest = repo
        .create(env_id, UserKind::Guest, None, Some("Guest1".into()))
        .await
        .expect("create guest failed");
    assert_eq!(guest.kind, UserKind::Guest);
    assert!(guest.external_identity.is_none());

    let found = repo
        .find_by_id(guest.id)
        .await
        .expect("find_by_id failed")
        .expect("guest not found");
    assert_eq!(found.id, guest.id);
    assert_eq!(found.kind, UserKind::Guest);
}

#[tokio::test]
async fn user_create_with_extid_and_find_by_extid() {
    let (env_id, _, _) = make_env().await;
    let repo = PgUserRepository::new(pool().await);

    let user = repo
        .create(
            env_id,
            UserKind::User,
            Some(ExternalIdentity {
                provider: "steam".into(),
                external_uid: "76561198000000001".into(),
            }),
            Some("SteamPlayer".into()),
        )
        .await
        .expect("create user failed");

    let found = repo
        .find_by_external_identity(env_id, "steam", "76561198000000001")
        .await
        .expect("find_by_extid failed")
        .expect("user not found");
    assert_eq!(found.id, user.id);
    assert_eq!(found.kind, UserKind::User);
    assert!(found.external_identity.is_some());
}

#[tokio::test]
async fn user_create_without_extid_for_user_kind_rejected() {
    let (env_id, _, _) = make_env().await;
    let repo = PgUserRepository::new(pool().await);

    let r = repo
        .create(env_id, UserKind::User, None, None)
        .await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn user_update_state_banned() {
    let (env_id, _, _) = make_env().await;
    let repo = PgUserRepository::new(pool().await);
    let user = repo
        .create(env_id, UserKind::User, Some(ExternalIdentity { provider: "x".into(), external_uid: "y".into() }), None)
        .await
        .unwrap();
    repo.update_state(user.id, UserState::Banned)
        .await
        .expect("ban failed");
    let reloaded = repo.find_by_id(user.id).await.unwrap().unwrap();
    assert_eq!(reloaded.state, UserState::Banned);
}

// ============================================================================
// PgDeviceSessionRepository 测试
// ============================================================================

#[tokio::test]
async fn device_session_create_find_revoke() {
    let (_env_id, user_id, _) = make_env().await;
    let repo = PgDeviceSessionRepository::new(pool().await);

    let s1 = repo
        .create(user_id, Some("fingerprint-aaa"), "hash-of-token-1")
        .await
        .expect("create session failed");
    assert!(s1.revoked_at.is_none());

    let found = repo
        .find_by_refresh_token_hash(user_id, "hash-of-token-1")
        .await
        .expect("find failed")
        .expect("session not found");
    assert_eq!(found.id, s1.id);

    // 不同 user_id 查不到(防止横向越权)
    let not_found = repo
        .find_by_refresh_token_hash(UserId::new(), "hash-of-token-1")
        .await
        .expect("find failed");
    assert!(not_found.is_none());

    // 撤销
    repo.revoke(s1.id).await.expect("revoke failed");

    let after_revoke = repo
        .find_by_refresh_token_hash(user_id, "hash-of-token-1")
        .await
        .expect("find failed");
    assert!(after_revoke.is_none(), "session should be hidden after revoke");
}

// ============================================================================
// PgFriendshipRepository 测试
// ============================================================================

#[tokio::test]
async fn friendship_request_idempotent() {
    let (env_id, alice, bob) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);

    let r1 = repo.create_request(env_id, alice, bob).await.expect("1st failed");
    let r2 = repo.create_request(env_id, alice, bob).await.expect("2nd (idempotent) failed");
    assert_eq!(r1.id, r2.id, "idempotent: same row");
    assert_eq!(r1.state, FriendRequestState::Pending);
}

#[tokio::test]
async fn friendship_request_self_rejected() {
    let (env_id, alice, _) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);
    let r = repo.create_request(env_id, alice, alice).await;
    assert!(matches!(r, Err(im_common::AppError::Validation(_))));
}

#[tokio::test]
async fn friendship_accept_creates_two_way() {
    let (env_id, alice, bob) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);

    let req = repo.create_request(env_id, alice, bob).await.expect("create failed");
    repo.respond_request(req.id, true).await.expect("accept failed");

    // 双向 friendships 都建好
    let alice_friends = repo.list_friends(alice, None, 100).await.expect("list failed");
    let bob_friends = repo.list_friends(bob, None, 100).await.expect("list failed");
    assert!(alice_friends.contains(&bob), "Alice should see Bob as friend");
    assert!(bob_friends.contains(&alice), "Bob should see Alice as friend");
}

#[tokio::test]
async fn friendship_block_and_is_blocked() {
    let (_env_id, alice, bob) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);

    // Alice blocks Bob: row (user_id=alice, friend_id=bob, blocked)
    repo.block(alice, bob).await.expect("block failed");

    // "is alice blocked by bob" = "does bob have alice in block list" = NO
    // (alice 主动 block bob,不是被 bob block)
    let alice_blocked_by_bob = repo.is_blocked(alice, bob).await.expect("is_blocked failed");
    assert!(!alice_blocked_by_bob, "alice is NOT blocked by bob (alice blocked bob)");

    // "is bob blocked by alice" = "does alice have bob in block list" = YES
    let bob_blocked_by_alice = repo.is_blocked(bob, alice).await.expect("is_blocked failed");
    assert!(bob_blocked_by_alice, "bob IS blocked by alice");
}

#[tokio::test]
async fn device_session_create_find_revoke_dup_removed() {
    // 留位,实际测试在 device_session_create_find_revoke 里
}

#[tokio::test]
async fn friendship_reject_marks_rejected() {
    let (env_id, alice, bob) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);

    let req = repo.create_request(env_id, alice, bob).await.expect("create failed");
    repo.respond_request(req.id, false).await.expect("reject failed");
    let after = repo.find_request(req.id).await.expect("find failed").expect("missing");
    assert_eq!(after.state, FriendRequestState::Rejected);
}

// ============================================================================
// PgConversationRepository 测试
// ============================================================================

#[tokio::test]
async fn conversation_create_and_find_dm() {
    let (env_id, alice, bob) = make_env().await;
    let repo = PgConversationRepository::new(pool().await);

    let conv = repo
        .create(env_id, ConversationKind::Dm, json!({}))
        .await
        .expect("create failed");
    assert_eq!(conv.kind, ConversationKind::Dm);

    // 模拟 ConversationService::create_dm:add member + 建 dm_pairs
    let (a, b) = if alice.0 < bob.0 { (alice, bob) } else { (bob, alice) };
    repo.add_member(conv.id, a, MemberRole::Member).await.expect("add a failed");
    repo.add_member(conv.id, b, MemberRole::Member).await.expect("add b failed");

    // 插 dm_pairs 行
    sqlx::query(
        r#"INSERT INTO dm_pairs (environment_id, user_a, user_b, conversation_id) VALUES ($1, $2, $3, $4)"#,
    )
    .bind(env_id.0)
    .bind(a.0)
    .bind(b.0)
    .bind(conv.id.0)
    .execute(&pool().await)
    .await
    .expect("dm_pairs insert failed");

    let found = repo.find_dm(env_id, alice, bob).await.expect("find_dm failed");
    assert!(found.is_some(), "DM should be findable");
    let is_m = repo.is_member(conv.id, alice).await.expect("is_member failed");
    assert!(is_m);
}

#[tokio::test]
async fn conversation_list_for_user() {
    let (env_id, alice, bob) = make_env().await;
    let repo = PgConversationRepository::new(pool().await);

    let conv = repo
        .create(env_id, ConversationKind::Group, json!({"game.topic": "test"}))
        .await
        .expect("create group failed");
    repo.add_member(conv.id, alice, MemberRole::Owner).await.unwrap();
    repo.add_member(conv.id, bob, MemberRole::Member).await.unwrap();

    let alice_list = repo.list_for_user(alice, None, 50).await.expect("list failed");
    assert!(alice_list.iter().any(|c| c.id == conv.id));

    let members = repo.list_members(conv.id).await.expect("members failed");
    assert_eq!(members.len(), 2);
}

// ============================================================================
// PgMessageRepository + PgSequenceAllocator 集成测试
// ============================================================================

#[tokio::test]
async fn message_send_full_flow_5_steps() {
    let (env_id, alice, _) = make_env().await;
    let conv_repo = PgConversationRepository::new(pool().await);
    let msg_repo = PgMessageRepository::new(pool().await);
    let seq = PgSequenceAllocator::new(pool().await);

    // 1. 建 conversation
    let conv = conv_repo
        .create(env_id, ConversationKind::Group, json!({}))
        .await
        .unwrap();
    conv_repo.add_member(conv.id, alice, MemberRole::Owner).await.unwrap();

    // 2. 模拟 send_message 5 步
    let mut tx = msg_repo.begin_tx().await.expect("begin_tx failed");
    let s1 = seq.next(&mut tx, conv.id).await.expect("seq1 failed");
    let m1 = msg_repo
        .insert_in_tx(
            &mut tx,
            NewMessage {
                id: MessageId::new(),
                conversation_id: conv.id,
                sequence: s1,
                sender_id: Some(alice),
                kind: "text".into(),
                content: json!({"text": "hello"}),
                reply_to: None,
                idempotency_key: "idem-001".into(),
                state: MessageState::Sent,
            },
        )
        .await
        .expect("insert1 failed");
    tx.commit().await.expect("commit1 failed");
    assert_eq!(m1.sequence, 1);

    // 3. 强单调:第 2 条 sequence=2
    let mut tx2 = msg_repo.begin_tx().await.expect("begin_tx2 failed");
    let s2 = seq.next(&mut tx2, conv.id).await.expect("seq2 failed");
    let m2 = msg_repo
        .insert_in_tx(
            &mut tx2,
            NewMessage {
                id: MessageId::new(),
                conversation_id: conv.id,
                sequence: s2,
                sender_id: Some(alice),
                kind: "text".into(),
                content: json!({"text": "world"}),
                reply_to: None,
                idempotency_key: "idem-002".into(),
                state: MessageState::Sent,
            },
        )
        .await
        .expect("insert2 failed");
    tx2.commit().await.expect("commit2 failed");
    assert_eq!(m2.sequence, 2);
    assert!(s2 > s1, "sequence must be strictly increasing");

    // 4. 幂等:同 (conv, sender, key) 再发,find_by_idempotency_key 命中
    let hit = msg_repo
        .find_by_idempotency_key(conv.id, alice, "idem-001")
        .await
        .expect("find idem failed")
        .expect("idem row missing");
    assert_eq!(hit.id, m1.id);

    // 5. 增量拉取(after_sequence=0 → 2 条)
    let list = msg_repo
        .list_after_sequence(conv.id, 0, 10)
        .await
        .expect("list failed");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].sequence, 1);
    assert_eq!(list[1].sequence, 2);
}

#[tokio::test]
async fn message_idempotency_null_sender_dedup() {
    let (env_id, _, _) = make_env().await;
    let conv_repo = PgConversationRepository::new(pool().await);
    let msg_repo = PgMessageRepository::new(pool().await);
    let seq = PgSequenceAllocator::new(pool().await);

    let conv = conv_repo.create(env_id, ConversationKind::System, json!({})).await.unwrap();

    // 系统消息 sender=NULL,同 idempotency_key 不应重复
    let mut tx = msg_repo.begin_tx().await.unwrap();
    let s = seq.next(&mut tx, conv.id).await.unwrap();
    let m1 = msg_repo
        .insert_in_tx(
            &mut tx,
            NewMessage {
                id: MessageId::new(),
                conversation_id: conv.id,
                sequence: s,
                sender_id: None, // 系统消息
                kind: "system".into(),
                content: json!({"event": "room_created"}),
                reply_to: None,
                idempotency_key: "sys-evt-001".into(),
                state: MessageState::Sent,
            },
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // 查 NULL sender 的同 key
    let hit = msg_repo
        .find_by_idempotency_key(conv.id, UserId::nil(), "sys-evt-001")
        .await
        .expect("find failed")
        .expect("missing");
    assert_eq!(hit.id, m1.id, "NULL sender + idem key must dedup per UNIQUE NULLS NOT DISTINCT");
}

// ============================================================================
// PgReactionRepository 测试
// ============================================================================

#[tokio::test]
async fn reaction_add_remove_idempotent() {
    let (env_id, alice, bob) = make_env().await;
    let conv_repo = PgConversationRepository::new(pool().await);
    let msg_repo = PgMessageRepository::new(pool().await);
    let seq = PgSequenceAllocator::new(pool().await);
    let react_repo = PgReactionRepository::new(pool().await);

    // 建 conv + 1 条 message
    let conv = conv_repo.create(env_id, ConversationKind::Group, json!({})).await.unwrap();
    let mut tx = msg_repo.begin_tx().await.unwrap();
    let s = seq.next(&mut tx, conv.id).await.unwrap();
    let m = msg_repo
        .insert_in_tx(
            &mut tx,
            NewMessage {
                id: MessageId::new(),
                conversation_id: conv.id,
                sequence: s,
                sender_id: Some(alice),
                kind: "text".into(),
                content: json!({"text": "reactable"}),
                reply_to: None,
                idempotency_key: "r-001".into(),
                state: MessageState::Sent,
            },
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // 1. add 幂等
    let r1 = react_repo.add(m.id, bob, "👍").await.expect("add 1 failed");
    let r2 = react_repo.add(m.id, bob, "👍").await.expect("add 2 (idempotent) failed");
    assert_eq!(r1.message_id, r2.message_id);
    assert_eq!(r1.user_id, r2.user_id);

    // 2. list 1 条
    let list = react_repo.list_for_message(m.id).await.expect("list failed");
    assert_eq!(list.len(), 1);

    // 3. remove
    let removed = react_repo.remove(m.id, bob, "👍").await.expect("remove failed");
    assert!(removed, "should remove existing");
    let list_after = react_repo.list_for_message(m.id).await.expect("list failed");
    assert_eq!(list_after.len(), 0);

    // 4. remove 幂等(再删返回 false,不报错)
    let removed_again = react_repo.remove(m.id, bob, "👍").await.expect("re-remove failed");
    assert!(!removed_again);
}

// ============================================================================
// C-6: IdentityService::link_account (Guest → 正式) 集成测试
// 依据: SRS §11 IM-ID-005 + GAME-ID-004
// ============================================================================

use chrono::Duration as ChronoDuration;
use secrecy::SecretString;
use std::collections::HashMap;
use std::sync::Arc;

// SigningKey helper (与 token.rs tests 用同 key, 保持 HS256 兼容)
mod link_test_token_key {
    use im_core::identity::token::SigningKey;
    use secrecy::SecretString;
    pub fn make(kid: &str) -> SigningKey {
        SigningKey {
            kid: kid.into(),
            key: SecretString::new(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
            ),
        }
    }
}
use link_test_token_key::make as make_token_key;

/// 单独建一个隔离 env (test 间不共享 env_id → 不冲突)
async fn make_link_env() -> EnvironmentId {
    let p = pool().await;
    let env_id: Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'link-tenant-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'link-game-' || gen_random_uuid()::text FROM t
            RETURNING id
        )
        INSERT INTO environments (id, game_id, name)
        SELECT gen_random_uuid(), g.id, 'test' FROM g
        RETURNING id
        "#,
    )
    .fetch_one(&p)
    .await
    .expect("make_link_env failed");
    EnvironmentId(env_id)
}

/// 共享 TokenService + Issue access token (用于构造合法的 client access JWT)
fn issue_test_token(ts: &Arc<TokenService>, user: &User) -> String {
    ts.issue_access_token(user)
        .expect("issue_access_token failed")
        .0
}

#[tokio::test]
async fn link_account_guest_upgrade_happy_path() {
    let user_repo = PgUserRepository::new(pool().await);
    let ts = Arc::new(TokenService::new(
        vec![make_token_key("v1")],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ));
    let svc = IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        ts.clone(),
        HashMap::new(),
    );

    // 1. 创建 Guest (kind=guest, extid=NULL)
    let env_id = make_link_env().await;
    let guest = user_repo
        .create(env_id, UserKind::Guest, None, Some("Guest1".into()))
        .await
        .expect("create guest failed");
    assert_eq!(guest.kind, UserKind::Guest);
    assert!(guest.external_identity.is_none());

    // 2. 用相同 key 的 TokenService 签发 access (模拟 client 持有)
    let access = issue_test_token(&ts, &guest);

    // 3. link_account
    let ext = ExternalIdentity {
        provider: "steam".into(),
        external_uid: "76561198000000001".into(),
    };
    let upgraded = svc
        .link_account(&access, ext.clone())
        .await
        .expect("link_account failed");

    // 4. TokenPair.user_id 保持不变 (IM-ID-005: 保留历史消息)
    assert_eq!(upgraded.user_id, guest.id, "user_id must persist (保留历史)");

    // 5. DB reload: kind='user' + external_identity 已绑定
    let reloaded = user_repo
        .find_by_id(guest.id)
        .await
        .expect("find_by_id failed")
        .expect("user not found");
    assert_eq!(reloaded.kind, UserKind::User, "kind must be upgraded");
    let ext2 = reloaded
        .external_identity
        .as_ref()
        .expect("external_identity must be set");
    assert_eq!(ext2.provider, "steam");
    assert_eq!(ext2.external_uid, "76561198000000001");
}

#[tokio::test]
async fn link_account_merge_conflict_returns_409() {
    let user_repo = PgUserRepository::new(pool().await);
    let ts = Arc::new(TokenService::new(
        vec![make_token_key("v1")],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ));
    let svc = IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        ts.clone(),
        HashMap::new(),
    );

    let env_id = make_link_env().await;

    // Guest A link → 升级成功
    let guest_a = user_repo
        .create(env_id, UserKind::Guest, None, Some("GuestA".into()))
        .await
        .expect("create guest_a failed");
    let a_access = issue_test_token(&ts, &guest_a);
    let ext = ExternalIdentity {
        provider: "steam".into(),
        external_uid: "76561198000000002".into(),
    };
    let r1 = svc.link_account(&a_access, ext.clone()).await;
    assert!(r1.is_ok(), "first link should succeed, got: {:?}", r1.err());

    // Guest B 同 (env, steam, 76561198000000002) → AccountMergeConflict
    let guest_b = user_repo
        .create(env_id, UserKind::Guest, None, Some("GuestB".into()))
        .await
        .expect("create guest_b failed");
    let b_access = issue_test_token(&ts, &guest_b);
    let r2 = svc.link_account(&b_access, ext).await;
    assert!(
        matches!(r2, Err(im_common::AppError::AccountMergeConflict)),
        "expected AccountMergeConflict, got: {:?}",
        r2
    );
}

#[tokio::test]
async fn link_account_banned_user_rejected_as_not_found() {
    let user_repo = PgUserRepository::new(pool().await);
    let ts = Arc::new(TokenService::new(
        vec![make_token_key("v1")],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ));
    let svc = IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        ts.clone(),
        HashMap::new(),
    );

    let env_id = make_link_env().await;
    let guest = user_repo
        .create(env_id, UserKind::Guest, None, Some("ToBeBanned".into()))
        .await
        .expect("create guest failed");
    user_repo
        .update_state(guest.id, UserState::Banned)
        .await
        .expect("ban failed");

    // 即使 token 有效, banned user link 必须被拒为 NotFound (不暴露存在性)
    let access = issue_test_token(&ts, &guest);
    let r = svc
        .link_account(
            &access,
            ExternalIdentity {
                provider: "xbox".into(),
                external_uid: "fatal-uid".into(),
            },
        )
        .await;
    assert!(
        matches!(r, Err(im_common::AppError::NotFound(_))),
        "expected NotFound (don't leak existence), got: {:?}",
        r
    );
}

#[tokio::test]
async fn link_account_idempotent_same_user() {
    let user_repo = PgUserRepository::new(pool().await);
    let ts = Arc::new(TokenService::new(
        vec![make_token_key("v1")],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ));
    let svc = IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        ts.clone(),
        HashMap::new(),
    );

    let env_id = make_link_env().await;
    let guest = user_repo
        .create(env_id, UserKind::Guest, None, Some("IdemGuest".into()))
        .await
        .expect("create guest failed");
    let access = issue_test_token(&ts, &guest);

    let ext = ExternalIdentity {
        provider: "epic".into(),
        external_uid: "epic-abc-001".into(),
    };

    // 第一次 link 升级
    let r1 = svc.link_account(&access, ext.clone()).await;
    assert!(r1.is_ok(), "first link should succeed: {:?}", r1.err());

    // 第二次 link 同 (user, ext) → 走 fast-path: 同 user, 续 token pair
    let r2 = svc.link_account(&access, ext).await;
    assert!(r2.is_ok(), "idempotent link should succeed: {:?}", r2.err());
    assert_eq!(r2.unwrap().user_id, guest.id);
}

#[tokio::test]
async fn link_account_already_linked_user_returns_validation() {
    let user_repo = PgUserRepository::new(pool().await);
    let ts = Arc::new(TokenService::new(
        vec![make_token_key("v1")],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ));
    let svc = IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        ts.clone(),
        HashMap::new(),
    );

    let env_id = make_link_env().await;
    // 直接创建 kind='user' 的账号
    let user = user_repo
        .create(
            env_id,
            UserKind::User,
            Some(ExternalIdentity {
                provider: "psn".into(),
                external_uid: "psn-001".into(),
            }),
            Some("PSNUser".into()),
        )
        .await
        .expect("create user failed");
    let reloaded = user_repo.find_by_id(user.id).await.unwrap().unwrap();
    let access = issue_test_token(&ts, &reloaded);

    // 对 kind='user' 二次 link → Validation
    let r = svc
        .link_account(
            &access,
            ExternalIdentity {
                provider: "xbox".into(),
                external_uid: "xbox-different-uid".into(),
            },
        )
        .await;
    assert!(
        matches!(r, Err(im_common::AppError::Validation(_))),
        "expected Validation, got: {:?}",
        r
    );
}

#[tokio::test]
async fn link_account_invalid_token_returns_unauthorized() {
    let ts = Arc::new(TokenService::new(
        vec![make_token_key("v1")],
        ChronoDuration::seconds(900),
        SecretString::new("test-pepper".into()),
    ));
    let svc = IdentityService::new(
        PgUserRepository::new(pool().await),
        PgDeviceSessionRepository::new(pool().await),
        ts,
        HashMap::new(),
    );

    let r = svc
        .link_account(
            "this-is-not-a-valid-jwt",
            ExternalIdentity {
                provider: "steam".into(),
                external_uid: "x".into(),
            },
        )
        .await;
    assert!(
        matches!(r, Err(im_common::AppError::Unauthorized(_))),
        "expected Unauthorized, got: {:?}",
        r
    );
}
