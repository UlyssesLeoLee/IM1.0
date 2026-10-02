//! `#[cfg(test)]` 共享夹具 —— 真 PG + 真 AppState + 真实 Bearer token
//!
//! ## 为什么需要它
//!
//! 三个端点族(消息动作 / 好友 / 用户资料)的 e2e 测试都要「一条真 PG 连接 +
//! 一个装满全部 service 的 `AppState` + 一对真签名的 access token」。复制三份
//! 的结果是: 往 `AppState::new` 加第 7 个 service 时要改三处, 漏一处则
//! **编译不过**(好) —— 但更糟的是漏改的那份夹具里, 某个 service 拿到的是
//! 上一个测试残留的实例, 绿灯却看起来正常。
//!
//! ## 为什么走真 PG 而不是 mock
//!
//! 这些 e2e 要验证的东西里, 有一半**只在真数据库上存在**:
//! - 路径与消息归属的一致性(要真的存一条消息再换 URL 打过去)
//! - `mark_read` 的单调性(要真的看 `last_read_sequence` 变没变)
//! - 幂等重放(要真的撞 UNIQUE 约束)
//!
//! mock 掉仓储层, 恰好把要测的那部分也 mock 掉了。
//!
//! ## 连不上 DB 时**跳过**而非失败
//!
//! 沿用 `auth_handlers` 的既有约定(见 gap-ledger §1.15): CI 无
//! `DATABASE_URL` 时不阻塞。代价是「没跑」与「跑过」在输出上无法区分 ——
//! 这个假绿灯向量已单独记录, 未擅自改。

#![cfg(test)]

use std::sync::Arc;

use uuid::Uuid;

use im_core::identity::repository::{User, UserKind, UserState};
use im_core::identity::token::{SigningKey, TokenService};

use super::error_response::json_response;
use super::state::AppState;

/// e2e 用的连接池 (max_connections=2: 单个测试只串行用两条)
pub async fn e2e_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .ok()
}

/// 一个 env + 三个用户 + 两个会话 + 一条 Alice 发的消息
///
/// - `alice` — **发送者 + 成员**, 正常路径
/// - `bob`   — **成员但不是发送者** (测「非 sender 编辑」)
/// - `carol` — **非成员** (测「越权」)
///
/// bob 单独存在是因为「不是发送者」和「不是成员」是**两种不同的拒绝理由**:
/// 合并成一个用户的话, 成员检查会先命中, 发送者检查那条分支根本没跑到,
/// 而测试名字仍然写着「非 sender」。
pub struct RestFixture {
    pub state: web::Data<AppState>,
    pub alice: im_common::ids::UserId,
    pub bob: im_common::ids::UserId,
    pub alice_token: String,
    pub bob_token: String,
    pub carol_token: String,
    pub conv: im_common::ids::ConversationId,
    pub other_conv: im_common::ids::ConversationId,
    pub msg: im_common::ids::MessageId,
}

