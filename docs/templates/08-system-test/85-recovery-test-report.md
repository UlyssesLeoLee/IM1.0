---
doc_id: 85
title_ja: 復旧試験結果
title_zh: 恢复测试
phase: 08-system-test
activity_no: 85
owners: SRE + DBA
status: Draft
version: 1.0.0
---

# 85. 復旧試験結果 / 恢复测试

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.85 (システム試験 / System Test)
> 责任方: SRE + DBA

## 1. 目的 (Purpose)

验证灾备 / 跨区恢复。

## 2. 适用范围 (Scope)

RTO / RPO 目标必须达成。

## 3. 责任方 (Owners)

SRE + DBA

## 4. 前置依赖 (Prerequisites / Inputs)

- バックアップ設計
- 灾备方案

## 5. 输出 / 模板正文 (Body)

### 1. 场景

| 场景 | 范围 | 实际 RTO | 实际 RPO | 目标 |
|---|---|---|---|---|
| 单实例故障 | | | | |
| AZ 故障 | | | | |
| 区域故障 | | | | |

### 2. 步骤

1. 触发
2. 切换
3. 验证
4. 回切

### 3. 结论

- [ ] RTO / RPO 达成


## 6. 验收标准 (Acceptance Criteria)

场景 / 步骤 / 指标 / 结论齐;每项对照目标。

## 7. 关联文档 (References)

- 上游: 84 障害試験
- 下游: 86 B/R
- 略称: Recovery

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
