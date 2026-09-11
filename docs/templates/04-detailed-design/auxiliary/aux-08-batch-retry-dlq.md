---
doc_id: aux-08
title_ja: バッチリトライ & デッドレター (IM1.0)
title_zh: 批处理重试 / 死信队列策略 (IM1.0)
phase: 04-detailed-design-aux
owners: 架构师 (Mavis 接手 agent per DEC-008) + SRE + Tech Lead
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 49 批处理详细, 50 错误处理, 32 批处理设计
---

# aux-08. バッチリトライ & デッドレター (IM1.0) / 批处理重试 / 死信队列策略 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: 架构师 + SRE + Tech Lead
> 批任务源: `docs/Day1-Task-List.md` 14 天任务 + `ImplementationSpec §10 测试不变量` + `migrations/0006 audit_logs` 审计事件
> 重试源: `ImplementationSpec §6.3 配置热重载` + `crates/im-gateway/ratelimit.rs` Valkey 限流
> DLQ 源: `aux-13 §1.3.5` WS 帧路由失败 / `NatsEventPublisher` (D-3 真实实现)

## 1. 目的 (Purpose)

为 IM1.0 批处理作业(在线 + 离线 + ETL)统一重试 / 死信 / 幂等策略,避免无限重试和沉默失败。MVP 阶段覆盖 4 类批任务(§A 表),V1+ 扩展;本表给出统一重试参数 + DLQ 协议 + 幂等保证。

## 2. 适用范围 (Scope)

| 类型 | 范围 | MVP 状态 |
|---|---|---|
| 定时批处理 | cron 触发(如每日归档) | V1+ (MVP 无 cron runner) |
| 事件触发批处理 | NATS 事件 → batch sink | V1+ (D-3 NATS 真实实现后) |
| 手动批处理 | 运维 `jobctl` CLI | V1+ |
| 同步在线批处理 | 单请求涉及 N 条写入(如撤回全员通知) | ✅ MVP(本表 §A.3) |
| ETL / 离线归档 | 媒体清理 / 审计归档 | V1+ (M-1 月 30 天后) |

## 3. 责任方 (Owners)

架构师(定义 + 评审)+ SRE(监控 + DLQ 处置 + 告警)+ Tech Lead(实现 + 单测)。任何批任务引入需 SRE 同意 DLQ 方案。

## 4. 前置依赖 (Prerequisites / Inputs)

- 错误码(`aux-03 §B`)
- 状态机(`aux-04 §B`)
- 配置项(`ImplementationSpec §6`)
- DB schema(`migrations/0006 audit_logs`)
- 事件总线(`ImplementationSpec §7.4.5 NatsEventPublisher`,MVP 占位 → V1+ 真实)

## 5. 输出 / 模板正文 (Body)

## A. IM1.0 批处理任务清单(4 类)

### A.1 定时批处理(预留 V1+)

> MVP 阶段无 cron runner;V1+ 引入 `im-cron` 进程,基于 NATS JetStream 定时触发。

| 任务 ID | 名称 | 频率 | 输入 | 输出 | 重试策略 | DLQ |
|---|---|---|---|---|---|---|
| **JOB-001** | 撤回时间窗扫描 | 每 1h | `messages WHERE state IN ('sent','delivered','read') AND created_at < now() - INTERVAL '2 hours'` | (无操作,仅监控) | 瞬时错误重试 3 次 | 写入 `audit_logs(action=batch.scan_error)` |
| **JOB-002** | 好友申请过期扫描 | 每 6h | `friend_requests WHERE state='pending' AND created_at < now() - INTERVAL '30 days'` | UPDATE state='expired' + 通知 | 持久错误直接 DLQ | 同上 |
| **JOB-003** | 媒体 30 天清理 | 每日 03:00 UTC | MinIO objects > 30 days | DELETE MinIO + 写 audit | 瞬时错误重试 5 次 | 通知 SRE,人工处置 |
| **JOB-004** | audit_logs 季度归档 | 每季度 | `audit_logs WHERE created_at < now() - INTERVAL '1 year'` | 导出 Parquet → 冷存储 | 持久错误直接 DLQ | 通知 SRE |

> **MVP 阶段不实装 JOB-001 ~ JOB-004**;本表为 V1+ 设计占位。

