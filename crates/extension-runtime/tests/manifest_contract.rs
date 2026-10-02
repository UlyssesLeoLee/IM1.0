//! ExtensionManifest 契约测试 (2026-10-03 新增)
//!
//! 依据: `docs/SRS.md` §26 **EXT-FR-001 (P0, MVP 范围内)** ——
//!   "Extension 通过 Manifest 声明 Permission / Scope / Capability /
//!    Event Subscription / Command / Webhook"。
//!
//! 这份 Manifest 是**第三方扩展作者填写的输入**, 也就是不可信边界。扩展靠
//! `permissions` / `capabilities` / `event_subscriptions` / `commands` 声明
//! 自己被允许做什么, 宿主据此做授权判定。因此它的反序列化行为必须被钉死 ——
//! 否则"多写一个字段""少写一个字段""写成 null"这类差异会静默改变扩展的权限面。
//!
//! 本文件只测**契约行为**, 不测业务逻辑(V1 实装沙箱/能力白名单/版本仲裁后再补)。
//! 不新增依赖: serde / serde_json 已是 extension-runtime 的 [dependencies]。
//!
//! 不修改 `ExtensionManifest` 的 derive (没有加 `PartialEq` 之类只为测试服务的
//! 改动); 往返比较用 `serde_json::Value` 做, 避免动生产代码。

use extension_runtime::ExtensionManifest;
use serde_json::json;

// ============================================================================
// 1. 最小 manifest: 只有必填字段, 可选集合应全部落到默认值
// ============================================================================

#[test]
fn minimal_manifest_falls_back_to_empty_collections() {
    let m: ExtensionManifest =
        serde_json::from_value(json!({ "name": "acme.plugin", "version": "1.0.0" }))
            .expect("只含 name/version 的 manifest 应当能反序列化");

    assert_eq!(m.name, "acme.plugin");
    assert_eq!(m.version, "1.0.0");

    // 四个集合字段带 #[serde(default)], 缺省必须是空 Vec 而不是反序列化失败
    assert!(m.permissions.is_empty(), "permissions 缺省应为空");
    assert!(m.capabilities.is_empty(), "capabilities 缺省应为空");
    assert!(
        m.event_subscriptions.is_empty(),
        "event_subscriptions 缺省应为空"
    );
    assert!(m.commands.is_empty(), "commands 缺省应为空");

    assert!(m.webhook_url.is_none(), "webhook_url 缺省应为 None");
}

// ============================================================================
// 2. 完整 manifest 往返
// ============================================================================

#[test]
fn full_manifest_round_trips_without_loss() {
    let src = json!({
        "name": "acme.ai-moderator",
        "version": "0.3.1",
        "permissions": ["im.message.read", "im.user.profile.read"],
        "capabilities": ["text.classify", "text.moderate"],
        "event_subscriptions": ["im.message.created"],
        "commands": ["moderate_message"],
        "webhook_url": "https://example.com/hooks/im"
    });

    let m: ExtensionManifest =
        serde_json::from_value(src.clone()).expect("完整 manifest 应可反序列化");

    assert_eq!(m.name, "acme.ai-moderator");
    assert_eq!(m.version, "0.3.1");
    assert_eq!(
        m.permissions,
        vec!["im.message.read", "im.user.profile.read"]
    );
    assert_eq!(m.capabilities, vec!["text.classify", "text.moderate"]);
    assert_eq!(m.event_subscriptions, vec!["im.message.created"]);
    assert_eq!(m.commands, vec!["moderate_message"]);
    assert_eq!(
        m.webhook_url.as_deref(),
        Some("https://example.com/hooks/im")
    );

    // 往返: 序列化回去应与输入逐字段等价 (用 Value 比, 不给结构体加 PartialEq)
    let back = serde_json::to_value(&m).expect("重新序列化");
    assert_eq!(back, src, "manifest 往返后应与原始 JSON 完全一致");
}

// ============================================================================
// 3. 必填字段缺失必须被拒
// ============================================================================

