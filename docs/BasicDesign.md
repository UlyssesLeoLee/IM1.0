# 可嵌入式游戏 IM 平台 基本设计书（Basic Design）

版本：v0.1 Draft ｜ 范围：**MVP**（对应 `docs/SRS.md` 第45章 MVP Scope）
上游依据：`docs/SRS.md`（产品/需求）、`docs/LiveKit-Voice-Subsystem.md`（语音，V1起纳入，本版本仅预留接口边界）
技术栈基线（不可变更）：Rust（主）+ Python（仅AI扩展，MVP不启用）+ Next.js（Web Dashboard）+ K3s + PostgreSQL + Valkey + NATS JetStream + MinIO，全部依赖须开源可商用。

本文档面向详细设计与编码阶段，粒度到：服务边界、进程内模块、数据表结构、API 契约草案、消息队列 Topic、部署拓扑、配置项。不包含语音子系统的详细设计（见 LiveKit 文档，其详细设计另行输出）。

---

## 1. 设计范围与非目标

**本版本设计的是 MVP**（SRS 第45章）：Identity(User/Guest/Device/Token)、Relationship(好友基础模型)、Conversation(DM/Group/Channel/System)、Message(文本/图片/回复/mention/编辑/删除/撤回)、Presence(在线状态)、Game SDK 五步接入(Unity优先)、Game Identity Bridging(Guest+自定义JWT)、Laser HUD、基础安全(Token/RBAC/ABAC/审计)、Extension Runtime **骨架**(不含任何具体AI/Work扩展实现)。

**不在本版本**：语音（LiveKit集成，V1）、AI Extension、Work Extension、Guild/Party等游戏社交映射的完整实现（仅预留metadata扩展点）、多区域部署、Vector/Graph能力。

## 2. 服务拓扑（对应 SRS 第39章逻辑边界）

```
                         ┌─────────────────────────┐
                         │   Next.js Web Dashboard  │  （管理台/Full Client Web版）
                         └────────────┬─────────────┘
                                      │ HTTPS (REST/GraphQL 待定，MVP用REST)
                                      ▼
┌──────────────────────────────────────────────────────────────────────┐
│                              im-gateway (Rust)                        │
│  职责：WebSocket长连接终结、HTTP API入口、鉴权前置校验、Rate Limit、    │
│         协议转换（WS帧 ⇄ 内部gRPC）、连接会话状态（本地+Valkey）        │
└───────────┬───────────────────────────────┬───────────────────────────┘
            │ gRPC (tonic)                  │ gRPC
            ▼                               ▼
┌───────────────────────┐        ┌──────────────────────────┐
│      im-core (Rust)     │        │   extension-runtime (Rust) │
│  Identity/Relationship  │◄──────►│  Manifest/Event Bus订阅/   │
│  Conversation/Message   │ events │  Command分发/隔离沙箱骨架   │
│  （事务边界，PostgreSQL主存）│        │ (MVP: 骨架+审计，无具体扩展)│
└───────────┬─────────────┘        └───────────┬────────────────┘
            │                                   │
            ├───────────────┐                   │
            ▼               ▼                   ▼
   ┌────────────────┐ ┌───────────────┐  ┌─────────────┐
   │  im-presence    │ │   im-media     │  │ NATS JetStream│（Event Bus，全组件共用）
   │ (Rust, Valkey主)│ │ (Rust, MinIO)  │  └─────────────┘
   └────────────────┘ └───────────────┘
            │               │
            ▼               ▼
        Valkey          PostgreSQL / MinIO
```

**拆分依据回填**（呼应 SRS "拆分必须来自 Scaling/Failure/Security/Ownership Boundary"）：
- `im-gateway` 独立：Scaling Boundary（连接数与业务逻辑吞吐特征不同，网关需要横向扩展以承接连接数，im-core 按事务吞吐扩展）。
- `im-presence` 独立：Scaling Boundary（高频写、可容忍最终一致，适合 Valkey 为主存，与 im-core 的强一致 PostgreSQL 事务边界不同）+ Failure Boundary（Presence抖动不应拖慢消息事务）。
- `im-media` 独立：Security Boundary（媒体上传走预签名URL，不应让上传流量路径经过消息主路径）。
- `extension-runtime` 独立：Failure Boundary（SRS `EXT-FR-002` 红线，扩展故障不得影响 Core，进程级隔离的前提是独立部署单元）。
- MVP 阶段 `im-core` 暂不进一步拆分 identity/relationship/conversation/message 子服务——当前负载下没有实测数据支持拆分，遵循"初期禁止过度微服务化"（ADR-011，见第13章）。