### A.2 事件触发批处理(V1+)

> 依赖 `NatsEventPublisher` (D-3) 真实实现, MVP 阶段 45 行占位代码。

| 任务 ID | 触发事件 | 处理 | 重试策略 | DLQ |
|---|---|---|---|---|
| **JOB-010** | `im.message.created` | (无,实时 WS push 已用) | — | — |
| **JOB-011** | `im.user.banned` | 强制断开该 user 所有 WS 连接 | 瞬时重试 3 次 | 写入 NATS DLQ subject `dlq.user.banned` |
| **JOB-012** | `im.friend.accepted` | (无,实时 push) | — | — |
| **JOB-013** | `im.token.revoked` | 强制断开该 device_session 的 WS | 瞬时重试 3 次 | DLQ + 告警 |

### A.3 同步在线批处理(MVP 必实装,3 个场景)

> 同步触发:用户单次请求触发 N 条 DB / 事件写,但**在用户感知时延内**完成(< 200ms)。

| 任务 ID | 场景 | 触发 | 关键操作 | 失败处理 |
|---|---|---|---|---|
| **JOB-100** | 撤回消息全员通知 | `POST /v1/conversations/{id}/messages/{msg_id}/recall` | UPDATE message.state + NATS publish `im.message.recalled` + 各在线成员 WS push | 事务回滚 + 返 `RECALL_WINDOW_EXPIRED` / `FORBIDDEN` |
| **JOB-101** | 接受好友申请 | `POST /v1/friends/requests/{id}/respond {accept: true}` | UPDATE friend_request.state + INSERT friendships + NATS publish `im.friend.accepted` | 事务回滚 + 返 `FRIEND_REQUEST_NOT_FOUND` / `INVALID_STATE_TRANSITION` |
| **JOB-102** | 拉黑 + 取消好友 | `POST /v1/friends/{id}/block` | INSERT/UPDATE friendships.state='blocked' + 若有 accepted 关系 → 自动转 blocked | 幂等(重复拉黑 204) |

### A.4 手动批处理(运维 CLI,V1+)

> 预留 `im-cli` 工具,提供 `jobctl` 子命令。M-1 月后需要时实装。

| 命令 | 说明 |
|---|---|
| `jobctl status <job_name>` | 查看作业状态 / 上次执行 / 失败次数 |
| `jobctl reset <job_name> --from <step>` | 重置检查点 |
| `jobctl run <job_name> [--from <step>] [--dry-run]` | 重跑(可选 dry-run) |
| `jobctl verify <job_name>` | 验证执行结果 |
| `jobctl dlq list [--since <ts>]` | 列 DLQ 消息 |
| `jobctl dlq replay <dlq_id>` | 重放 DLQ 单条 |

## B. 错误分类(IM1.0 实战场景)

| 类别 | 示例 | IM1.0 出现处 | 重试? |
|---|---|---|---|
| **瞬时错误** | 网络超时 / DB 死锁 / 限流 / Valkey 临时不可用 | A-002 Refresh 校验, A-003 sequence 行锁, A-009 限流 | **是**(指数退避) |
| **持久错误** | 参数错误 / 业务规则违反 / `RECALL_WINDOW_EXPIRED` / `FRIEND_REQUEST_EXISTS` | A-100 撤回超窗, A-101 重复申请 | **否**(直接返 4xx, 不进 DLQ) |
| **资源错误** | 磁盘满 / OOM | (V1+ 媒体上传) | **是**(等资源恢复) |
| **外部错误** | 第三方 API 5xx (S3 / SMS) | A-012 S3 预签 | **是**(退避) |
| **数据错误** | JSON 解析失败 / schema 不匹配 | (M-1 月 ETL 备份恢复时) | **否,DLQ + 告警** |
| **致命错误** | DB 不可达 / 配置文件 corrupt | 启动阶段 | **否**(进程退出,系统级告警) |

## C. 重试策略(IM1.0 统一参数)

### C.1 通用参数

| 参数 | 默认 | 备注 |
|---|---|---|
| 最大重试次数 | 3 | 瞬时错误;持久错误 0 |
| 初始退避 | 1s | |
| 退避倍数 | 2 | 指数 |
| 最大退避 | 60s | 防止间隔过大 |
| 抖动 | ±20% | 防雪崩 |
| 总超时 | 5 min | 含全部重试 |

