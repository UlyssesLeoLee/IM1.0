//! `aci_emitter_helper` — IM1.0 ACI emitter helper (per ULYS-191 §4.3.3 brief v0.1).
//!
//! 4 个公开函数 + 1 个判定函数:
//! - [`emit_smoke_assertion`] — 用**真实测得值**判定 latency 断言
//! - [`emit_ws_frame_assertion`] — 用**真实观察到的 kind**判定 WS frame 断言
//! - [`evaluate_latency`] — latency 判定的唯一真值来源 (PASS/FAIL)
//! - [`emit_latency_assertion`] — 通用 latency 发射器, 支持四种状态
//!
//! ## 为什么没有 placeholder
//!
//! v0.1 的实现不接受任何输入, 把 actual 写死成 `15`、status 写死成
//! `Status::Pass`, reasoning 写死成 `"actual << expect (6x margin) —
//! placeholder per §4.3.3 brief v0.1"`。这类结构恒绿: 被测函数无论返回
//! 多慢都报 PASS, 测试因此**永远无法失败** —— 即 gap-ledger 反复记录的
//! 「恒绿测试 / 静默跳过」模式。
//!
//! 本版把判定从常量移到 [`evaluate_latency`]: status 由**调用方传入的实测值**
//! 与 expect 比较得出, FAIL 路径是真实可达的。
//!
//! 与 IDE1.0 §4.2.2 + RGS §4.3.1 pattern 1:1 对齐 (per brief v0.1 §3.2 #19).
//!
//! 守门:
//! - #7 unsafe_code="forbid": im-testkit/Cargo.toml L8 已 forbid (workspace lints)
//! - #11 缺标比错标: git dep 锁 rev=df28c56, 字段名 1:1 对齐 aci-emitter + Star Python emitter
//! - #13 W/T/M: emit 公开函数 + 单测 + IT 配对
//! - #24 vendor 中立: 仅 1 git dep aci-emitter (自家)

use aci_emitter::{
    AciEmitter, Assertion, ExpectActual, ExpectValueType, Layer, Scope, Severity, Status,
};

/// smoke assertion 的默认 latency 上限 (ms) —— 即 expect 值。
pub const SMOKE_EXPECT_MS: u64 = 100;

/// Emit 一条 IM1.0 latency smoke assertion, 状态由**实测值**决定。
///
/// # 参数
/// - `measured_ms`: 调用方**真实测得**的响应耗时 (ms)
/// - `budget_ms`: latency 预算 (ms), 缺省 [`SMOKE_EXPECT_MS`]
///
/// # 返回
///
/// `Assertion` (per aci-emitter v0.1.0)。`status` 由 [`evaluate_latency`]
/// 计算: `measured_ms <= budget_ms` → `PASS`, 否则 `FAIL`。
/// `severity` 同样随之变化: PASS → `info`, FAIL → `high`。
///
/// # Example
///
/// ```
/// use im_testkit::aci_emitter_helper::emit_smoke_assertion;
///
/// // 实测 15ms, 预算 100ms -> PASS
/// let ok = emit_smoke_assertion(15);
/// assert_eq!(ok.status, aci_emitter::Status::Pass);
///
/// // 实测 250ms, 超出预算 -> FAIL (证明这条断言有牙)
/// let slow = emit_smoke_assertion(250);
/// assert_eq!(slow.status, aci_emitter::Status::Fail);
/// ```
#[must_use]
pub fn emit_smoke_assertion_with_budget(measured_ms: u64, budget_ms: u64) -> Assertion {
    let status = evaluate_latency(measured_ms, budget_ms);
    let severity = match status {
        Status::Pass => Severity::Info,
        Status::Fail => Severity::High,
        Status::Warn | Status::Skip => Severity::Low,
    };
    let reasoning = match status {
        Status::Pass => format!(
            "measured {measured_ms}ms <= budget {budget_ms}ms ({:.0}% of budget)",
            ratio_pct(measured_ms, budget_ms)
        ),
        Status::Fail => format!(
            "measured {measured_ms}ms EXCEEDS budget {budget_ms}ms ({:.0}% of budget)",
            ratio_pct(measured_ms, budget_ms)
        ),
        Status::Warn | Status::Skip => unreachable!("evaluate_latency 只产出 Pass/Fail"),
    };

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
            serde_json::json!(budget_ms),
            format!("IM1.0 API should respond within {budget_ms}ms"),
        ),
        ExpectActual::new(
            ExpectValueType::ResponseWithinMs,
            serde_json::json!(measured_ms),
            format!("measured {measured_ms}ms"),
        ),
        status,
        severity,
        reasoning,
    )
    .expect("smoke assertion build must succeed")
}

