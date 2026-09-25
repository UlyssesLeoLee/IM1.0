# IM1.0 ACI 集成设计 v0.1

> **Issue**: ULYS-191 §4.3.3 (ULYS-227, ULYS-191.5)
> **日期**: 2026-09-24 JST
> **前置**: ULYS-191.1 (aci-emitter v0.1.0) ✅ SHIPPED 2026-09-23 22:55 JST
> **同 Stage 3.0 同模式**: ULYS-191.3 RGS §4.3.1 ✅ SHIPPED 2026-09-24 11:18 JST (PR #47 merged)

## 1. 背景

**目的**: 为 **IM1.0** 项目落地 `aci-emitter v0.1.0` 集成 — 在现有 `crates/im-testkit/` (5 项目 mock 框架中**最成熟**, 7 src + 1 test + assertions.rs 9.7K + fixtures.rs 15K) 加 `.aci.json` schema + emit ACI assertion 的最小可交付件.

**关键验证点** (§4.3 5 项目跨项目集成之第 3 个):
- 跨 monorepo (IM1.0 9-crate workspace) 接 git dep `aci-emitter`
- 加 `.aci.json` schema 1:1 拷贝 + emit 2 条 sample assertion (smoke + WS frame)
- 与 im-testkit 现有 `assertions::assert_ws_frame_shape` 配对 (字段名 parity, 后续 brief 实装集成)
- 跑通现有 `im_testkit_smoke.rs` 不破坏

**与 ULYS-191.3 RGS 差异**:
- RGS: standalone workspace `tools/rgs-flash-mock/` (独立 1 crate)
- **IM1.0: 9-crate workspace 子 crate `crates/im-testkit/`** (dev-only, 不发布)
- RGS emit 1 个 `emit_smoke_assertion()`; IM1.0 emit 2 个 (smoke + WS frame, 与 im-testkit 现有 `assert_ws_frame_shape` 配对)

## 2. 集成路径

### 2.1 git dep 锁定

```toml
# Cargo.toml (workspace 根)
# ACI 跨项目 emitter (per ULYS-191 §4.3.3 brief v0.1)
# 守门 #11 缺标比错标: git dep 锁 rev=df28c56 (= ULYS-191.1 / ULYS-224 main HEAD)
aci-emitter = { git = "https://github.com/UlyssesLeoLee/aci-emitter", rev = "df28c56" }

# crates/im-testkit/Cargo.toml
[dependencies]
aci-emitter = { workspace = true }
```

### 2.2 模块接线

- `src/aci_emitter_helper.rs` (~140 LOC): 2 个公开函数 `emit_smoke_assertion()` + `emit_ws_frame_assertion(kind)` + 2 个 JSON 包装 (`emit_smoke_assertion_json()` + `emit_ws_frame_assertion_json(kind)`) + 1 个常量 `REQUIRED_FIELDS`
- `src/lib.rs` 加 `pub mod aci_emitter_helper;`
- `tests/aci_integration.rs` 3 个 IT (schema roundtrip + emit helper + cross-language parity)
- `scripts/aci-smoke.sh` 4 step verify (调 cargo test 间接 emit)
- `docs/aci-integration.md` (本文件) + `docs/regression-report-2026-09-24.md`

### 2.3 `.aci.json` schema

1:1 拷贝自 Star `tools/star-flash-mock/.aci.json` v0.1 (4,327 B), `project` 字段重命名为 `im1.0`:
- 17 个 `expect_value_types`
- 6 个 `scope_dimensions` (project / module / domain / subdomain / operation / http_method)
- 10 个 `schema_required_fields` (assertion_id / aci_version / layer / scope / expect / actual / status / severity / reasoning / captured_at)

### 2.4 WS frame 集成点

`emit_ws_frame_assertion(kind)` 与 im-testkit 现有 `assertions::assert_ws_frame_shape(frame, kind)` 字段名配对:

| emit (per ACI v0.1)             | assert (im-testkit)        | 配对语义 |
|---------------------------------|---------------------------|---|
| `assertion_id = "im1.0:ws:connected"` | `assert_ws_frame_shape(frame, "connected")` | ID 前缀 + kind 一致 |
| `scope.project = "im1.0"`      | implicit (test ctx)        | scope 维度 |
| `expect.value = "connected"` (field_equals) | frame.kind == "connected" | expect value == assert param |
| `actual.value = "connected"` (field_equals) | frame.kind 实际值      | actual 测得 |

> **注**: emit + assert 配对实装待 Stage 3.2 后续 brief. 本笔仅字段名 parity.

## 3. CI 跨项目验证

### 3.1 本笔 (本机守门)

5 项验收 (per brief v0.1 §4):
1. `cargo fmt --all -- --check` (IM1.0 workspace) — exit 0
2. `cargo check -p im-testkit --all-targets -j 2` — exit 0
3. `cargo clippy -p im-testkit --all-targets -j 2 -- -D warnings` — exit 0
4. `cargo test -p im-testkit --all-targets -j 2` — ≥7 测试全过 (3 IT + 3 单测 + 1 既有 smoke)
5. `bash crates/im-testkit/scripts/aci-smoke.sh` — 4 step verify 全过

### 3.2 跨项目 CI (后续 §4.5)

- IM1.0 CI 在 integration job 加 `cargo update -p aci-emitter` 检测上游漂移
- 上游 `aci-emitter` release tag 通知
- §4.5 跨项目 CI 落地后, 改 `crates.io` publish 或 path 依赖

## 4. 未来扩展

| Stage | 范围 | 备注 |
|---|---|---|
| §4.3.1 | RGS 集成最小可交付 | ✅ ULYS-226 (SHIPPED) |
| §4.3.2 | CATs 集成 (接 cats-mock) | ULYS-228 / 后续 brief |
| §4.3.3 | **IM1.0 集成 (接 im-testkit, 最成熟)** | ✅ ULYS-227 (本笔) |
| §4.3.4 | Ada 集成 (接 ada-mock) | ULYS-229 / 后续 brief |
| §4.3.5 | GitGit 集成 (TS emitter) | ULYS-230 / 后续 brief |
| §4.3.6+ | IM1.0 全量集成 (现有 7 mock 工具 + assertions.rs 9.7K 加 aci_assertion) | Stage 3.2 后续 brief |

## 5. 风险 (4 项)

| # | 风险 | 缓解 |
|---:|---|---|
| R-1 | **git dep aci-emitter 跨项目漂移**: IM1.0 锁 rev=`df28c56`, 上游 master 推进时 IM1.0 dev 分支可能用旧版本 | (a) IM1.0 CI 不主动 update (b) §4.5 跨项目 CI 落地 |
| R-2 | **im-testkit 是 workspace dev-deps (per Cargo.toml description "仅 dev 用,不发布")**: 本笔 aci-emitter 加进 im-testkit 不会传 IM1.0 release build | (a) workspace deps 隔离 (b) im-testkit 仅 dev/test 编译 |
| R-3 | **现有 `im_testkit_smoke.rs` 1 测可能因 git dep 引入变慢**: cargo build 网络 fetch 第一次会慢 | (a) `-j 2` workaround (b) 后续 cache 命中后正常 |
| R-4 | **im-testkit 现有 assertions.rs 已成熟 (9.7K)**: 用户可能误以为已"完整 ACI 集成", 但实际本笔只加独立 helper, 不与现有 assertions.rs 集成 | README + docs 显式标注 "Stage 3.1 最小可交付, 与现有 assertions.rs 集成待 Stage 3.2+" |

## 6. 已知缺口 (3 项 G-ACI)

| # | 缺口 | 缓解 |
|---:|---|---|
| G-ACI-03 | 跨语言 emitter ≥4 种 (Python ✅, Bash ✅, Rust ✅, TS ⏳ §4.3 GitGit). 本笔 IM1.0 用 Rust, 不引入新语言 | ⏳ TS emitter 单独 brief (ULYS-191.7) |
| G-ACI-07 | im-testkit 现有 fixtures.rs 15K + mock_grpc 7.4K + mock_rest 9.2K 不全面改 | ⏳ Stage 3.2 后续 brief 集成到现有 mock 工具 |
| **G-ACI-10 (新)** | **emit_ws_frame_assertion 与 assert_ws_frame_shape 仅字段名配对, 未实装配对调用**: 后续 brief 在 im_testkit_smoke.rs 加 emit + assert 配对 demo | (a) 本笔 emit 仅占位 (b) 后续 brief 集成 demo |

## 7. 守门对齐 (13 项)

| 守门 | 本笔落地 |
|---|---|
| #5 no secret leak | IM1.0 0 secret (.env 不动, design docs 已有) |
| #6 中文默认 | docs 全中文 |
| #7 `unsafe_code="forbid"` | im-testkit/Cargo.toml L8 已设, 本笔不动 |
| #9 subprocess | aci-smoke.sh 调 `cargo build/test`, 不调任意用户脚本 |
| #10 author=Ulysses | `git -c user.name=Ulysses -c user.email=ulysses@mavis.local commit ...` |
| #11 缺标比错标 | 1 git dep `aci-emitter` 锁 rev=`df28c56`, workspace deps 复用现有 |
| #12 docs 同步 | 1 aci-integration.md + 1 regression-report.md 随代码 ship |
| #13 W/T/M | 单元 (aci_emitter_helper 3 单测) + 集成 (3 IT) + 系统 (aci-smoke.sh) |
| #14v4 PR merge | 1 commit → IM1.0 dev → CI (本地不触发 CI, 视 IM1.0 CI 配置) → D-Boy 拍板 |
| #15 scope creep | 1 sub-agent 1 切点 (本笔 = IM1.0 集成最小, 不含 CATs/Ada/GitGit) |
| #17 commit 完整 | 1 commit 含 8 文件 |
| #19v19 Python 化 | IT-3 跨语言 parity 测 |
| #24 vendor 中立 | 1 git dep aci-emitter (自家) + 复用现有 workspace deps |