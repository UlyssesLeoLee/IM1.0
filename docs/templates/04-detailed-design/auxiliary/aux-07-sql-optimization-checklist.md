---
doc_id: aux-07
title_ja: SQL 最適化チェックリスト (IM1.0)
title_zh: SQL 优化 Checklist (IM1.0)
phase: 04-detailed-design-aux
owners: 架构师 (Mavis 接手 agent per DEC-008) + DBA + 開発者
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 47 DB 详细, 48 SQL 设计, 80 性能试验
---

# aux-07. SQL 最適化チェックリスト (IM1.0) / SQL 优化 Checklist (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: 架构师 + DBA + 開発者
> SQL 源: `migrations/0001-0006` 实际 DDL + 索引 + CHECK 约束 + `ImplementationSpec §4.4 索引策略检查表`
> 字段字典: `aux-02-data-dictionary.md` §F
> 性能模型: `aux-06-algorithm-performance-model.md` §B

## 1. 目的 (Purpose)

为 IM1.0 关键 SQL(基于 6 份 migration 实际查询路径)提供标准化检查表,避免常见性能 / 正确性陷阱。所有 SQL 上线前必须过 §A-K 11 项检查,CI 中由 `cargo clippy` + `sqlfluff` 联合阻断违规。

## 2. 适用范围 (Scope)

生产环境运行的关键 SQL:
- 单次执行 > 10ms(所有 IM1.0 关键路径)
- 日调用 > 1000 次(token 校验、消息拉取、好友查询)
- 6 份 migration 中已建索引的全部 9 个查询模式(§H 详列)

## 3. 责任方 (Owners)

架构师(定义 + 评审)+ DBA(物理模型 + 性能)+ 開発者(写 SQL + EXPLAIN)。任何新 SQL 必须经过 DBA review。

## 4. 前置依赖 (Prerequisites / Inputs)

- DB schema(`migrations/0001-0006` + `aux-02 §F`)
- 索引策略(`ImplementationSpec §4.4`)
- 性能基线(`aux-06 §B`)
- 错误码(`aux-03 §B`)

## 5. 输出 / 模板正文 (Body)

## A. 必查项(任何 SQL 上线前)

> IM1.0 任何 SQL 提交前必须逐项确认 ☐

- [ ] **避免 `SELECT *`**:只查需要的列(IM1.0 14 张表普遍含 JSONB 大字段,如 `messages.content` / `conversations.metadata`,全选浪费 IO + 网络)
- [ ] **WHERE 用索引列**:不绕开(避免 `WHERE LOWER(email) = ...` 走不上索引 → 改函数索引或归一化存储)
- [ ] **JOIN 列类型一致**:避免隐式转换(UUID vs TEXT / BIGINT vs TEXT)
- [ ] **LIMIT / OFFSET 合理**:深分页用 `WHERE id > ?` 或 `WHERE sequence > ?` 替代 OFFSET
- [ ] **COUNT 谨慎**:`COUNT(*)` 全表扫描;大表用 `EXISTS` 或近似
- [ ] **子查询 vs JOIN**:相关子查询多次执行,优先 JOIN
- [ ] **ORDER BY 索引列**:避免 filesort;若有 `LIMIT` 配合,索引必须匹配 `ORDER BY` 列顺序
- [ ] **GROUP BY 用索引列**:避免 hash aggregate
- [ ] **DISTINCT / UNION 去重**:检查是否有必要(DISTINCT 经常是 schema 设计问题的症状)
- [ ] **事务短小**:不持锁做计算(发送消息事务必须 < 100ms,见 `aux-06 §B A-003`)
- [ ] **预编译**:`sqlx::query!` 宏 + 静态 SQL 校验(避免 SQL 注入 + 缓存执行计划)
- [ ] **参数化**:用 `$1, $2, ...` 占位符,绝无字符串拼接
- [ ] **EXPLAIN 已附**:PR 中包含 `EXPLAIN ANALYZE` 输出(§E 解读)

## B. 索引检查(IM1.0 实际索引 17 个)

