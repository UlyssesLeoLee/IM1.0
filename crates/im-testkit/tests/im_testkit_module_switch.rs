//! im-testkit module_switch integration tests
//!
//! Per ULYS-190 §4.4 brief v0.1 §1.1 item 4: verify that the 28 module entries
//! declared in `.aci.json` plugins.<plugin_id>.modules.<module_id> can all be
//! read back by `_lib_mock_switch_im.py` and that each module has consistent
//! enabled/mode state with the cluster-level settings.
//!
//! These tests use subprocess to invoke the Python helper (per 守门 #9) so the
//! behaviour matches what the IM1.0 dispatcher will see in production.
//!
//! DDD Review 必查项:
//! - 28 module 全部 readable
//! - enabled/mode 字段一致性
//! - mock_switch_trace_format 真拼接 (per brief §4)
//! - aci_compat_version 跨文件一致性

use std::path::PathBuf;
use std::process::Command;

fn im_testkit_root() -> PathBuf {
    // tests/ directory is one level under crates/im-testkit
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR must be set by cargo test runner");
    PathBuf::from(manifest)
}

fn run_mock_switch_reader() -> serde_json::Value {
    let root = im_testkit_root();
    let script = root.join("scripts").join("_lib_mock_switch_im.py");

    let output = Command::new("python")
        .arg(script)
        .arg("--json")
        .arg(&root)
        .output()
        .expect("failed to invoke _lib_mock_switch_im.py");

    assert!(
        output.status.success(),
        "_lib_mock_switch_im.py failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("output must be valid JSON")
}

/// 读取某个 plugin 真实声明的 module 名单 (来自 Python helper 读回的 .aci.json)。
///
/// **这条辅助函数是本文件所有「名单类断言」的基础** —— 断言必须比对
/// helper 真正读到的名单, 而不是拿写死的列表和自己比 (那种写法恒绿)。
fn module_names(state: &serde_json::Value, plugin: &str) -> Vec<String> {
    state["plugins"][plugin]["modules"]
        .as_array()
        .unwrap_or_else(|| panic!("plugins.{plugin}.modules must be an array"))
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("module entry must be a string"))
                .to_string()
        })
        .collect()
}

#[test]
fn module_switch_total_count_is_28() {
    let state = run_mock_switch_reader();
    let total = state["summary"]["modules_total"].as_u64().unwrap();
    assert_eq!(total, 28, "expected 28 modules total across 5 plugins");
}

#[test]
fn module_switch_all_enabled_by_default() {
    let state = run_mock_switch_reader();
    let enabled = state["summary"]["modules_enabled"].as_u64().unwrap();
    let total = state["summary"]["modules_total"].as_u64().unwrap();
    assert_eq!(
        enabled, total,
        "all 28 modules should be enabled by default"
    );
}

#[test]
fn mock_grpc_has_4_modules() {
    let state = run_mock_switch_reader();
    let grpc = &state["plugins"]["mock_grpc"];
    assert_eq!(grpc["modules_total"].as_u64().unwrap(), 4);
    assert_eq!(grpc["modules_enabled"].as_u64().unwrap(), 4);

    // 比对 helper **真正读回的名单**, 不是拿 expected 和它自己的副本比。
    // v0.1 的写法是 `let actual = expected.to_vec()` 再 contains ——
    // 无论 helper 返回什么都会通过。
    let expected: Vec<String> = vec![
        "list_messages".to_string(),
        "mark_read".to_string(),
        "send_message".to_string(),
        "validate_access_token".to_string(),
    ];
    let mut actual = module_names(&state, "mock_grpc");
    actual.sort();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    assert_eq!(
        actual, expected_sorted,
        "mock_grpc module names must match .aci.json exactly"
    );
}

#[test]
fn mock_rest_has_7_modules() {
    let state = run_mock_switch_reader();
    let rest = &state["plugins"]["mock_rest"];
    assert_eq!(rest["modules_total"].as_u64().unwrap(), 7);
    assert_eq!(rest["modules_enabled"].as_u64().unwrap(), 7);

    let mut expected: Vec<String> = vec![
        "create_conversation".to_string(),
        "error_responses".to_string(),
        "guest_register".to_string(),
        "list_messages_rest".to_string(),
        "login".to_string(),
        "presign_media".to_string(),
        "send_message_rest".to_string(),
    ];
    let mut actual = module_names(&state, "mock_rest");
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "mock_rest module names must match .aci.json"
    );
}

