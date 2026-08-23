---
doc_id: 58
title_ja: CI Pipeline 構成
title_zh: CI 流水线配置
phase: 05-implementation
activity_no: 58
owners: DevOps
status: Draft
version: 1.0.0
---

# 58. CI Pipeline 構成 / CI 流水线配置

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.58 (実装 / Implementation)
> 责任方: DevOps

## 1. 目的 (Purpose)

定义 CI 流水线的阶段、门禁、产物。

## 2. 适用范围 (Scope)

流水线是后续 GATE 判定的来源。

## 3. 责任方 (Owners)

DevOps

## 4. 前置依赖 (Prerequisites / Inputs)

- ビルド成果物管理
- SAST 設定

## 5. 输出 / 模板正文 (Body)

### 1. 流水线阶段

```
lint → test(unit) → sast → build → test(integration) → sign → push
```

### 2. 门禁(Gate)

| 阶段 | 通过条件 | 不通过行为 |
|---|---|---|
| lint | 无 error | 阻断 |
| unit | 全部通过 | 阻断 |
| sast | Critical = 0 | 阻断 |
| build | 成功 | 阻断 |
| integration | 全部通过 | 警告 / 阻断(主分支) |

### 3. 缓存 / 优化

### 4. 通知

- Slack / 飞书


## 6. 验收标准 (Acceptance Criteria)

阶段 / 门禁 / 优化 / 通知齐全;门禁可配。

## 7. 关联文档 (References)

- 上游: 57 ビルド
- 下游: 62 単体試験実施
- 略称: CI

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