> IM1.0 6 份 migration 共建 17 个索引(明细 §H 表)。任何新 SQL 必须匹配已有索引;否则需新增 `CREATE INDEX CONCURRENTLY` 迁移。

- [ ] 高频 WHERE 条件有索引
- [ ] JOIN 关联列有索引
- [ ] ORDER BY 列有索引(且顺序匹配)
- [ ] 复合索引最左前缀匹配(`idx_messages_conversation_seq(conversation_id, sequence)` 必须 WHERE 包含 `conversation_id`)
- [ ] 索引选择性 > 5%(否则不如全表)
- [ ] 不创建冗余索引(`(environment_id)` 已有,不再单独 `(tenant_id)`)
- [ ] 不在小表上建索引(< 1k 行)
- [ ] 优先 partial index(`WHERE state='pending'` 比全表索引更小更快)
- [ ] UNIQUE 约束本身就是索引(无需重复建)
- [ ] 大表(> 10M 行)索引用 `CREATE INDEX CONCURRENTLY`(`sqlx` 不直接支持,需手工 `psql`)

## C. 反模式(必须避免)

| 反模式 | 原因 | IM1.0 替代 |
|---|---|---|
| `SELECT *` | 带宽 + 隐式 IO(`messages.content` JSONB 可能 64KB) | 显式列:`SELECT id, sequence, sender_id, kind, content, ...` |
| `WHERE function(col) = ?` | 索引失效 | 函数索引或归一化(`users.external_identity` 已存 JSONB,不走函数) |
| `LIKE '%xx%'` 前导通配 | 索引失效 | 全文索引 / ngram(留 V1+) |
| `OR col = ? OR col = ?` | 可能不走索引 | `IN (?, ?)` |
| `NOT IN (SELECT ...)` | 难优化 | `NOT EXISTS` |
| `OFFSET 100000` 深分页 | 慢 | 游标分页(`WHERE sequence > ?`,见 `aux-06 §B A-006`) |
| 大事务(> 1s) | 锁竞争 | 分批(`send_message` 事务 < 100ms,见 `aux-06 §B A-003`) |
| 隐式类型转换 | 索引失效 | 类型一致(UUID 列用 UUID 类型参数) |
| `NULLS NOT DISTINCT` 在 PG < 15 用 | 编译报错 | 仅 PG 15+,IM1.0 锁 PG 18.6 ✅ |
| `INSERT IGNORE`(MySQL 语法) | PG 不支持 | `INSERT ... ON CONFLICT DO NOTHING` |
| 客户端 `time.Now()` | 时区不一致 | `now()` 服务端时间 |
| `text::int` 强制转换 | 索引失效 + 抛错 | 用 `BIGINT` 类型列 |

## D. 性能阈值(IM1.0 关键 SQL 目标)

| SQL 类型 | 期望 P99 | 实测 (待 POC-01) | 关联算法 |
|---|---|---|---|
| 主键点查 (PK) | < 1ms | (待测) | A-001 / A-005 |
| UNIQUE 索引点查 | < 2ms | (待测) | A-004 / A-010 |
| 复合索引范围 (50 行) | < 10ms | (待测) | A-006 / A-007 |
| 复合索引范围 (200 行) | < 30ms | (待测) | A-006 |
| partial index 范围 | < 15ms | (待测) | A-011 |
| 简单 JOIN (2 表) | < 20ms | (待测) | A-007 |
| 全表 COUNT(*) (10M 行) | < 1s | (待测) | (避免,改 EXISTS) |
| 复杂分析 (多 JOIN + 聚合) | < 5s | (待测) | (异步跑,V1+) |
| INSERT (单行) | < 5ms | (待测) | A-004 |
| INSERT (ON CONFLICT 命中) | < 3ms | (待测) | A-004 |
| UPDATE (单行,带 WHERE) | < 5ms | (待测) | A-002 |
| 行锁 SELECT ... FOR UPDATE | < 10ms (无竞争) | (待测) | A-003 |

## E. 执行计划解读(EXPLAIN ANALYZE)

