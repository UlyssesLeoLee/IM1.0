---
doc_id: 36
title_ja: ネットワーク基本設計書
title_zh: 网络基本设计
phase: 03-basic-design
activity_no: 36
owners: Network / SRE
status: Draft
version: 1.0.0
---

# 36. ネットワーク基本設計書 / 网络基本设计

> 项目: **IM1.0** — 面向游戏与 AI 场景的可嵌入式实时通信平台 (Rust / Next.js / K3s)
> 工程: No.36 (基本設計 / Basic Design)
> 责任方: Network / SRE

## 1. 目的 (Purpose)

设计网络分段、Firewall、负载均衡、DNS。

## 2. 适用范围 (Scope)

为安全评审与运维提供输入。

## 3. 责任方 (Owners)

Network / SRE

## 4. 前置依赖 (Prerequisites / Inputs)

- Infra 基本
- セキュリティ設計

## 5. 输出 / 模板正文 (Body)

### 1. 网络分段

| 段 | 用途 | CIDR | 出口策略 |
|---|---|---|---|
| public | LB / WAF | | |
| app | 服务 | | |
| data | DB | 禁止 Internet | |
| mgmt | 运维 | 受限访问 | |

### 2. 负载均衡

- L4 vs L7
- 会话保持
- 健康检查

### 3. Firewall / WAF 规则

| 编号 | 源 | 目标 | 端口 | 备注 |
|---|---|---|---|---|
| | | | | |

### 4. DNS / 证书

- 证书申请 / 续期自动化


## 6. 验收标准 (Acceptance Criteria)

分段清楚;Firewall 规则有表;证书自动续期。

## 7. 关联文档 (References)

- 上游: 35 インフラ基本
- 下游: 104 本番環境構築
- 略称: NW

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
