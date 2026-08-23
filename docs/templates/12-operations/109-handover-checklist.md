---
doc_id: 109
title_ja: 運用引継ぎチェックリスト
title_zh: 运维交接清单
phase: 12-operations
activity_no: 109
owners: PM + SRE Lead
status: Draft
version: 1.0.0
---

# 109. 運用引継ぎチェックリスト / 运维交接清单

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.109 (運用 / Operations)
> 责任方: PM + SRE Lead

## 1. 目的 (Purpose)

开发向运维正式交接的逐项确认。

## 2. 适用范围 (Scope)

所有项必须勾选,否则不算完成。

## 3. 责任方 (Owners)

PM + SRE Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 運用設計
- Runbook
- Hypercare レポート

## 5. 输出 / 模板正文 (Body)

### 1. 文档 / Runbook

- [ ] 運用設計書
- [ ] 監視設計書
- [ ] バックアップ設計書
- [ ] 障害対応 Runbook
- [ ] インシデント対応 Runbook
- [ ] 変更手順

### 2. 工具 / 权限

- [ ] 监控 / 告警账号
- [ ] IaC 仓库权限
- [ ] 制品仓库权限
- [ ] 值班系统

### 3. 培训

- [ ] 业务背景
- [ ] 架构讲解
- [ ] 故障演练

### 4. 签字

| 角色 | 签字 | 日期 |
|---|---|---|
| 开发 Lead | | |
| 运维 Lead | | |


## 6. 验收标准 (Acceptance Criteria)

文档 / 权限 / 培训 / 签字齐全。

## 7. 关联文档 (References)

- 上游: 108 Hypercare
- 下游: 110 監視
- 略称: Handover

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