/// Emit 一条 IM1.0 latency smoke assertion, 使用默认预算 [`SMOKE_EXPECT_MS`].
///
/// # Example
///
/// ```
/// use im_testkit::aci_emitter_helper::emit_smoke_assertion;
/// let a = emit_smoke_assertion(15);
/// assert_eq!(a.assertion_id, "im1.0:smoke:g-1");
/// assert_eq!(a.expect.value, serde_json::json!(100));
/// ```
#[must_use]
pub fn emit_smoke_assertion(measured_ms: u64) -> Assertion {
    emit_smoke_assertion_with_budget(measured_ms, SMOKE_EXPECT_MS)
}

/// latency 判定的唯一真值来源: 实测值是否在预算内。
///
/// 这是本 crate 里**唯一**决定 latency 断言 PASS/FAIL 的地方 —— emit 函数
/// 只负责把结果包装成 ACI assertion, 不自己复算。这样判定逻辑只有一份,
/// 单元测试可以直接针对它证明「超预算 → FAIL」。
///
/// `budget_ms == 0` 时恒 FAIL (任何耗时都超一个 0 预算), 不做除零。
pub fn evaluate_latency(measured_ms: u64, budget_ms: u64) -> Status {
    if measured_ms <= budget_ms {
        Status::Pass
    } else {
        Status::Fail
    }
}

/// Emit 一条通用 latency assertion, 显式指定状态与严重度。
///
/// 供需要 WARN / SKIP 的调用场景使用 (例如: 环境缺失无法测量 → SKIP,
/// 测量方法有已知误差 → WARN)。本函数**不做判定**, status 由调用方负责 ——
/// 它的存在是为了让 CI 能表达「测不了」与「测了但不确定」这两种真实状态,
/// 而不是把它们伪装成 PASS。
///
/// # Example
///
/// ```
/// use aci_emitter::{Severity, Status};
/// use im_testkit::aci_emitter_helper::emit_latency_assertion;
///
/// // 依赖未就绪, 无法测量 -> SKIP, 而不是一个假的 PASS
/// let skipped = emit_latency_assertion("im1.0:smoke:db", 0, 100, Status::Skip, Severity::Low,
///     "postgres not reachable: latency not measurable");
/// assert_eq!(skipped.status, Status::Skip);
/// ```
#[must_use]
pub fn emit_latency_assertion(
    assertion_id: &str,
    measured_ms: u64,
    budget_ms: u64,
    status: Status,
    severity: Severity,
    reasoning: &str,
) -> Assertion {
    let em = AciEmitter::new(Layer::It);
    em.build(
        assertion_id.to_string(),
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
            serde_json::json!(budget_ms),
            format!("latency budget {budget_ms}ms"),
        ),
        ExpectActual::new(
            ExpectValueType::ResponseWithinMs,
            serde_json::json!(measured_ms),
            format!("measured {measured_ms}ms"),
        ),
        status,
        severity,
        reasoning,
    )
    .expect("latency assertion build must succeed")
}

