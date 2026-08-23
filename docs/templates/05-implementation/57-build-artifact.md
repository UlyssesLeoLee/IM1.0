---
doc_id: 57
title_ja: ビルド成果物管理台帳
title_zh: 构建产物管理台账
phase: 05-implementation
activity_no: 57
owners: DevOps
status: Draft
version: 1.0.0
---

# 57. ビルド成果物管理台帳 / 构建产物管理台账

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.57 (実装 / Implementation)
> 责任方: DevOps

## 1. 目的 (Purpose)

记录每个 release 的构建产物:版本、镜像、SBOM、签名。

## 2. 适用范围 (Scope)

可追溯:任何在产环境跑的镜像都能回到 commit。

## 3. 责任方 (Owners)

DevOps

## 4. 前置依赖 (Prerequisites / Inputs)

- CI / ビルド pipeline

## 5. 输出 / 模板正文 (Body)

| Release | Commit | 镜像 tag | SBOM | 签名 | 部署环境 | 部署时间 |
|---|---|---|---|---|---|---|
| v1.0.0 | abc123 | im-core:1.0.0 | sbom.spdx.json | cosign | staging / prod | |


## 6. 验收标准 (Acceptance Criteria)

每行可追溯到 commit;SBOM / 签名齐全。

## 7. 关联文档 (References)

- 上游: 56 CR
- 下游: 58 CI
- 略称: Build

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
