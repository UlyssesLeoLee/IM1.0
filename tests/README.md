# tests/ — IM1.0 集成 / 端到端测试资产

> 责任方: **Worker (Mavis 接手 agent per DEC-008)**
> 创建: 2026-08-31 JST, 响应 Ulysses 16:26 JST 指令
> 协议源: [`docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md`](../docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md) v1.1.0 (752 行, `[PROTOCOL-FROZEN]` 2026-08-26 JST)

## 1. 范围与定位

本目录提供 **跨 crate 的集成 / 端到端测试资产**:

- **mock 数据**: 协议帧 / REST / gRPC 的 JSON 样例,SQL fixture 行
- **测试脚本**: PowerShell (Windows) + Bash (WSL/Git Bash/macOS/Linux)
- **不包含**: Rust 代码 (Test-1 worker 已建 `crates/im-testkit`); CI 编排 (Test-4)

## 2. 目录树

```
tests/
├── README.md                       ← 本文件
├── data/
│   ├── ws_frames/                  ← 12 个 WS 帧 (per aux-13 §1)
│   ├── grpc/                       ← 4 个 gRPC RPC JSON 形态 (token_exchange, device_register)
│   ├── rest/                       ← 7 个 REST 请求/响应 (login/refresh/logout/user_get)
│   └── sql/                        ← 6 个 SQL fixture (per migrations/0001-0006)
├── scripts/
│   ├── run_ut.ps1 / run_ut.sh      ← [ULYS-138] UT 编排: cargo test --lib, 不需 PG
│   ├── run_it.ps1 / run_it.sh      ← [ULYS-138] IT 编排: PG 集成测试 + summary
│   ├── run_all_tests.ps1           ← [ULYS-138] 升级为 UT→IT→ST 三层编排
│   ├── run_all_tests.sh            ← [ULYS-138] POSIX 等价物
│   ├── generate_regression_report.ps1 ← [ULYS-138] 离线汇总 ut/it/st summary
│   ├── setup_test_db.ps1           ← 起 PG test db + migration + fixture
│   ├── teardown_test_db.ps1        ← 删 test db (带 im1test_ 前缀安全护栏)
│   ├── start_im_gateway_mock.ps1   ← 启动 im-gateway 连 test db
│   ├── stop_im_gateway.ps1         ← 停止 im-gateway
│   ├── wscat_examples.sh           ← WS 端点手工探针 (WSL/Git Bash)
│   └── curl_examples.sh            ← REST 端点手工探针 (WSL/Git Bash)
├── logs/                           ← [ULYS-138] 各层日志落盘 (ut-*.log / it-*.log / st-*.log / regression-*.log)
└── reports/                        ← [ULYS-138] 回归报告落盘 (regression-<stamp>.{md,json})
```

## 3. 文件清单 (37 个)

### 3.1 WS 帧 (12 个 JSON)

| 文件 | 帧类型 | 方向 | aux-13 章节 |
|---|---|---|---|
| `ping.json` | `ping` | C→S | §1.1.8 |
| `pong.json` | `pong` | S→C | §1.2.11 |
| `auth_request.json` | `auth` | C→S | §1.1.1 |
| `auth_response.json` | `connected` | S→C | §1.2.1 |
| `message_new.json` | `message_new` | S→C | §1.2.5 |
| `message_ack.json` | `ack` | S→C | §1.2.2 |
| `presence_update.json` | `presence_update` | S→C | §1.2.9 |
| `typing.json` | `typing` | S→C | §1.2.10 |
| `friend_request.json` | `friend_request` | C→S | (派生 / DetailedDesign §4.6) |
| `conversation_create.json` | `conversation_create` | C→S | (派生 / REST §3.3 WS 对偶) |
| `conversation_join.json` | `conversation_join` | C→S | (派生 / conversation_members 0004) |
| `conversation_leave.json` | `conversation_leave` | C→S | (派生 / conversation_members 0004) |

> 注: aux-13 §1 总共定义 18 个 WS 帧(8 C→S + 10 S→C);本目录提供 IM1.0 Day-3 测试套件所需的 **12 个核心帧**。其余 6 帧(`edit_message` / `recall_message` / `react` / `mark_read` / `message_edited` / `message_recalled` / `reaction_added` / `force_disconnect`)在 im-testkit crate 内已有等效 fixture。

### 3.2 gRPC 帧 (4 个 JSON)

