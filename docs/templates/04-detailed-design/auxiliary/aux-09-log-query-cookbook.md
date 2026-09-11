---
doc_id: aux-09
title_ja: ログクエリ Cookbook (IM1.0)
title_zh: 日志查询 Cookbook (IM1.0)
phase: 04-detailed-design-aux
owners: 架构师 (Mavis 接手 agent per DEC-008) + SRE
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 51 日志设计, 110 监控, 84 故障注入, 114 事件报告
---

# aux-09. ログクエリ Cookbook (IM1.0) / 日志查询 Cookbook (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: 架构师 + SRE
> 日志规范源: `docs/Observability.md` §8(JSON 格式 + 字段规范 + 等级策略 + 敏感字段 + 采样 + 保留)+ §9(Trace Context 传播)
> 必接字段源: `ImplementationSpec §9.4`(MVP 5 服务)
> 错误码: `aux-03 §B`(21 项)
> 协议帧: `aux-13 §1`(WS 12 类)+ §3(REST 26 端点)
> 后端选型: **Loki + Tempo + Prometheus + Grafana**(`Observability §5.3-§5.4` OBS-ARCH-003/004)

## 1. 目的 (Purpose)

为 IM1.0 沉淀常见故障 / 性能问题的日志查询模式,新人 5 分钟内上手排障。本 Cookbook 覆盖 5 个服务(`im-gateway` / `im-core` / `im-presence` / `im-media` / `extension-runtime`)的典型查询 + 故障模式映射 + 性能排障模板。

## 2. 适用范围 (Scope)

| 环境 | 工具 | 数据源 |
|---|---|---|
| **生产 (prod)** | Grafana Explore + LogQL | Loki(`im1-prod` ns) |
| **预发 (staging)** | Grafana Explore | Loki(`im1-staging` ns) |
| **开发 (dev)** | kubectl logs + Loki | Loki(`im1-dev` ns) + stdout |
| **本地 (laptop)** | `cargo run` + stdout (JSON) | tracing JSON 输出 |

> 协议层查询语法基于 **Loki / LogQL**(Observability §5.3 决策);K8s 原生 `kubectl logs` 补充。

## 3. 责任方 (Owners)

架构师(定义 + 更新)+ SRE(执行 + 维护 + 季度更新)。每个新故障模式 → 新增查询 + 复盘。

## 4. 前置依赖 (Prerequisites / Inputs)

- 日志规范(`Observability §8`)
- 必接字段(`ImplementationSpec §9.4`)
- 错误码(`aux-03 §B`)
- 协议帧(`aux-13 §1`)
- 性能基线(`aux-06 §B`)
- Loki / Tempo 部署(`Observability §4` 架构图)

## 5. 输出 / 模板正文 (Body)

## A. IM1.0 服务清单(5 个 + 共享组件)

| 服务 | Pod 名 pattern | 端口 | 关键路径 |
|---|---|---|---|
| `im-gateway` | `im-gateway-*` | 8080 (HTTP) / 9000 (gRPC) | REST API + WS 终结 + 限流 |
| `im-core` | `im-core-*` | 9000 (gRPC Server) | 业务逻辑 + DB 唯一写入方 |
| `im-presence` | `im-presence-*` | (V1+) | 在线状态(MVP 占位) |
| `im-media` | `im-media-*` | (V1+) | 媒体预签 + 处理(MVP 最小) |
| `extension-runtime` | `extension-runtime-*` | (V1+) | Extension 沙箱(MVP 占位) |
| `postgres` (共享) | `postgres-0` | 5432 | DB(由 im-core 写入) |
| `valkey` (共享) | `valkey-0` | 6379 | 限流 + 缓存(MVP) |
| `nats` (共享) | `nats-0` | 4222 | 事件总线(MVP 占位,D-3 真实) |

## B. 必接日志字段(IM1.0 强制,所有服务)

> 来自 `ImplementationSpec §9.4` + `Observability §8.2`;**绝不含** `password` / `token` / `secret`(Observability §8.4 OBS-LOG-002)

