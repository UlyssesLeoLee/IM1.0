---
doc_id: 127
title_ja: 品質計画書
title_zh: 质量计划
phase: 14-quality
activity_no: 127
owners: QA Lead
status: Draft
version: 1.0.0
---

# 127. 品質計画書 / 质量计划

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.127 (品質管理 / Quality)
> 责任方: QA Lead

## 1. 目的 (Purpose)

定义全生命周期的质量目标、指标、评审节点。

## 2. 适用范围 (Scope)

与项目计划一并发布。

## 3. 责任方 (Owners)

QA Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 企画書
- 要件

## 5. 输出 / 模板正文 (Body)

### 1. 质量目标

| 维度 | 目标 |
|---|---|
| 缺陷密度 | < X 个 / KLOC |
| 缺陷逃逸率 | < Y% |
| NFR 达成率 | 100% |
| 客户满意度 | ≥ Z 分 |

### 2. 质量活动

| 阶段 | 活动 | 产出 |
|---|---|---|
| 要件 | 评审 | 决议 |
| 設計 | 评审 | 决议 |
| 実装 | CR + SAST | 报告 |
| 試験 | 各阶段评审 | 报告 |
| リリース | GATE 决议 | 记录 |

### 3. 度量与改进


## 6. 验收标准 (Acceptance Criteria)

目标 / 活动 / 度量齐。

## 7. 关联文档 (References)

- 上游: 21 Baseline
- 下游: 128 QA Review
- 略称: QA Plan

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
