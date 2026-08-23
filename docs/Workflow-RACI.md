# Workflow RACI 矩阵 (IM1.0 极简版)

> **适用场景**:2-3 人极简团队(IM Core 阶段)
> **对照**:完整 16 Phase 流程见 `docs/Workflow.md`
> **关键简化**:1 人多角色、跳过部分评审、合并若干阶段

---

## 0. 角色定义(2-3 人下)

| 代号 | 角色 | 2-3 人下谁担任 | 真实姓名(待填) |
|---|---|---|---|
| **PM** | 项目经理 / 企画 | Tech Lead 兼 | |
| **TL** | Tech Lead / 架构师 | 主开发者 | |
| **DEV-A** | 开发者 A | TL 兼 + 第 2 人 | |
| **DEV-B** | 开发者 B(可选) | 第 3 人 | |
| **SRE** | 运维 / SRE | DEV-A 兼 | |
| **QA** | 测试 / 质量 | TL 兼 | |

> **关键原则**:**RACI 中 1 项只能有 1 个 A**;其余 R / C / I 可以多人或同人多角。
> 极简团队下,R 和 A 经常是同 1 人,这是合理的。

---

## 1. Phase RACI 速查(16 Phase)

> **R** = Responsible(执行) / **A** = Accountable(拍板) / **C** = Consulted(咨询) / **I** = Informed(知情)

| Phase | 关键工程 | A | R | C | I |
|---|---|---|---|---|---|
| 01 超上流 | 01-09 | PM | PM | TL | DEV |
| 02 要件定义 | 10-21 | PM | PM | TL | DEV |
| 03 基本设计 | 22-41 | TL | TL | PM | DEV |
| 04 详细设计 | 42-52 | TL | TL | DEV | PM |
| 05 实现 | 53-58 | TL | DEV-A | TL | PM |
| 06 单元测试 | 59-65 | TL | DEV-A | TL | PM |
| 07 集成测试 | 66-75 | TL | DEV-A | PM | TL |
| 08 系统测试 | 76-89 | TL | DEV-A + DEV-B | TL | PM |
| 09 验收测试 | 90-95 | PM | PM | TL | DEV |
| 10 迁移 | 96-101 | TL | TL + SRE | PM | DEV |
| 11 发布 | 102-108 | TL | TL + SRE | PM | DEV |
| 12 运维 | 109-117 | SRE | SRE | TL | PM |
| 13 维护 | 118-126 | TL | DEV | PM | SRE |
| 14 质量管理 | 127-130 | PM | PM | TL | DEV |
| 15 管理 | 131-144 | PM | PM | TL | DEV |
| 16 终结 | 145-150 | PM | PM | TL | DEV |

---

## 2. 关键工程 RACI(挑出 20 个最常发生的)

> 完整 150 工程的 RACI 见 `Workflow-RACI-Full.md`(可按需生成)。这里只列**最容易踢皮球**的。

### 2.1 代码与评审

| 工程 | 动作 | A | R | C | I | 备注 |
|---|---|---|---|---|---|---|
| 54 编码 | 写代码 | TL | DEV-A | TL | PM | |
| 55 SAST | 跑 SAST | TL | TL(自动) | SRE | PM | CI 自动 + Tech Lead 看 |
| 56 CR | Code Review | TL | TL(必须 1 人 approve) | DEV-B(可选) | PM | **极简:1 approve 即可** |
| 58 CI | 流水线 | SRE | SRE(自动) | TL | DEV | |

### 2.2 试验 / 部署

| 工程 | 动作 | A | R | C | I | 备注 |
|---|---|---|---|---|---|---|
| 62 UT 实施 | 跑单测 | TL | DEV-A | TL | PM | |
| 65 UT 完了承認 | UT 通过签字 | TL | TL | DEV | PM | 简化为 1 人 |
| 75 Regression | 回归 | TL | DEV-A | TL | PM | 合并进 CI |
| 78 功能试验 | ST | TL | DEV-A + DEV-B | TL | PM | 简化为 4 类 |
| 80 PT | 性能试验 | TL | TL | SRE | PM | 简化为基线 |
| 83 Security | 安全试验 | TL | TL | 外包 | PM | **不省**,可外包 |
| 88 OT | 运维试验 | SRE | SRE | TL | DEV | |
| 89 ST 完了承認 | ST 通过签字 | TL | TL + PM | DEV | SRE | PM 参与 |
| 95 検収 | 验收 | PM | PM | TL | DEV | |
| 103 Go/No-Go | 发布决议 | PM | PM | TL | DEV | |
| 105 Deploy | 部署 | SRE | SRE | TL | PM | |