```json
{
  "timestamp": "2026-08-20T22:00:00.123Z",
  "level": "INFO",
  "service": "im-gateway",
  "version": "0.1.0",
  "environment": "dev",
  "instance": "im-gateway-7d8b9c-x7f2d",
  "trace_id": "abc123def456",
  "span_id": "789ghi",
  "request_id": "req-uuid",
  "user_id": "u_123",
  "env_id": "env_456",
  "module": "im_gateway::middleware",
  "event": "http_request",
  "error_code": "AUTH_TOKEN_EXPIRED",
  "message": "Token validation failed",
  "latency_ms": 45,
  "req_id": "uuid",
  "kind": "send_message",
  "message_id": "uuid",
  "conversation_id": "uuid"
}
```

## C. 通用查询语法(Loki LogQL)

### C.1 基础语法

```logql
# 选择服务
{service="im-gateway"}

# 多服务
{service=~"im-.*"}

# JSON 字段提取
{service="im-gateway"} | json

# 字符串匹配
{service="im-gateway"} |= "error"
{service="im-gateway"} != "healthz"
{service="im-gateway"} |~ "AUTH_TOKEN_(EXPIRED|INVALID)"

# 时间范围
{service="im-gateway"} [5m]  # 最近 5 分钟
{service="im-gateway"} [1h]

# 字段过滤
{service="im-gateway"} | json | error_code="UNAUTHORIZED"
{service="im-gateway"} | json | user_id="12345"
{service="im-gateway"} | json | latency_ms > 200
{service="im-gateway"} | json | level="ERROR"

# 聚合
sum by (status) (rate({service="im-gateway"} |= "publish" [5m]))
sum by (error_code) (count_over_time({service=~"im-.*"} |= "ERROR" [1h]))
```

### C.2 排障常用 pattern

| 场景 | LogQL |
|---|---|
| 找最近错误 | `{service="im-core"} \|= "ERROR" \| json \| line_format "{{.msg}}"` |
| 追踪单个 user | `{service=~"im-.*"} \| json \| user_id="12345" \| line_format "{{.ts}} {{.service}} {{.msg}}"` |
| 慢请求 | `{service="im-gateway"} \| json \| latency_ms > 200` |
| 限流命中 | `{service=~"im-.*"} \|= "rate_limited" \| json` |
| 错误码 Top 10 | `sum by (error_code) (count_over_time({service=~"im-.*"} \|= "ERROR" [1h]))` |
| 跨服务链路 | `{trace_id="abc123"}` |
| 单 message_id 全链路 | `{message_id="7c9e6679-7425-40de-944b-e07fc1f90ae7"}` |
| 撤回时间窗扫描 | `{service="im-core"} \|= "RECALL_WINDOW_EXPIRED" \| json` |
| Token 过期 | `{service=~"im-.*"} \|= "AUTH_TOKEN_EXPIRED" \| json` |
| Valkey 临时不可用 | `{service="im-gateway"} \|= "Valkey" \| json \| level="WARN"` |

## D. 故障模式 → 查询(IM1.0 实战)

### D.1 客户端报错 5xx

**现象**: 客户端 HTTP 5xx 错误 / WS `ack.ok=false` with 5xx 风格错误码

**第一步**:
```logql
# 找最近 5xx
{service=~"im-.*"} | json | level="ERROR" | line_format "{{.error_code}} {{.message}}"
```

**第二步**:
```logql
# 按错误码聚合
sum by (error_code) (count_over_time({service=~"im-.*"} | json | level="ERROR" [15m]))
```

**第三步**: 取具体 `error_code` 查 `aux-03 §B` 含义 + 客户端建议处理

| 错误码 | 含义 | 处理 |
|---|---|---|
| `INTERNAL_ERROR` | 服务内部异常 | 查 stack trace → 关联 commit |
| `SERVICE_UNAVAILABLE` | 依赖不可用 | 查依赖(见 D.10) |
| `MESSAGE_TOO_LARGE` | content 超 `IM_MESSAGE_MAX_SIZE_BYTES` | 客户端分片 |
| `RATE_LIMITED` | 限流命中 | 读 Retry-After |