## 3. im-core 内部模块划分（单进程内的模块边界，为将来拆分埋点）

```
im-core/
├── identity/       // User, Guest, DeviceSession, Token, AccountState
├── relationship/    // Friend, FriendRequest, Block, Follow, RecentContact
├── conversation/    // Conversation, Membership
├── message/         // Message, Reaction, DeliveryState, Sequence Generator
├── event/           // 领域事件定义 + 发布到 NATS
└── db/              // sqlx models, migrations
```

模块间禁止跨模块直接访问对方的数据库表（每个模块通过自己的 Repository 接口访问自己拥有的表），为未来按此边界拆分为独立服务做准备（呼应 SRS Ownership Boundary）。

## 4. 数据模型（PostgreSQL，核心表，MVP范围）

```sql
-- identity
CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE games (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name TEXT NOT NULL,
    UNIQUE (tenant_id, name)
);

CREATE TABLE environments (
    id UUID PRIMARY KEY,
    game_id UUID NOT NULL REFERENCES games(id),
    name TEXT NOT NULL,     -- production / test
    settings JSONB NOT NULL DEFAULT '{}',   -- 租户/环境级配置（速率限制、好友开关、Retention策略）
    UNIQUE (game_id, name)
);

CREATE TABLE users (
    id UUID PRIMARY KEY,
    environment_id UUID NOT NULL REFERENCES environments(id),
    kind TEXT NOT NULL CHECK (kind IN ('user','guest')),
    external_identity JSONB,        -- {provider, external_uid} 见第6章身份映射
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active','banned','deleted','suspended')),
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (environment_id, external_identity)
);

CREATE TABLE device_sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    device_fingerprint TEXT,
    refresh_token_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

-- relationship（MVP：好友基础模型，可按 tenant/game 配置为禁用，见 SRS IM-REL-002）
CREATE TABLE friend_requests (
    id UUID PRIMARY KEY,
    environment_id UUID NOT NULL REFERENCES environments(id),
    sender_id UUID NOT NULL REFERENCES users(id),
    recipient_id UUID NOT NULL REFERENCES users(id),
    state TEXT NOT NULL CHECK (state IN ('pending','accepted','rejected','expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (environment_id, sender_id, recipient_id)
);

CREATE TABLE friendships (
    environment_id UUID NOT NULL REFERENCES environments(id),
    user_id UUID NOT NULL REFERENCES users(id),
    friend_id UUID NOT NULL REFERENCES users(id),
    state TEXT NOT NULL CHECK (state IN ('accepted','blocked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (environment_id, user_id, friend_id)
);

-- conversation（Core Schema，禁止游戏专有字段，仅 metadata 扩展点）
CREATE TABLE conversations (
    id UUID PRIMARY KEY,
    environment_id UUID NOT NULL REFERENCES environments(id),
    kind TEXT NOT NULL CHECK (kind IN ('dm','group','channel','system','broadcast')),
    metadata JSONB NOT NULL DEFAULT '{}',   -- namespaced: game.*, ai.*, work.*
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 会话内 Sequence 分配器（强单调、行锁原子自增）
CREATE TABLE conversation_sequences (
    conversation_id UUID PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    next_sequence BIGINT NOT NULL DEFAULT 1
);

-- DM 会话双人唯一对（规避并发创建重复私聊，规范化 user_a < user_b）
CREATE TABLE dm_pairs (
    environment_id UUID NOT NULL REFERENCES environments(id),
    user_a UUID NOT NULL REFERENCES users(id),
    user_b UUID NOT NULL REFERENCES users(id),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    PRIMARY KEY (environment_id, user_a, user_b),
    CHECK (user_a < user_b)
);

CREATE TABLE conversation_members (
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    role TEXT NOT NULL DEFAULT 'member',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_read_sequence BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (conversation_id, user_id)
);

-- message
CREATE TABLE messages (
    id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    sequence BIGINT NOT NULL,          -- 会话内单调递增，见第5章
    sender_id UUID REFERENCES users(id),   -- NULL = 系统消息
    kind TEXT NOT NULL CHECK (kind IN ('text','image','file','sticker','system','custom')),
    content JSONB NOT NULL,
    reply_to UUID REFERENCES messages(id),
    idempotency_key TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'sent' CHECK (state IN ('sent','delivered','read','recalled','deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ,
    UNIQUE (conversation_id, sequence),
    UNIQUE NULLS NOT DISTINCT (conversation_id, sender_id, idempotency_key)
);

CREATE TABLE message_reactions (
    message_id UUID NOT NULL REFERENCES messages(id),
    user_id UUID NOT NULL REFERENCES users(id),
    emoji TEXT NOT NULL,
    PRIMARY KEY (message_id, user_id, emoji)
);

-- audit（呼应 SEC-NFR-006）
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    actor_id UUID,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id UUID,
    detail JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

索引策略：`messages(conversation_id, sequence)` 为核心查询路径（增量同步），`conversation_members(user_id)` 支持"我的会话列表"查询。分区策略（按 `conversation_id` hash 或按时间）留待容量评估后在详细设计中确定（**ADR REQUIRED**，非MVP阻塞项，MVP先用普通表）。

## 5. 消息 Sequence 与幂等设计

- **Sequence 生成**：每个 Conversation 的 Sequence 由 PostgreSQL 内 `conversation_sequences(conversation_id, next_sequence)` 表在数据库事务内执行 `SELECT next_sequence FROM conversation_sequences WHERE conversation_id = $1 FOR UPDATE`，取得当前序号并在同一事务内递增更新且写入 `messages`，保证同一 Conversation 内严格单调、无空洞、无重复（呼应 `IM-FR-002`）。**MVP 采用行锁方案**（实现简单，正确性优先于极限吞吐），若后续压测显示为瓶颈，再评估分布式ID方案（ADR候选，非MVP决策点）。
- **幂等**：客户端生成 `idempotency_key`（如 UUID），与 `(conversation_id, sender_id)` 在 PostgreSQL 15+ 下通过 `UNIQUE NULLS NOT DISTINCT` 联合唯一约束；重复提交返回已存在的消息而非报错，语义为"幂等成功"。
- **Delivery State 状态机**：`sent → delivered → read`，允许旁路到 `recalled`/`deleted`；状态转换记录经由 NATS 事件驱动 `im-presence`/推送逻辑异步更新，不阻塞发送主路径（呼应 IM Core 短路径原则）。

## 6. Identity / Token 设计

```
POST /v1/auth/token/exchange   (Server-to-Server, 游戏服务器调用，携带 Server Secret)
  body: { environment_id, external_provider, external_uid, display_name? }
  → { access_token, refresh_token, user_id }
