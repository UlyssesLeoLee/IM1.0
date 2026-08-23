---
doc_id: 83
title_ja: セキュリティ試験結果
title_zh: 安全测试结果
phase: 08-system-test
activity_no: 83
owners: セキュリティ + 外部第三方
status: Draft
version: 1.0.0
---

# 83. セキュリティ試験結果 / 安全测试结果

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.83 (システム試験 / System Test)
> 责任方: セキュリティ + 外部第三方

## 1. 目的 (Purpose)

验证安全设计的有效性:渗透、依赖扫描、配置审计。

## 2. 适用范围 (Scope)

Critical / High 漏洞不得遗留。

## 3. 责任方 (Owners)

セキュリティ + 外部第三方

## 4. 前置依赖 (Prerequisites / Inputs)

- セキュリティ設計
- 依赖清单

## 5. 输出 / 模板正文 (Body)

### 1. 范围

- SAST(已并入 CI,见 55)
- DAST(动态扫描)
- 依赖扫描(SCA)
- 渗透测试(关键路径)
- 配置审计

### 2. 结果

| 类别 | Critical | High | Medium | Low |
|---|---|---|---|---|
| SAST | | | | |
| DAST | | | | |
| SCA | | | | |
| 渗透 | | | | |
| 配置 | | | | |

### 3. 处置

- Critical / High:不允许遗留
- Medium:限期
- Low:Backlog

### 4. 结论

- [ ] 无 Critical / High 残留


## 6. 验收标准 (Acceptance Criteria)

5 类安全测试齐;Critical/High 不得遗留。

## 7. 关联文档 (References)

- 上游: 82 Stress
- 下游: 84 障害試験
- 略称: Security

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