### D.2 延迟飙高(P99 > 500ms)

**现象**: SLO 告警,端到端 P99 > 500ms

**第一步**: 全局图
```logql
# 各服务 P99 latency(by service)
quantile_over_time(0.99, {service=~"im-.*"} | json | unwrap latency_ms [5m])
```

**第二步**: 找异常服务
```logql
# 各服务错误率
sum by (service) (rate({service=~"im-.*"} | json | level="ERROR" [5m]))
/
sum by (service) (rate({service=~"im-.*"} [5m]))
```

**第三步**: 深挖慢服务
```logql
# 慢 SQL(由 im-core 输出)
{service="im-core"} | json | event="sql_query" | latency_ms > 100
```

**第四步**: 根因
```logql
# 找 DB 锁
{service="postgres"} |= "lock" | json

# 找配置变更
{service=~"im-.*"} | json | event="config_reload"
```

### D.3 消息丢失 / 重复

**现象**: 客户端反馈消息未收到 / 收到多次

**第一步**: 查 message_id 全链路
```logql
{message_id="7c9e6679-7425-40de-944b-e07fc1f90ae7"} | json | line_format "{{.ts}} {{.service}} {{.event}} {{.message}}"
```

**第二步**: 验证 5 步
| 步骤 | 服务 | 关键 event |
|---|---|---|
| 1. 发送 | im-gateway | `ws_message_received` / `send_message_start` |
| 2. 鉴权 | im-gateway | `auth_ok` |
| 3. 限流 | im-gateway | `rate_limit_check` |
| 4. 路由 | im-core | `send_message_start` / `idempotency_check` |
| 5. 持久化 | im-core | `message_inserted` / `sequence_allocated` |
| 6. 广播 | im-core | `nats_publish` (V1+) |
| 7. WS 推送 | im-gateway | `ws_pushed` / `ws_pushed_failed` |

**第三步**: 哪一步无 log → 该步骤失败

| 现象 | 可能根因 |
|---|---|
| 步骤 1 无 log | 客户端没发,或 WS 已断 |
| 步骤 2 失败 | token 过期/无效(`UNAUTHORIZED`) |
| 步骤 3 失败 | 限流命中(`RATE_LIMITED`) |
| 步骤 4 失败 | `MESSAGE_NOT_FOUND` / `FRIEND_REQUEST_EXISTS` / `FORBIDDEN` |
| 步骤 5 失败 | DB 写失败(`INTERNAL_ERROR`) |
| 步骤 6 失败 | NATS 不可用(D-3 真实实现后才有 log) |
| 步骤 7 失败 | im-gateway 推 WS 失败,接收方离线 |

### D.4 登录失败 / Token 过期

**现象**: 客户端报 401

**第一步**:
```logql
{service=~"im-.*"} |= "AUTH_TOKEN" | json | line_format "{{.ts}} {{.service}} {{.error_code}} {{.message}}"
```

**第二步**:
```logql
# 区分过期 vs 无效
sum by (error_code) (count_over_time({service=~"im-.*"} | json | error_code=~"AUTH_TOKEN_.*" [1h]))
```

| 错误码 | 含义 | 处理 |
|---|---|---|
| `UNAUTHORIZED` + message 含 `expired` | JWT 过期 | 客户端走 refresh |
| `UNAUTHORIZED` + message 含 `invalid` | 签名错 / kid 找不到 | 检查 `IM_JWT_SIGNING_KEYS` 轮换 |
| `UNAUTHORIZED` + message 含 `missing` | 缺 Authorization 头 | 客户端 bug |
| `RATE_LIMITED` | refresh 限流 | 退避重试 |

### D.5 限流命中

**现象**: 客户端 429 / WS `ack.ok=false` with `RATE_LIMITED`

**第一步**:
```logql
# 找触发限流的 user
{service=~"im-.*"} | json | error_code="RATE_LIMITED" | line_format "{{.user_id}} {{.op}}"
```

