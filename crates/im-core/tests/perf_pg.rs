//! aux-06 §D.2 的真 PG 基准:A-002 / A-003 / A-004 / A-005 / A-006 / A-007 / A-011
//!
//! ## 为什么这些以前没测
//!
//! aux-06 §D.2 把这 7 项列为「尚未实测」, 理由是
//! 「criterion 基准跑不了异步 DB 往返; 需 criterion-async 或自建计时 harness +
//! **CI 的 PG service container**(Docker 目前在**本机**不可用)」。
//!
//! 括号里那句是关键: 挡住这 7 项的从来不是**没有 PG**, 而是**本机**没有 PG。
//! CI 的 `test-integration` job 一直带着 `postgres:18.6` service container,
//! 而那 7 项的宿主代码(仓库 / service)早就在 22 个真 PG 集成测试里被跑过。
//! 也就是说: 缺的是把基准搬进 CI 的那一步, 不是能力。
//!
//! 本文件就是那一步。
//!
//! ## 为什么是 `#[ignore]` 而不是普通集成测试
//!
//! 7 项里有 A-002, 它每次都跑 argon2id(默认参数, 故意慢, ~30ms+)。
//! 再乘上迭代次数, 塞进 `cargo test --workspace` 会明显拖慢**每个**开发者
//! 的本地全量测试, 而本地大多没有 PG, 本来也跑不了。
//!
//! 故默认 `#[ignore]`, 由 CI 用显式命令跑(见 `.github/workflows/ci.yml`)。
//! **代价**: 没人本地能顺手跑它 —— 所以下面的 `report` 把数字打成
//! `PERF|...|` 单行, CI 日志里可以直接 grep, 不必翻人类可读表格。
//!
//! ## 断言策略(以及为什么不是 1.0× 目标)
//!
//! 每个用例的 ceiling 都取 aux-06 目标的一个**明确倍数**, 而不是目标本身:
//! CI runner 是共享虚机, P99 的噪声量级与被测延迟同阶; 而这里要抓的是
//! 「**数量级**回退」(索引没了 / N+1 查询 / 全表扫描), 那种回退是 10×~100×,
//! 不会藏在 3× 的余量里。
//!
//! 用 P50 断言而不是 P99: 本文件的迭代次数是几十量级, 小样本的 P99 等于
//! 「最大值 + 噪声」, 拿它做门禁是在跟 runner 的抖动对赌。P99 仍然**打印**
//! —— aux-06 §D.2 要的是实测值, 打印出来不等于不用于门禁。
//!
//! ## 跑法
//! ```sh
//! export DATABASE_URL=postgres://im:im@localhost:5432/im_test   # 迁移须先跑过
//! cargo test -p im-core --test perf_pg -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` 是必须的: A-003 测的是**行锁竞争**, 多个用例并行会
//! 互相制造它本不该有的竞争。

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Duration as ChronoDuration;
use secrecy::Secret;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_core::conversation::pg::PgConversationRepository;
use im_core::conversation::service::ConversationService;
use im_core::identity::pg::{PgDeviceSessionRepository, PgUserRepository};
use im_core::identity::service::IdentityService;
use im_core::identity::token::{SigningKey, TokenService};
use im_core::message::pg::{PgMessageRepository, PgSequenceAllocator};
use im_core::message::repository::{MessageRepository, MessageState, NewMessage};
use im_core::message::sequence::SequenceAllocator;
use im_core::relationship::pg::PgFriendshipRepository;
use im_core::relationship::repository::FriendshipRepository;

// ============================================================================
// 计时 / 报告 / 断言
// ============================================================================

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 取分位数。`p` 为 0~1; 索引向上取整再减 1, 使 p=1.0 落在最后一个样本上。
fn pct(sorted: &[Duration], p: f64) -> Duration {
    assert!(!sorted.is_empty(), "分位数不能对空样本计算");
    let n = sorted.len();
    let idx = ((n as f64 * p).ceil() as usize)
        .saturating_sub(1)
        .min(n - 1);
    sorted[idx]
}

