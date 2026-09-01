---
doc_id: H-1
title_zh: 经营/投资人截止日决策（选项清单）
status: Draft for PM Decision
owner: PM (Ulysses) / 调研: 架构师 (Mavis 接手 agent per DEC-008)
date: 2026-09-01 JST
wbs_ref: 132-wbs.md v1.0.0 §5.8 (H-1)
upstream: Project-Status.md §2.1
---

# H-1. 经营 / 投资人截止日决策（选项清单）

> **本文档性质**：Mavis 调研输出，**3-5 个选项 + 推荐**。**最终拍板由 PM (Ulysses) 用 ask_user 选项决定**，Mavis 不代签。
>
> **数据基线**：132-wbs.md v1.0.0（全量 21.2M tokens ≈ 21 周，关键路径 5.31M tokens）+ Project-Status.md §2.1 + DEC-008（1 人公司 = Ulysses + Mavis）
>
> **强约束**（per Ulysses 2026-09-01 13:03/13:05 JST 派生）：Mavis 1 人公司上限 = 1M tokens/周（per RGS-TS-001 §6.2 token-OLU 框架）；下游招人/独立 Lead 不在 2 SRE ≤ 20 人·天/周约束内（per 2026-08-21 8 域独立 Lead 偏好）

---

## 1. 背景与约束

### 1.1 现有数据基线（来自 132-wbs.md v1.0.0 §6 关键路径）

```
H-1 → H-3 → C-1 → C-2 → C-9 → C-11 → D-3 → E-3 → F-2 → F-3
       ↓
      B-1 ──────(依赖 F-1)
       ↓
      F-1 (Docker daemon bridge 修复, 当前 Blocked)
```

- **关键路径 token 预算（取 max）**：H-1(30K) + H-3(80K) + C-1(1.2M) + C-2(600K) + C-9(500K) + C-11(800K) + D-3(600K) + E-3(300K) + F-2(800K) + F-3(400K) = **5.31M tokens**
- **全量 WBS token 预算（取 max）**：约 **21.2M tokens** = 1 SRE ≈ 21 周（按 1M/周）
- **当前 Blocked 任务**：F-1（Docker Desktop daemon bridge 修复，2 分钟超时）+ B-1（6 份 SQL migration 真 PG 验证，待 F-1）

### 1.2 4 个里程碑（per Project-Status.md §2.1）

| 节点 | 当前 | 必填约束 |
|---|---|---|
| IM Core MVP 上线 | TBD | 必先内部 demo（团队 + 经营）再对外 |
| 第一个客户 PoC | TBD | 依赖 H-1 截止日 |
| 第一个付费 / GA | TBD | 依赖 PoC 验收 |
| 完整 Voice + SDK 集成 | TBD | V1+，不在 MVP 范围 |

### 1.3 经营/投资人可能约束（per 132-wbs.md §5.8 H-1 备注 + Project-Status §2.1 催办）

- 经营 / 投资人有截止日 → **必须给具体日期**，不能"尽快"
- MVP 必先内部 demo 再对外（防止未达演示标准就承诺）
- 招人节奏：是否在 MVP 之前补齐 4-6 人？1 人公司默认 = 否

---

## 2. 截止日选项（5 个，从激进到保守）

### 选项 A: 激进型（4 周后，2026-10-01）

- **承诺**：IM Core MVP 仅含关键路径子集上线（无 PoC 对外）
- **覆盖任务**：关键路径 10 项 = 5.31M tokens
- **周均 token 负载**：5.31M / 4 = 1.33M tokens/周 → **超 Mavis 上限 33%**
- **风险**：
  - F-1（Docker daemon bridge 修复）当前 Blocked，4 周内可能无法解锁 → B-1/F-2 全部滑期
  - 无测试补齐（E-1/2/4 跳过），质量风险高
  - 招人同步（4-6 周）= 截止日 = 0 业务产出
- **适合场景**：经营方强 deadline（如种子轮 close 后 90 天内必须 demo）+ 接受 0 测试覆盖 + F-1 立即解

### 选项 B: 标准型（8 周后，2026-10-29）

- **承诺**：IM Core MVP 完整上线 + 内部 demo + 1 客户 PoC
- **覆盖任务**：关键路径 + B-1/2/4 + D-1/2/4 + E-1/2/4 + F-1 = 约 6M tokens
- **周均 token 负载**：6M / 8 = 0.75M tokens/周 → **落在 Mavis 上限内**
- **风险**：
  - F-1 仍为 Blocked，需 2-3 周集中修；剩余 5 周内完成 6M tokens 偏紧
  - 不含 G 阶段任何任务（Voice / SDK / Presence）→ 客户需 100% IM-only
- **适合场景**：经营方 deadline 在 8-9 周 + 接受不含 Voice/SDK 的 IM-only 演示

### 选项 C: 稳健型（12 周后，2026-11-26）⭐ 推荐

- **承诺**：IM Core MVP + 全测试补齐 + 第一个客户 PoC 启动 + 内部 demo
- **覆盖任务**：全 Phase A-F（含 B-1 真 PG 验证 + F-1 修复 + 全 E 阶段）+ G-1/2 媒体 = 约 12M tokens
- **周均 token 负载**：12M / 12 = 1.0M tokens/周 → **正好达 Mavis 上限**
- **风险**：
  - 12 周连续不间断，1 人公司无 redundancy
  - 含 F-1 修复（200K）+ B-1（200K）= 约 2 周缓冲留给阻塞项
