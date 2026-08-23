---
doc_id: 16
title_ja: 外部 IF 要件一覧
title_zh: 外部接口需求清单
phase: 02-requirements
activity_no: 16
owners: BA + 外部システム担当
status: Draft
version: 1.0.0
---

# 16. 外部 IF 要件一覧 / 外部接口需求清单

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.16 (要件定義 / Requirements)
> 责任方: BA + 外部システム担当

## 1. 目的 (Purpose)

梳理与外部系统的所有交互点,明确契约级要求。

## 2. 适用范围 (Scope)

下游 IF 设计与外部系统对接的依据。

## 3. 责任方 (Owners)

BA + 外部システム担当

## 4. 前置依赖 (Prerequisites / Inputs)

- 業務要件定義書
- 外部システム仕様

## 5. 输出 / 模板正文 (Body)

| 编号 | 外部系统 | 方向 | 数据 / 事件 | 频度 | 协议 | SLA | 责任方 |
|---|---|---|---|---|---|---|---|
| IF-REQ-001 | | Inbound/Outbound | | | HTTP/gRPC/MQ | | |


## 6. 验收标准 (Acceptance Criteria)

每条 IF 有:数据/事件定义 + 协议 + 频度 + SLA + 责任方。

## 7. 关联文档 (References)

- 上游: 13 機能要件定義
- 下游: 29 外部インターフェース設計
- 略称: IF

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
