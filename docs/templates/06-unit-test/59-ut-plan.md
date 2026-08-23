---
doc_id: 59
title_ja: UT 計画書
title_zh: 单元测试计划
phase: 06-unit-test
activity_no: 59
owners: QA + Tech Lead
status: Draft
version: 1.0.0
---

# 59. UT 計画書 / 单元测试计划

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.59 (単体試験 / Unit Test)
> 责任方: QA + Tech Lead

## 1. 目的 (Purpose)

定义单测范围、目标覆盖率、工具、Mock 策略。

## 2. 适用范围 (Scope)

评审通过后冻结;变更须重审。

## 3. 责任方 (Owners)

QA + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 詳細設計書
- API 詳細仕様

## 5. 输出 / 模板正文 (Body)

### 1. 范围

- In Scope:模块 / 公共 API / 错误处理路径
- Out of Scope:UI 集成、跨服务、数据库(交给 IT)

### 2. 目标

| 指标 | 目标 |
|---|---|
| Line Coverage | ≥ 80% |
| Branch Coverage | ≥ 70% |
| Critical 模块 | ≥ 90% |

### 3. 工具

- Rust:cargo test / cargo-tarpaulin
- TS:vitest / jest
- Mock:mockall / msw

### 4. Mock / Stub 策略

- 外部 HTTP / gRPC:必 mock
- DB:在 IT 层做;UT 不直接打

### 5. 失败处理

- 阻塞发布:覆盖率不达标、Critical 用例未通过


## 6. 验收标准 (Acceptance Criteria)

范围 / 目标 / 工具 / Mock / 失败处理齐全。

## 7. 关联文档 (References)

- 上游: 52 DD Review
- 下游: 60 UT 仕様書
- 略称: UT Plan

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
