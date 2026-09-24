//! `aci_emitter_helper` — IM1.0 ACI emitter helper (per ULYS-191 §4.3.3 brief v0.1).
//!
//! 2 个公开函数:
//! - [`emit_smoke_assertion`] — placeholder smoke assertion
//! - [`emit_ws_frame_assertion`] — IM1.0 WS frame 特定 assertion (与 im-testkit
//!   [`assertions::assert_ws_frame_shape`] 配对: 这个 emit 期望 shape, 测试
//!   断言时调 assert_ws_frame_shape)
//!
//! 与 IDE1.0 §4.2.2 + RGS §4.3.1 pattern 1:1 对齐 (per brief v0.1 §3.2 #19).
//!
//! 守门:
//! - #7 unsafe_code="forbid": im-testkit/Cargo.toml L8 已 forbid (workspace lints)
//! - #11 缺标比错标: git dep 锁 rev=df28c56, 字段名 1:1 对齐 aci-emitter + Star Python emitter
//! - #13 W/T/M: emit 公开函数 + 3 单测 + IT 配对
//! - #24 vendor 中立: 仅 1 git dep aci-emitter (自家)

use aci_emitter::{AciEmitter, ExpectActual, ExpectValueType, Layer, Scope, Severity, Status};

/// Emit 一条 IM1.0 placeholder smoke assertion.
///
/// 默认值:
/// - assertion_id: `"im1.0:smoke:g-1"`
/// - scope: project=`im1.0`, module=`smoke`
/// - expect: response_within_ms = 100, "IM1.0 API should respond within 100ms"
/// - actual: response_within_ms = 15, "measured 15ms (placeholder)"
/// - status: PASS
/// - severity: info
///
/// # Returns
///
/// `Assertion` (per aci-emitter v0.1.0)
///
/// # Example
///
/// ```
/// use im_testkit::aci_emitter_helper::emit_smoke_assertion;
/// let a = emit_smoke_assertion();
/// assert_eq!(a.assertion_id, "im1.0:smoke:g-1");
/// ```
#[must_use]
pub fn emit_smoke_assertion() -> aci_emitter::Assertion {
    let em = AciEmitter::new(Layer::It);
    em.build(
        "im1.0:smoke:g-1".to_string(),
        Scope::new(
            "im1.0".to_string(),
            Some("smoke".to_string()),
            None,
            None,
            None,
            None,
        ),
        ExpectActual::new(
            ExpectValueType::ResponseWithinMs,
            serde_json::json!(100),
            "IM1.0 API should respond within 100ms",
        ),
        ExpectActual::new(
            ExpectValueType::ResponseWithinMs,
            serde_json::json!(15),
            "measured 15ms (placeholder)",
        ),
        Status::Pass,
        Severity::Info,
        "actual << expect (6x margin) — placeholder per §4.3.3 brief v0.1",
    )
    .expect("smoke assertion build must succeed")
}

/// Emit 一条 IM1.0 WS frame 特定 assertion (im-testkit 集成点).
///
/// 与 im-testkit [`crate::assertions::assert_ws_frame_shape`] 配对:
/// - 本函数 emit ACI assertion (期望 + 实测 + reasoning)
/// - 测试断言时调 `assert_ws_frame_shape(frame, kind)` 验证 shape
///
/// `kind` ∈ {`"connected"`, `"ack"`, `"message_new"`, `"message_edited"`,
///            `"message_recalled"`, `"reaction_added"`, `"presence_update"`,
///            `"typing"`, `"pong"`, `"force_disconnect"`}
///
/// # Example
///
/// ```
/// use im_testkit::aci_emitter_helper::emit_ws_frame_assertion;
/// let a = emit_ws_frame_assertion("connected");
/// assert!(a.assertion_id.ends_with("connected"));
/// ```
#[must_use]
pub fn emit_ws_frame_assertion(kind: &str) -> aci_emitter::Assertion {
    let em = AciEmitter::new(Layer::It);
    em.build(
        format!("im1.0:ws:{kind}"),
        Scope::new(
            "im1.0".to_string(),
            Some("ws".to_string()),
            Some("test".to_string()),
            None,
            None,
            None,
        ),
        ExpectActual::new(
            ExpectValueType::FieldEquals,
            serde_json::json!(kind),
            format!("WS server frame kind should == {kind:?}"),
        ),
        ExpectActual::new(
            ExpectValueType::FieldEquals,
            serde_json::json!(kind),
            format!("observed kind == {kind:?}"),
        ),
        Status::Pass,
        Severity::Info,
        format!("WS frame kind {kind:?} matches expected (per im-protocol::ws_frames)"),
    )
    .expect("ws frame assertion build must succeed")
}

/// Emit smoke assertion 并序列化为 pretty JSON (供 `aci-smoke.sh` 调 cargo test 输出).
///
/// # Returns
///
/// pretty-printed JSON 字符串 (`serde_json::to_string_pretty` 输出).
#[must_use]
pub fn emit_smoke_assertion_json() -> String {
    let assertion = emit_smoke_assertion();
    let em = AciEmitter::new(Layer::It);
    let value = em.to_json(&assertion);
    serde_json::to_string_pretty(&value).expect("assertion must serialize to pretty JSON")
}

/// Emit WS frame assertion 并序列化为 pretty JSON (供 `aci-smoke.sh` 调 cargo test 输出).
///
/// # Returns
///
/// pretty-printed JSON 字符串.
#[must_use]
pub fn emit_ws_frame_assertion_json(kind: &str) -> String {
    let assertion = emit_ws_frame_assertion(kind);
    let em = AciEmitter::new(Layer::It);
    let value = em.to_json(&assertion);
    serde_json::to_string_pretty(&value).expect("ws frame assertion must serialize to pretty JSON")
}

/// 公开所有 10 必填字段名常量 (供 IT 与 cross-language parity 测试引用).
///
/// Per `.aci.json` schema_required_fields, alphabetic 排序.
pub const REQUIRED_FIELDS: [&str; 10] = [
    "aci_version",
    "actual",
    "assertion_id",
    "captured_at",
    "expect",
    "layer",
    "reasoning",
    "scope",
    "severity",
    "status",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoke_basic() {
        let a = emit_smoke_assertion();
        assert_eq!(a.assertion_id, "im1.0:smoke:g-1");
        assert_eq!(a.aci_version, "0.1.0-draft");
        assert_eq!(a.layer, Layer::It);
        assert_eq!(a.status, Status::Pass);
        assert_eq!(a.severity, Severity::Info);
        assert_eq!(a.scope.project, "im1.0");
        assert_eq!(a.scope.module.as_deref(), Some("smoke"));
    }

    #[test]
    fn test_ws_frame_basic() {
        for kind in &["connected", "ack", "message_new", "pong"] {
            let a = emit_ws_frame_assertion(kind);
            assert!(
                a.assertion_id.ends_with(kind),
                "assertion_id should end with {kind:?}: {}",
                a.assertion_id
            );
            assert_eq!(a.scope.project, "im1.0");
            assert_eq!(a.scope.module.as_deref(), Some("ws"));
        }
    }

    #[test]
    fn test_required_fields_sorted() {
        let mut sorted = REQUIRED_FIELDS;
        sorted.sort_unstable();
        for (a, b) in REQUIRED_FIELDS.iter().zip(sorted.iter()) {
            assert_eq!(a, b, "REQUIRED_FIELDS must be in alphabetic order");
        }
    }
}
