---
doc_id: 31
title_ja: ER 図
title_zh: ER 图(概念 / 逻辑)
phase: 03-basic-design
activity_no: 31
owners: DB 設計者 + BA
status: Draft
version: 1.0.0
---

# 31. ER 図 / ER 图(概念 / 逻辑)

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.31 (基本設計 / Basic Design)
> 责任方: DB 設計者 + BA

## 1. 目的 (Purpose)

用 ER 图表达数据模型,粒度到属性。

## 2. 适用范围 (Scope)

逻辑模型;物理分区 / 索引留给详细设计。

## 3. 责任方 (Owners)

DB 設計者 + BA

## 4. 前置依赖 (Prerequisites / Inputs)

- データ要件
- DB 基本設計書

## 5. 输出 / 模板正文 (Body)

### 1. 概念 ER

> 全局图(Mermaid / dbdiagram)

### 2. 主题域拆分

- 用户域:
- 消息域:
- 群组域:
- 计费域:

### 3. 实体清单

| 编号 | 实体 | 主题域 | 主键 | 关联 |
|---|---|---|---|---|
| | | | | |

### 4. 关键属性的枚举 / 字典

| 字段 | 取值 | 备注 |
|---|---|---|
| | | |


## 6. 验收标准 (Acceptance Criteria)

ER 覆盖所有主数据;属性完整;关键枚举有字典。

## 7. 关联文档 (References)

- 上游: 15 データ要件
- 下游: 47 DB 詳細設計
- 略称: ER

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
