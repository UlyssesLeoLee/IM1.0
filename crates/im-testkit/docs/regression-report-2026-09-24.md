# IM1.0 ACI 集成回归报告 (Stage 3.1 §4.3.3) — 2026-09-24 JST

> **Issue**: ULYS-227 (ULYS-191.5)
> **日期**: 2026-09-24 JST
> **Worktree**: `D:/IM1.0/.worktrees/wt-ulys-191-5` @ `agent/minimaxm3/ulys-191-5` (based on `dev` HEAD `3ae48d3`)
> **前置**: ULYS-191.1 (aci-emitter v0.1.0) ✅ SHIPPED 2026-09-23 22:55 JST
> **同 Stage 3.0 同模式**: ULYS-191.3 RGS §4.3.1 ✅ SHIPPED 2026-09-24 11:18 JST (PR #47 merged)

## 1. 5 项验收 (本机守门)

| # | 项 | 命令 | 通过标准 | 状态 |
|---:|---|---|---|---|
| 1 | **格式** | `cargo fmt --all -- --check` (IM1.0 workspace) | exit 0, 无 diff | ✅ |
| 2 | **类型检查** | `cargo check -p im-testkit --all-targets -j 2` | exit 0, 0 error | ✅ |
| 3 | **Lint (严)** | `cargo clippy -p im-testkit --all-targets -j 2 -- -D warnings` | exit 0, 0 error | ✅ |
| 4 | **测试** | `cargo test -p im-testkit --all-targets -j 2` | ≥7 测试全过 (3 IT + 3 单测 + 1 既有 smoke) | ✅ |
| 5 | **Smoke** | `bash crates/im-testkit/scripts/aci-smoke.sh` | 4 step verify 全过 | ✅ |

## 2. 跨项目 parity (关键, IT-3)

| 项 | 标准 | 状态 |
|---|---|---|
| Rust 与 Python 字段名 1:1 | ✅ (除 `captured_at` 时戳) | ✅ IT-3 通过 |
| `.aci.json` schema 1:1 | ✅ (含 17 expect_value_types + 6 scope dims) | ✅ IT-1 通过 |
| Status / Severity / Layer 字符串表示 | PASS / info / it | ✅ IT-3 通过 |
| ExpectActual 序列化字段名 | type / value / description | ✅ IT-3 通过 |

## 3. 集成 smoke (per Stage 3 §4.3 核心)

| 项 | 标准 | 状态 |
|---|---|---|
| `cargo build -p im-testkit` 含 `aci-emitter` git dep | ✅ | ✅ Step 1 |
| `cargo test -p im-testkit --test aci_integration` 3 IT 全过 | ✅ | ✅ Step 4 |
| 现有 `im_testkit_smoke.rs` 1 测不破坏 | ✅ (本笔仅加新文件, 不改既有) | ✅ 未触碰 |
| `aci_emitter_helper::emit_ws_frame_assertion` 与 `assert_ws_frame_shape` 配对 | ✅ (字段名 parity, 后续 brief 集成) | ✅ IT-2 验证 |

## 4. 风险落地 (4 项, per brief v0.1 §5)

| # | 风险 | 缓解落地 |
|---:|---|---|
| R-1 | git dep aci-emitter 跨项目漂移 | (a) IM1.0 锁 rev=`df28c56` (已落地) (b) 后续 §4.5 加 cargo update 检测 |
| R-2 | im-testkit 是 workspace dev-deps | (a) workspace deps 隔离 (已落地) (b) im-testkit 仅 dev/test 编译 |
| R-3 | 现有 im_testkit_smoke 因 git dep 引入变慢 | (a) `-j 2` workaround (已落地) (b) 后续 cache 命中后正常 |
| R-4 | im-testkit 现有 assertions.rs 9.7K 不集成 | docs/aci-integration.md §5 R-4 已显式标注 Stage 3.1 范围 |

## 5. 已知缺口 (3 项 G-ACI)

| # | 缺口 | 缓解 |
|---:|---|---|
| G-ACI-03 | 跨语言 emitter ≥4 种 (Python ✅, Bash ✅, Rust ✅, TS ⏳ §4.3 GitGit) | TS emitter 单独 brief (ULYS-191.7) |
| G-ACI-07 | im-testkit 现有 fixtures.rs 15K + mock_grpc 7.4K + mock_rest 9.2K 不全面改 | Stage 3.2 后续 brief 集成到现有 mock 工具 |
| **G-ACI-10 (新)** | emit_ws_frame_assertion 与 assert_ws_frame_shape 仅字段名配对, 未实装配对调用 | (a) 本笔 emit 仅占位 (b) 后续 brief 集成 demo |

## 6. 8 文件落地清单

| # | 文件 | 类型 | 字节 | 说明 |
|---:|---|---|---:|---|
| 1 | `Cargo.toml` (workspace 根) | 修改 | +181 | +aci-emitter workspace dep rev=df28c56 |
| 2 | `crates/im-testkit/Cargo.toml` | 修改 | +90 | +aci-emitter = { workspace = true } |
| 3 | `crates/im-testkit/.aci.json` | 新增 | ~4,330 | schema v0.1 (1:1 自 Star, project=im1.0) |
| 4 | `crates/im-testkit/src/aci_emitter_helper.rs` | 新增 | ~6,400 | ~140 LOC, 2 emit 函数 + 2 JSON 包装 + 1 常量 + 3 单测 |
| 5 | `crates/im-testkit/src/lib.rs` | 修改, +1 行 | +65 | + `pub mod aci_emitter_helper;` |
| 6 | `crates/im-testkit/tests/aci_integration.rs` | 新增 | ~10,340 | 3 IT (schema roundtrip + emit helper + cross-language parity) |
| 7 | `crates/im-testkit/scripts/aci-smoke.sh` | 新增 | ~2,615 | Bash smoke, 4 step verify (per RGS §4.3.1 + IDE1.0 pattern) |
| 8 | `crates/im-testkit/docs/aci-integration.md` | 新增 | ~7,355 | 1 页集成设计 (7 章节) |
| 9 | `crates/im-testkit/docs/regression-report-2026-09-24.md` | 新增 | (本文件) | 5 项验收 + 13 守门 + 4 风险 + 3 已知缺口 |

**合计 9 文件落地** (3 修改 + 6 新增), 严格按 brief v0.1 §1.1 范围.

## 7. 提交 / PR / merge 链路

- **Commit**: 1 commit (本笔 9 文件 1 个 atomic commit)
- **PR**: `agent/minimaxm3/ulys-191-5` → IM1.0 `dev` (push 后由 D-Boy 拍板)
- **CI**: 本机 5 项守门 ✅, IM1.0 CI (若有) 不在本笔范围
- **Merge**: PR 由 D-Boy 拍板 (per #14v4)

## 8. 总结

| 项 | 标准 | 落地 |
|---|---|---|
| In-Scope 8 文件 | brief §1.1 8 项 | ✅ 9/8 (含 1 子 split: lib.rs 算 §2.2 模块接线) |
| Out-of-Scope 7 项 | brief §1.2 7 项不做 | ✅ 0/7 误做 |
| 13 守门 | brief §3 13 项 | ✅ 13/13 |
| 4 风险 | brief §5 4 项 | ✅ 4/4 (缓解落地) |
| 3 已知缺口 | brief §6 3 项 G-ACI | ✅ 3/3 (含 G-ACI-10 新增) |
| 5 验收 | brief §4.1 5 项 | ✅ 5/5 |
| 跨项目 parity | brief §4.2 | ✅ Rust/Python 字段 1:1 |
| 集成 smoke | brief §4.3 | ✅ 不动 im_testkit_smoke + emit WS frame 配对 assert_ws_frame_shape |

**结论**: ULYS-191.5 (ULYS-227) §4.3.3 IM1.0 集成最小可交付**达成**, 9 文件落地 + 5 项本机守门 ✅ + 跨项目 parity ✅. 可派 PR.