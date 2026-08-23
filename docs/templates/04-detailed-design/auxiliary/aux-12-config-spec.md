---
doc_id: aux-12
title_ja: 構成仕様書
title_zh: 配置项规格
phase: 04-detailed-design-aux
owners: SRE + Tech Lead
status: Draft
version: 1.0.0
related_activities: 35 Infra 基本, 53 开发环境, 104 本番环境
---

# aux-12. 構成仕様書 / 配置项规格

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助)
> 责任方: SRE + Tech Lead

## 1. 目的 (Purpose)

为所有运行期配置建立统一规格,涵盖分类 / 加载 / 热更新 / 密钥 / 环境差异。

## 2. 适用范围 (Scope)

所有服务 + 客户端(可配置部分)。

## 3. 责任方 (Owners)

SRE + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 35 Infra 基本
- 104 本番環境
- 52 DD Review

## 5. 输出 / 模板正文 (Body)

## A. 配置分类

| 类别 | 来源 | 加载时机 | 可热更新 | 示例 |
|---|---|---|---|---|
| 构建期 | 代码 / Cargo.toml | 编译 | ❌ | `version` |
| 环境变量 | 容器 env | 启动 | ❌ | `DATABASE_URL` |
| 配置中心 | Consul / Nacos | 启动 + 定期拉取 | ✅ | `rate_limit` |
| 密钥管理 | Vault / KMS | 启动 | ✅(需重连) | `db_password` |
| 特性开关 | 配置中心 | 启动 + 实时 | ✅ | `feature.voice.enabled` |

## B. 命名规范

- snake_case + 点分级:`im.gateway.port`, `im.gateway.ws.timeout_ms`
- 单位后缀:`_ms`(毫秒), `_s`(秒), `_bytes`(字节)
- 范围后缀:`_min`, `_max`, `_default`

## C. 配置项清单(模板)

### 服务: `im-gateway`

| 名称 | 类型 | 默认 | 范围 | 必填 | 类别 | 说明 |
|---|---|---|---|---|---|---|
| `im.gateway.http.port` | int | 8080 | 1024-65535 | Y | env | HTTP 端口 |
| `im.gateway.ws.port` | int | 8081 | 1024-65535 | Y | env | WebSocket 端口 |
| `im.gateway.ws.max_conn` | int | 100000 | 1-1000000 | Y | center | 最大连接数 |
| `im.gateway.ws.timeout_ms` | int | 30000 | 1000-300000 | N | center | 心跳超时 |
| `im.gateway.rate_limit.publish_per_min` | int | 60 | 1-1000 | Y | center | 发消息限流 |
| `im.gateway.feature.voice_v2` | bool | false | — | N | center | 语音 v2 灰度 |
| `im.gateway.db.url` | string | — | — | Y | vault | DB URL |
| `im.gateway.db.password` | string | — | — | Y | vault | DB 密码 |

### 服务: `im-router`

(同样结构)

## D. 环境差异

| 配置 | dev | staging | prod |
|---|---|---|---|
| `*.log_level` | debug | info | warn |
| `*.rate_limit.*` | 高(宽松) | 中 | 低(严格) |
| `*.feature.*` | 全开 | 部分 | 默认 |
| `*.db.*` | 本地 | 脱敏副本 | 生产 |
| `*.external.*` | mock | 沙箱 | 真实 |

## E. 加载顺序

1. 构建期配置(代码常量)
2. 环境变量
3. 配置中心 / 密钥管理
4. 命令行参数(最高优先级)

## F. 热更新策略

- **可热更**:限流阈值 / 特性开关 / 业务参数
- **不可热更**:端口 / 数据库地址 / 密钥
- **重载方式**:SIGHUP / 定期 poll(30s) / 主动推送
- **一致性**:多实例下,使用 pub/sub 广播

## G. 密钥管理

- 所有 `*_password`, `*_secret`, `*_key` 必须走 Vault / KMS
- 不入配置中心;不写日志;不输出到前端
- 轮换:DB 密码季度;API 密钥月度
- 紧急轮换:有"立即作废"通道

## H. 校验

- 启动时校验所有必填 + 范围
- 不合法直接退出,不静默
- 配置项变更要走 PR + 评审
- 配置模板化:`config.example.yaml` 提交仓库,真实值不入库

## I. 监控

- 配置项总数 / 变更次数
- 热更新失败告警
- 密钥轮换到期提醒
- 未注册配置项使用告警(避免拼写错误)


## 6. 验收标准 (Acceptance Criteria)

所有服务有完整配置清单;密钥 100% 走 Vault;热更与启动加载分离;变更可审计。

## 7. 关联文档 (References)

- 关联工程活动: 35 Infra 基本, 53 开发环境, 104 本番环境
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