| 文件 | RPC | 协议参考 |
|---|---|---|
| `token_exchange_request.json` | TokenExchange | aux-13 §3.1 REST body over gRPC |
| `token_exchange_response.json` | TokenExchange | aux-13 §3.1 200 body |
| `device_register_request.json` | DeviceRegister | 派生 / device_sessions 0002 |
| `device_register_response.json` | DeviceRegister | 派生 / device_sessions 0002 |

> proto 源: `crates/im-proto/proto/core.proto` (package `im.core.v1`)

### 3.3 REST 帧 (7 个 JSON)

| 文件 | 端点 |
|---|---|
| `login_request.json` / `login_response.json` | POST /v1/auth/token/exchange (aux-13 §3.1) |
| `refresh_request.json` / `refresh_response.json` | POST /v1/auth/refresh |
| `logout_request.json` | POST /v1/auth/logout |
| `user_get_request.json` / `user_get_response.json` | GET /v1/users/{id} |

### 3.4 SQL Fixture (6 个)

| 文件 | 对应 migration | 表 |
|---|---|---|
| `tenant_fixture.sql` | 0001 | tenants / games / environments |
| `user_fixture.sql` | 0002 | users / device_sessions |
| `friend_fixture.sql` | 0003 | friend_requests / friendships |
| `conversation_fixture.sql` | 0004 | conversations / sequences / members / dm_pairs |
| `message_fixture.sql` | 0005 | messages / reactions |
| `audit_log_fixture.sql` | 0006 | audit_logs |

### 3.5 脚本 (12 个, [ULYS-138] 升级)

ULYS-138 把原有 5 个 PS1 + 2 个 sh 升级到 12 个 (PS1 + sh 配对) + 1 个报告生成器:

| 文件 | 平台 | 用途 | 阶段 |
|---|---|---|---|
| `run_ut.ps1` / `run_ut.sh` | Win / POSIX | cargo test --workspace --lib, 不需 PG | UT |
| `run_it.ps1` / `run_it.sh` | Win / POSIX | cargo test --workspace --tests, 需 PG | IT |
| `run_all_tests.ps1` | Win | UT→IT→ST 三层编排 + 回归报告 | ST |
| `run_all_tests.sh` | POSIX | POSIX 等价 run_all_tests.ps1 | ST |
| `generate_regression_report.ps1` | Win | 离线汇总 ut/it/st summary 成 md | — |
| `setup_test_db.ps1` | Win | 起 test db + 跑 migration + 灌 fixture | DB |
| `teardown_test_db.ps1` | Win | 删 test db (护栏: 仅删 `im1test_*`) | DB |
| `start_im_gateway_mock.ps1` | Win | 启动 im-gateway 连 test db | ST |
| `stop_im_gateway.ps1` | Win | 停 im-gateway | ST |
| `wscat_examples.sh` | POSIX | WS 端点手工探针 | (manual) |
| `curl_examples.sh` | POSIX | REST 端点手工探针 | (manual) |

**回归测试调用链**:

```
run_all_tests.{ps1,sh}
    ├── Layer 1: run_ut.{ps1,sh}    →  cargo test --workspace --lib
    │      └── 落盘: ut-<stamp>.log + ut-summary-<stamp>.txt
    ├── Layer 2: run_it.{ps1,sh}    →  cargo test --workspace --tests --test-threads=1
    │      └── 落盘: it-<stamp>.log + it-summary-<stamp>.txt
    └── Layer 3: ST (in-script)
           └── 落盘: st-<stamp>.log

最终: tests/logs/regression-<stamp>.log
     + tests/reports/regression-<stamp>.md   (人类可读)
     + tests/reports/regression-<stamp>.json (机器可读)
```

## 4. 快速上手

### 4.0 单层运行 (新, [ULYS-138])

```bash
# UT (不需 PG, 最快, ~秒级)
bash tests/scripts/run_ut.sh                                       # POSIX
pwsh tests/scripts/run_ut.ps1                                     # Windows

# IT (需 PG; 默认连 postgres://leo19@127.0.0.1:5544/postgres)
bash tests/scripts/run_it.sh --setup-db --keep-db                 # POSIX
pwsh tests/scripts/run_it.ps1 -SetupDb -KeepDb                   # Windows

# 仅跑某一个 integration test
pwsh tests/scripts/run_it.ps1 -TestName pg_repos_integration
```

### 4.1 完整回归 (UT → IT → ST, 推荐)

