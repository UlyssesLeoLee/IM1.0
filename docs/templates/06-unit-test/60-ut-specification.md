---
doc_id: 60
title_ja: UT 仕様書
title_zh: 单元测试规格
phase: 06-unit-test
activity_no: 60
owners: 開発者 + QA
status: Draft
version: 1.0.0
---

# 60. UT 仕様書 / 单元测试规格

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.60 (単体試験 / Unit Test)
> 责任方: 開発者 + QA

## 1. 目的 (Purpose)

为每个模块编写测试用例:正常 / 异常 / 边界 / 性能。

## 2. 适用范围 (Scope)

用例可被工具读取或半自动执行。

## 3. 责任方 (Owners)

開発者 + QA

## 4. 前置依赖 (Prerequisites / Inputs)

- UT Plan
- API 詳細仕様

## 5. 输出 / 模板正文 (Body)

### MOD-001 <模块名>

| 编号 | 类别 | 描述 | 输入 | 预期输出 | 优先级 |
|---|---|---|---|---|---|
| TC-001 | 正常 | 合法输入 | | | P0 |
| TC-002 | 异常 | 空输入 | | | P0 |
| TC-003 | 边界 | 极值 | | | P1 |
| TC-004 | 性能 | < 10ms | | | P2 |

(每个模块 1 张子表)


## 6. 验收标准 (Acceptance Criteria)

每模块有正常 / 异常 / 边界 / 性能四类用例。

## 7. 关联文档 (References)

- 上游: 59 UT Plan
- 下游: 61 UT Review
- 略称: UT

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
