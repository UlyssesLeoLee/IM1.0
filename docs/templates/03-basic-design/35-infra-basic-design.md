---
doc_id: 35
title_ja: インフラ基本設計書
title_zh: 基础设施基本设计
phase: 03-basic-design
activity_no: 35
owners: SRE / Infra リード
status: Draft
version: 1.0.0
---

# 35. インフラ基本設計書 / 基础设施基本设计

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.35 (基本設計 / Basic Design)
> 责任方: SRE / Infra リード

## 1. 目的 (Purpose)

设计服务器 / 容器 / 编排 / 镜像仓库 / 制品管理。

## 2. 适用范围 (Scope)

为 IaC 与运维 Runbook 提供输入。

## 3. 责任方 (Owners)

SRE / Infra リード

## 4. 前置依赖 (Prerequisites / Inputs)

- SA / アーキテクチャ
- NFR

## 5. 输出 / 模板正文 (Body)

### 1. 服务器 / 节点

| 角色 | 数量(最小) | 数量(峰值) | 规格 |
|---|---|---|---|
| API | | | |
| Worker | | | |
| DB | | | |

### 2. 容器 / 编排

- K3s / Kubernetes
- 命名空间划分

### 3. 镜像仓库 / 制品

- Harbor / GHCR
- 镜像标签策略
- SBOM 制品

### 4. IaC 工具选型

- Terraform / Pulumi / Helm
- 目录结构约定

### 5. 环境

| 环境 | 用途 | 数据 |
|---|---|---|
| dev | 开发者本地 | 合成 |
| staging | 联调 / ST | 脱敏副本 |
| prod | 生产 | 真实 |


## 6. 验收标准 (Acceptance Criteria)

节点数 / 规格明确;环境分层清楚;IaC 工具有选型。

## 7. 关联文档 (References)

- 上游: 22 システム方式
- 下游: 104 本番環境構築
- 略称: Infra

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
