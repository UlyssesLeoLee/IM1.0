---
doc_id: aux-06
title_ja: アルゴリズム性能モデル (IM1.0)
title_zh: 关键算法复杂度 / 性能模型 (IM1.0)
phase: 04-detailed-design-aux
owners: 架构师 (Mavis 接手 agent per DEC-008) + Tech Lead + SRE
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 14 NFR, 45 逻辑设计, 80 性能试验
---

# aux-06. アルゴリズム性能モデル (IM1.0) / 关键算法复杂度 / 性能模型 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: 架构师 + Tech Lead + SRE
> 算法源: `docs/ImplementationSpec.md` §4.5(sequence 分配)+ §5(错误码)+ §6(配置)+ §7.4(trait)+ §10(测试不变量)
> 字段源: `aux-02-data-dictionary.md` §F + `migrations/0001-0006` 索引
> POC 编号: `WBS H-3` POC-01 / POC-02 / POC-03 启动跟踪

## 1. 目的 (Purpose)

为 IM1.0 关键算法(Token 校验 / sequence 分配 / 幂等键查重 / DM 创建幂等 / WS 帧路由 / 限流 / 双密钥 JWT 等 13 项)建立复杂度 + 性能基线,作为性能 NFR 的依据,指导后续 POC 验证 + 性能回归测试。

## 2. 适用范围 (Scope)

性能敏感的所有算法(单次操作 ≤ 100ms 的路径),MVP 阶段覆盖 13 项关键算法(§A 表);非性能敏感(管理面、批处理)留 V1+。

## 3. 责任方 (Owners)

架构师(定义 + 评审)+ Tech Lead(实现 + 单测)+ SRE(基线 + 压测)。算法修改必须经过 POC 验证 + 性能基线更新。

## 4. 前置依赖 (Prerequisites / Inputs)

- NFR 性能(`SRS.md` §14,MVP 未硬约束,本表给出 POC 校准目标)
- 逻辑设计(`ImplementationSpec §7.4` Rust trait)
- DB schema + 索引(`migrations/0001-0006` + `aux-02 §F`)
- 配置项(`ImplementationSpec §6`,本表 §C 引用环境变量)
- POC 编号(`WBS H-3` POC-01 端到端 / POC-02 sequence 行锁压测 / POC-03 双密钥轮换)

## 5. 输出 / 模板正文 (Body)

## A. 关键算法清单(IM1.0 MVP 13 项)

| 编号 | 算法 | 出现位置 | 输入规模 | 性能目标 (P99) | POC 验证 |
|---|---|---|---|---|---|
| A-001 | Access Token 校验 + Claims 解析 | im-gateway auth_middleware | 1 token | < 5ms(带 5s 缓存) | POC-01 |
| A-002 | Refresh Token 校验 + 旋转 | im-core TokenService.refresh | 1 refresh_token | < 50ms(含 1 次 DB 写) | POC-01 |
| A-003 | Conversation Sequence 分配 | im-core MessageService.send_message | 同会话并发写 | < 10ms(行锁等待) | POC-02 |
| A-004 | Idempotency Key 查重 | im-core MessageRepository | 1 (conv, sender, idem_key) | < 5ms(走 UNIQUE 索引) | POC-01 |
| A-005 | DM 重复创建幂等 | im-core ConversationService | 1 对用户 | < 20ms(走 dm_pairs PK) | POC-01 |
| A-006 | 消息增量拉取 (after_sequence) | im-core MessageService.list_messages | 1 conv, N messages | < 50ms(50 条/页) | POC-01 |
| A-007 | 好友列表查询 (含 blocked 过滤) | im-core RelationshipService.list_friends | 1 user | < 50ms(分页 50) | POC-01 |
| A-008 | WS 帧路由 (8 client frames) | im-gateway WsSession | 1 帧 | < 1ms(纯 in-memory dispatch) | POC-01 |
| A-009 | Valkey 令牌桶限流 | im-gateway ratelimit.rs | 1 user + 1 op | < 2ms(1 次 INCR) | POC-01 |
| A-010 | 双密钥 JWT 校验 (v1/v2 并存) | im-core TokenService.validate | 1 token | < 5ms(线性 2 个 key 尝试) | POC-03 |
| A-011 | 好友申请 / 拉黑查询 | im-core RelationshipService | 1 user | < 20ms(走 partial index) | POC-01 |
| A-012 | 媒体预签名 URL 生成 (S3/MinIO) | im-core MediaService.presign | 1 user | < 50ms(S3 PUT 签名) | (无 POC) |
| A-013 | WS 心跳 (ping/pong) | im-gateway heartbeat.rs | 1 连接 | < 1ms(timer tick) | POC-01 |