### C.2 重试公式

```
delay(attempt) = min(initial * base^attempt, max_delay) * (1 ± jitter)

attempt 1: 1s ± 20%  → [0.8s, 1.2s]
attempt 2: 2s ± 20%  → [1.6s, 2.4s]
attempt 3: 4s ± 20%  → [3.2s, 4.8s]
attempt 4 (超限): → DLQ
```

### C.3 触发场景(IM1.0 具体应用)

| 场景 | 重试参数 | 退避 | DLQ 主题 |
|---|---|---|---|
| A-002 Refresh 校验 DB 临时失败 | 3 次 | 1s/2s/4s | (无,直接返 5xx 给客户端) |
| A-003 Sequence 行锁死锁 | 自动 retry(由 PG 检测) | 0ms(应用层无感) | (无,PG 内部处理) |
| A-009 Valkey 临时不可用 | 3 次 | 100ms/200ms/400ms | 降级:放行所有请求 + 告警 |
| A-012 S3 预签失败 | 3 次 | 1s/2s/4s | `dlq.media.presign` |
| JOB-001~004 定时任务失败 | 5 次(批任务可更长) | 5s/10s/20s/40s/60s | `dlq.batch.<job_id>` |
| JOB-011/013 NATS 事件推送失败 | 3 次 | 500ms/1s/2s | `dlq.event.<event_type>` |

## D. 死信队列 (DLQ) — IM1.0 协议

### D.1 触发条件

1. 重试耗尽仍失败(瞬时错误 3 次后)
2. 持久错误(不重试,直接 DLQ)
3. 数据错误(数据进入 DLQ 等待人工)

### D.2 死信结构(IM1.0 DLQ Record)

```json
{
  "dlq_id": "uuid",
  "original_task": "send_message",
  "original_payload": {
    "conversation_id": "uuid",
    "sender_id": "uuid",
    "idempotency_key": "uuid",
    "kind": "text",
    "content": { "text": "..." },
    "reply_to": null
  },
  "error": {
    "code": "EXT_S3_TIMEOUT",
    "message": "S3 presign timeout after 3 retries",
    "stack": "...(应用层 stack trace,脱敏后)...",
    "http_status": 503
  },
  "context": {
    "trace_id": "tr_01HXY...",
    "user_id": "uuid",
    "env_id": "uuid",
    "attempt_count": 3,
    "first_attempt_at": "2026-09-01T...",
    "last_attempt_at": "2026-09-01T..."
  },
  "failed_at": "2026-09-01T...",
  "dlq_destination": "NATS_subject_dlq.media.presign"
}
```

### D.3 存储策略

| 通道 | 用途 | 实现 |
|---|---|---|
| **NATS JetStream** | 实时事件 DLQ(微秒级) | `dlq.*` subject,consumer 持久化 7 天 |
| **PostgreSQL `dlq_records` 表** | 长留存(V1+ 加,V1 预留) | `action=dlq_record` 写入 `audit_logs.detail` JSONB |
| **S3 / MinIO** | 冷备份(月度归档) | V1+ ETL |

> **MVP 阶段**:仅 NATS DLQ subject + `audit_logs(action=dlq_record, detail=JSONB)`,V1+ 加 `dlq_records` 专表。

### D.4 处理流程

1. DLQ 入库 / NATS persist
2. 告警(企业微信 / 邮件 / Slack 集成 — V1+)
3. 运维 / 业务侧人工处置(`jobctl dlq list` → `jobctl dlq replay` 或丢弃)
4. 重跑 / 跳过 / 永久丢弃(必须留 audit 记录)
5. 复盘:同类型 1 周内 > N 次 → 自动建 P0/P1 issue

### D.5 告警阈值

| 指标 | 阈值 | 告警等级 | 通知 |
|---|---|---|---|
| DLQ 新增(1h) | > 10 条 | P3 | Slack #sre |
| DLQ 新增(1h) | > 50 条 | P2 | Slack #sre + 邮件 |
| DLQ 累积(未处置) | > 100 条 | P2 | 同上 |
| DLQ 单条未处置 | > 24h | P1 | 升级 on-call |
| DLQ 失败率(同任务) | > 5% / 1h | P2 | 同上 |