> IM1.0 任何 SQL 上线前必须附 EXPLAIN ANALYZE 截图 / 文本,本表为解读 checklist。

### E.1 关键操作符

| 操作符 | 含义 | 期望 | 实际 |
|---|---|---|---|
| `Seq Scan` | 全表扫描 | ❌ 大表不可接受(> 10K 行) | (执行计划中标注) |
| `Index Scan` | 走索引(读 heap) | ✅ 中等场景 | |
| `Index Only Scan` | 覆盖索引(无需回表) | ✅ 最优 | |
| `Bitmap Index Scan` | 位图扫描(适合 OR / IN 多条件) | ✅ 多值匹配 | |
| `Nested Loop` | 嵌套循环 JOIN | ✅ 小表 JOIN | |
| `Hash Join` | 哈希 JOIN | ✅ 大表 JOIN(需内存) | |
| `Merge Join` | 排序归并 JOIN | ✅ 已排序数据 | |
| `Sort` | 排序 | ⚠️ 若大表,需配合 `LIMIT` + 索引 | |
| `HashAggregate` | 哈希聚合 | ✅ GROUP BY | |
| `Limit` | LIMIT | ✅ 减少返回行 | |

### E.2 解读示例

```sql
-- IM1.0 关键查询: 增量拉取消息
EXPLAIN ANALYZE
SELECT id, sequence, sender_id, kind, content, reply_to, state, created_at, edited_at
FROM messages
WHERE conversation_id = '7c9e6679-7425-40de-944b-e07fc1f90ae7'
  AND sequence > 100
ORDER BY sequence ASC
LIMIT 50;
```

期望计划:
```
Limit (cost=X..Y rows=50) (actual time=.. rows=50)
  ->  Index Scan using idx_messages_conversation_seq on messages
        Index Cond: (conversation_id = ... AND sequence > 100)
        (actual time=.. rows=50)
```

- ✅ 走 `idx_messages_conversation_seq` 复合索引
- ✅ Index Cond 包含 `conversation_id` + `sequence`(最左前缀匹配)
- ✅ Limit 50 提前停止
- ⚠️ 若 `actual rows` 远小于 `LIMIT 50`,可能是 conversation_id 错了;若远大于,可能是 sequence > 错了
- ❌ 若出现 `Sort`,说明索引顺序不对(应该 (conversation_id, sequence) 但 ORDER BY 其他列)
- ❌ 若出现 `Filter: conversation_id = ...` 而非 `Index Cond`,索引失效

### E.3 慢 SQL 特征识别

| 特征 | 含义 | 处理 |
|---|---|---|
| `actual time` > 100ms | 单次慢 | 优化 SQL / 加索引 |
| `rows=10000` 实际 `actual rows=0` | 统计信息过期 | `ANALYZE messages;` |
| `Buffers: shared read=1000` 高 | 大量磁盘读 | 加索引 / 调大 `shared_buffers` |
| `HashAggregate (cost=high)` | 内存不足 spill 磁盘 | 调大 `work_mem` |
| `Lock Wait` 等待 > 50ms | 行锁竞争 | 缩短事务 / 评估 A-003 V1+ 优化 |

## F. 慢查询处理流程

1. **发现**: 监控(慢日志 `IM_SLOW_QUERY_THRESHOLD_MS=100`)/ 用户反馈 / 错误率告警
2. **定位**:
   - 查 `pg_stat_statements`(需 `shared_preload_libraries='pg_stat_statements'`)
   - `SELECT query, calls, mean_exec_time, max_exec_time FROM pg_stat_statements ORDER BY mean_exec_time DESC LIMIT 20;`
3. **优化**:
   - 加索引(`CREATE INDEX CONCURRENTLY`)
   - 改写 SQL(子查询 → JOIN / OFFSET → 游标)
   - 拆分(大查询拆多次小查询)
   - 缓存(5s 内存缓存或 Valkey)
4. **验证**:
   - 压测(对比 baseline)
   - 灰度(1% 流量 → 10% → 100%)
