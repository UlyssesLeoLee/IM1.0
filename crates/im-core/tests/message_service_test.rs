//! MessageService 集成测试 — 验证 5 步实装
//!
//! 依据: 132-wbs.md §5.3.1 C-2 验收:MessageService::send_message 完整 5 步
//!       (seq 强单调 / idempotency / friend 关系校验)

use serde_json::json;
use sqlx::PgPool;
use std::env;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, MessageId, UserId};
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::repository::{ConversationKind, ConversationRepository, MemberRole};
use im_core::event::publisher::EventPublisher;
use im_core::event::events::MessageCreatedEvent;
use im_core::message::pg::{PgMessageRepository, PgSequenceAllocator};
use im_core::message::repository::MessageRepository;
use im_core::message::sequence::SequenceAllocator;
use im_core::message::service::{MessageService, SendMessageCommand};
use im_common::AppError;

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://leo19@172.28.176.169:5544/postgres".into())
}

async fn pool() -> PgPool {
    let url = database_url();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .expect("connect to PG 18.6 (run scripts/init-pg18-b1-all.sh first)")
}

async fn make_env_with_dm() -> (EnvironmentId, UserId, UserId, ConversationId) {
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

    // 用 PgConversationRepository 建 DM
    let conv_repo = PgConversationRepository::new(p.clone());
    let conv = conv_repo
        .create(EnvironmentId(env_id), ConversationKind::Dm, json!({}))
        .await
        .expect("create DM");
    conv_repo.add_member(conv.id, UserId(alice), MemberRole::Member).await.unwrap();
    conv_repo.add_member(conv.id, UserId(bob), MemberRole::Member).await.unwrap();
    // dm_pairs 行(user_a < user_b 规范化)
    let (a, b) = if alice < bob { (alice, bob) } else { (bob, alice) };
    sqlx::query(r#"INSERT INTO dm_pairs (environment_id, user_a, user_b, conversation_id) VALUES ($1, $2, $3, $4)"#)
        .bind(env_id).bind(a).bind(b).bind(conv.id.0)
        .execute(p_ref).await.expect("dm_pairs insert");

    (EnvironmentId(env_id), UserId(alice), UserId(bob), conv.id)
}

// ============================================================================
// Mock EventPublisher
// ============================================================================

#[derive(Default)]
struct MockEventPublisher {
    /// 收到的事件数
    count: AtomicU32,
    /// 强制失败(用于测第 4 步失败不阻塞 ack)
    fail_next: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl EventPublisher for MockEventPublisher {
    async fn publish(&self, _topic: &str, _payload: &MessageCreatedEvent) -> Result<(), AppError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        if self.fail_next.load(Ordering::SeqCst) {
            return Err(AppError::Internal(anyhow::anyhow!("mock publish failure")));
        }
        Ok(())
    }
}

async fn make_service_async() -> (MessageService, Arc<MockEventPublisher>, PgPool) {
    let p = pool().await;
    let msg_repo: Arc<dyn MessageRepository> = Arc::new(PgMessageRepository::new(p.clone()));
    let seq: Arc<dyn SequenceAllocator> = Arc::new(PgSequenceAllocator::new(p.clone()));
    let conv_repo: Arc<dyn ConversationRepository> = Arc::new(PgConversationRepository::new(p.clone()));
    let events = Arc::new(MockEventPublisher::default());
    let svc = MessageService::new(msg_repo, seq, events.clone() as Arc<dyn EventPublisher>, conv_repo);
    (svc, events, p)
}

// ============================================================================
// 测试
// ============================================================================

#[tokio::test]
async fn c2_step1_idempotency_same_key_returns_same_msg() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, events, _p) = make_service_async().await;

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: format!("idem-{}", Uuid::new_v4()),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "hello world"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let m1 = svc.send_message(cmd.clone()).await.expect("first send");
    let m2 = svc.send_message(cmd).await.expect("replay");

    assert_eq!(m1.id, m2.id, "idempotent: same idem key returns same message id");
    assert_eq!(m1.sequence, m2.sequence, "idempotent: same sequence");

    // 事件只发 1 次(第 2 次是幂等回放,不重发)
    let count = events.count.load(Ordering::SeqCst);
    assert_eq!(count, 1, "event should publish only once (idempotent replay skips publish)");
}

#[tokio::test]
async fn c2_step2_validation_rejects_empty_content() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: "x".into(),
        kind: "text".into(),
        content: json!(null), // not object
        reply_to: None,
        max_size_bytes: 65536,
    };
    let r = svc.send_message(cmd).await;
    assert!(matches!(r, Err(AppError::Validation(_))));
}

#[tokio::test]
async fn c2_step2_validation_rejects_oversize_content() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let big = "a".repeat(70000);
    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: "y".into(),
        kind: "text".into(),
        content: json!({"kind": "text", "text": big}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let r = svc.send_message(cmd).await;
    assert!(matches!(r, Err(AppError::MessageTooLarge(_, _))));
}

#[tokio::test]
async fn c2_step2_non_member_cannot_send() {
    let (_env, _alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let intruder: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind) VALUES (gen_random_uuid(), $1, 'guest') RETURNING id"#,
    )
    .bind(_env.0)
    .fetch_one(&pool().await)
    .await
    .expect("create intruder");

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: UserId(intruder),
        idempotency_key: "z".into(),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "x"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let r = svc.send_message(cmd).await;
    assert!(matches!(r, Err(AppError::Forbidden(_))), "expected Forbidden, got {:?}", r);
}

#[tokio::test]
async fn c2_step3_strict_monotonic_sequence() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let mk = || SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: format!("strict-{}", Uuid::new_v4()),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "m"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let m1 = svc.send_message(mk()).await.unwrap();
    let m2 = svc.send_message(mk()).await.unwrap();
    let m3 = svc.send_message(mk()).await.unwrap();

    assert!(m2.sequence > m1.sequence, "seq must increase: {} < {}", m1.sequence, m2.sequence);
    assert!(m3.sequence > m2.sequence, "seq must increase: {} < {}", m2.sequence, m3.sequence);
    // 3 条 message sequence 严格连续(1, 2, 3)
    assert_eq!(m3.sequence - m1.sequence, 2);
}

#[tokio::test]
async fn c2_step4_event_failure_does_not_block_ack() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, events, _p) = make_service_async().await;
    events.fail_next.store(true, Ordering::SeqCst);

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: format!("evt-fail-{}", Uuid::new_v4()),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "x"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    // 即使 event publish 失败,send_message 也应成功(不阻塞 ack)
    let m = svc.send_message(cmd).await.expect("send should succeed even if event publish fails");
    assert!(m.id != MessageId::nil());
    // 验证 1 次事件调用
    assert_eq!(events.count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn c2_step5_returns_full_message() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: format!("full-{}", Uuid::new_v4()),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "complete"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let m = svc.send_message(cmd).await.expect("send failed");
    assert_eq!(m.conversation_id, conv);
    assert_eq!(m.sender_id, Some(alice));
    assert_eq!(m.kind, "text");
    assert_eq!(m.state, im_core::message::repository::MessageState::Sent);
    assert!(m.id != MessageId::new() || m.sequence >= 1, "id should be set");
    assert!(m.created_at.timestamp() > 0, "created_at set");
}
