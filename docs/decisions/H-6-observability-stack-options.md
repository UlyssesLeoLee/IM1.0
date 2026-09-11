---
doc_id: H-6
title_zh: 监控/日志/IM 沟通工具选型（选项清单）
status: Draft for PM Decision
owner: PM (Ulysses) / 调研: 架构师 (Mavis 接手 agent per DEC-008)
date: 2026-09-01 JST
wbs_ref: 132-wbs.md v1.0.0 §5.8 (H-6)
upstream: Observability.md v1.0.0, BasicDesign.md §14, Project-Status.md §2.2
---

# H-6. 监控/日志/IM 沟通工具选型（选项清单）

> **本文档性质**：Mavis 调研输出，**3-5 个 stack 候选 + 成本/复杂度对比**。**最终拍板由 PM (Ulysses) 用 ask_user 选项决定**，Mavis 不代签。
>
> **数据基线**：Observability.md v1.0.0 §5 已选 Grafana + Prometheus + Loki + Tempo + OTel Collector（这是 Observability Stack，**已选，本调研不重新评估**）
>
> **H-6 真正要选的是**：
> 1. **IM 沟通工具**（经营/投资人/团队日常沟通 + 告警通道）
> 2. **Observability 后端部署形态**（自建 K3s vs SaaS）
> 3. **告警通道集成**（IM 工具的 Webhook 接入 OTel Alertmanager）
>
> **强约束**（per Ulysses 2026-09-01 13:03/13:05 JST）：
> - 反向代理 / 边缘层默认 envoy 独立 deployment, 不选 nginx, 不选 istio sidecar
> - 此约束不影响 IM 沟通工具选型，但影响自建 stack 时的 ingress
>
> **强约束**（1 人公司）：Mavis 上限 1M tokens/周 = 运维 1 个 Grafana + 1 个 IM 工具 = 0.5 token/周给基础设施 = 实际不应自建重组件

---

## 1. 选型范围澄清

### 1.1 Observability Stack（已选，per Observability.md §5.7）

```
Grafana  ─ 查询 + Dashboard + Alert
   │
   ├── Prometheus (Metrics, 30d)
   ├── Loki (Logs, 30d, 压缩)
   └── Tempo (Traces, 7d, 对象存储)
                ▲
                │ OTLP
                │
       OTel Collector (im1-obs ns)
                ▲
                │ OTLP
                │
       业务服务(各 ns)统一用 OTel SDK
```

**H-6 不再评估"是否用 Grafana / Prometheus / Loki / Tempo"**——这是 2026-08-20 OBS-ARCH-001~006 已锁定

### 1.2 H-6 真正要选 3 件事

| 选型 | 选项范围 | 关键权衡 |
|---|---|---|
| **IM 沟通工具** | 飞书 / Slack / Discord / 钉钉 / Rocket.Chat / 企业微信 | 国内访问 / 海外通用 / 社区 / 自建 |
| **Observability 后端形态** | 自建 K3s (Prometheus/Loki/Tempo) / Grafana Cloud SaaS / Datadog SaaS / New Relic SaaS | 成本 / 运维负担 / 数据位置 |
| **告警通道** | 上述 IM 工具的 Webhook + Alertmanager 集成 | 与 IM 工具同栈 |

---

## 2. Stack 候选（5 个）

### Stack A: 飞书 + Grafana Cloud (SaaS Free Tier)

#### 组件
- **IM 沟通**：飞书 Lark（国内访问快，中文原生，集成日程/文档/审批）
- **Observability 后端**：Grafana Cloud Free Tier（10K 指标 / 50GB Logs / 50GB Traces / 14 天保留）
- **告警通道**：飞书 Webhook → Alertmanager
- **边缘代理**：envoy 独立 deployment（per 2026-09-01 13:05 JST 约束）
- **K3s Ingress**：envoy（per 2026-09-01 13:03 JST 约束）

#### 成本（USD/月）
| 项 | 数量 | 单价 | 小计 |
|---|---|---|---|
| 飞书 免费版 | 团队 ≤ 50 人 | $0 | $0 |
| Grafana Cloud Free | 1 active series 10K | $0 | $0 |
| K3s 自建 | 1 节点 4 vCPU 8GB | 已有 | $0 |
| envoy 独立 deployment | 1 pod | $0 (软件) | $0 |
| **合计** | | | **$0/月** |

#### 复杂度
- **运维负担**：低（Grafana Cloud 管后端，K3s 上只跑 im 服务）
- **数据位置**：Grafana Cloud = 海外（可能需考虑 CN 数据本地化）
- **升级路径**：Free Tier 超限 → Pro $8/active series/月
- **1 人公司适配**：✅ 1 人可运维

#### 适合
- 经营/团队在 CN / JP / 东南亚
- 目标客户是国内/亚洲游戏开发商
- 数据本地化要求不严（Free Tier 是海外节点）

---

### Stack B: Slack + 自建 K3s (Prometheus/Loki/Tempo) ⭐ 推荐 之一

