# IM1.0 测试设计书（Test Design）

| 字段 | 取值 |
|---|---|
| 文档编号 | IM1.0-TD-001 |
| 文档类型 | テスト設計書（Test Design） |
| 版本 | v0.1 Draft |
| 状态 | Draft |
| 关联系统 | IM1.0 / im-gateway / im-core / im-presence / im-media |
| 上游文档 | docs/SRS.md v0.2, docs/DetailedDesign.md v0.2, docs/templates/04-detailed-design/auxiliary/aux-13-protocol-frame-samples.md |
| 下游文档 | tests/data/ (per feat/tests-scripts [pending]), tests/scripts/ (per feat/tests-scripts [pending]) |
| 编写者 | Mavis 接手 agent per DEC-008 |
| 审批者 | Ulysses (DDD Review pending) |

> 修订者: Mavis 接手 agent per DEC-008
> 协议基线: [PROTOCOL-FROZEN] per commit `12c7662` (WS + gRPC + REST 三套冻结)

## 1. 修订历史

| 版本 | 日期 | 修改者 | 修改内容 |
|---|---|---|---|
| v0.1 Draft | 2026-08-31 | Mavis 接手 agent per DEC-008 | 初稿：建立测试金字塔、UT/IT/E2E 草案、已知缺口显式化 |

> 派生约束（per 用户偏好 2026-08-26 强证据）：
> ① 禁回溯叙事（不写"per X 历史形态" / "per X 升版前"等无 git 证据的叙事）
> ② 引用 BAS / SRS / DetailedDesign 必须以 git commit hash 实证
> ③ 缺标比错标安全：所有 [草案] / [未实测] / [pending] 标清晰
> ④ 子代理授权加 git 实证约束

## 2. 测试目标

1. **协议一致性**：验证 im-gateway 业务路由实现符合 [PROTOCOL-FROZEN] 协议冻结（per commit `12c7662`）
2. **业务函数单元化**：验证 im-core / im-presence / im-media 业务函数可独立测试
3. **数据库契约**：验证 6 SQL migration 应用后表结构符合 SRS §数据需求 + DetailedDesign §数据模型
4. **多实例通信**：验证 2 个 im-gateway 实例可通过共享 DB 通信（MVP Day 3+ 目标）
5. **跨引擎嵌入边界**（[草案]）：为后续 Unity / Unreal / Godot 客户端 SDK 测试预留接口

## 3. 测试范围

| 测试类型 | 范围 | 工具 |
|---|---|---|
| 单元测试 (Unit Test) | im-core / im-presence / im-media 内部函数 | cargo test + im-testkit (per branch `feat/testkit-crate` [pending]) |
| 集成测试 (Integration Test) | im-gateway 业务路由 ↔ im-core 业务函数 | cargo test --test '*' + im-testkit |
| 端到端测试 (E2E Test) | 客户端 → im-gateway (WS+gRPC+REST) → DB | wscat / curl / 自研 E2E runner |
| 协议契约测试 (Contract Test) | aux-13 §12 WS 帧 / §4 gRPC RPC / §7 REST endpoint | im-testkit::frame_samples |
| 性能测试 (Performance Test) | [PROTOCOL-FROZEN] SLO 数值（**草案，未实测**） | criterion [未做] |

**范围外**（v0.1 显式排除）：
- 客户端 SDK 跨引擎（Unity / Unreal / Godot）测试 — 等 RustGameServer 5 域独立 Lead 方案确认后启动
- LiveKit 语音子系统 — 引用 docs/LiveKit-Voice-Subsystem.md [pending DDD Review]
- miri / fuzz — 见 §9 已知缺口

## 4. 测试策略

### 4.1 测试金字塔

```
       ┌─────────┐
       │  E2E    │  10% （端到端冒烟）
       ├─────────┤
       │   IT    │  20% （im-gateway ↔ im-core 集成）
       ├─────────┤
       │   UT    │  70% （im-core / im-presence / im-media 内部）
       └─────────┘
```

- **70% Unit** — 业务函数纯函数化、依赖通过 im-testkit 注入
- **20% Integration** — im-gateway 路由表 + im-core handler 一一对应
- **10% E2E** — 客户端 → 网关 → DB 冒烟，验证 MVP Day 3+ 双实例通信