### 2.3 变更 / 应急

| 工程 | 动作 | A | R | C | I | 备注 |
|---|---|---|---|---|---|---|
| 118 CR | 变更要求 | PM | PM | TL | DEV | |
| 119 影響分析 | 影响分析 | TL | TL | DEV | PM | |
| 120 変更管理 | CAB 批准 | PM | PM | TL | DEV | **极简:1 人 PM 批即可,2 票:PM + TL** |
| 125 Hotfix | 紧急修复 | TL | TL | DEV | PM | 事后补流程 |
| 126 Regression | 维护回归 | TL | DEV-A | TL | PM | |
| 130 Audit | 审计 | PM | PM(外包) | TL | DEV | |
| 144 Baseline | 基线 | PM | PM | TL | DEV | |

### 2.4 监控 / 告警

| 工程 | 动作 | A | R | C | I | 备注 |
|---|---|---|---|---|---|---|
| 110 监控 | 仪表盘 | SRE | SRE | TL | PM | |
| 114 Incident | 事件响应 | SRE | SRE(首要) | TL | PM | SRE 7x24 on-call |
| 116 Problem | RCA | TL | TL | SRE | PM | TL 主导 RCA |
| 117 Support | 咨询 | SRE | DEV | TL | PM | DEV 一线,SRE 二线 |

---

## 3. 4 个关键 GATE(简化的 12 → 4)

| 原始 GATE | 极简 GATE | 拍板人 | 触发 |
|---|---|---|---|
| UT + IT + ST + 性能 + 安全 + 部署 | **PR 合入** | TL(必须 approve) | 任何代码变更 |
| UT 完了 + IT 完了 + ST 完了 | **单测通过** | TL | PR 阶段 CI 跑完 |
| Smoke + 部署成功 + OT | **Smoke 通过** | SRE + TL | 部署后 |
| 検収 + Go-Live | **部署成功** | SRE | Smoke + 监控验证 |

> **关键**:即便简化,CR + SAST + 单测 + Smoke 这 4 道闸门**不省**。否则必出事。

---

## 4. 升升级路径(极简下)

```
P2 / P3 事件:
  SRE / DEV 直接处理 → 通知 TL(异步)

P1 事件:
  SRE 响应 → 5 分钟内通知 TL → TL 决定是否升级 PM
  → 处理后 30 分钟内发 Slack / 飞书短报

P0 事件:
  立刻电话 PM + TL → 30 分钟内给 Sponsor 短报
  → 24h 内 Blameless Postmortem
```

> 详见 `aux-09-log-query-cookbook.md` 的故障模式查询。

---

## 5. 例外(强制升级到 PM)

以下情况**必须**PM 拍板,TL 不能独断:

- 跨产品线决策(Voice / SDK 启动)
- 数据库 / 部署架构变更
- 公开 API breaking change
- 性能 NFR 数字变更
- 关键依赖替换(Rust crate 重大变更)
- 任何涉及客户 / 经营的决策

---

## 6. 决策日志(谁决定过什么)

> **关键决策必须有 1 行记录**,否则未来 6 个月不知道当时为什么这么定。

| 日期 | 决策 | 拍板人 | 理由 | 链接 |
|---|---|---|---|---|
| 2026-08-20 | 极简流程(2-3 人) | PM | 团队规模限制 | Project-Status §1.4 |
| 2026-08-20 | GitHub + GHA | PM | 主流开源 + 生态全 | Project-Status §1.3 |
| 2026-08-20 | IM Core 优先 | PM | Voice / SDK 依赖 IM | Project-Status §1.2 |
| | | | | |

---

## 7. 维护

- RACI 变更须经 PM 同意
- 任何"谁负责不清楚"问题先看本表,再看原始 `Workflow.md`
- 完整版 RACI(150 工程逐条)按需生成

---

**版本**:v1.0.0 | **维护**:PM | **更新周期**:每月 1 次 或 重大变更时