```

- **Server Secret**：按 Environment 独立签发，存储于 im-core 侧做 HMAC 校验；游戏客户端**永不持有** Server Secret（呼应 `GAME-ID-003` 红线）。
- **Access Token**：JWT，短生命周期（Candidate: 15分钟，**BENCHMARK REQUIRED**），claims 包含 `user_id, environment_id, tenant_id, exp`。
- **Refresh Token**：与 `device_sessions.refresh_token_hash` 绑定，支持 Rotation（每次刷新旧 Token 失效）。
- **Guest**：客户端直接调用 `POST /v1/auth/guest`（无需 Server Secret，但受 im-gateway Rate Limit 保护），生成 `kind='guest'` 的 User。
- **Guest Upgrade**：`POST /v1/auth/link`，携带 Guest Access Token + 外部身份凭证，若目标外部身份已存在 User 则触发 `ISS-VOICE`同类型的 **Account Merge 冲突流程**（MVP：拒绝合并并返回冲突错误，交由客户端提示用户选择；自动合并策略非MVP范围，标记 Open Issue）。

## 7. Conversation / Message API 草案（REST，MVP）

```
POST   /v1/conversations                       创建会话（kind=dm|group|channel）
GET    /v1/conversations                        我的会话列表（分页，含last_message摘要）
GET    /v1/conversations/{id}/messages?after_sequence=&limit=   增量拉取（离线同步核心接口）
POST   /v1/conversations/{id}/messages          发送消息（需idempotency_key）
PATCH  /v1/conversations/{id}/messages/{msg_id} 编辑消息
POST   /v1/conversations/{id}/messages/{msg_id}/recall   撤回（时间窗校验）
POST   /v1/conversations/{id}/messages/{msg_id}/reactions  添加reaction
POST   /v1/conversations/{id}/read              上报已读（更新last_read_sequence）
```

实时通道（WebSocket，im-gateway 终结，契约与 DetailedDesign 严格一致）：

```
Client → Gateway:  { "type": "send_message", "req_id": "uuid", "conversation_id": "...", "idempotency_key": "...", "kind": "text", "content": {"text": "hi"}, "reply_to": null }
Gateway → Client:  { "type": "ack", "req_id": "uuid", "ok": true, "data": { "message_id": "...", "sequence": 42 } }
Gateway → Client:  { "type": "ack", "req_id": "uuid", "ok": false, "error": { "code": "IDEMPOTENCY_CONFLICT", "message": "..." } }
Gateway → Client:  { "type": "message_new", "message": { ... } }          // 广播给会话在线成员
Gateway → Client:  { "type": "presence_update", "user_id": "...", "status": "online" }
Gateway → Client:  { "type": "typing", "conversation_id": "...", "user_id": "..." }
```

**离线/断线重连**：客户端携带 `last_known_sequence`（每会话）通过 `GET /v1/conversations/{id}/messages?after_sequence=` 增量拉取，WebSocket 仅推送重连后的新增；不做"服务端主动补发离线队列"这类有状态设计，退化为客户端拉取模式，实现更简单且天然幂等（呼应弱网设计原则，MVP优先简单正确）。

## 8. 事件总线（NATS JetStream）Topic 设计

```
im.message.created         { message_id, conversation_id, sender_id, sequence, kind }
im.message.edited          { message_id, conversation_id, content, edited_at }       // 2026-08-23 补
im.message.recalled        { message_id, conversation_id }
im.message.reaction_added  { message_id, user_id, emoji }                            // 2026-08-23 补
im.conversation.created    { conversation_id, kind, metadata }
im.conversation.member_joined / member_left
im.presence.changed        { user_id, status }
im.identity.state_changed  { user_id, state }   // ban/delete等，供audit与其他服务响应
im.auth.token_rotated      { user_id, device_session_id }                            // 2026-08-23 补,触发 force_disconnect
```

Extension Runtime（骨架）通过 JetStream Consumer 订阅上述 Topic 的子集（按 Manifest 声明的 `event_subscription` 过滤），**MVP 阶段无任何 Extension 实际消费这些事件**，仅验证订阅链路可用性与权限隔离（呼应 `EXT-FR-001`）。

## 9. im-gateway 设计要点

- 单个 WebSocket 连接对应一个 `device_session`；同一 User 允许多端同时连接（多设备），但 Token 校验/续期独立。
- **连接会话状态与 TTL 管理**：连接建立后将路由元数据（`user_id → gateway_instance_id → connection_id`）写入 Valkey，并设置 `TTL = 120s`（2倍心跳周期）。客户端通过定周期 `ping` 帧驱动网关对该 Key 执行 `EXPIRE` 续期。若客户端异常断电或网关实例异常宕机，过期键将在 120s 内自动失效，避免 Fan-out 路由向死连接投递产生悬空 gRPC 开销。
- Fan-out 路径：`im-core` 写入消息并提交事务后，发布 `im.message.created` 事件 → 订阅该事件的 Fan-out 组件（MVP 阶段可内置于 im-gateway 或作为 im-core 的轻量 sidecar 逻辑，**不新建独立服务**，避免过早拆分）→ 查询 Valkey 中会话在线成员的 Gateway 位置 → 通过内部 gRPC 推送到对应 Gateway 实例 → WebSocket 下发。
- Rate Limit：按 `user_id` + `ip` 维度，基于 Valkey 令牌桶实现，MVP 覆盖发送消息、创建会话、Guest 注册三个高风险端点。

## 10. Media（im-media）设计要点

```
POST /v1/media/presign   { content_type, size_hint } → { upload_url, media_id, expires_at }
```

- 客户端直接 PUT 到 MinIO 预签名 URL，不经过 im-core/im-gateway（呼应 `IM-MEDIA-002`）。
- 消息 `content` 中仅存 `media_id` 引用，展示时客户端调用 `GET /v1/media/{id}` 换取短期可读 URL。
- MVP 不做病毒扫描/内容审核（Moderation Extension 范围，V1+）。

## 11. Game SDK 五步接入映射到本设计的实际调用

```
Voice/Game SDK.Initialize(config)      → 加载 environment_id / gateway endpoint 配置
SDK.Authenticate(server_issued_token)  → 客户端使用游戏服务器下发的 access_token（服务器已完成 §6 Token Exchange）
SDK.Connect()                          → 建立 WebSocket 到 im-gateway，携带 access_token
SDK.Join(conversation_id)              → 订阅会话（服务端校验 conversation_members 或按metadata自动加入逻辑）
SDK.Send(conversation_id, content)     → 上文 WebSocket send_message
```

Unity 官方 SDK（MVP交付形态）：C# 封装上述五步 + 本地消息缓存 + 自动重连（指数退避）+ 心跳（30s间隔，**BENCHMARK REQUIRED**确认合适间隔）。

## 12. Laser HUD（MVP范围：Desktop独立窗口方案）

依据 SRS `HUD-OV-002`（默认禁止Hook）与 MVP 优先级，**MVP 阶段 HUD 采用独立置顶窗口方案**（非游戏内渲染插件），跨引擎零集成成本，规避反作弊风险，代价是无法在游戏独占全屏(exclusive fullscreen，非borderless)模式下正确置顶——此限制需在MVP发布说明中明确告知，Engine Plugin方案作为V1的追加能力（覆盖独占全屏场景），而非推翻MVP方案重做。

```
HUD Runtime (Tauri 或等效轻量webview容器, 待技术选型ADR)
   │
   ├─ Rust Core（复用 SDK 的连接/状态管理逻辑）
   └─ Next.js构建产物作为WebView内容（复用Dashboard的UI组件库，降低维护成本）