pub async fn rest_fixture(p: &sqlx::PgPool) -> RestFixture {
    use im_core::conversation::pg::PgConversationRepository;
    use im_core::conversation::repository::{ConversationKind, ConversationRepository, MemberRole};
    use im_core::message::pg::PgMessageRepository;
    use im_core::message::pg::PgSequenceAllocator;
    use im_core::message::repository::MessageRepository;
    use im_core::reaction::pg::PgReactionRepository;
    use serde_json::json;

    let env_id: Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'rest-' || gen_random_uuid()::text) RETURNING id),
             g AS (INSERT INTO games (id, tenant_id, name) SELECT gen_random_uuid(), t.id, 'rest-game-' || gen_random_uuid()::text FROM t RETURNING id)
        INSERT INTO environments (id, game_id, name) SELECT gen_random_uuid(), g.id, 'test' FROM g RETURNING id
        "#,
    )
    .fetch_one(p)
    .await
    .expect("make env");
    let env = im_common::ids::EnvironmentId(env_id);

    async fn mk_user(p: &sqlx::PgPool, env_id: Uuid, name: &str) -> im_common::ids::UserId {
        let id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO users (id, environment_id, kind, display_name)
               VALUES (gen_random_uuid(), $1, 'user', $2) RETURNING id"#,
        )
        .bind(env_id)
        .bind(name)
        .fetch_one(p)
        .await
        .expect("create user");
        im_common::ids::UserId(id)
    }
    let alice = mk_user(p, env_id, "RestAlice").await;
    let bob = mk_user(p, env_id, "RestBob").await;
    let carol = mk_user(p, env_id, "RestCarol").await;

    let conv_repo: Arc<dyn ConversationRepository> =
        Arc::new(PgConversationRepository::new(p.clone()));
    let conv = conv_repo
        .create(env, ConversationKind::Dm, json!({}))
        .await
        .expect("create conv");
    conv_repo
        .add_member(conv.id, alice, MemberRole::Member)
        .await
        .unwrap();
    // Bob 在群里但没发过消息 —— 与 Carol(非成员)分开, 用于隔离「非 sender」这条分支
    conv_repo
        .add_member(conv.id, bob, MemberRole::Member)
        .await
        .unwrap();
    // 另一个会话: Alice 也在里面, 但不含那条消息 —— 用于测「路径与消息归属」
    let other_conv = conv_repo
        .create(env, ConversationKind::Dm, json!({}))
        .await
        .expect("create other conv");
    conv_repo
        .add_member(other_conv.id, alice, MemberRole::Member)
        .await
        .unwrap();

    let msg_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (conversation_id, sequence, sender_id, kind, content, idempotency_key) \
         VALUES ($1, 1, $2, 'text', '{\"kind\":\"text\",\"text\":\"hi\"}', gen_random_uuid()::text) RETURNING id",
    )
    .bind(conv.id.0)
    .bind(alice.0)
    .fetch_one(p)
    .await
    .expect("insert message");
    let msg = im_common::ids::MessageId(msg_id);

    // 与 main.rs 装配同构: 6 个 service
    let message_repo: Arc<dyn MessageRepository> = Arc::new(PgMessageRepository::new(p.clone()));
    let sequencer: Arc<dyn im_core::message::sequence::SequenceAllocator> =
        Arc::new(PgSequenceAllocator::new(p.clone()));
    let events: Arc<dyn im_core::event::publisher::EventPublisher> = Arc::new(
        im_core::event::publisher::NatsEventPublisher::connect("nats://stub:4222")
            .await
            .expect("stub publisher"),
    );

    let token_service = Arc::new(TokenService::new(
        vec![SigningKey {
            kid: "v1".into(),
            key: secrecy::SecretString::new(
                "test-key-must-be-32-bytes-or-more-padding-padding".into(),
            ),
        }],
        chrono::Duration::seconds(900),
        secrecy::SecretString::new("test-pepper".into()),
    ));

    let mk_token = |uid: im_common::ids::UserId| {
        let u = User {
            id: uid,
            environment_id: env,
            kind: UserKind::User,
            external_identity: None,
            state: UserState::Active,
            display_name: Some("e2e".into()),
            username: None,
            password_hash: None,
            created_at: chrono::Utc::now(),
        };
        token_service.issue_access_token(&u).expect("签 token").0
    };
    let alice_token = mk_token(alice);
    let bob_token = mk_token(bob);
    let carol_token = mk_token(carol);

    let mut secrets = std::collections::HashMap::new();
    secrets.insert(env, secrecy::SecretString::new("rest-test-secret".into()));

    let conversation_repo: Arc<dyn ConversationRepository> =
        Arc::new(PgConversationRepository::new(p.clone()));
    let state = web::Data::new(AppState::new(
        Arc::new(im_core::conversation::service::ConversationService::new(
            conversation_repo.clone(),
        )),
        Arc::new(im_core::message::service::MessageService::new(
            message_repo,
            sequencer,
            events,
            conversation_repo.clone(),
        )),
        token_service.clone(),
        Arc::new(im_core::identity::service::IdentityService::new(
            im_core::identity::pg::PgUserRepository::new(p.clone()),
            im_core::identity::pg::PgDeviceSessionRepository::new(p.clone()),
            token_service,
            secrets,
        )),
        Arc::new(im_core::settings::service::SettingsService::new(p.clone())),
        Arc::new(im_core::reaction::service::ReactionService::new(
            Arc::new(PgReactionRepository::new(p.clone())),
            Arc::new(PgMessageRepository::new(p.clone())),
            conversation_repo,
        )),
        Arc::new(im_core::relationship::service::RelationshipService::new(
            Arc::new(im_core::relationship::pg::PgFriendshipRepository::new(
                p.clone(),
            )),
        )),
    ));

    RestFixture {
        state,
        alice,
        bob,
        alice_token,
        bob_token,
        carol_token,
        conv: conv.id,
        other_conv: other_conv.id,
        msg,
    }
}

use actix_web::web;

/// `AppError` → HTTP 错误 —— 与各 handler 内部那份同构
///
/// 供夹具里的测试构造预期错误响应时使用。
#[allow(dead_code)]
pub fn err(code: im_common::ErrorCode) -> actix_web::Error {
    json_response(code, None, None)
}
