---
doc_id: 27
title_ja: 帳票定義書
title_zh: 报表设计书
phase: 03-basic-design
activity_no: 27
owners: BA + 開発者
status: Draft
version: 1.0.0
---

# 27. 帳票定義書 / 报表设计书

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.27 (基本設計 / Basic Design)
> 责任方: BA + 開発者

## 1. 目的 (Purpose)

定义所有输出报表的布局、数据源、生成方式。

## 2. 适用范围 (Scope)

覆盖全部需输出报表;含 PDF / Excel / CSV 等。

## 3. 责任方 (Owners)

BA + 開発者

## 4. 前置依赖 (Prerequisites / Inputs)

- FR(报表相关)
- データ要件

## 5. 输出 / 模板正文 (Body)

| 编号 | 报表名 | 数据源 | 输出格式 | 频度 | 关联 FR | 接收人 |
|---|---|---|---|---|---|---|
| RPT-001 | | | PDF/XLSX/CSV | | | |

### 详细定义(每张报表)

#### RPT-001 销售月报

- 标题 / 期间 / 汇总维度
- 字段清单(类型 / 格式 / 脱敏要求)
- 排序 / 分组 / 过滤
- 模板示例


## 6. 验收标准 (Acceptance Criteria)

每张报表有完整字段清单 + 模板示例;数据源可追溯。

## 7. 关联文档 (References)

- 上游: 25 機能設計
- 下游: 32 バッチ設計
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
