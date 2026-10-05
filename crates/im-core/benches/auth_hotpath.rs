//! A-001 / A-010 —— Access Token 签发与校验, 以及「5s 缓存」到底值多少
//!
//! 依据 `aux-06 §B A-001`(P99 `< 5ms`)、`A-010 双密钥 JWT`(`< 5ms`)、
//! 以及 §E「测量优先: 不优化未测量的代码」。
//!
//! ## 为什么专门量「缓存」这个并不存在的实现
//!
//! `aux-06` A-001 的「优化策略」第 1 条写的是:
//!
//! > Auth_Middleware **5s 内存缓存**(已纳入 `ImplementationSpec §7.5` 设计)
//!
//! 2026-10-06 逐条核对后:
//!
//! - `ImplementationSpec.md` 里**搜不到** `Auth_Middleware` 或 `5s 缓存` ——
//!   「已纳入 §7.5 设计」这句**不成立**;
//! - `crates/im-gateway/src/http/auth.rs` 的 `AuthedUser` extractor
//!   **每个请求都完整跑一次 `validate_access_token`**, 无任何缓存。
//!
//! 也就是说这是一条**规范点名、但没落地**的优化。它没落地**可能是有道理的**:
//! 缓存意味着「登出/吊销后最多 5s 内 token 仍然有效」, 而
//! `POST /v1/auth/logout` 是真的会吊销 device session 的。
//!
//! 那要不要做, 取决于它值多少钱 —— 而 aux-06 §D 里 A-001/A-010 的「实测」栏
//! 从头到尾都是"(待测)"。**本文件就是为了把那个数字填出来**: 校验一次多少
//! 纳秒, 缓存命中一次多少纳秒, 两者差多少。安全语义的取舍由人决定, 但
//! 取舍的**代价**不该是「未知」。
//!
//! ## 本文件**不**做的事
//!
//! 不实现那个缓存。它涉及吊销语义, 属安全决策, 不是性能基准该顺手带上的。

use std::collections::HashMap;
use std::hint::black_box;

use chrono::Duration as ChronoDuration;
use criterion::{criterion_group, criterion_main, Criterion};
use im_common::ids::{EnvironmentId, UserId};
use im_core::identity::repository::{User, UserKind, UserState};
use im_core::identity::token::{SigningKey, TokenClaims, TokenService};
use secrecy::SecretString;
use uuid::Uuid;

/// 32 字节密钥(HS256 的常见长度)。刻意**不**用更短的值: 密钥长度本身会
/// 影响 HMAC 的常数因子, 用一个不像生产的短密钥会让整张表失真。
fn signing_key(kid: &str) -> SigningKey {
    SigningKey {
        kid: kid.to_string(),
        key: SecretString::from(format!("bench-secret-key-for-{kid}-0123456789")),
    }
}

fn service(keys: Vec<SigningKey>) -> TokenService {
    // 15min = 900s, 与 `app_config` 的 `access_token_ttl_seconds` 默认一致
    TokenService::new(
        keys,
        ChronoDuration::seconds(900),
        SecretString::from("bench-refresh-pepper".to_string()),
    )
}

fn sample_user() -> User {
    User {
        id: UserId(Uuid::new_v4()),
        environment_id: EnvironmentId(Uuid::new_v4()),
        kind: UserKind::User,
        external_identity: None,
        state: UserState::Active,
        display_name: Some("bench".into()),
        username: Some("bench_user".into()),
        password_hash: Some("$argon2id$v=19$m=19456,t=2,p=1$bench$bench".into()),
        created_at: chrono::Utc::now(),
    }
}

