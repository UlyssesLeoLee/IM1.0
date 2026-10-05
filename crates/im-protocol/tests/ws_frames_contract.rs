//! AsyncAPI ↔ serde 运行时契约: 真序列化每一类帧, 对拍规范的 `required`
//!
//! 依据: `docs/api/asyncapi.json` 自称「每个字段的必填性都取自 serde 派生」。
//! `scripts/check-asyncapi.ps1` 从 Rust **源码文本**推导 `skip_serializing_if`
//! 属性并据此核对 `required` —— 那是推断。本文件是**运行期**的同一断言:
//! 把每个变体用「所有 Option 字段都取 None」构造出来, 真跑一次 `to_string`,
//! 然后要求出现的 key 集合**恰好**等于规范声明的 `required` 集合。
//!
//! ## 为什么静态门禁不够
//!
//! 门禁读到 `#[serde(default)]` 就能推出「没有 skip, 所以恒序列化」。那是一条
//! **关于 serde 行为的断言**, 不是对本仓 serde 实际行为的观测。它在三种情况
//! 下会静默失效:
//!
//! 1. 属性拼写写错 / 挂在相邻字段上 —— 文本匹配照样「有 skip」, 实际没有
//! 2. 将来换成 `skip_serializing_if = "Vec::is_empty"` 之类的自定义谓词
//! 3. 有人给某个变体加字段时, 规范与 Rust 都改了, 但语义已经变了
//!
//! 真序列化不依赖任何关于 serde 语义的推理: 字段出现与否, 就是出现了与否。
//!
//! ## 为什么必须是「恰好相等」而不是「包含」
//!
//! 只断言「required 里的字段都出现了」会漏掉一半: 某个字段若**本该**带
//! `skip_serializing_if` 却没有, 它会以 `null` 出现, 而规范没把它列进
//! required —— 包含关系照样通过, 但规范正是在描述一个错误形状。
//! 故要求 `actual_keys == required ∪ {tag}`, 双向都不能多也不能少。
//!
//! ## 不复用 im-protocol 现有的 roundtrip 用例
//!
//! `ws_frames.rs` 里的用例断言的是「`"type":"ack"` 出现在字符串里」这类
//! **子串**性质。本文件断言的是 **key 集合的精确相等**。子串断言对
//! 「多了个字段」完全免疫 —— 而多字段正是这里要抓的。

use std::collections::BTreeSet;

use serde_json::{json, Value};
use uuid::Uuid;

use im_protocol::content::MessageContent;
use im_protocol::error_body::{ErrorBody, FieldError};
use im_protocol::ws_frames::{AckData, ClientFrame, ServerFrame, WireMessage};

/// 仓内 AsyncAPI 3.0 规范(3 层上溯 = 仓根)。
const SPEC_JSON: &str = include_str!("../../../docs/api/asyncapi.json");

fn spec() -> Value {
    serde_json::from_str(SPEC_JSON).expect("docs/api/asyncapi.json 必须是合法 JSON")
}