```

Laser层四态（Idle/NewMessage/Mention/Priority）→ Peek/Chat 层复用 Next.js 组件，Full Client 暂不在 MVP 交付（用 Web Dashboard 替代 Full Client 的管理类功能）。

## 13. K3s 部署清单（MVP）

```
Namespace: im-platform
Deployments:
  im-gateway         (replicas: 2+, HPA on connection count)
  im-core            (replicas: 2+, HPA on CPU, 无状态，PG为状态)
  im-presence        (replicas: 2+)
  im-media           (replicas: 1-2)
  extension-runtime  (replicas: 1, MVP骨架，低负载)
  web-dashboard      (Next.js, replicas: 2+)
StatefulSets/Operators:
  postgres (建议CloudNativePG Operator，Apache-2.0/PostgreSQL License，符合SRS开源约束)
  nats (JetStream, NATS官方Helm Chart)
  valkey (Valkey官方或社区Helm Chart)
  minio (MinIO Operator)
Ingress:
  gateway.{tenant}.example.com → im-gateway (WSS)
  api.{tenant}.example.com     → im-gateway HTTP路由 / im-core内部API不直接暴露
  dashboard.{tenant}.example.com → web-dashboard
```

**ADR-012（新增）**：PostgreSQL Operator 选型（CloudNativePG vs Zalando Postgres Operator vs 手工StatefulSet）—— Candidate: CloudNativePG（活跃维护、Apache-2.0），Needs PoC 验证故障切换行为（呼应 SRS VOICE-POC-009 同类验证方法论）。

## 14. 配置与密钥管理

### 14.1 配置分层

| 层级 | 来源 | 范围 | 生效时机 | 变更流程 |
|---|---|---|---|---|
| **构建期配置** | Cargo features + 编译时常量 | 进程级开关（如启用 Prometheus exporter） | 重新构建 | Git 提交即变更 |
| **启动期配置** | 环境变量（`IM_*` 前缀）+ 配置文件（`config/{env}.toml`） | 服务实例级 | 进程启动时加载 | 重启服务 |
| **运行期配置** | `environments.settings` JSONB 字段 | 租户/环境级 | im-core 启动 + 定期重载（Valkey 缓存 + pub/sub 失效） | DB UPDATE → 失效广播 → 重载 |
| **密钥/凭证** | K3s Secret（`im-env-{environment_id}`） | 环境级 | 启动时挂载为环境变量 | 双密钥校验窗口轮换（§14.4） |

### 14.2 启动期配置项分类

| 类别 | 典型变量 | 加载方式 | 缺失行为 |
|---|---|---|---|
| **必填**（缺则启动失败） | `IM_DATABASE_URL`, `IM_JWT_SIGNING_KEY`, `IM_NATS_URL`, `IM_VALKEY_URL` | 启动时校验 | panic + 非 0 退出码 |
| **有默认值** | `IM_HTTP_PORT`（默认 8080）、`IM_WS_HEARTBEAT_TIMEOUT_SECONDS`（默认 60） | 启动时填充 | 取默认值 |
| **环境相关** | `IM_ENV` ∈ {`dev`, `staging`, `prod`} 决定日志级别、metrics 开关、错误堆栈暴露策略 | 启动时决定 | 默认 `dev` |

详细配置项清单（每个变量的名称、类型、范围、默认值、影响范围）见 `DetailedDesign.md §10`，本节不重复。

### 14.3 租户级运行期配置（`environments.settings` 字段）

`environments` 表的 `settings` JSONB 列承载租户/环境级可调参数，MVP 落地的字段：

| 字段 | 类型 | 默认值 | 含义 | 影响 |
|---|---|---|---|---|
| `friend_system_enabled` | bool | `true` | 是否暴露好友 UI/API（呼应 `IM-REL-002`） | 关闭后 `POST /v1/friends/*` 全部 403 |
| `rate_limit.send_message.per_min` | int | `60` | 单用户每分钟发送消息上限 | 触发后 `RATE_LIMITED` 错误 |
| `rate_limit.guest_register.per_hour` | int | `10` | Guest 注册按 IP 每小时上限 | 防脚本注册 |
| `message.recall_window_seconds` | int | `120` | 消息撤回时间窗 | 超窗后只能 delete |
| `message.retention_days.dm` | int | `null`（永久） | DM 消息保留天数 | 过期后台 job 物理删除 |
| `message.retention_days.group` | int | `null` | 群消息保留天数 | 同上 |
| `message.retention_days.channel` | int | `365` | 频道消息保留天数 | 同上 |
| `voice.enabled` | bool | `false` | V1 语音子系统启用开关 | 关闭后 SDK 不暴露 Voice API |
| `audit.detailed` | bool | `false` | 是否记录详细审计（详细级别影响性能） | 影响 `audit_logs` 写入量 |

加载与缓存：
- im-core 启动时全量加载所有 `environments` 行 → 写入 Valkey（Key 形如 `env:settings:{environment_id}`，TTL 3600s）
- 提供 `PATCH /v1/internal/environments/{id}/settings` 内部 API（仅 `im-gateway` 可调用，签名校验）触发 Valkey pub/sub 失效广播
- 收到广播后 im-core 各实例主动重载本实例已缓存的 settings（避免全集群击穿 DB）

### 14.4 密钥管理

#### 14.4.1 密钥清单

每个 Environment 独立持有以下密钥（存储于 K3s Secret `im-env-{environment_id}`，MVP 阶段不引入外部 KMS）：

| 密钥 | 用途 | 生成 | 长度 | 轮换频率 |
|---|---|---|---|---|
| `server_secret` | 游戏服务器 Token Exchange HMAC 签名（§6） | `openssl rand -hex 32` | 32 字节（64 hex） | 季度 |
| `jwt_signing_key` | Access Token 签名 | `openssl rand -hex 32` | 32 字节 | 季度 |
| `refresh_token_pepper` | Refresh Token 哈希加盐 | `openssl rand -hex 32` | 32 字节 | 半年 |
| `im_jwt_signing_keys_v1/v2` | Access Token 签名（轮换期双密钥） | 同上 | 32 字节 | 轮换期 7 天 |

#### 14.4.2 轮换流程（呼应 `SEC-NFR-003`）

1. **生成新密钥** → 写入新 Secret Key（如 `im_jwt_signing_keys_v2`），不覆盖 `v1`。
2. **应用层支持双密钥校验**：`im-core` 的 Token 校验逻辑同时接受 `v1`/`v2` 两个 Key 签名（Key ID 嵌入 JWT `kid` 头），实现细节见 `DetailedDesign.md §6/§10`。
3. **下发明文配置变更**：通过 im-gateway 的内部 admin API（`POST /v1/internal/auth/rotate-start`）告知游戏服务器"已开始轮换，请用 `kid=v2` 签发新 Token"。
4. **观察期（默认 7 天）**：监控 Token 流量分布，v1 占比 < 1% 时进入下一步。
5. **下线 v1**：发布配置移除 `v1` Key，所有 `kid=v1` 的 Token 视为非法。
6. **归档审计**：轮换过程写入 `audit_logs`（`action=secret_rotation`, `target_type=environment`）。

#### 14.4.3 密钥访问控制

- K3s Secret 仅 im-core 命名空间内 Pod 可挂载（`imagePullSecrets` + `serviceAccount` 限定）
- 应用进程以非 root 用户运行（Dockerfile `USER 1000:1000`，见 `Platform-Specifics.md §6`）
- 密钥**永不**写入日志（tracing 层 filter 掉 `server_secret` 等字段名）
- 密钥**永不**进入 Git（`.gitignore` 已包含 `.env*`，Secret 仅通过 `kubectl create secret` 注入）

### 14.5 配置加载与故障行为

```rust
// crates/im-common/src/config.rs 概念示意
pub struct AppConfig {
    pub database_url: Secret<String>,        // 必填,缺失 panic
    pub jwt_signing_keys: Vec<SigningKey>,    // 至少 1 个,缺失 panic
    pub nats_url: String,                     // 必填
    pub valkey_url: String,                   // 必填
    pub env: Environment,                     // dev/staging/prod
    pub settings_refresh_interval: Duration,  // 默认 300s
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        // 1. 读取 IM_ENV 决定 config/{env}.toml
        // 2. 读取环境变量覆盖
        // 3. 校验必填项
        // 4. 解析双密钥(v1/v2)
        // 5. 启动 background task 监听 settings 失效
    }
}
```

启动期任意必填项缺失或格式错误 → `process::exit(78)` (sysexits.h `EX_CONFIG`)，由 K3s 自动重启（CrashLoopBackOff 在监控中显形）。

### 14.6 V1+ 演进路径

- **V1**：评估外部 KMS（候选：HashiCorp Vault，BSL 1.1 需评估 / OpenBao，Apache-2.0 / 青云 KBS），引入 secret 自动轮换 + 审计推送
- **V1**：拆分 `config-server` 服务统一管理配置分发，im-core 通过 gRPC 拉取而非读 DB
- **V2**：支持租户自助修改 settings（通过 Web Dashboard，不再依赖内部 API）

## 15. 可观测性落地（MVP最小集）

- OpenTelemetry Collector 部署为 K3s DaemonSet 或 Deployment，接收 im-gateway/im-core/im-presence 的 Trace/Metrics。
- MVP 落地指标（SRS第37章全集的子集）：`active_connections, message_latency, message_delivery_latency, connection_latency, reconnect_rate`。其余指标（`extension_latency`等）待Extension有实际负载后再接入。

## 16. 详细设计阶段的遗留决策点（非本文档阻塞项，需在下一阶段闭环）

- **ADR-011**：im-core 未来拆分的触发阈值（QPS/连接数具体数字，需第一轮压测后回填）。
- **ADR-012**：PostgreSQL Operator 选型（第13章）。
- **ADR-013**：HUD Runtime 容器技术选型（Tauri vs 其他，第12章）。
- **ADR-014**：WS 帧序列化格式（JSON vs Protobuf）—— DetailedDesign §3 Candidate 暂定 JSON，需压测确认（与 SRS §50 ADR-Candidate 同步）。
- **ADR-015**：Valkey vs KeyDB 选型（Valkey 已默认，详细设计阶段需确认集群模式与 Sentinel 配置）。
- Sequence生成方案的性能天花板需要 POC-02（SRS第47章）实测后确认是否需要升级为分布式方案。
- `environments.settings` 字段清单已落 §14.3，**产品确认后**即定版；如需扩展字段按 `F. 字段变更控制`（aux-02）流程。
- `IM_ACCESS_TOKEN_TTL_SECONDS` / `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` / `IM_MESSAGE_RECALL_WINDOW_SECONDS` 等 Candidate 默认值需在第一轮压测 + 业务确认后固化（详细设计 §10 已标 Candidate）。

---

## 附录：与 SRS/LiveKit 文档的一致性检查

- 本设计未在 `conversations`/`messages` 表中引入任何游戏专有字段，符合 SRS `IM-CONV-002`。
- Extension Runtime 骨架已预留事件订阅隔离机制，符合 `EXT-FR-002`，但具体沙箱强度（进程级/WASM）留待 ADR-005 决议后在详细设计中落地，本设计不预先假设。
- Voice Control Service（LiveKit文档）与本设计的 im-core 共享租户/用户模型，但作为独立服务不在本版本部署清单中（V1加入），本设计的 `users`/`environments` 表结构已预留被语音子系统复用的兼容性（无需修改即可被引用）。