> **A-014 / A-015** 全文搜索 / Presence 广播 MVP 不接,留 V1+。Observability.md §1.4 现状说明无 OpenSearch / 无 Redis cache 集群。

## B. 性能模型(逐项)

### A-001 Access Token 校验 + Claims 解析

**输入**:
- 1 个 `access_token` (JWT, HS256 签名,默认 TTL 15min = 900s)
- 1 个 `IM_JWT_SIGNING_KEYS` 数组(默认 1 项,轮换期 2 项)

**算法**:
1. 解析 JWT header → 选 `kid` 对应的 signing key(从 v1/v2 数组中找)
2. `HMAC-SHA256.verify(signed_bytes, signature, key)` → Rust `jsonwebtoken` crate
3. 检查 `exp` / `iat` / `nbf` claims
4. 提取 `user_id` / `environment_id` / `tenant_id` / `kind`
5. (可选) 5s 内存缓存(避免 1s 内重复校验同 token,`ImplementationSpec §7.5` Auth_Middleware 缓存 5s)

**复杂度**:
- 时间: O(1) (HMAC 校验 O(1), JSON 解析 O(n),n = claims 长度 ≈ 200 字节)
- 空间: O(1) (单 token 不留缓存条目 < 200 字节)

**性能模型**:
- 单次校验延迟 = JWT 解析 1ms + HMAC verify 0.5ms + claim 提取 0.5ms = ~2ms
- 5s 缓存命中时: < 0.1ms(纯 hashmap lookup)
- **P99 目标**: < 5ms(无缓存); < 1ms(命中缓存)

**优化策略**:
1. **Auth_Middleware 5s 内存缓存**(已纳入 `ImplementationSpec §7.5` 设计)
2. JSON 解析用 `serde_json::from_slice` 避免 `String` 分配
3. JWKS 不预加载(单 secret,直接读 env)

**NFR 链接**: IM-NFR-001 (待 WBS H-4 校准,POC-01 后回填)

**关联实现**: `crates/im-core/identity/token.rs::TokenService::validate_access_token` (待 C-3 实装)

---

### A-002 Refresh Token 校验 + 旋转

**输入**:
- 1 个 `refresh_token` (明文,argon2id hash 后入库;明文传输便于客户端复制)

**算法**:
1. 客户端发 `POST /v1/auth/refresh` 带 `refresh_token`
2. im-gateway 调 im-core `RefreshToken` RPC
3. im-core: argon2id 校验(慢,故意;`argon2 = 0.5` 默认参数)
4. DB: `SELECT FROM device_sessions WHERE user_id = $1 AND refresh_token_hash = $2 AND revoked_at IS NULL` (走 `uniq_device_sessions_active_refresh` UNIQUE 索引 + `idx_device_sessions_revoked_at` partial index)
5. 旋转: UPDATE `revoked_at = now()` + INSERT 新 device_session + 签发新 access
6. 响应新 access + refresh 对

**复杂度**:
- 时间: O(1) argon2id 校验 ~30ms(默认参数)+ 2 次 DB write ~5ms = **~35ms**
- 空间: O(1) 单次无额外占用

**性能模型**:
- 单次延迟 = argon2 ~30ms + DB 校验 ~3ms + DB 旋转 ~5ms + JWT 签发 ~2ms = ~40ms
- **P99 目标**: < 50ms(可接受;argon2 慢是反暴力破解特性)

**优化策略**:
1. argon2 参数用默认 OWASP 推荐(m=19MiB, t=2, p=1)— 不优化
2. DB 旋转放在同一事务,避免部分失败
3. 失败次数计数(单 user 1h 失败 > 10 → 临时锁定 device_session)