### 4.2 Mock 优先

- 跨 crate 测试一律用 `im-testkit`（per branch `feat/testkit-crate` [pending]）
- im-testkit 公开 API **v0.1 不锁定**（等 Test-1/2 落地后 v0.2 校准）
- DB fixture 走 `tests/data/sql/` 共享 SQL 文件

### 4.3 fixture 复用

- `tests/data/` 共享 mock JSON + SQL fixture（per branch `feat/tests-scripts` [pending]）
- `tests/scripts/` PowerShell + Bash 自动化（per branch `feat/tests-scripts` [pending]）

### 4.4 脚本化

- 所有回归测试可由 `tests/scripts/regression.ps1` 一键跑（[pending]）
- 性能测试走 criterion bench harness（[未做]）

## 5. 测试用例设计（草案）

> **草案说明**：v0.1 仅列代表性用例，真实用例数 v0.2 补（见 §9 已知缺口）

### 5.1 单元测试用例

#### 5.1.1 im-core::auth

| 用例 ID | 描述 | 输入 | 期望 |
|---|---|---|---|
| UT-AUTH-001 | token 签发成功 | valid {user_id, claims} | 返回 (token, expires_at)，HMAC 校验通过 |
| UT-AUTH-002 | token 过期拒绝 | expired token | 拒绝 + error code `AUTH_TOKEN_EXPIRED` |
| UT-AUTH-003 | token 篡改拒绝 | tampered HMAC | 拒绝 + error code `AUTH_TOKEN_INVALID` |
| UT-AUTH-004 | refresh_token 一次性 | replay 同 token | 第二次拒绝 + 记 audit_log |

#### 5.1.2 im-core::message（[草案，v0.2 扩]）

| 用例 ID | 描述 |
|---|---|
| UT-MSG-001 | message 持久化成功（valid input） |
| UT-MSG-002 | message 超长拒绝（> 4 KB） |
| UT-MSG-003 | message kind 非法拒绝 |

#### 5.1.3 im-presence::status（[草案，v0.2 扩]）

| 用例 ID | 描述 |
|---|---|
| UT-PRES-001 | online → offline 状态切换 |
| UT-PRES-002 | presence_update 广播给订阅者 |

#### 5.1.4 im-media::upload（[草案，v0.2 扩]）

| 用例 ID | 描述 |
|---|---|
| UT-MED-001 | 媒体上传签名 URL 生成 |
| UT-MED-002 | MIME 类型非法拒绝 |

### 5.2 集成测试用例

#### 5.2.1 im-gateway ↔ im-core

| 用例 ID | 描述 | 协议 |
|---|---|---|
| IT-GW-001 | POST /v1/auth/login 成功路径 | REST (aux-13 §7) |
| IT-GW-002 | POST /v1/auth/login 失败路径（密码错） | REST (aux-13 §7) |
| IT-GW-003 | WS 帧 `message_new` 路由到 im-core | WS (aux-13 §12) |
| IT-GW-004 | WS 帧 `presence_update` 路由到 im-presence | WS (aux-13 §12) |
| IT-GW-005 | gRPC `MessageService/Send` 路由到 im-core | gRPC (aux-13 §4) [草案] |

#### 5.2.2 im-gateway 双实例通信

| 用例 ID | 描述 |
|---|---|
| IT-MULTI-001 | 客户端 A 连 GW-1 发消息，客户端 B 连 GW-2 收到（通过 DB 共享） |
| IT-MULTI-002 | GW-1 崩溃后，客户端重连 GW-2 状态一致 |

### 5.3 端到端测试用例

| 用例 ID | 描述 |
|---|---|
| E2E-001 | 1 客户端登录 → 发消息 → 收消息（自发自收） |
| E2E-002 | 2 客户端双向通信（1v1 IM） |
| E2E-003 | 客户端掉线重连，消息不丢（offline queue 验证）[草案] |

### 5.4 协议契约测试用例

| 用例 ID | 描述 | 引用 |
|---|---|---|
| CT-WS-001 | aux-13 §12 12 个 WS 帧 round-trip | aux-13-protocol-frame-samples.md §12 |
| CT-GRPC-001 | aux-13 §4 4 个 gRPC RPC round-trip | aux-13-protocol-frame-samples.md §4 |
| CT-REST-001 | aux-13 §7 7 个 REST endpoint round-trip | aux-13-protocol-frame-samples.md §7 |

