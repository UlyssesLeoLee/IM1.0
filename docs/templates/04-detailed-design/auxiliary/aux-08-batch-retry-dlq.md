---
doc_id: aux-08
title_ja: バッチリトライ & デッドレター
title_zh: 批处理重试 / 死信队列策略
phase: 04-detailed-design-aux
owners: SRE + Tech Lead
status: Draft
version: 1.0.0
related_activities: 49 批处理详细, 50 错误处理, 32 批处理设计
---

# aux-08. バッチリトライ & デッドレター / 批处理重试 / 死信队列策略

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助)
> 责任方: SRE + Tech Lead

## 1. 目的 (Purpose)

为批处理作业统一重试 / 死信 / 幂等策略,避免无限重试和沉默失败。

## 2. 适用范围 (Scope)

全部定时 / 事件触发 / 手动批处理作业。

## 3. 责任方 (Owners)

SRE + Tech Lead

## 4. 前置依赖 (Prerequisites / Inputs)

- 49 批处理详细
- 50 错误处理

## 5. 输出 / 模板正文 (Body)

## A. 错误分类

| 类别 | 示例 | 重试? |
|---|---|---|
| 瞬时错误 | 网络超时 / DB 死锁 / 限流 | 是 |
| 持久错误 | 参数错误 / 业务规则违反 | 否,直接 DLQ |
| 资源错误 | 磁盘满 / OOM | 是(等资源恢复) |
| 外部错误 | 第三方 API 5xx | 是(退避) |
| 数据错误 | 解析失败 / schema 不匹配 | 否,DLQ + 告警 |

## B. 重试策略

### B.1 通用参数

| 参数 | 默认 | 备注 |
|---|---|---|
| 最大重试次数 | 3 | |
| 初始退避 | 1s | |
| 退避倍数 | 2 | 指数 |
| 最大退避 | 60s | 防止间隔过大 |
| 抖动 | ±20% | 防雪崩 |

### B.2 重试公式

```
delay = min(initial * base^attempt, max_delay) * (1 ± jitter)
attempt 1: 1s
attempt 2: 2s
attempt 3: 4s
...
```

## C. 死信队列 (DLQ)

### C.1 触发条件

- 重试耗尽仍失败
- 持久错误(不重试)
- 数据错误(数据进入 DLQ 等待人工)

### C.2 死信结构

```
{
  "id": "uuid",
  "original_task": "send_email",
  "payload": {...},
  "error": {
    "code": "EXT_SMTP_TIMEOUT",
    "message": "...",
    "stack": "..."
  },
  "failed_at": "2026-08-20T...",
  "attempts": 3
}
```

### C.3 处理流程

1. DLQ 入库 / 持久化
2. 告警(企业微信 / 邮件)
3. 运维 / 业务侧人工处置
4. 重跑 / 跳过 / 永久丢弃(必须留记录)
5. 复盘:同类型 1 周内 > N 次 → 自动 Jira

## D. 幂等保证

### D.1 必备属性

- 同一作业**重复执行结果一致**
- 同一作业**部分执行可恢复**

### D.2 实现

- **业务幂等**:用业务键(如 `message_id`)做唯一约束
- **状态机**:每个步骤记录状态,失败从断点继续
- **去重表**:`processed_tasks(id, processed_at)`,执行前先 INSERT IGNORE
- **乐观锁**:version 字段
- **输出幂等**:写操作 upsert 而非 insert

## E. 监控 / 告警

| 指标 | 阈值 | 告警 |
|---|---|---|
| 失败率 | > 5% | 通知 SRE |
| DLQ 累积 | > 100 条 | 通知业务 |
| 重试次数 > 2 | 每次 | 记录 |
| 执行超时 | 超阈值 | 通知 |

## F. 重跑步骤模板(每作业)

```bash
# 1. 检查作业状态
./jobctl status <job_name>

# 2. (可选)重置检查点
./jobctl reset <job_name> --from <step>

# 3. 重跑
./jobctl run <job_name> [--from <step>] [--dry-run]

# 4. 验证
./jobctl verify <job_name>
```

## G. 案例

### JOB-001 用户日报生成

- 失败:DB 临时不可用 → 瞬时错误
- 行为:重试 3 次,指数退避
- 失败:进入 DLQ,告警业务
- 修复后:运维手动重跑,选 `--from step=2` 跳过已完成步骤


## 6. 验收标准 (Acceptance Criteria)

所有批处理有重试 + DLQ + 幂等保证;DLQ 24h 内有人工处置记录;失败率超阈值告警。

## 7. 关联文档 (References)

- 关联工程活动: 49 批处理详细, 50 错误处理, 32 批处理设计
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