**关联实现**: `crates/im-core/identity/token.rs::TokenService::refresh` (待 C-5 实装)

---

### A-003 Conversation Sequence 分配(行锁,关键路径)

**输入**:
- 1 个 `conversation_id`
- 1 条新消息(待 INSERT)

**算法**(`ImplementationSpec §4.5` + `migrations/0004` 第 24 行):
1. `BEGIN TRANSACTION`
2. `SELECT next_sequence FROM conversation_sequences WHERE conversation_id = $1 FOR UPDATE` (行锁)
3. `next_sequence` 自增 1
4. `UPDATE conversation_sequences SET next_sequence = next_sequence + 1 WHERE conversation_id = $1`
5. `INSERT INTO messages (conversation_id, sequence, ...) VALUES ($1, $next_sequence, ...)`
6. `COMMIT`

**复杂度**:
- 时间: O(1) 锁等待 + 3 次 SQL = **~3-5ms(无竞争) / ~50-100ms(高竞争,行锁串行)**
- 空间: O(1)

**性能模型**:
- 单会话串行: 3ms
- 100 并发同会话: P99 达 50ms(100 个事务排队)
- 1000 并发同会话: P99 达 500ms(锁等待,瓶颈)
- **MVP 不优化单会话瓶颈**(POC-02 校准后决定)
- **P99 目标**: < 10ms (单会话 < 100 QPS); < 100ms (单会话 < 1000 QPS)

**优化策略(留 V1+ ADR 候选)**:
1. **Snowflake ID**: 跨服务生成,不依赖 DB 行锁(算法可参考 Twitter Snowflake)
2. **批量预分配**: 每 N ms 一次预分配 1000 个 sequence 到内存
3. **分片 sequence**: 按 sender 拆,每 sender 独立 sequence 空间
4. MVP 阶段**不优化**,先 POC-02 校准单会话并发天花板

**不变量测试**(`ImplementationSpec §10.3` 第 1 条):
- 1000 并发同会话发消息 → sequence 严格单调,无重复,空洞可接受(回滚导致)

**关联实现**: `crates/im-core/message/sequence.rs::SequenceAllocator` (待 C-2 实装)

---

### A-004 Idempotency Key 查重

**输入**:
- `(conversation_id, sender_id, idempotency_key)` 三元组
- `idempotency_key` 长度 ≤ 128 字符,UUID 形态(`ImplementationSpec §11.2`)

**算法**:
1. 客户端必须在 `X-IM-Idempotency-Key` Header 携带
2. 缺 → 400 `INVALID_IDEMPOTENCY_KEY`
3. 查重: `SELECT message_id FROM messages WHERE conversation_id = $1 AND sender_id IS NOT DISTINCT FROM $2 AND idempotency_key = $3` (走 `uniq_messages_idem` UNIQUE NULLS NOT DISTINCT 索引,`migrations/0005` 第 27 行)
4. 命中 → 返回原 message_id,WS 帧 `idempotent_replay: true`
5. 未命中 → INSERT 新消息,序列继续

**复杂度**:
- 时间: O(1) 索引点查 = **~1-3ms**
- 空间: O(1) 单次无额外占用

**性能模型**:
- 单次查重 = 索引 B+ tree 查找 ~1ms + 网络 ~1ms = ~2ms
- **P99 目标**: < 5ms

**关键约束**:
- `UNIQUE NULLS NOT DISTINCT` 是 PG 15+ 特性(`migrations/0005` 注释):系统消息 sender_id=NULL 也要去重
- 若同 key 并发 100 次 INSERT → 99 次 unique violation + 1 次成功(应用层捕获后转 SELECT)
- 实现: `INSERT ... ON CONFLICT (conversation_id, sender_id, idempotency_key) DO NOTHING RETURNING id` — 单 SQL 原子

**不变量测试**(`ImplementationSpec §10.3` 第 2 条):
- 同 `idempotency_key` 100 次并发 → 1 条消息,100 次响应同 message_id

**关联实现**: `crates/im-core/message/repository.rs::MessageRepository::find_by_idempotency_key` (待 C-1 实装)

---

### A-005 DM 重复创建幂等

