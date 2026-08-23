---
doc_id: 66
title_ja: IT 計画書
title_zh: 集成测试计划
phase: 07-integration-test
activity_no: 66
owners: QA + Tech Lead
status: Draft
version: 1.0.0
---

# 66. IT 計画書 / 集成测试计划

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.66 (結合試験 / Integration Test)
> 责任方: QA + Tech Lead

## 1. 目的 (Purpose)

定义 IT 范围、策略、环境、依赖。

## 2. 适用范围 (Scope)

包括 ITa(内部) / ITb(外部) / API / DB / 外部 IF 五大类。

## 3. 责任方 (Owners)

QA + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 基本設計
- IF 詳細
- NFR

## 5. 输出 / 模板正文 (Body)

### 1. 范围与策略

| 类型 | 范围 | 环境 | 主导方 |
|---|---|---|---|
| ITa(内部) | 模块间 | staging | 内部 |
| ITb(外部) | 与外部系统 | 联调 | 双方 |
| API 結合 | API 契约 | API 测试台 | 内部 |
| DB 結合 | DB 跨服务 | DB 测试台 | 内部 |
| 外部 IF | 完整端到端 | 联调 | 双方 |

### 2. 环境

- IT 环境:K3s 命名空间 / DB / 缓存
- 数据:脱敏副本 + 合成

### 3. 依赖

- 外部系统的联调时间窗
- 关键模块交付时间

### 4. 通过条件

- 全部用例通过
- 残留问题按既定 SLA 处置


## 6. 验收标准 (Acceptance Criteria)

5 类范围清楚;环境 / 依赖 / 通过条件齐全。

## 7. 关联文档 (References)

- 上游: 65 UT 完了
- 下游: 67 IT 仕様書
- 略称: IT Plan

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