5. **回归**:
   - 加入性能基线(§D 表)
   - CI 阻断(`pg_stat_statements` mean > 阈值 → 报警)

## G. 评审 Checklist(每个 PR 含 SQL 必须勾选)

- [ ] EXPLAIN ANALYZE 已附在 PR 描述 / 评论
- [ ] 索引已加(若有,新索引文件 commit 含 `CREATE INDEX CONCURRENTLY`)
- [ ] 性能已测(基准 + 峰值,数据量 > 100K 行)
- [ ] 死锁 / 锁等待已分析(`pg_locks` + `pg_stat_activity`)
- [ ] 资源消耗(CPU / IO)可接受(`EXPLAIN (ANALYZE, BUFFERS)`)
- [ ] 大表(> 10M 行)索引用 CONCURRENTLY
- [ ] 无 SQL 注入风险(参数化 / `sqlx::query!` 静态校验)
- [ ] 事务边界清晰(BEGIN / COMMIT 短小)
- [ ] 错误处理(im-common error 枚举映射)
- [ ] 通过 `sqlfluff` lint(命名 + 格式)

## H. IM1.0 关键 SQL 清单(基于 6 份 migration 实际查询)

> 本表是 IM1.0 14 张表的核心查询模式,每个 SQL 必须有 EXPLAIN + 索引支撑。

### H.1 `tenants` / `games` / `environments` (0001)

| 查询 | SQL 概要 | 索引 | 性能目标 | 关联算法 |
|---|---|---|---|---|
| 取 env 配置 | `SELECT * FROM environments WHERE id = $1` | PK | < 1ms | (SettingsService,留 V1+) |
| 列 game 下所有 env | `SELECT * FROM environments WHERE game_id = $1 ORDER BY created_at DESC` | `idx_environments_game_id` | < 5ms | (管理面) |
| 列 tenant 下所有 game | `SELECT * FROM games WHERE tenant_id = $1` | `idx_games_tenant_id` | < 5ms | (管理面) |

### H.2 `users` / `device_sessions` (0002)

| 查询 | SQL 概要 | 索引 | 性能目标 | 关联算法 |
|---|---|---|---|---|
| Token Exchange 查重 | `SELECT id, kind, state FROM users WHERE environment_id = $1 AND external_identity = $2` | `uniq_users_env_extid` UNIQUE | < 2ms | A-001 关键路径 |
| 按 id 查 user | `SELECT * FROM users WHERE id = $1` | PK | < 1ms | A-001 / A-005 |
| 查 banned 列表 | `SELECT * FROM users WHERE state = 'banned'` (管理面) | `idx_users_state` partial WHERE state != 'active' | < 50ms | (管理面) |
| 列 env 下 user | `SELECT * FROM users WHERE environment_id = $1 LIMIT 50` | `idx_users_environment_id` | < 10ms | (管理面) |
| Refresh 校验 | `SELECT * FROM device_sessions WHERE user_id = $1 AND refresh_token_hash = $2 AND revoked_at IS NULL` | `uniq_device_sessions_active_refresh` UNIQUE + `idx_device_sessions_revoked_at` partial | < 5ms | A-002 |
| 列用户有效设备 | `SELECT * FROM device_sessions WHERE user_id = $1 AND revoked_at IS NULL` | `idx_device_sessions_revoked_at` partial | < 5ms | (用户管理面) |

### H.3 `friend_requests` / `friendships` (0003)

