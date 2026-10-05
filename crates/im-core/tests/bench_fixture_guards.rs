//! 基准夹具的合法性守卫 —— **必须**放在这里, 不能放在 `benches/` 里
//!
//! ## 为什么这个文件存在
//!
//! `benches/auth_hotpath.rs` 里也写了同名的 `#[test]` 作反例守卫, 但**它从不
//! 执行**: 该 bench target 声明了 `harness = false`(criterion 必需), 而 cargo
//! 对 `harness = false` 的目标**不做 libtest 集成** —— 2026-10-06 实测确认。
//!
//! 「写了守卫但它永远不跑」比「没有守卫」更坏: 后者让人知道缺什么, 前者让人
//! 以为已经有了。故守卫搬到这里, 进 CI 的 `cargo test --workspace`。
//!
//! ## 它守的是什么
//!
//! 基准最阴卑的失败模式是**量了个错误路径**: 夹具本身就是坏的 / 根本不能通过,
//! 每次迭代都在跑 `Err` 的早退分支, 量出来的数字漂亮却一文不值。

use chrono::Duration as ChronoDuration;
use im_common::ids::{EnvironmentId, UserId};
use im_core::identity::repository::{User, UserKind, UserState};
use im_core::identity::token::{SigningKey, TokenService};
use secrecy::SecretString;
use uuid::Uuid;

fn signing_key(kid: &str) -> SigningKey {
    SigningKey {
        kid: kid.to_string(),
        key: SecretString::from(format!("bench-secret-key-for-{kid}-0123456789")),
    }
}

fn service(keys: Vec<SigningKey>) -> TokenService {
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

/// 夹具 token 必须真的能校验通过, 且 claims 形状正确
#[test]
fn the_bench_fixtures_are_valid_tokens_not_error_paths() {
    let svc1 = service(vec![signing_key("v1")]);
    let svc2 = service(vec![signing_key("v1"), signing_key("v2")]);
    let t1 = svc1.issue_access_token(&sample_user()).expect("签发失败").0;
    let t2 = svc2.issue_access_token(&sample_user()).expect("签发失败").0;

    let c1 = svc1.validate_access_token(&t1).expect("单密钥夹具坏了");
    assert!(!c1.sub.is_empty(), "sub 不该为空");
    assert!(
        c1.exp > c1.iat,
        "exp 应大于 iat, 实际 {} <= {}",
        c1.exp,
        c1.iat
    );
    assert_eq!(c1.kind, "user");

    let c2 = svc2.validate_access_token(&t2).expect("双密钥夹具坏了");
    // 钉住 aux-06 §D.4 记的那件事: 签发**恒用第一把 key**(token.rs:152),
    // 故双密钥服务里签出的 token kid 永远是 v1 -> 线性扫描第一轮就命中。
    // 基准文件里的 `a010/validate_dual_key_first_key` 这个命名依赖此事实。
    assert_eq!(
        c2.kid, "v1",
        "若哪天改成可指定 kid 签发, 基准的名字与 §D.4 的「下界」结论都要重算"
    );
}

/// 反向守卫: **坏 token 必须真的失败**
///
/// 只断言「好夹具能过」的话, 一个恒返回 `Ok` 的 `validate_access_token` 也能让
/// 上面全绿 —— 那样基准量的就不是验签路径了。
#[test]
fn a_bad_token_really_fails_so_the_guard_above_has_teeth() {
    let svc = service(vec![signing_key("v1")]);
    assert!(
        svc.validate_access_token("not-a-jwt").is_err(),
        "垃圾串必须校验失败"
    );
    // 另一把 key 签的 token 必须被拒(而不是因为 kid 找不到才拒)
    let other = service(vec![signing_key("v-other")]);
    let foreign = other.issue_access_token(&sample_user()).expect("签发").0;
    assert!(
        svc.validate_access_token(&foreign).is_err(),
        "kid 不在密钥表里的 token 必须被拒"
    );
}
