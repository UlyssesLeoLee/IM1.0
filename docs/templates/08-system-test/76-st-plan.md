---
doc_id: 76
title_ja: ST 計画書
title_zh: 系统测试计划
phase: 08-system-test
activity_no: 76
owners: QA + SRE + Tech Lead
status: Draft
version: 1.0.0
---

# 76. ST 計画書 / 系统测试计划

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.76 (システム試験 / System Test)
> 责任方: QA + SRE + Tech Lead

## 1. 目的 (Purpose)

定义系统测试范围、策略、环境、工具、入口 / 出口条件。

## 2. 适用范围 (Scope)

本计划经 PMO 批准后冻结。

## 3. 责任方 (Owners)

QA + SRE + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- NFR
- SR
- アーキテクチャ

## 5. 输出 / 模板正文 (Body)

### 1. 范围

- In Scope:功能 / 性能 / 负载 / 压力 / 安全 / 恢复 / B/R / 可用性 / 运维
- Out of Scope:已经 UAT 覆盖的纯业务验收

### 2. 策略

- 生产相当环境(staging-prod)
- 真实数据(脱敏) + 合成
- 自动化为主,手工补足

### 3. 入口 / 出口

- 入口:IT 完了承認(75)
- 出口:全部用例通过 + NFR 达成 + 残留问题已批准


## 6. 验收标准 (Acceptance Criteria)

范围 / 策略 / 入口 / 出口齐全;NFR 可追溯。

## 7. 关联文档 (References)

- 上游: 75 回帰
- 下游: 77 ST 仕様
- 略称: ST Plan

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
