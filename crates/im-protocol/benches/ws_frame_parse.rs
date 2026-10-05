//! A-008 WS 帧解析 —— 量化 serde **内部标记(internally-tagged)** 的代价
//!
//! 依据 `aux-06 §B A-008 WS 帧路由` 的 P99 目标 `< 1ms`, 以及 §E「测量优先:
//! 不优化未测量的代码」。
//!
//! ## 这次量的是**什么**
//!
//! WS 每一条消息都要走一遍 `serde_json::from_slice::<ClientFrame>()`。而本仓的
//! 帧类型是**两层**内部标记:
//!
//! ```ignore
//! #[serde(tag = "type")]  enum ClientFrame { ... }
//! #[serde(tag = "kind")]  enum MessageContent { ... }   // 嵌在 SendMessage.content
//! ```
//!
//! serde 的内部标记要求**先把整个输入缓冲成 `Content` 树再按 tag 分派**
//! (对比外部标记只需向前扫描到第一个 `{`)。两层叠加时每条消息被解析两遍。
//!
//! 这是 **serde 生态里一个已知的性能特征**, 不是 IM1.0 独有的问题。但本仓
//! 到底为此付了多少钱, 在建这套基准之前**没有人知道** —— 而 aux-06 §D 的
//! 13 项「实测」栏全是"(待测)"。本文件就是去把那个数字填出来。
//!
//! ## 三个用例分别量到了什么(以及**没**量到什么)
//!
//! | 用例 | 标记层数 | 载荷 | 实测均值 |
//! |---|---|---|---|
//! | `ping` | 1 (`type`) | 1 个整数 | ~104 ns |
//! | `message_content` | 1 (`kind`) | 1 段文本 | ~127 ns |
//! | `send_message` | 2 (`type`+`kind`) | 3 个 UUID + 文本 | ~727 ns |
//!
//! ### ⚠️ **不要**把 `send_message - message_content` 当成「第二层标记的代价」
//!
//! 这三个用例的**载荷不同**, 所以差值里混着载荷差异, 不是纯粹的标记层数差异
//! (`send_message` 多了 3 个 UUID 的解析 + 更长的文本)。真要隔离标记层数,
//! 得让两组的**载荷完全一致**、只变枚举嵌套层数 —— 那是另一个基准, 本次没做。
//!
//! 可以据以下结论的是**绝对量级**: aux-06 A-008 的 P99 目标是 `< 1ms`
//! (1 000 000 ns), 而主路径实测 ~727 ns, **差约 3 个数量级**。故
//! 「双层内部标记」在本仓不是瓶颈, **不需要**为它改设计 —— 这正是 §E
//! 「不优化未测量的代码」要避免的反面: 先测, 发现不痛, 就别动。
//!
//! ## 边界: **只测解析, 不测路由**
//!
//! aux-06 A-008 的完整算法是「帧路由 + 成员过滤」, 后半段在
//! `crates/im-gateway/src/ws/handler.rs`。而 `im-gateway` 是 **bin-only
//! crate**(无 `[lib]` target), criterion 的 bench 只能挂在 lib 上, 所以
//! **路由那半段本次测不了**。这个缺口已记入 `docs/gap-ledger.md`, 不在这里
//! 假装它被覆盖了。

use criterion::{criterion_group, criterion_main, Criterion};
use im_protocol::content::MessageContent;
use im_protocol::ws_frames::ClientFrame;
use uuid::Uuid;

const PING: &str = r#"{"type":"ping","ts":42}"#;

fn send_message_json() -> String {
    // 字段与 aux-13 §1.1.2 / §4.1 对齐: type / req_id / conversation_id /
    // idempotency_key / kind / content / reply_to
    format!(
        r#"{{"type":"send_message","req_id":"{}","conversation_id":"{}","idempotency_key":"{}","kind":"text","content":{{"kind":"text","text":"{}"}},"reply_to":null}}"#,
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "这是一条用于基准测试的消息文本,长度刻意取接近 aux-13 §4.1 的 4000 字上限之下。"
    )
}

fn content_json() -> String {
    format!(
        r#"{{"kind":"text","text":"{}"}}"#,
        "这是一条用于基准测试的消息文本,长度刻意取接近 aux-13 §4.1 的 4000 字上限之下。"
    )
}

fn bench_parse(c: &mut Criterion) {
    // 1 层标记 + 极小载荷 —— 单层内部标记的下界
    c.bench_function("a008/ping_parse_1_tag", |b| {
        b.iter(|| {
            let f: ClientFrame = serde_json::from_str(PING).expect("ping 帧应能解析");
            // black_box 防止把整个解析优化掉: 结果必须被「用」到
            std::hint::black_box(&f);
        })
    });

    // 2 层标记 + 真实主路径
    let sm = send_message_json();
    c.bench_function("a008/send_message_parse_2_tags", |b| {
        b.iter(|| {
            let f: ClientFrame = serde_json::from_str(&sm).expect("send_message 帧应能解析");
            std::hint::black_box(&f);
        })
    });

    // 单独量第二层(content 自己也是内部标记枚举)
    let ct = content_json();
    c.bench_function("a008/message_content_parse_1_tag", |b| {
        b.iter(|| {
            let m: MessageContent = serde_json::from_str(&ct).expect("content 应能解析");
            std::hint::black_box(&m);
        })
    });
}

// ⚠️ 本文件**没有** `#[test]` 反例守卫, 这是**故意的**
//
// 2026-10-06 曾在这里写过 `fixtures_are_real_frames_not_error_paths`。它
// **从不执行**: 本 target 声明了 `harness = false`(criterion 必需), 而 cargo
// 对 `harness = false` 的目标不做 libtest 集成 —— 实测 `cargo test
// -p im-protocol --benches` 的输出里根本没有它的名字。
//
// 「写了守卫但它永远不跑」比「没有守卫」更坏: 后者让人知道缺什么, 前者
// 让人以为已经有了。守卫已搬到 `crates/im-protocol/tests/bench_fixture_guards.rs`,
// 那里的会随 `cargo test --workspace` 进 CI。
//
// 夹具合法性另有一层**运行期**保障: 三个 bench 循环体内都是
// `.expect("...帧应能解析")`, 夹具若坏了, 基准会直接 panic, 而不是安静地
// 量出一个漂亮但无意义的数字。
//
// (注: 这段用 `//` 而非 `///` —— 它紧邻 `criterion_group!`, 宏调用上的
//  doc comment 会触发 `unused_doc_comment`。)

criterion_group!(benches, bench_parse);
criterion_main!(benches);
