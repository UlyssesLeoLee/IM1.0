---
doc_id: 71
title_ja: API 結合試験結果
title_zh: API 集成测试结果
phase: 07-integration-test
activity_no: 71
owners: QA + Tech Lead
status: Draft
version: 1.0.0
---

# 71. API 結合試験結果 / API 集成测试结果

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.71 (結合試験 / Integration Test)
> 责任方: QA + Tech Lead

## 1. 目的 (Purpose)

验证 API 契约一致性(请求 / 响应 / 错误码 / 性能)。

## 2. 适用范围 (Scope)

工具化(Contract Test)优先。

## 3. 责任方 (Owners)

QA + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- API 詳細仕様
- API 結合環境

## 5. 输出 / 模板正文 (Body)

### 1. 工具

- Pact / Dredd / 自研脚本

### 2. 覆盖

| 端点 | 用例数 | 通过 | 失败 |
|---|---|---|---|
| | | | |

### 3. 契约偏差

| 端点 | 实际 | 期望 | 处置 |
|---|---|---|---|

### 4. 结论

- [ ] 契约一致


## 6. 验收标准 (Acceptance Criteria)

每个端点有覆盖;契约偏差有处置。

## 7. 关联文档 (References)

- 上游: 46 API 詳細
- 下游: 72 DB 結合
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