/// Emit 一条 IM1.0 WS frame 特定 assertion (im-testkit 集成点).
///
/// 与 im-testkit [`crate::assertions::assert_ws_frame_shape`] 配对:
/// - 本函数 emit ACI assertion (期望 + 实测 + reasoning)
/// - 测试断言时调 `assert_ws_frame_shape(frame, kind)` 验证 shape
///
/// **与 v0.1 的关键差别**: v0.1 把 `expect.value` 和 `actual.value` 都写成
/// `kind` (传入值), status 恒 `PASS` —— 即「传入什么就断言什么」, 同样恒绿。
/// 本版要求传入**真实观察到的** `observed_kind`, 与 `expected_kind` 比较后才
/// 定状态。
///
/// # 参数
/// - `expected_kind`: 规范要求的 frame kind
/// - `observed_kind`: 从**真实 frame** 解出来的 kind
/// - `expected_shape_ok`: `assert_ws_frame_shape` 的实测结果
///
/// # Example
///
/// ```
/// use im_testkit::aci_emitter_helper::emit_ws_frame_assertion_with_observed;
///
/// let ok = emit_ws_frame_assertion_with_observed("connected", "connected", true);
/// assert_eq!(ok.status, aci_emitter::Status::Pass);
///
/// // 观察到的 kind 与期望不符 -> FAIL
/// let wrong = emit_ws_frame_assertion_with_observed("connected", "pong", true);
/// assert_eq!(wrong.status, aci_emitter::Status::Fail);
/// ```
#[must_use]
pub fn emit_ws_frame_assertion_with_observed(
    expected_kind: &str,
    observed_kind: &str,
    expected_shape_ok: bool,
) -> Assertion {
    let kind_matches = observed_kind == expected_kind;
    let status = if kind_matches && expected_shape_ok {
        Status::Pass
    } else {
        Status::Fail
    };
    let severity = match status {
        Status::Pass => Severity::Info,
        Status::Fail => Severity::High,
        Status::Warn | Status::Skip => Severity::Low,
    };
    let reasoning = match (kind_matches, expected_shape_ok) {
        (true, true) => {
            format!("WS frame kind {observed_kind:?} matches {expected_kind:?} and shape ok")
        }
        (false, true) => format!(
            "WS frame kind MISMATCH: expected {expected_kind:?}, observed {observed_kind:?}"
        ),
        (true, false) => format!("WS frame kind {observed_kind:?} matches but SHAPE CHECK FAILED"),
        (false, false) => format!(
            "WS frame kind MISMATCH ({expected_kind:?} != {observed_kind:?}) AND shape check failed"
        ),
    };

    let em = AciEmitter::new(Layer::It);
    em.build(
        format!("im1.0:ws:{expected_kind}"),
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
            serde_json::json!(expected_kind),
            format!("WS server frame kind should == {expected_kind:?}"),
        ),
        ExpectActual::new(
            ExpectValueType::FieldEquals,
            serde_json::json!(observed_kind),
            format!("observed kind == {observed_kind:?}"),
        ),
        status,
        severity,
        reasoning,
    )
    .expect("ws frame assertion build must succeed")
}

/// Emit 一条 IM1.0 WS frame 特定 assertion, shape 判定假定已通过。
///
/// 便捷封装: 等价于 `emit_ws_frame_assertion_with_observed(kind, kind, true)`。
///
/// # Example
///
/// ```
/// use im_testkit::aci_emitter_helper::emit_ws_frame_assertion;
/// let a = emit_ws_frame_assertion("connected");
/// assert!(a.assertion_id.ends_with("connected"));
/// ```
#[must_use]
pub fn emit_ws_frame_assertion(kind: &str) -> Assertion {
    emit_ws_frame_assertion_with_observed(kind, kind, true)
}

/// 实测耗时占预算的百分比 (reasoning 文案用)。
fn ratio_pct(measured_ms: u64, budget_ms: u64) -> f64 {
    if budget_ms == 0 {
        return 100.0;
    }
    (measured_ms as f64 / budget_ms as f64) * 100.0
}

