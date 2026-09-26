//! im-testkit: IM1.0 共享 mock + fixture + assertion 库
//!
//! 依据: `docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md`
//!
//! ## 模块
//! - [`mock_ws_frames`] — 12 个 WS 帧 mock 数据(per aux-13 §1)
//! - [`mock_grpc`] — 4 个 gRPC RPC mock 数据(per aux-13 §2)
//! - [`mock_rest`] — 7 个 REST endpoint mock 数据(per aux-13 §3)
//! - [`fixtures`] — 共享测试数据 builder(tenant / game / environment / user / device / session / conversation / message)
//! - [`assertions`] — IM1.0 特定断言(ErrorCode / WS frame shape / JSON schema)
//! - [`server`] — mock IM server(actix-web,供 im-gateway 集成测试用)
//!
//! ## 使用约束
//! - **仅作 dev-dependency 使用,不发布** — 在其他 crate 的 `[dev-dependencies]` 加 `im-testkit = { workspace = true }`
//! - 字段命名严格对齐 im-core / im-protocol 公开 API;若 im-core 字段调整,本 crate 同步更新
//! - 引用 aux-13 协议时**只读不写**;aux-13 任何变更需走 DDD 流程,本 crate 不自行修改
//!
//! ## DDD Review 必查项
//! - 字段命名 vs im-core 一致性
//! - 错误码字符串 vs aux-03 §B 一致性
//! - 帧 `type` 字段 vs aux-13 §1 一致性
//!
//! ## 已知缺口(2026-08-31 v0.1)
//! - aux-13 §1 12 帧: 6 帧已严格按文档实现(`ping` / `pong` / `auth` / `connected` / `ack` / `message_new`),
//!   剩余 6 帧(`send_message` / `edit_message` / `recall_message` / `react` / `mark_read` / `typing`)
//!   仅做占位 + 测试用回环(per `git log -p --follow` aux-13 引用历史)
//! - mock struct 字段可能跟 im-core 真实字段不完全对齐(如 im-core 加新字段时 mock 未同步)
//! - mock server 未做性能基准
//! - 引用 im-proto 的 protobuf 类型 vs serde JSON 类型——当前 mock 主要用 serde JSON,未实测 protobuf wire format
//! - miri / criterion 未做

//! ## module_switch 接入 (per ULYS-190 §4.4 brief v0.1)
//!
//! IM1.0 im-testkit 通过 `.aci.json` 的 `plugins.<plugin_id>.modules.<module_id>`
//! 三层开关暴露 28 个 module_switch (assertions 3 + fixtures 5 + mock_grpc 4 +
//! mock_rest 7 + mock_ws_frames 9)。读法:
//!
//! ```text
//! 跨语言 dispatch:
//!   python crates/im-testkit/scripts/_lib_mock_switch_im.py crates/im-testkit
//! ```
//!
//! Rust native 版本跨 session (per G-MS-BRIEF-S44-01); 当前 Python helper 通过
//! subprocess 暴露 (per AGENTS.md 守门 #9)。CI gate `mock-switch-validate.py
//! validate-one im1` (Star ship) 验证 28 module 全部 enabled + cluster_ok=True.
//!
//! mock_switch_trace_format (per .mock-cluster.json) 输出例:
//!   `cluster.enabled=True,mode=offline,plugins=[assertions(3m),fixtures(5m),mock_grpc(4m),mock_rest(7m),mock_ws_frames(9m)]=28/28 modules`
//!
//! 注意: trace 字段约 ~120 字, 超 G-MS-08 推荐 ~80 字, 跨 session 截断; 本
//! stage 不做 (per brief §6 G-MS-BRIEF-S44-02)。

pub mod aci_emitter_helper;
pub mod assertions;
pub mod fixtures;
pub mod mock_grpc;
pub mod mock_rest;
pub mod mock_ws_frames;
pub mod server;

// 重新导出常用类型
pub use im_common::{AppError, AppResult, ErrorCode};
pub use im_protocol::{
    content::MessageContent,
    error_body::{ErrorBody, FieldError},
    ws_frames::{AckData, ClientFrame, ServerFrame, WireMessage},
};