## E. 幂等保证(IM1.0 强制)

### E.1 必备属性

- 同一作业**重复执行结果一致**
- 同一作业**部分执行可恢复**

### E.2 IM1.0 幂等实现矩阵

| 场景 | 幂等策略 | 实现 |
|---|---|---|
| **发消息**(MVP 关键) | 业务键 `idempotency_key` | `UNIQUE NULLS NOT DISTINCT (conversation_id, sender_id, idempotency_key)`(`migrations/0005` 第 27 行) |
| **DM 创建** | 业务键 `(env, user_a, user_b)` | `dm_pairs` PK(`migrations/0004` 第 35 行) |
| **Refresh 旋转** | 旧 token 失效,新 token 唯一 | `device_sessions.refresh_token_hash` UNIQUE + UPDATE `revoked_at` |
| **拉黑** | 重复拉黑返 204(无副作用) | `friendships` PK + `state` UPDATE idempotent |
| **mark_read** | 幂等 UPDATE(单调 last_read_sequence) | `GREATEST(last_read_sequence, $1)`(避免回退) |
| **撤回** | 重复撤回返 `INVALID_STATE_TRANSITION` | `messages.state` 状态机 |
| **添加 reaction** | 重复 emoji 返幂等 | `message_reactions` PK `(message_id, user_id, emoji)` |
| **批任务(JOB-001 等)** | 去重表 `processed_tasks(id, processed_at)` | V1+ 预留 |
| **NATS 事件推送** | event_id 去重 | 消费者维护 seen_set(60s TTL) |

### E.3 实现指引

```rust
// crates/im-common/src/retry.rs(待实装,V1+)
pub async fn retry_with_backoff<F, Fut, T, E>(
    op: F,
    max_attempts: u32,
    initial: Duration,
    base: u32,
    max_delay: Duration,
    jitter_pct: f64,
) -> Result<T, RetryError<E>>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: IsTransient,  // 自定义 trait,标记是否瞬时错误
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if e.is_transient() && attempt < max_attempts => {
                let exp = initial * base.pow(attempt - 1);
                let delay = exp.min(max_delay);
                let jitter = delay.mul_f64(jitter_pct * (rand::random::<f64>() - 0.5) * 2.0);
                tokio::time::sleep(delay + jitter).await;
                continue;
            }
            Err(e) => return Err(RetryError::Exhausted { attempts: attempt, last: e }),
        }
    }
}
```

## F. 监控 / 告警

| 指标 | 阈值 | 告警 | 备注 |
|---|---|---|---|
| 批任务失败率 | > 5% (1h) | 通知 SRE | JOB-001~004 |
| DLQ 累积 | > 100 条 | 通知 SRE | 全局 |
| 单任务 1 周 DLQ 次数 | > 10 次 | 自动 issue | 同 task_id 聚合 |
| 重试次数 > 2 | 每次 | 记录 audit | 全部 batch |
| 批任务执行超时 | > 阈值(任务定) | 通知 SRE | |
| NATS DLQ consumer lag | > 1000 | 通知 SRE | V1+ |

## G. 重跑步骤模板(每作业)

> 实际 `jobctl` CLI V1+ 实装;MVP 阶段手动跑 SQL / 调用 API。

```bash
# 1. 检查作业状态(V1+)
./jobctl status JOB-001

# 2. (可选)重置检查点(V1+)
./jobctl reset JOB-001 --from step=2

# 3. 重跑(可选 dry-run,V1+)
./jobctl run JOB-001 [--from <step>] [--dry-run]

# 4. 验证
./jobctl verify JOB-001

# 5. DLQ 处置
./jobctl dlq list --since 2026-09-01T00:00:00Z
./jobctl dlq replay <dlq_id>     # 重放单条
./jobctl dlq discard <dlq_id>    # 永久丢弃(写 audit)
```

## H. IM1.0 MVP 实际案例

### H.1 JOB-100 撤回消息全员通知(同步批处理)

**输入**: `POST /v1/conversations/{id}/messages/{msg_id}/recall`(actor = sender)

