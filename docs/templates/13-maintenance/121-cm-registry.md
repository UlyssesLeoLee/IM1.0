---
doc_id: 121
title_ja: 構成管理台帳
title_zh: 配置管理台账
phase: 13-maintenance
activity_no: 121
owners: DevOps + Tech Lead
status: Draft
version: 1.0.0
---

# 121. 構成管理台帳 / 配置管理台账

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.121 (保守 / Maintenance)
> 责任方: DevOps + Tech Lead

## 1. 目的 (Purpose)

记录生产环境的实际配置项:版本、镜像、配置。

## 2. 适用范围 (Scope)

可回答"现在生产跑的是什么"。

## 3. 责任方 (Owners)

DevOps + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- CMDB
- CI 产物

## 5. 输出 / 模板正文 (Body)

| 服务 | 版本 / commit | 镜像 tag | 配置 | 部署环境 | 部署时间 |
|---|---|---|---|---|---|


## 6. 验收标准 (Acceptance Criteria)

每行有:服务 / 版本 / 镜像 / 配置 / 部署信息。

## 7. 关联文档 (References)

- 上游: 120 変更承認
- 下游: 122 Patch
- 略称: CM

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
