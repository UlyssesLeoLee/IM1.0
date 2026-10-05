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

/// 连不上 PG 时:静默跳过, 还是直接失败
///
/// ## 为什么必须在**代码里**做, 而不能靠 grep CI 输出
///
/// 最自然的想法是「跑完 grep 一下输出里有没有 `skip:`, 有就红」。行不通:
/// `cargo test` **默认丢弃通过测试的 stdout/stderr**, 而「跳过」在 libtest 眼里
/// 就是「通过」—— 那句 `eprintln!("skip: ...")` 根本不会出现在正常输出里。
/// 实测: 全量 `cargo test --workspace` 日志里 `SKIP_LINES=0`, 而当时确实有大批
/// 用例在跳过。只有加 `--nocapture` 才看得见, 但那会让 CI 输出变得又吵又不可靠。
///
/// 所以开关做在这里: 把「跳过」变成**真正的失败**, libtest 就一定会显示它。
///
/// ## 两种模式各自解决什么
///
/// - 未设 `IM_REQUIRE_PG`: 连不上就跳过。本机没 PG 时不该全红 —— 开发者
///   仍能跑其余 200+ 个用例。
/// - `IM_REQUIRE_PG=1`(CI): 连不上就 panic。CI 明确挂了 PG service container,
///   此时「连不上」只可能是 job 配错了 —— 而配置错误若以绿灯形式混过去,
///   整个门禁就失去意义。
///
/// ## 2026-10-03 的真实教训
///
/// 新写的 WS e2e 第一次运行「6 个全通过」, 实际是 6 个全部静默跳过(Docker
/// Desktop 已被关掉, PG 容器随之消失)。若不是顺手核对了耗时(10.01s ≈
/// `acquire_timeout(10s)`), 这会作为「WS 端到端已就位」被写进台账。
/// **任何新写 PG 相关测试的人都默认会踩这个坑** —— 它的形态是「看起来
/// 一切正常」, 所以必须由机制挡住, 不能靠自觉。
pub fn pg_required() -> bool {
    std::env::var("IM_REQUIRE_PG").as_deref() == Ok("1")
}

/// 供各测试文件的 `pool()` 在连不上时调用
///
/// 返回 `true` = 调用方应当 `return`(跳过); `IM_REQUIRE_PG=1` 时不返回,
/// 直接 panic。
pub fn skip_or_fail_pg(context: &str) -> bool {
    if pg_required() {
        panic!(
            "IM_REQUIRE_PG=1 但连不上 PG({context})。这个环境**声称**要跑 PG 测试, \
             连不上只能是配置错了。静默跳过会让「跑过」与「没跑」无法区分 —— \
             见 docs/gap-ledger.md §1.15 / §1.22。"
        );
    }
    eprintln!("skip: {context} —— 未设 DATABASE_URL 或连不上 PG");
    true
}

/// e2e 用的连接池 (max_connections=2: 单个测试只串行用两条)
pub async fn e2e_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&url)
        .await
        .ok();
    if pool.is_none() {
        skip_or_fail_pg("test_support::e2e_pool");
    }
    pool
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

    // 2026-10-03 修正: 种子消息此前是**手写 SQL** 插的, 且硬编码 `sequence = 1`。
    //
    // 那条 SQL 绕过了 `conversation_sequences` 分配器, 于是夹具造出一个生产中
    // **不可能出现**的状态: `messages` 里已有 sequence=1, 而分配器的
    // `next_sequence` 仍停在 1。之后任何一次真实 `send_message` 都会拿到 1,
    // 与既有行撞 `messages(conversation_id, sequence)` 唯一约束 ->
    // `sqlx insert` 失败 -> WS 侧表现为 `INTERNAL_ERROR: internal error`。
    //
    // 症状出现在广播链路上, 真正的原因却在夹具里 —— 这也是为什么 WS 用例
    // 必须先断言发送方的 ack(见 ws/e2e.rs): 否则会拿着「广播没送达」去查
    // WsHub, 而问题根本不在那里。
    //
    // 改法: 种子消息也走 `MessageService::send_message` —— 真实路径, 顺带
    // 不再在测试里复刻一遍生产逻辑(分配器 / 幂等 / 内容 schema 校验)。
    // 注意它必须在 message_service 构造**之后**才能调用, 故挪到下面。

    // 与 main.rs 装配同构: 6 个 service
    let message_repo: Arc<dyn MessageRepository> = Arc::new(PgMessageRepository::new(p.clone()));
    let sequencer: Arc<dyn im_core::message::sequence::SequenceAllocator> =
        Arc::new(PgSequenceAllocator::new(p.clone()));
    // 2026-10-04 D-3: 用**显式的** stub 类型, 不再借道 NatsEventPublisher。
    // 旧写法 `NatsEventPublisher::connect("nats://stub:4222")` 之所以能工作,
    // 正是因为那个 connect 根本不连 —— 一旦 D-3 让它真的去连, 这些用例就会
    // 变成「等 5 秒连接超时」的假失败。stub 的选择应当是显式的。
    let events: Arc<dyn im_core::event::publisher::EventPublisher> =
        Arc::new(im_core::event::publisher::StubEventPublisher::new());

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
        // 2026-10-03 修正: 此前用 `issue_access_token(&u)` —— 那是**刻意不带 dsid**
        // 的遗留入口, 其文档明写「签发登录/注册类 token 时应改用
        // issue_access_token_for_session」。后果: WS 鉴权
        // (`ws::handler::handle_auth` 读 `dsid` claim, 缺则 UNAUTHORIZED) 对本夹具
        // 签出的 token **一律拒绝**, 表现为
        // `access token carries no device session (dsid); re-authenticate`。
        //
        // 这与真实登录流程不一致: `IdentityService::issue_token_pair` 是
        // **先建 device session 再签 token**, 所以生产 token 带 dsid。夹具必须
        // 跟上, 否则 e2e 测的是一个生产中不存在的 token 形状。
        //
        // 这里用构造出的 session id 即可: `handle_auth` 只**读 claim**、不回查
        // `device_sessions` 表, 所以不需要真的插一行。
        token_service
            .issue_access_token_for_session(&u, Some(im_common::ids::DeviceSessionId::new()))
            .expect("签 token")
            .0
    };
    let alice_token = mk_token(alice);
    let bob_token = mk_token(bob);
    let carol_token = mk_token(carol);

    let mut secrets = std::collections::HashMap::new();
    secrets.insert(env, secrecy::SecretString::new("rest-test-secret".into()));

    let conversation_repo: Arc<dyn ConversationRepository> =
        Arc::new(PgConversationRepository::new(p.clone()));

    // 种子消息走真实路径(见上方长注释): 分配器 / 幂等 / schema 校验全都不绕。
    let message_service = im_core::message::service::MessageService::new(
        message_repo,
        sequencer,
        events,
        conversation_repo.clone(),
    );
    let seed = message_service
        .send_message(im_core::message::service::SendMessageCommand {
            conversation_id: conv.id,
            sender_id: alice,
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            kind: "text".to_string(),
            content: json!({"kind": "text", "text": "hi"}),
            reply_to: None,
            max_size_bytes: 65_536,
        })
        .await
        .expect("夹具的种子消息必须能通过真实 send_message 路径");
    let msg = seed.id;

    let state = web::Data::new(AppState::new(
        Arc::new(im_core::conversation::service::ConversationService::new(
            conversation_repo.clone(),
        )),
        Arc::new(message_service),
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