/// 打印一行机器可读的结果, **返回 P50(ms)**。
///
/// P99 也在这一行里打印, 但**不返回** —— 返回它就得在每个调用点写
/// `let _ = p99;` 消告警, 而那行代码对读者毫无意义。门禁只对 P50 断言
/// (理由见文件头), P99 的用途是写进 aux-06 §D.2, 那靠的是**日志**。
fn report(alg: &str, what: &str, mut samples: Vec<Duration>) -> f64 {
    samples.sort_unstable();
    let p50 = ms(pct(&samples, 0.50));
    let p99 = ms(pct(&samples, 0.99));
    let min = ms(samples[0]);
    let max = ms(*samples.last().expect("样本非空"));
    println!(
        "PERF|{alg}|{what}|n={}|min_ms={min:.3}|p50_ms={p50:.3}|p99_ms={p99:.3}|max_ms={max:.3}",
        samples.len()
    );
    p50
}

/// 对 P50 断言。`ceiling_ms` 是 aux-06 目标的 `mult` 倍, 写进消息里,
/// 这样 CI 红了能立刻看出是「差多少倍」而不是「多少毫秒」。
fn assert_p50_ceiling(alg: &str, what: &str, p50_ms: f64, target_ms: f64, mult: f64) {
    let ceiling = target_ms * mult;
    assert!(
        p50_ms <= ceiling,
        "PERF 回退 {alg} / {what}: P50 = {p50_ms:.3} ms, 超过 ceiling {ceiling:.1} ms \
         (aux-06 目标 {target_ms} ms × {mult})。\
         若是共享 runner 抖动请直接重跑; 若重跑仍红, 说明确实回退了 \
         (先查索引是否还在、是否变成了全表扫描)。"
    );
}

// ============================================================================
// Fixtures
// ============================================================================

fn database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| {
        // 故意**不**给本地默认值兜底成本地能跑: 这个文件就是要真 PG,
        // 默认值指向本机那套不存在的实例只会把「环境没配好」伪装成「测试失败」。
        // fail-closed: 取不到 DATABASE_URL 就明确报错。
        panic!(
            "DATABASE_URL 未设置。\
             本文件是**真 PG** 基准(aux-06 §D.2 的 A-002/003/004/005/006/007/011), \
             不接受 mock —— 跑它必须先有 PG 并跑过迁移。\n\
             本地: export DATABASE_URL=postgres://...; \
             CI: 由 test-integration job 的 postgres service container 提供。"
        );
    })
}

async fn pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url())
        .await
        .expect("连不上 PG(是否跑过迁移? DATABASE_URL 对不对?)")
}