#### 组件
- **IM 沟通**：Slack（全球通用，游戏开发者社区主流）
- **Observability 后端**：自建 K3s Prometheus + Loki + Tempo（per Observability.md §5）
- **告警通道**：Slack Webhook → Alertmanager
- **边缘代理**：envoy 独立 deployment
- **数据存储**：PG 18.6 (已有) + S3 兼容 (MinIO) for Tempo traces

#### 成本（USD/月）
| 项 | 数量 | 单价 | 小计 |
|---|---|---|---|
| Slack 免费版 | 历史 90 天 + 1k 消息 | $0 | $0 |
| K3s 节点 | 1 节点 4 vCPU 8GB | 已有 | $0 |
| Prometheus 存储 | 30d × 1GB | $0 (本地 PV) | $0 |
| Loki 存储 | 30d × 5GB | $0 (本地 PV) | $0 |
| Tempo 存储 | 7d × 2GB | $0 (MinIO 已有) | $0 |
| 1 SRE 运维 | 1 人公司 1 周 1h 维护 | — | (token cost) |
| **合计** | | | **$0/月 + token 成本** |

#### 复杂度
- **运维负担**：中（K3s + 4 个组件 + 1 SRE 维护）
- **数据位置**：完全本地（私有化部署）
- **升级路径**：自建可扩，但需 SRE 投入
- **1 人公司适配**：⚠️ Prometheus + Loki + Tempo 维护需 ~1-2h/周 = 4K-8K tokens/周

#### 适合
- 经营/团队在海外或多区域
- 目标客户是海外游戏开发商
- 数据本地化要求严

---

### Stack C: Rocket.Chat (自建) + 自建 Observability

#### 组件
- **IM 沟通**：Rocket.Chat 自建（K3s，开源 Apache-2.0）
- **Observability 后端**：自建 K3s Prometheus + Loki + Tempo
- **告警通道**：Rocket.Chat Webhook → Alertmanager
- **边缘代理**：envoy 独立 deployment

#### 成本（USD/月）
| 项 | 数量 | 单价 | 小计 |
|---|---|---|---|
| Rocket.Chat 自建 | 1 pod + MongoDB 1 pod | 已有 K3s | $0 |
| MongoDB (Rocket.Chat 依赖) | 1 StatefulSet | 已有 PV | $0 |
| 运维负担 | 1 SRE × 2-3h/周 | — | (token cost) |
| **合计** | | | **$0/月 + token 成本** |

#### 复杂度
- **运维负担**：**高**（Rocket.Chat + MongoDB + 4 个 Observability 组件 = 5 个系统）
- **数据位置**：完全本地
- **升级路径**：自建可扩
- **1 人公司适配**：❌ **不推荐**（5 个系统维护 ≥ 4-6h/周 = 16K-24K tokens/周，挤占核心开发）

#### 适合
- 强合规要求（数据全本地 + 自托管 IM）
- 多区域团队协作
- 1 人公司**不适合**

---

### Stack D: Discord + Grafana Cloud ⭐ 推荐 之一

#### 组件
- **IM 沟通**：Discord（游戏开发者社区事实标准，免费、永久历史、机器人集成强）
- **Observability 后端**：Grafana Cloud Free Tier
- **告警通道**：Discord Webhook → Alertmanager
- **边缘代理**：envoy 独立 deployment

#### 成本（USD/月）
| 项 | 数量 | 单价 | 小计 |
|---|---|---|---|
| Discord 免费版 | 无限 | $0 | $0 |
| Grafana Cloud Free | 1 active series 10K | $0 | $0 |
| K3s 自建 | 1 节点 | 已有 | $0 |
| **合计** | | | **$0/月** |

#### 复杂度
- **运维负担**：低（Grafana Cloud 管后端）
- **数据位置**：Grafana Cloud = 海外
- **1 人公司适配**：✅ 1 人可运维
- **附加价值**：Discord 服务器本身就是"用户社区"，可作 IM1.0 早期反馈渠道

#### 适合
- 目标客户是游戏开发者（天然在 Discord 社区）
- 经营/团队海外
- 1 人公司 MVP

---

### Stack E: 钉钉 + 自建 Observability

#### 组件
- **IM 沟通**：钉钉（国内企业首选）
- **Observability 后端**：自建 K3s Prometheus + Loki + Tempo
- **告警通道**：钉钉 Webhook → Alertmanager
- **边缘代理**：envoy 独立 deployment

#### 成本（USD/月）
| 项 | 数量 | 单价 | 小计 |
|---|---|---|---|
| 钉钉免费版 | ≤ 50 人 | $0 | $0 |
| 自建 Observability | 1 SRE 1-2h/周 | — | (token cost) |
| **合计** | | | **$0/月 + token 成本** |

#### 复杂度
- **运维负担**：中
- **数据位置**：完全本地
- **1 人公司适配**：⚠️ 与 Stack B 类似

#### 适合
- 纯国内 B 端（不面向 C 端游戏玩家）
- 经营/团队在 CN

---

## 3. Stack 对比矩阵

