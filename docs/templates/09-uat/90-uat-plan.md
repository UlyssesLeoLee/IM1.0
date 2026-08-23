---
doc_id: 90
title_ja: UAT 計画書
title_zh: 验收测试计划
phase: 09-uat
activity_no: 90
owners: QA + 业务側(主导)
status: Draft
version: 1.0.0
---

# 90. UAT 計画書 / 验收测试计划

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.90 (受入試験 / UAT)
> 责任方: QA + 业务側(主导)

## 1. 目的 (Purpose)

由用户主导的验收计划,贴近真实业务。

## 2. 适用范围 (Scope)

用户是测试主体,IT / SRE 协助。

## 3. 责任方 (Owners)

QA + 业务側(主导)

## 4. 前置依赖 (Prerequisites / Inputs)

- ST 結果
- 業務シナリオ

## 5. 输出 / 模板正文 (Body)

### 1. 范围

- 全部核心业务场景
- 关键 NFR 的可观察性

### 2. 环境

- 本番环境(或用户指定)
- 真实数据(脱敏)

### 3. 参与方

| 角色 | 职责 |
|---|---|
| 业务用户 | 主导测试 |
| QA | 用例支持 |
| SRE | 环境 / 故障处理 |
| Dev | 问题修复 |

### 4. 通过条件

- 业务场景 100% 通过
- 残留问题有处置


## 6. 验收标准 (Acceptance Criteria)

范围 / 环境 / 参与方 / 通过条件齐全。

## 7. 关联文档 (References)

- 上游: 89 ST 完了
- 下游: 91 UAT 仕様
- 略称: UAT Plan

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