/// 某一侧的 (wire 取值 -> schema) 映射。
///
/// 刻意**按方向分三张表**而不是一张全局表: `typing` 在上下行都存在
/// (`ClientFrame::Typing` 与 `ServerFrame::Typing` 的 `properties.type.const`
/// 都是 `"typing"`), 单张表里一条会静默覆盖另一条。本测试的第一版就是这么
/// 写的, 于是 `ServerFrame::Typing` 被判成「对应 2 个 schema」。
/// `scripts/check-asyncapi.ps1` 独立地踩了同一个坑并已修复 —— 同一处碰撞在
/// 两个实现里各犯一次, 说明它是真实存在的, 不是某个语言的怪癖。
///
/// 上下行两张表取自 operation 的 message 列表(而非按 schema 名猜), 所以
/// 「被文档登记」与「被 operation 引用」是同一件事。
/// `MessageContent` 不在任何 operation 里, 单独按 `kind` + schema 名前缀取。
fn discriminator_table(side: &str) -> std::collections::BTreeMap<String, Value> {
    let doc = spec();
    let mut out = std::collections::BTreeMap::new();
    let operation = match side {
        "client" => doc.pointer("/operations/clientToServer").cloned(),
        "server" => doc.pointer("/operations/serverToClient").cloned(),
        "content" => None,
        other => panic!("unknown side {other:?}"),
    };
    match operation {
        Some(op) => {
            let msgs = op
                .get("messages")
                .and_then(Value::as_array)
                .expect("operation 必须有 messages");
            for mr in msgs {
                let mname = mr
                    .get("$ref")
                    .and_then(Value::as_str)
                    .and_then(|r| r.rsplit('/').next())
                    .expect("message ref 必须有 $ref");
                let payload = doc
                    .pointer(&format!("/components/messages/{mname}/payload/$ref"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let sname = payload.rsplit('/').next().unwrap_or_default().to_string();
                let sc = match doc.pointer(&format!("/components/schemas/{sname}")) {
                    Some(s) => s.clone(),
                    None => continue,
                };
                if let Some(c) = sc.pointer("/properties/type/const").and_then(Value::as_str) {
                    out.insert(c.to_string(), sc);
                }
            }
        }
        None => {
            let schemas = doc
                .pointer("/components/schemas")
                .and_then(Value::as_object)
                .expect("规范必须含 components.schemas");
            for (name, sc) in schemas {
                if !name.starts_with("MessageContent") {
                    continue;
                }
                if let Some(c) = sc.pointer("/properties/kind/const").and_then(Value::as_str) {
                    out.insert(c.to_string(), sc.clone());
                }
            }
        }
    }
    out
}

/// 在**指定方向内**按判别常量找 payload schema。
///
/// 按 tag 取值而不是按 schema 名, 是因为 spec 里 schema 名是文档作者的命名
/// 决定, 而 tag 取值才是 wire 事实 —— 断言必须钉在 wire 事实上。
fn schema_by_discriminator(side: &str, value: &str) -> Value {
    let table = discriminator_table(side);
    match table.get(value) {
        Some(sc) => sc.clone(),
        None => panic!(
            "[{side}] 没有判别取值 {value:?} 的 schema。该方向的表里只有: {:?}",
            table.keys().collect::<Vec<_>>()
        ),
    }
}

fn schema_property_names(sc: &Value) -> BTreeSet<String> {
    sc.get("properties")
        .and_then(Value::as_object)
        .expect("payload schema 必须有 properties")
        .keys()
        .cloned()
        .collect()
}

fn schema_required(sc: &Value) -> BTreeSet<String> {
    sc.get("required")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn actual_keys(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .expect("序列化结果必须是 JSON 对象")
        .keys()
        .cloned()
        .collect()
}

/// 核心断言: 「所有 Option 取 None」的帧, 其 key 集合必须**恰好**是
/// 规范声明的 `required` ∪ {tag}。
#[track_caller]
fn assert_all_none_frame_matches_required(
    label: &str,
    side: &str,
    tag: &str,
    wire: &str,
    frame: &Value,
) {
    let sc = schema_by_discriminator(side, wire);
    let actual = actual_keys(frame);
    let mut expected = schema_required(&sc);
    expected.insert(tag.to_string());

    assert_eq!(
        actual,
        expected,
        "[{label}] 所有 Option 字段取 None 时, 序列化出的 key 集合与规范 `required` 不一致。\n\
         实际: {actual:?}\n\
         规范: {expected:?}\n\
         多出来的 key = 该字段本该带 skip_serializing_if(空值时整个消失)却恒出现了;\n\
         少掉的 key   = 该字段被规范标成必填, 但实际在 None 时根本不出现。\n\
         规范 schema 的 required: {:?}",
        schema_required(&sc)
    );

    // 反向: 规范里列出的**可选**属性, 在本帧里绝不能出现。
    // (上面的集合相等已蕴含这一点, 但单独断言一次, 失败信息更直指「反例」。)
    let optional = schema_property_names(&sc)
        .difference(&expected)
        .cloned()
        .collect::<BTreeSet<_>>();
    let leaked: Vec<&String> = optional.intersection(&actual).collect();
    assert!(
        leaked.is_empty(),
        "[{label}] 规范标为可选的属性却出现在帧里: {leaked:?}\n\
         这说明规范把它写错了 —— 服务端会发, 规范却说它可以不出现。"
    );

    // 顺带锁住 wire 取值本身(snake_case 转换)。
    assert_eq!(
        frame.get(tag).and_then(Value::as_str),
        Some(wire),
        "[{label}] 判别字段 {tag} 必须是 {wire:?}"
    );
}

// ---------------------------------------------------------------------------
// ClientFrame: 8 类
// ---------------------------------------------------------------------------

#[test]
fn every_client_frame_with_none_options_matches_the_spec() {
    let id = Uuid::nil();

    let cases: Vec<(&str, &str, Value)> = vec![
        // `auth` **不在**本表里 —— 见下方
        // `first_frame_auth_is_not_the_im_protocol_variant` 单独说明:
        // 规范里的 ClientAuthFrame 描述的是 im-gateway 私有的 AuthFrame,
        // 不是这个枚举, 所以它不能靠序列化本枚举来验证。
        (
            "ClientFrame::SendMessage",
            "send_message",
            serde_json::to_value(ClientFrame::SendMessage {
                req_id: id,
                conversation_id: id,
                idempotency_key: id,
                kind: "text".into(),
                content: MessageContent::Text { text: "hi".into() },
                reply_to: None,
            })
            .unwrap(),
        ),
        (
            "ClientFrame::EditMessage",
            "edit_message",
            serde_json::to_value(ClientFrame::EditMessage {
                req_id: id,
                message_id: id,
                content: MessageContent::Text { text: "hi".into() },
            })
            .unwrap(),
        ),
        (
            "ClientFrame::RecallMessage",
            "recall_message",
            serde_json::to_value(ClientFrame::RecallMessage {
                req_id: id,
                message_id: id,
            })
            .unwrap(),
        ),
        (
            "ClientFrame::React",
            "react",
            serde_json::to_value(ClientFrame::React {
                req_id: id,
                message_id: id,
                emoji: "👍".into(),
            })
            .unwrap(),
        ),
        (
            "ClientFrame::MarkRead",
            "mark_read",
            serde_json::to_value(ClientFrame::MarkRead {
                req_id: id,
                conversation_id: id,
                sequence: 1,
            })
            .unwrap(),
        ),
        (
            "ClientFrame::Typing",
            "typing",
            serde_json::to_value(ClientFrame::Typing {
                req_id: id,
                conversation_id: id,
            })
            .unwrap(),
        ),
        (
            "ClientFrame::Ping",
            "ping",
            serde_json::to_value(ClientFrame::Ping { ts: None }).unwrap(),
        ),
    ];

    assert_eq!(
        cases.len(),
        7,
        "本表覆盖 ClientFrame 的 7 个**可由本枚举验证**的变体(auth 另有一条用例)。\
         数量对不上说明本测试漏了或多了某个变体。"
    );
    for (label, wire, v) in cases {
        assert_all_none_frame_matches_required(label, "client", "type", wire, &v);
    }
}

/// 首帧 `auth` 的形状**不由 `im_protocol::ClientFrame::Auth` 决定**, 所以它
/// 单独一条用例, 且断言的是「两个类型确实不同」这个事实本身。
///
/// ## 背景
///
/// 规范把 `ClientAuthFrame.req_id` 标成**可选**, 而本枚举里它是
/// `req_id: Uuid`(非 Option), 序列化时恒出现。第一版把 `auth` 混在统一表里
/// 跑, 于是本测试红了 —— 报「规范少要求了一个字段」。
///
/// 那不是规范错, 是**验证对象选错了**: 生产路径的 `handler.rs:331` 走私有的
/// `AuthFrame`(Deserialize-only, `req_id` 是 `#[serde(default)] Option<Uuid>`),
/// 从不用 `im_protocol::ClientFrame::Auth` 解析首帧。
///
/// ## 为什么不能简单豁免
///
/// 豁免掉就等于「不再验证 auth 的任何东西」。故这里反过来把差异**锁死**:
/// ① 本枚举确实恒发 `req_id`; ② 规范确实标它可选; ③ 规范里必须写明原因。
/// 三条任一被改动而另一方没跟上, 本用例就红。
#[test]
fn first_frame_auth_is_not_the_im_protocol_variant() {
    // ① im_protocol 的枚举: req_id 是非 Option, 恒出现
    let v = serde_json::to_value(ClientFrame::Auth {
        req_id: Uuid::nil(),
        access_token: "t".into(),
    })
    .unwrap();
    assert_eq!(
        v.as_object().unwrap().len(),
        3,
        "ClientFrame::Auth 恒有 3 个 key(type/req_id/access_token): {v}"
    );
    assert!(
        v.get("req_id").is_some(),
        "本枚举的 req_id 非 Option, 必然出现: {v}"
    );

    // ② 规范: req_id 可选
    let sc = schema_by_discriminator("client", "auth");
    let required = schema_required(&sc);
    let properties = schema_property_names(&sc);
    assert!(
        !required.contains("req_id"),
        "规范把首帧 req_id 标成可选, 因为生产路径走 handler.rs 的 AuthFrame"
    );
    assert!(
        properties.contains("req_id"),
        "req_id 仍必须被记录为属性(可选不等于不存在)"
    );
    assert!(
        properties.contains("access_token") && required.contains("access_token"),
        "access_token 必填且必被记录"
    );

    // ③ 规范必须把原因写进文档, 否则「为什么可选」就丢失了
    let doc = spec();
    let msg_desc = doc
        .pointer("/components/messages/clientAuth/description")
        .and_then(Value::as_str)
        .expect("clientAuth message 必须有 description");
    assert!(
        msg_desc.contains("AuthFrame"),
        "clientAuth 的 description 必须点名真正的解析类型 AuthFrame, \
         理由: 读者会以为规范描述的是 im_protocol::ClientFrame::Auth。实际: {msg_desc}"
    );
}

// ---------------------------------------------------------------------------
// ServerFrame: 10 类
// ---------------------------------------------------------------------------

#[test]
fn every_server_frame_with_none_options_matches_the_spec() {
    let id = Uuid::nil();
    let now = chrono::Utc::now();

    let cases: Vec<(&str, &str, Value)> = vec![
        (
            "ServerFrame::Connected",
            "connected",
            serde_json::to_value(ServerFrame::Connected { session_id: id }).unwrap(),
        ),
        (
            "ServerFrame::Ack",
            "ack",
            serde_json::to_value(ServerFrame::Ack {
                req_id: id,
                ok: false,
                data: None,
                error: None,
            })
            .unwrap(),
        ),
        (
            "ServerFrame::MessageNew",
            "message_new",
            serde_json::to_value(ServerFrame::MessageNew {
                message: WireMessage {
                    id,
                    conversation_id: id,
                    sequence: 1,
                    sender_id: None,
                    kind: "text".into(),
                    content: MessageContent::Text { text: "hi".into() },
                    reply_to: None,
                    state: "sent".into(),
                    created_at: now,
                    edited_at: None,
                    reactions: Vec::new(),
                },
            })
            .unwrap(),
        ),
        (
            "ServerFrame::MessageEdited",
            "message_edited",
            serde_json::to_value(ServerFrame::MessageEdited {
                message_id: id,
                content: MessageContent::Text { text: "hi".into() },
                edited_at: now,
            })
            .unwrap(),
        ),
        (
            "ServerFrame::MessageRecalled",
            "message_recalled",
            serde_json::to_value(ServerFrame::MessageRecalled {
                message_id: id,
                conversation_id: id,
            })
            .unwrap(),
        ),
        (
            "ServerFrame::ReactionAdded",
            "reaction_added",
            serde_json::to_value(ServerFrame::ReactionAdded {
                message_id: id,
                user_id: id,
                emoji: "👍".into(),
            })
            .unwrap(),
        ),
        (
            "ServerFrame::PresenceUpdate",
            "presence_update",
            serde_json::to_value(ServerFrame::PresenceUpdate {
                user_id: id,
                status: "online".into(),
            })
            .unwrap(),
        ),
        (
            "ServerFrame::Typing",
            "typing",
            serde_json::to_value(ServerFrame::Typing {
                conversation_id: id,
                user_id: id,
            })
            .unwrap(),
        ),
        (
            "ServerFrame::Pong",
            "pong",
            serde_json::to_value(ServerFrame::Pong { ts: 1 }).unwrap(),
        ),
        (
            "ServerFrame::ForceDisconnect",
            "force_disconnect",
            serde_json::to_value(ServerFrame::ForceDisconnect {
                reason: "token_revoked".into(),
            })
            .unwrap(),
        ),
    ];

    assert_eq!(
        cases.len(),
        10,
        "ServerFrame 应有 10 个变体; 数量对不上说明本测试漏了或多了某个变体"
    );
    for (label, wire, v) in cases {
        assert_all_none_frame_matches_required(label, "server", "type", wire, &v);
    }
}

// ---------------------------------------------------------------------------
// MessageContent: 6 类, 判别字段是 kind
// ---------------------------------------------------------------------------

#[test]
fn every_message_content_with_none_options_matches_the_spec() {
    let id = Uuid::new_v4();
    let cases: Vec<(&str, &str, MessageContent)> = vec![
        (
            "MessageContent::Text",
            "text",
            MessageContent::Text { text: "hi".into() },
        ),
        (
            "MessageContent::Image",
            "image",
            MessageContent::Image {
                media_id: id,
                width: None,
                height: None,
                thumbnail_media_id: None,
            },
        ),
        (
            "MessageContent::File",
            "file",
            MessageContent::File {
                media_id: id,
                file_name: "a.pdf".into(),
                size_bytes: 1,
                mime_type: None,
            },
        ),
        (
            "MessageContent::Sticker",
            "sticker",
            MessageContent::Sticker {
                sticker_id: "s1".into(),
            },
        ),
        (
            "MessageContent::System",
            "system",
            MessageContent::System {
                event: "join".into(),
                actor_user_id: None,
                target_user_id: None,
            },
        ),
        (
            "MessageContent::Custom",
            "custom",
            MessageContent::Custom {
                schema: "s".into(),
                data: json!({}),
            },
        ),
    ];

    assert_eq!(
        cases.len(),
        6,
        "MessageContent 应有 6 个变体; 数量对不上说明本测试漏了或多了某个变体"
    );
    for (label, wire, c) in cases {
        let v = serde_json::to_value(&c).unwrap();
        assert_all_none_frame_matches_required(label, "content", "kind", wire, &v);
    }
}

// ---------------------------------------------------------------------------
// 嵌套结构: ErrorBody / AckData
// ---------------------------------------------------------------------------

/// `ErrorBody.details` 带 `skip_serializing_if = "Vec::is_empty"`, 所以空数组
/// 时整个 key 消失 —— 规范不得把它列为必填。
#[test]
fn error_body_omits_details_when_empty() {
    let empty = serde_json::to_value(ErrorBody::new("VALIDATION_ERROR", "m", "tr")).unwrap();
    assert!(
        !empty.as_object().unwrap().contains_key("details"),
        "details 为空时整个 key 必须消失, 而不是 []: {empty}"
    );
    assert_eq!(
        empty.as_object().unwrap().len(),
        4,
        "恰好 4 个必填字段: {empty}"
    );

    let sc = spec()
        .pointer("/components/schemas/ErrorBody")
        .cloned()
        .expect("规范必须有 ErrorBody schema");
    assert_eq!(
        schema_property_names(&sc)
            .difference(&schema_required(&sc))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["details".to_string()],
        "ErrorBody 里唯一可选的属性必须是 details"
    );

    let full = serde_json::to_value(ErrorBody::new("VALIDATION_ERROR", "m", "tr").with_details(
        vec![FieldError {
            field: "text".into(),
            reason: "too long".into(),
        }],
    ))
    .unwrap();
    assert_eq!(
        full.pointer("/details/0/field").and_then(Value::as_str),
        Some("text"),
        "details 非空时必须真的出现: {full}"
    );
}

/// `AckData.idempotent_replay` 用自定义谓词 `is_false`, 故 `false` 时 key
/// 消失。这是全仓唯一一处「非 Option 也可能不出现」的字段, 值得单独锁住 ——
/// 它的判断语义是「key 存在且为 true」, 而不是「值不等于 false」。
#[test]
fn ack_data_omits_idempotent_replay_when_false() {
    let v = serde_json::to_value(AckData {
        message_id: None,
        sequence: None,
        idempotent_replay: false,
    })
    .unwrap();
    assert_eq!(
        v.as_object().unwrap().len(),
        0,
        "三个字段全空时 AckData 应序列化成 {{}}: {v}"
    );

    let v2 = serde_json::to_value(AckData {
        message_id: Some(Uuid::nil()),
        sequence: Some(7),
        idempotent_replay: true,
    })
    .unwrap();
    assert_eq!(
        v2.as_object().unwrap().len(),
        3,
        "非空时三个字段都该出现: {v2}"
    );

    let sc = spec()
        .pointer("/components/schemas/AckData")
        .cloned()
        .expect("规范必须有 AckData schema");
    assert_eq!(
        schema_property_names(&sc)
            .difference(&schema_required(&sc))
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            "idempotent_replay".to_string(),
            "message_id".to_string(),
            "sequence".to_string()
        ],
        "AckData 的三个字段全部可选(都带 skip_serializing_if)"
    );
}

/// `WireMessage` 的三个 Option 字段**只带 default 不带 skip**, 故恒出现为
/// `null`; `reactions` 无条件出现。规范必须把 11 个字段全列为必填。
#[test]
fn wire_message_emits_every_field_even_when_null() {
    let v = serde_json::to_value(WireMessage {
        id: Uuid::nil(),
        conversation_id: Uuid::nil(),
        sequence: 1,
        sender_id: None,
        kind: "text".into(),
        content: MessageContent::Text { text: "hi".into() },
        reply_to: None,
        state: "sent".into(),
        created_at: chrono::Utc::now(),
        edited_at: None,
        reactions: Vec::new(),
    })
    .unwrap();

    let obj = v.as_object().unwrap();
    for key in ["sender_id", "reply_to", "edited_at"] {
        assert_eq!(
            obj.get(key),
            Some(&Value::Null),
            "{key} 只带 serde(default) 不带 skip, 故必须以 null 出现而不是消失: {v}"
        );
    }
    assert_eq!(
        obj.get("reactions"),
        Some(&json!([])),
        "reactions 无条件出现(空则 []): {v}"
    );
    assert_eq!(obj.len(), 11, "WireMessage 恰好 11 个字段: {v}");

    let sc = spec()
        .pointer("/components/schemas/WireMessage")
        .cloned()
        .expect("规范必须有 WireMessage schema");
    let required = schema_required(&sc);
    assert_eq!(
        required.len(),
        11,
        "规范必须把 WireMessage 的 11 个字段全列为必填, 实际 {required:?}"
    );
}

/// 规范自身不能把 `type`/`kind` 这类判别字段漏出 `properties`。
#[test]
fn every_payload_schema_documents_its_discriminator() {
    let doc = spec();
    let schemas = doc
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .unwrap();
    let mut checked = 0usize;
    for (name, sc) in schemas {
        for tag in ["type", "kind"] {
            if let Some(c) = sc
                .pointer(&format!("/properties/{tag}/const"))
                .and_then(Value::as_str)
            {
                let required = schema_required(sc);
                assert!(
                    required.contains(tag),
                    "{name}: 判别字段 {tag} (const={c:?}) 必须同时出现在 required 里"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(
        checked, 25,
        "带判别常量的 schema 应有 19 个帧(8 client + 10 server + 1 auth_ok) \
         + 6 个 content = 25 个; 实际 {checked}"
    );
}
