---
doc_id: 53
title_ja: 開発環境構築手順書
title_zh: 开发环境搭建
phase: 05-implementation
activity_no: 53
owners: DevOps + Tech Lead
status: Draft
version: 1.0.0
---

# 53. 開発環境構築手順書 / 开发环境搭建

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.53 (実装 / Implementation)
> 责任方: DevOps + Tech Lead

## 1. 目的 (Purpose)

定义开发者本地 / CI 环境的搭建步骤、依赖、常见问题。

## 2. 适用范围 (Scope)

新成员能依此 1 天内完成环境搭建。

## 3. 责任方 (Owners)

DevOps + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- Infra 基本設計
- CI/CD 構成

## 5. 输出 / 模板正文 (Body)

### 1. 前置依赖

| 工具 | 版本 | 安装方式 |
|---|---|---|
| Rust | 1.7x+ | rustup |
| Node | 20+ | nvm |
| Docker | 24+ | 官方包 |
| k3d | v5+ | 官方包 |

### 2. 仓库与权限

### 3. 本地启动

```bash
# 步骤
```

### 4. CI 环境

- GitHub Actions / GitLab CI 配置位置
- 必需 secrets

### 5. 常见问题(FAQ)


## 6. 验收标准 (Acceptance Criteria)

新成员 1 天内能跑通;常见问题有记录。

## 7. 关联文档 (References)

- 上游: 52 DD Review
- 下游: 54 コーディング
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