async fn make_env() -> (EnvironmentId, UserId, UserId) {
    let p = pool().await;
    let env_id: Uuid = sqlx::query_scalar(
        r#"
        WITH t AS (
            INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'perf-' || gen_random_uuid()::text)
            RETURNING id
        ), g AS (
            INSERT INTO games (id, tenant_id, name)
            SELECT gen_random_uuid(), t.id, 'perf-game-' || gen_random_uuid()::text FROM t
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

    let a: Uuid = sqlx::query_scalar(
        "INSERT INTO users (id, environment_id, kind, display_name) \
         VALUES (gen_random_uuid(), $1, 'user', 'PerfAlice') RETURNING id",
    )
    .bind(env_id)
    .fetch_one(&p)
    .await
    .expect("create Alice failed");

    let b: Uuid = sqlx::query_scalar(
        "INSERT INTO users (id, environment_id, kind, display_name) \
         VALUES (gen_random_uuid(), $1, 'user', 'PerfBob') RETURNING id",
    )
    .bind(env_id)
    .fetch_one(&p)
    .await
    .expect("create Bob failed");

    (EnvironmentId(env_id), UserId(a), UserId(b))
}

/// 建一个 DM 会话, 并预置 `conversation_sequences` 行。
///
/// sequence 分配器是 `UPDATE ... RETURNING`, 行不存在会直接报
/// `conversation_sequences row not found` —— 不预置就会把「fixture 没搭好」
/// 混进性能数字里。
async fn make_dm(env: EnvironmentId, a: UserId, b: UserId) -> ConversationId {
    let repo = Arc::new(PgConversationRepository::new(pool().await));
    let svc = ConversationService::new(repo);
    let conv = svc.create_dm(env, a, b).await.expect("create_dm failed");

    let p = pool().await;
    sqlx::query(
        "INSERT INTO conversation_sequences (conversation_id, next_sequence) \
         VALUES ($1, 1) ON CONFLICT (conversation_id) DO NOTHING",
    )
    .bind(conv.id.0)
    .execute(&p)
    .await
    .expect("seed conversation_sequences failed");

    conv.id
}

/// 往会话里灌 `n` 条消息(供 A-006 增量拉取用)。
async fn seed_messages(conv: ConversationId, sender: UserId, n: i64) {
    let repo = PgMessageRepository::new(pool().await);
    let alloc = PgSequenceAllocator::new(pool().await);
    for i in 0..n {
        let mut tx = repo.begin_tx().await.expect("begin_tx failed");
        let seq = alloc
            .next(&mut tx, conv)
            .await
            .expect("sequence alloc failed");
        repo.insert_in_tx(
            &mut tx,
            NewMessage {
                id: im_common::ids::MessageId(Uuid::new_v4()),
                conversation_id: conv,
                sequence: seq,
                sender_id: Some(sender),
                kind: "text".into(),
                content: json!({ "kind": "text", "text": format!("seed {i}") }),
                reply_to: None,
                idempotency_key: format!("seed-{i}-{}", Uuid::new_v4()),
                state: MessageState::Sent,
            },
        )
        .await
        .expect("insert seed message failed");
        tx.commit().await.expect("commit failed");
    }
}

fn token_service() -> Arc<TokenService> {
    Arc::new(TokenService::new(
        vec![SigningKey {
            kid: "v1".into(),
            key: Secret::new("perf-bench-key-must-be-32-bytes-or-more-padding-padding".into()),
        }],
        ChronoDuration::seconds(900),
        Secret::new("perf-bench-pepper".into()),
    ))
}

// ============================================================================
// A-002 Refresh Token 校验 + 旋转 —— 目标 < 50ms (含 1 次 DB 写)
// ============================================================================

/// 迭代次数刻意压低: 每次 `refresh` 都要跑一次 argon2id(默认参数 ~30ms),
/// 而每个样本都得先 `authenticate` 拿一个**未被用过**的 refresh token
/// (refresh 会旋转并撤销旧的, 同一个 token 只能用一次)。N=8 已足够看出
/// 数量级, 再多只是让 CI 多等几秒。
const A002_N: usize = 8;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a002_refresh_token_rotate() {
    let (env, _alice, _bob) = make_env().await;
    let p = pool().await;

    let svc = IdentityService::new(
        PgUserRepository::new(p.clone()),
        PgDeviceSessionRepository::new(p.clone()),
        token_service(),
        HashMap::new(),
    );

    let username = format!("perf_a002_{}", Uuid::new_v4());
    svc.register(im_core::identity::service::RegisterCommand {
        environment_id: env,
        username: username.clone(),
        password: "perf-password-123".into(),
        display_name: Some("A002".into()),
    })
    .await
    .expect("register failed");

    let mut samples = Vec::with_capacity(A002_N);
    for _ in 0..A002_N {
        // 每次都要新的 refresh token —— 旋转会让旧 token 失效。
        // `authenticate` 的开销是 setup, **不计入**采样。
        let pair = svc
            .authenticate(env, &username, "perf-password-123")
            .await
            .expect("authenticate failed");
        let rt = pair.refresh_token.0;

        let t0 = Instant::now();
        let rotated = svc
            .refresh(&rt)
            .await
            .expect("refresh failed —— 令牌未被用过的前提下不该失败");
        samples.push(t0.elapsed());

        assert_ne!(rotated.refresh_token.0, rt, "旋转必须换新 refresh token");
    }

    let p50 = report("A-002", "refresh_token 校验+旋转(含 argon2id)", samples);
    assert_p50_ceiling("A-002", "refresh", p50, 50.0, 3.0);
}

// ============================================================================
// A-003 Conversation Sequence 分配 —— 目标 < 10ms (无竞争)
// ============================================================================

const A003_N: usize = 50;
/// aux-06 §B A-003 的性能模型直接给了 100 并发同会话的 P99 目标(50ms),
/// 故并发档就用 100, 不缩小 —— 缩小就没有对照意义了。
const A003_CONCURRENT: usize = 100;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a003_sequence_alloc() {
    let (env, alice, bob) = make_env().await;
    let conv = make_dm(env, alice, bob).await;

    let p = pool().await;
    let repo = PgMessageRepository::new(p.clone());
    let alloc = PgSequenceAllocator::new(p.clone());

    // --- 无竞争: 串行分配 ---
    let mut samples = Vec::with_capacity(A003_N);
    for _ in 0..A003_N {
        let t0 = Instant::now();
        let mut tx = repo.begin_tx().await.expect("begin_tx failed");
        let seq = alloc.next(&mut tx, conv).await.expect("next failed");
        tx.commit().await.expect("commit failed");
        samples.push(t0.elapsed());
        assert!(seq > 0, "sequence 必须为正");
    }
    let p50 = report("A-003", "sequence 分配(无竞争, 串行)", samples);
    assert_p50_ceiling("A-003", "serial", p50, 10.0, 3.0);

    // --- 竞争: 100 并发同会话, 量行锁串行化 ---
    //
    // 用 `tokio::spawn` + join_all 一次性发起。若逐个 await 就没有并发,
    // 测到的还是无竞争延迟 —— 那是**最容易被自己骗到**的一种测法。
    let mut set = tokio::task::JoinSet::new();
    let wall = Instant::now();
    for _ in 0..A003_CONCURRENT {
        let p = p.clone();
        set.spawn(async move {
            let repo = PgMessageRepository::new(p.clone());
            let alloc = PgSequenceAllocator::new(p);
            let t0 = Instant::now();
            let mut tx = repo.begin_tx().await.expect("begin_tx failed");
            let seq = alloc.next(&mut tx, conv).await.expect("next failed");
            tx.commit().await.expect("commit failed");
            (t0.elapsed(), seq)
        });
    }
    let mut per_op = Vec::with_capacity(A003_CONCURRENT);
    while let Some(joined) = set.join_next().await {
        let (d, seq) = joined.expect("并发任务 panic");
        assert!(seq > 0);
        per_op.push(d);
    }
    let total_ms = ms(wall.elapsed());
    println!("PERF|A-003|{A003_CONCURRENT} 并发同会话|wall_ms={total_ms:.3}");
    let p50 = report("A-003", "sequence 分配(100 并发, 含锁等待)", per_op);
    // 100 并发全部排在行锁后面, P50 含约 50 次锁等待; aux-06 给的目标是 50ms。
    // 这里只挡「数量级回退」: 10× 余量。
    assert_p50_ceiling("A-003", "concurrent-100", p50, 50.0, 10.0);
}

// ============================================================================
// A-004 Idempotency Key 查重 —— 目标 < 5ms (走 UNIQUE 索引)
// ============================================================================

const A004_N: usize = 50;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a004_idempotency_key_lookup() {
    let (env, alice, bob) = make_env().await;
    let conv = make_dm(env, alice, bob).await;
    seed_messages(conv, alice, 1).await;

    let repo = PgMessageRepository::new(pool().await);
    let key = format!("idem-{}", Uuid::new_v4());

    // 先真的插一条, 否则每次都走「查不到」的分支 —— 那是**不同的**查询路径
    // (aux-07 也把「命中」与「未命中」分开列了)。测的是「命中」。
    let existing = repo
        .find_by_idempotency_key(conv, alice, &key)
        .await
        .expect("lookup failed");
    assert!(existing.is_none(), "预置的 key 不应已存在");

    let mut tx = repo.begin_tx().await.expect("begin_tx failed");
    let alloc = PgSequenceAllocator::new(pool().await);
    let seq = alloc.next(&mut tx, conv).await.expect("next failed");
    repo.insert_in_tx(
        &mut tx,
        NewMessage {
            id: im_common::ids::MessageId(Uuid::new_v4()),
            conversation_id: conv,
            sequence: seq,
            sender_id: Some(alice),
            kind: "text".into(),
            content: json!({ "kind": "text", "text": "idem" }),
            reply_to: None,
            idempotency_key: key.clone(),
            state: MessageState::Sent,
        },
    )
    .await
    .expect("insert failed");
    tx.commit().await.expect("commit failed");

    let mut samples = Vec::with_capacity(A004_N);
    for _ in 0..A004_N {
        let t0 = Instant::now();
        let hit = repo
            .find_by_idempotency_key(conv, alice, &key)
            .await
            .expect("lookup failed");
        samples.push(t0.elapsed());
        assert!(hit.is_some(), "命中路径: 第二次必须查到");
    }
    let p50 = report("A-004", "idempotency key 查重(命中)", samples);
    assert_p50_ceiling("A-004", "hit", p50, 5.0, 3.0);
}

// ============================================================================
// A-005 DM 重复创建幂等 —— 目标 < 20ms (走 dm_pairs PK)
// ============================================================================

const A005_N: usize = 30;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a005_dm_create_idempotent() {
    let (env, alice, bob) = make_env().await;
    let repo = Arc::new(PgConversationRepository::new(pool().await));
    let svc = ConversationService::new(repo);

    // 第一次是真创建; 之后每次都应命中 `find_dm` 的幂等短路(aux-08 语义)。
    svc.create_dm(env, alice, bob)
        .await
        .expect("1st create_dm failed");

    let mut samples = Vec::with_capacity(A005_N);
    let mut first: Option<ConversationId> = None;
    for _ in 0..A005_N {
        let t0 = Instant::now();
        let conv = svc
            .create_dm(env, alice, bob)
            .await
            .expect("重复 create_dm 应幂等成功");
        samples.push(t0.elapsed());
        match first {
            None => first = Some(conv.id),
            Some(id) => assert_eq!(id, conv.id, "重复创建必须返回同一个会话"),
        }
    }
    let p50 = report("A-005", "DM 重复创建(幂等命中)", samples);
    assert_p50_ceiling("A-005", "idempotent-hit", p50, 20.0, 3.0);
}

// ============================================================================
// A-006 消息增量拉取 (after_sequence) —— 目标 < 50ms (50 条/页)
// ============================================================================

const A006_N: usize = 30;
const A006_PAGE: i32 = 50;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a006_incremental_pull() {
    let (env, alice, bob) = make_env().await;
    let conv = make_dm(env, alice, bob).await;
    // aux-06 §B A-006 的目标就是「50 条/页」, 所以按 50 条铺数据。
    seed_messages(conv, alice, 200).await;

    let repo = PgMessageRepository::new(pool().await);
    let mut samples = Vec::with_capacity(A006_N);
    for _ in 0..A006_N {
        let t0 = Instant::now();
        let page = repo
            .list_after_sequence(conv, 100, A006_PAGE)
            .await
            .expect("list_after_sequence failed");
        samples.push(t0.elapsed());
        assert_eq!(page.len(), A006_PAGE as usize, "游标后应恰好还有 50 条");
    }
    let p50 = report("A-006", "增量拉取(50 条/页)", samples);
    assert_p50_ceiling("A-006", "page-50", p50, 50.0, 3.0);
}

// ============================================================================
// A-007 好友列表查询 (含 blocked 过滤) —— 目标 < 50ms (分页 50)
// ============================================================================

const A007_N: usize = 30;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a007_list_friends() {
    let (env, alice, _bob) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);

    // 铺 40 个已接受好友: 让分页真的在做活, 而不是「空表 + LIMIT 50」。
    //
    // ## 必须走 `respond_request(true)`, 不能只改 friend_requests.state
    //
    // `list_friends` 读的是 **`friendships` 表**(`state='accepted'`), 不是
    // `friend_requests`。只有 `respond_request(id, true)` 才会插那两行双向
    // friendships(`relationship/pg.rs` 第 3 步)。
    //
    // 直接 `UPDATE friend_requests SET state='accepted'` 会让 friendships 空着,
    // 于是测到的是**空查询**的延迟 —— 一个漂亮的小数字, 而且毫无意义。
    // 这正是本文件每个用例都断言「对照组非空」要挡的那类假绿。
    let p = pool().await;
    for i in 0..40 {
        let friend: Uuid = sqlx::query_scalar(
            "INSERT INTO users (id, environment_id, kind, display_name) \
             VALUES (gen_random_uuid(), $1, 'user', $2) RETURNING id",
        )
        .bind(env.0)
        .bind(format!("PerfFriend{i}"))
        .fetch_one(&p)
        .await
        .expect("create friend failed");

        let req = repo
            .create_request(env, alice, UserId(friend))
            .await
            .expect("create_request failed");
        repo.respond_request(req.id, true)
            .await
            .expect("respond_request(true) failed —— 否则 friendships 表是空的");
    }

    let mut samples = Vec::with_capacity(A007_N);
    for _ in 0..A007_N {
        let t0 = Instant::now();
        let friends = repo
            .list_friends(alice, None, 50)
            .await
            .expect("list_friends failed");
        samples.push(t0.elapsed());
        assert!(!friends.is_empty(), "对照组非空: 数据集必须是 40 个好友");
    }
    let p50 = report("A-007", "好友列表(分页 50, 40 好友)", samples);
    assert_p50_ceiling("A-007", "list-50", p50, 50.0, 3.0);
}

