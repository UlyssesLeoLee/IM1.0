---
doc_id: 14
title_ja: NFR マトリクス
title_zh: 非功能需求矩阵
phase: 02-requirements
activity_no: 14
owners: SA + 技術リード
status: Draft
version: 1.0.0
---

# 14. NFR マトリクス / 非功能需求矩阵

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.14 (要件定義 / Requirements)
> 责任方: SA + 技術リード

## 1. 目的 (Purpose)

明确性能 / 可用性 / 维护性 / 安全性 / 合规等非功能目标及度量方式。

## 2. 适用范围 (Scope)

NFR 是性能 / 负载 / 故障 / 安全试验的判定基准。

## 3. 责任方 (Owners)

SA + 技術リード

## 4. 前置依赖 (Prerequisites / Inputs)

- 企画書(SLA / 成功指标)
- システム要件定義書

## 5. 输出 / 模板正文 (Body)

| 类别 | 编号 | 指标 | 目标值 | 度量方式 | 备注 |
|---|---|---|---|---|---|
| 性能 | NFR-P-001 | 端到端 P99 延迟 | < 200ms | 压测 | |
| 性能 | NFR-P-002 | 峰值 TPS | 10k | 压测 | |
| 可用性 | NFR-A-001 | 月度可用性 | 99.95% | 监控 | |
| 容量 | NFR-C-001 | 在线用户 | 100 万 | 监控 | |
| 维护性 | NFR-M-001 | MTTR | < 30min | 运维 | |
| 安全 | NFR-S-001 | 漏洞等级 | High = 0 | 扫描 | |
| 合规 | NFR-L-001 | 法规 | 列出 | 审计 | |


## 6. 验收标准 (Acceptance Criteria)

NFR 全部有量化目标;有度量方式;有责任人。

## 7. 关联文档 (References)

- 上游: 12 システム要件定義
- 下游: 76 システム試験計画
- 略称: NFR

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