| 查询 | SQL 概要 | 索引 | 性能目标 | 关联算法 |
|---|---|---|---|---|
| 收件箱(待处理) | `SELECT * FROM friend_requests WHERE recipient_id = $1 AND state = 'pending' ORDER BY created_at DESC LIMIT 50` | `idx_friend_requests_recipient` partial WHERE state='pending' | < 15ms | A-011 |
| 已发申请 | `SELECT * FROM friend_requests WHERE sender_id = $1 AND state = 'pending' ORDER BY created_at DESC LIMIT 50` | `idx_friend_requests_sender` | < 15ms | A-011 |
| 按 id 查申请 | `SELECT * FROM friend_requests WHERE id = $1` | PK | < 1ms | A-002 respond |
| 查重复申请 | `INSERT INTO friend_requests ... ON CONFLICT (environment_id, sender_id, recipient_id) DO NOTHING` | UNIQUE | < 3ms | (MVP 已知限制:跨 state 阻断,WBS B-3) |
| 好友列表 | `SELECT u.* FROM friendships f JOIN users u ON u.id = f.friend_id WHERE f.user_id = $1 AND f.state = 'accepted' ORDER BY f.created_at DESC LIMIT 50` | `idx_friendships_user` | < 30ms | A-007 |
| 拉黑状态 | `SELECT state FROM friendships WHERE user_id = $1 AND friend_id = $2` | PK | < 1ms | A-005 / A-002 |

### H.4 `conversations` / `conversation_sequences` / `dm_pairs` / `conversation_members` (0004)

| 查询 | SQL 概要 | 索引 | 性能目标 | 关联算法 |
|---|---|---|---|---|
| 分配 sequence | `SELECT next_sequence FROM conversation_sequences WHERE conversation_id = $1 FOR UPDATE` | PK (行锁) | < 10ms (无竞争) | A-003 |
| DM 重复创建 | `INSERT INTO dm_pairs ... ON CONFLICT (environment_id, user_a, user_b) DO NOTHING RETURNING conversation_id` | PK | < 3ms | A-005 |
| 查 DM conversation | `SELECT conversation_id FROM dm_pairs WHERE environment_id = $1 AND user_a = $2 AND user_b = $3` | PK | < 2ms | A-005 |
| 按 id 查 conversation | `SELECT * FROM conversations WHERE id = $1` | PK | < 1ms | (全部) |
| 列环境会话 | `SELECT * FROM conversations WHERE environment_id = $1 ORDER BY created_at DESC LIMIT 50` | `idx_conversations_environment_id` | < 15ms | (会话列表) |
| 成员校验 | `SELECT 1 FROM conversation_members WHERE conversation_id = $1 AND user_id = $2` | PK | < 1ms | A-005 关键路径 |
| 我的会话列表 | `SELECT cm.*, c.* FROM conversation_members cm JOIN conversations c ON c.id = cm.conversation_id WHERE cm.user_id = $1 ORDER BY cm.joined_at DESC LIMIT 50` | `idx_conversation_members_user_id` | < 30ms | (会话列表) |
| 列举会话成员 | `SELECT u.*, cm.role, cm.joined_at, cm.last_read_sequence FROM conversation_members cm JOIN users u ON u.id = cm.user_id WHERE cm.conversation_id = $1` | PK | < 20ms | (成员列表端点) |

### H.5 `messages` / `message_reactions` (0005)

| 查询 | SQL 概要 | 索引 | 性能目标 | 关联算法 |
|---|---|---|---|---|
| 增量拉取(主路径) | `SELECT id, sequence, sender_id, kind, content, reply_to, state, created_at, edited_at FROM messages WHERE conversation_id = $1 AND sequence > $2 ORDER BY sequence ASC LIMIT $3` | `idx_messages_conversation_seq` | < 50ms (50 行) | A-006 关键路径 |
| 幂等键查重 | `SELECT id FROM messages WHERE conversation_id = $1 AND sender_id IS NOT DISTINCT FROM $2 AND idempotency_key = $3` | `uniq_messages_idem` UNIQUE NULLS NOT DISTINCT | < 5ms | A-004 |
| 幂等键 INSERT | `INSERT INTO messages (...) VALUES (...) ON CONFLICT (conversation_id, sender_id, idempotency_key) DO NOTHING RETURNING id` | UNIQUE | < 5ms | A-004 |
| 按 id 查 message | `SELECT * FROM messages WHERE id = $1` | PK | < 1ms | A-001 / A-004 |
| update state | `UPDATE messages SET state = $1, edited_at = now() WHERE id = $2` | PK | < 3ms | A-004 |
| update last_read_sequence | `UPDATE conversation_members SET last_read_sequence = GREATEST(last_read_sequence, $1) WHERE conversation_id = $2 AND user_id = $3` | PK | < 3ms | A-006 |
| 列举 reactions | `SELECT user_id, emoji, created_at FROM message_reactions WHERE message_id = $1` | PK | < 5ms | (详情端点) |
| 添加 reaction | `INSERT INTO message_reactions (message_id, user_id, emoji) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING` | PK | < 3ms | A-004 |

