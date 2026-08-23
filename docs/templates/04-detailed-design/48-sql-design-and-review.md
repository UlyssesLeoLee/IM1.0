---
doc_id: 48
title_ja: SQL 設計 + レビュー記録
title_zh: SQL 设计与评审记录
phase: 04-detailed-design
activity_no: 48
owners: DBA + 開発者
status: Draft
version: 1.0.0
---

# 48. SQL 設計 + レビュー記録 / SQL 设计与评审记录

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.48 (詳細設計 / Detailed Design)
> 责任方: DBA + 開発者

## 1. 目的 (Purpose)

为关键 SQL 提供设计与评审记录,避免生产环境性能事故。

## 2. 适用范围 (Scope)

每条关键 SQL 都有:说明 / 执行计划 / 评审人。

## 3. 责任方 (Owners)

DBA + 開発者

## 4. 前置依赖 (Prerequisites / Inputs)

- DB 詳細設計書

## 5. 输出 / 模板正文 (Body)

### SQL-001 <名称>

**业务目的**:
**预期返回行数**:
**调用频度**:

#### SQL

```sql
SELECT ...
```

#### 执行计划(EXPLAIN ANALYZE)

```
...
```

#### 评审结论

| 评审人 | 意见 | 日期 |
|---|---|---|
| | | |


## 6. 验收标准 (Acceptance Criteria)

每条关键 SQL 有执行计划与评审结论。

## 7. 关联文档 (References)

- 上游: 47 DB 詳細設計
- 下游: 62 単体試験実施
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