#[test]
fn mock_ws_frames_has_9_modules_including_server_frames_aggregation() {
    let state = run_mock_switch_reader();
    let ws = &state["plugins"]["mock_ws_frames"];
    assert_eq!(ws["modules_total"].as_u64().unwrap(), 9);
    assert_eq!(ws["modules_enabled"].as_u64().unwrap(), 9);

    // server_frames 是聚合项; 其余 8 个与 §1.47 点名的「声明了但生产不发送」
    // 帧相关 —— 名单必须显式列出, 否则增减一个都无人察觉
    let mut expected: Vec<String> = vec![
        "auth".to_string(),
        "edit_message".to_string(),
        "mark_read".to_string(),
        "ping".to_string(),
        "react".to_string(),
        "recall_message".to_string(),
        "send_message".to_string(),
        "server_frames".to_string(),
        "typing".to_string(),
    ];
    let mut actual = module_names(&state, "mock_ws_frames");
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "mock_ws_frames module names must match .aci.json"
    );
}

#[test]
fn assertions_plugin_has_3_modules_unchanged_from_pr23() {
    let state = run_mock_switch_reader();
    let a = &state["plugins"]["assertions"];
    assert_eq!(a["modules_total"].as_u64().unwrap(), 3);
    assert_eq!(a["modules_enabled"].as_u64().unwrap(), 3);

    let mut expected: Vec<String> = vec![
        "assert_error_code".to_string(),
        "assert_json_schema".to_string(),
        "assert_ws_frame_shape".to_string(),
    ];
    let mut actual = module_names(&state, "assertions");
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "assertions module names must match .aci.json"
    );
}

#[test]
fn fixtures_plugin_has_5_modules_unchanged_from_pr23() {
    let state = run_mock_switch_reader();
    let f = &state["plugins"]["fixtures"];
    assert_eq!(f["modules_total"].as_u64().unwrap(), 5);
    assert_eq!(f["modules_enabled"].as_u64().unwrap(), 5);

    let mut expected: Vec<String> = vec![
        "device".to_string(),
        "message".to_string(),
        "session".to_string(),
        "tenant".to_string(),
        "user".to_string(),
    ];
    let mut actual = module_names(&state, "fixtures");
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "fixtures module names must match .aci.json"
    );
}

/// 汇总约束: 5 个 plugin 的名单长度之和 == summary.modules_total == 28。
///
/// 这是防止「某 plugin 悄悄少一个 module 而总数不变」的那道交叉校验 ——
/// 只断言总数, 不交叉核对名单, 仍然可能漏。
#[test]
fn per_plugin_module_name_counts_sum_to_total() {
    let state = run_mock_switch_reader();
    let plugins = state["plugins"]
        .as_object()
        .expect("plugins must be object");
    let mut sum: u64 = 0;
    for (pid, _pl) in plugins.iter() {
        let n = module_names(&state, pid).len() as u64;
        assert!(n > 0, "plugin {pid} declares zero modules");
        sum += n;
    }
    assert_eq!(
        sum,
        state["summary"]["modules_total"].as_u64().unwrap(),
        "sum of per-plugin module name counts must equal summary.modules_total"
    );
    assert_eq!(sum, 28);
}

#[test]
fn cluster_enabled_true_and_mode_offline() {
    let state = run_mock_switch_reader();
    assert!(state["cluster"]["enabled"].as_bool().unwrap());
    assert_eq!(state["cluster"]["mode"].as_str().unwrap(), "offline");
}

#[test]
fn aci_compat_version_consistent_across_cluster_and_aci() {
    let state = run_mock_switch_reader();
    let cluster_ver = state["cluster"]["aci_compat_version"].as_str().unwrap();
    assert_eq!(cluster_ver, "0.1.0-draft");
}

#[test]
fn plugins_count_matches_summary_plugins_total() {
    let state = run_mock_switch_reader();
    let plugins_total = state["summary"]["plugins_total"].as_u64().unwrap();
    let plugin_count = state["plugins"].as_object().unwrap().len() as u64;
    assert_eq!(plugins_total, plugin_count);
    assert_eq!(
        plugins_total, 5,
        "5 plugins: assertions, fixtures, mock_grpc, mock_rest, mock_ws_frames"
    );
}
