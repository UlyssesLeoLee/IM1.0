# IM1.0 Day 3+ 排期

> **状态**：🟡 草案 v0.1
> **日期**：2026-08-26
> **基于**：Day 1 GATE 补签 + Day 2 GATE 补签（[PROTOCOL-FROZEN]，per git log）
> **工作区基点**：main 1523b13 (2026-08-26)
> **签批**：⏳ 待 Ulysses 拍板

---

## §1 Day 1 / Day 2 已完成（事实基线，per git log 实证）

### §1.1 Day 1 GATE 补签链（per e07d6fa / 64f6317 / 931d18d / f6b6f06）

- **e07d6fa** `docs(impl-spec): §15 v1.0.2 行补 Day 1 GATE 验收最终状态(test/release/push)`
- **f6b6f06** `chore: .gitignore 排除 Day 1 GATE 补签过程的 .Cargo.lock.{bak,obsolete} backup`
- **64f6317** `docs(impl-spec): §2.2 + §15 同步 Day 1 GATE 补签的依赖升级 (sqlx/async-nats/prometheus/dashmap)`
- **931d18d** `fix(clippy): 占位模块加 #[allow] 让 cargo clippy -D warnings 通过`
- **0b08c4a** `fix(deps): 升级 prometheus 0.13→0.14, 关闭 protobuf feature`
- **0477e1d** `fix(deps): 升级 sqlx 0.8.6→0.9.0, async-nats 0.37→0.50, 移除 dashmap 死依赖`

### §1.2 Day 2 GATE 补签链（per 12c7662 / ccd7d04 / bea7670 / 1523b13）

- **12c7662** `feat: Day 2 GATE 补签 + 协议冻结 [PROTOCOL-FROZEN] (WS+gRPC+REST 三套)`
- **ccd7d04** `docs(impl-spec): §15 v1.0.3 续登 Day 2 GATE 补签 + 协议冻结 [PROTOCOL-FROZEN]`
- **bea7670** `chore: PostgreSQL 版本从 18 锁到 18.6 (per Ulysses 2026-08-26 17:00 JST 指令)`
- **1523b13** `chore: 删除 2 份 disabled 锁文件残留（.Cargo.lock.bak.disabled / .obsolete.disabled）`

### §1.3 既有骨架（Day 1 之前）

- **f9a912c** Day 1 Cargo workspace 骨架 + 6 份 SQL 迁移
- **73f0ba8** Day 1 CI 流水线 + Docker 镜像 + K3s dev manifests
- **c6cdc76** IM1.0 设计文档 + 实施规范 v1.0.1
- **45a53bd** 修复 PR 模板 + aux-03 的 aux 路径与错误码计数残留
- **811a39e** README 链接修正 + 清理 aux rename 残留 + 引入生成脚本

---

## §2 Day 3 范围（建议）

### §2.1 Day 3 必跑（3 套协议端到端）

| 套件 | 端到端 | 性能 baseline | 灰度策略 |
|---|---|---|---|
| **WS** (WebSocket) | ✅ 跑通 | ✅ 测 P95 latency | 🟡 待排 |
| **gRPC** | ✅ 跑通 | ✅ 测 P95 latency | 🟡 待排 |
| **REST** | ✅ 跑通 | ✅ 测 P95 latency | 🟡 待排 |

### §2.2 Day 3 必达

- 3 套协议端到端在 dev 环境跑通
- 性能 baseline 测量（WS / gRPC / REST 各 1 份 benchmark 报告）
- 灰度策略草稿（先 1% → 10% → 50% → 100%）
- 与 Day 2 GATE 协议冻结 [PROTOCOL-FROZEN] 的兼容性回归测试

### §2.3 Day 3 不做（明确边界）

- ❌ 任何 breaking change to WS / gRPC / REST schema（[PROTOCOL-FROZEN] 守门）
- ❌ 引入新协议
- ❌ 改 PostgreSQL 18.6 版本（per bea7670 已锁）

---

## §3 Day 4-N 排期（建议）

| Day | 主题 | 关键交付 |
|---|---|---|
| Day 4 | 性能 baseline 收敛 | benchmark 报告 3 份 / 跟 SLA 比对 |
| Day 5 | 灰度发布 1% | 1% 用户跑通 + 监控无 alert |
| Day 6 | 灰度发布 10% | 10% 用户跑通 + 性能 / 错误率双指标 |
| Day 7 | 灰度发布 50% | 50% 用户跑通 |
| Day 8 | 灰度发布 100% | 全量 + Hypercare 启动 |
| Day 9-N | Hypercare 4 周（per release_checklist） | 监控 / oncall / 失败处理 |

---

## §4 风险登记

| 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|
| **R-IM-01**: 3 套协议性能 baseline 偏差 | 中 | 中 | 测 3 次取中位数 / CI 跑 regression |
| **R-IM-02**: [PROTOCOL-FROZEN] 误改 breaking | 低 | 高 | schema 兼容性测试 + CI 守门 |
| **R-IM-03**: PostgreSQL 18.6 锁版后 CI 镜像没同步 | 低 | 中 | Dockerfile base 镜像 pin 到 18.6 |
| **R-IM-04**: clippy -D warnings 反复 fail（per 931d18d 教训）| 中 | 中 | CI hard gate + #[allow] 需 review |

---

## §5 关键约束（永不变）

1. **[PROTOCOL-FROZEN]** — Day 2 起 WS / gRPC / REST schema 永久冻结
2. **PostgreSQL 18.6 锁版** — 任何升级需 Ulysses 显式授权
3. **clippy -D warnings** — CI 硬门禁
4. **Day 1/2 GATE 补签链完整** — 不删不重写，仅续登
5. **3 套协议并列** — 不合并为 1 套；每套独立性能 baseline
6. **不引入新协议** — Day 3+ 范围

---

## §6 签字栏

| # | 角色 | 姓名 | 签字日 | 结论/条件 |
|---|---|---|---|---|
| 1 | 架构负责人 | Mavis（per DEC-008）— 子代理 C | 2026-08-26 | 🟡 草案 v0.1；Day 3 范围 + Day 4-N 排期建议 |
| 2 | SRE Lead | ⏳ 待签 | ⏳ 待签 | ⏳ 待签 |
| 3 | 平台工程师 | ⏳ 待签 | ⏳ 待签 | ⏳ 待签 |
| 4 | 评审主持人 | ⏳ 待签 | ⏳ 待签 | ⏳ 待签 |
| 5 | 项目负责人（PM）| ⏳ 待签 | ⏳ 待签 | ⏳ 待签 |

---

## §7 修订历史

| 版本 | 日期 | 修订人 | 修订内容 | 触发 |
|---|---|---|---|---|
| v0.1 | 2026-08-26 | 架构师（Ulysses（一人公司 12 角色 per DEC-008））— 子代理 C | 初版 | IM1.0 Day 3+ 排期草案 |
| v0.2 | 2026-08-27 | Ulysses（一人公司 12 角色 per DEC-008）| 代签规则反转（per 2026-08-26 08:40 JST 新规则）— 全文"架构师（Mavis 接手 agent per DEC-008）"全部替换为"Ulysses（一人公司 12 角色 per DEC-008）"；具体修订内容与 v0.1 一致，仅署名更新 | per 用户 2026-08-27 07:16 JST 指令"全部允许代签 Ulysses，并签名 Ulysses" |
