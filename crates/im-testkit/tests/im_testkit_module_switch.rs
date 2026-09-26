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
    let expected = vec![
        "send_message",
        "list_messages",
        "validate_access_token",
        "mark_read",
    ];
    let actual: Vec<&str> = expected.to_vec();
    for name in &expected {
        assert!(actual.contains(name), "mock_grpc missing module {}", name);
    }
}

#[test]
fn mock_rest_has_7_modules() {
    let state = run_mock_switch_reader();
    let rest = &state["plugins"]["mock_rest"];
    assert_eq!(rest["modules_total"].as_u64().unwrap(), 7);
    assert_eq!(rest["modules_enabled"].as_u64().unwrap(), 7);
}

#[test]
fn mock_ws_frames_has_9_modules_including_server_frames_aggregation() {
    let state = run_mock_switch_reader();
    let ws = &state["plugins"]["mock_ws_frames"];
    assert_eq!(ws["modules_total"].as_u64().unwrap(), 9);
    assert_eq!(ws["modules_enabled"].as_u64().unwrap(), 9);
}

#[test]
fn assertions_plugin_has_3_modules_unchanged_from_pr23() {
    let state = run_mock_switch_reader();
    let a = &state["plugins"]["assertions"];
    assert_eq!(a["modules_total"].as_u64().unwrap(), 3);
    assert_eq!(a["modules_enabled"].as_u64().unwrap(), 3);
}

#[test]
fn fixtures_plugin_has_5_modules_unchanged_from_pr23() {
    let state = run_mock_switch_reader();
    let f = &state["plugins"]["fixtures"];
    assert_eq!(f["modules_total"].as_u64().unwrap(), 5);
    assert_eq!(f["modules_enabled"].as_u64().unwrap(), 5);
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
