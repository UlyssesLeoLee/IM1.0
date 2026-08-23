---
doc_id: 84
title_ja: 障害試験結果
title_zh: 故障注入测试
phase: 08-system-test
activity_no: 84
owners: SRE + Tech Lead
status: Draft
version: 1.0.0
---

# 84. 障害試験結果 / 故障注入测试

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.84 (システム試験 / System Test)
> 责任方: SRE + Tech Lead

## 1. 目的 (Purpose)

注入故障验证:检测、告警、隔离、恢复。

## 2. 适用范围 (Scope)

Chaos engineering 实践。

## 3. 责任方 (Owners)

SRE + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- アーキテクチャ
- 監視設計

## 5. 输出 / 模板正文 (Body)

### 1. 注入场景

| 编号 | 场景 | 注入方式 | 预期 |
|---|---|---|---|
| FI-001 | 杀进程 | kill -9 | 监控告警 + 自愈 |
| FI-002 | 网络分区 | toxiproxy | 熔断 |
| FI-003 | DB 主从切换 | | 30s 内恢复 |
| FI-004 | 依赖变慢 | | 降级 |

### 2. 观察

| 编号 | 实际行为 | 是否符合 |
|---|---|---|
| | | |

### 3. 结论

- [ ] 故障行为符合设计


## 6. 验收标准 (Acceptance Criteria)

注入场景有清单;观察对照设计;结论明确。

## 7. 关联文档 (References)

- 上游: 83 Security
- 下游: 85 Recovery
- 略称: —

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
