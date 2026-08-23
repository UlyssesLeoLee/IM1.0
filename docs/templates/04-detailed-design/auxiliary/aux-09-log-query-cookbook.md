---
doc_id: aux-09
title_ja: ログクエリ Cookbook
title_zh: 日志查询 Cookbook
phase: 04-detailed-design-aux
owners: SRE
status: Draft
version: 1.0.0
related_activities: 51 日志设计, 110 监控, 84 故障注入, 114 事件报告
---

# aux-09. ログクエリ Cookbook / 日志查询 Cookbook

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助)
> 责任方: SRE

## 1. 目的 (Purpose)

沉淀常见故障 / 性能问题的日志查询模式,新人 5 分钟内上手排障。

## 2. 适用范围 (Scope)

生产 + staging 环境(查询语法基于 Loki / ELK / 平台特定)。

## 3. 责任方 (Owners)

SRE

## 4. 前置依赖 (Prerequisites / Inputs)

- 51 日志设计
- 38 监控设计

## 5. 输出 / 模板正文 (Body)

## A. 通用查询语法(Loki / LogQL 为例)

```
{service="im-core"} |= "error" | json | latency_ms > 100
{service="im-gateway"} |~ "AUTH_TOKEN_(EXPIRED|INVALID)"
sum by (status) (rate({service="im-gateway"} |= "publish" [5m]))
```

## B. 常见场景查询

### B.1 找最近的错误

```
{service="im-core"} |= "ERROR" | json | line_format "{{.msg}}"
```

### B.2 追踪单个用户

```
{service=~"im-.*"} | json | user_id="12345" | line_format "{{.ts}} {{.service}} {{.msg}}"
```

### B.3 慢请求排查

```
{service="im-gateway"} |= "publish" | json | latency_ms > 200
```

### B.4 限流命中

```
{service=~".*"} |= "rate_limited" | json
| sum by (user_id) (rate({...}[1m])) > 10
```

### B.5 错误码 Top 10

```
sum by (error_code) (
  count_over_time({service=~"im-.*"} |= "ERROR" [1h])
)
```

### B.6 跨服务链路

```
{trace_id="abc123"}
```

## C. 故障模式 → 查询

| 现象 | 第一步查询 | 进一步 |
|---|---|---|
| 客户端报错 5xx | `error_code=5*` | 看具体 `error_code` |
| 延迟飙高 | `latency_ms > 200` by service | 定位慢服务 |
| 消息丢失 | 查 `message_id` 全链路 | 验证是否发出 / 是否 ack |
| 登录失败 | `AUTH_TOKEN_*` | 看是过期还是无效 |
| 语音加入失败 | `VOICE_JOIN_FAILED` | 区分 SFU / 客户端 |
| 资源告警 | 系统日志 | 看 CPU / 内存 / 连接 |

## D. 关键字段速查

| 字段 | 含义 | 出现处 |
|---|---|---|
| `trace_id` | 链路追踪 ID | 所有服务 |
| `user_id` | 用户 ID | 业务日志 |
| `room_id` | 房间 ID | 业务日志 |
| `error_code` | 错误码 | 错误日志 |
| `latency_ms` | 耗时 | 慢日志 |
| `msg_id` | 消息 ID | 消息日志 |

## E. 性能问题排查模板

1. **定位时间点**:用户反馈 / 监控告警 → 找时间窗口
2. **全局图**:错误率 / 延迟 / QPS 变化曲线
3. **找异常服务**:`by (service)` 看哪个先降级
4. **深挖**:慢查询 / 错误日志 / DB 锁
5. **定位根因**:配置变更 / 流量尖峰 / 依赖故障
6. **验证恢复**:对照修复前后曲线

## F. 必备仪表盘

- **业务总览**:QPS / 错误率 / 在线用户
- **服务总览**:每服务 P50 / P99 / 错误码 Top
- **数据库**:QPS / 锁 / 慢查询
- **依赖**:外部 API 状态

## G. 排障禁忌

- ❌ 在生产环境 `cat` 大文件
- ❌ 长时间开 debug 日志
- ❌ 不带过滤的全量 grep
- ❌ 直接删日志 / 重启服务(先评估)


## 6. 验收标准 (Acceptance Criteria)

每个 SRE 能用本 Cookbook 5 分钟内找到常见问题的根因;查询模式每季度更新。

## 7. 关联文档 (References)

- 关联工程活动: 51 日志设计, 110 监控, 84 故障注入, 114 事件报告
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