**输入**:
- 1 对用户 `(user_a, user_b)`,env 隔离
- `user_a < user_b` 强制规范化(`migrations/0004` 第 37 行 CHECK)

**算法**:
1. 客户端 `POST /v1/conversations {kind: "dm", member_user_ids: [other_user_id]}`
2. im-core: 规范化 `user_a = min(creator, member[0])`,`user_b = max(creator, member[0])`
3. `INSERT INTO dm_pairs (environment_id, user_a, user_b, conversation_id) VALUES (...) ON CONFLICT (environment_id, user_a, user_b) DO NOTHING RETURNING conversation_id`
4. 若 RETURNING 为空 → 重新 `SELECT conversation_id FROM dm_pairs WHERE environment_id = $1 AND user_a = $2 AND user_b = $3`(返回原 conversation)
5. 若 RETURNING 有值 → INSERT conversations + conversation_sequences + conversation_members(2 行)

**复杂度**:
- 时间: O(1) 主键点查 + 1-2 次 INSERT = **~3-5ms**
- 空间: O(1)

**性能模型**:
- 首次创建 = 4 次 SQL(INSERT conversations + INSERT sequence + INSERT dm_pairs + INSERT 2 members)= ~8ms
- 重复命中 = 1 次 SELECT + 1 次 INSERT(走 ON CONFLICT) = ~3ms
- **P99 目标**: < 20ms

**不变量测试**(`ImplementationSpec §10.3` 第 6 条):
- 同对用户 100 次并发创建 DM → 1 个 conversation,100 次返回同 conversation_id

**关联实现**: `crates/im-core/conversation/service.rs::ConversationService::find_or_create_dm` (待 C-8 实装)

---

### A-006 消息增量拉取 (after_sequence)

**输入**:
- 1 个 `conversation_id`
- `after_sequence: i64`(默认 0)
- `limit: i32`(默认 50,最大 200)

**算法**:
1. 客户端 `GET /v1/conversations/{id}/messages?after_sequence=N&limit=50`
2. im-gateway 调 im-core `ListMessages` RPC
3. im-core: `SELECT id, sequence, sender_id, kind, content, reply_to, state, created_at, edited_at FROM messages WHERE conversation_id = $1 AND sequence > $2 ORDER BY sequence ASC LIMIT $3` (走 `idx_messages_conversation_seq` 复合索引,`migrations/0005` 第 32 行)
4. 响应 `{messages: [...], has_more: bool, latest_sequence: i64}`

**复杂度**:
- 时间: O(log N + limit) 索引范围扫描 = **~5-20ms(50 条/页)**
- 空间: O(limit) 返回消息体 JSON

**性能模型**:
- 50 条 = 索引扫描 + 50 行 IO + JSON 序列化 ~10ms
- 200 条 = ~30ms
- 10000 条历史 → 游标分页(N → N+50 → N+100 → ...),单次 < 50ms
- **P99 目标**: < 50ms (50 条/页)

**优化策略**:
1. **Cursor-based 分页**:避免 OFFSET 深分页(传统 OFFSET 100000 = 100ms+)
2. 增量拉取不返回 reactions(单独 API)
3. 大消息体 content 字段单独存(避免 SELECT 全字段)— V1+ 评估

**关联实现**: `crates/im-core/message/service.rs::MessageService::list_messages` (待 C-9 实装)

---

### A-007 好友列表查询 (含 blocked 过滤)

**输入**:
- 1 个 `user_id`
- `cursor: Option<String>`, `limit: i32`(默认 50)

**算法**:
1. 客户端 `GET /v1/friends?cursor=&limit=50`
2. im-gateway 调 im-core `ListFriends` RPC
3. im-core:
   ```sql
   SELECT u.id, u.display_name, u.kind, f.created_at
   FROM friendships f
   JOIN users u ON u.id = f.friend_id AND u.environment_id = f.environment_id
   WHERE f.user_id = $1 AND f.state = 'accepted'
   ORDER BY f.created_at DESC
   LIMIT $2
   ```
   (走 `idx_friendships_user` 索引,`migrations/0003` 第 39 行)
4. **过滤**: 双向 is_blocked 检查(单边 blocked 不出现在列表)

