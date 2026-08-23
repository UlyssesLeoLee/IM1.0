---
doc_id: 30
title_ja: DB 基本設計書
title_zh: 数据库基本设计
phase: 03-basic-design
activity_no: 30
owners: DB 設計者 + Tech Lead
status: Draft
version: 1.0.0
---

# 30. DB 基本設計書 / 数据库基本设计

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.30 (基本設計 / Basic Design)
> 责任方: DB 設計者 + Tech Lead

## 1. 目的 (Purpose)

确定 DB 种类、库 / 表群、容量估算、备份策略。

## 2. 适用范围 (Scope)

为详细设计的物理 DDL 提供输入。

## 3. 责任方 (Owners)

DB 設計者 + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- データ要件
- NFR(容量/性能)

## 5. 输出 / 模板正文 (Body)

### 1. DB 选型

| 用途 | DB | 理由 |
|---|---|---|
| 业务 OLTP | PostgreSQL | |
| 缓存 | Redis | |
| 检索 | OpenSearch | |
| 时序 | InfluxDB | |
| 离线分析 | ClickHouse | |

### 2. 库 / Schema 划分

| 库 | 用途 | 数据量预估(3y) | 增长率 |
|---|---|---|---|
| | | | |

### 3. 容量估算

| 表 | 行数(3y) | 单行大小 | 总大小 |
|---|---|---|---|
| | | | |

### 4. 备份 / 高可用策略(高层)


## 6. 验收标准 (Acceptance Criteria)

DB 选型有理由;库划分清楚;3 年容量可估算。

## 7. 关联文档 (References)

- 上游: 15 データ要件
- 下游: 47 DB 詳細設計
- 略称: DB

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
