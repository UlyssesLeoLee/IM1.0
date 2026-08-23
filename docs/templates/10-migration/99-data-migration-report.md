---
doc_id: 99
title_ja: データ移行結果
title_zh: 数据迁移结果
phase: 10-migration
activity_no: 99
owners: DBA + SRE
status: Draft
version: 1.0.0
---

# 99. データ移行結果 / 数据迁移结果

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.99 (移行 / Migration)
> 责任方: DBA + SRE

## 1. 目的 (Purpose)

记录数据迁移的执行与校验。

## 2. 适用范围 (Scope)

完整性 100% 是底线。

## 3. 责任方 (Owners)

DBA + SRE

## 4. 前置依赖 (Prerequisites / Inputs)

- 移行手順書
- リハ結果

## 5. 输出 / 模板正文 (Body)

| 表 | 源记录数 | 目标记录数 | 校验 |
|---|---|---|---|
| | | | ✅ / ❌ |

### 关键校验

- 合计 / 抽样 / 业务校验


## 6. 验收标准 (Acceptance Criteria)

每表有:源 / 目标 / 校验。

## 7. 关联文档 (References)

- 上游: 98 リハ
- 下游: 100 システム移行
- 略称: Data Migration

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