**复杂度**:
- 时间: O(log N + limit) = **~10-30ms(50 条)**
- 空间: O(limit) 50 个 user JSON

**性能模型**:
- 50 条 = ~15ms
- 200 条 = ~40ms
- 1000 好友 → 游标分页
- **P99 目标**: < 50ms

**不变量**:
- 不返回已被 blocked 的关系(`ImplementationSpec §3.1.4` 实施要求)
- 不返回 `kind=guest` 的临时访客(仅 user)

**关联实现**: `crates/im-core/relationship/service.rs::RelationshipService::list_friends` (待 C-1 实装)

---

### A-008 WS 帧路由 (8 client frames)

**输入**:
- 1 个 WS Text Frame(JSON)
- 1 个 `WsSession`(已 auth 绑定 user_id)

**算法**(`ImplementationSpec §3.2.1` + §7.5 `ws/router.rs`):
1. JSON parse: `serde_json::from_str::<ClientFrame>(text)` → 1ms
2. `match frame.type { auth, send_message, edit_message, recall_message, react, mark_read, typing, ping }` → 0.1ms
3. 路由到对应 handler:
   - `auth` → `WsSession::handle_auth` (调 im-core ValidateAccessToken RPC)
   - `send_message` → `MessageService::send_message` (gRPC)
   - 其他 → 类似
4. 构造响应帧 → push 回 WS

**复杂度**:
- 时间: O(1) 路由 + handler 耗时 = **~1ms(路由) + handler P99**
- 空间: O(帧大小) ≈ 200 字节

**性能模型**:
- 帧路由 = 1ms(纯 in-memory)
- `send_message` handler = 5ms(A-003 + A-004)
- 总 P99: < 10ms(不含下游)

**关联实现**: `crates/im-gateway/ws/router.rs::route` + `crates/im-gateway/ws/session.rs::WsSession` (待 C-11 实装)

---

### A-009 Valkey 令牌桶限流

**输入**:
- 1 个 `(env_id, user_id, op)` 元组
- 桶容量: `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` (默认 60)
- 时间窗: 60s

**算法**(`ImplementationSpec §11.3` + `crates/im-gateway/ratelimit.rs`):
1. 客户端发请求
2. im-gateway: `RATE_LIMIT_KEY = "rate:{env_id}:{user_id}:{op}"`
3. `INCR rate_key` → 当前计数
4. 若计数 == 1 → `EXPIRE rate_key 60`(设置窗口)
5. 若计数 > 桶容量 → 拒绝(429 RATE_LIMITED + `Retry-After` 头)
6. 否则放行

**复杂度**:
- 时间: O(1) 1 次 INCR + 1 次 EXPIRE = **~1-2ms(网络 RTT 0.5ms × 2)**
- 空间: O(1) Valkey 1 个 key

**性能模型**:
- Valkey 同集群: < 2ms P99
- 跨集群: < 5ms
- **P99 目标**: < 2ms(同集群)

**优化策略**:
1. **Lua 脚本原子化**: 1 次 EVAL(避免 INCR + EXPIRE 2 次 RTT)
2. 滑动窗口(替代固定窗口):避免边界 burst
3. 集群客户端路由(单 key 在单 slot,无跨 slot 问题)

**NFR 链接**: IM-NFR-RATE-001 (60 msg/min/user,可在 env 覆盖)

**关联实现**: `crates/im-gateway/ratelimit.rs::RateLimiter` (待 D-4 实装)

---

### A-010 双密钥 JWT 校验 (v1/v2 并存)

**输入**:
- 1 个 `access_token` (HS256)
- `IM_JWT_SIGNING_KEYS` JSON 数组(轮换期 2 项,kid 区分 v1/v2)

**算法**(`ImplementationSpec §6.4` + §14.4 密钥轮换):
1. 解析 JWT header,提取 `kid`
2. 在 `IM_JWT_SIGNING_KEYS` 数组中找 `kid` 对应的 key
3. 找不到 → 401 `UNAUTHORIZED` (kid 已下线)
4. 找到 → HMAC verify + claims 校验
5. 轮换期 7 天后,移除 v1,只保留 v2

**复杂度**:
- 时间: O(1) 线性查找 + HMAC verify = **~2-3ms(2 keys)**
- 空间: O(1)