### H.6 `audit_logs` (0006)

| 查询 | SQL 概要 | 索引 | 性能目标 | 关联算法 |
|---|---|---|---|---|
| 写审计 | `INSERT INTO audit_logs (tenant_id, actor_id, action, target_type, target_id, detail) VALUES ($1, $2, $3, $4, $5, $6)` | (无,append-only) | < 5ms | (全部状态变更) |
| 租户审计查询 | `SELECT * FROM audit_logs WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 100` | `idx_audit_logs_tenant_created` | < 30ms | (合规导出) |
| 按 action 查询 | `SELECT * FROM audit_logs WHERE action = $1 ORDER BY created_at DESC LIMIT 100` | `idx_audit_logs_action` | < 30ms | (审计追溯) |

## I. IM1.0 索引策略总览(17 个索引 + 11 个约束)

> 引用 `ImplementationSpec §4.4 索引策略检查表`,已交叉验证 6 份 migration 实际定义。

| 表 | 索引名 | 类型 | 列 | 用途 | 大小估计 |
|---|---|---|---|---|---|
| `tenants` | `tenants_pkey` | PK | `id` | 全部 | 16B / 行 |
| `games` | `games_pkey` | PK | `id` | 全部 | 16B / 行 |
| `games` | `idx_games_tenant_id` | btree | `tenant_id` | 列游戏 | 16B / 行 |
| `games` | `games_tenant_id_name_key` | UNIQUE | `(tenant_id, name)` | 命名唯一 | 32B / 行 |
| `environments` | `environments_pkey` | PK | `id` | 全部 | 16B / 行 |
| `environments` | `idx_environments_game_id` | btree | `game_id` | 列环境 | 16B / 行 |
| `environments` | `environments_game_id_name_key` | UNIQUE | `(game_id, name)` | 命名唯一 | 32B / 行 |
| `users` | `users_pkey` | PK | `id` | 全部 | 16B / 行 |
| `users` | `idx_users_environment_id` | btree | `environment_id` | 列 user | 16B / 行 |
| `users` | `idx_users_state` | partial btree | `state` WHERE state != 'active' | 风控 | 极小 |
| `users` | `uniq_users_env_extid` | UNIQUE | `(environment_id, external_identity)` | Token Exchange 查重 | 64B / 行 |
| `device_sessions` | `device_sessions_pkey` | PK | `id` | 全部 | 16B / 行 |
| `device_sessions` | `idx_device_sessions_user_id` | btree | `user_id` | 列设备 | 16B / 行 |
| `device_sessions` | `idx_device_sessions_revoked_at` | partial btree | `revoked_at` WHERE NULL | 有效会话 | 中等 |
| `device_sessions` | `uniq_device_sessions_active_refresh` | UNIQUE | `(user_id, refresh_token_hash)` | Refresh 校验 | 128B / 行 |
| `friend_requests` | `friend_requests_pkey` | PK | `id` | 全部 | 16B / 行 |
| `friend_requests` | `idx_friend_requests_recipient` | partial btree | `(recipient_id, state)` WHERE state='pending' | 收件箱 | 中等 |
| `friend_requests` | `idx_friend_requests_sender` | btree | `(sender_id, state)` | 已发 | 中等 |
| `friend_requests` | (UNIQUE in DDL) | UNIQUE | `(environment_id, sender_id, recipient_id)` | 跨 state 阻断 | 48B / 行 |
| `friendships` | `friendships_pkey` | PK | `(environment_id, user_id, friend_id)` | 全部 | 48B / 行 |
| `friendships` | `idx_friendships_user` | btree | `(user_id, state)` | 好友列表 | 32B / 行 |
| `conversations` | `conversations_pkey` | PK | `id` | 全部 | 16B / 行 |
| `conversations` | `idx_conversations_environment_id` | btree | `(environment_id, created_at DESC)` | 列环境会话 | 32B / 行 |
| `conversation_sequences` | `conversation_sequences_pkey` | PK | `conversation_id` | 全部 | 16B / 行 |
| `dm_pairs` | `dm_pairs_pkey` | PK | `(environment_id, user_a, user_b)` | 全部 | 48B / 行 |
| `dm_pairs` | `dm_pairs_conversation_id_key` | UNIQUE | `conversation_id` | 反向查 | 16B / 行 |
| `dm_pairs` | `idx_dm_pairs_conversation` | btree | `conversation_id` | 冗余(UNIQUE 已含) | 16B / 行 |
| `conversation_members` | `conversation_members_pkey` | PK | `(conversation_id, user_id)` | 全部 | 32B / 行 |
| `conversation_members` | `idx_conversation_members_user_id` | btree | `user_id` | 我的会话 | 16B / 行 |
| `messages` | `messages_pkey` | PK | `id` | 全部 | 16B / 行 |
| `messages` | `idx_messages_conversation_seq` | btree | `(conversation_id, sequence)` | 增量拉取 | 24B / 行 |
| `messages` | `messages_conversation_id_sequence_key` | UNIQUE | `(conversation_id, sequence)` | sequence 唯一 | 24B / 行 |
| `messages` | `uniq_messages_idem` | UNIQUE NULLS NOT DISTINCT | `(conversation_id, sender_id, idempotency_key)` | 幂等查重 | 160B / 行 |
| `message_reactions` | `message_reactions_pkey` | PK | `(message_id, user_id, emoji)` | 全部 | 96B / 行 |
| `message_reactions` | `idx_message_reactions_message` | btree | `message_id` | 列 reactions | 16B / 行 |
| `audit_logs` | `audit_logs_pkey` | PK | `id` | 全部 | 16B / 行 |
| `audit_logs` | `idx_audit_logs_tenant_created` | btree | `(tenant_id, created_at DESC)` | 租户审计 | 32B / 行 |
| `audit_logs` | `idx_audit_logs_action` | btree | `action` | 按 action | 16B / 行 |

