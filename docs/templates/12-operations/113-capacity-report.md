---
doc_id: 113
title_ja: キャパシティレポート
title_zh: 容量报告
phase: 12-operations
activity_no: 113
owners: SRE
status: Draft
version: 1.0.0
---

# 113. キャパシティレポート / 容量报告

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.113 (運用 / Operations)
> 责任方: SRE

## 1. 目的 (Purpose)

月度 / 季度容量评审。

## 2. 适用范围 (Scope)

为扩容 / 缩容决策提供数据。

## 3. 责任方 (Owners)

SRE

## 4. 前置依赖 (Prerequisites / Inputs)

- 監視データ
- NFR(容量)

## 5. 输出 / 模板正文 (Body)

### 1. 当前用量

| 维度 | 用量 | 容量 | 使用率 |
|---|---|---|---|
| CPU | | | |
| 内存 | | | |
| 磁盘 | | | |
| DB 连接 | | | |
| 带宽 | | | |

### 2. 增长趋势

### 3. 预测与建议

- 何时达到瓶颈:
- 扩容建议:


## 6. 验收标准 (Acceptance Criteria)

当前 / 趋势 / 预测 / 建议齐全。

## 7. 关联文档 (References)

- 上游: 112 Backup
- 下游: 114 Incident
- 略称: Capacity

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