**第二步**:
```logql
# Top 限流 user
topk(10, sum by (user_id) (count_over_time({service=~"im-.*"} | json | error_code="RATE_LIMITED" [1h])))
```

**第三步**:
```logql
# 按 op 聚合
sum by (op) (count_over_time({service=~"im-.*"} | json | error_code="RATE_LIMITED" [1h]))
```

| op | 默认限流 | 来源 |
|---|---|---|
| `send_message` | 60/min/user | `ImplementationSpec §11.3` |
| `guest_register` | 10/hour/IP | `ImplementationSpec §11.3` |
| `token_exchange` | 1000/min/env | `ImplementationSpec §11.3` |
| `global` | 100 req/s/user | `IM_RATE_LIMIT_BUCKET_SIZE` |

### D.6 好友申请 / 拉黑相关

**现象**: 客户端报 4xx

**第一步**:
```logql
{service="im-core"} | json | event=~"friend_.*" | line_format "{{.ts}} {{.user_id}} {{.event}} {{.result}}"
```

| 错误码 | 含义 | 处理 |
|---|---|---|
| `FRIEND_REQUEST_EXISTS` | 已存在待处理申请 | UI 提示 |
| `FRIEND_REQUEST_NOT_FOUND` | 申请 ID 不存在 | UI 刷新 |
| `USER_BLOCKED` | 对方拉黑你 | UI 隐藏入口 |
| `INVALID_STATE_TRANSITION` | 状态机非法(如重复 accept) | UI 刷新本地状态 |
| `FORBIDDEN` | 非 recipient 试图 respond | 客户端 bug |

### D.7 WS 连接异常

**现象**: 客户端 WS 频繁断连

**第一步**:
```logql
# 找 WS close 原因
{service="im-gateway"} | json | event="ws_close" | line_format "{{.user_id}} {{.reason}} {{.duration_sec}}"
```

| close reason | 含义 | 处理 |
|---|---|---|
| `auth_timeout` | 100ms 内未发 auth 帧 | 客户端 SDK bug |
| `heartbeat_timeout` | 60s 无帧 | 网络 / 客户端 SDK |
| `force_disconnect` | 服务端主动断开 | 查 `force_disconnect` 事件 |
| `client_disconnect` | 客户端正常关闭 | 无 |
| `error` | 内部错误 | 查 stack trace |

**第二步**:
```logql
# 查 force_disconnect 原因
{service="im-gateway"} | json | event="force_disconnect" | line_format "{{.user_id}} {{.reason}}"
```

| reason | 含义 |
|---|---|
| `token_revoked` | Refresh Token 旋转后旧 device_session 撤销 |
| `account_banned` | `users.state='banned'` |
| `account_suspended` | `users.state='suspended'` |
| `admin_kick` | 管理员踢出(V1+) |

### D.8 撤回时间窗相关

**现象**: 客户端报 `RECALL_WINDOW_EXPIRED`

**第一步**:
```logql
{service="im-core"} | json | error_code="RECALL_WINDOW_EXPIRED" | line_format "{{.user_id}} {{.message_id}} {{.window_seconds}} {{.elapsed_seconds}}"
```

**第二步**: 查 `environments.settings.message.recall_window_seconds`
```logql
# 在 im-core settings 加载时
{service="im-core"} | json | event="env_settings_loaded" | line_format "{{.env_id}} {{.recall_window_seconds}}"
```

| env | 默认值 | 来源 |
|---|---|---|
| `dev` | 3600s (1h,宽松) | env 覆盖 |
| `staging` | 600s (10min) | env 覆盖 |
| `prod` | 120s (严格) | `IM_MESSAGE_RECALL_WINDOW_SECONDS` 默认 |

### D.9 DM 重复创建

**现象**: 客户端重复 `POST /v1/conversations` 应返同 conversation

**第一步**:
```logql
{service="im-core"} | json | event="dm_create" | line_format "{{.ts}} {{.user_a}} {{.user_b}} {{.result}} {{.conversation_id}}"
```

