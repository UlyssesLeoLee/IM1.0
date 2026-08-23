---
doc_id: 47
title_ja: DB 詳細設計書
title_zh: 数据库详细设计
phase: 04-detailed-design
activity_no: 47
owners: DB 設計者
status: Draft
version: 1.0.0
---

# 47. DB 詳細設計書 / 数据库详细设计

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.47 (詳細設計 / Detailed Design)
> 责任方: DB 設計者

## 1. 目的 (Purpose)

把 ER / DB 基本设计落实到物理 DDL、索引、分区。

## 2. 适用范围 (Scope)

DDL 是迁移脚本的源头;不替代迁移脚本。

## 3. 责任方 (Owners)

DB 設計者

## 4. 前置依赖 (Prerequisites / Inputs)

- DB 基本設計書
- ER 図

## 5. 输出 / 模板正文 (Body)

### 1. 物理模型

> Mermaid erDiagram / SQL DDL

### 2. 索引策略

| 表 | 索引 | 目的 |
|---|---|---|

### 3. 分区 / 分表

| 表 | 策略 | 分区键 |
|---|---|---|

### 4. 关键 DDL

```sql
CREATE TABLE ...;
```

### 5. 数据迁移影响

- 是否需要双写 / 回填


## 6. 验收标准 (Acceptance Criteria)

物理模型完整;索引有理由;DDL 可直接执行。

## 7. 关联文档 (References)

- 上游: 30 DB 基本設計, 31 ER 図
- 下游: 48 SQL 設計
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