**流程**:
1. 校验 actor == message.sender_id(否则 `FORBIDDEN`)
2. 校验 now - message.created_at ≤ env.settings.message.recall_window_seconds(否则 `RECALL_WINDOW_EXPIRED`)
3. 事务内:
   a. `UPDATE messages SET state = 'recalled' WHERE id = $1 AND state IN ('sent', 'delivered', 'read')` (CAS 乐观锁)
   b. INSERT `audit_logs(action='message.recalled', target_type='message', target_id=msg_id, detail={"actor_id":...})`
4. 事务提交后:
   a. `NatsEventPublisher.publish("im.message.recalled", {message_id, conversation_id, sequence})` (MVP 占位,失败重试 3 次 → 失败 DLQ)
   b. 各在线成员的 WS 连接收到 `message_recalled` 帧(im-gateway 已订阅 NATS)

**失败处理**:
- 步骤 3 失败 → 整体回滚,返 5xx(无 DLQ,客户端可重试)
- 步骤 4a 失败(NATS) → 步骤 3 已提交,消息已撤回;DLQ 记录 + 告警;客户端轮询补偿
- 步骤 4b 失败(WS 推送) → NATS 已发,im-gateway 离线时下次 reconnect 拉 `after_sequence` 看不到 recalled(因为 WS 帧需服务端补发,IM1.0 协议不补发,见 `ImplementationSpec §3.2.3` 注释)

**幂等保证**:
- 重复撤回(已 recalled) → `INVALID_STATE_TRANSITION`(状态机阻断)
- NATS 重复 publish(同 event_id) → 消费者 60s 内去重

### H.2 JOB-101 接受好友申请(同步批处理)

**输入**: `POST /v1/friends/requests/{id}/respond {accept: true}`

**流程**:
1. 校验 actor == friend_request.recipient_id(否则 `FORBIDDEN`)
2. 事务内:
   a. `UPDATE friend_requests SET state = 'accepted' WHERE id = $1 AND state = 'pending'`
   b. INSERT `friendships (env, user_id=sender, friend_id=recipient, state='accepted')` + 反向 `friendships (env, user_id=recipient, friend_id=sender, state='accepted')`
   c. INSERT `audit_logs(action='friend.accepted', target_type='user', target_id=sender)`
3. 事务提交后:
   a. `NatsEventPublisher.publish("im.friend.accepted", {...})`

**失败处理**: 同 H.1(NATS 失败 DLQ)

**幂等保证**:
- 重复接受(已 accepted) → `INVALID_STATE_TRANSITION`
- DB 唯一约束阻断反向 friendships 重复 INSERT

### H.3 JOB-102 拉黑(V1+ 半自动)

**输入**: `POST /v1/friends/{id}/block`(204 成功)

**流程**:
1. 校验 actor != target(否则 `VALIDATION_ERROR`)
2. `INSERT INTO friendships (env, user_id=actor, friend_id=target, state='blocked') ON CONFLICT (env, user_id, friend_id) DO UPDATE SET state='blocked'`
3. **副作用**(V1+):若之前 accepted 关系存在 → 通知双方(send system message)

**失败处理**: 幂等(重复拉黑返 204)

**幂等保证**: `ON CONFLICT DO UPDATE` + state 唯一

## I. 验收标准 (Acceptance Criteria)

- [ ] §A 4 类批任务(定时 / 事件 / 同步 / 手动)分类清晰,MVP / V1+ 区分明确
- [ ] §B 6 类错误分类 + IM1.0 出现处
- [ ] §C 重试参数统一(3 次 / 1s / 2x / 60s / ±20% / 5min)
- [ ] §D DLQ 协议完整(触发 / 结构 / 存储 / 处理 / 告警)
- [ ] §E 9 个幂等场景实现矩阵 + Rust trait 草案
- [ ] §F 监控指标 + 阈值明确
- [ ] §H 3 个 MVP 实际案例(JOB-100/101/102)含失败处理 + 幂等保证
- [ ] 所有批任务写 `audit_logs`(对应 `migrations/0006 §known actions`)
- [ ] 不编造未实装的工具(如 `jobctl`),明确标记 V1+

## J. 关联文档 (References)

