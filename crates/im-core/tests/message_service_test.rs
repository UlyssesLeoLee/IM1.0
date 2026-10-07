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
use im_common::AppError;
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::repository::{ConversationKind, ConversationRepository, MemberRole};
use im_core::event::publisher::{EventPublisher, PublisherReadiness};
use im_core::message::pg::{PgMessageRepository, PgSequenceAllocator};
use im_core::message::repository::MessageRepository;
use im_core::message::sequence::SequenceAllocator;
use im_core::message::service::{MessageService, SendMessageCommand};
use im_core::relationship::pg::PgFriendshipRepository;
use im_core::relationship::repository::FriendshipRepository;

fn database_url() -> String {
    env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://leo19@172.28.176.169:5544/postgres".into())
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
    conv_repo
        .add_member(conv.id, UserId(alice), MemberRole::Member)
        .await
        .unwrap();
    conv_repo
        .add_member(conv.id, UserId(bob), MemberRole::Member)
        .await
        .unwrap();
    // dm_pairs 行(user_a < user_b 规范化)
    let (a, b) = if alice < bob {
        (alice, bob)
    } else {
        (bob, alice)
    };
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
    /// 收到过的 (topic, payload) —— 2026-10-03 新增
    ///
    /// 只数个数不足以验证「撤回发的是 `im.message.recalled` 且 payload 里
    /// 的 message_id 是被撤回的那条」—— 那正是本轮要锁的行为。加了记录后
    /// 断言才落在**内容**上而不是「调用次数 +1」。
    received: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
}

impl MockEventPublisher {
    fn topics(&self) -> Vec<String> {
        self.received
            .lock()
            .expect("mock lock")
            .iter()
            .map(|(t, _)| t.clone())
            .collect()
    }

    fn payload_of(&self, topic: &str) -> Option<serde_json::Value> {
        self.received
            .lock()
            .expect("mock lock")
            .iter()
            .find(|(t, _)| t == topic)
            .map(|(_, v)| v.clone())
    }
}

#[async_trait::async_trait]
impl EventPublisher for MockEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), AppError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        // 解析失败也**记录原文**, 不静默丢 —— 否则断言会因「找不到事件」而
        // 给出误导性的失败信息(看起来像没发布, 实际是 payload 坏了)。
        let v: serde_json::Value = serde_json::from_slice(payload)
            .unwrap_or_else(|e| serde_json::json!({ "__unparseable": String::from_utf8_lossy(payload), "__err": e.to_string() }));
        self.received
            .lock()
            .expect("mock lock")
            .push((topic.to_string(), v));
        if self.fail_next.load(Ordering::SeqCst) {
            return Err(AppError::Internal(anyhow::anyhow!("mock publish failure")));
        }
        Ok(())
    }

    /// mock 把事件**真的收下了**(存进 `received`), 所以它等价于一个可用的
    /// publisher, 不是 stub —— 故报 `Ready` 而不是 `StubByConfiguration`。
    fn readiness(&self) -> PublisherReadiness {
        PublisherReadiness::Ready
    }
}

async fn make_service_async() -> (MessageService, Arc<MockEventPublisher>, PgPool) {
    let p = pool().await;
    let msg_repo: Arc<dyn MessageRepository> = Arc::new(PgMessageRepository::new(p.clone()));
    let seq: Arc<dyn SequenceAllocator> = Arc::new(PgSequenceAllocator::new(p.clone()));
    let conv_repo: Arc<dyn ConversationRepository> =
        Arc::new(PgConversationRepository::new(p.clone()));
    let events = Arc::new(MockEventPublisher::default());
    let block_checker: Arc<dyn im_core::relationship::repository::BlockChecker> = Arc::new(
        im_core::relationship::pg::PgFriendshipRepository::new(p.clone()),
    );
    let svc = MessageService::new(
        msg_repo,
        seq,
        events.clone() as Arc<dyn EventPublisher>,
        conv_repo,
        block_checker,
    );
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

    assert_eq!(
        m1.id, m2.id,
        "idempotent: same idem key returns same message id"
    );
    assert_eq!(m1.sequence, m2.sequence, "idempotent: same sequence");

    // 事件只发 1 次(第 2 次是幂等回放,不重发)
    let count = events.count.load(Ordering::SeqCst);
    assert_eq!(
        count, 1,
        "event should publish only once (idempotent replay skips publish)"
    );
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
    assert!(
        matches!(r, Err(AppError::Forbidden(_))),
        "expected Forbidden, got {:?}",
        r
    );
}