| result | 含义 |
|---|---|
| `created` | 首次创建 |
| `idempotent_hit` | 命中 dm_pairs PK,返原 conversation |
| `unique_violation` | 并发创建,99 次命中 1 次成功 |

### D.10 依赖故障(PG / Valkey / NATS)

**PG 不可达**:
```logql
{service="im-core"} | json | event="db_error" | line_format "{{.ts}} {{.sql_state}} {{.message}}"
```

| sql_state | 含义 |
|---|---|
| `57P01` | admin_shutdown |
| `57P02` | crash_shutdown |
| `57P03` | cannot_connect_now |
| `40001` | serialization_failure(乐观锁) |
| `40P01` | deadlock_detected |

**Valkey 不可达**:
```logql
{service="im-gateway"} | json | event="valkey_error" | line_format "{{.ts}} {{.op}} {{.error}}"
```

| 现象 | 处理 |
|---|---|
| 频繁 timeout | 限流降级(放行所有请求 + 告警) |
| OOM | 评估 LRU + `maxmemory-policy` |
| cluster 跨 slot | 改 key 命名空间 |

**NATS 不可达** (V1+ 真实实装后):
```logql
{service="im-core"} | json | event="nats_error" | line_format "{{.ts}} {{.subject}} {{.error}}"
```

## E. 关键字段速查(IM1.0 必接)

| 字段 | 含义 | 出现处 | 示例 |
|---|---|---|---|
| `trace_id` | 链路追踪 ID(Otel hex 32) | 所有服务 | `4bf92f3577b34da6a3ce929d0e0e4736` |
| `span_id` | 当前 span ID | 所有服务 | `00f067aa0ba902b7` |
| `user_id` | 用户 UUID | 业务日志 | `7c9e6679-7425-40de-944b-e07fc1f90ae7` |
| `env_id` | 环境 UUID | 业务日志 | UUID |
| `tenant_id` | 租户 UUID | im-core 业务 | UUID |
| `conversation_id` | 会话 UUID | 消息路径 | UUID |
| `message_id` | 消息 UUID | 消息路径 | UUID |
| `req_id` | WS 请求 ID(UUID v4) | WS 帧路径 | `33333333-3333-4333-8333-333333333333` |
| `error_code` | 错误码(aux-03 §B) | 错误日志 | `UNAUTHORIZED` |
| `latency_ms` | 耗时(ms) | 慢日志 | `123` |
| `kind` | 消息 kind / 操作 kind | 多处 | `text` / `send_message` |
| `event` | tracing event 名 | tracing | `http_request` / `ws_message_received` / `message_inserted` |
| `module` | rust module path | tracing | `im_gateway::ws::session` |

## F. 性能问题排查模板(7 步)

1. **定位时间点**: 用户反馈 / 监控告警 → 找时间窗口(`{...}[15m]` 缩小范围)
2. **全局图**: 错误率 / 延迟 / QPS 变化曲线
3. **找异常服务**: `sum by (service) (rate(...))` 看哪个先降级
4. **深挖**: 慢查询(`event="sql_query" latency_ms>100`)/ 错误日志 / DB 锁(`pg_locks`)
5. **定位根因**: 配置变更(`event="config_reload"`)/ 流量尖峰 / 依赖故障(§D.10)
6. **trace 串联**: 拿 `trace_id` 跨服务查 Tempo(`{trace_id="..."} | json`)
7. **验证恢复**: 对照修复前后曲线 / 触发回归测试

## G. 必备仪表盘(V1+ 实施,MVP 占位)

> MVP 阶段仅 Prometheus + Grafana 暴露,无 dashboard;V1+ 引入

| Dashboard | 关键指标 |
|---|---|
| **业务总览** | 在线连接数 / QPS / 错误率 / 注册用户数 |
| **服务总览** | 每服务 P50 / P99 / 错误码 Top |
| **数据库** | QPS / 锁 / 慢查询 / 连接数 / replication lag |
| **依赖** | 外部 API 状态 / S3 延迟 / NATS lag |
| **认证** | Token 签发 / 验证 / refresh 比例 |
| **限流** | 触发次数 / Top user / Top op |