### 5.5 性能测试用例（[草案，未实测]）

| 用例 ID | 描述 | SLO 草案 |
|---|---|---|
| PERF-MSG-001 | 单实例消息吞吐 | ≥ 10K msg/s [草案] |
| PERF-LAT-001 | WS 消息端到端 P99 延迟 | ≤ 50 ms [草案] |
| PERF-CONN-001 | 并发 WS 连接数 | ≥ 10K [草案] |

> **缺标警告**：以上 SLO 数字与 SRS v0.2 NFR 章节一致性 **v0.2 校准**（见 §9 已知缺口）

## 6. 测试数据设计

### 6.1 mock JSON 数据（per `tests/data/` [pending]）

| 来源 | 数量 | 章节引用 |
|---|---|---|
| WS 帧样本 | 12 个 | aux-13 §12 |
| gRPC RPC 样本 | 4 个 | aux-13 §4 |
| REST endpoint 样本 | 7 个 | aux-13 §7 |
| 错误码样本 | 5 个（草案） | aux-03-error-code-registry.md |

### 6.2 SQL fixture（per `tests/data/sql/` [pending]）

| 表 | 行数 | 说明 |
|---|---|---|
| tenant | 1 | 默认测试租户 |
| game | 1 | 默认测试 game |
| environment | 1 | dev / staging / prod 三选一 |
| user | 2 | user_a / user_b |
| conversation | 1 | DM 会话 |
| message | 5 | text / image / reaction 混合 |
| audit_log | 3 | 登录 / 发消息 / 状态变更 |

> **缺口**：6 SQL fixture 字段 **未与 im-core 实际 schema 100% 对齐**（per Test-1/2 异步落地，见 §9）

## 7. 测试环境

| 资源 | 配置 | 状态 |
|---|---|---|
| OS | Windows 11 + WSL2（Git Bash 备选） | 已装 |
| PostgreSQL | 18.6 (port 5555) | 已装 |
| 测试 DB | `im1dev` | 已建 [pending 确认] |
| Rust | 1.98.0 (cargo + rustc) | 已装 |
| PowerShell | 5.1 | 已装 |
| Bash | 4+ (WSL/Git Bash) | 已装 |
| 协议冻结 | [PROTOCOL-FROZEN] per commit `12c7662` | 已冻结 |
| 文档基线 | SRS v0.2 / DetailedDesign v0.2 per `0cd0d75` | 已合并 main |

## 8. 测试工具链

| 工具 | 用途 | 状态 | 引用 |
|---|---|---|---|
| `crates/im-testkit` | 共享 mock + fixture | [pending] | branch `feat/testkit-crate` (per 2026-08-31 16:26 JST 指令) |
| `tests/data/` | mock JSON + SQL fixture | [pending] | branch `feat/tests-scripts` (per 2026-08-31 16:26 JST 指令) |
| `tests/scripts/` | PowerShell + Bash 自动化 | [pending] | branch `feat/tests-scripts` (per 2026-08-31 16:26 JST 指令) |
| `sqlx-cli 0.9` | migration 跑通 | 未装 | `cargo install sqlx-cli` |
| `wscat` | WS 客户端测试 | 未实测 | npm |
| `curl` | REST 客户端测试 | 已装 | system |
| `grpcurl` | gRPC 客户端测试 | 未装 [pending] | brew / apt |
| `criterion` | 性能基准 | 未做 | crate |

## 9. 已知缺口（DDD Review 必查）

> **缺标比错标安全**：以下缺口 v0.1 显式列，v0.2 校准时逐项勾除