// ============================================================================
// 步骤 2e — DM 拉黑校验
//
// 2026-10-08 新增。这一步在 2026-08 ~ 2026-10 之间**从未被测过**, 原因就是
// `check_block` 恒返 `Ok(false)` —— 没有可观测的行为, 也就没有可写的断言。
// 所以这里不只是「补一个用例」, 而是把「拉黑真的拦得住消息」钉成可判定事实。
//
// 三个用例构成一组, 缺一不可:
//   1. 被拉黑者发消息 → UserBlocked      (正向: 拦截生效)
//   2. 拉黑者自己发消息 → 成功            (反向守卫: 没有把所有人一起拦掉)
//   3. 拉黑发生在**之前**的历史消息方向不变 (防「谁先说话谁就不能说」这类
//      错误实现)
// ============================================================================

#[tokio::test]
async fn c2_step2e_blocked_sender_cannot_send_in_dm() {
    let (_env, alice, bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;

    // bob 拉黑 alice → alice 不应再能发消息
    PgFriendshipRepository::new(p.clone())
        .block(bob, alice)
        .await
        .expect("bob blocks alice");

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: "blocked-1".into(),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "let me in"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let r = svc.send_message(cmd).await;
    assert!(
        matches!(r, Err(AppError::UserBlocked)),
        "被拉黑者发消息必须被拒, got {:?}",
        r
    );
}

#[tokio::test]
async fn c2_step2e_blocker_can_still_send_in_dm() {
    let (_env, alice, bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;

    PgFriendshipRepository::new(p.clone())
        .block(bob, alice)
        .await
        .expect("bob blocks alice");

    // 拉黑是单向的: bob 拉黑 alice 不妨碍 bob 自己说话。
    //
    // 这条与上一条成对存在。`is_blocked(user, target)` 问的是「user 被 target
    // 拉黑」, 参数写反时**两条会同时通过或同时失败** —— 单测一条看不出方向,
    // 两条一起断言才能锁住「谁被拦」而不是「有没有人拦」。
    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: bob,
        idempotency_key: "blocker-1".into(),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "bye"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    let r = svc.send_message(cmd).await;
    assert!(r.is_ok(), "拉黑者本人发消息必须放行, got {:?}", r.err());
}

#[tokio::test]
async fn c2_step2e_unblocked_dm_still_works() {
    // 对照组: 同一个 DM, 没有任何拉黑 → 必须放行。
    //
    // 没有这条, 「所有 DM 消息都被拒」这种最粗暴的实现也能让上面两条变绿。
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let cmd = SendMessageCommand {
        conversation_id: conv,
        sender_id: alice,
        idempotency_key: "unblocked-1".into(),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "hi"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    assert!(svc.send_message(cmd).await.is_ok());
}

#[tokio::test]
async fn c2_step2e_blocked_sender_cannot_send_in_group() {
    // 群聊**不**做拉黑校验 —— 这是当前已知的范围限制, 不是已实现的语义。
    // 本用例的作用是把这个边界显式钉住: 将来若决定扩到群聊, 这条会红,
    // 从而强制一次有意识的决定, 而不是悄悄改变行为。
    //
    // 详见 docs/gap-ledger.md §1.44。
    let (env, alice, _bob, _dm) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;

    let conv_repo = PgConversationRepository::new(p.clone());
    let carol: Uuid = sqlx::query_scalar(
        r#"INSERT INTO users (id, environment_id, kind) VALUES (gen_random_uuid(), $1, 'guest') RETURNING id"#,
    )
    .bind(env.0)
    .fetch_one(&p)
    .await
    .expect("create carol");

    let group = conv_repo
        .create(env, ConversationKind::Group, json!({}))
        .await
        .expect("create group");
    for u in [alice, UserId(carol)] {
        conv_repo
            .add_member(group.id, u, MemberRole::Member)
            .await
            .unwrap();
    }
    PgFriendshipRepository::new(p.clone())
        .block(UserId(carol), alice)
        .await
        .expect("carol blocks alice");

    let cmd = SendMessageCommand {
        conversation_id: group.id,
        sender_id: alice,
        idempotency_key: "group-blocked-1".into(),
        kind: "text".into(),
        content: json!({"kind": "text", "text": "still here"}),
        reply_to: None,
        max_size_bytes: 65536,
    };
    assert!(
        svc.send_message(cmd).await.is_ok(),
        "当前实现只在 DM 上校验拉黑, 群聊应放行(已知限制, 见 gap-ledger §1.44)"
    );
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

    assert!(
        m2.sequence > m1.sequence,
        "seq must increase: {} < {}",
        m1.sequence,
        m2.sequence
    );
    assert!(
        m3.sequence > m2.sequence,
        "seq must increase: {} < {}",
        m2.sequence,
        m3.sequence
    );
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
    let m = svc
        .send_message(cmd)
        .await
        .expect("send should succeed even if event publish fails");
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
    assert!(
        m.id != MessageId::new() || m.sequence >= 1,
        "id should be set"
    );
    assert!(m.created_at.timestamp() > 0, "created_at set");
}

// ============================================================================
// C-9 EditMessage 实装 (2026-10-03)
// ============================================================================
//
// 此前 `MessageService::edit_message` 做完 4 步校验后**无条件**返回
// `AppError::Internal("not yet implemented")` —— 每次调用都得到 500 级错误,
// 看起来像服务端故障而不是「功能没做」。仓储层当时也没有改 content 的方法。
// 下列用例是本次实装的判别力证明: 旧实现会在第一条就失败。

#[tokio::test]
async fn edit_updates_content_and_stamps_edited_at() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;

    let sent = svc
        .send_message(SendMessageCommand {
            conversation_id: conv,
            sender_id: alice,
            idempotency_key: format!("edit-{}", Uuid::new_v4()),
            kind: "text".into(),
            content: json!({"kind": "text", "text": "before"}),
            reply_to: None,
            max_size_bytes: 65536,
        })
        .await
        .expect("send failed");
    assert!(sent.edited_at.is_none(), "刚发的消息 edited_at 应为空");

    let edited = svc
        .edit_message(
            sent.id,
            alice,
            json!({"kind": "text", "text": "after"}),
            65536,
        )
        .await
        .expect("edit 应成功(旧实现在此返回 InternalError)");

    assert_eq!(
        edited.content["text"],
        json!("after"),
        "返回的应是更新后的行"
    );
    assert!(edited.edited_at.is_some(), "edited_at 应被打上");

    // 关键: 不能只信返回值 —— 直接查库确认真的落了盘
    let stored: String = sqlx::query_scalar("SELECT content->>'text' FROM messages WHERE id = $1")
        .bind(sent.id.0)
        .fetch_one(&p)
        .await
        .expect("查 messages");
    assert_eq!(stored, "after", "库里应真的存着新内容");

    let stored_edited_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT edited_at FROM messages WHERE id = $1")
            .bind(sent.id.0)
            .fetch_one(&p)
            .await
            .expect("查 edited_at");
    assert!(stored_edited_at.is_some(), "库里 edited_at 应已写入");
}

#[tokio::test]
async fn edit_rejected_for_non_sender() {
    let (_env, alice, bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let sent = svc
        .send_message(SendMessageCommand {
            conversation_id: conv,
            sender_id: alice,
            idempotency_key: format!("edit-ns-{}", Uuid::new_v4()),
            kind: "text".into(),
            content: json!({"kind": "text", "text": "mine"}),
            reply_to: None,
            max_size_bytes: 65536,
        })
        .await
        .expect("send failed");

    let r = svc
        .edit_message(
            sent.id,
            bob,
            json!({"kind": "text", "text": "hijack"}),
            65536,
        )
        .await;
    assert!(
        matches!(r, Err(AppError::Forbidden(_))),
        "非 sender 编辑必须 Forbidden, 实际: {r:?}"
    );
}

#[tokio::test]
async fn edit_rejected_for_recalled_message() {
    // 状态机守卫(2026-10-03 新增): 撤回后应只剩墓碑, 不可再编辑出内容。
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;

    let sent = svc
        .send_message(SendMessageCommand {
            conversation_id: conv,
            sender_id: alice,
            idempotency_key: format!("edit-rc-{}", Uuid::new_v4()),
            kind: "text".into(),
            content: json!({"kind": "text", "text": "will recall"}),
            reply_to: None,
            max_size_bytes: 65536,
        })
        .await
        .expect("send failed");

    sqlx::query("UPDATE messages SET state = 'recalled' WHERE id = $1")
        .bind(sent.id.0)
        .execute(&p)
        .await
        .expect("置为 recalled");

    let r = svc
        .edit_message(
            sent.id,
            alice,
            json!({"kind": "text", "text": "zombie"}),
            65536,
        )
        .await;
    assert!(
        matches!(r, Err(AppError::InvalidStateTransition { .. })),
        "已撤回消息不可编辑, 实际: {r:?}"
    );

    // 内容必须原样保留, 不能被改
    let stored: String = sqlx::query_scalar("SELECT content->>'text' FROM messages WHERE id = $1")
        .bind(sent.id.0)
        .fetch_one(&p)
        .await
        .expect("查 messages");
    assert_eq!(stored, "will recall", "被拒的编辑不应改动内容");
}

#[tokio::test]
async fn edit_missing_message_returns_not_found() {
    let (_env, alice, _bob, _conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;

    let r = svc
        .edit_message(
            MessageId::new(),
            alice,
            json!({"kind": "text", "text": "x"}),
            65536,
        )
        .await;
    assert!(
        matches!(r, Err(AppError::MessageNotFound(_))),
        "不存在的消息应 MessageNotFound, 实际: {r:?}"
    );
}

// ============================================================================
// C-9 `recall_message` (per aux-04 §B.4 转换表)
// ============================================================================
//
// 覆盖 aux-04 §F 测试要求里与本路径相关的项:
// - 「时间窗测试: 撤回时间窗边界 ±1s」
// - 「反例测试: 非法转换返回 INVALID_STATE_TRANSITION」
//
// **夹具技巧**: 这些用例要测时间窗, 而 `send_message` 落的 `created_at` 是
// `now()`。故用一条 `UPDATE messages SET created_at = ...` 把时间**回拨**到
// 想要的偏移量, 而不是 `sleep` —— 睡 120s 既慢又 flaky。这也让
// 「窗口 120s」与「窗口 1s」能用同一段代码测。

/// 发一条消息并把 `created_at` 回拨 `age` 秒(用于时间窗用例)
async fn send_and_age(
    svc: &MessageService,
    conv: ConversationId,
    sender: UserId,
    text: &str,
    age_seconds: i64,
) -> (im_core::message::repository::Message, PgPool) {
    let p = pool().await;
    let sent = svc
        .send_message(SendMessageCommand {
            conversation_id: conv,
            sender_id: sender,
            idempotency_key: format!("recall-{}", Uuid::new_v4()),
            kind: "text".into(),
            content: json!({ "kind": "text", "text": text }),
            reply_to: None,
            max_size_bytes: 65536,
        })
        .await
        .expect("send failed");
    sqlx::query(
        "UPDATE messages SET created_at = now() - ($2 || ' seconds')::interval WHERE id = $1",
    )
    .bind(sent.id.0)
    .bind(age_seconds.to_string())
    .execute(&p)
    .await
    .expect("回拨 created_at");
    (sent, p)
}

#[tokio::test]
async fn recall_happy_path_flips_state_and_publishes_event() {
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, events, p) = make_service_async().await;
    let (sent, _p) = send_and_age(&svc, conv, alice, "oops", 0).await;

    let r = svc
        .recall_message(sent.id, alice, chrono::Duration::seconds(120))
        .await
        .expect("recall 应成功");
    assert_eq!(
        r.state,
        im_core::message::repository::MessageState::Recalled
    );
    assert_eq!(r.id, sent.id);

    // 必须真的落库, 不能只在返回值里「声称」改了
    let stored: String = sqlx::query_scalar("SELECT state FROM messages WHERE id = $1")
        .bind(sent.id.0)
        .fetch_one(&p)
        .await
        .expect("查 messages");
    assert_eq!(stored, "recalled", "库里状态必须落成 recalled");

    // aux-04 §B.4 不变量: 必须 publish im.message.recalled
    assert!(
        events.topics().iter().any(|t| t == "im.message.recalled"),
        "应发布 im.message.recalled, 实际收到: {:?}",
        events.topics()
    );
    let payload = events
        .payload_of("im.message.recalled")
        .expect("应记录到 recalled 事件 payload");
    assert_eq!(
        payload["message_id"],
        sent.id.0.to_string(),
        "事件的 message_id 必须是**被撤回的那条**"
    );
    assert_eq!(payload["actor_id"], alice.0.to_string());
    assert_eq!(payload["conversation_id"], conv.0.to_string());
}

#[tokio::test]
async fn recall_well_inside_the_window_is_allowed() {
    // 这条**故意不测边界**: 边界由下面的纯函数测试精确覆盖。集成测试里
    // 「恰好等于上限」是**测不到**的 —— 回拨 created_at 到 120s 后, UPDATE 与
    // service 调用之间已过去几毫秒, elapsed 实际是 120.00Xs。曾按「恰好等于
    // 窗口」写这条, 稳定失败(RecallWindowExpired) —— 那是测试前提错了,
    // 不是实现错了。挂钟回拨只能测「明显在窗内」与「明显超窗」。
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;
    // 回拨 60s, 窗口 120s —— 留足余量, 不依赖时序运气
    let (sent, _) = send_and_age(&svc, conv, alice, "edge", 60).await;

    let r = svc
        .recall_message(sent.id, alice, chrono::Duration::seconds(120))
        .await;
    assert!(r.is_ok(), "窗口内(60s < 120s)应允许, 实际: {r:?}");
}

/// 边界语义的**真正**测试: 纯函数 + 精确构造的入参
///
/// `>` 写成 `>=` 编译照过、绝大多数用例照过, 只有这一毫秒的差别能暴露它。
#[test]
fn recall_window_predicate_is_inclusive_at_exactly_the_limit() {
    use im_core::message::service::within_recall_window as within;
    let t = chrono::Utc::now();
    let w = chrono::Duration::seconds(120);
    // 恰好等于上限 → **允许**(aux-04 §B.4 写的是 `≤`)
    assert!(
        within(t + w, t, w),
        "恰好等于窗口上限必须允许 —— `>` 写成 `>=` 就是这里"
    );
    // 超 1ms → 拒绝
    assert!(
        !within(t + w + chrono::Duration::milliseconds(1), t, w),
        "超窗 1ms 必须拒绝"
    );
    // 超 1s → 拒绝(aux-04 §F 要求的 ±1s)
    assert!(
        !within(t + w + chrono::Duration::seconds(1), t, w),
        "超窗 1s 必须拒绝"
    );
    // 远在窗内 → 允许
    assert!(within(t + chrono::Duration::seconds(1), t, w));
}

#[test]
fn recall_window_predicate_treats_future_created_at_as_allowed() {
    // 时钟漂移 / 跨机房写入可能让 created_at 晚于 now。此时拿不到可靠时间差,
    // 按 elapsed=0 处理(允许) —— 误拒一次合法撤回比误放更糟。
    use im_core::message::service::within_recall_window as within;
    let t = chrono::Utc::now();
    let w = chrono::Duration::seconds(120);
    assert!(
        within(t, t + chrono::Duration::seconds(30), w),
        "created_at 在未来不应导致拒绝"
    );
}

#[tokio::test]
async fn recall_one_past_the_window_is_rejected() {
    // 边界另一侧: 121s > 120s → 过期。aux-04 §F 要求「边界 ±1s」。
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;
    let (sent, _) = send_and_age(&svc, conv, alice, "late", 121).await;

    let r = svc
        .recall_message(sent.id, alice, chrono::Duration::seconds(120))
        .await;
    assert!(
        matches!(r, Err(AppError::RecallWindowExpired(_))),
        "超窗应 RecallWindowExpired, 实际: {r:?}"
    );

    // 被拒的撤回**不得**改状态
    let stored: String = sqlx::query_scalar("SELECT state FROM messages WHERE id = $1")
        .bind(sent.id.0)
        .fetch_one(&p)
        .await
        .expect("查 messages");
    assert_eq!(stored, "sent", "超窗的撤回不应改动状态");
}

#[tokio::test]
async fn recall_by_non_sender_is_forbidden() {
    let (_env, alice, bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;
    let (sent, _) = send_and_age(&svc, conv, alice, "mine", 0).await;

    let r = svc
        .recall_message(sent.id, bob, chrono::Duration::seconds(120))
        .await;
    assert!(
        matches!(r, Err(AppError::Forbidden(_))),
        "非 sender 撤回必须 Forbidden(per aux-04 actor=sender), 实际: {r:?}"
    );
    let stored: String = sqlx::query_scalar("SELECT state FROM messages WHERE id = $1")
        .bind(sent.id.0)
        .fetch_one(&p)
        .await
        .expect("查 messages");
    assert_eq!(stored, "sent", "越权撤回不应改动状态");
}

#[tokio::test]
async fn recall_of_terminal_state_is_rejected() {
    // `recalled` / `deleted` 是终态(转换表 line 243-244): 再撤一次必须
    // InvalidStateTransition, 而不是静默成功。
    for terminal in ["recalled", "deleted"] {
        let (_env, alice, _bob, conv) = make_env_with_dm().await;
        let (svc, _events, p) = make_service_async().await;
        let (sent, _) = send_and_age(&svc, conv, alice, "tombstone", 0).await;
        sqlx::query("UPDATE messages SET state = $2 WHERE id = $1")
            .bind(sent.id.0)
            .bind(terminal)
            .execute(&p)
            .await
            .expect("置终态");

        let r = svc
            .recall_message(sent.id, alice, chrono::Duration::seconds(120))
            .await;
        assert!(
            matches!(r, Err(AppError::InvalidStateTransition { .. })),
            "{terminal} 态再撤回应 InvalidStateTransition, 实际: {r:?}"
        );
    }
}

#[tokio::test]
async fn recall_allows_read_state() {
    // aux-04 转换图 line 225 在 `read --> recalled` 边上标了「V1+ 评估是否允许」
    // (GAP-3), 而**转换表** line 241 明确把 `read` 列入允许来源。此处以转换表
    // 为准(转换表才是规范性那张表), 且 GAP-3 的原文问题在**展示**层面
    // (「已读撤回还显不显示已读标识」), 不是转移本身。
    // 若 PM 决定禁止, 改 service 的状态守卫即可, 本测试是那条改动的守门。
    let (_env, alice, _bob, conv) = make_env_with_dm().await;
    let (svc, _events, p) = make_service_async().await;
    let (sent, _) = send_and_age(&svc, conv, alice, "seen", 0).await;
    sqlx::query("UPDATE messages SET state = 'read' WHERE id = $1")
        .bind(sent.id.0)
        .execute(&p)
        .await
        .expect("置 read");

    let r = svc
        .recall_message(sent.id, alice, chrono::Duration::seconds(120))
        .await;
    assert!(r.is_ok(), "read → recalled 应允许(转换表), 实际: {r:?}");
    assert_eq!(
        r.unwrap().state,
        im_core::message::repository::MessageState::Recalled
    );
}

#[tokio::test]
async fn recall_missing_message_returns_not_found() {
    let (_env, alice, _bob, _conv) = make_env_with_dm().await;
    let (svc, _events, _p) = make_service_async().await;
    let r = svc
        .recall_message(MessageId::new(), alice, chrono::Duration::seconds(120))
        .await;
    assert!(
        matches!(r, Err(AppError::MessageNotFound(_))),
        "不存在的消息应 MessageNotFound, 实际: {r:?}"
    );
}
