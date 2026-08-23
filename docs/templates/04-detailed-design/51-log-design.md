---
doc_id: 51
title_ja: ログ設計書
title_zh: 日志设计
phase: 04-detailed-design
activity_no: 51
owners: Tech Lead + SRE
status: Draft
version: 1.0.0
---

# 51. ログ設計書 / 日志设计

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.51 (詳細設計 / Detailed Design)
> 责任方: Tech Lead + SRE

## 1. 目的 (Purpose)

定义日志级别、字段、脱敏、保留、查询。

## 2. 适用范围 (Scope)

为实现与运维提供统一规范。

## 3. 责任方 (Owners)

Tech Lead + SRE

## 4. 前置依赖 (Prerequisites / Inputs)

- 運用設計
- セキュリティ設計

## 5. 输出 / 模板正文 (Body)

### 1. 日志级别

| 级别 | 用途 | 示例 |
|---|---|---|
| ERROR | 影响业务的错误 | |
| WARN | 需关注但不阻塞 | |
| INFO | 关键业务事件 | |
| DEBUG | 调试信息(默认关闭) | |

### 2. 日志字段

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| ts | timestamp | ✅ | |
| trace_id | string | ✅ | |
| user_id | string |  | |
| msg | string | ✅ | |

### 3. 脱敏规则

| 字段 | 规则 |
|---|---|
| 密码 | 全脱敏 |
| 手机号 | 中间 4 位 |
| 身份证 | 仅保留地区码 |

### 4. 保留 / 归档

- 热:30 天
- 冷:1 年


## 6. 验收标准 (Acceptance Criteria)

日志级别 / 字段 / 脱敏 / 保留齐全;脱敏规则可执行。

## 7. 关联文档 (References)

- 上游: 37 運用設計
- 下游: 54 コーディング
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