#[test]
fn missing_name_is_rejected() {
    let r = serde_json::from_value::<ExtensionManifest>(json!({ "version": "1.0.0" }));
    assert!(
        r.is_err(),
        "缺 name 的 manifest 应当被拒 —— name 是必填标识"
    );
}

#[test]
fn missing_version_is_rejected() {
    let r = serde_json::from_value::<ExtensionManifest>(json!({ "name": "acme.plugin" }));
    assert!(
        r.is_err(),
        "缺 version 的 manifest 应当被拒 —— 宿主需据此做版本仲裁 (EXT-FR-004)"
    );
}

// ============================================================================
// 4. 类型错误必须被拒(而不是静默降级成空)
// ============================================================================

#[test]
fn wrong_type_is_rejected_not_silently_defaulted() {
    // permissions 期望数组, 给了字符串。**不能**因此回退成空 Vec ——
    // 那等于把一个写错类型的扩展当成"什么都没声明"放行。
    let r = serde_json::from_value::<ExtensionManifest>(json!({
        "name": "acme.plugin",
        "version": "1.0.0",
        "permissions": "im.message.read"   // 错: 应为数组
    }));
    assert!(
        r.is_err(),
        "permissions 类型错误应报错, 不应静默回退为空 —— 否则扩展权限面被无声改变"
    );
}

// ============================================================================
// 5. webhook_url 的三种 JSON 形态
// ============================================================================

#[test]
fn webhook_url_accepts_absent_null_and_value() {
    let absent: ExtensionManifest =
        serde_json::from_value(json!({ "name": "n", "version": "1" })).unwrap();
    assert!(absent.webhook_url.is_none());

    let explicit_null: ExtensionManifest =
        serde_json::from_value(json!({ "name": "n", "version": "1", "webhook_url": null }))
            .unwrap();
    assert!(explicit_null.webhook_url.is_none());

    let present: ExtensionManifest = serde_json::from_value(
        json!({ "name": "n", "version": "1", "webhook_url": "https://example.com/h" }),
    )
    .unwrap();
    assert_eq!(
        present.webhook_url.as_deref(),
        Some("https://example.com/h")
    );
}

// ============================================================================
// 6. 畸形输入不得 panic
// ============================================================================

#[test]
fn malformed_input_errors_instead_of_panicking() {
    for bad in [
        json!(null),
        json!([]),
        json!("just a string"),
        json!({}),
        json!({ "name": 42, "version": "1.0.0" }),
    ] {
        let r = serde_json::from_value::<ExtensionManifest>(bad.clone());
        assert!(
            r.is_err(),
            "畸形输入 {bad} 应当报错, 而不是 panic 或静默接受"
        );
    }
}

// ============================================================================
// 7. 未知字段的当前行为 (钉住现状, 附带待决问题)
// ============================================================================

#[test]
fn unknown_fields_are_currently_ignored() {
    // serde 未加 deny_unknown_fields, 故未知字段被忽略 —— 这给了扩展作者
    // 前向兼容(新版 manifest 字段喂给旧宿主不会炸)。
    //
    // ⚠️ **待团队拍板**: 对不可信输入, 忽略未知字段有风险 —— 一个扩展作者可以
    // 写 "permissionz": [...](拼错) 而静默拿不到权限, 却以为已声明成功。
    // 两种取舍:
    //   (a) 保持忽略 —— 前向兼容好, 代价是拼写错误静默失效
    //   (b) 加 deny_unknown_fields —— 早失败, 代价是宿主升级前旧 manifest 会被拒
    // 本测试**只钉住当前行为**, 不代表 (a) 已被认定为正确策略。
    let m: ExtensionManifest = serde_json::from_value(json!({
        "name": "acme.plugin",
        "version": "1.0.0",
        "future_field_from_newer_host": { "whatever": true }
    }))
    .expect("当前实现忽略未知字段");
    assert_eq!(m.name, "acme.plugin");
}