**性能模型**:
- 单 key: 2ms
- 双 key: 3ms(线性尝试 2 次)
- **P99 目标**: < 5ms(双 key 状态)

**优化策略**:
1. 用 `HashMap<kid, key>` 预建索引(避免线性查找)— O(1) 替代 O(N)
2. 短数组(N ≤ 4),线性查找也可接受

**不变量**(`POC-03`):
- 双 key 状态稳定运行 7 天,期间 v1 token 占比 < 1% → 可移除 v1
- 任何时间点至少 1 个有效 kid 存在

**关联实现**: `crates/im-core/identity/token.rs::TokenService::validate_access_token` (待 C-3 + 密钥轮换脚本)

---

### A-011 好友申请 / 拉黑查询

**输入**:
- 1 个 `user_id`(查询该用户的"待处理收件箱"或"已发申请")

**算法**:
1. 收件箱:`SELECT FROM friend_requests WHERE recipient_id = $1 AND state = 'pending' ORDER BY created_at DESC LIMIT 50` (走 `idx_friend_requests_recipient` partial index WHERE state='pending',`migrations/0003` 第 24 行)
2. 已发:`SELECT FROM friend_requests WHERE sender_id = $1 AND state = 'pending' ORDER BY created_at DESC LIMIT 50` (走 `idx_friend_requests_sender`,`migrations/0003` 第 25 行)

**复杂度**:
- 时间: O(log N + 50) = **~5-15ms**
- 空间: O(50)

**性能模型**:
- 收件箱 50 条 = ~10ms
- 已发 50 条 = ~10ms
- **P99 目标**: < 20ms

**关联实现**: `crates/im-core/relationship/service.rs::RelationshipService::list_*_requests` (V1+ 扩展;MVP 仅 list_friends)

---

### A-012 媒体预签名 URL 生成 (S3/MinIO)

**输入**:
- 1 个 `user_id`
- `content_type: String`
- `size_hint: i64`

**算法**(`ImplementationSpec §3.1.5` + `crates/im-core/media/`):
1. 校验 size ≤ `IM_MESSAGE_MAX_SIZE_BYTES` (默认 64KB,但媒体可能更大 → V1+ 评估)
2. 生成 `media_id = gen_random_uuid()`
3. 用 S3 SDK 签 PUT URL: `s3.get_object_put_url(bucket, key, ttl=15min)`
4. 响应 `{upload_url, media_id, expires_at_unix}`

**复杂度**:
- 时间: O(1) S3 签名 = **~5-20ms(含 S3 RTT)**
- 空间: O(1) URL 字符串

**性能模型**:
- MinIO 同集群: ~10ms
- 跨 region: ~50ms
- **P99 目标**: < 50ms(同集群)

**NFR 链接**: IM-NFR-MEDIA-001 (15min URL 过期)

**关联实现**: `crates/im-core/media/presign.rs` (V1+ 实装;MVP 仅返回 mock URL)

---

### A-013 WS 心跳 (ping/pong)

**输入**:
- 1 个 WS 连接
- 30s 周期(`IM_WS_PING_INTERVAL_SECONDS`)
- 60s 超时(`IM_WS_HEARTBEAT_TIMEOUT_SECONDS`)

**算法**(`ImplementationSpec §3.2.3` + §7.5 `heartbeat.rs`):
1. 客户端每 30s 发 `{"type": "ping"}` 帧
2. im-gateway 立即回 `{"type": "pong", "ts": now}`(< 1ms)
3. 任何 60s 内无帧 → 服务端主动断开
4. WS 鉴权超时:100ms 内未发 `auth` 帧 → 服务端断开(`ImplementationSpec §3.2` 注释)

**复杂度**:
- 时间: O(1) timer tick = **< 1ms**
- 空间: O(连接数) tokio task

**性能模型**:
- 单连接 ping 处理: 0.5ms
- 1 万连接: 5ms / tick(批量处理)
- **P99 目标**: < 1ms(单连接)

**配置**: `IM_WS_PING_INTERVAL_SECONDS=30`, `IM_WS_HEARTBEAT_TIMEOUT_SECONDS=60`(`ImplementationSpec §6.2` 校验表,后者必须 > 前者)