```bash
# POSIX
bash tests/scripts/run_all_tests.sh --keep-db 2>&1 | tee /tmp/regression.log

# Windows
pwsh tests/scripts/run_all_tests.ps1 -KeepDb

# 完整回归报告 (md + json) 落盘:
#   tests/reports/regression-<YYYYMMDD-HHMMSS>.md
#   tests/reports/regression-<YYYYMMDD-HHMMSS>.json
#   tests/logs/regression-<YYYYMMDD-HHMMSS>.log   (master log, 串联 UT/IT/ST)

# 离线归因 (不重跑, 只汇总已有 log):
pwsh tests/scripts/generate_regression_report.ps1 -Stamp <YYYYMMDD-HHMMSS>
```

### 4.2 原 shell 脚本手工探针 (per 2026-08-31 原版, 仍保留)

```powershell
# 1. 起 test db
pwsh tests/scripts/setup_test_db.ps1

# 2. 启 im-gateway (前台, Ctrl-C 停)
pwsh tests/scripts/start_im_gateway_mock.ps1 -WaitForReady

# 3. (新 shell) 探针
bash tests/scripts/curl_examples.sh
bash tests/scripts/wscat_examples.sh "$TOKEN" "$CONV_ID"

# 4. 停 gateway, 删 test db
pwsh tests/scripts/stop_im_gateway.ps1 -Force
pwsh tests/scripts/teardown_test_db.ps1 -Force
```

或者一键 (Windows, 老 entry point, 仍可用, 但推荐 §4.1):

```powershell
pwsh tests/scripts/run_all_tests.ps1
```

## 5. 已知缺口 (DDD Review 必查)

1. **aux-13 §1 仅引用 12 帧 (实有 18 帧)**: 本目录覆盖 MVP 必测子集,其余 6 帧由 im-testkit crate 内部维护。
2. **SQL fixture 字段未与 im-core 实际表 schema 100% 对齐**: 字段以 migration `0001-0006` 为准,im-core 实际实现如有列名差异需 Test-3 worker 二次复核。
3. **PowerShell 脚本跨平台未支持**: macOS / Linux 必须用 `.sh` 脚本;PowerShell 7+ 跨平台需在 Test-3 阶段补 `pwsh` 适配。[ULYS-138] 部分缓解: 已为 run_ut / run_it / run_all_tests / generate_regression_report 提供 PS1 + sh 配对。
4. **wscat / curl 工具未在本机实测安装**: 脚本 fail-fast 而不 auto-install,确保不强加副作用。
5. **setup_test_db.ps1 用 superuser `postgres`**: 未建应用专用 user (`im_test_app`);生产前需收紧 (Test-3 阶段)。
6. **psql 工具链**已在 PATH 假设;**本机确认**该假设后**未实测执行** `setup_test_db.ps1` (per "不写运行代码"硬约束)。
7. **start_im_gateway_mock.ps1 未实测启动**: 仅当 `target/release/im-gateway.exe` 或 `target/debug/im-gateway.exe` 存在时才执行,否则优雅退出。[ULYS-138] 强化: run_all_tests.ps1 自动从 `target/{debug,release}/im-gateway.exe` + `E:/DevCache/cargo/target/im1.0/{debug,release}/im-gateway.exe` 4 个候选查找, 找不到时 Layer 3 = INCONCLUSIVE 而非 fail。
8. **im-gateway 端口假设** 18080/18081/19001 — 若 im-gateway 实际默认端口不同,需在 Test-3 阶段对齐。
9. **[ULYS-138] ST 层暂仅含 healthz smoke**: 完整的 WS/gRPC/REST 协议层 ST 需在 im-gateway 实装 WS / 完整 REST handler 后 (C-9 / C-11) 才能跑;当前 im-gateway 仅放骨架。当前 ST = 仅起 binary → 等 → curl /healthz → 停。
10. **[ULYS-138] IT 层依赖 PG 18.6**: run_it.{ps1,sh} 假定 PG 在 `postgres://leo19@127.0.0.1:5544/postgres`(per scripts/init-pg18-b1.sh)。若 PG 未启,IT 会失败, 整层失败不会让 UT 也失败 (UT 在前且独立)。

## 6. 协议引用 (禁止改写, 仅引用)

- `docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md` v1.1.0 — 主协议样例源 (752 行, `[PROTOCOL-FROZEN]`)
- `docs/DetailedDesign.md` §2-§5 — gRPC / WS / REST 协议源
- `migrations/0001_create_tenants_games_environments.sql` … `0006_create_audit_logs.sql` — DB schema
- `crates/im-proto/proto/core.proto` — gRPC proto 定义

## 7. 修订历史

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-08-31 | Mavis 接手 agent per DEC-008 | 初版: 12 WS + 4 gRPC + 7 REST + 6 SQL fixture + 5 PS1 + 2 sh + README |