// ============================================================================
// A-011 好友申请 / 拉黑查询 —— 目标 < 20ms (走 partial index)
// ============================================================================

const A011_N: usize = 50;

#[tokio::test]
#[ignore = "需要真 PG; 由 CI test-integration job 显式运行(见 ci.yml)"]
async fn perf_a011_request_and_block_query() {
    let (env, alice, bob) = make_env().await;
    let repo = PgFriendshipRepository::new(pool().await);

    let p = pool().await;
    for i in 0..30 {
        let sender: Uuid = sqlx::query_scalar(
            "INSERT INTO users (id, environment_id, kind, display_name) \
             VALUES (gen_random_uuid(), $1, 'user', $2) RETURNING id",
        )
        .bind(env.0)
        .bind(format!("Req{i}"))
        .fetch_one(&p)
        .await
        .expect("create sender failed");
        repo.create_request(env, UserId(sender), alice)
            .await
            .expect("create_request failed");
    }
    // ## 拉黑方向: `block` 与 `is_blocked` 的参数**不是同一个语义**
    //
    // `block(user, target)` 写的是 `friendships(user_id=user, friend_id=target)`,
    // 即「user 拉黑了 target」。
    //
    // 而 `is_blocked(user, target)` 查的是
    // `friendships(user_id=target, friend_id=user, state='blocked')`,
    // 即「**target** 拉黑了 **user**」—— 它回答的是「**我**有没有**被**拉黑」,
    // 不是「我拉黑了谁」。
    //
    // 所以想让 `is_blocked(alice, bob)` 为 true, 必须先 `block(bob, alice)`。
    // 我第一版写成 `block(alice, bob)` 然后断言 `is_blocked(alice, bob)` 为 true,
    // 方向反了 —— 本机无 PG 跑不出来, 是读 `relationship/pg.rs` 才发现的。
    repo.block(bob, alice).await.expect("block 失败");

    let mut req_samples = Vec::with_capacity(A011_N);
    let mut blocked_hits = Vec::with_capacity(A011_N);
    let mut blocked_miss = Vec::with_capacity(A011_N);
    for _ in 0..A011_N {
        let t0 = Instant::now();
        let ok = repo
            .is_blocked(alice, bob)
            .await
            .expect("is_blocked failed");
        blocked_hits.push(t0.elapsed());
        assert!(ok, "对照组非空: alice 确实拉黑了 bob");

        let t1 = Instant::now();
        let not = repo
            .is_blocked(bob, alice)
            .await
            .expect("is_blocked failed");
        blocked_miss.push(t1.elapsed());
        assert!(!not, "拉黑是有向的, 反向不应命中");
    }
    // 「收件箱待处理查询」以 A-011 §B 的 partial index 查询形状为准。
    // 这里不新增 service(仓里没有对外暴露的 pending 列表方法),
    // 改用 `find_request` 覆盖单条主键点查, 两者都走 partial/PK 索引。
    let req_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM friend_requests WHERE environment_id = $1 \
         AND recipient_id = $2 AND state = 'pending' LIMIT 1",
    )
    .bind(env.0)
    .bind(alice.0)
    .fetch_one(&p)
    .await
    .expect("取一个 pending 申请 id 失败");
    for _ in 0..A011_N {
        let t0 = Instant::now();
        let got = repo
            .find_request(req_id)
            .await
            .expect("find_request failed");
        req_samples.push(t0.elapsed());
        assert!(got.is_some());
    }

    let p50 = report("A-011", "拉黑查询(命中)", blocked_hits);
    assert_p50_ceiling("A-011", "block-hit", p50, 20.0, 3.0);
    let p50 = report("A-011", "拉黑查询(未命中)", blocked_miss);
    assert_p50_ceiling("A-011", "block-miss", p50, 20.0, 3.0);
    let p50 = report("A-011", "申请按主键点查", req_samples);
    assert_p50_ceiling("A-011", "request-by-id", p50, 20.0, 3.0);
}
