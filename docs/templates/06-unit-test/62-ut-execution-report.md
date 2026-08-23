---
doc_id: 62
title_ja: UT 結果報告書
title_zh: 单元测试结果
phase: 06-unit-test
activity_no: 62
owners: QA + 開発者
status: Draft
version: 1.0.0
---

# 62. UT 結果報告書 / 单元测试结果

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.62 (単体試験 / Unit Test)
> 责任方: QA + 開発者

## 1. 目的 (Purpose)

记录单测执行结果、通过 / 失败 / 跳过、覆盖率。

## 2. 适用范围 (Scope)

用于 UT 完了承認。

## 3. 责任方 (Owners)

QA + 開発者

## 4. 前置依赖 (Prerequisites / Inputs)

- UT 仕様
- CI 結果

## 5. 输出 / 模板正文 (Body)

### 1. 执行概要

- 工具 / 时间 / commit
- 用例总数:xx
- 通过 / 失败 / 跳过:

### 2. 覆盖率

| 模块 | Line | Branch |
|---|---|---|
| | | |

### 3. 失败用例

| 编号 | 描述 | 失败原因 | 状态 |
|---|---|---|---|
| | | | Open / Closed |

### 4. 结论

- [ ] 达到覆盖率目标
- [ ] 无 Critical 失败
- [ ] 可进入 IT


## 6. 验收标准 (Acceptance Criteria)

覆盖率与失败用例清楚;有进入 IT 的条件勾选。

## 7. 关联文档 (References)

- 上游: 61 UT Review
- 下游: 63 Bug Fix
- 略称: UT

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