## H. K8s 原生查询(`kubectl logs` 补充)

```bash
# 实时跟踪
kubectl logs -n im1-dev -l app=im-gateway -f --tail=100

# 上次启动
kubectl logs -n im1-dev -l app=im-core --previous --tail=200

# 多 pod 聚合
kubectl logs -n im1-prod -l app=im-gateway --tail=500 | jq -r 'select(.level=="ERROR") | .message'

# JSON 字段提取(需 jq)
kubectl logs -n im1-prod -l app=im-core --tail=1000 | jq 'select(.error_code=="RECALL_WINDOW_EXPIRED")'

# 时间范围
kubectl logs -n im1-prod -l app=im-gateway --since=15m --until=now
```

## I. 排障禁忌(避免常见错误)

- ❌ 在生产环境 `cat` 大文件(> 1GB) — 触发 OOM
- ❌ 长时间开 debug 日志(`IM_LOG_LEVEL=debug`) — 性能下降 5x + 磁盘爆炸
- ❌ 不带过滤的全量 `grep` / `rg` — 慢 + 内存压力
- ❌ 直接删日志 / 重启服务(先评估影响) — 丢失证据
- ❌ 在 `production` 环境 `psql` 直连改数据 — 绕过审计 + 阻塞其他会话
- ❌ 关闭 `tracing-subscriber::EnvFilter` 过滤 — 磁盘爆

## J. 告警阈值(SRE 应配置,Observability §OBS-ALT 跟进)

| 指标 | 阈值 | 告警等级 | 通知 |
|---|---|---|---|
| 服务 5xx 错误率 | > 1% (5m) | P2 | Slack #sre |
| 服务 5xx 错误率 | > 5% (1m) | P1 | on-call |
| API P99 延迟 | > 500ms (5m) | P2 | Slack #sre |
| API P99 延迟 | > 1s (1m) | P1 | on-call |
| WS 连接数 | 单 pod > 8 万 | P3 | Slack #sre |
| WS reconnect rate | > 10% (5m) | P2 | Slack #sre |
| PG 慢查询数 | > 10/min | P3 | Slack #sre |
| PG lock wait | > 30s | P1 | on-call |
| Valkey 内存 | > 80% | P3 | Slack #sre |
| NATS DLQ 累积 | > 100 条 | P2 | Slack #sre |
| 审计失败 | > 5/min | P1 | on-call |

## K. 验收标准 (Acceptance Criteria)

- [ ] §B 14 个必接字段完整定义(对应 `ImplementationSpec §9.4`)
- [ ] §C LogQL 基础 + 10 个常用 pattern
- [ ] §D 10 个故障模式 + 排查步骤(5xx / 延迟 / 消息丢失 / Token / 限流 / 好友 / WS / 撤回 / DM / 依赖)
- [ ] §E 13 个关键字段速查
- [ ] §F 7 步性能排查模板
- [ ] §G 6 个必备 dashboard 占位
- [ ] §H kubectl logs 6 个常用命令
- [ ] §I 6 项排障禁忌
- [ ] §J 11 项告警阈值 + 等级
- [ ] 引用 `aux-03` 错误码 21 项中至少 10 个(`UNAUTHORIZED` / `RATE_LIMITED` / `MESSAGE_NOT_FOUND` / `RECALL_WINDOW_EXPIRED` / `FRIEND_REQUEST_EXISTS` / `FRIEND_REQUEST_NOT_FOUND` / `USER_BLOCKED` / `INVALID_STATE_TRANSITION` / `FORBIDDEN` / `MESSAGE_TOO_LARGE` / `INTERNAL_ERROR`)
- [ ] 引用 `aux-13` WS 帧(`auth` / `send_message` / `force_disconnect` 等)

## L. 关联文档 (References)

