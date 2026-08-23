---
doc_id: 24
title_ja: アーキテクチャ図 + ADR
title_zh: 架构图 + 架构决策记录
phase: 03-basic-design
activity_no: 24
owners: SA
status: Draft
version: 1.0.0
---

# 24. アーキテクチャ図 + ADR / 架构图 + 架构决策记录

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.24 (基本設計 / Basic Design)
> 责任方: SA

## 1. 目的 (Purpose)

用图 + 决策记录说明架构选型,作为后续所有设计的源头。

## 2. 适用范围 (Scope)

图(组件 / 部署 / 时序) + ADR(决策日志)。

## 3. 责任方 (Owners)

SA

## 4. 前置依赖 (Prerequisites / Inputs)

- ソフトウェア方式書

## 5. 输出 / 模板正文 (Body)

## A. 架构图

### A.1 组件图

> C4 model / Mermaid / draw.io

### A.2 部署图

### A.3 关键场景时序图

- 场景 1:用户发送消息
- 场景 2:管理员封禁
- 场景 3:...

## B. ADR(Architecture Decision Records)

### ADR-001: 选择 X 作为消息中间件

**状态**: Accepted
**日期**:
**背景**:
**决定**:
**备选**:
**影响**:

(更多 ADR ...)


## 6. 验收标准 (Acceptance Criteria)

至少有组件图 + 部署图 + 关键时序图;每条关键决定有 ADR。

## 7. 关联文档 (References)

- 上游: 23 ソフトウェア方式設計
- 下游: 25 機能設計
- 略称: Architecture

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