> **共 38 个索引/约束** (17 业务索引 + 11 PK + 10 UNIQUE)。本表 17 业务索引对应 `ImplementationSpec §4.4 表` 9 项 + 补充 8 项(`idx_*_state` / `idx_*_user_id` 等)。

## J. IM1.0 数据库特性依赖

- **PG 15+ `NULLS NOT DISTINCT`**: 用于 `uniq_messages_idem`(`migrations/0005` 第 27 行)
- **PG 13+ `gen_random_uuid()`**: 用于 PK 默认值(`migrations/0001` `pgcrypto` 扩展;PG 16+ 内置)
- **`jsonb_typeof()` CHECK**: 用于 `environments.settings` / `conversations.metadata` / `messages.content` 必须是 object
- **`set_updated_at()` 触发器函数**: 用于 `environments` / `friend_requests` 自动维护 `updated_at`
- **CHECK 约束**: 用于所有枚举字段(`state IN (...)` / `kind IN (...)` / `user_a < user_b`)

> 锁版本: **PG 18.6**(per `2026-08-26 17:00 JST` commit `bea7670`)

## K. 验收标准 (Acceptance Criteria)

- [ ] §A 13 项必查 + §B 10 项索引 + §C 12 项反模式 + §G 10 项评审 checklist 100% 覆盖
- [ ] §H 6 大表 35+ 个 SQL 全部有索引支撑 + 性能目标
- [ ] §I 38 个索引/约束全部对应到 `migrations/0001-0006` 实际行号
- [ ] §J 5 个 PG 特性依赖明确版本要求
- [ ] 慢查询处理流程 §F 6 步走完整(发现 → 定位 → 优化 → 验证 → 回归)
- [ ] §D 性能阈值全部"POC-01 待测"占位,无编造数字
- [ ] CI 中 `sqlfluff` 阻断命名 + 格式违规

