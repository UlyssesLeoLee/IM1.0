---
doc_id: 56
title_ja: コードレビュー記録
title_zh: 代码评审记录
phase: 05-implementation
activity_no: 56
owners: レビュアー(全開発者)
status: Draft
version: 1.0.0
---

# 56. コードレビュー記録 / 代码评审记录

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.56 (実装 / Implementation)
> 责任方: レビュアー(全開発者)

## 1. 目的 (Purpose)

记录 PR 评审的问题、决议、合并。

## 2. 适用范围 (Scope)

每个 PR 至少有 1 名 owner + 1 名 cross-reviewer。

## 3. 责任方 (Owners)

レビュアー(全開発者)

## 4. 前置依赖 (Prerequisites / Inputs)

- PR + コメント

## 5. 输出 / 模板正文 (Body)

### PR 基本信息

- PR 编号 / 标题 / 作者
- 关联 ticket
- 关联 Phase / Activity
- 变更行数 +/-

### 评审要点

- [ ] 功能正确性
- [ ] 测试覆盖
- [ ] 错误处理
- [ ] 安全 / 性能
- [ ] 风格 / 命名
- [ ] 文档更新

### 评审意见

| 编号 | 严重度 | 意见 | 状态 |
|---|---|---|---|
| | | | Open / Closed |

### 合并条件

- [ ] CI Green
- [ ] SAST Critical = 0
- [ ] 评审者 × 2 同意


## 6. 验收标准 (Acceptance Criteria)

每个 PR 有评审意见表;有合并条件;无 Critical 遗留。

## 7. 关联文档 (References)

- 上游: 54 コーディング, 55 SAST
- 下游: 57 ビルド
- 略称: CR

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
