---
doc_id: 106
title_ja: 稼働確認 (Smoke Test) 結果
title_zh: 冒烟测试
phase: 11-release
activity_no: 106
owners: SRE + QA
status: Draft
version: 1.0.0
---

# 106. 稼働確認 (Smoke Test) 結果 / 冒烟测试

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.106 (リリース / Release)
> 责任方: SRE + QA

## 1. 目的 (Purpose)

部署后立即跑关键路径冒烟,确认服务正常。

## 2. 适用范围 (Scope)

5-10 分钟出结果;失败立即回退。

## 3. 责任方 (Owners)

SRE + QA

## 4. 前置依赖 (Prerequisites / Inputs)

- 本番デプロイ
- Smoke ケース

## 5. 输出 / 模板正文 (Body)

| 编号 | 场景 | 结果 |
|---|---|---|
| SMK-001 | 关键 API 探活 | ✅ / ❌ |
| SMK-002 | 登录 | |
| SMK-003 | 核心业务 | |


## 6. 验收标准 (Acceptance Criteria)

关键路径覆盖;失败立即回退。

## 7. 关联文档 (References)

- 上游: 105 Deploy
- 下游: 107 Go-Live
- 略称: Smoke Test

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
