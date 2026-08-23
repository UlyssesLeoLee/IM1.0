---
doc_id: 55
title_ja: SAST レポート
title_zh: 静态分析报告
phase: 05-implementation
activity_no: 55
owners: DevOps + セキュリティ
status: Draft
version: 1.0.0
---

# 55. SAST レポート / 静态分析报告

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.55 (実装 / Implementation)
> 责任方: DevOps + セキュリティ

## 1. 目的 (Purpose)

记录每次 SAST 扫描结果,跟踪 Critical / High 问题关闭情况。

## 2. 适用范围 (Scope)

高优问题 24h 内关闭;中优 7d;低优下次迭代。

## 3. 责任方 (Owners)

DevOps + セキュリティ

## 4. 前置依赖 (Prerequisites / Inputs)

- 代码 + SAST 工具(clippy / eslint-security / semgrep)

## 5. 输出 / 模板正文 (Body)

### 1. 扫描基本信息

- 工具 / 版本
- 范围(模块 / commit)
- 时间

### 2. 结果汇总

| 等级 | 数量 | 与上次对比 |
|---|---|---|
| Critical | | |
| High | | |
| Medium | | |
| Low | | |

### 3. 详细问题清单

| 编号 | 等级 | 位置 | 规则 | 状态 |
|---|---|---|---|---|
| | | | | Open / Closed |

### 4. SLA

- Critical:24h 内
- High:7d
- Medium:下次迭代
- Low:Backlog


## 6. 验收标准 (Acceptance Criteria)

每次扫描有报告;问题有等级 + SLA + 状态。

## 7. 关联文档 (References)

- 上游: 54 コーディング
- 下游: 56 コードレビュー, 58 CI
- 略称: SAST

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
