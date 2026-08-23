---
doc_id: 54
title_ja: コーディング規約 + 実装ガイド
title_zh: 编码规范 + 实现指南
phase: 05-implementation
activity_no: 54
owners: Tech Lead
status: Draft
version: 1.0.0
---

# 54. コーディング規約 + 実装ガイド / 编码规范 + 实现指南

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.54 (実装 / Implementation)
> 责任方: Tech Lead

## 1. 目的 (Purpose)

统一定义编码风格、目录结构、命名、错误处理、提交信息。

## 2. 适用范围 (Scope)

新人依此写第一行代码。

## 3. 责任方 (Owners)

Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 言語 / フレームワーク best practice
- エラー処理方針
- ログ設計

## 5. 输出 / 模板正文 (Body)

### 1. 风格与静态检查

- rustfmt / clippy / eslint / prettier 配置
- 提交前必须通过:lint + 类型检查 + 单元测试

### 2. 命名

| 类型 | 风格 | 示例 |
|---|---|---|
| 变量 / 函数 | snake_case | |
| 类型 / Trait | PascalCase | |
| 常量 | SCREAMING_SNAKE_CASE | |

### 3. 错误处理

- 业务错误:Result / 自定义 error
- 不使用 unwrap / panic(在库代码中)

### 4. 提交信息

```
<type>(<scope>): <subject>

<body>

<footer>
```

### 5. 目录 / 模块

(引用 42)


## 6. 验收标准 (Acceptance Criteria)

风格 / 命名 / 错误 / 提交信息 / 目录 5 块齐全。

## 7. 关联文档 (References)

- 上游: 53 開発環境
- 下游: 55 静的解析
- 略称: PG

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