- **适合场景**：1 人公司现实节奏 + 内部 demo 优先 + 第一个 PoC 启动

### 选项 D: 保守型（21 周后，2027-01-27）

- **承诺**：全 WBS 含 Phase G Voice + SDK 完整
- **覆盖任务**：21.2M tokens（按 1M/周）
- **周均 token 负载**：21.2M / 21 = 1.01M tokens/周 → **紧贴上限**
- **风险**：
  - 超过"2-3 人 2 周 MVP 预算" 4 个月以上（但 1 人公司约束下其实合理）
  - 投资方可能因周期太长而失去信心
- **适合场景**：经营方 deadline 实际 > 5 个月 + 含完整 Voice + Game SDK

### 选项 E: 招人加速型（招 2 SRE 后 10 周内，即 2026-11-12 起跑，招人前置 4-6 周）

- **承诺**：2 SRE 加入后，并行执行 C/D/E 多个 Phase，加速 30-40%
- **覆盖任务**：全 Phase A-F + G-1/2 = 约 14M tokens（2 SRE × 1M/周 × 10 周 = 20M capacity 够用）
- **周均 token 负载**：14M / 10 周 = 1.4M tokens/周（2 SRE 并行）→ **每个 SRE 0.7M/周，落在上限内**
- **风险**：
  - **招人前置 4-6 周 = deadline 实质后移到 2026-12-24 之后才进入正轨**（招 2 SRE 用掉 4-6 周）
  - Ulysses 仍需亲自带 2 SRE（per 8 域独立 Lead 偏好：5 域架构不兼任）
  - 1 人公司 + 2 SRE = 3 人 RACI 矩阵需重画
- **适合场景**：经营方要求 1-2 个 SRE Lead + 接受招人前置期

---

## 3. 推荐

### 推荐选项：**C（稳健型 12 周后，2026-11-26）**

### 推荐理由

1. **现实节奏**（per DEC-008 1 人公司约束）：Mavis 上限 1M tokens/周 = 12 周正好覆盖 12M token 全 Phase A-F 任务
2. **阻塞项缓冲**（F-1 + B-1）：F-1 修复（200K）+ B-1 真 PG 验证（200K）合计 2 周缓冲，避免被 F-1 卡死
3. **内部 demo 优先**（per Project-Status §2.1）：先 demo 给团队 + 经营，再对外 → 12 周 = 第 10 周 demo + 第 12 周 PoC 启动
4. **质量不省**（per Project-Status §1.4 不简化项）：E-1/2/3/4 全测试补齐 + clippy + 单测全覆盖
5. **关键路径 5.31M 占 44%**：剩余 56% 留给非关键路径与重构 = 1.5x 缓冲比
6. **不擅自扩 scope**：不含 G 阶段（Voice/SDK）= 与 SRS §45 MVP 范围一致

### 备选（如果经营方 deadline 紧）

- 若经营方 deadline 在 8-9 周：**选项 B**（标准型 8 周，IM-only）
- 若经营方 deadline 在 4-6 周：**选项 A**（激进型 4 周，仅关键路径）+ 明确"无测试覆盖"风险
- 若招人预算批准：**选项 E**（招 2 SRE + 10 周 + 招人前置期）

### 推荐 4 个里程碑日期（per 选项 C 12 周）

| 节点 | 日期 | 备注 |
|---|---|---|
| IM Core MVP 内部 demo | 2026-11-12 | 第 10 周，团队 + 经营 demo |
| IM Core MVP 上线 | 2026-11-26 | 第 12 周，K3s dev 可演示 |
| 第一个客户 PoC 启动 | 2026-12-03 | 第 13 周（+1 周缓冲），依赖 H-3 决策 |
| 第一个付费 / GA | 2027-01-22 | 第 18 周，PoC 验收 + GA 部署 |
| 完整 Voice + SDK 集成 | 2027-04-22 | 第 32 周（V1 范围，不在 MVP 内） |

---

## 4. 已知缺口

1. **经营/投资人实际 deadline 未明**：Mavis 无 access 到投资人 contract / 经营 OKR，**截止日需 Ulysses 直接告知**
2. **F-1 修复时间未知**：当前 Blocked，2 分钟 `docker ps` 超时 → 修复可能 1 天或 1 周
3. **招人预算/周期未明**：选项 E 需 Ulysses 确认是否启动招聘 + 预算 + 招聘周期
4. **客户实际 deadline 未明**：H-3 决策后才有真实客户需求，选项 C 的 12 周可能不够第一个 PoC 验证
5. **里程碑 4（Voice + SDK 集成）vs 招人**：不在 MVP 范围，V1 时间表待 V1 启动时定
6. **token/周 实际波动**：1M/周是上限估算，AI 协作下多轮决策对话可能拉低效率（1M 实际可能 = 0.7M 实际产出）
7. **Ulysses 12 周连续可用性**：1 人公司 12 周全程无中断 = 强假设，需 Ulysses 自评

---

## 5. 修订历史

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版：5 个截止日选项 + 推荐 C（12 周）+ 4 里程碑日期 |
