---
doc_id: 137
title_ja: 構成管理台帳(管理)
title_zh: 配置管理(管理层)
phase: 15-management
activity_no: 137
owners: PMO + DevOps
status: Draft
version: 1.0.0
---

# 137. 構成管理台帳(管理) / 配置管理(管理层)

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.137 (管理 / Management)
> 责任方: PMO + DevOps

## 1. 目的 (Purpose)

管理层视角的配置台账,与 121 互补。

## 2. 适用范围 (Scope)

管理"哪些版本是被批准运行的"。

## 3. 责任方 (Owners)

PMO + DevOps

## 4. 前置依赖 (Prerequisites / Inputs)

- CI 产物
- Release 记录

## 5. 输出 / 模板正文 (Body)

| Release | 状态 | 批准日 | 部署环境 |
|---|---|---|---|
| v1.0.0 | Approved / Retired | | staging / prod |


## 6. 验收标准 (Acceptance Criteria)

每 Release:状态 / 批准日 / 环境。

## 7. 关联文档 (References)

- 上游: 136 変更管理
- 下游: 138 成果物
- 略称: CM

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