**关联实现**: `crates/im-gateway/ws/heartbeat.rs` (待 C-12 实装)

---

## C. 容量 / 性能 NFR 矩阵(MVP 占位,待 POC 校准)

> **重要**:NFR 具体数字由 WBS H-4 从竞品推导 + POC-01/02 实测后回填;本表给出**基线**目标,MVP 阶段**不硬约束**。

| 维度 | 目标 | 关联算法 | POC 校准 |
|---|---|---|---|
| 单条消息端到端 P99 | < 200ms | A-003, A-004, A-008 | POC-01 |
| 单会话并发写 P99 | < 100ms (100 QPS) | A-003 | POC-02 |
| 好友 / 申请查询 P99 | < 50ms | A-007, A-011 | POC-01 |
| 消息增量拉取 P99 | < 50ms (50 条/页) | A-006 | POC-01 |
| WS 鉴权 P99 | < 50ms (含 RPC) | A-001 | POC-01 |
| 限流判定 P99 | < 2ms (同集群) | A-009 | POC-01 |
| 双密钥轮换 | 7 天观察期 | A-010 | POC-03 |
| 媒体预签 P99 | < 50ms | A-012 | (无 POC,V1+ 评估) |
| 在线连接数 | 10 万 (单 im-gateway pod) | A-013 | POC-01 压测 |
| 总注册用户 | 100 万 (V1 目标) | A-007 | (V1 POC) |

## D. 实测 vs 目标

> MVP 阶段**未跑压测**,本表预留位置;WBS E-1/E-2 完成后回填。

| 算法 | 目标 | 实测平均 | 实测 P99 | 状态 |
|---|---|---|---|---|
| A-001 Token 校验 | < 5ms | (待测) | (待测) | ⏳ POC-01 |
| A-002 Refresh 旋转 | < 50ms | (待测) | (待测) | ⏳ POC-01 |
| A-003 Sequence 分配 | < 10ms | (待测) | (待测) | ⏳ POC-02 |
| A-004 幂等键查重 | < 5ms | (待测) | (待测) | ⏳ POC-01 |
| A-005 DM 创建幂等 | < 20ms | (待测) | (待测) | ⏳ POC-01 |
| A-006 消息增量拉取 | < 50ms | (待测) | (待测) | ⏳ POC-01 |
| A-007 好友列表 | < 50ms | (待测) | (待测) | ⏳ POC-01 |
| A-008 WS 帧路由 | < 1ms | (待测) | (待测) | ⏳ POC-01 |
| A-009 限流判定 | < 2ms | (待测) | (待测) | ⏳ POC-01 |
| A-010 双密钥 JWT | < 5ms | (待测) | (待测) | ⏳ POC-03 |
| A-011 好友申请查询 | < 20ms | (待测) | (待测) | ⏳ POC-01 |
| A-012 媒体预签 | < 50ms | (待测) | (待测) | ⏳ V1+ |
| A-013 WS 心跳 | < 1ms | (待测) | (待测) | ⏳ POC-01 |

## E. 性能优化原则

- **测量优先**: 不优化未测量的代码(避免过早优化)
- **局部性**: 数据访问模式优先(走索引、避免全表)
- **批处理**: 减少 RTT(INCR + EXPIRE 用 Lua 合并)
- **缓存**: 注意一致性(5s 短期缓存可接受,长缓存要失效)
- **异步**: 不阻塞关键路径(NATS publish 走 background task)

## F. 性能回退 / 复现(留 V1+)

- 性能回归测试: 每个 PR 与 baseline 对比(留 V1+ CI 集成)
- 回归超阈值: PR 阻断(留 V1+)
- 性能基准: 每周跑全量,看趋势(留 V1+)

## G. POC 编号交叉引用

| POC | 范围 | 责任方 | 状态 | 关联算法 |
|---|---|---|---|---|
| POC-01 | 端到端 smoke(2 终端收发) | Mavis | WBS E-3 + H-3 | A-001, A-002, A-004, A-005, A-006, A-007, A-008, A-009, A-011, A-013 |
| POC-02 | 单会话 sequence 行锁压测 | Mavis + SRE | WBS C-2 + H-3 | A-003 (单会话 100/1000 并发压测) |
| POC-03 | 双密钥轮换 + v1/v2 兼容 | Mavis + SRE | WBS H-3 + 密钥轮换流程 | A-010 (7 天观察期) |
| POC-04 | 媒体上传下载 | Mavis | V1+ G-2 | A-012 |