- 日志规范: `docs/Observability.md` §8(完整规范)+ §9(Trace Context)+ §5.3(Loki 选型)
- 必接字段: `ImplementationSpec §9.4` 5 个 MVP 必接字段
- 错误码: `aux-03-error-code-registry.md` §B(21 项)
- 协议帧: `aux-13-protocol-frame-samples.md` §1(WS 12 类)
- 性能基线: `aux-06-algorithm-performance-model.md` §B(13 项关键算法)
- SQL 索引: `migrations/0001-0006` + `aux-07 §H`
- 状态机: `aux-04-state-machine-spec.md` §B
- CRC: `aux-05-crc-card.md` §5
- 限流: `ImplementationSpec §11.3` + `crates/im-gateway/ratelimit.rs`
- 命名规范: `aux-01-naming-convention.md` §G(业务术语)
- Loki / Tempo 部署: `Observability §4 总体架构` + §6(SDK)
- 关键表查询: `aux-07 §H` 35+ SQL 查询模式

## M. 已知缺口 (Known Gaps)

| 编号 | 缺口 | 影响 | 跟进 |
|---|---|---|---|
| GAP-1 | MVP 阶段无 dashboard,只暴露 Prometheus /metrics 端点 | 排障靠 log query,无图 | V1+ 引入 Grafana |
| GAP-2 | Loki / Tempo / Prometheus 部署清单(`deploy/k3s/dev/`)未在本表 | 实际查询时无 URL | WBS F-2 K3s dev 跑通后补 |
| GAP-3 | NATS 真实实装后的事件流查询(`im.message.*` / `im.user.banned`)V1+ 才生效 | 当前 45 行占位 | WBS D-3 token 300K-600K |
| GAP-4 | 性能回归 CI 阻断未实施 | 性能回退无自动发现 | V1+ CI 集成 |
| GAP-5 | 告警渠道(企业微信 / 邮件 / Slack)V1+ 集成 | 当前 P0/P1 靠人巡检 | V1+ Alertmanager + Slack |
| GAP-6 | `im-presence` / `im-media` / `extension-runtime` 服务占位,MVP 无实质 log | 故障模式 D.10 NATS 不可达在该服务不适用 | V1+ 实装后补 |
| GAP-7 | §D.10 依赖故障部分 PG sql_state 表仅列常见 5 项,IM1.0 可能遇到其他 | 排查不全 | V1+ 完整 sql_state 表 |
| GAP-8 | §G 仪表盘仅占位,无具体 PromQL / LogQL | Dashboard 实施依赖 V1+ | V1+ 编写 |
| GAP-9 | IM1.0 实际跑出 5xx 错误率 / P99 延迟数据未回填 | §D.1-D.10 故障模式全靠"理论预设" | POC-01 跑后回填 |
| GAP-10 | 错误码 21 项中,本表只引 11 个,10 个未引(`ACCOUNT_BANNED` / `ACCOUNT_SUSPENDED` / `IDEMPOTENCY_CONFLICT` 等) | 覆盖不全 | V1+ 补 |
| GAP-11 | 客户端 SDK 错误处理建议缺失 | 客户端不知道错误码如何分支 | SDK 集成文档 |
| GAP-12 | 日志采样 10% 后期实现,MVP 全量 | 高流量时磁盘可能爆 | 监控 + 动态调整 |
| GAP-13 | `OpenTelemetry SDK` 集成 Rust 代码待实装(`tracing-opentelemetry` 适配器) | 当前 `trace_id` 靠 `tracing` Span 上下文 | V1+ |

## N. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 填实 IM1.0 5 服务日志查询 Cookbook;§A 5 服务 + 3 共享组件;§B 14 必接字段(对应 ImplementationSpec §9.4);§C LogQL 10+ pattern;§D 10 故障模式(5xx/延迟/消息丢失/Token/限流/好友/WS/撤回/DM/依赖)+ 完整排查步骤 + 错误码 11 项;§E 13 关键字段速查;§F 7 步性能排查;§G 6 dashboard 占位;§H kubectl logs 6 命令;§I 6 排障禁忌;§J 11 告警阈值;§M 13 已知缺口( V1+ dashboard/NATS 真实/dashboard PromQL 实测回填等);引用 Observability §5-§9 + ImplementationSpec §9.4/§11.3 + aux-03/04/05/06/07/13 完整交叉引用 |