- 协议源: `docs/ImplementationSpec.md` §3.2(WS 协议)+ §4.6(迁移规则)+ §6.3(配置热重载)+ §7.4.5(EventPublisher)+ §10(测试)
- 字段字典: `aux-02-data-dictionary.md` §F.6(friend_requests)/ §F.7(friendships)/ §F.12(messages)/ §F.14(audit_logs)
- 状态机: `aux-04-state-machine-spec.md` §B.2 / §B.3 / §B.4
- 错误码: `aux-03-error-code-registry.md` §B(`RECALL_WINDOW_EXPIRED` / `FRIEND_REQUEST_EXISTS` / `INVALID_STATE_TRANSITION`)
- 协议帧: `aux-13-protocol-frame-samples.md` §1.1(WS 帧)
- SQL 索引: `migrations/0005_create_messages_reactions.sql` 第 25/27 行(`UNIQUE NULLS NOT DISTINCT`)+ `migrations/0004` 第 35 行(`dm_pairs` PK)+ `migrations/0006_create_audit_logs.sql`
- 性能基线: `aux-06-algorithm-performance-model.md` §B
- 命名规范: `aux-01-naming-convention.md` §D / §G
- 限流: `ImplementationSpec §11.3` + `crates/im-gateway/ratelimit.rs`(Valkey 令牌桶)
- 任务清单: `docs/Day1-Task-List.md` 14 天任务

## K. 已知缺口 (Known Gaps)

> **缺标比错标安全** — Ulysses 2026-08-26 08:40 JST 硬约束

| 编号 | 缺口 | 影响 | 跟进 |
|---|---|---|---|
| GAP-1 | `jobctl` CLI 工具 V1+ 实装,MVP 阶段手动跑 SQL/API | 运维无可视化 | V1+ 实装 |
| GAP-2 | `dlq_records` 专表 V1+ 才建,MVP 阶段用 `audit_logs.detail` JSONB 替代 | DLQ 检索不友好 | V1+ 单独迁移文件 |
| GAP-3 | NATS JetStream 真实实装待 D-3,当前 45 行占位 | §D.3 NATS DLQ 无法落地 | WBS D-3 token 300K-600K |
| GAP-4 | JOB-001~004 定时任务 MVP 不实装,仅设计占位 | 撤回 / 过期 / 清理全靠应用层 | V1+ 实装 |
| GAP-5 | 告警渠道(企业微信 / 邮件 / Slack)MVP 无 | DLQ 靠人巡检 | V1+ Observability §OBS-ALT |
| GAP-6 | NATS 消费者 event_id 去重 60s TTL 留 V1+ 实装 | 极端情况重复推送 | V1+ Valkey Set |
| GAP-7 | `IsTransient` Rust trait 待实装 | 重试逻辑分散在各 service | V1+ `im-common/src/retry.rs` |
| GAP-8 | 限流降级策略(§C.3 A-009)MVP 阶段"放行所有请求"风险 | Valkey 长时间不可用 → 限流失效 | V1+ 双层限流(本地 + 远程) |
| GAP-9 | JOB-100 撤回后 `im.message.recalled` 事件失败,客户端无补偿机制 | 接收方可能不知消息被撤回 | 客户端轮询 `/v1/conversations/{id}/messages?after_sequence=N`(已含 recalled 状态) |
| GAP-10 | 自动 issue(P1 告警触发)V1+ 集成 GitHub | 复盘自动化缺失 | V1+ GitHub API |
| GAP-11 | 批任务执行超时不阻断后续,可能任务堆积 | 资源耗尽 | V1+ 信号量 + 任务队列 |
| GAP-12 | DLQ 永久丢弃(`discard`)需要二次确认 + 双人审批 | 误操作风险 | V1+ 流程 |

## L. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 填实 IM1.0 4 类批处理(定时/事件/同步/手动)+ 3 个 MVP 同步批处理案例(JOB-100/101/102);§A 4 任务表 + §B 6 错误分类 + §C 重试参数(3/1s/2x/60s/±20%/5min) + §D DLQ 协议(JSON 结构 + NATS/PG/S3 三层存储) + §E 9 幂等场景实现矩阵;§F 6 监控指标 + §G jobctl CLI 模板(V1+) + §H 3 MVP 案例含失败处理 + 幂等保证;§K 12 项已知缺口(含 D-3 NATS 真实实现 + V1+ `dlq_records` 表 + 双层限流);引用 aux-02/03/04/13 + ImplementationSpec §3.2/§6.3/§7.4.5/§10 + migrations 实际行号 + Day1-Task-List 完整交叉引用 |
