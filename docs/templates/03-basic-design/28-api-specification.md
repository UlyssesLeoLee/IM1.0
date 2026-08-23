---
doc_id: 28
title_ja: API 仕様書(OpenAPI / proto)
title_zh: API 规格书
phase: 03-basic-design
activity_no: 28
owners: Tech Lead + 開発者
status: Draft
version: 1.0.0
---

# 28. API 仕様書(OpenAPI / proto) / API 规格书

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.28 (基本設計 / Basic Design)
> 责任方: Tech Lead + 開発者

## 1. 目的 (Purpose)

定义对外 / 对内 API 的契约;冻结于基线化时点。

## 2. 适用范围 (Scope)

OpenAPI 3.0+ 或 gRPC proto;每个端点有:描述 / 参数 / 响应 / 错误码。

## 3. 责任方 (Owners)

Tech Lead + 開発者

## 4. 前置依赖 (Prerequisites / Inputs)

- 機能設計書
- IF 要件

## 5. 输出 / 模板正文 (Body)

### 1. API 风格

- REST(OpenAPI 3.0+)/ gRPC / WebSocket

### 2. 鉴权

### 3. 错误码约定

| 类别 | 范围 | 含义 |
|---|---|---|
| 客户端 | 4xx | 请求错误 |
| 服务端 | 5xx | 系统错误 |

### 4. 端点清单

| 编号 | 方法 | 路径 / 服务 | 描述 | 关联 FR |
|---|---|---|---|---|
| API-001 | POST /v1/... | | | FR-001 |

### 5. OpenAPI / proto 文件位置

> 引用 repo 中具体文件路径


## 6. 验收标准 (Acceptance Criteria)

每个端点可追溯到 FR;OpenAPI/proto 完整且 lint 通过。

## 7. 关联文档 (References)

- 上游: 25 機能設計
- 下游: 46 API 詳細設計
- 略称: API

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