## H. 验收标准 (Acceptance Criteria)

- [ ] 13 项关键算法(§A 表)有性能模型(算法 / 复杂度 / 性能 / 优化)
- [ ] 每项算法引用具体 Rust trait + SQL 索引 + 配置项
- [ ] POC 编号明确(§G 表)且关联到 WBS H-3
- [ ] 不变量测试引用 `ImplementationSpec §10.3` 10 条
- [ ] 实测位置预留 §D 表,WBS E-1/E-2 完成后回填
- [ ] 容量 NFR 矩阵 (§C) 标注"待 POC 校准",避免编造未验证数字

## I. 关联文档 (References)

- 协议源: `docs/ImplementationSpec.md` §3(API 契约)+ §4.5(sequence 分配)+ §6(配置)+ §7.4(trait)+ §10(测试不变量)+ §11(安全红线)
- 字段字典: `aux-02-data-dictionary.md` §F.4 / §F.6 / §F.8 / §F.10 / §F.11 / §F.12
- 状态机: `aux-04-state-machine-spec.md` §B
- 错误码: `aux-03-error-code-registry.md` §B
- 协议帧: `aux-13-protocol-frame-samples.md` §1
- DB schema: `migrations/0001-0006`
- 性能 NFR 占位: `SRS.md §14`(待 WBS H-4 校准)
- POC 编号: `docs/Project-Status.md` §1 + `132-wbs.md` §H-3

## J. 已知缺口 (Known Gaps)

| 编号 | 缺口 | 影响 | 跟进 |
|---|---|---|---|
| GAP-1 | A-014 全文搜索 / A-015 Presence 广播 MVP 不接,留 V1+ | 搜索 / 在线状态功能缺失 | V1+ 实装 |
| GAP-2 | 实测数据 §D 全部空,需 POC-01/02/03 跑后回填 | 性能基线未建立 | WBS E-1/E-2 + H-3 |
| GAP-3 | A-012 媒体预签 MVP 无 POC,V1+ 才验 | 媒体功能未压测 | V1+ POC-04 |
| GAP-4 | NFR 具体数字 §C 全部 "待 POC 校准"占位,无硬约束 | MVP 性能基线模糊 | WBS H-4 校准 |
| GAP-5 | A-003 单会话 1000+ 并发 P99 500ms 已知瓶颈,MVP 不优化 | 高 QPS 单会话可能超时 | V1+ ADR 候选(§C 优化策略) |
| GAP-6 | A-005 DM 重复创建"ON CONFLICT"依赖 PG 9.5+ 语法 | PG 18.6 支持,但需 CI 验证 | WBS B-1 PG 18.6 真跑 |
| GAP-7 | A-009 限流 INCR + EXPIRE 非原子(中间崩溃可能无 TTL) | 极端情况下 key 永驻 | Lua 脚本原子化(留 V1+) |
| GAP-8 | A-010 双密钥轮换 7 天观察期具体监控指标未定义 | 轮换期无自动判断 v1 占比 | POC-03 实装时定 |
| GAP-9 | A-008 WS 帧路由无超时机制(handler 阻塞 → WS 不响应) | 1 个慢 handler 阻塞整个连接 | 加 tokio timeout(留 V1+) |
| GAP-10 | 性能回归测试 CI 阻断未实施 | 性能回退无自动发现 | V1+ CI 集成 |

## K. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 填实 IM1.0 13 项关键算法(A-001~A-013);每项含算法/复杂度/性能模型/优化策略/NFR 链接;§C NFR 矩阵占位待 POC 校准;§D 实测表预留;§G POC-01/02/03/04 编号与 WBS H-3 + 132-wbs.md 关联;§J 10 项已知缺口;引用 ImplementationSpec §3/§4.5/§6/§7.4/§10 + migrations 索引 + aux-02/03/04 完整交叉引用 |
