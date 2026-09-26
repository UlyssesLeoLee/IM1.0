# IM1.0 im-testkit module_switch Stage 1 Regression Report

> **状态**: 🟢 Stage 1 全部 28 module + 5 验收脚本全过 + 13 守门合规
> **日期**: 2026-09-26 JST (per §4.4 brief v0.1 派工触发 reply `01a0db03`)
> **设计稿引用**: [ULYS-190 §4.4 design-analysis v0.4 §4.4](https://github.com/UlyssesLeoLee/Star/blob/agent/minimaxm3/ulys-190-s4-4-brief/docs/architecture/2026-09-22-mock-switches/00-design-analysis.md)
> **brief**: [`docs/briefs/ulys-190-s4-4-im1-testkit-module-switch-stage1.md`](https://github.com/UlyssesLeoLee/Star/blob/agent/minimaxm3/ulys-190-s4-4-brief/docs/briefs/ulys-190-s4-4-im1-testkit-module-switch-stage1.md)

---

## 1. 范围

本报告验证 ULYS-190 §4.4 IM1.0 module_switch stage1 brief v0.1 的 7 交付物:
1. `crates/im-testkit/.aci.json` plugins section modules 全填
2. `crates/im-testkit/.mock-cluster.json` `mock_switch_trace_format` 真拼接
3. `crates/im-testkit/scripts/_lib_mock_switch_im.py` Python helper
4. `crates/im-testkit/tests/im_testkit_module_switch.rs` 单元测试 (10 个)
5. `crates/im-testkit/src/lib.rs` 模块文档 (## module_switch 接入)
6. 本 regression report
7. Star `docs/architecture/2026-09-22-mock-switches/00-design-analysis.md` v0.4 → v0.5 (per brief, 跨 session 续做)

本 stage 不含 #7 (Star design-analysis v0.5 升版), per brief §1.2 — 跨 session 续做.

---

## 2. 28 module 落地清单

| Plugin | Module ID | 来源 (mock_*.rs pub fn) | enabled | mode |
|---|---|---|---|---|
| assertions | `assert_error_code` | `assertions.rs::assert_error_code` 系列 | true | offline |
| assertions | `assert_ws_frame_shape` | `assertions.rs::assert_ws_frame_shape` | true | offline |
| assertions | `assert_json_schema` | `assertions.rs::assert_json_schema` | true | offline |
| fixtures | `tenant` | `fixtures.rs::TenantFixture` | true | offline |
| fixtures | `user` | `fixtures.rs::UserFixture` | true | offline |
| fixtures | `device` | `fixtures.rs::stable_*` 系列 | true | offline |
| fixtures | `session` | `fixtures.rs::SessionFixture` | true | offline |
| fixtures | `message` | `fixtures.rs::MessageFixture` | true | offline |
| mock_grpc | `send_message` | `mock_grpc::send_message_request/response` | true | offline |
| mock_grpc | `list_messages` | `mock_grpc::list_messages_request/response` | true | offline |
| mock_grpc | `validate_access_token` | `mock_grpc::validate_access_token_request/response` (+ `_invalid` variant) | true | offline |
| mock_grpc | `mark_read` | `mock_grpc::mark_read_request/empty_response` | true | offline |
| mock_rest | `login` | `mock_rest::login_request/response` (+ `unauthorized` variant) | true | offline |
| mock_rest | `guest_register` | `mock_rest::guest_register_request/response` | true | offline |
| mock_rest | `create_conversation` | `mock_rest::create_conversation_request/response` (+ `blocked` variant) | true | offline |
| mock_rest | `list_messages_rest` | `mock_rest::list_messages_response` (REST 版) | true | offline |
| mock_rest | `send_message_rest` | `mock_rest::send_message_rest_request/response` (+ `idempotent_replay` variant) | true | offline |
| mock_rest | `presign_media` | `mock_rest::presign_media_request/response` | true | offline |
| mock_rest | `error_responses` | `mock_rest::validation_error_response/not_found_error_response` | true | offline |
| mock_ws_frames | `auth` | `mock_ws_frames::auth_request_frame/json` | true | offline |
| mock_ws_frames | `send_message` | `mock_ws_frames::send_message_frame/json` | true | offline |
| mock_ws_frames | `edit_message` | `mock_ws_frames::edit_message_frame` | true | offline |
| mock_ws_frames | `recall_message` | `mock_ws_frames::recall_message_frame` | true | offline |
| mock_ws_frames | `react` | `mock_ws_frames::react_frame` | true | offline |
| mock_ws_frames | `mark_read` | `mock_ws_frames::mark_read_frame` | true | offline |
| mock_ws_frames | `typing` | `mock_ws_frames::typing_frame` | true | offline |
| mock_ws_frames | `ping` | `mock_ws_frames::ping_frame/json/pong_frame` | true | offline |
| mock_ws_frames | `server_frames` | 12 server frame factory fn 聚合 (connected/ack/message_*/presence/typing_broadcast/force_disconnect/validation_error) | true | offline |

**总计**: 28 module (per 设计稿 v0.4 §4.4 + brief §3 命名锁定)

---

## 3. 5 验收脚本结果

| § | 验收项 | 命令 | 结果 |
|---|---|---|---|
| §4.1 | `cargo fmt --check` | `cargo fmt -p im-testkit --check` | ✅ 干净 |
| §4.2 | `cargo check -p im-testkit --all-targets` | `cargo check -p im-testkit --all-targets` | ✅ 干净 (2m 52s, 包含 im-testkit + deps) |
| §4.3 | `cargo clippy -p im-testkit --all-targets -- -D warnings` | `cargo clippy -p im-testkit --all-targets -- -D warnings` | ✅ 干净 (0 warnings) |
| §4.4 | `cargo test -p im-testkit --all-targets` | `cargo test -p im-testkit --all-targets` | ✅ 97 passed / 0 failed (61 unit + 3 IT + 10 module_switch + 23 smoke) |
| §4.5 | `mock-switch-validate.py validate-one im1` (Star 已 ship) | `python tools/mock-switch-validate.py validate-one --cluster-config D:/IM1.0/crates/im-testkit/.mock-cluster.json --aci-config D:/IM1.0/crates/im-testkit/.aci.json` | ✅ CLUSTER=OK + ACI=OK |

### 3.1 module_switch 单元测试 (10 项, 全部 PASS)

```
running 10 tests
test mock_rest_has_7_modules ... ok
test cluster_enabled_true_and_mode_offline ... ok
test assertions_plugin_has_3_modules_unchanged_from_pr23 ... ok
test aci_compat_version_consistent_across_cluster_and_aci ... ok
test module_switch_all_enabled_by_default ... ok
test mock_ws_frames_has_9_modules_including_server_frames_aggregation ... ok
test fixtures_plugin_has_5_modules_unchanged_from_pr23 ... ok
test mock_grpc_has_4_modules ... ok
test plugins_count_matches_summary_plugins_total ... ok
test module_switch_total_count_is_28 ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
```

---

## 4. mock_switch_trace_format 真拼接 (per brief §4)

### 4.1 PR #23 ship 的静态 trace

```
"mock_switch_trace_format": "cluster.enabled={cluster.enabled}, cluster.mode={cluster.mode}, plugins=[assertions,fixtures,mock_grpc,mock_rest,mock_ws_frames], modules=per_plugin (TBD)"
```

### 4.2 本 stage 落地的真拼接 (per §3.4 + G-MS-08)

```
"mock_switch_trace_format": "cluster.enabled={cluster.enabled},mode={cluster.mode},plugins=[assertions(3m),fixtures(5m),mock_grpc(4m),mock_rest(7m),mock_ws_frames(9m)]=28/28 modules"
```

`_lib_mock_switch_im.py` 输出 trace (Python):

```text
$ python crates/im-testkit/scripts/_lib_mock_switch_im.py crates/im-testkit
{ ... JSON state ... }
---
trace: cluster.enabled=True,mode=offline,plugins=[assertions(3m),fixtures(5m),mock_grpc(4m),mock_rest(7m),mock_ws_frames(9m)]=28/28 modules
```

trace 长度 ~120 字, 超 G-MS-08 推荐 ~80 字, 跨 session 截断 (per brief §6 G-MS-BRIEF-S44-02).

---

## 5. 守门合规 (per AGENTS.md §4)

| 守门 | 检查 | 结果 |
|---|---|---|
| #1 docs 必含 | 本 regression report + lib.rs 文档 + Star design v0.5 (跨 session) | ✅ |
| #3 5 域 Lead | D-Boy 「a」 reply `01a0db03` = 派工触发 override | ✅ (per 守門 #15 v3) |
| #5 env hard ban | 0 影响 | ✅ |
| #6 PowerShell only | 0 bash `&&` in committed files | ✅ |
| #7 0 unsafe | 0 unsafe Rust 代码 | ✅ |
| #9 subprocess | `_lib_mock_switch_im.py` 跨 Rust/Python subprocess 读 | ✅ |
| #10 author=Ulysses | commit author 统一 | ✅ |
| #11 缺標比錯標 | brief §6 列 5 已知缺口 (G-MS-BRIEF-S44-01..05) | ✅ |
| #12 docs 同步 | lib.rs + .aci.json + .mock-cluster.json + regression report 4 处 | ✅ |
| #13 W/T/M | module_switch 配置属 Master SCD-2 | ✅ |
| #14 v4 Mavis 审核 | author=Ulysses, IM1.0 repo brief 走 Mavis 审核 | ✅ |
| #15 1 sub-agent 1 切点 | 本 stage 1 sub-agent, 1 repo (IM1.0), 1 brief, 1 commit (跨 session PR #24 merge 后) | ✅ |
| #19 v19 conflict-of-interest | 0 conflict | ✅ |
| #20 dispatcher brief | brief v0.1 跟 §4.1 §4.2 範式一致 | ✅ |
| #24 docs 同步 | lib.rs + README + .aci.json + .mock-cluster.json + regression report 5 处同步 | ✅ |

---

## 6. 已知缺口 (per 守門 #11)

- **G-MS-BRIEF-S44-01**: `_lib_mock_switch_im.py` 是 Python, IM1.0 主仓是 Rust — 跨语言调用通过 subprocess (per 守門 #9). Rust native 版本跨 session.
- **G-MS-BRIEF-S44-02**: `mock_switch_trace_format` 拼接 ~120 字, 超 G-MS-08 推荐 ~80 字. 截断跨 session.
- **G-MS-BRIEF-S44-03**: `mock_ws_frames` 的 server frame group (12 frame) 归到 `server_frames` 1 module, 简化粒度但失去 frame 级开关. 跟设计稿 §3.4 粒度原则有 trade-off, 跨 session 评估.
- **G-MS-BRIEF-S44-04**: `_lib_mock_switch_im.py` 不支持 hot reload (per G-MS-09 v0.1 不做), 改 `.aci.json` 后需重跑 test.
- **G-MS-BRIEF-S44-05**: 28 module 的命名跟 aux-13 协议 frame 名一致性, 跨项目 (Star/RGS/CATs/IDE1.0/GitGit) 是否沿用此 convention 待 §4.4 stage2-7 验证 (G-MS-04 跨项目 plugin_id/module_id 命名锁版).

---

## 7. 跨 session 续做入口

1. 🟡 **§4.4 stage2-7** — 6 项目 (Star/RGS/CATs/IDE1.0/GitGit/Ada[降級]) module_switch 扩展, 每项目 1 brief, 共 ~5-6M tokens
2. 🟡 **G-MS-08 mock_switch_trace 截断到 ~80 字** (本 stage ~120 字, 跨 session)
3. 🟡 **G-MS-05 開關變更審計日誌** (Transaction audit SCD-2, 跨 session)
4. 🟡 **G-MS-09 hot reload** (v0.2 評估, 跨 session)
5. 🟡 **Rust native `_lib_mock_switch.rs`** (跟 IM1.0 `_lib_mock_switch_im.py` 模板对齐, 跨 session, 替换 Python subprocess)
6. 🟡 **CI `mock-switch-validate` 验证 module_switch 字段** (Star ship 时只校验 cluster, 加 module 级校验, 跨 session)
7. 🟡 **Star design-analysis v0.4 → v0.5** 加 §10 module_switch 落地回顧 (per brief §1.1 item 7, 跨 session)
8. 🟡 **Layer 1→Layer 2→Layer 3 贯通验收** (per AGENTS.md §3, 顶层 sub-task)
