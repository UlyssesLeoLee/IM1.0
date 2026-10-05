//! 基准夹具的合法性守卫 —— **必须**放在这里, 不能放在 `benches/` 里
//!
//! ## 为什么这个文件存在
//!
//! `benches/ws_frame_parse.rs` 里也写了同名的 `#[test]` 作反例守卫, 但**它
//! 从不执行**: 该 bench target 声明了 `harness = false`(criterion 必需), 而
//! cargo 对 `harness = false` 的目标**不做 libtest 集成** —— 2026-10-06 实测:
//! `cargo test -p im-protocol --benches` 的输出里找不到任何 `fixtures_are`
//! 名字, `cargo test -p im-protocol` 更连 bench 二进制都不构建。
//!
//! 「写了守卫但它永远不跑」比「没有守卫」更坏: 后者让人知道缺什么, 前者
//! 让人以为已经有了。所以守卫搬到本文件, 进 CI 的 `cargo test --workspace`。
//!
//! ## 它守的是什么
//!
//! 基准最阴卑的失败模式是**量了个错误路径**: 夹具本来就是坏的, 每次迭代都在
//! 跑 `Err` 的早退分支, 量出来的数字漂亮却一文不值。这里把三条夹具真的解析
//! 一次, 并断言落到**预期的变体与字段**上。

use im_protocol::content::MessageContent;
use im_protocol::ws_frames::ClientFrame;
use uuid::Uuid;

/// 与 `benches/ws_frame_parse.rs` 的 `PING` 逐字一致
const PING: &str = r#"{"type":"ping","ts":42}"#;

/// 与 `benches/ws_frame_parse.rs` 的 `send_message_json()` 字段集一致
fn send_message_json() -> String {
    format!(
        r#"{{"type":"send_message","req_id":"{}","conversation_id":"{}","idempotency_key":"{}","kind":"text","content":{{"kind":"text","text":"{}"}},"reply_to":null}}"#,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "这是一条用于基准测试的消息文本。"
    )
}

/// 与 `benches/ws_frame_parse.rs` 的 `content_json()` 一致
fn content_json() -> String {
    r#"{"kind":"text","text":"这是一条用于基准测试的消息文本。"}"#.to_string()
}

#[test]
fn ping_fixture_parses_to_the_ping_variant() {
    let f: ClientFrame = serde_json::from_str(PING).expect("ping 夹具坏了");
    assert!(
        matches!(f, ClientFrame::Ping { ts: Some(42) }),
        "ping 夹具应落到 Ping{{ts:Some(42)}}, 实际 {f:?}"
    );
}

#[test]
fn send_message_fixture_parses_through_both_internal_tags() {
    let f: ClientFrame = serde_json::from_str(&send_message_json()).expect("send_message 夹具坏了");
    match f {
        ClientFrame::SendMessage { kind, content, .. } => {
            assert_eq!(kind, "text", "外层 type 标记应分派到 SendMessage");
            assert!(
                matches!(content, MessageContent::Text { .. }),
                "内层 kind 标记应分派到 Text, 实际 {content:?}"
            );
        }
        other => panic!("send_message 夹具解析成了 {other:?}"),
    }
}

#[test]
fn content_fixture_parses_to_the_text_variant() {
    let m: MessageContent = serde_json::from_str(&content_json()).expect("content 夹具坏了");
    match m {
        MessageContent::Text { text } => assert!(!text.is_empty(), "text 不该是空串"),
        other => panic!("content 夹具解析成了 {other:?}"),
    }
}

/// 反向守卫: **坏夹具必须真的失败**
///
/// 只断言「好夹具能过」的话, 一个恒返回 `Err` 的 `from_str` 也能让上面三条全绿
/// —— 那样基准量的是错误路径, 而守卫在说「没问题」。故这里证明错误路径
/// **确实**会走到 `Err`。
#[test]
fn a_malformed_frame_really_fails_so_the_guards_above_have_teeth() {
    assert!(
        serde_json::from_str::<ClientFrame>(r#"{"type":"no_such_frame"}"#).is_err(),
        "未知 type 必须解析失败。若这条失败, 说明 from_str 变成了恒 Ok, \
         上面的守卫就全是空转"
    );
    assert!(
        serde_json::from_str::<ClientFrame>(r#"{"nope":1}"#).is_err(),
        "缺 type 标记必须解析失败"
    );
}
