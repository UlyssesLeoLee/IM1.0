---
doc_id: 29
title_ja: 外部 IF 詳細設計書
title_zh: 外部接口详细设计
phase: 03-basic-design
activity_no: 29
owners: BA + 開発者 + 外部担当
status: Draft
version: 1.0.0
---

# 29. 外部 IF 詳細設計書 / 外部接口详细设计

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.29 (基本設計 / Basic Design)
> 责任方: BA + 開発者 + 外部担当

## 1. 目的 (Purpose)

把 IF 要件落实到契约级:协议、消息结构、错误处理。

## 2. 适用范围 (Scope)

1 条 IF 1 份;关键 IF 需对方联签。

## 3. 责任方 (Owners)

BA + 開発者 + 外部担当

## 4. 前置依赖 (Prerequisites / Inputs)

- IF 要件一覧
- 外部システム仕様

## 5. 输出 / 模板正文 (Body)

### IF-DET-001 <接口名>

**方向**: Inbound / Outbound
**协议**: HTTP / gRPC / MQ / WebSocket
**鉴权**:
**频度**:
**SLA**:

#### 消息结构

```json / protobuf
{ ... }
```

#### 错误处理

| 错误 | 行为 | 重试策略 |
|---|---|---|
| | | |

#### 双方责任

- 己方:
- 对方:

#### 联签

| 角色 | 签字 |
|---|---|
| 己方 SA | |
| 对方接口担当 | |


## 6. 验收标准 (Acceptance Criteria)

每条 IF 有完整契约 + 错误处理 + 责任划分 + 联签。

## 7. 关联文档 (References)

- 上游: 16 IF 要件
- 下游: 73 外部システム連携試験
- 略称: IF

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