fn bench_auth(c: &mut Criterion) {
    // ---- A-001: 单密钥(常规态) ----
    let svc1 = service(vec![signing_key("v1")]);
    let token1 = svc1
        .issue_access_token(&sample_user())
        .expect("签发应成功")
        .0;

    c.bench_function("a001/validate_single_key", |b| {
        b.iter(|| {
            let claims = svc1
                .validate_access_token(black_box(&token1))
                .expect("校验应成功");
            black_box(&claims);
        })
    });

    // ---- A-010: 双密钥轮换, token 由第一把签发 ----
    //
    // ⚠️ **这不是最坏情况**, 是最好情况。`issue_access_token` 恒用
    // `signing_keys[0]`(`token.rs:152`), 所以本仓签出来的 token `kid`
    // **永远是 v1**, 线性扫描第一轮就命中。
    //
    // 真正的最坏情况(轮换期**仍在用 v1 签发**的旧 token, 但线上 `IM_JWT_
    // SIGNING_KEYS` 已被换成 `["v2","v1"]` 顺序) 走不到第 2 轮 —— 那种
    // token 必须由外部签发, 当前 API 造不出来。故本用例给出的是双密钥场景的
    // **下界**, 真实上界还要加上一次 `SigningKey` 克隆与一次额外比较
    // (量级在几十 ns, 不改变结论)。见下面 `fixtures_are_valid_tokens...`
    // 里的断言 —— 那条断言是发现我这个标注错误的地方。
    let svc2 = service(vec![signing_key("v1"), signing_key("v2")]);
    let token2 = svc2
        .issue_access_token(&sample_user())
        .expect("签发应成功")
        .0;

    c.bench_function("a010/validate_dual_key_first_key", |b| {
        b.iter(|| {
            let claims = svc2
                .validate_access_token(black_box(&token2))
                .expect("校验应成功");
            black_box(&claims);
        })
    });

    // ---- 签发, 作为同一算法的另一侧 ----
    let user = sample_user();
    c.bench_function("a001/issue_access_token", |b| {
        b.iter(|| {
            let t = svc1.issue_access_token(black_box(&user)).expect("签发");
            black_box(&t);
        })
    });

    // ---- 「5s 缓存命中」的下界 ----
    //
    // 这是**若实现** A-001 优化策略第 1 条, 每次请求能省掉的那部分。
    // 这里只量最朴素的形态: `HashMap<String, TokenClaims>` 的一次 get。
    // 真实实现还需要校验 TTL(拿 `Instant` 比一下), 只会比这个略慢, 不会更慢
    // 一个数量级 —— 所以这个数是**乐观下界**, 当作上限看。
    let mut cache: HashMap<String, TokenClaims> = HashMap::new();
    cache.insert(
        token1.clone(),
        svc1.validate_access_token(&token1).expect("校验应成功"),
    );
    c.bench_function("a001/cache_hit_lower_bound", |b| {
        b.iter(|| {
            let claims = cache.get(black_box(&token1)).expect("缓存应有该 token");
            black_box(claims);
        })
    });
}

// ⚠️ 本文件**没有** `#[test]` 反例守卫, 这是**故意的**
//
// 2026-10-06 曾在这里写过 `fixtures_are_valid_tokens_not_error_paths`。它
// **从不执行**: 本 target 声明了 `harness = false`(criterion 必需), 而 cargo
// 对 `harness = false` 的目标不做 libtest 集成 —— 实测确认过。
//
// 「写了守卫但它永远不跑」比「没有守卫」更坏。守卫已搬到
// `crates/im-core/tests/bench_fixture_guards.rs`, 那里的会随
// `cargo test --workspace` 进 CI, 且它顺带钉住了「签发恒用第一把 key」
// 这件本文件基准命名所依赖的事(见上)。
//
// 夹具合法性另有一层**运行期**保障: bench 循环体内是
// `.expect("校验应成功")`, 夹具若坏了, 基准会直接 panic。
//
// (注: 这段用 `//` 而非 `///` —— 它紧邻 `criterion_group!`, 宏调用上的
//  doc comment 会触发 `unused_doc_comment`。)

criterion_group!(benches, bench_auth);
criterion_main!(benches);