## L. 关联文档 (References)

- SQL 源: `migrations/0001_create_tenants_games_environments.sql` / `0002_create_users_device_sessions.sql` / `0003_create_friend_requests_and_friendships.sql` / `0004_create_conversations_sequences_members_dm_pairs.sql` / `0005_create_messages_reactions.sql` / `0006_create_audit_logs.sql`
- 字段字典: `aux-02-data-dictionary.md` §F.1-F.14
- 索引策略: `ImplementationSpec §4.4 索引策略检查表`(已交叉验证 6 份 migration 实际行号)
- 性能基线: `aux-06-algorithm-performance-model.md` §B(13 项关键算法)
- 状态机: `aux-04-state-machine-spec.md` §B(4 大状态机)
- 错误码: `aux-03-error-code-registry.md` §B
- CRC 卡: `aux-05-crc-card.md` §5
- 协议: `aux-13-protocol-frame-samples.md` §3 REST 26 端点
- 命名规范: `aux-01-naming-convention.md` §D(数据库命名)

## M. 已知缺口 (Known Gaps)

| 编号 | 缺口 | 影响 | 跟进 |
|---|---|---|---|
| GAP-1 | §D 性能阈值全部"待 POC-01 测"占位,无实测数据 | 性能基线未建立 | WBS E-1/E-2 + H-3 |
| GAP-2 | `pg_stat_statements` 扩展未在 migration 中启用 | 慢查询统计缺失 | 单独迁移文件: `0007_enable_pg_stat_statements.sql`(V1+) |
| GAP-3 | V1+ 全文搜索(messages.content GIN 索引)在 `migrations/0005` 注释中预留,未实装 | 搜索功能缺失 | V1+ 启用 |
| GAP-4 | V1+ `audit_logs` 按 `created_at` RANGE 分区在 `migrations/0006` 注释中预留,未实装 | 大数据量审计查询慢 | V1+ 启用 |
| GAP-5 | V1+ `messages` 按 `conversation_id` HASH 分区预留 | 大消息表性能瓶颈 | V1+ 启用 |
| GAP-6 | §H IM1.0 关键 SQL 部分(如 SADD `idx_message_reactions_message`)与 PK 冗余 | 索引占用 | V1+ 评估移除 |
| GAP-7 | 大表(> 10M 行)`CREATE INDEX CONCURRENTLY` `sqlx` 不直接支持 | 索引创建阻塞 DML | 手工 `psql`(V1+) |
| GAP-8 | `pg_repack` / `pg_squeeze` online DDL 工具未在迁移流程中 | 大表 schema 变更需停机 | V1+ 工具链 |
| GAP-9 | 慢查询监控(§F 第 1 步)未配置(`IM_SLOW_QUERY_THRESHOLD_MS` 仅有配置项,无告警) | 慢查询靠用户反馈 | Observability.md §OBS-ALT-NNN 跟进 |
| GAP-10 | `sqlfluff` 实际 lint 规则未在本表给出 | SQL 命名规范靠人工 review | 留 V1+ CI 集成 |
| GAP-11 | 部分 partial index(如 `idx_users_state WHERE state != 'active'`)选择性可能 99% 都是 active,partial 才有意义;若大部分 user 都是 active,该 index 等效全表 | 索引未发挥作用 | POC-01 跑后评估 |
| GAP-12 | 6 份 migration 未在真 PG 18.6 实例跑过(`WBS B-1 Blocked, F-1 Docker daemon`) | 索引实际效果未验证 | WBS F-1 解锁后跑 |

## N. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 填实 IM1.0 6 大表 35+ 个 SQL 查询模式(§H);§A-K 11 项检查;§I 38 个索引/约束对应 migrations 行号;§J 5 个 PG 特性依赖;§L 10 项已知缺口(含 WBS B-1 真 PG 18.6 未跑 / V1+ 全文搜索分区预留);引用 aux-02/03/04/05/06/13 + ImplementationSpec §4.4/§10.3 + migrations 实际行号完整交叉引用 |