| 维度 | A 飞书+GC | B Slack+自建 | C RC+自建 | D Discord+GC | E 钉钉+自建 |
|---|---|---|---|---|---|
| 月成本 | $0 | $0 + token | $0 + token | $0 | $0 + token |
| 运维负担 | 低 | 中 | **高** | 低 | 中 |
| 1 人公司适配 | ✅ | ⚠️ | ❌ | ✅ | ⚠️ |
| 数据位置 | 海外 | 本地 | 本地 | 海外 | 本地 |
| 目标客户 | CN/亚洲 | 全球 | 全合规 | 游戏社区 | 纯 CN B |
| 社区属性 | 弱 | 中 | 弱 | **强** | 弱 |
| 告警延迟 | < 5s | < 5s | < 5s | < 5s | < 5s |
| 历史消息 | 长期 | 90d 免费 | 长期 | 长期 | 长期 |
| 集成能力 | 中 | **强** | 中 | **强** | 中 |

---

## 4. 推荐

### 推荐：**Stack B (Slack + 自建 K3s)** 或 **Stack D (Discord + Grafana Cloud)**

**两栈都满足 1 人公司 + MVP 起步成本 $0**

### 推荐 Stack D (Discord + Grafana Cloud) 作为首个 MVP 起点

#### 推荐理由
1. **1 人公司最低运维**：Grafana Cloud Free Tier 包后端 → 0 SRE 投入
2. **目标客户贴合**：IM1.0 目标 = 游戏开发者，**天然在 Discord 社区**
3. **社区属性**：Discord 服务器可作 IM1.0 早期反馈 + 用户支持渠道
4. **永久历史**：Discord 免费版历史无限（vs Slack 90 天）
5. **机器人生态强**：可写机器人自动回复 + 集成 GitHub/CI
6. **国内访问**：Discord 国内访问受限（⚠️ 唯一风险）

#### Stack D 风险
- **国内访问慢/被墙**：如经营/团队在 CN → 改 Stack A
- **Grafana Cloud Free Tier 限制**：10K active series / 50GB / 14 天保留（超限需升级 $8/active/月）

### 推荐 Stack B (Slack + 自建) 作为 V1 升级路径

#### 何时切换
- Grafana Cloud Free Tier 超限 → 自建 Prometheus/Loki/Tempo
- 需要私有化部署交付 → 自建
- 数据本地化要求严 → 自建

#### 1 人公司成本估算
- 自建维护 1-2h/周 = 4K-8K tokens/周
- 4 周 / 月 = 16K-32K tokens/月 = 0.016-0.032M tokens/月
- 在 Mavis 上限 1M/周 (4M/月) 内 = < 1% 占比

---

## 5. 不推荐

### Stack C (Rocket.Chat 自建)
- **运维负担重**：5 个系统（RC + Mongo + Prom + Loki + Tempo）
- 1 人公司**不推荐**

### 自建 IM + 自建 Observability 组合
- 1 人公司无法承担
- 仅在合规强制要求时考虑（V1+ 评估）

---

## 6. 派生决策（已锁定，per Ulysses 2026-09-01 13:03/13:05 JST）

| 决策 | 内容 | 适用 |
|---|---|---|
| **反向代理** | envoy（不选 nginx） | k3s ingress / 静态资源 / reverse proxy / mTLS |
| **部署模式** | envoy 独立 deployment（不选 istio sidecar） | 所有边缘层 |
| **不引入** | nginx / istio 控制面 + sidecar 自动注入 | — |

**保留**：仅当 envoy 实在不可达 / 旧 BPO 强约束才退 nginx，需在 commit + 修订历史写明

---

## 7. 已知缺口

### 7.1 业务缺口
1. **目标市场未明**：CN / JP / US / 欧洲 / 东南亚中哪些是真实市场？决定 IM 工具选型
2. **目标客户品类未明**：游戏 vs 企业 vs 通用
3. **经营/团队位置未明**：CN / 海外？影响飞书 vs Slack vs Discord
4. **Discord 国内访问**：GFW 阻断风险

### 7.2 技术缺口
5. **Grafana Cloud Free Tier 是否够用**：未知实际指标量级（需 PoC-01 跑过估算）
6. **Slack 国内访问**：与 Discord 类似风险
7. **告警通道 SLA**：5 个 stack 都是 webhook，延迟 1-5s，能否满足 IM Core SLO < 5min 通知（per Observability §3 NFR）
8. **飞书/钉钉 海外团队访问**：可能需海外节点 / VPN
9. **Grafana Cloud 区域选择**：可能需选 JP 节点（CN/JP 客户）

### 7.3 流程缺口
10. **告警值班流程未明**：1 人公司 7×24 告警响应 = 不可能
11. **Critical vs Info 告警分级**：per Observability §3 NFR 但未在 H-6 决策中
12. **告警去重 / 抑制策略**：Alertmanager 配置未在 H-6 范围

### 7.4 法规缺口
13. **告警消息含敏感数据**：CN PIPL / JP APPI 都对消息内容有约束，告警内容需脱敏
14. **海外 IM 工具数据传输**：飞书/钉钉国内服务器 vs Slack/Discord 海外

---

## 8. 修订历史

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版：5 个 stack 候选 + 推荐 D (Discord+GC) MVP / B (Slack+自建) V1 升级 + 派生决策锁定 envoy |