1. **im-testkit 实际公开 API 未知** — v0.1 设计书仅引用 branch `feat/testkit-crate` 名字，**不猜 API 字段**
2. **性能数字 / SLO 数字未实测** — 全部标 `[草案]`，等 SRS v0.2 NFR 章节 DDD Review 后回填
3. **6 SQL fixture 字段未与 im-core 实际 schema 100% 对齐** — per Test-1/2 异步落地
4. **UT/IT/E2E 用例数仅草案** — 真实用例数 v0.2 补
5. **跨平台 (macOS / Linux) 未实测** — 工具链脚本只在 Windows + WSL2 跑过
6. **aux-13 §6 JSON Schema 6 kind 未逐项引用** — 仅引用 §12 WS / §4 gRPC / §7 REST 章节
7. **工具链版本未与 `versions.toml` 锁版** — 缺独立 lockfile 管理策略
8. **miri / fuzz 未做** — UB / 边界条件覆盖不足
9. **LiveKit 语音子系统测试范围未定** — 引用 docs/LiveKit-Voice-Subsystem.md [pending DDD Review]
10. **客户端 SDK 跨引擎（Unity / Unreal / Godot）测试 v0.1 排除** — 等 5 域独立 Lead 方案确认后启动

## 10. DDD Review 必查

1. **与 SRS v0.2 性能 NFR 章节一致性**
   - 对照 SRS v0.2 NFR-PERF-* 条目核对 §5.5 性能测试用例
   - 当前 SLO 数字全部 `[草案]`，DDD Review 时定稿
2. **与 aux-13 协议帧样本一致性**
   - §5.4 协议契约测试是否覆盖 §4 / §7 / §12 全部样本
   - §6.1 mock JSON 数量与 aux-13 章节是否 1:1
3. **与 im-testkit crate 公开 API 一致性**
   - v0.1 **不锁 API**，仅引用 branch `feat/testkit-crate`
   - 等 im-testkit v0.1 落地后 v0.2 校准
4. **缺标比错标安全**
   - 所有 [草案] / [未实测] / [pending] 标清晰
   - §9 已知缺口 10 项逐一 review
5. **commit / branch 引用实证**
   - 引用 `12c7662` 协议冻结 commit：已 `git log --all | grep PROTOCOL-FROZEN` 实证
   - 引用 `0cd0d75` main HEAD：已 `git log --oneline -1 main` 实证
   - 引用 branch `feat/testkit-crate` / `feat/b1-db-migration`：已 `git branch -a` 实证存在
   - 引用 branch `feat/tests-scripts`：**当前不存在**，标 [pending] 正确

## 11. 下一步建议

1. **与 Test-1 / Test-2 合并 DDD Review**：
   - Test-1: 6 SQL fixture 与 im-core schema 对齐（per §9.3）
   - Test-2: im-testkit crate 公开 API 锁定（per §9.1）
2. **Ulysses DDD Review 通过后**：
   - 合并 `feat/test-design-doc` → `main`
   - 删除 worktree `D:/IM1.0-wt/feat-test-design-doc`
3. **v0.2 计划**：
   - 真实用例数补全（per §9.4）
   - 性能数字实测（per §9.2）
   - aux-13 §6 JSON Schema 6 kind 逐项引用（per §9.6）
4. **不动**：docs/SRS.md / docs/BasicDesign.md / docs/DetailedDesign.md（per 硬约束 #4）
5. **不 push** 到 origin（per 硬约束 #6/#9）

---

**附录 A：引用 commit / branch 实证清单**

| 引用对象 | 实证方式 | 实证结果 |
|---|---|---|
| [PROTOCOL-FROZEN] 协议冻结 | `git log --all --oneline \| grep PROTOCOL-FROZEN` | 找到 `12c7662` (Day 2 GATE) + `ccd7d04` (§15 v1.0.3 续登) |
| branch `feat/testkit-crate` | `git branch -a` | 存在，未合并 main |
| branch `feat/b1-db-migration` | `git branch -a` | 存在，未合并 main |
| branch `feat/tests-scripts` | `git branch -a` | **不存在**，标 [pending] |
| `crates/im-testkit` | 目录检查 | **不存在**（仅 branch 名引用，per 硬约束 #5） |
| `tests/data/` | 目录检查 | **不存在**（仅 branch 名引用） |
| `tests/scripts/` | 目录检查 | **不存在**（仅 branch 名引用） |
| main HEAD `0cd0d75` | `git log --oneline -1 main` | merge: docs/detailed-v02 |

---

> **文档结束** — IM1.0-TD-001 v0.1 Draft
> 修订者：Mavis 接手 agent per DEC-008
> DDD Review pending：Ulysses