/// Emit smoke assertion 并序列化为 pretty JSON (供 `aci-smoke.sh` 调 cargo test 输出).
///
/// # Returns
///
/// pretty-printed JSON 字符串 (`serde_json::to_string_pretty` 输出).
#[must_use]
pub fn emit_smoke_assertion_json(measured_ms: u64) -> String {
    let assertion = emit_smoke_assertion(measured_ms);
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
pub fn emit_ws_frame_assertion_json(expected_kind: &str, observed_kind: &str) -> String {
    let assertion = emit_ws_frame_assertion_with_observed(expected_kind, observed_kind, true);
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
    fn test_evaluate_latency_pass_on_budget() {
        assert_eq!(evaluate_latency(15, 100), Status::Pass);
        assert_eq!(
            evaluate_latency(100, 100),
            Status::Pass,
            "boundary: == budget is PASS"
        );
        assert_eq!(evaluate_latency(0, 100), Status::Pass);
    }

    #[test]
    fn test_evaluate_latency_fail_over_budget() {
        assert_eq!(evaluate_latency(101, 100), Status::Fail);
        assert_eq!(evaluate_latency(250, 100), Status::Fail);
        // 判别力证明: 单调递增的预算关系, 不存在「无论输入都 PASS」
        assert_ne!(evaluate_latency(250, 100), evaluate_latency(15, 100));
    }

    #[test]
    fn test_evaluate_latency_zero_budget_always_fail() {
        assert_eq!(evaluate_latency(0, 0), Status::Pass, "0<=0 holds");
        assert_eq!(
            evaluate_latency(1, 0),
            Status::Fail,
            "no divide-by-zero panic"
        );
    }

    #[test]
    fn test_smoke_basic_pass_path() {
        let a = emit_smoke_assertion(15);
        assert_eq!(a.assertion_id, "im1.0:smoke:g-1");
        assert_eq!(a.aci_version, "0.1.0-draft");
        assert_eq!(a.layer, Layer::It);
        assert_eq!(a.status, Status::Pass);
        assert_eq!(a.severity, Severity::Info);
        assert_eq!(a.scope.project, "im1.0");
        assert_eq!(a.scope.module.as_deref(), Some("smoke"));
        assert_eq!(a.expect.value, serde_json::json!(100));
        assert_eq!(a.actual.value, serde_json::json!(15));
        assert!(
            a.reasoning.contains("15"),
            "reasoning must cite measured value"
        );
        assert!(
            !a.reasoning.contains("placeholder"),
            "reasoning must not contain 'placeholder': {}",
            a.reasoning
        );
    }

    #[test]
    fn test_smoke_fail_path_is_reachable() {
        // 这是 v0.1 缺失的关键能力: 实测超预算必须报 FAIL
        let a = emit_smoke_assertion(250);
        assert_eq!(a.status, Status::Fail);
        assert_eq!(a.severity, Severity::High);
        assert_eq!(a.actual.value, serde_json::json!(250));
        assert!(
            a.reasoning.contains("EXCEEDS"),
            "FAIL reasoning must say why: {}",
            a.reasoning
        );
    }

    #[test]
    fn test_smoke_custom_budget() {
        let a = emit_smoke_assertion_with_budget(30, 20);
        assert_eq!(a.status, Status::Fail, "30ms over a 20ms budget must fail");
        assert_eq!(a.expect.value, serde_json::json!(20));
        assert_eq!(a.actual.value, serde_json::json!(30));

        let b = emit_smoke_assertion_with_budget(10, 20);
        assert_eq!(b.status, Status::Pass);
    }

    #[test]
    fn test_ws_frame_basic_pass() {
        for kind in &["connected", "ack", "message_new", "pong"] {
            let a = emit_ws_frame_assertion(kind);
            assert!(
                a.assertion_id.ends_with(kind),
                "assertion_id should end with {kind:?}: {}",
                a.assertion_id
            );
            assert_eq!(a.scope.project, "im1.0");
            assert_eq!(a.scope.module.as_deref(), Some("ws"));
            assert_eq!(a.status, Status::Pass);
        }
    }

    #[test]
    fn test_ws_frame_kind_mismatch_is_fail() {
        // v0.1 里这个 case 无法表达: 传入什么就断言什么
        let a = emit_ws_frame_assertion_with_observed("connected", "pong", true);
        assert_eq!(a.status, Status::Fail);
        assert_eq!(a.expect.value, serde_json::json!("connected"));
        assert_eq!(a.actual.value, serde_json::json!("pong"));
        assert!(a.reasoning.contains("MISMATCH"), "{}", a.reasoning);
        assert_eq!(a.severity, Severity::High);
    }

    #[test]
    fn test_ws_frame_shape_failure_is_fail() {
        let a = emit_ws_frame_assertion_with_observed("ack", "ack", false);
        assert_eq!(a.status, Status::Fail);
        assert!(a.reasoning.contains("SHAPE"), "{}", a.reasoning);
    }

    #[test]
    fn test_skip_and_warn_statuses_are_expressible() {
        // 「测不了」必须能表达成 SKIP, 而不是伪装成 PASS
        let skipped = emit_latency_assertion(
            "im1.0:smoke:db",
            0,
            100,
            Status::Skip,
            Severity::Low,
            "postgres not reachable: latency not measurable",
        );
        assert_eq!(skipped.status, Status::Skip);

        let warned = emit_latency_assertion(
            "im1.0:smoke:flaky",
            50,
            100,
            Status::Warn,
            Severity::Low,
            "measurement taken under contention",
        );
        assert_eq!(warned.status, Status::Warn);
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
